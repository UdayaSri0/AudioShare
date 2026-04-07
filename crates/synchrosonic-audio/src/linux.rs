use synchrosonic_core::{
    services::AudioBackend, AudioError, AudioSource, PlaybackTarget,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxAudioBackend;

impl LinuxAudioBackend {
    pub fn new() -> Self {
        Self
    }
}

impl AudioBackend for LinuxAudioBackend {
    fn backend_name(&self) -> &'static str {
        "linux-pipewire"
    }

    fn list_sources(&self) -> Result<Vec<AudioSource>, AudioError> {
        Err(AudioError::NotActive(
            "PipeWire source enumeration belongs to the Linux audio capture phase".to_string(),
        ))
    }

    fn list_playback_targets(&self) -> Result<Vec<PlaybackTarget>, AudioError> {
        Err(AudioError::NotActive(
            "PipeWire playback target enumeration belongs to the Linux audio capture phase"
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_is_honest_about_inactive_pipewire_integration() {
        let backend = LinuxAudioBackend::new();

        assert_eq!(backend.backend_name(), "linux-pipewire");
        assert!(backend.list_sources().is_err());
        assert!(backend.list_playback_targets().is_err());
    }
}

