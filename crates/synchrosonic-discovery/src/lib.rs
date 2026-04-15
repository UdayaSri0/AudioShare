use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use mdns_sd::{Receiver, ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use synchrosonic_core::{
    config::DiscoveryConfig, services::DiscoveryService, DeviceAvailability, DeviceCapabilities,
    DeviceId, DeviceStatus, DiscoveredDevice, DiscoveryError, DiscoveryEvent, DiscoverySnapshot,
    TransportEndpoint, DISCOVERY_PROTOCOL_VERSION,
};

const TXT_APP_ID: &str = "app";
const TXT_APP_VERSION: &str = "app_version";
const TXT_AVAILABILITY: &str = "availability";
const TXT_BLUETOOTH: &str = "bluetooth";
const TXT_DEVICE_ID: &str = "device_id";
const TXT_DEVICE_NAME: &str = "device_name";
const TXT_LOCAL_OUTPUT: &str = "local_output";
const TXT_PROTOCOL_VERSION: &str = "protocol_version";
const TXT_RECEIVER: &str = "receiver";
const TXT_SENDER: &str = "sender";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryAnnouncement {
    pub service_type: String,
    pub instance_name: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDiscoveryProfile {
    pub device_id: DeviceId,
    pub device_name: String,
    pub app_version: String,
    pub capabilities: DeviceCapabilities,
    pub availability: DeviceAvailability,
    pub port: u16,
}

impl LocalDiscoveryProfile {
    pub fn new(
        device_id: impl Into<String>,
        device_name: impl Into<String>,
        app_version: impl Into<String>,
        capabilities: DeviceCapabilities,
        availability: DeviceAvailability,
        port: u16,
    ) -> Self {
        Self {
            device_id: DeviceId::new(device_id),
            device_name: device_name.into(),
            app_version: app_version.into(),
            capabilities,
            availability,
            port,
        }
    }
}

pub struct MdnsDiscoveryService {
    config: DiscoveryConfig,
    profile: LocalDiscoveryProfile,
    registry: DeviceRegistry,
    daemon: Option<ServiceDaemon>,
    browse_receiver: Option<Receiver<ServiceEvent>>,
    advertised_fullname: Option<String>,
    pending_events: VecDeque<DiscoveryEvent>,
}

impl MdnsDiscoveryService {
    pub fn new(config: DiscoveryConfig, announcement_name: impl Into<String>) -> Self {
        let announcement_name = announcement_name.into();
        let profile = LocalDiscoveryProfile::new(
            deterministic_device_id(&announcement_name),
            announcement_name,
            env!("CARGO_PKG_VERSION"),
            DeviceCapabilities::sender_receiver(),
            DeviceAvailability::Available,
            51_700,
        );

        Self::with_profile(config, profile)
    }

    pub fn with_profile(config: DiscoveryConfig, profile: LocalDiscoveryProfile) -> Self {
        let stale_timeout = Duration::from_secs(config.stale_timeout_secs.max(1));
        Self {
            config,
            profile,
            registry: DeviceRegistry::new(stale_timeout),
            daemon: None,
            browse_receiver: None,
            advertised_fullname: None,
            pending_events: VecDeque::new(),
        }
    }

    pub fn announcement(&self, port: u16) -> DiscoveryAnnouncement {
        DiscoveryAnnouncement {
            service_type: self.config.service_type.clone(),
            instance_name: self.profile.device_name.clone(),
            port,
        }
    }

    pub fn registry(&self) -> &DeviceRegistry {
        &self.registry
    }

    fn build_service_info(&self) -> Result<ServiceInfo, DiscoveryError> {
        let hostname = format!("{}.local.", sanitized_dns_label(&self.profile.device_name));
        let props = txt_properties(&self.profile);

        ServiceInfo::new(
            &self.config.service_type,
            &self.profile.device_name,
            &hostname,
            "",
            self.profile.port,
            &props[..],
        )
        .map(ServiceInfo::enable_addr_auto)
        .map_err(|error| DiscoveryError::ServiceInfo(error.to_string()))
    }

    fn handle_mdns_event(&mut self, event: ServiceEvent) -> Result<(), DiscoveryError> {
        match event {
            ServiceEvent::SearchStarted(service_type) => {
                tracing::trace!(service_type, "mDNS discovery browsing started");
            }
            ServiceEvent::ServiceFound(service_type, fullname) => {
                tracing::debug!(service_type, fullname, "mDNS service found");
            }
            ServiceEvent::ServiceResolved(resolved) => {
                let device = discovered_device_from_resolved(&resolved)?;
                let Some(event) = self.registry.upsert(device) else {
                    return Ok(());
                };
                match &event {
                    DiscoveryEvent::DeviceDiscovered(device) => {
                        tracing::info!(
                            device_id = %device.id,
                            name = device.display_name,
                            endpoint = ?device.endpoint,
                            "discovered SynchroSonic device"
                        );
                    }
                    DiscoveryEvent::DeviceUpdated(device) => {
                        tracing::debug!(
                            device_id = %device.id,
                            name = device.display_name,
                            endpoint = ?device.endpoint,
                            "updated SynchroSonic device"
                        );
                    }
                    _ => {}
                }
                self.pending_events.push_back(event);
            }
            ServiceEvent::ServiceRemoved(_service_type, fullname) => {
                if let Some(event) = self.registry.mark_removed_by_fullname(&fullname) {
                    tracing::info!(fullname, "removed SynchroSonic device");
                    self.pending_events.push_back(event);
                }
            }
            ServiceEvent::SearchStopped(service_type) => {
                tracing::trace!(service_type, "mDNS discovery browsing stopped");
            }
            _ => {
                tracing::debug!("ignored unknown mDNS service event");
            }
        }

        Ok(())
    }
}

impl DiscoveryService for MdnsDiscoveryService {
    fn service_type(&self) -> &str {
        &self.config.service_type
    }

    fn planned_announcement_name(&self) -> &str {
        &self.profile.device_name
    }

    fn start(&mut self) -> Result<(), DiscoveryError> {
        if !self.config.enabled {
            tracing::info!("mDNS discovery is disabled by configuration");
            return Ok(());
        }
        if self.daemon.is_some() {
            tracing::debug!("mDNS discovery is already started");
            return Ok(());
        }

        let mdns =
            ServiceDaemon::new().map_err(|error| DiscoveryError::Daemon(error.to_string()))?;
        let service_info = self.build_service_info()?;
        let fullname = service_info.get_fullname().to_string();

        mdns.register(service_info)
            .map_err(|error| DiscoveryError::Register(error.to_string()))?;
        let browse_receiver = mdns
            .browse(&self.config.service_type)
            .map_err(|error| DiscoveryError::Browse(error.to_string()))?;

        tracing::info!(
            service_type = self.config.service_type,
            fullname,
            device_id = %self.profile.device_id,
            port = self.profile.port,
            "mDNS discovery service started"
        );

        self.advertised_fullname = Some(fullname);
        self.browse_receiver = Some(browse_receiver);
        self.daemon = Some(mdns);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), DiscoveryError> {
        let Some(mdns) = self.daemon.take() else {
            return Ok(());
        };

        if let Some(fullname) = self.advertised_fullname.take() {
            if let Err(error) = mdns.unregister(&fullname) {
                tracing::warn!(fullname, error = %error, "failed to unregister mDNS service");
            }
        }

        if let Err(error) = mdns.stop_browse(&self.config.service_type) {
            tracing::debug!(error = %error, "failed to stop mDNS browse");
        }

        mdns.shutdown()
            .map_err(|error| DiscoveryError::Stop(error.to_string()))?;
        self.browse_receiver = None;
        tracing::info!("mDNS discovery service stopped");
        Ok(())
    }

    fn poll_event(&mut self) -> Result<Option<DiscoveryEvent>, DiscoveryError> {
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(Some(event));
        }

        let Some(receiver) = &self.browse_receiver else {
            return Ok(None);
        };

        match receiver.try_recv() {
            Ok(event) => {
                self.handle_mdns_event(event)?;
                Ok(self.pending_events.pop_front())
            }
            Err(flume::TryRecvError::Empty) => Ok(None),
            Err(flume::TryRecvError::Disconnected) => Err(DiscoveryError::Event(
                "mDNS browse channel disconnected".to_string(),
            )),
        }
    }

    fn prune_stale(&mut self) -> Result<Vec<DiscoveryEvent>, DiscoveryError> {
        let events = self.registry.prune_stale();
        for event in &events {
            if let DiscoveryEvent::DeviceExpired(device) = event {
                tracing::warn!(
                    device_id = %device.id,
                    name = device.display_name,
                    "mDNS device registry entry became stale"
                );
            }
        }
        Ok(events)
    }

    fn snapshot(&self) -> DiscoverySnapshot {
        self.registry.snapshot()
    }
}

impl Drop for MdnsDiscoveryService {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Debug)]
struct RegistryEntry {
    device: DiscoveredDevice,
    last_seen: Instant,
    marked_stale: bool,
}

