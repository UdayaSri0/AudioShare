use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    audio::{AudioSampleFormat, CaptureOutputs, CaptureSettings},
    error::ConfigError,
    models::QualityPreset,
    receiver::ReceiverLatencyPreset,
};

pub const APP_CONFIG_SCHEMA_VERSION: u32 = 1;

fn current_config_schema_version() -> u32 {
    APP_CONFIG_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLoadReport {
    pub config: AppConfig,
    pub warnings: Vec<String>,
    pub repaired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    #[serde(default = "current_config_schema_version")]
    pub schema_version: u32,
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
            schema_version: APP_CONFIG_SCHEMA_VERSION,
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
        Ok(Self::load_with_report_from_path(path)?.config)
    }

    pub fn load_with_report_from_path(
        path: impl AsRef<Path>,
    ) -> Result<ConfigLoadReport, ConfigError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let config =
            toml::from_str::<AppConfig>(&contents).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            })?;

        if config.schema_version > APP_CONFIG_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedVersion {
                path: path.to_path_buf(),
                found: config.schema_version,
                supported: APP_CONFIG_SCHEMA_VERSION,
            });
        }

        Ok(config.repaired())
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let encoded = toml::to_string_pretty(&self.clone().repaired().config)
            .map_err(ConfigError::Serialize)?;
        fs::write(path, encoded).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn repaired(mut self) -> ConfigLoadReport {
        let original = self.clone();
        let defaults = AppConfig::default();
        let mut warnings = Vec::new();

        if self.schema_version != APP_CONFIG_SCHEMA_VERSION {
            warnings.push(format!(
                "Config schema version {} was upgraded to {}.",
                self.schema_version, APP_CONFIG_SCHEMA_VERSION
            ));
            self.schema_version = APP_CONFIG_SCHEMA_VERSION;
        }

        if self.audio.sample_rate_hz == 0 {
            warnings.push(format!(
                "audio.sample_rate_hz must be greater than zero; using {}.",
                defaults.audio.sample_rate_hz
            ));
            self.audio.sample_rate_hz = defaults.audio.sample_rate_hz;
        }
        if self.audio.channels == 0 {
            warnings.push(format!(
                "audio.channels must be greater than zero; using {}.",
                defaults.audio.channels
            ));
            self.audio.channels = defaults.audio.channels;
        }
        if self.audio.capture_buffer_frames == 0 {
            warnings.push(format!(
                "audio.capture_buffer_frames must be greater than zero; using {}.",
                defaults.audio.capture_buffer_frames
            ));
            self.audio.capture_buffer_frames = defaults.audio.capture_buffer_frames;
        }
        if self.audio.capture_latency_ms == 0 {
            warnings.push(format!(
                "audio.capture_latency_ms must be greater than zero; using {}.",
                defaults.audio.capture_latency_ms
            ));
            self.audio.capture_latency_ms = defaults.audio.capture_latency_ms;
        }

        if self.discovery.service_type.trim().is_empty() {
            warnings.push(format!(
                "discovery.service_type cannot be empty; using {}.",
                defaults.discovery.service_type
            ));
            self.discovery.service_type = defaults.discovery.service_type;
        }
        if self.discovery.stale_timeout_secs == 0 {
            warnings.push(format!(
                "discovery.stale_timeout_secs must be greater than zero; using {}.",
                defaults.discovery.stale_timeout_secs
            ));
            self.discovery.stale_timeout_secs = defaults.discovery.stale_timeout_secs;
        }

        if self.transport.bind_host.trim().is_empty() {
            warnings.push(format!(
                "transport.bind_host cannot be empty; using {}.",
                defaults.transport.bind_host
            ));
            self.transport.bind_host = defaults.transport.bind_host;
        }
        if self.transport.stream_port == 0 {
            warnings.push(format!(
                "transport.stream_port must be greater than zero; using {}.",
                defaults.transport.stream_port
            ));
            self.transport.stream_port = defaults.transport.stream_port;
        }
        if self.transport.target_latency_ms == 0 {
            warnings.push(format!(
                "transport.target_latency_ms must be greater than zero; using {}.",
                defaults.transport.target_latency_ms
            ));
            self.transport.target_latency_ms = defaults.transport.target_latency_ms;
        }
        if self.transport.connect_timeout_ms < 250 {
            warnings.push(format!(
                "transport.connect_timeout_ms must be at least 250; using {}.",
                defaults.transport.connect_timeout_ms
            ));
            self.transport.connect_timeout_ms = defaults.transport.connect_timeout_ms;
        }
        if self.transport.heartbeat_interval_ms < 250 {
            warnings.push(format!(
                "transport.heartbeat_interval_ms must be at least 250; using {}.",
                defaults.transport.heartbeat_interval_ms
            ));
            self.transport.heartbeat_interval_ms = defaults.transport.heartbeat_interval_ms;
        }

        if self.receiver.advertised_name.trim().is_empty() {
            warnings.push(format!(
                "receiver.advertised_name cannot be empty; using {}.",
                defaults.receiver.advertised_name
            ));
            self.receiver.advertised_name = defaults.receiver.advertised_name;
        }
        if self.receiver.bind_host.trim().is_empty() {
            warnings.push(format!(
                "receiver.bind_host cannot be empty; using {}.",
                defaults.receiver.bind_host
            ));
            self.receiver.bind_host = defaults.receiver.bind_host;
        }
        if self.receiver.listen_port == 0 {
            warnings.push(format!(
                "receiver.listen_port must be greater than zero; using {}.",
                defaults.receiver.listen_port
            ));
            self.receiver.listen_port = defaults.receiver.listen_port;
        }

        if self.ui.last_view_name.trim().is_empty() {
            warnings.push(format!(
                "ui.last_view_name cannot be empty; using {}.",
                defaults.ui.last_view_name
            ));
            self.ui.last_view_name = defaults.ui.last_view_name;
        }

        ConfigLoadReport {
            repaired: self != original,
            config: self,
            warnings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub preferred_source_id: Option<String>,
    pub local_playback_enabled: bool,
    pub local_playback_target_id: Option<String>,
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
            local_playback_target_id: None,
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
#[serde(default)]
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
#[serde(default)]
pub struct TransportConfig {
    pub bind_host: String,
    pub stream_port: u16,
    pub quality: QualityPreset,
    pub target_latency_ms: u16,
    pub connect_timeout_ms: u16,
    pub heartbeat_interval_ms: u16,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            bind_host: "0.0.0.0".to_string(),
            stream_port: 51_700,
            quality: QualityPreset::Balanced,
            target_latency_ms: 150,
            connect_timeout_ms: 2_000,
            heartbeat_interval_ms: 1_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReceiverConfig {
    pub enabled: bool,
    pub start_on_launch: bool,
    pub advertised_name: String,
    pub bind_host: String,
    pub listen_port: u16,
    pub playback_target_id: Option<String>,
    pub latency_preset: ReceiverLatencyPreset,
}

impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            start_on_launch: false,
            advertised_name: "SynchroSonic Receiver".to_string(),
            bind_host: "0.0.0.0".to_string(),
            listen_port: 51_700,
            playback_target_id: None,
            latency_preset: ReceiverLatencyPreset::Balanced,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub prefer_dark_theme: bool,
    pub last_view_name: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            prefer_dark_theme: true,
            last_view_name: "dashboard".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
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

        assert_eq!(config.schema_version, APP_CONFIG_SCHEMA_VERSION);
        assert!(config.discovery.enabled);
        assert_eq!(config.transport.stream_port, 51_700);
        assert_eq!(config.transport.connect_timeout_ms, 2_000);
        assert_eq!(config.audio.sample_rate_hz, 48_000);
        assert_eq!(config.audio.capture_buffer_frames, 480);
        assert!(config.audio.local_playback_enabled);
        assert!(config.receiver.enabled);
        assert_eq!(config.receiver.listen_port, 51_700);
        assert_eq!(config.ui.last_view_name, "dashboard");
    }

    #[test]
    fn config_round_trips_through_toml_file() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let mut config = AppConfig::default();
        config.receiver.start_on_launch = true;
        config.receiver.advertised_name = "Office Receiver".to_string();
        config.ui.last_view_name = "receiver".to_string();

        config
            .save_to_path(&path)
            .expect("config should save to temp file");
        let loaded = AppConfig::load_from_path(&path).expect("config should load from temp file");

        assert_eq!(loaded, config);
    }

    #[test]
    fn config_loader_repairs_invalid_values() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
schema_version = 1

[audio]
sample_rate_hz = 0
channels = 0
capture_buffer_frames = 0
capture_latency_ms = 0

[discovery]
service_type = ""
stale_timeout_secs = 0

[transport]
bind_host = ""
stream_port = 0
target_latency_ms = 0
connect_timeout_ms = 0
heartbeat_interval_ms = 0

[receiver]
advertised_name = ""
bind_host = ""
listen_port = 0

[ui]
last_view_name = ""
"#,
        )
        .expect("config fixture should save");

        let report = AppConfig::load_with_report_from_path(&path)
            .expect("config should repair invalid values");

        assert!(report.repaired);
        assert!(!report.warnings.is_empty());
        assert_eq!(report.config.audio.sample_rate_hz, 48_000);
        assert_eq!(report.config.audio.channels, 2);
        assert_eq!(report.config.transport.stream_port, 51_700);
        assert_eq!(report.config.receiver.listen_port, 51_700);
        assert_eq!(report.config.ui.last_view_name, "dashboard");
    }

    #[test]
    fn config_loader_rejects_future_schema_versions() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            format!(
                r#"
schema_version = {}
"#,
                APP_CONFIG_SCHEMA_VERSION + 1
            ),
        )
        .expect("config fixture should save");

        let error = AppConfig::load_with_report_from_path(&path)
            .expect_err("future schema should be rejected");
        match error {
            ConfigError::UnsupportedVersion {
                found, supported, ..
            } => {
                assert_eq!(found, APP_CONFIG_SCHEMA_VERSION + 1);
                assert_eq!(supported, APP_CONFIG_SCHEMA_VERSION);
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
