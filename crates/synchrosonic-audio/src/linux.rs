use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{self, Receiver, TryRecvError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use serde_json::Value;
use synchrosonic_core::{
    services::{AudioBackend, AudioCapture},
    AudioError, AudioFrame, AudioSampleFormat, AudioSource, AudioSourceKind, CaptureSettings,
    CaptureState, CaptureStats, PlaybackTarget,
};

impl LinuxAudioBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tools(pw_dump_bin: impl Into<PathBuf>, pw_record_bin: impl Into<PathBuf>) -> Self {
        Self {
            pw_dump_bin: pw_dump_bin.into(),
            pw_record_bin: pw_record_bin.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinuxAudioBackend {
    pw_dump_bin: PathBuf,
    pw_record_bin: PathBuf,
}

impl Default for LinuxAudioBackend {
    fn default() -> Self {
        Self {
            pw_dump_bin: PathBuf::from("pw-dump"),
            pw_record_bin: PathBuf::from("pw-record"),
        }
    }
}

impl AudioBackend for LinuxAudioBackend {
    fn backend_name(&self) -> &'static str {
        "linux-pipewire"
    }

    fn list_sources(&self) -> Result<Vec<AudioSource>, AudioError> {
        tracing::info!(
            backend = self.backend_name(),
            "enumerating PipeWire audio sources"
        );
        let dump = self.pipewire_dump()?;
        parse_pipewire_sources(&dump)
    }

    fn list_playback_targets(&self) -> Result<Vec<PlaybackTarget>, AudioError> {
        tracing::info!(
            backend = self.backend_name(),
            "enumerating PipeWire playback targets"
        );
        let dump = self.pipewire_dump()?;
        parse_pipewire_playback_targets(&dump)
    }

    fn start_capture(
        &self,
        mut settings: CaptureSettings,
    ) -> Result<Box<dyn AudioCapture>, AudioError> {
        validate_capture_settings(&settings)?;

        if settings.source_id.is_none() {
            let sources = self.list_sources()?;
            let selected = sources
                .iter()
                .find(|source| source.is_default && source.kind == AudioSourceKind::Monitor)
                .or_else(|| sources.iter().find(|source| source.is_default))
                .or_else(|| sources.first())
                .cloned()
                .ok_or_else(|| {
                    AudioError::BackendUnavailable(
                        "no PipeWire audio sources or monitor sources were found".to_string(),
                    )
                })?;
            settings.source_id = Some(selected.id);
        }

        let source_id = settings
            .source_id
            .clone()
            .ok_or_else(|| AudioError::InvalidSettings("source_id is required".to_string()))?;
        let format = pipewire_format_name(settings.sample_format);
        let latency = format!("{}ms", settings.target_latency_ms);

        tracing::info!(
            backend = self.backend_name(),
            source_id,
            rate = settings.sample_rate_hz,
            channels = settings.channels,
            format,
            latency,
            buffer_frames = settings.buffer_frames,
            local_monitoring = settings.outputs.local_monitoring,
            network_streaming = settings.outputs.network_streaming,
            "starting PipeWire capture"
        );

        let mut command = Command::new(&self.pw_record_bin);
        command
            .arg("--target")
            .arg(&source_id)
            .arg("--raw")
            .arg("--rate")
            .arg(settings.sample_rate_hz.to_string())
            .arg("--channels")
            .arg(settings.channels.to_string())
            .arg("--format")
            .arg(format)
            .arg("--latency")
            .arg(&latency)
            .arg("-")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => {
                AudioError::CommandUnavailable(self.pw_record_bin.display().to_string())
            }
            _ => AudioError::ProcessStart {
                command: self.pw_record_bin.display().to_string(),
                source,
            },
        })?;

        let stdout = child.stdout.take().ok_or_else(|| AudioError::ProcessIo {
            context: "opening pw-record stdout".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "pw-record stdout was not piped",
            ),
        })?;
        let stderr = child.stderr.take();

        let stats = Arc::new(Mutex::new(CaptureStats {
            state: CaptureState::Starting,
            ..CaptureStats::default()
        }));
        let child = Arc::new(Mutex::new(Some(child)));
        let (tx, rx) = mpsc::channel();

        let reader_stats = Arc::clone(&stats);
        let reader_settings = settings.clone();
        let reader_thread = thread::spawn(move || {
            capture_stdout_loop(stdout, reader_settings, reader_stats, tx);
        });

        let stderr_thread = stderr.map(|stderr| {
            let stats = Arc::clone(&stats);
            thread::spawn(move || {
                capture_stderr_loop(stderr, stats);
            })
        });

        Ok(Box::new(PipeWireCaptureSession {
            frames: rx,
            child,
            stats,
            reader_thread: Some(reader_thread),
            stderr_thread,
        }))
    }
}

