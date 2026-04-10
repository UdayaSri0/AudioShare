use std::{fmt, net::SocketAddr};

use serde::{Deserialize, Serialize};

pub const DISCOVERY_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceRole {
    Sender,
    Receiver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceStatus {
    Discovered,
    Connecting,
    Connected,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub supports_sender: bool,
    pub supports_receiver: bool,
    pub supports_local_output: bool,
    pub supports_bluetooth_output: bool,
}

impl DeviceCapabilities {
    pub fn sender() -> Self {
        Self {
            supports_sender: true,
            supports_receiver: false,
            supports_local_output: true,
            supports_bluetooth_output: false,
        }
    }

    pub fn receiver() -> Self {
        Self {
            supports_sender: false,
            supports_receiver: true,
            supports_local_output: true,
            supports_bluetooth_output: false,
        }
    }

    pub fn sender_receiver() -> Self {
        Self {
            supports_sender: true,
            supports_receiver: true,
            supports_local_output: true,
            supports_bluetooth_output: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceAvailability {
    Available,
    Busy,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioSourceKind {
    Monitor,
    Microphone,
    Application,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSource {
    pub id: String,
    pub display_name: String,
    pub kind: AudioSourceKind,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackTargetKind {
    #[default]
    Standard,
    Bluetooth,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackTargetAvailability {
    #[default]
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackTarget {
    pub id: String,
    pub display_name: String,
    pub is_default: bool,
    pub kind: PlaybackTargetKind,
    pub availability: PlaybackTargetAvailability,
    pub bluetooth_address: Option<String>,
}

impl PlaybackTarget {
    pub fn is_bluetooth(&self) -> bool {
        self.kind == PlaybackTargetKind::Bluetooth
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityPreset {
    LowLatency,
    Balanced,
    HighQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportEndpoint {
    pub device_id: DeviceId,
    pub address: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub id: DeviceId,
    pub display_name: String,
    pub app_version: String,
    pub protocol_version: u16,
    pub capabilities: DeviceCapabilities,
    pub availability: DeviceAvailability,
    pub status: DeviceStatus,
    pub endpoint: Option<TransportEndpoint>,
    pub service_fullname: String,
    pub last_seen_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryEvent {
    DeviceDiscovered(DiscoveredDevice),
    DeviceUpdated(DiscoveredDevice),
    DeviceRemoved {
        device_id: DeviceId,
        service_fullname: String,
    },
    DeviceExpired(DiscoveredDevice),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverySnapshot {
    pub devices: Vec<DiscoveredDevice>,
    pub updated_at_unix_ms: u64,
}