#[derive(Debug)]
pub struct DeviceRegistry {
    devices: HashMap<DeviceId, RegistryEntry>,
    stale_timeout: Duration,
}

impl DeviceRegistry {
    pub fn new(stale_timeout: Duration) -> Self {
        Self {
            devices: HashMap::new(),
            stale_timeout,
        }
    }

    pub fn upsert(&mut self, mut device: DiscoveredDevice) -> Option<DiscoveryEvent> {
        let device_id = device.id.clone();
        device.status = DeviceStatus::Discovered;
        device.last_seen_unix_ms = now_unix_ms();

        match self.devices.get_mut(&device_id) {
            Some(entry) => {
                let merged = merge_discovered_device(&entry.device, device);
                let changed = !materially_same_discovered_device(&entry.device, &merged);
                entry.device = merged;
                entry.last_seen = Instant::now();
                entry.marked_stale = false;
                changed.then(|| DiscoveryEvent::DeviceUpdated(entry.device.clone()))
            }
            None => {
                self.devices.insert(
                    device_id,
                    RegistryEntry {
                        device: device.clone(),
                        last_seen: Instant::now(),
                        marked_stale: false,
                    },
                );
                Some(DiscoveryEvent::DeviceDiscovered(device))
            }
        }
    }

    pub fn mark_removed_by_fullname(&mut self, fullname: &str) -> Option<DiscoveryEvent> {
        let device_id = self
            .devices
            .iter()
            .find(|(_, entry)| entry.device.service_fullname == fullname)
            .map(|(device_id, _)| device_id.clone())?;
        self.devices.remove(&device_id);
        Some(DiscoveryEvent::DeviceRemoved {
            device_id,
            service_fullname: fullname.to_string(),
        })
    }

