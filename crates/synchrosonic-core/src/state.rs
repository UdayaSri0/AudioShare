use serde::{Deserialize, Serialize};

use crate::{
    config::AppConfig,
    diagnostics::DiagnosticEvent,
    models::{DeviceId, DeviceStatus},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastSessionState {
    Idle,
    Preparing,
    Casting,
    Stopping,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceState {
    pub id: DeviceId,
    pub display_name: String,
    pub status: DeviceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    pub config: AppConfig,
    pub cast_session: CastSessionState,
    pub devices: Vec<DeviceState>,
    pub diagnostics: Vec<DiagnosticEvent>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            cast_session: CastSessionState::Idle,
            devices: Vec::new(),
            diagnostics: vec![DiagnosticEvent::info(
                "app",
                "Project scaffold initialized; audio streaming is not active yet.",
            )],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_starts_idle_with_diagnostic() {
        let state = AppState::new(AppConfig::default());

        assert_eq!(state.cast_session, CastSessionState::Idle);
        assert_eq!(state.diagnostics.len(), 1);
        assert_eq!(state.devices.len(), 0);
    }
}

