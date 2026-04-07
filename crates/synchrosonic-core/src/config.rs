use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    audio::{AudioSampleFormat, CaptureOutputs, CaptureSettings},
    error::ConfigError,
    models::QualityPreset,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub audio: AudioConfig,
    pub discovery: DiscoveryConfig,
    pub transport: TransportConfig,
    pub receiver: ReceiverConfig,
    pub ui: UiConfig,
    pub diagnostics: DiagnosticsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            discovery: DiscoveryConfig::default(),
            transport: TransportConfig::default(),
            receiver: ReceiverConfig::default(),
            ui: UiConfig::default(),
            diagnostics: DiagnosticsConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let encoded = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;
        fs::write(path, encoded).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConfig {
    pub preferred_source_id: Option<String>,
    pub local_playback_enabled: bool,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sample_format: AudioSampleFormat,
    pub capture_buffer_frames: u32,
    pub capture_latency_ms: u16,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            preferred_source_id: None,
            local_playback_enabled: true,
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: AudioSampleFormat::S16Le,
            capture_buffer_frames: 480,
            capture_latency_ms: 50,
        }
    }
}

impl AudioConfig {
    pub fn capture_settings(&self) -> CaptureSettings {
        CaptureSettings {
            source_id: self.preferred_source_id.clone(),
            sample_rate_hz: self.sample_rate_hz,
            channels: self.channels,
            sample_format: self.sample_format,
            buffer_frames: self.capture_buffer_frames,
            target_latency_ms: self.capture_latency_ms,
            outputs: CaptureOutputs {
                local_monitoring: self.local_playback_enabled,
                network_streaming: true,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub service_type: String,
    pub stale_timeout_secs: u64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_type: "_synchrosonic._tcp.local.".to_string(),
            stale_timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportConfig {
    pub bind_host: String,
    pub stream_port: u16,
    pub quality: QualityPreset,
    pub target_latency_ms: u16,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            bind_host: "0.0.0.0".to_string(),
            stream_port: 51_700,
            quality: QualityPreset::Balanced,
            target_latency_ms: 150,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverConfig {
    pub enabled: bool,
    pub advertised_name: String,
}

impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            advertised_name: "SynchroSonic Receiver".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiConfig {
    pub prefer_dark_theme: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            prefer_dark_theme: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsConfig {
    pub verbose_logging: bool,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            verbose_logging: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_lan_streaming_defaults() {
        let config = AppConfig::default();

        assert!(config.discovery.enabled);
        assert_eq!(config.transport.stream_port, 51_700);
        assert_eq!(config.audio.sample_rate_hz, 48_000);
        assert_eq!(config.audio.capture_buffer_frames, 480);
        assert!(config.audio.local_playback_enabled);
    }

    #[test]
    fn config_round_trips_through_toml_file() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let mut config = AppConfig::default();
        config.receiver.enabled = true;
        config.receiver.advertised_name = "Office Receiver".to_string();

        config
            .save_to_path(&path)
            .expect("config should save to temp file");
        let loaded = AppConfig::load_from_path(&path).expect("config should load from temp file");

        assert_eq!(loaded, config);
    }
}
