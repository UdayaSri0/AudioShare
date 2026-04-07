use synchrosonic_core::{
    services::TransportService, TransportEndpoint, TransportError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportSessionState {
    Idle,
    Starting,
    Active,
    Stopping,
}

#[derive(Debug, Clone)]
pub struct LanTransportService {
    endpoint: Option<TransportEndpoint>,
    state: TransportSessionState,
}

impl LanTransportService {
    pub fn new(endpoint: Option<TransportEndpoint>) -> Self {
        Self {
            endpoint,
            state: TransportSessionState::Idle,
        }
    }

    pub fn state(&self) -> TransportSessionState {
        self.state
    }
}

impl TransportService for LanTransportService {
    fn endpoint(&self) -> Option<&TransportEndpoint> {
        self.endpoint.as_ref()
    }

    fn start(&mut self) -> Result<(), TransportError> {
        Err(TransportError::NotActive(
            "LAN stream framing and sockets belong to the sender-to-receiver streaming phase"
                .to_string(),
        ))
    }

    fn stop(&mut self) -> Result<(), TransportError> {
        self.state = TransportSessionState::Idle;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_starts_idle_and_stop_is_idempotent() {
        let mut service = LanTransportService::new(None);

        assert_eq!(service.state(), TransportSessionState::Idle);
        service.stop().expect("stop should be safe when idle");
        assert_eq!(service.state(), TransportSessionState::Idle);
    }
}

