use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use flume::{Receiver, RecvTimeoutError, Sender};
use synchrosonic_core::{
    config::ReceiverConfig, services::ReceiverService, ReceiverConnectionInfo, ReceiverError,
    ReceiverLatencyProfile, ReceiverMetrics, ReceiverServiceState, ReceiverSnapshot,
    ReceiverStreamConfig, ReceiverSyncSnapshot, ReceiverSyncState, ReceiverTransportEvent,
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

    pub fn set_playback_target(&mut self, target_id: Option<String>) -> Result<(), ReceiverError> {
        self.config.playback_target_id = target_id.clone();

        if let Some(command_tx) = &self.command_tx {
            command_tx
                .send(ReceiverCommand::SetPlaybackTarget(target_id.clone()))
                .map_err(|_| ReceiverError::ChannelClosed)?;
        }

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.playback_target_id = target_id;
            if snapshot.state == ReceiverServiceState::Idle {
                snapshot.last_error = None;
            }
        }

        Ok(())
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
    SetPlaybackTarget(Option<String>),
    Shutdown,
}

#[derive(Debug, Clone, Copy)]
struct SyncClock {
    anchor_local_instant: Instant,
    anchor_sender_timestamp_ms: u64,
}

impl SyncClock {
    fn new(anchor_local_instant: Instant, anchor_sender_timestamp_ms: u64) -> Self {
        Self {
            anchor_local_instant,
            anchor_sender_timestamp_ms,
        }
    }

    fn write_deadline(&self, sender_timestamp_ms: u64, target_buffer_ms: u16) -> Instant {
        let sender_offset_ms = sender_timestamp_ms.saturating_sub(self.anchor_sender_timestamp_ms);
        self.anchor_local_instant
            + Duration::from_millis(sender_offset_ms.saturating_add(target_buffer_ms as u64))
    }
}

struct ReceiverWorker {
    config: ReceiverConfig,
    playback_engine: Arc<dyn PlaybackEngine>,
    playback_sink: Option<Box<dyn PlaybackSink>>,
    buffer: ReceiverPacketBuffer,
    buffer_profile: ReceiverLatencyProfile,
    snapshot: Arc<Mutex<ReceiverSnapshot>>,
    metrics: ReceiverMetrics,
    sync: ReceiverSyncSnapshot,
    sync_clock: Option<SyncClock>,
    connection: Option<ReceiverConnectionInfo>,
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
        let buffer = ReceiverPacketBuffer::new(buffer_profile, &ReceiverStreamConfig::default());

