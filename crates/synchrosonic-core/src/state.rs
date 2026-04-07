use serde::{Deserialize, Serialize};

use crate::{
    audio::CaptureState,
    config::AppConfig,
    diagnostics::DiagnosticEvent,
    models::{AudioSource, AudioSourceKind, DeviceId, DeviceStatus},
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
    pub capture_state: CaptureState,
    pub audio_sources: Vec<AudioSource>,
    pub selected_audio_source_id: Option<String>,
    pub devices: Vec<DeviceState>,
    pub diagnostics: Vec<DiagnosticEvent>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            cast_session: CastSessionState::Idle,
            capture_state: CaptureState::Idle,
            audio_sources: Vec::new(),
            selected_audio_source_id: None,
            devices: Vec::new(),
            diagnostics: vec![DiagnosticEvent::info(
                "app",
                "Project scaffold initialized; audio capture is ready for backend enumeration.",
            )],
        }
    }

    pub fn set_audio_sources(&mut self, sources: Vec<AudioSource>) {
        if self.selected_audio_source_id.is_none() {
            self.selected_audio_source_id = sources
                .iter()
                .find(|source| source.is_default && source.kind == AudioSourceKind::Monitor)
                .or_else(|| sources.iter().find(|source| source.is_default))
                .or_else(|| sources.first())
                .map(|source| source.id.clone());
        }

        if let Some(selected_id) = &self.selected_audio_source_id {
            if !sources.iter().any(|source| &source.id == selected_id) {
                self.selected_audio_source_id = sources.first().map(|source| source.id.clone());
                self.capture_state = CaptureState::SourceChanged;
            }
        }

        self.config.audio.preferred_source_id = self.selected_audio_source_id.clone();
        self.audio_sources = sources;
    }

    pub fn select_audio_source(&mut self, source_id: impl Into<String>) -> bool {
        let source_id = source_id.into();
        if !self.audio_sources.iter().any(|source| source.id == source_id) {
            return false;
        }

        self.selected_audio_source_id = Some(source_id.clone());
        self.config.audio.preferred_source_id = Some(source_id);
        self.capture_state = CaptureState::SourceChanged;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_starts_idle_with_diagnostic() {
        let state = AppState::new(AppConfig::default());

        assert_eq!(state.cast_session, CastSessionState::Idle);
        assert_eq!(state.capture_state, CaptureState::Idle);
        assert_eq!(state.diagnostics.len(), 1);
        assert_eq!(state.devices.len(), 0);
    }

    #[test]
    fn state_selects_default_audio_source_from_backend_results() {
        let mut state = AppState::new(AppConfig::default());
        state.set_audio_sources(vec![
            AudioSource {
                id: "mic".to_string(),
                display_name: "Microphone".to_string(),
                kind: crate::models::AudioSourceKind::Microphone,
                is_default: false,
            },
            AudioSource {
                id: "speaker".to_string(),
                display_name: "Speakers (monitor)".to_string(),
                kind: crate::models::AudioSourceKind::Monitor,
                is_default: true,
            },
        ]);

        assert_eq!(state.selected_audio_source_id.as_deref(), Some("speaker"));
        assert_eq!(state.config.audio.preferred_source_id.as_deref(), Some("speaker"));
    }
}