    pub fn prune_stale(&mut self) -> Vec<DiscoveryEvent> {
        let mut events = Vec::new();
        for entry in self.devices.values_mut() {
            if !entry.marked_stale && entry.last_seen.elapsed() >= self.stale_timeout {
                entry.device.status = DeviceStatus::Unavailable;
                entry.device.availability = DeviceAvailability::Unavailable;
                entry.marked_stale = true;
                events.push(DiscoveryEvent::DeviceExpired(entry.device.clone()));
            }
        }
        events
    }

    pub fn snapshot(&self) -> DiscoverySnapshot {
        let mut devices: Vec<_> = self
            .devices
            .values()
            .map(|entry| entry.device.clone())
            .collect();
        devices.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        DiscoverySnapshot {
            devices,
            updated_at_unix_ms: now_unix_ms(),
        }
    }
}

fn discovered_device_from_resolved(
    service: &ResolvedService,
) -> Result<DiscoveredDevice, DiscoveryError> {
    let fullname = service.get_fullname().to_string();
    let device_id = service
        .get_property_val_str(TXT_DEVICE_ID)
        .filter(|value| !value.is_empty())
        .unwrap_or(&fullname);
    let display_name = service
        .get_property_val_str(TXT_DEVICE_NAME)
        .filter(|value| !value.is_empty())
        .unwrap_or(&fullname);
    let protocol_version = service
        .get_property_val_str(TXT_PROTOCOL_VERSION)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    let app_version = service
        .get_property_val_str(TXT_APP_VERSION)
        .unwrap_or("unknown")
        .to_string();
    let availability = parse_availability(
        service
            .get_property_val_str(TXT_AVAILABILITY)
            .unwrap_or("available"),
    );

    let endpoint = select_preferred_endpoint(
        &DeviceId::new(device_id),
        service
            .get_addresses()
            .iter()
            .map(|address| address.to_ip_addr()),
        service.get_port(),
    );

    Ok(DiscoveredDevice {
        id: DeviceId::new(device_id),
        display_name: display_name.to_string(),
        app_version,
        protocol_version,
        capabilities: DeviceCapabilities {
            supports_sender: txt_bool(service, TXT_SENDER),
            supports_receiver: txt_bool(service, TXT_RECEIVER),
            supports_local_output: txt_bool(service, TXT_LOCAL_OUTPUT),
            supports_bluetooth_output: txt_bool(service, TXT_BLUETOOTH),
        },
        availability,
        status: DeviceStatus::Discovered,
        endpoint,
        service_fullname: fullname,
        last_seen_unix_ms: now_unix_ms(),
    })
}