#[derive(Debug)]
pub struct PipeWireCaptureSession {
    frames: Receiver<AudioFrame>,
    child: Arc<Mutex<Option<Child>>>,
    stats: Arc<Mutex<CaptureStats>>,
    reader_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl AudioCapture for PipeWireCaptureSession {
    fn recv_frame(&mut self) -> Result<AudioFrame, AudioError> {
        self.frames.recv().map_err(|_| AudioError::CaptureEnded)
    }

    fn try_recv_frame(&mut self) -> Result<Option<AudioFrame>, AudioError> {
        match self.frames.try_recv() {
            Ok(frame) => Ok(Some(frame)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(AudioError::CaptureEnded),
        }
    }

    fn stats(&self) -> CaptureStats {
        self.stats
            .lock()
            .map(|stats| stats.clone())
            .unwrap_or_else(|_| CaptureStats {
                state: CaptureState::Failed,
                last_error: Some("capture stats lock was poisoned".to_string()),
                ..CaptureStats::default()
            })
    }

    fn stop(&mut self) -> Result<(), AudioError> {
        tracing::info!("stopping PipeWire capture");
        if let Ok(mut stats) = self.stats.lock() {
            stats.state = CaptureState::Stopped;
        }

        let mut child_slot = self.child.lock().map_err(|_| {
            AudioError::BackendUnavailable("capture process lock was poisoned".to_string())
        })?;

        if let Some(mut child) = child_slot.take() {
            match child.try_wait() {
                Ok(Some(_status)) => {}
                Ok(None) => {
                    child.kill().map_err(|source| AudioError::ProcessIo {
                        context: "stopping pw-record".to_string(),
                        source,
                    })?;
                    child.wait().map_err(|source| AudioError::ProcessIo {
                        context: "waiting for pw-record to stop".to_string(),
                        source,
                    })?;
                }
                Err(source) => {
                    return Err(AudioError::ProcessIo {
                        context: "checking pw-record process state".to_string(),
                        source,
                    });
                }
            }
        }

        drop(child_slot);

        if let Some(reader_thread) = self.reader_thread.take() {
            let _ = reader_thread.join();
        }
        if let Some(stderr_thread) = self.stderr_thread.take() {
            let _ = stderr_thread.join();
        }

        Ok(())
    }
}

impl Drop for PipeWireCaptureSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl LinuxAudioBackend {
    fn pipewire_dump(&self) -> Result<String, AudioError> {
        let output = Command::new(&self.pw_dump_bin)
            .arg("--no-colors")
            .output()
            .map_err(|source| match source.kind() {
                std::io::ErrorKind::NotFound => {
                    AudioError::CommandUnavailable(self.pw_dump_bin.display().to_string())
                }
                _ => AudioError::ProcessStart {
                    command: self.pw_dump_bin.display().to_string(),
                    source,
                },
            })?;

        if !output.status.success() {
            return Err(AudioError::CommandFailed {
                command: self.pw_dump_bin.display().to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

fn validate_capture_settings(settings: &CaptureSettings) -> Result<(), AudioError> {
    if settings.sample_rate_hz == 0 {
        return Err(AudioError::InvalidSettings(
            "sample_rate_hz must be greater than zero".to_string(),
        ));
    }
    if settings.channels == 0 {
        return Err(AudioError::InvalidSettings(
            "channels must be greater than zero".to_string(),
        ));
    }
    if settings.buffer_frames == 0 {
        return Err(AudioError::InvalidSettings(
            "buffer_frames must be greater than zero".to_string(),
        ));
    }

    Ok(())
}

fn capture_stdout_loop(
    stdout: impl Read,
    settings: CaptureSettings,
    stats: Arc<Mutex<CaptureStats>>,
    tx: mpsc::Sender<AudioFrame>,
) {
    let start = Instant::now();
    let mut stdout = BufReader::new(stdout);
    let chunk_bytes = settings.chunk_bytes().max(settings.bytes_per_frame());
    let mut sequence = 0_u64;

    if let Ok(mut stats) = stats.lock() {
        stats.state = CaptureState::Capturing;
    }

    loop {
        let mut payload = vec![0_u8; chunk_bytes];
        match stdout.read(&mut payload) {
            Ok(0) => {
                let stopped = stats
                    .lock()
                    .map(|stats| stats.state == CaptureState::Stopped)
                    .unwrap_or(false);

                if stopped {
                    tracing::debug!("PipeWire capture stream stopped");
                } else {
                    tracing::warn!("PipeWire capture stream ended");
                    if let Ok(mut stats) = stats.lock() {
                        stats.state = CaptureState::DeviceDisconnected;
                        stats.last_error = Some("PipeWire capture stream ended".to_string());
                    }
                }
                break;
            }
            Ok(bytes_read) => {
                payload.truncate(bytes_read);
                let frame = AudioFrame::from_payload(sequence, start.elapsed(), &settings, payload);

                if let Ok(mut stats) = stats.lock() {
                    stats.frames_emitted += 1;
                    stats.bytes_captured += frame.payload.len() as u64;
                    stats.last_frame_stats = frame.stats;
                    stats.state = CaptureState::Capturing;
                }

                if sequence % 100 == 0 {
                    tracing::debug!(
                        sequence,
                        peak = frame.stats.peak_amplitude,
                        rms = frame.stats.rms_amplitude,
                        "PipeWire capture frames flowing"
                    );
                }

                if tx.send(frame).is_err() {
                    tracing::debug!("capture frame receiver dropped");
                    break;
                }

                sequence += 1;
            }
            Err(source) => {
                tracing::error!(error = %source, "failed to read from pw-record stdout");
                if let Ok(mut stats) = stats.lock() {
                    stats.state = CaptureState::Failed;
                    stats.last_error = Some(source.to_string());
                }
                break;
            }
        }
    }
}

fn capture_stderr_loop(stderr: impl Read, stats: Arc<Mutex<CaptureStats>>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        match line {
            Ok(line) if !line.trim().is_empty() => {
                tracing::debug!(
                    target: "synchrosonic_audio::pipewire",
                    message = %line,
                    "pw-record stderr"
                );
            }
            Ok(_) => {}
            Err(source) => {
                if let Ok(mut stats) = stats.lock() {
                    stats.last_error = Some(source.to_string());
                }
                tracing::warn!(error = %source, "failed to read pw-record stderr");
                break;
            }
        }
    }
}

fn pipewire_format_name(sample_format: AudioSampleFormat) -> &'static str {
    match sample_format {
        AudioSampleFormat::S16Le => "s16",
        AudioSampleFormat::F32Le => "f32",
    }
}

fn parse_pipewire_sources(dump: &str) -> Result<Vec<AudioSource>, AudioError> {
    let objects: Vec<Value> = serde_json::from_str(dump)
        .map_err(|source| AudioError::BackendUnavailable(source.to_string()))?;
    let default_sink = default_node_name(&objects, "default.audio.sink");
    let default_source = default_node_name(&objects, "default.audio.source");

    let mut sources = Vec::new();
    for object in objects
        .iter()
        .filter(|object| object_type(object) == Some("PipeWire:Interface:Node"))
    {
        let Some(props) = object_props(object) else {
            continue;
        };
        let Some(media_class) = prop_str(props, "media.class") else {
            continue;
        };
        let Some(node_name) = prop_str(props, "node.name") else {
            continue;
        };

        match media_class {
            "Audio/Sink" => sources.push(AudioSource {
                id: node_name.to_string(),
                display_name: format!("{} (monitor)", display_name(props, node_name)),
                kind: AudioSourceKind::Monitor,
                is_default: default_sink.as_deref() == Some(node_name),
            }),
            "Audio/Source" => sources.push(AudioSource {
                id: node_name.to_string(),
                display_name: display_name(props, node_name),
                kind: AudioSourceKind::Microphone,
                is_default: default_source.as_deref() == Some(node_name),
            }),
            _ => {}
        }
    }

    sources.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| capture_source_rank(left.kind).cmp(&capture_source_rank(right.kind)))
            .then_with(|| left.display_name.cmp(&right.display_name))
    });

    Ok(sources)
}

fn capture_source_rank(kind: AudioSourceKind) -> u8 {
    match kind {
        AudioSourceKind::Monitor => 0,
        AudioSourceKind::Microphone => 1,
        AudioSourceKind::Application => 2,
    }
}

fn parse_pipewire_playback_targets(dump: &str) -> Result<Vec<PlaybackTarget>, AudioError> {
    let objects: Vec<Value> = serde_json::from_str(dump)
        .map_err(|source| AudioError::BackendUnavailable(source.to_string()))?;
    let default_sink = default_node_name(&objects, "default.audio.sink");

    let mut targets = Vec::new();
    for object in objects
        .iter()
        .filter(|object| object_type(object) == Some("PipeWire:Interface:Node"))
    {
        let Some(props) = object_props(object) else {
            continue;
        };
        if prop_str(props, "media.class") != Some("Audio/Sink") {
            continue;
        }
        let Some(node_name) = prop_str(props, "node.name") else {
            continue;
        };

        targets.push(PlaybackTarget {
            id: node_name.to_string(),
            display_name: display_name(props, node_name),
            is_default: default_sink.as_deref() == Some(node_name),
        });
    }

    targets.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });

