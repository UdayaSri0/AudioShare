use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use flume::{Receiver, RecvTimeoutError, Sender};
use synchrosonic_core::{
    config::ReceiverConfig,
    services::ReceiverService,
    ReceiverError, ReceiverLatencyProfile, ReceiverMetrics, ReceiverServiceState,
    ReceiverSnapshot, ReceiverTransportEvent,
};

use crate::{
    buffer::{BufferPushOutcome, ReceiverPacketBuffer},
    playback::{LinuxPlaybackEngine, PlaybackEngine, PlaybackSink, PlaybackStartRequest},
};

pub struct ReceiverRuntime {
    config: ReceiverConfig,
    playback_engine: Arc<dyn PlaybackEngine>,
    snapshot: Arc<Mutex<ReceiverSnapshot>>,
    command_tx: Option<Sender<ReceiverCommand>>,
    worker: Option<JoinHandle<()>>,
}

impl ReceiverRuntime {
    pub fn new(config: ReceiverConfig) -> Self {
        Self::with_playback_engine(config, Arc::new(LinuxPlaybackEngine::new()))
    }

    pub fn with_playback_engine(
        config: ReceiverConfig,
        playback_engine: Arc<dyn PlaybackEngine>,
    ) -> Self {
        let mut snapshot = ReceiverSnapshot::from_config(&config);
        snapshot.playback_backend = Some(playback_engine.backend_name().to_string());

        Self {
            config,
            playback_engine,
            snapshot: Arc::new(Mutex::new(snapshot)),
            command_tx: None,
            worker: None,
        }
    }

    pub fn snapshot(&self) -> ReceiverSnapshot {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_else(|_| {
                let mut snapshot = ReceiverSnapshot::from_config(&self.config);
                snapshot.playback_backend = Some(self.playback_engine.backend_name().to_string());
                snapshot
            })
    }
}

impl ReceiverService for ReceiverRuntime {
    fn advertised_name(&self) -> &str {
        &self.config.advertised_name
    }

    fn state(&self) -> ReceiverServiceState {
        self.snapshot().state
    }

    fn snapshot(&self) -> ReceiverSnapshot {
        ReceiverRuntime::snapshot(self)
    }

    fn start(&mut self) -> Result<(), ReceiverError> {
        if !self.config.enabled {
            return Err(ReceiverError::Disabled(
                "receiver mode is disabled in the current configuration".to_string(),
            ));
        }

        if self.command_tx.is_some() {
            return Ok(());
        }

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.state = ReceiverServiceState::Listening;
            snapshot.last_error = None;
        }

        let (command_tx, command_rx) = flume::unbounded();
        let config = self.config.clone();
        let snapshot = Arc::clone(&self.snapshot);
        let playback_engine = Arc::clone(&self.playback_engine);

        self.worker = Some(thread::spawn(move || {
            receiver_worker_loop(config, playback_engine, snapshot, command_rx);
        }));
        self.command_tx = Some(command_tx);

        Ok(())
    }

    fn stop(&mut self) -> Result<(), ReceiverError> {
        if let Some(command_tx) = self.command_tx.take() {
            command_tx
                .send(ReceiverCommand::Shutdown)
                .map_err(|_| ReceiverError::ChannelClosed)?;
        }

        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| ReceiverError::ThreadJoin)?;
        }

        let mut snapshot = ReceiverSnapshot::from_config(&self.config);
        snapshot.playback_backend = Some(self.playback_engine.backend_name().to_string());
        if let Ok(mut guard) = self.snapshot.lock() {
            *guard = snapshot;
        }

        Ok(())
    }

    fn submit_transport_event(&self, event: ReceiverTransportEvent) -> Result<(), ReceiverError> {
        let command_tx = self.command_tx.as_ref().ok_or(ReceiverError::NotStarted)?;
        command_tx
            .send(ReceiverCommand::Transport(event))
            .map_err(|_| ReceiverError::ChannelClosed)
    }
}

impl Drop for ReceiverRuntime {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

enum ReceiverCommand {
    Transport(ReceiverTransportEvent),
    Shutdown,
}

struct ReceiverWorker {
    config: ReceiverConfig,
    playback_engine: Arc<dyn PlaybackEngine>,
    playback_sink: Option<Box<dyn PlaybackSink>>,
    buffer: ReceiverPacketBuffer,
    buffer_profile: ReceiverLatencyProfile,
    snapshot: Arc<Mutex<ReceiverSnapshot>>,
    metrics: ReceiverMetrics,
    connection: Option<synchrosonic_core::ReceiverConnectionInfo>,
    state: ReceiverServiceState,
    next_playback_deadline: Option<Instant>,
    last_transport_activity: Option<Instant>,
    last_error: Option<String>,
}

impl ReceiverWorker {
    fn new(
        config: ReceiverConfig,
        playback_engine: Arc<dyn PlaybackEngine>,
        snapshot: Arc<Mutex<ReceiverSnapshot>>,
    ) -> Self {
        let buffer_profile = config.latency_preset.profile();
        let buffer = ReceiverPacketBuffer::new(buffer_profile);

        let worker = Self {
            config,
            playback_engine,
            playback_sink: None,
            buffer,
            buffer_profile,
            snapshot,
            metrics: ReceiverMetrics::default(),
            connection: None,
            state: ReceiverServiceState::Listening,
            next_playback_deadline: None,
            last_transport_activity: None,
            last_error: None,
        };
        worker.sync_snapshot();
        worker
    }