fn txt_properties(profile: &LocalDiscoveryProfile) -> Vec<(&'static str, String)> {
    vec![
        (TXT_APP_ID, "synchrosonic".to_string()),
        (TXT_DEVICE_ID, profile.device_id.as_str().to_string()),
        (TXT_DEVICE_NAME, profile.device_name.clone()),
        (TXT_APP_VERSION, profile.app_version.clone()),
        (TXT_PROTOCOL_VERSION, DISCOVERY_PROTOCOL_VERSION.to_string()),
        (TXT_SENDER, profile.capabilities.supports_sender.to_string()),
        (
            TXT_RECEIVER,
            profile.capabilities.supports_receiver.to_string(),
        ),
        (
            TXT_BLUETOOTH,
            profile.capabilities.supports_bluetooth_output.to_string(),
        ),
        (
            TXT_LOCAL_OUTPUT,
            profile.capabilities.supports_local_output.to_string(),
        ),
        (
            TXT_AVAILABILITY,
            availability_txt(profile.availability).to_string(),
        ),
    ]
}

fn txt_bool(service: &ResolvedService, key: &str) -> bool {
    service
        .get_property_val_str(key)
        .map(|value| matches!(value, "true" | "1" | "yes" | "on"))
        .unwrap_or(false)
}

fn parse_availability(value: &str) -> DeviceAvailability {
    match value {
        "available" => DeviceAvailability::Available,
        "busy" => DeviceAvailability::Busy,
        "unavailable" => DeviceAvailability::Unavailable,
        _ => DeviceAvailability::Unavailable,
    }
}

fn availability_txt(availability: DeviceAvailability) -> &'static str {
    match availability {
        DeviceAvailability::Available => "available",
        DeviceAvailability::Busy => "busy",
        DeviceAvailability::Unavailable => "unavailable",
    }
}

fn deterministic_device_id(name: &str) -> String {
    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown-host".to_string());
    format!("synchrosonic-{hostname}-{}", sanitized_dns_label(name))
}

