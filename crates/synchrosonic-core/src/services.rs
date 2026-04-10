use crate::{
    audio::{AudioFrame, CaptureSettings, CaptureStats},
    error::{AudioError, DiscoveryError, ReceiverError, TransportError},
    models::{AudioSource, DiscoveryEvent, DiscoverySnapshot, PlaybackTarget, TransportEndpoint},
    receiver::{ReceiverServiceState, ReceiverSnapshot, ReceiverTransportEvent},
};

pub trait AudioCapture: Send {
    fn recv_frame(&mut self) -> Result<AudioFrame, AudioError>;
    fn try_recv_frame(&mut self) -> Result<Option<AudioFrame>, AudioError>;
    fn stats(&self) -> CaptureStats;
    fn stop(&mut self) -> Result<(), AudioError>;
}

pub trait AudioBackend {
    fn backend_name(&self) -> &'static str;
    fn list_sources(&self) -> Result<Vec<AudioSource>, AudioError>;
    fn list_playback_targets(&self) -> Result<Vec<PlaybackTarget>, AudioError>;
    fn start_capture(&self, settings: CaptureSettings)
        -> Result<Box<dyn AudioCapture>, AudioError>;
}

pub trait DiscoveryService {
    fn service_type(&self) -> &str;
    fn planned_announcement_name(&self) -> &str;
    fn start(&mut self) -> Result<(), DiscoveryError>;
    fn stop(&mut self) -> Result<(), DiscoveryError>;
    fn poll_event(&mut self) -> Result<Option<DiscoveryEvent>, DiscoveryError>;
    fn prune_stale(&mut self) -> Result<Vec<DiscoveryEvent>, DiscoveryError>;
    fn snapshot(&self) -> DiscoverySnapshot;
}

pub trait TransportService {
    fn endpoint(&self) -> Option<&TransportEndpoint>;
    fn start(&mut self) -> Result<(), TransportError>;
    fn stop(&mut self) -> Result<(), TransportError>;
}

pub trait ReceiverService {
    fn advertised_name(&self) -> &str;
    fn state(&self) -> ReceiverServiceState;
    fn snapshot(&self) -> ReceiverSnapshot;
    fn start(&mut self) -> Result<(), ReceiverError>;
    fn stop(&mut self) -> Result<(), ReceiverError>;
    fn submit_transport_event(&self, event: ReceiverTransportEvent) -> Result<(), ReceiverError>;
}