    fn handle_transport_event(&mut self, event: ReceiverTransportEvent) {
        match event {
            ReceiverTransportEvent::Connected(connection) => {
                self.handle_connect(connection);
            }
            ReceiverTransportEvent::AudioPacket(packet) => {
                self.handle_packet(packet);
            }
            ReceiverTransportEvent::KeepAlive => {
                self.last_transport_activity = Some(Instant::now());
            }
            ReceiverTransportEvent::Disconnected {
                reason,
                reconnect_suggested,
            } => {
                self.handle_disconnect(&reason, reconnect_suggested);
            }
            ReceiverTransportEvent::Error { message } => {
                self.set_error(message);
            }
        }

        self.sync_snapshot();
    }

    fn handle_connect(&mut self, connection: synchrosonic_core::ReceiverConnectionInfo) {
        self.stop_playback_sink();
        self.buffer.clear();
        self.next_playback_deadline = None;

        let request = PlaybackStartRequest {
            stream: connection.stream.clone(),
            target_id: self.config.playback_target_id.clone(),
            latency_ms: self.buffer_profile.playback_latency_ms,
        };

        match self.playback_engine.start_stream(request) {
            Ok(playback_sink) => {
                tracing::info!(
                    session_id = connection.session_id,
                    remote = ?connection.remote_addr,
                    rate = connection.stream.sample_rate_hz,
                    channels = connection.stream.channels,
                    frames_per_packet = connection.stream.frames_per_packet,
                    "receiver connected to inbound stream"
                );
                self.playback_sink = Some(playback_sink);
                self.connection = Some(connection);
                self.state = ReceiverServiceState::Connected;
                self.last_transport_activity = Some(Instant::now());
                self.last_error = None;
            }
            Err(error) => {
                self.connection = None;
                self.set_error(error.to_string());
            }
        }
    }

