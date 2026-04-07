use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioDeviceDirection {
    Input,
    Output,
    Monitor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub display_name: String,
    pub direction: AudioDeviceDirection,
    pub backend_name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioSampleFormat {
    S16Le,
    F32Le,
}

impl AudioSampleFormat {
    pub fn bytes_per_sample(self) -> usize {
        match self {
            Self::S16Le => 2,
            Self::F32Le => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureState {
    Idle,
    Starting,
    Capturing,
    SourceChanged,
    DeviceDisconnected,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureOutputs {
    pub local_monitoring: bool,
    pub network_streaming: bool,
}

impl Default for CaptureOutputs {
    fn default() -> Self {
        Self {
            local_monitoring: true,
            network_streaming: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureSettings {
    pub source_id: Option<String>,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sample_format: AudioSampleFormat,
    pub buffer_frames: u32,
    pub target_latency_ms: u16,
    pub outputs: CaptureOutputs,
}

impl CaptureSettings {
    pub fn bytes_per_frame(&self) -> usize {
        self.channels as usize * self.sample_format.bytes_per_sample()
    }

    pub fn chunk_bytes(&self) -> usize {
        self.buffer_frames as usize * self.bytes_per_frame()
    }
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            source_id: None,
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: AudioSampleFormat::S16Le,
            buffer_frames: 480,
            target_latency_ms: 50,
            outputs: CaptureOutputs::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub sequence: u64,
    pub captured_at: Duration,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sample_format: AudioSampleFormat,
    pub payload: Vec<u8>,
    pub stats: AudioFrameStats,
}

impl AudioFrame {
    pub fn from_payload(
        sequence: u64,
        captured_at: Duration,
        settings: &CaptureSettings,
        payload: Vec<u8>,
    ) -> Self {
        let stats = AudioFrameStats::from_payload(settings.sample_format, &payload);

        Self {
            sequence,
            captured_at,
            sample_rate_hz: settings.sample_rate_hz,
            channels: settings.channels,
            sample_format: settings.sample_format,
            payload,
            stats,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioFrameStats {
    pub peak_amplitude: f32,
    pub rms_amplitude: f32,
}

impl AudioFrameStats {
    pub fn from_payload(sample_format: AudioSampleFormat, payload: &[u8]) -> Self {
        match sample_format {
            AudioSampleFormat::S16Le => stats_from_s16le(payload),
            AudioSampleFormat::F32Le => stats_from_f32le(payload),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaptureStats {
    pub state: CaptureState,
    pub frames_emitted: u64,
    pub bytes_captured: u64,
    pub last_frame_stats: AudioFrameStats,
    pub last_error: Option<String>,
}

impl Default for CaptureState {
    fn default() -> Self {
        Self::Idle
    }
}

fn stats_from_s16le(payload: &[u8]) -> AudioFrameStats {
    let mut peak = 0.0_f32;
    let mut sum_squares = 0.0_f32;
    let mut count = 0_u64;

    for sample in payload.chunks_exact(2) {
        let value = i16::from_le_bytes([sample[0], sample[1]]);
        let amplitude = (value as f32 / i16::MAX as f32).abs().min(1.0);
        peak = peak.max(amplitude);
        sum_squares += amplitude * amplitude;
        count += 1;
    }

    stats_from_accumulators(peak, sum_squares, count)
}

fn stats_from_f32le(payload: &[u8]) -> AudioFrameStats {
    let mut peak = 0.0_f32;
    let mut sum_squares = 0.0_f32;
    let mut count = 0_u64;

    for sample in payload.chunks_exact(4) {
        let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
        let amplitude = value.abs().min(1.0);
        peak = peak.max(amplitude);
        sum_squares += amplitude * amplitude;
        count += 1;
    }

    stats_from_accumulators(peak, sum_squares, count)
}

fn stats_from_accumulators(peak: f32, sum_squares: f32, count: u64) -> AudioFrameStats {
    if count == 0 {
        return AudioFrameStats::default();
    }

    AudioFrameStats {
        peak_amplitude: peak,
        rms_amplitude: (sum_squares / count as f32).sqrt(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_settings_make_buffering_explicit() {
        let settings = CaptureSettings::default();

        assert_eq!(settings.bytes_per_frame(), 4);
        assert_eq!(settings.chunk_bytes(), 1_920);
        assert!(settings.outputs.local_monitoring);
        assert!(settings.outputs.network_streaming);
    }

    #[test]
    fn frame_stats_are_computed_for_s16_payloads() {
        let payload = [
            0_i16.to_le_bytes(),
            i16::MAX.to_le_bytes(),
            (-i16::MAX).to_le_bytes(),
        ]
        .concat();

        let stats = AudioFrameStats::from_payload(AudioSampleFormat::S16Le, &payload);

        assert_eq!(stats.peak_amplitude, 1.0);
        assert!(stats.rms_amplitude > 0.8);
    }
}