fn sanitized_dns_label(value: &str) -> String {
    let label: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let label = label.trim_matches('-');
    if label.is_empty() {
        "synchrosonic-device".to_string()
    } else {
        label.chars().take(63).collect()
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn merge_discovered_device(
    existing: &DiscoveredDevice,
    mut incoming: DiscoveredDevice,
) -> DiscoveredDevice {
    incoming.endpoint = preferred_endpoint(existing.endpoint.as_ref(), incoming.endpoint.as_ref());
    incoming
}

fn materially_same_discovered_device(
    existing: &DiscoveredDevice,
    incoming: &DiscoveredDevice,
) -> bool {
    let mut existing = existing.clone();
    let mut incoming = incoming.clone();
    existing.last_seen_unix_ms = 0;
    incoming.last_seen_unix_ms = 0;
    existing == incoming
}

fn select_preferred_endpoint<I>(
    device_id: &DeviceId,
    addresses: I,
    port: u16,
) -> Option<TransportEndpoint>
where
    I: IntoIterator<Item = IpAddr>,
{
    select_preferred_ip_address(addresses).map(|address| TransportEndpoint {
        device_id: device_id.clone(),
        address: SocketAddr::new(address, port),
    })
}

fn preferred_endpoint(
    existing: Option<&TransportEndpoint>,
    incoming: Option<&TransportEndpoint>,
) -> Option<TransportEndpoint> {
    match (existing, incoming) {
        (Some(existing), Some(incoming))
            if endpoint_rank(incoming.address.ip()) > endpoint_rank(existing.address.ip()) =>
        {
            Some(incoming.clone())
        }
        (Some(existing), _) => Some(existing.clone()),
        (None, Some(incoming)) => Some(incoming.clone()),
        (None, None) => None,
    }
}

fn select_preferred_ip_address<I>(addresses: I) -> Option<IpAddr>
where
    I: IntoIterator<Item = IpAddr>,
{
    addresses
        .into_iter()
        .filter(|address| is_remote_selection_candidate(*address))
        .max_by_key(|address| endpoint_rank(*address))
}

fn endpoint_rank(address: IpAddr) -> (u8, u8) {
    match address {
        IpAddr::V4(address) => {
            let quality =
                if address.is_loopback() || address.is_unspecified() || address.is_multicast() {
                    0
                } else if is_likely_docker_ipv4(address) {
                    1
                } else if address.is_link_local() {
                    2
                } else if address.is_private() {
                    5
                } else {
                    4
                };
            (quality, 1)
        }
        IpAddr::V6(address) => {
            let quality =
                if address.is_loopback() || address.is_unspecified() || address.is_multicast() {
                    0
                } else if address.is_unicast_link_local() {
                    2
                } else if address.is_unique_local() {
                    3
                } else {
                    4
                };
            (quality, 0)
        }
    }
}

fn is_likely_docker_ipv4(address: Ipv4Addr) -> bool {
    let [first, second, ..] = address.octets();
    first == 172 && (17..=31).contains(&second)
}

fn is_remote_selection_candidate(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            !(address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || is_likely_docker_ipv4(address))
        }
        IpAddr::V6(address) => {
            !(address.is_loopback() || address.is_unspecified() || address.is_multicast())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcement_uses_configured_service_type() {
        let service = MdnsDiscoveryService::new(DiscoveryConfig::default(), "Laptop");
        let announcement = service.announcement(51_700);

        assert_eq!(announcement.service_type, "_synchrosonic._tcp.local.");
        assert_eq!(announcement.instance_name, "Laptop");
        assert_eq!(announcement.port, 51_700);
    }

    #[test]
    fn txt_properties_include_versioned_capabilities() {
        let profile = LocalDiscoveryProfile::new(
            "device-1",
            "Receiver",
            env!("CARGO_PKG_VERSION"),
            DeviceCapabilities::receiver(),
            DeviceAvailability::Available,
            51_700,
        );
        let props = txt_properties(&profile);

        assert!(props.contains(&(TXT_PROTOCOL_VERSION, "1".to_string())));
        assert!(props.contains(&(TXT_RECEIVER, "true".to_string())));
        assert!(props.contains(&(TXT_SENDER, "false".to_string())));
        assert!(props.contains(&(TXT_LOCAL_OUTPUT, "true".to_string())));
    }

    #[test]
    fn registry_updates_duplicate_device_announcements() {
        let mut registry = DeviceRegistry::new(Duration::from_secs(30));
        let first = test_device("device-1", "Receiver A", "a._synchrosonic._tcp.local.");
        let second = test_device("device-1", "Receiver B", "b._synchrosonic._tcp.local.");

        assert!(matches!(
            registry.upsert(first),
            Some(DiscoveryEvent::DeviceDiscovered(_))
        ));
        assert!(matches!(
            registry.upsert(second),
            Some(DiscoveryEvent::DeviceUpdated(_))
        ));

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].display_name, "Receiver B");
    }

    #[test]
    fn preferred_endpoint_selection_avoids_loopback_and_docker_when_lan_exists() {
        let endpoint = select_preferred_endpoint(
            &DeviceId::new("device-1"),
            [
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                IpAddr::V4(Ipv4Addr::new(172, 17, 0, 1)),
                IpAddr::V4(Ipv4Addr::new(192, 168, 8, 127)),
            ],
            51_700,
        )
        .expect("a preferred endpoint should be selected");

        assert_eq!(
            endpoint.address,
            SocketAddr::from(([192, 168, 8, 127], 51_700))
        );
    }

    #[test]
    fn preferred_endpoint_selection_returns_none_for_loopback_and_docker_only_addresses() {
        let endpoint = select_preferred_endpoint(
            &DeviceId::new("device-1"),
            [
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                IpAddr::V4(Ipv4Addr::new(172, 17, 0, 1)),
            ],
            51_700,
        );

        assert!(endpoint.is_none());
    }

    #[test]
    fn registry_keeps_the_best_endpoint_for_duplicate_device_updates() {
        let mut registry = DeviceRegistry::new(Duration::from_secs(30));
        let docker_endpoint = Some(TransportEndpoint {
            device_id: DeviceId::new("device-1"),
            address: SocketAddr::from(([172, 17, 0, 1], 51_700)),
        });
        let lan_endpoint = Some(TransportEndpoint {
            device_id: DeviceId::new("device-1"),
            address: SocketAddr::from(([192, 168, 8, 127], 51_700)),
        });
        let loopback_endpoint = Some(TransportEndpoint {
            device_id: DeviceId::new("device-1"),
            address: SocketAddr::from(([127, 0, 0, 1], 51_700)),
        });

        registry.upsert(test_device_with_endpoint(
            "device-1",
            "Receiver",
            "receiver._synchrosonic._tcp.local.",
            docker_endpoint,
        ));
        registry.upsert(test_device_with_endpoint(
            "device-1",
            "Receiver",
            "receiver._synchrosonic._tcp.local.",
            lan_endpoint,
        ));
        registry.upsert(test_device_with_endpoint(
            "device-1",
            "Receiver",
            "receiver._synchrosonic._tcp.local.",
            loopback_endpoint,
        ));

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(
            snapshot.devices[0]
                .endpoint
                .as_ref()
                .expect("device should keep an endpoint")
                .address,
            SocketAddr::from(([192, 168, 8, 127], 51_700))
        );
    }

    #[test]
    fn registry_marks_stale_entries_unavailable() {
        let mut registry = DeviceRegistry::new(Duration::from_millis(0));
        registry.upsert(test_device(
            "device-1",
            "Receiver",
            "receiver._synchrosonic._tcp.local.",
        ));

        let events = registry.prune_stale();

        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DiscoveryEvent::DeviceExpired(_)));
        assert_eq!(
            registry.snapshot().devices[0].availability,
            DeviceAvailability::Unavailable
        );
    }

    #[test]
    fn registry_suppresses_duplicate_updates_without_material_changes() {
        let mut registry = DeviceRegistry::new(Duration::from_secs(30));
        let device = test_device_with_endpoint(
            "device-1",
            "Receiver",
            "receiver._synchrosonic._tcp.local.",
            Some(TransportEndpoint {
                device_id: DeviceId::new("device-1"),
                address: SocketAddr::from(([192, 168, 8, 127], 51_700)),
            }),
        );

        assert!(matches!(
            registry.upsert(device.clone()),
            Some(DiscoveryEvent::DeviceDiscovered(_))
        ));
        assert_eq!(registry.upsert(device), None);
    }

    fn test_device(id: &str, name: &str, fullname: &str) -> DiscoveredDevice {
        test_device_with_endpoint(id, name, fullname, None)
    }

    fn test_device_with_endpoint(
        id: &str,
        name: &str,
        fullname: &str,
        endpoint: Option<TransportEndpoint>,
    ) -> DiscoveredDevice {
        DiscoveredDevice {
            id: DeviceId::new(id),
            display_name: name.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: DISCOVERY_PROTOCOL_VERSION,
            capabilities: DeviceCapabilities::receiver(),
            availability: DeviceAvailability::Available,
            status: DeviceStatus::Discovered,
            endpoint,
            service_fullname: fullname.to_string(),
            last_seen_unix_ms: now_unix_ms(),
        }
    }
}
