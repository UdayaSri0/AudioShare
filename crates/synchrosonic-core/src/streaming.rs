use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::{models::DeviceId, receiver::ReceiverStreamConfig};

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

impl StreamMetrics {
    pub fn accumulate(&mut self, other: &Self) {
        self.packets_sent += other.packets_sent;
        self.packets_received += other.packets_received;
        self.bytes_sent += other.bytes_sent;
        self.bytes_received += other.bytes_received;
        self.estimated_bitrate_bps += other.estimated_bitrate_bps;
        self.latency_estimate_ms = match (self.latency_estimate_ms, other.latency_estimate_ms) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        };
        self.packet_gaps += other.packet_gaps;
        self.keepalives_sent += other.keepalives_sent;
        self.keepalives_received += other.keepalives_received;
    }
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamTargetHealth {
    #[default]
    Pending,
    Healthy,
    Degraded,
    Unreachable,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamTargetFailureKind {
    Refused,
    Timeout,
    ResolveFailure,
    ProtocolMismatch,
    SelfTargetBlocked,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamTargetSnapshot {
    pub receiver_id: DeviceId,
    pub receiver_name: String,
    pub endpoint: SocketAddr,
    pub state: StreamSessionState,
    pub health: StreamTargetHealth,
    pub session_id: Option<String>,
    pub codec: Option<StreamCodec>,
    pub stream: Option<ReceiverStreamConfig>,
    pub network_buffer: StreamBranchBufferSnapshot,
    pub metrics: StreamMetrics,
    pub attempt_count: u32,
    pub next_retry_at_unix_ms: Option<u64>,
    pub last_error_kind: Option<StreamTargetFailureKind>,
    pub last_error: Option<String>,
}

impl StreamTargetSnapshot {
    pub fn new(
        receiver_id: DeviceId,
        receiver_name: impl Into<String>,
        endpoint: SocketAddr,
    ) -> Self {
        Self {
            receiver_id,
            receiver_name: receiver_name.into(),
            endpoint,
            state: StreamSessionState::Idle,
            health: StreamTargetHealth::Pending,
            session_id: None,
            codec: None,
            stream: None,
            network_buffer: StreamBranchBufferSnapshot::default(),
            metrics: StreamMetrics::default(),
            attempt_count: 0,
            next_retry_at_unix_ms: None,
            last_error_kind: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSessionSnapshot {
    pub state: StreamSessionState,
    pub session_id: Option<String>,
    pub stream: Option<ReceiverStreamConfig>,
    pub targets: Vec<StreamTargetSnapshot>,
    pub local_mirror: LocalMirrorSnapshot,
    pub metrics: StreamMetrics,
    pub last_error: Option<String>,
}

impl Default for StreamSessionSnapshot {
    fn default() -> Self {
        Self {
            state: StreamSessionState::Idle,
            session_id: None,
            stream: None,
            targets: Vec::new(),
            local_mirror: LocalMirrorSnapshot::default(),
            metrics: StreamMetrics::default(),
            last_error: None,
        }
    }
}

impl StreamSessionSnapshot {
    pub fn with_target(target: StreamTargetSnapshot) -> Self {
        Self {
            targets: vec![target],
            ..Self::default()
        }
    }

    pub fn target(&self, device_id: &DeviceId) -> Option<&StreamTargetSnapshot> {
        self.targets
            .iter()
            .find(|target| &target.receiver_id == device_id)
    }

    pub fn active_target_count(&self) -> usize {
        self.targets.len()
    }

    pub fn healthy_target_count(&self) -> usize {
        self.targets
            .iter()
            .filter(|target| target.health == StreamTargetHealth::Healthy)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn stream_snapshot_can_be_seeded_with_target_receiver() {
        let snapshot = StreamSessionSnapshot::with_target(StreamTargetSnapshot::new(
            DeviceId::new("receiver-1"),
            "Receiver",
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 51_700),
        ));

        assert_eq!(snapshot.state, StreamSessionState::Idle);
        assert_eq!(snapshot.targets.len(), 1);
        assert_eq!(snapshot.targets[0].receiver_name, "Receiver");
        assert_eq!(snapshot.targets[0].endpoint.port(), 51_700);
        assert_eq!(snapshot.local_mirror.state, LocalMirrorState::Disabled);
    }

    #[test]
    fn metrics_can_accumulate_across_multiple_targets() {
        let mut aggregate = StreamMetrics {
            packets_sent: 3,
            latency_estimate_ms: Some(10),
            ..StreamMetrics::default()
        };
        let other = StreamMetrics {
            packets_sent: 5,
            bytes_sent: 100,
            latency_estimate_ms: Some(20),
            ..StreamMetrics::default()
        };

        aggregate.accumulate(&other);

        assert_eq!(aggregate.packets_sent, 8);
        assert_eq!(aggregate.bytes_sent, 100);
        assert_eq!(aggregate.latency_estimate_ms, Some(20));
    }
}
