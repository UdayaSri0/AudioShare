use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    thread::{self, JoinHandle},
};

use synchrosonic_core::{AudioError, ReceiverStreamConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackStartRequest {
    pub stream: ReceiverStreamConfig,
    pub target_id: Option<String>,
    pub latency_ms: u16,
}

pub trait PlaybackSink: Send {
    fn write(&mut self, payload: &[u8]) -> Result<(), AudioError>;
    fn stop(&mut self) -> Result<(), AudioError>;
}

pub trait PlaybackEngine: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn start_stream(
        &self,
        request: PlaybackStartRequest,
    ) -> Result<Box<dyn PlaybackSink>, AudioError>;
}

impl LinuxPlaybackEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pw_play_bin(pw_play_bin: impl Into<PathBuf>) -> Self {
        Self {
            pw_play_bin: pw_play_bin.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinuxPlaybackEngine {
    pw_play_bin: PathBuf,
}

impl Default for LinuxPlaybackEngine {
    fn default() -> Self {
        Self {
            pw_play_bin: PathBuf::from("pw-play"),
        }
    }
}

impl PlaybackEngine for LinuxPlaybackEngine {
    fn backend_name(&self) -> &'static str {
        "linux-pipewire-playback"
    }

    fn start_stream(
        &self,
        request: PlaybackStartRequest,
    ) -> Result<Box<dyn PlaybackSink>, AudioError> {
        validate_playback_request(&request)?;

        let format = pipewire_format_name(request.stream.sample_format);
        let latency = format!("{}ms", request.latency_ms);

        tracing::info!(
            backend = self.backend_name(),
            rate = request.stream.sample_rate_hz,
            channels = request.stream.channels,
            frames_per_packet = request.stream.frames_per_packet,
            latency,
            target = request.target_id.as_deref().unwrap_or("default"),
            "starting PipeWire playback stream"
        );

        let mut command = Command::new(&self.pw_play_bin);
        command
            .arg("--raw")
            .arg("--rate")
            .arg(request.stream.sample_rate_hz.to_string())
            .arg("--channels")
            .arg(request.stream.channels.to_string())
            .arg("--format")
            .arg(format)
            .arg("--latency")
            .arg(&latency);

        if let Some(target_id) = &request.target_id {
            command.arg("--target").arg(target_id);
        }

        command
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => {
                AudioError::CommandUnavailable(self.pw_play_bin.display().to_string())
            }
            _ => AudioError::ProcessStart {
                command: self.pw_play_bin.display().to_string(),
                source,
            },
        })?;

        let stdin = child.stdin.take().ok_or_else(|| AudioError::ProcessIo {
            context: "opening pw-play stdin".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "pw-play stdin was not piped",
            ),
        })?;
        let stderr_thread = child.stderr.take().map(|stderr| {
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    match line {
                        Ok(line) if !line.trim().is_empty() => {
                            tracing::debug!(
                                target: "synchrosonic_audio::pipewire",
                                message = %line,
                                "pw-play stderr"
                            );
                        }
                        Ok(_) => {}
                        Err(source) => {
                            tracing::warn!(error = %source, "failed to read pw-play stderr");
                            break;
                        }
                    }
                }
            })
        });

        Ok(Box::new(PipeWirePlaybackSink {
            child: Some(child),
            stdin: Some(stdin),
            stderr_thread,
        }))
    }
}

#[derive(Debug)]
struct PipeWirePlaybackSink {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl PlaybackSink for PipeWirePlaybackSink {
    fn write(&mut self, payload: &[u8]) -> Result<(), AudioError> {
        let stdin = self.stdin.as_mut().ok_or_else(|| AudioError::ProcessIo {
            context: "writing playback payload".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "playback stdin is not available",
            ),
        })?;

        stdin
            .write_all(payload)
            .map_err(|source| AudioError::ProcessIo {
                context: "writing audio payload to pw-play".to_string(),
                source,
            })
    }

    fn stop(&mut self) -> Result<(), AudioError> {
        self.stdin.take();

        if let Some(mut child) = self.child.take() {
            match child.try_wait() {
                Ok(Some(_status)) => {}
                Ok(None) => {
                    child.kill().map_err(|source| AudioError::ProcessIo {
                        context: "stopping pw-play".to_string(),
                        source,
                    })?;
                    child.wait().map_err(|source| AudioError::ProcessIo {
                        context: "waiting for pw-play to stop".to_string(),
                        source,
                    })?;
                }
                Err(source) => {
                    return Err(AudioError::ProcessIo {
                        context: "checking pw-play process state".to_string(),
                        source,
                    });
                }
            }
        }

        if let Some(stderr_thread) = self.stderr_thread.take() {
            let _ = stderr_thread.join();
        }

        Ok(())
    }
}

impl Drop for PipeWirePlaybackSink {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn validate_playback_request(request: &PlaybackStartRequest) -> Result<(), AudioError> {
    if request.stream.sample_rate_hz == 0 {
        return Err(AudioError::InvalidSettings(
            "sample_rate_hz must be greater than zero".to_string(),
        ));
    }
    if request.stream.channels == 0 {
        return Err(AudioError::InvalidSettings(
            "channels must be greater than zero".to_string(),
        ));
    }
    if request.stream.frames_per_packet == 0 {
        return Err(AudioError::InvalidSettings(
            "frames_per_packet must be greater than zero".to_string(),
        ));
    }

    Ok(())
}

fn pipewire_format_name(sample_format: synchrosonic_core::AudioSampleFormat) -> &'static str {
    match sample_format {
        synchrosonic_core::AudioSampleFormat::S16Le => "s16",
        synchrosonic_core::AudioSampleFormat::F32Le => "f32",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synchrosonic_core::AudioSampleFormat;

    #[test]
    fn backend_uses_pipewire_playback_name() {
        let engine = LinuxPlaybackEngine::new();

        assert_eq!(engine.backend_name(), "linux-pipewire-playback");
    }

    #[test]
    fn playback_request_requires_non_zero_stream_shape() {
        let request = PlaybackStartRequest {
            stream: ReceiverStreamConfig {
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: AudioSampleFormat::S16Le,
                frames_per_packet: 0,
            },
            target_id: None,
            latency_ms: 120,
        };

        assert!(validate_playback_request(&request).is_err());
    }
}