        let worker = Self {
            config,
            playback_engine,
            playback_sink: None,
            buffer,
            buffer_profile,
            snapshot,
            metrics: ReceiverMetrics::default(),
            sync: ReceiverSyncSnapshot::from_profile(buffer_profile),
            sync_clock: None,
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

    fn handle_playback_target_change(&mut self, target_id: Option<String>) {
        self.config.playback_target_id = target_id;
        self.last_error = None;

        if matches!(self.state, ReceiverServiceState::Error) && self.connection.is_none() {
            self.state = ReceiverServiceState::Listening;
        }

        if self.connection.is_some() {
            let request = PlaybackStartRequest {
                stream: self
                    .connection
                    .as_ref()
                    .expect("connection should still be present")
                    .stream
                    .clone(),
                target_id: self.config.playback_target_id.clone(),
                latency_ms: self.buffer_profile.playback_latency_ms,
            };

            let mut previous_sink = self.playback_sink.take();
            match self.playback_engine.start_stream(request) {
                Ok(new_sink) => {
                    if let Some(mut previous_sink) = previous_sink.take() {
                        if let Err(error) = previous_sink.stop() {
                            tracing::warn!(
                                error = %error,
                                "failed to stop previous receiver playback sink during target change"
                            );
                            self.last_error = Some(error.to_string());
                        }
                    }
                    self.playback_sink = Some(new_sink);
                    if self.state == ReceiverServiceState::Connected {
                        self.state = ReceiverServiceState::Buffering;
                    }
                }
                Err(error) => {
                    self.playback_sink = previous_sink;
                    self.last_error = Some(error.to_string());
                }
            }
        }

        self.sync_snapshot();
    }

    fn handle_connect(&mut self, connection: ReceiverConnectionInfo) {
        self.stop_playback_sink();
        self.buffer = ReceiverPacketBuffer::new(self.buffer_profile, &connection.stream);
        self.reset_sync_timeline(false);
        self.sync = ReceiverSyncSnapshot::from_profile(self.buffer_profile);
        self.sync.state = ReceiverSyncState::Priming;
        self.sync.requested_latency_ms = Some(connection.requested_latency_ms);

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

        let received_at = Instant::now();
        if self.sync_clock.is_none() {
            self.sync_clock = Some(SyncClock::new(received_at, packet.captured_at_ms));
        }
        self.sync.last_sender_timestamp_ms = Some(packet.captured_at_ms);
        self.sync.last_sender_capture_unix_ms = Some(packet.captured_at_unix_ms);

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
                self.last_transport_activity = Some(received_at);
                self.refresh_sync_buffer_metrics();

                if self.buffer.is_ready() {
                    if self.state != ReceiverServiceState::Playing {
                        self.state = ReceiverServiceState::Buffering;
                    }
                    if self.next_playback_deadline.is_none() {
                        self.next_playback_deadline = self
                            .buffer
                            .front()
                            .and_then(|front| self.write_deadline_for(front.packet.captured_at_ms));
                    }
                    if matches!(
                        self.sync.state,
                        ReceiverSyncState::Idle
                            | ReceiverSyncState::Priming
                            | ReceiverSyncState::Recovering
                    ) {
                        self.sync.state = ReceiverSyncState::Priming;
                    }
                } else if matches!(
                    self.state,
                    ReceiverServiceState::Connected | ReceiverServiceState::Buffering
                ) {
                    self.state = ReceiverServiceState::Buffering;
                    if self.sync.state != ReceiverSyncState::Recovering {
                        self.sync.state = ReceiverSyncState::Priming;
                    }
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
        self.refresh_sync_buffer_metrics();
        self.sync_snapshot();
    }

    fn drain_ready_packets(&mut self) {
        if self.connection.is_none() {
            return;
        }

        if self.sync_clock.is_none() {
            if self.state == ReceiverServiceState::Connected {
                self.state = ReceiverServiceState::Buffering;
            }
            return;
        }

        if self.next_playback_deadline.is_none() && !self.buffer.is_ready() {
            if self.state != ReceiverServiceState::Playing {
                self.state = ReceiverServiceState::Buffering;
            }
            if self.sync.state != ReceiverSyncState::Recovering {
                self.sync.state = ReceiverSyncState::Priming;
            }
            self.sync.schedule_error_ms = 0;
            return;
        }

        // The receiver uses the sender's capture timestamp as the media clock and anchors it
        // to the local `Instant` when the first packet of the current sync window arrives.
        // Every queued packet is then written to the playback process when its sender-side
        // media time plus the target jitter buffer becomes due locally.
        loop {
            let Some(front) = self.buffer.front() else {
                if self.state == ReceiverServiceState::Playing {
                    self.metrics.underruns += 1;
                    self.state = ReceiverServiceState::Buffering;
                    self.metrics.buffer_fill_percent = self.buffer.snapshot().fill_percent();
                    self.reset_sync_timeline(true);
                    self.sync.state = ReceiverSyncState::Recovering;
                    tracing::warn!(
                        underruns = self.metrics.underruns,
                        "receiver playback buffer underrun"
                    );
                }
                break;
            };

            let Some(deadline) = self.write_deadline_for(front.packet.captured_at_ms) else {
                break;
            };
            self.next_playback_deadline = Some(deadline);

            let now = Instant::now();
            let schedule_error_ms = signed_instant_delta_ms(now, deadline);
            self.sync.schedule_error_ms = schedule_error_ms;

            if schedule_error_ms > self.buffer_profile.late_packet_drop_ms as i32 {
                let dropped = self
                    .buffer
                    .pop()
                    .expect("buffer.front() already ensured a packet is available");
                self.sync.late_packet_drops += 1;
                self.sync.state = ReceiverSyncState::Late;
                self.metrics.buffer_fill_percent = self.buffer.snapshot().fill_percent();
                self.refresh_sync_buffer_metrics();
                tracing::warn!(
                    sequence = dropped.packet.sequence,
                    lateness_ms = schedule_error_ms,
                    late_packet_drops = self.sync.late_packet_drops,
                    "receiver dropped a stale packet to recover sync"
                );

                if self.buffer.front().is_none() {
                    self.reset_sync_timeline(true);
                    self.sync.state = ReceiverSyncState::Recovering;
                    break;
                }

                continue;
            }

            if now < deadline {
                if self.state != ReceiverServiceState::Playing {
                    self.state = ReceiverServiceState::Buffering;
                }
                break;
            }

            let packet = self
                .buffer
                .pop()
                .expect("buffer.front() already ensured a packet is available");
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
            self.sync.state = if schedule_error_ms > self.buffer_profile.latency_tolerance_ms as i32
            {
                ReceiverSyncState::Late
            } else {
                ReceiverSyncState::Locked
            };
            self.next_playback_deadline = None;
            self.refresh_sync_buffer_metrics();

            tracing::debug!(
                packets_played = self.metrics.packets_played,
                frames_played = self.metrics.frames_played,
                buffer_fill = self.metrics.buffer_fill_percent,
                schedule_error_ms,
                "receiver played a packet against the sender media clock"
            );
        }
    }

    fn write_deadline_for(&self, sender_timestamp_ms: u64) -> Option<Instant> {
        self.sync_clock.as_ref().map(|clock| {
            clock.write_deadline(sender_timestamp_ms, self.buffer_profile.target_buffer_ms)
        })
    }

    fn refresh_sync_buffer_metrics(&mut self) {
        let snapshot = self.buffer.snapshot();
        self.sync.queued_audio_ms = snapshot.queued_audio_ms;
        self.sync.buffer_delta_ms =
            snapshot.queued_audio_ms as i32 - self.sync.target_buffer_ms as i32;
    }

    fn reset_sync_timeline(&mut self, count_reset: bool) {
        self.sync_clock = None;
        self.next_playback_deadline = None;
        self.sync.schedule_error_ms = 0;
        self.sync.last_sender_timestamp_ms = None;
        self.sync.last_sender_capture_unix_ms = None;
        if count_reset {
            self.sync.sync_resets += 1;
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
        self.reset_sync_timeline(false);
        self.sync = ReceiverSyncSnapshot::from_profile(self.buffer_profile);
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
        self.reset_sync_timeline(false);
        self.sync = ReceiverSyncSnapshot::from_profile(self.buffer_profile);
        self.last_transport_activity = None;
        self.state = ReceiverServiceState::Error;
        self.last_error = Some(message);
        self.metrics.buffer_fill_percent = 0;
        self.sync.state = ReceiverSyncState::Recovering;
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
            snapshot.sync = self.sync.clone();
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
        self.reset_sync_timeline(false);
        self.sync = ReceiverSyncSnapshot::from_profile(self.buffer_profile);
        self.sync_snapshot();
    }
}

fn signed_instant_delta_ms(left: Instant, right: Instant) -> i32 {
    if left >= right {
        left.duration_since(right).as_millis().min(i32::MAX as u128) as i32
    } else {
        -((right.duration_since(left).as_millis().min(i32::MAX as u128)) as i32)
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
            Ok(ReceiverCommand::SetPlaybackTarget(target_id)) => {
                worker.handle_playback_target_change(target_id)
            }
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
        AudioError, AudioSampleFormat, ReceiverAudioPacket, ReceiverConnectionInfo,
        ReceiverLatencyPreset, ReceiverStreamConfig, ReceiverTransportEvent,
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
        ) -> Result<Box<dyn PlaybackSink>, AudioError> {
            Ok(Box::new(MockPlaybackSink {
                writes: Arc::clone(&self.writes),
            }))
        }
    }

    struct MockPlaybackSink {
        writes: Arc<AtomicUsize>,
    }

    impl PlaybackSink for MockPlaybackSink {
        fn write(&mut self, payload: &[u8]) -> Result<(), AudioError> {
            self.writes.fetch_add(payload.len(), Ordering::SeqCst);
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioError> {
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
    fn runtime_updates_selected_playback_target_before_start() {
        let engine = Arc::new(MockPlaybackEngine {
            writes: Arc::new(AtomicUsize::new(0)),
        });
        let mut runtime = ReceiverRuntime::with_playback_engine(ReceiverConfig::default(), engine);

        runtime
            .set_playback_target(Some("bluez_output.11_22_33_44_55_66.a2dp-sink".to_string()))
            .expect("playback target should update");

        assert_eq!(
            runtime.snapshot().playback_target_id.as_deref(),
            Some("bluez_output.11_22_33_44_55_66.a2dp-sink")
        );
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

        thread::sleep(Duration::from_millis(120));

        let snapshot = runtime.snapshot();
        assert!(matches!(
            snapshot.state,
            ReceiverServiceState::Buffering | ReceiverServiceState::Playing
        ));
        assert!(snapshot.metrics.packets_received >= 4);
        assert!(matches!(
            snapshot.sync.state,
            ReceiverSyncState::Priming
                | ReceiverSyncState::Locked
                | ReceiverSyncState::Late
                | ReceiverSyncState::Recovering
        ));
        assert!(snapshot.sync.expected_output_latency_ms > 0);
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
            requested_latency_ms: 150,
        }
    }

    fn test_packet(sequence: u64) -> ReceiverAudioPacket {
        let stream = test_connection().stream;
        ReceiverAudioPacket {
            sequence,
            captured_at_ms: sequence * 10,
            captured_at_unix_ms: 1_000 + sequence * 10,
            payload: vec![0; stream.packet_bytes_hint()],
        }
    }
}
