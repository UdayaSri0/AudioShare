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
pub enum ReceiverLatencyPreset {
    LowLatency,
    #[default]
    Balanced,
    Stable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiverLatencyProfile {
    pub playback_latency_ms: u16,
    pub start_buffer_packets: usize,
    pub max_buffer_packets: usize,
    pub reconnect_grace_period: Duration,
}

impl ReceiverLatencyPreset {
    pub fn profile(self) -> ReceiverLatencyProfile {
        match self {
            Self::LowLatency => ReceiverLatencyProfile {
                playback_latency_ms: 60,
                start_buffer_packets: 2,
                max_buffer_packets: 6,
                reconnect_grace_period: Duration::from_millis(750),
            },
            Self::Balanced => ReceiverLatencyProfile {
                playback_latency_ms: 120,
                start_buffer_packets: 4,
                max_buffer_packets: 10,
                reconnect_grace_period: Duration::from_millis(1_500),
            },
            Self::Stable => ReceiverLatencyProfile {
                playback_latency_ms: 180,
                start_buffer_packets: 6,
                max_buffer_packets: 16,
                reconnect_grace_period: Duration::from_millis(2_500),
            },
        }
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverAudioPacket {
    pub sequence: u64,
    pub captured_at_ms: u64,
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
    pub start_threshold_packets: u32,
    pub max_packets: u32,
}

impl ReceiverBufferSnapshot {
    pub fn fill_percent(&self) -> u8 {
        if self.max_packets == 0 {
            return 0;
        }

        ((self.queued_packets.saturating_mul(100)) / self.max_packets).min(100) as u8
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
    pub metrics: ReceiverMetrics,
    pub last_error: Option<String>,
}

impl ReceiverSnapshot {
    pub fn from_config(config: &ReceiverConfig) -> Self {
        let profile = config.latency_preset.profile();
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
                start_threshold_packets: profile.start_buffer_packets as u32,
                max_packets: profile.max_buffer_packets as u32,
                ..ReceiverBufferSnapshot::default()
            },
            metrics: ReceiverMetrics::default(),
            last_error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_presets_define_explicit_buffer_profiles() {
        let low = ReceiverLatencyPreset::LowLatency.profile();
        let balanced = ReceiverLatencyPreset::Balanced.profile();
        let stable = ReceiverLatencyPreset::Stable.profile();

        assert!(low.playback_latency_ms < balanced.playback_latency_ms);
        assert!(balanced.playback_latency_ms < stable.playback_latency_ms);
        assert!(low.start_buffer_packets < stable.start_buffer_packets);
        assert!(low.max_buffer_packets < stable.max_buffer_packets);
    }

    #[test]
    fn packet_frame_count_validates_payload_alignment() {
        let stream = ReceiverStreamConfig::default();
        let packet = ReceiverAudioPacket {
            sequence: 1,
            captured_at_ms: 0,
            payload: vec![0; stream.packet_bytes_hint()],
        };

        assert_eq!(
            packet.frame_count(&stream).expect("payload should align"),
            stream.frames_per_packet
        );
    }
}
