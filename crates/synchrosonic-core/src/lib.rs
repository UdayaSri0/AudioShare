pub mod audio;
pub mod config;
pub mod diagnostics;
pub mod error;
pub mod models;
pub mod receiver;
pub mod services;
pub mod state;
pub mod streaming;

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
pub use receiver::{
    ReceiverAudioPacket, ReceiverBufferSnapshot, ReceiverConnectionInfo, ReceiverLatencyPreset,
    ReceiverLatencyProfile, ReceiverMetrics, ReceiverServiceState, ReceiverSnapshot,
    ReceiverStreamConfig, ReceiverTransportEvent,
};
pub use state::{AppState, CastSessionState};
pub use streaming::{
    StreamCodec, StreamMetrics, StreamSessionSnapshot, StreamSessionState,
    STREAM_PROTOCOL_VERSION,
};