    Ok(targets)
}

fn object_type(object: &Value) -> Option<&str> {
    object.get("type")?.as_str()
}

fn object_props(object: &Value) -> Option<&serde_json::Map<String, Value>> {
    object
        .get("info")
        .and_then(|info| info.get("props"))
        .and_then(Value::as_object)
        .or_else(|| object.get("props").and_then(Value::as_object))
}

fn prop_str<'a>(props: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    props.get(key)?.as_str()
}

fn display_name(props: &serde_json::Map<String, Value>, fallback: &str) -> String {
    prop_str(props, "node.description")
        .or_else(|| prop_str(props, "node.nick"))
        .unwrap_or(fallback)
        .to_string()
}

fn default_node_name(objects: &[Value], key: &str) -> Option<String> {
    objects
        .iter()
        .filter(|object| object_type(object) == Some("PipeWire:Interface:Metadata"))
        .filter(|object| {
            object_props(object).and_then(|props| prop_str(props, "metadata.name"))
                == Some("default")
        })
        .flat_map(|object| {
            object
                .get("metadata")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find_map(|entry| {
            if entry.get("key").and_then(Value::as_str) != Some(key) {
                return None;
            }

            entry
                .get("value")
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_uses_pipewire_tool_names() {
        let backend = LinuxAudioBackend::new();

        assert_eq!(backend.backend_name(), "linux-pipewire");
    }

    #[test]
    fn pipewire_dump_parser_maps_sinks_to_monitor_sources() {
        let sources = parse_pipewire_sources(PIPEWIRE_DUMP_FIXTURE).expect("fixture should parse");

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].kind, AudioSourceKind::Monitor);
        assert_eq!(sources[0].id, "alsa_output.pci.stereo");
        assert!(sources[0].is_default);
        assert_eq!(sources[1].kind, AudioSourceKind::Microphone);
    }

    #[test]
    fn pipewire_dump_parser_maps_sinks_to_playback_targets() {
        let targets =
            parse_pipewire_playback_targets(PIPEWIRE_DUMP_FIXTURE).expect("fixture should parse");

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "alsa_output.pci.stereo");
        assert!(targets[0].is_default);
    }

    #[test]
    fn invalid_capture_settings_are_rejected() {
        let settings = CaptureSettings {
            channels: 0,
            ..CaptureSettings::default()
        };

        assert!(validate_capture_settings(&settings).is_err());
    }

    const PIPEWIRE_DUMP_FIXTURE: &str = r#"
[
  {
    "id": 40,
    "type": "PipeWire:Interface:Metadata",
    "props": { "metadata.name": "default" },
    "metadata": [
      {
        "subject": 0,
        "key": "default.audio.sink",
        "type": "Spa:String:JSON",
        "value": { "name": "alsa_output.pci.stereo" }
      },
      {
        "subject": 0,
        "key": "default.audio.source",
        "type": "Spa:String:JSON",
        "value": { "name": "alsa_input.pci.mic" }
      }
    ]
  },
  {
    "id": 59,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "node.name": "alsa_output.pci.stereo",
        "node.description": "Built-in Audio Analog Stereo",
        "media.class": "Audio/Sink"
      }
    }
  },
  {
    "id": 60,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "node.name": "alsa_input.pci.mic",
        "node.description": "Built-in Microphone",
        "media.class": "Audio/Source"
      }
    }
  }
]
"#;
}
