use std::{net::SocketAddr, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{audio::AudioSampleFormat, config::ReceiverConfig};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiverServiceState {
    #[default]
    Idle,
    Listening,
    Connected,
    Buffering,
    Playing,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiverSyncState {
    #[default]
    Idle,
    Priming,
    Locked,
    Late,
    Recovering,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiverLatencyPreset {
    LowLatency,
    #[default]
    Balanced,
    Stable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiverLatencyProfile {
    pub playback_latency_ms: u16,
    pub target_buffer_ms: u16,
    pub max_buffer_ms: u16,
    pub latency_tolerance_ms: u16,
    pub late_packet_drop_ms: u16,
    pub reconnect_grace_period: Duration,
}

impl ReceiverLatencyPreset {
    pub fn profile(self) -> ReceiverLatencyProfile {
        match self {
            Self::LowLatency => ReceiverLatencyProfile {
                playback_latency_ms: 50,
                target_buffer_ms: 30,
                max_buffer_ms: 110,
                latency_tolerance_ms: 20,
                late_packet_drop_ms: 25,
                reconnect_grace_period: Duration::from_millis(750),
            },
            Self::Balanced => ReceiverLatencyProfile {
                playback_latency_ms: 80,
                target_buffer_ms: 70,
                max_buffer_ms: 210,
                latency_tolerance_ms: 30,
                late_packet_drop_ms: 40,
                reconnect_grace_period: Duration::from_millis(1_500),
            },
            Self::Stable => ReceiverLatencyProfile {
                playback_latency_ms: 110,
                target_buffer_ms: 120,
                max_buffer_ms: 340,
                latency_tolerance_ms: 45,
                late_packet_drop_ms: 60,
                reconnect_grace_period: Duration::from_millis(2_500),
            },
        }
    }
}

impl ReceiverLatencyProfile {
    pub fn expected_output_latency_ms(self) -> u16 {
        self.playback_latency_ms
            .saturating_add(self.target_buffer_ms)
    }

    pub fn buffer_packet_limits(self, stream: &ReceiverStreamConfig) -> (usize, usize) {
        let packet_duration = stream.packet_duration();
        (
            packet_count_for_duration(
                Duration::from_millis(self.target_buffer_ms as u64),
                packet_duration,
            ),
            packet_count_for_duration(
                Duration::from_millis(self.max_buffer_ms as u64),
                packet_duration,
            ),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverStreamConfig {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sample_format: AudioSampleFormat,
    pub frames_per_packet: u32,
}

impl Default for ReceiverStreamConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: AudioSampleFormat::S16Le,
            frames_per_packet: 480,
        }
    }
}

impl ReceiverStreamConfig {
    pub fn bytes_per_frame(&self) -> usize {
        self.channels as usize * self.sample_format.bytes_per_sample()
    }

    pub fn packet_bytes_hint(&self) -> usize {
        self.frames_per_packet as usize * self.bytes_per_frame()
    }

    pub fn packet_duration(&self) -> Duration {
        if self.sample_rate_hz == 0 || self.frames_per_packet == 0 {
            return Duration::from_millis(0);
        }

        Duration::from_secs_f64(self.frames_per_packet as f64 / self.sample_rate_hz as f64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverConnectionInfo {
    pub session_id: String,
    pub remote_addr: Option<SocketAddr>,
    pub stream: ReceiverStreamConfig,
    pub requested_latency_ms: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverAudioPacket {
    pub sequence: u64,
    pub captured_at_ms: u64,
    pub captured_at_unix_ms: u64,
    pub payload: Vec<u8>,
}

impl ReceiverAudioPacket {
    pub fn frame_count(&self, stream: &ReceiverStreamConfig) -> Result<u32, String> {
        let bytes_per_frame = stream.bytes_per_frame();
        if bytes_per_frame == 0 {
            return Err("stream bytes_per_frame cannot be zero".to_string());
        }
        if self.payload.len() % bytes_per_frame != 0 {
            return Err(format!(
                "packet payload size {} is not divisible by bytes_per_frame {}",
                self.payload.len(),
                bytes_per_frame
            ));
        }

        Ok((self.payload.len() / bytes_per_frame) as u32)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiverTransportEvent {
    Connected(ReceiverConnectionInfo),
    AudioPacket(ReceiverAudioPacket),
    KeepAlive,
    Disconnected {
        reason: String,
        reconnect_suggested: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverBufferSnapshot {
    pub queued_packets: u32,
    pub queued_frames: u64,
    pub queued_audio_ms: u32,
    pub target_buffer_ms: u32,
    pub max_buffer_ms: u32,
    pub start_threshold_packets: u32,
    pub max_packets: u32,
}

impl ReceiverBufferSnapshot {
    pub fn fill_percent(&self) -> u8 {
        if self.max_buffer_ms > 0 {
            return ((self.queued_audio_ms.saturating_mul(100)) / self.max_buffer_ms).min(100)
                as u8;
        }
        if self.max_packets > 0 {
            return ((self.queued_packets.saturating_mul(100)) / self.max_packets).min(100) as u8;
        }
        0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverMetrics {
    pub packets_received: u64,
    pub frames_received: u64,
    pub bytes_received: u64,
    pub packets_played: u64,
    pub frames_played: u64,
    pub bytes_played: u64,
    pub buffer_fill_percent: u8,
    pub underruns: u64,
    pub overruns: u64,
    pub reconnect_attempts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverSyncSnapshot {
    pub state: ReceiverSyncState,
    pub requested_latency_ms: Option<u16>,
    pub target_buffer_ms: u16,
    pub playback_latency_ms: u16,
    pub expected_output_latency_ms: u16,
    pub queued_audio_ms: u32,
    pub buffer_delta_ms: i32,
    pub schedule_error_ms: i32,
    pub late_packet_drops: u64,
    pub sync_resets: u64,
    pub last_sender_timestamp_ms: Option<u64>,
    pub last_sender_capture_unix_ms: Option<u64>,
}

impl Default for ReceiverSyncSnapshot {
    fn default() -> Self {
        Self {
            state: ReceiverSyncState::Idle,
            requested_latency_ms: None,
            target_buffer_ms: 0,
            playback_latency_ms: 0,
            expected_output_latency_ms: 0,
            queued_audio_ms: 0,
            buffer_delta_ms: 0,
            schedule_error_ms: 0,
            late_packet_drops: 0,
            sync_resets: 0,
            last_sender_timestamp_ms: None,
            last_sender_capture_unix_ms: None,
        }
    }
}

impl ReceiverSyncSnapshot {
    pub fn from_profile(profile: ReceiverLatencyProfile) -> Self {
        Self {
            target_buffer_ms: profile.target_buffer_ms,
            playback_latency_ms: profile.playback_latency_ms,
            expected_output_latency_ms: profile.expected_output_latency_ms(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverSnapshot {
    pub state: ReceiverServiceState,
    pub advertised_name: String,
    pub bind_host: String,
    pub listen_port: u16,
    pub latency_preset: ReceiverLatencyPreset,
    pub playback_target_id: Option<String>,
    pub playback_backend: Option<String>,
    pub connection: Option<ReceiverConnectionInfo>,
    pub buffer: ReceiverBufferSnapshot,
    pub sync: ReceiverSyncSnapshot,
    pub metrics: ReceiverMetrics,
    pub last_error: Option<String>,
}

impl ReceiverSnapshot {
    pub fn from_config(config: &ReceiverConfig) -> Self {
        let profile = config.latency_preset.profile();
        let default_stream = ReceiverStreamConfig::default();
        let (start_threshold_packets, max_packets) = profile.buffer_packet_limits(&default_stream);
        Self {
            state: ReceiverServiceState::Idle,
            advertised_name: config.advertised_name.clone(),
            bind_host: config.bind_host.clone(),
            listen_port: config.listen_port,
            latency_preset: config.latency_preset,
            playback_target_id: config.playback_target_id.clone(),
            playback_backend: None,
            connection: None,
            buffer: ReceiverBufferSnapshot {
                target_buffer_ms: profile.target_buffer_ms as u32,
                max_buffer_ms: profile.max_buffer_ms as u32,
                start_threshold_packets: start_threshold_packets as u32,
                max_packets: max_packets as u32,
                ..ReceiverBufferSnapshot::default()
            },
            sync: ReceiverSyncSnapshot::from_profile(profile),
            metrics: ReceiverMetrics::default(),
            last_error: None,
        }
    }
}

fn packet_count_for_duration(target: Duration, packet_duration: Duration) -> usize {
    let packet_nanos = packet_duration.as_nanos();
    if packet_nanos == 0 {
        return 1;
    }

    let target_nanos = target.as_nanos().max(1);
    let packets = target_nanos.div_ceil(packet_nanos);
    packets.min(usize::MAX as u128) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_presets_define_explicit_buffer_profiles() {
        let low = ReceiverLatencyPreset::LowLatency.profile();
        let balanced = ReceiverLatencyPreset::Balanced.profile();
        let stable = ReceiverLatencyPreset::Stable.profile();

        assert!(low.expected_output_latency_ms() < balanced.expected_output_latency_ms());
        assert!(balanced.expected_output_latency_ms() < stable.expected_output_latency_ms());
        assert!(low.target_buffer_ms < stable.target_buffer_ms);
        assert!(low.max_buffer_ms < stable.max_buffer_ms);
    }

    #[test]
    fn packet_frame_count_validates_payload_alignment() {
        let stream = ReceiverStreamConfig::default();
        let packet = ReceiverAudioPacket {
            sequence: 1,
            captured_at_ms: 0,
            captured_at_unix_ms: 0,
            payload: vec![0; stream.packet_bytes_hint()],
        };

        assert_eq!(
            packet.frame_count(&stream).expect("payload should align"),
            stream.frames_per_packet
        );
    }

    #[test]
    fn default_snapshot_exposes_sync_defaults() {
        let snapshot = ReceiverSnapshot::from_config(&ReceiverConfig::default());

        assert_eq!(snapshot.sync.state, ReceiverSyncState::Idle);
        assert!(snapshot.sync.expected_output_latency_ms > 0);
        assert!(snapshot.buffer.target_buffer_ms > 0);
        assert!(snapshot.buffer.max_buffer_ms >= snapshot.buffer.target_buffer_ms);
    }
}