    fn handle_packet(&mut self, packet: synchrosonic_core::ReceiverAudioPacket) {
        let Some(connection) = &self.connection else {
            tracing::debug!(
                sequence = packet.sequence,
                "dropping receiver audio packet while no stream is connected"
            );
            return;
        };

        let frame_count = match packet.frame_count(&connection.stream) {
            Ok(frame_count) => frame_count,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        let payload_len = packet.payload.len() as u64;

        match self.buffer.push(packet, &connection.stream) {
            Ok(outcome) => {
                let snapshot = self.buffer.snapshot();
                if let BufferPushOutcome::DroppedOldest { dropped_sequence } = outcome {
                    self.metrics.overruns += 1;
                    tracing::warn!(
                        dropped_sequence,
                        overruns = self.metrics.overruns,
                        "receiver buffer overrun dropped oldest packet"
                    );
                }

                self.metrics.packets_received += 1;
                self.metrics.frames_received += frame_count as u64;
                self.metrics.bytes_received += payload_len;
                self.metrics.buffer_fill_percent = snapshot.fill_percent();
                self.last_transport_activity = Some(Instant::now());

                if self.buffer.is_ready() {
                    if self.state != ReceiverServiceState::Playing {
                        self.state = ReceiverServiceState::Buffering;
                    }
                    self.next_playback_deadline
                        .get_or_insert_with(Instant::now);
                } else if self.state == ReceiverServiceState::Connected {
                    self.state = ReceiverServiceState::Buffering;
                }

                tracing::debug!(
                    packets_received = self.metrics.packets_received,
                    frames_received = self.metrics.frames_received,
                    buffer_fill = self.metrics.buffer_fill_percent,
                    overruns = self.metrics.overruns,
                    "receiver buffered incoming packet"
                );
            }
            Err(error) => {
                self.set_error(error.to_string());
            }
        }
    }

    fn on_tick(&mut self) {
        self.drain_ready_packets();
        self.check_transport_timeout();
        self.sync_snapshot();
    }

    fn drain_ready_packets(&mut self) {
        let Some(connection) = &self.connection else {
            return;
        };

        let packet_duration = connection.stream.packet_duration();
        let packet_duration = if packet_duration.is_zero() {
            Duration::from_millis(10)
        } else {
            packet_duration
        };

        while let Some(deadline) = self.next_playback_deadline {
            if Instant::now() < deadline {
                break;
            }

            let Some(packet) = self.buffer.pop() else {
                self.metrics.underruns += 1;
                self.state = ReceiverServiceState::Buffering;
                self.metrics.buffer_fill_percent = self.buffer.snapshot().fill_percent();
                self.next_playback_deadline = None;
                tracing::warn!(
                    underruns = self.metrics.underruns,
                    "receiver playback buffer underrun"
                );
                break;
            };

            let Some(playback_sink) = self.playback_sink.as_mut() else {
                self.set_error("playback sink disappeared during active stream".to_string());
                return;
            };

            if let Err(error) = playback_sink.write(&packet.packet.payload) {
                self.set_error(error.to_string());
                return;
            }

            self.metrics.packets_played += 1;
            self.metrics.frames_played += packet.frame_count as u64;
            self.metrics.bytes_played += packet.packet.payload.len() as u64;
            self.metrics.buffer_fill_percent = self.buffer.snapshot().fill_percent();
            self.state = ReceiverServiceState::Playing;
            self.next_playback_deadline = Some(deadline + packet_duration);

            tracing::debug!(
                packets_played = self.metrics.packets_played,
                frames_played = self.metrics.frames_played,
                buffer_fill = self.metrics.buffer_fill_percent,
                "receiver played buffered packet"
            );
        }
    }

    fn check_transport_timeout(&mut self) {
        let Some(last_activity) = self.last_transport_activity else {
            return;
        };

        if self.connection.is_some()
            && last_activity.elapsed() >= self.buffer_profile.reconnect_grace_period
        {
            self.handle_disconnect("receiver transport timed out", true);
        }
    }

    fn handle_disconnect(&mut self, reason: &str, reconnect_suggested: bool) {
        if reconnect_suggested {
            self.metrics.reconnect_attempts += 1;
        }

        tracing::info!(
            reconnect_suggested,
            reason,
            reconnect_attempts = self.metrics.reconnect_attempts,
            "receiver disconnected from inbound stream"
        );

        self.stop_playback_sink();
        self.connection = None;
        self.buffer.clear();
        self.next_playback_deadline = None;
        self.last_transport_activity = None;
        self.state = ReceiverServiceState::Listening;
        self.metrics.buffer_fill_percent = 0;
        self.last_error = if reconnect_suggested {
            Some(reason.to_string())
        } else {
            None
        };
    }

    fn set_error(&mut self, message: String) {
        tracing::error!(error = %message, "receiver worker entered error state");
        self.stop_playback_sink();
        self.connection = None;
        self.buffer.clear();
        self.next_playback_deadline = None;
        self.last_transport_activity = None;
        self.state = ReceiverServiceState::Error;
        self.last_error = Some(message);
        self.metrics.buffer_fill_percent = 0;
    }

    fn stop_playback_sink(&mut self) {
        if let Some(mut playback_sink) = self.playback_sink.take() {
            if let Err(error) = playback_sink.stop() {
                tracing::warn!(error = %error, "failed to stop receiver playback sink");
                self.last_error = Some(error.to_string());
            }
        }
    }

    fn next_wait_duration(&self) -> Duration {
        let mut wait = Duration::from_millis(200);

        if let Some(deadline) = self.next_playback_deadline {
            wait = wait.min(deadline.saturating_duration_since(Instant::now()));
        }

        if let Some(last_activity) = self.last_transport_activity {
            let timeout = self
                .buffer_profile
                .reconnect_grace_period
                .saturating_sub(last_activity.elapsed());
            wait = wait.min(timeout);
        }

        wait.max(Duration::from_millis(5))
    }

    fn sync_snapshot(&self) {
        let buffer = self.buffer.snapshot();
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.state = self.state;
            snapshot.connection = self.connection.clone();
            snapshot.buffer = buffer;
            snapshot.metrics = ReceiverMetrics {
                buffer_fill_percent: buffer.fill_percent(),
                ..self.metrics
            };
            snapshot.last_error = self.last_error.clone();
        }
    }

