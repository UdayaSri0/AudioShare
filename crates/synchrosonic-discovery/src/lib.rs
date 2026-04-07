use synchrosonic_core::{config::DiscoveryConfig, services::DiscoveryService, DiscoveryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryAnnouncement {
    pub service_type: String,
    pub instance_name: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct MdnsDiscoveryService {
    config: DiscoveryConfig,
    announcement_name: String,
}

impl MdnsDiscoveryService {
    pub fn new(config: DiscoveryConfig, announcement_name: impl Into<String>) -> Self {
        Self {
            config,
            announcement_name: announcement_name.into(),
        }
    }

    pub fn announcement(&self, port: u16) -> DiscoveryAnnouncement {
        DiscoveryAnnouncement {
            service_type: self.config.service_type.clone(),
            instance_name: self.announcement_name.clone(),
            port,
        }
    }
}

impl DiscoveryService for MdnsDiscoveryService {
    fn service_type(&self) -> &str {
        &self.config.service_type
    }

    fn planned_announcement_name(&self) -> &str {
        &self.announcement_name
    }

    fn start(&mut self) -> Result<(), DiscoveryError> {
        Err(DiscoveryError::NotActive(
            "mDNS registration and browsing belong to the LAN discovery phase".to_string(),
        ))
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
}

