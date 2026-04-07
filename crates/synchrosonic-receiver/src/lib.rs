use synchrosonic_core::{
    config::ReceiverConfig, services::ReceiverService, ReceiverError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverRuntimeState {
    Disabled,
    Idle,
    Listening,
}

#[derive(Debug, Clone)]
pub struct ReceiverRuntime {
    config: ReceiverConfig,
    state: ReceiverRuntimeState,
}

impl ReceiverRuntime {
    pub fn new(config: ReceiverConfig) -> Self {
        let state = if config.enabled {
            ReceiverRuntimeState::Idle
        } else {
            ReceiverRuntimeState::Disabled
        };

        Self { config, state }
    }

    pub fn state(&self) -> ReceiverRuntimeState {
        self.state
    }
}

impl ReceiverService for ReceiverRuntime {
    fn advertised_name(&self) -> &str {
        &self.config.advertised_name
    }

    fn start(&mut self) -> Result<(), ReceiverError> {
        if !self.config.enabled {
            return Err(ReceiverError::NotActive(
                "receiver mode is disabled in the current configuration".to_string(),
            ));
        }

        Err(ReceiverError::NotActive(
            "receiver playback and inbound transport belong to the receiver-mode phase"
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_respects_disabled_default() {
        let runtime = ReceiverRuntime::new(ReceiverConfig::default());

        assert_eq!(runtime.state(), ReceiverRuntimeState::Disabled);
        assert_eq!(runtime.advertised_name(), "SynchroSonic Receiver");
    }
}