    fn shutdown(mut self) {
        self.stop_playback_sink();
        self.buffer.clear();
        self.connection = None;
        self.state = ReceiverServiceState::Idle;
        self.metrics.buffer_fill_percent = 0;
        self.sync_snapshot();
    }
}

fn receiver_worker_loop(
    config: ReceiverConfig,
    playback_engine: Arc<dyn PlaybackEngine>,
    snapshot: Arc<Mutex<ReceiverSnapshot>>,
    command_rx: Receiver<ReceiverCommand>,
) {
    let mut worker = ReceiverWorker::new(config, playback_engine, snapshot);

    loop {
        match command_rx.recv_timeout(worker.next_wait_duration()) {
            Ok(ReceiverCommand::Transport(event)) => worker.handle_transport_event(event),
            Ok(ReceiverCommand::Shutdown) => break,
            Err(RecvTimeoutError::Timeout) => worker.on_tick(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    worker.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use synchrosonic_core::{
        AudioSampleFormat, ReceiverAudioPacket, ReceiverConnectionInfo, ReceiverLatencyPreset,
        ReceiverStreamConfig, ReceiverTransportEvent,
    };

    struct MockPlaybackEngine {
        writes: Arc<AtomicUsize>,
    }

    impl PlaybackEngine for MockPlaybackEngine {
        fn backend_name(&self) -> &'static str {
            "mock-playback"
        }

        fn start_stream(
            &self,
            _request: PlaybackStartRequest,
        ) -> Result<Box<dyn PlaybackSink>, ReceiverError> {
            Ok(Box::new(MockPlaybackSink {
                writes: Arc::clone(&self.writes),
            }))
        }
    }

    struct MockPlaybackSink {
        writes: Arc<AtomicUsize>,
    }

    impl PlaybackSink for MockPlaybackSink {
        fn write(&mut self, payload: &[u8]) -> Result<(), ReceiverError> {
            self.writes.fetch_add(payload.len(), Ordering::SeqCst);
            Ok(())
        }

        fn stop(&mut self) -> Result<(), ReceiverError> {
            Ok(())
        }
    }

    #[test]
    fn runtime_starts_and_stops_cleanly() {
        let mut config = ReceiverConfig::default();
        config.enabled = true;
        let writes = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(MockPlaybackEngine {
            writes: Arc::clone(&writes),
        });
        let mut runtime = ReceiverRuntime::with_playback_engine(config, engine);

        runtime.start().expect("runtime should start");
        assert_eq!(runtime.state(), ReceiverServiceState::Listening);

        runtime.stop().expect("runtime should stop");
        assert_eq!(runtime.state(), ReceiverServiceState::Idle);
        assert_eq!(writes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn runtime_moves_from_buffering_to_playing_when_packets_arrive() {
        let mut config = ReceiverConfig::default();
        config.enabled = true;
        config.latency_preset = ReceiverLatencyPreset::LowLatency;
        let writes = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(MockPlaybackEngine {
            writes: Arc::clone(&writes),
        });
        let mut runtime = ReceiverRuntime::with_playback_engine(config, engine);

        runtime.start().expect("runtime should start");
        runtime
            .submit_transport_event(ReceiverTransportEvent::Connected(test_connection()))
            .expect("connect should be accepted");

        for sequence in 0..4 {
            runtime
                .submit_transport_event(ReceiverTransportEvent::AudioPacket(test_packet(sequence)))
                .expect("packet should be accepted");
        }

        thread::sleep(Duration::from_millis(60));

        let snapshot = runtime.snapshot();
        assert!(matches!(
            snapshot.state,
            ReceiverServiceState::Buffering | ReceiverServiceState::Playing
        ));
        assert!(snapshot.metrics.packets_received >= 4);
        assert!(writes.load(Ordering::SeqCst) > 0);

        runtime.stop().expect("runtime should stop");
    }

    #[test]
    fn runtime_tracks_reconnect_attempts_on_disconnect() {
        let mut config = ReceiverConfig::default();
        config.enabled = true;
        let engine = Arc::new(MockPlaybackEngine {
            writes: Arc::new(AtomicUsize::new(0)),
        });
        let mut runtime = ReceiverRuntime::with_playback_engine(config, engine);

        runtime.start().expect("runtime should start");
        runtime
            .submit_transport_event(ReceiverTransportEvent::Connected(test_connection()))
            .expect("connect should be accepted");
        runtime
            .submit_transport_event(ReceiverTransportEvent::Disconnected {
                reason: "network path lost".to_string(),
                reconnect_suggested: true,
            })
            .expect("disconnect should be accepted");

        thread::sleep(Duration::from_millis(20));

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.state, ReceiverServiceState::Listening);
        assert_eq!(snapshot.metrics.reconnect_attempts, 1);

        runtime.stop().expect("runtime should stop");
    }

    fn test_connection() -> ReceiverConnectionInfo {
        ReceiverConnectionInfo {
            session_id: "session-1".to_string(),
            remote_addr: None,
            stream: ReceiverStreamConfig {
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: AudioSampleFormat::S16Le,
                frames_per_packet: 480,
            },
        }
    }

    fn test_packet(sequence: u64) -> ReceiverAudioPacket {
        let stream = test_connection().stream;
        ReceiverAudioPacket {
            sequence,
            captured_at_ms: sequence * 10,
            payload: vec![0; stream.packet_bytes_hint()],
        }
    }
}
