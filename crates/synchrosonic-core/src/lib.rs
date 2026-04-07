pub mod audio;
pub mod config;
pub mod diagnostics;
pub mod error;
pub mod models;
pub mod services;
pub mod state;

pub use audio::{
    AudioDevice, AudioDeviceDirection, AudioFrame, AudioFrameStats, AudioSampleFormat,
    CaptureOutputs, CaptureSettings, CaptureState, CaptureStats,
};
pub use config::AppConfig;
pub use diagnostics::{DiagnosticEvent, DiagnosticLevel};
pub use error::{AppError, AudioError, ConfigError, DiscoveryError, ReceiverError, TransportError};
pub use models::{
    AudioSource, AudioSourceKind, DeviceAvailability, DeviceCapabilities, DeviceId, DeviceRole,
    DeviceStatus, DiscoveredDevice, DiscoveryEvent, DiscoverySnapshot, PlaybackTarget,
    QualityPreset, TransportEndpoint, DISCOVERY_PROTOCOL_VERSION,
};
pub use state::{AppState, CastSessionState};
