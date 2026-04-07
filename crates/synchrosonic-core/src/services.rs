use crate::{
    error::{AudioError, DiscoveryError, ReceiverError, TransportError},
    models::{AudioSource, PlaybackTarget, TransportEndpoint},
};

pub trait AudioBackend {
    fn backend_name(&self) -> &'static str;
    fn list_sources(&self) -> Result<Vec<AudioSource>, AudioError>;
    fn list_playback_targets(&self) -> Result<Vec<PlaybackTarget>, AudioError>;
}

pub trait DiscoveryService {
    fn service_type(&self) -> &str;
    fn planned_announcement_name(&self) -> &str;
    fn start(&mut self) -> Result<(), DiscoveryError>;
}

pub trait TransportService {
    fn endpoint(&self) -> Option<&TransportEndpoint>;
    fn start(&mut self) -> Result<(), TransportError>;
    fn stop(&mut self) -> Result<(), TransportError>;
}

pub trait ReceiverService {
    fn advertised_name(&self) -> &str;
    fn start(&mut self) -> Result<(), ReceiverError>;
}

