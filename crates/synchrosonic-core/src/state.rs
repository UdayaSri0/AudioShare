use serde::{Deserialize, Serialize};

use crate::{
    audio::CaptureState,
    config::AppConfig,
    diagnostics::DiagnosticEvent,
    models::{
        AudioSource, AudioSourceKind, DeviceId, DeviceStatus, DiscoveredDevice, DiscoveryEvent,
        DiscoverySnapshot,
    },
    receiver::ReceiverSnapshot,
    streaming::{StreamSessionSnapshot, StreamSessionState},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastSessionState {
    Idle,
    Preparing,
    Casting,
    Stopping,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceState {
    pub id: DeviceId,
    pub display_name: String,
    pub status: DeviceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    pub config: AppConfig,
    pub cast_session: CastSessionState,
    pub capture_state: CaptureState,
    pub receiver: ReceiverSnapshot,
    pub streaming: StreamSessionSnapshot,
    pub selected_receiver_device_id: Option<DeviceId>,
    pub audio_sources: Vec<AudioSource>,
    pub selected_audio_source_id: Option<String>,
    pub devices: Vec<DeviceState>,
    pub discovered_devices: Vec<DiscoveredDevice>,
    pub diagnostics: Vec<DiagnosticEvent>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let receiver = ReceiverSnapshot::from_config(&config.receiver);
        Self {
            receiver,
            streaming: StreamSessionSnapshot::default(),
            selected_receiver_device_id: None,
            config,
            cast_session: CastSessionState::Idle,
            capture_state: CaptureState::Idle,
            audio_sources: Vec::new(),
            selected_audio_source_id: None,
            devices: Vec::new(),
            discovered_devices: Vec::new(),
            diagnostics: vec![DiagnosticEvent::info(
                "app",
                "Project scaffold initialized; audio capture is ready for backend enumeration.",
            )],
        }
    }

    pub fn set_audio_sources(&mut self, sources: Vec<AudioSource>) {
        if self.selected_audio_source_id.is_none() {
            self.selected_audio_source_id = sources
                .iter()
                .find(|source| source.is_default && source.kind == AudioSourceKind::Monitor)
                .or_else(|| sources.iter().find(|source| source.is_default))
                .or_else(|| sources.first())
                .map(|source| source.id.clone());
        }

        if let Some(selected_id) = &self.selected_audio_source_id {
            if !sources.iter().any(|source| &source.id == selected_id) {
                self.selected_audio_source_id = sources.first().map(|source| source.id.clone());
                self.capture_state = CaptureState::SourceChanged;
            }
        }

        self.config.audio.preferred_source_id = self.selected_audio_source_id.clone();
        self.audio_sources = sources;
    }

    pub fn select_audio_source(&mut self, source_id: impl Into<String>) -> bool {
        let source_id = source_id.into();
        if !self.audio_sources.iter().any(|source| source.id == source_id) {
            return false;
        }

        self.selected_audio_source_id = Some(source_id.clone());
        self.config.audio.preferred_source_id = Some(source_id);
        self.capture_state = CaptureState::SourceChanged;
        true
    }

    pub fn apply_receiver_snapshot(&mut self, snapshot: ReceiverSnapshot) {
        self.receiver = snapshot;
    }

    pub fn apply_streaming_snapshot(&mut self, snapshot: StreamSessionSnapshot) {
        self.cast_session = match snapshot.state {
            StreamSessionState::Idle => CastSessionState::Idle,
            StreamSessionState::Connecting | StreamSessionState::Negotiating => {
                CastSessionState::Preparing
            }
            StreamSessionState::Streaming => CastSessionState::Casting,
            StreamSessionState::Stopping => CastSessionState::Stopping,
            StreamSessionState::Error => CastSessionState::Error,
        };
        self.streaming = snapshot;
    }

    pub fn select_receiver_device(&mut self, device_id: DeviceId) -> bool {
        let is_valid = self
            .discovered_devices
            .iter()
            .any(|device| device.id == device_id && device.capabilities.supports_receiver);
        if !is_valid {
            return false;
        }

        self.selected_receiver_device_id = Some(device_id);
        true
    }

    pub fn apply_discovery_snapshot(&mut self, snapshot: DiscoverySnapshot) {
        self.discovered_devices = snapshot.devices;
        self.devices = self
            .discovered_devices
            .iter()
            .map(|device| DeviceState {
                id: device.id.clone(),
                display_name: device.display_name.clone(),
                status: device.status,
            })
            .collect();
        self.reconcile_selected_receiver();
    }

    pub fn apply_discovery_event(&mut self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::DeviceDiscovered(device) | DiscoveryEvent::DeviceUpdated(device) => {
                self.upsert_discovered_device(device);
            }
            DiscoveryEvent::DeviceRemoved { device_id, .. } => {
                self.discovered_devices
                    .retain(|device| device.id != device_id);
                self.devices.retain(|device| device.id != device_id);
            }
            DiscoveryEvent::DeviceExpired(device) => {
                self.upsert_discovered_device(device);
            }
        }
        self.reconcile_selected_receiver();
    }

    fn upsert_discovered_device(&mut self, device: DiscoveredDevice) {
        match self
            .discovered_devices
            .iter_mut()
            .find(|existing| existing.id == device.id)
        {
            Some(existing) => *existing = device.clone(),
            None => self.discovered_devices.push(device.clone()),
        }

        let device_state = DeviceState {
            id: device.id,
            display_name: device.display_name,
            status: device.status,
        };

        match self
            .devices
            .iter_mut()
            .find(|existing| existing.id == device_state.id)
        {
            Some(existing) => *existing = device_state,
            None => self.devices.push(device_state),
        }
    }

    fn reconcile_selected_receiver(&mut self) {
        let selected_is_valid = self
            .selected_receiver_device_id
            .as_ref()
            .map(|selected_id| {
                self.discovered_devices.iter().any(|device| {
                    &device.id == selected_id
                        && device.capabilities.supports_receiver
                        && device.endpoint.is_some()
                })
            })
            .unwrap_or(false);

        if selected_is_valid {
            return;
        }

        self.selected_receiver_device_id = self
            .discovered_devices
            .iter()
            .find(|device| device.capabilities.supports_receiver && device.endpoint.is_some())
            .map(|device| device.id.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_starts_idle_with_diagnostic() {
        let state = AppState::new(AppConfig::default());

        assert_eq!(state.cast_session, CastSessionState::Idle);
        assert_eq!(state.capture_state, CaptureState::Idle);
        assert_eq!(state.receiver.state, crate::receiver::ReceiverServiceState::Idle);
        assert_eq!(state.streaming.state, StreamSessionState::Idle);
        assert_eq!(state.diagnostics.len(), 1);
        assert_eq!(state.devices.len(), 0);
        assert_eq!(state.discovered_devices.len(), 0);
    }

    #[test]
    fn state_selects_default_audio_source_from_backend_results() {
        let mut state = AppState::new(AppConfig::default());
        state.set_audio_sources(vec![
            AudioSource {
                id: "mic".to_string(),
                display_name: "Microphone".to_string(),
                kind: crate::models::AudioSourceKind::Microphone,
                is_default: false,
            },
            AudioSource {
                id: "speaker".to_string(),
                display_name: "Speakers (monitor)".to_string(),
                kind: crate::models::AudioSourceKind::Monitor,
                is_default: true,
            },
        ]);

        assert_eq!(state.selected_audio_source_id.as_deref(), Some("speaker"));
        assert_eq!(state.config.audio.preferred_source_id.as_deref(), Some("speaker"));
    }

    #[test]
    fn discovery_events_update_app_state_device_views() {
        let mut state = AppState::new(AppConfig::default());
        let device = DiscoveredDevice {
            id: DeviceId::new("receiver-1"),
            display_name: "Living Room".to_string(),
            app_version: "0.1.0".to_string(),
            protocol_version: crate::models::DISCOVERY_PROTOCOL_VERSION,
            capabilities: crate::models::DeviceCapabilities::receiver(),
            availability: crate::models::DeviceAvailability::Available,
            status: DeviceStatus::Discovered,
            endpoint: Some(crate::models::TransportEndpoint {
                device_id: DeviceId::new("receiver-1"),
                address: std::net::SocketAddr::from(([127, 0, 0, 1], 51_700)),
            }),
            service_fullname: "Living Room._synchrosonic._tcp.local.".to_string(),
            last_seen_unix_ms: 1,
        };

        state.apply_discovery_event(DiscoveryEvent::DeviceDiscovered(device.clone()));
        assert_eq!(state.discovered_devices.len(), 1);
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.selected_receiver_device_id.as_ref(), Some(&device.id));

        state.apply_discovery_event(DiscoveryEvent::DeviceRemoved {
            device_id: device.id,
            service_fullname: device.service_fullname,
        });
        assert!(state.discovered_devices.is_empty());
        assert!(state.devices.is_empty());
        assert!(state.selected_receiver_device_id.is_none());
    }
}
