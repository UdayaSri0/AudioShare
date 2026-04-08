use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::{
    models::DeviceId,
    receiver::ReceiverStreamConfig,
};

pub const STREAM_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamCodec {
    RawPcm,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamSessionState {
    #[default]
    Idle,
    Connecting,
    Negotiating,
    Streaming,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamMetrics {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub estimated_bitrate_bps: u64,
    pub latency_estimate_ms: Option<u32>,
    pub packet_gaps: u64,
    pub keepalives_sent: u64,
    pub keepalives_received: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamBranchBufferSnapshot {
    pub queued_packets: u32,
    pub max_packets: u32,
    pub dropped_packets: u64,
}

impl StreamBranchBufferSnapshot {
    pub fn fill_percent(&self) -> u8 {
        if self.max_packets == 0 {
            return 0;
        }

        ((self.queued_packets.saturating_mul(100)) / self.max_packets).min(100) as u8
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalMirrorState {
    #[default]
    Disabled,
    Idle,
    Starting,
    Mirroring,
    Stopping,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMirrorSnapshot {
    pub desired_enabled: bool,
    pub state: LocalMirrorState,
    pub playback_backend: Option<String>,
    pub playback_target_id: Option<String>,
    pub buffer: StreamBranchBufferSnapshot,
    pub packets_played: u64,
    pub bytes_played: u64,
    pub last_error: Option<String>,
}

impl Default for LocalMirrorSnapshot {
    fn default() -> Self {
        Self {
            desired_enabled: false,
            state: LocalMirrorState::Disabled,
            playback_backend: None,
            playback_target_id: None,
            buffer: StreamBranchBufferSnapshot::default(),
            packets_played: 0,
            bytes_played: 0,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSessionSnapshot {
    pub state: StreamSessionState,
    pub session_id: Option<String>,
    pub receiver_id: Option<DeviceId>,
    pub receiver_name: Option<String>,
    pub endpoint: Option<SocketAddr>,
    pub codec: Option<StreamCodec>,
    pub stream: Option<ReceiverStreamConfig>,
    pub network_buffer: StreamBranchBufferSnapshot,
    pub local_mirror: LocalMirrorSnapshot,
    pub metrics: StreamMetrics,
    pub last_error: Option<String>,
}

impl Default for StreamSessionSnapshot {
    fn default() -> Self {
        Self {
            state: StreamSessionState::Idle,
            session_id: None,
            receiver_id: None,
            receiver_name: None,
            endpoint: None,
            codec: None,
            stream: None,
            network_buffer: StreamBranchBufferSnapshot::default(),
            local_mirror: LocalMirrorSnapshot::default(),
            metrics: StreamMetrics::default(),
            last_error: None,
        }
    }
}

impl StreamSessionSnapshot {
    pub fn with_target(
        receiver_id: DeviceId,
        receiver_name: impl Into<String>,
        endpoint: SocketAddr,
    ) -> Self {
        Self {
            receiver_id: Some(receiver_id),
            receiver_name: Some(receiver_name.into()),
            endpoint: Some(endpoint),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn stream_snapshot_can_be_seeded_with_target_receiver() {
        let snapshot = StreamSessionSnapshot::with_target(
            DeviceId::new("receiver-1"),
            "Receiver",
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 51_700),
        );

        assert_eq!(snapshot.state, StreamSessionState::Idle);
        assert_eq!(snapshot.receiver_name.as_deref(), Some("Receiver"));
        assert_eq!(snapshot.endpoint.map(|addr| addr.port()), Some(51_700));
        assert_eq!(snapshot.local_mirror.state, LocalMirrorState::Disabled);
    }
}
