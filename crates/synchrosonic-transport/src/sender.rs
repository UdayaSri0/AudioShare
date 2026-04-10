use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use flume::{Receiver, RecvTimeoutError, Sender};
use synchrosonic_audio::{LinuxPlaybackEngine, PlaybackEngine, PlaybackStartRequest};
use synchrosonic_core::{
    config::TransportConfig,
    services::{AudioBackend, AudioCapture},
    AudioError, CaptureSettings, DeviceId, LocalMirrorState, QualityPreset, ReceiverStreamConfig,
    StreamCodec, StreamMetrics, StreamSessionSnapshot, StreamSessionState, StreamTargetHealth,
    StreamTargetSnapshot, TransportEndpoint, TransportError,
};

use crate::{
    fanout::{
        BufferedBranchQueue, BufferedPushOutcome, FanoutAudioFrame, LocalMirrorBranch,
        LocalMirrorEvent,
    },
    protocol::{
        decode_metadata, read_frame, write_message, AcceptMessage, AudioMessage, ErrorMessage,
        FrameKind, HeartbeatMessage, HelloMessage, StopMessage,
    },
};

const BRANCH_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(5);
const MIN_BRANCH_QUEUE_PACKETS: usize = 4;
const BRANCH_QUEUE_HEADROOM_PACKETS: usize = 2;
const MAX_BRANCH_QUEUE_PACKETS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderTarget {
    pub receiver_id: DeviceId,
    pub receiver_name: String,
    pub endpoint: TransportEndpoint,
}

impl SenderTarget {
    pub fn new(
        receiver_id: DeviceId,
        receiver_name: impl Into<String>,
        endpoint: TransportEndpoint,
    ) -> Self {
        Self {
            receiver_id,
            receiver_name: receiver_name.into(),
            endpoint,
        }
    }
}

pub struct LanSenderSession {
    config: TransportConfig,
    playback_engine: Arc<dyn PlaybackEngine>,
    local_playback_target_id: Option<String>,
    snapshot: Arc<Mutex<StreamSessionSnapshot>>,
    control_tx: Option<Sender<SenderCommand>>,
    worker: Option<JoinHandle<()>>,
}

impl LanSenderSession {
    pub fn new(config: TransportConfig) -> Self {
        Self::with_playback_engine(config, Arc::new(LinuxPlaybackEngine::new()))
    }

    pub fn with_playback_engine(
        config: TransportConfig,
        playback_engine: Arc<dyn PlaybackEngine>,
    ) -> Self {
        let mut snapshot = StreamSessionSnapshot::default();
        snapshot.local_mirror.playback_backend = Some(playback_engine.backend_name().to_string());

        Self {
            config,
            playback_engine,
            local_playback_target_id: None,
            snapshot: Arc::new(Mutex::new(snapshot)),
            control_tx: None,
            worker: None,
        }
    }

    pub fn snapshot(&self) -> StreamSessionSnapshot {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default()
    }

    pub fn start<B>(
        &mut self,
        backend: B,
        capture_settings: CaptureSettings,
        target: SenderTarget,
        sender_name: impl Into<String>,
    ) -> Result<(), TransportError>
    where
        B: AudioBackend + Send + Sync + 'static,
    {
        let sender_name = sender_name.into();
        let desired_stream = receiver_stream_config(&capture_settings);

        if let Some(control_tx) = &self.control_tx {
            let current_snapshot = self.snapshot();
            if let Some(current_stream) = &current_snapshot.stream {
                if current_stream != &desired_stream {
                    return Err(TransportError::Negotiation(
                        "changing capture stream parameters while the sender manager is active is not supported".to_string(),
                    ));
                }
            }

            control_tx
                .send(SenderCommand::AddTarget(target))
                .map_err(|_| TransportError::ChannelClosed)?;
            return Ok(());
        }

        let (control_tx, control_rx) = flume::unbounded();
        let manager_session_id = format!("sender-{}", now_unix_ms());
        let config = self.config.clone();
        let snapshot = Arc::clone(&self.snapshot);
        let playback_engine = Arc::clone(&self.playback_engine);
        let initial_target = target.clone();
        let local_playback_target_id = self.local_playback_target_id.clone();

        let mut seeded_snapshot = StreamSessionSnapshot::default();
        seeded_snapshot.session_id = Some(manager_session_id.clone());
        seeded_snapshot.stream = Some(desired_stream.clone());
        seeded_snapshot.local_mirror.desired_enabled = capture_settings.outputs.local_monitoring;
        seeded_snapshot.local_mirror.playback_target_id = local_playback_target_id.clone();
        seeded_snapshot.local_mirror.state = if capture_settings.outputs.local_monitoring {
            LocalMirrorState::Idle
        } else {
            LocalMirrorState::Disabled
        };
        seeded_snapshot.local_mirror.playback_backend =
            Some(self.playback_engine.backend_name().to_string());
        seeded_snapshot
            .targets
            .push(pending_target_snapshot(&initial_target));
        seeded_snapshot.state = StreamSessionState::Connecting;
        sync_snapshot(&self.snapshot, seeded_snapshot);

        self.worker = Some(thread::spawn(move || {
            let mut manager = SenderManager::new(
                backend,
                playback_engine,
                config,
                capture_settings,
                sender_name,
                manager_session_id,
                local_playback_target_id,
                snapshot,
                control_rx,
            );
            manager.queue_target_connect(initial_target);
            manager.run();
        }));
        self.control_tx = Some(control_tx);

        Ok(())
    }

    pub fn stop_target(&self, device_id: &DeviceId) -> Result<(), TransportError> {
        let control_tx = self.control_tx.as_ref().ok_or(TransportError::NotRunning)?;
        control_tx
            .send(SenderCommand::RemoveTarget(device_id.clone()))
            .map_err(|_| TransportError::ChannelClosed)
    }

    pub fn set_local_playback_enabled(&self, enabled: bool) -> Result<(), TransportError> {
        if let Some(control_tx) = &self.control_tx {
            control_tx
                .send(SenderCommand::SetLocalMirrorEnabled(enabled))
                .map_err(|_| TransportError::ChannelClosed)?;
        }

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.local_mirror.desired_enabled = enabled;
            if snapshot.state == StreamSessionState::Idle {
                snapshot.local_mirror.state = if enabled {
                    LocalMirrorState::Idle
                } else {
                    LocalMirrorState::Disabled
                };
            }
            if enabled && snapshot.local_mirror.state == LocalMirrorState::Disabled {
                snapshot.local_mirror.state = LocalMirrorState::Starting;
                snapshot.local_mirror.last_error = None;
            }
        }

        Ok(())
    }

    pub fn set_local_playback_target(
        &mut self,
        target_id: Option<String>,
    ) -> Result<(), TransportError> {
        self.local_playback_target_id = target_id.clone();

        if let Some(control_tx) = &self.control_tx {
            control_tx
                .send(SenderCommand::SetLocalMirrorTarget(target_id.clone()))
                .map_err(|_| TransportError::ChannelClosed)?;
        }

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.local_mirror.playback_target_id = target_id;
            snapshot.local_mirror.last_error = None;
        }

        Ok(())
    }

    pub fn set_quality_preset(&mut self, quality: QualityPreset) -> Result<(), TransportError> {
        self.config.quality = quality;

        if let Some(control_tx) = &self.control_tx {
            control_tx
                .send(SenderCommand::SetQualityPreset(quality))
                .map_err(|_| TransportError::ChannelClosed)?;
        }

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), TransportError> {
        if let Some(control_tx) = self.control_tx.take() {
            let _ = control_tx.send(SenderCommand::Shutdown);
        }

        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| TransportError::ThreadJoin)?;
        }

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.state = StreamSessionState::Idle;
            snapshot.targets.clear();
            finalize_local_mirror_state(&mut snapshot.local_mirror, false);
        }

        Ok(())
    }
}

impl Drop for LanSenderSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

enum SenderCommand {
    AddTarget(SenderTarget),
    RemoveTarget(DeviceId),
    SetLocalMirrorEnabled(bool),
    SetLocalMirrorTarget(Option<String>),
    SetQualityPreset(QualityPreset),
    Shutdown,
}

enum InboundControl {
    HeartbeatAck { nonce: u64, wire_bytes: u64 },
    Stop { reason: String, wire_bytes: u64 },
    Error { message: String, wire_bytes: u64 },
    Disconnected,
}

enum NetworkControl {
    Heartbeat { nonce: u64 },
    Stop { reason: String },
    Shutdown,
}

enum NetworkEvent {
    AudioSent { wire_bytes: u64 },
    HeartbeatSent { wire_bytes: u64 },
    StopSent { wire_bytes: u64 },
    Error(String),
}

struct NetworkBranch {
    queue: BufferedBranchQueue<FanoutAudioFrame>,
    control_tx: Sender<NetworkControl>,
    event_rx: Receiver<NetworkEvent>,
    worker: Option<JoinHandle<()>>,
}

impl NetworkBranch {
    fn new(stream: TcpStream, queue_capacity: usize) -> Self {
        let queue = BufferedBranchQueue::new(queue_capacity);
        let frame_rx = queue.receiver();
        let (control_tx, control_rx) = flume::unbounded();
        let (event_tx, event_rx) = flume::unbounded();
        let worker =
            thread::spawn(move || network_writer_loop(stream, control_rx, frame_rx, event_tx));

        Self {
            queue,
            control_tx,
            event_rx,
            worker: Some(worker),
        }
    }

    fn push_frame(&self, frame: FanoutAudioFrame) -> Result<BufferedPushOutcome, TransportError> {
        self.queue.push(frame)
    }

    fn send_heartbeat(&self, nonce: u64) -> Result<(), TransportError> {
        self.control_tx
            .send(NetworkControl::Heartbeat { nonce })
            .map_err(|_| TransportError::ChannelClosed)
    }

    fn send_stop(&self, reason: impl Into<String>) -> Result<(), TransportError> {
        self.control_tx
            .send(NetworkControl::Stop {
                reason: reason.into(),
            })
            .map_err(|_| TransportError::ChannelClosed)
    }

    fn snapshot(&self) -> synchrosonic_core::StreamBranchBufferSnapshot {
        self.queue.snapshot()
    }

    fn drain_events(&self) -> Vec<NetworkEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    fn stop_and_shutdown(&mut self, reason: impl Into<String>) -> Result<(), TransportError> {
        let _ = self.send_stop(reason);
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| TransportError::ThreadJoin)?;
        }
        self.queue.clear();
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), TransportError> {
        let _ = self.control_tx.send(NetworkControl::Shutdown);
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| TransportError::ThreadJoin)?;
        }
        self.queue.clear();
        Ok(())
    }
}

impl Drop for NetworkBranch {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

struct ManagedTargetSession {
    target: SenderTarget,
    snapshot: StreamTargetSnapshot,
    stream: TcpStream,
    network_branch: NetworkBranch,
    inbound_rx: Receiver<InboundControl>,
    reader: Option<JoinHandle<()>>,
    pending_heartbeats: HashMap<u64, Instant>,
    heartbeat_nonce: u64,
    last_heartbeat: Instant,
    started_at: Instant,
}

impl ManagedTargetSession {
    fn connect(
        target: SenderTarget,
        request: &TargetConnectRequest,
    ) -> Result<Self, StreamTargetSnapshot> {
        let mut snapshot = pending_target_snapshot(&target);
        let session_id = format!(
            "{}-{}-{}",
            request.manager_session_id,
            target.receiver_id,
            now_unix_ms()
        );

        let mut stream =
            TcpStream::connect_timeout(&target.endpoint.address, request.connect_timeout).map_err(
                |source| {
                    snapshot.state = StreamSessionState::Error;
                    snapshot.health = StreamTargetHealth::Unreachable;
                    snapshot.last_error = Some(
                        TransportError::Connect {
                            address: target.endpoint.address.to_string(),
                            source,
                        }
                        .to_string(),
                    );
                    snapshot.clone()
                },
            )?;
        stream.set_nodelay(true).map_err(|source| {
            snapshot.state = StreamSessionState::Error;
            snapshot.health = StreamTargetHealth::Error;
            snapshot.last_error = Some(
                TransportError::Io {
                    context: format!("enabling TCP_NODELAY for {}", target.endpoint.address),
                    source,
                }
                .to_string(),
            );
            snapshot.clone()
        })?;

        snapshot.state = StreamSessionState::Negotiating;

        let hello = HelloMessage::new(
            session_id.clone(),
            request.sender_name.clone(),
            request.stream_config.clone(),
            request.quality,
            request.target_latency_ms,
            request.heartbeat_interval_ms,
        );
        snapshot.metrics.bytes_sent += write_message(&mut stream, FrameKind::Hello, &hello, &[])
            .map_err(|error| {
                snapshot.state = StreamSessionState::Error;
                snapshot.health = StreamTargetHealth::Error;
                snapshot.last_error = Some(error.to_string());
                snapshot.clone()
            })? as u64;

        let accept_frame = read_frame(&mut stream).map_err(|error| {
            snapshot.state = StreamSessionState::Error;
            snapshot.health = StreamTargetHealth::Unreachable;
            snapshot.last_error = Some(error.to_string());
            snapshot.clone()
        })?;
        snapshot.metrics.bytes_received += accept_frame.wire_bytes as u64;
        snapshot.metrics.packets_received += 1;
        if accept_frame.kind != FrameKind::Accept {
            snapshot.state = StreamSessionState::Error;
            snapshot.health = StreamTargetHealth::Error;
            snapshot.last_error = Some("receiver did not respond with Accept".to_string());
            return Err(snapshot);
        }

        let accept: AcceptMessage = decode_metadata(&accept_frame).map_err(|error| {
            snapshot.state = StreamSessionState::Error;
            snapshot.health = StreamTargetHealth::Error;
            snapshot.last_error = Some(error.to_string());
            snapshot.clone()
        })?;
        validate_accept(&accept, &session_id, &request.stream_config).map_err(|error| {
            snapshot.state = StreamSessionState::Error;
            snapshot.health = StreamTargetHealth::Error;
            snapshot.last_error = Some(error.to_string());
            snapshot.clone()
        })?;

        snapshot.session_id = Some(session_id);
        snapshot.codec = Some(accept.codec);
        snapshot.stream = Some(accept.stream.clone());
        snapshot.state = StreamSessionState::Streaming;
        snapshot.health = StreamTargetHealth::Healthy;
        snapshot.last_error = None;

        let queue_capacity =
            branch_queue_capacity(accept.stream.packet_duration(), request.target_latency_ms);
        let network_branch = NetworkBranch::new(
            stream.try_clone().map_err(|source| {
                snapshot.state = StreamSessionState::Error;
                snapshot.health = StreamTargetHealth::Error;
                snapshot.last_error = Some(
                    TransportError::Io {
                        context: format!("cloning TCP stream for {}", target.endpoint.address),
                        source,
                    }
                    .to_string(),
                );
                snapshot.clone()
            })?,
            queue_capacity,
        );
        snapshot.network_buffer = network_branch.snapshot();

        let read_stream = stream.try_clone().map_err(|source| {
            snapshot.state = StreamSessionState::Error;
            snapshot.health = StreamTargetHealth::Error;
            snapshot.last_error = Some(
                TransportError::Io {
                    context: format!("cloning TCP stream for {}", target.endpoint.address),
                    source,
                }
                .to_string(),
            );
            snapshot.clone()
        })?;
        let (inbound_tx, inbound_rx) = flume::unbounded();
        let reader = thread::spawn(move || sender_reader_loop(read_stream, inbound_tx));

        Ok(Self {
            target,
            snapshot,
            stream,
            network_branch,
            inbound_rx,
            reader: Some(reader),
            pending_heartbeats: HashMap::new(),
            heartbeat_nonce: 0,
            last_heartbeat: Instant::now(),
            started_at: Instant::now(),
        })
    }

    fn tick(&mut self, heartbeat_interval: Duration) -> bool {
        while let Ok(inbound) = self.inbound_rx.try_recv() {
            match inbound {
                InboundControl::HeartbeatAck { nonce, wire_bytes } => {
                    self.snapshot.metrics.bytes_received += wire_bytes;
                    self.snapshot.metrics.packets_received += 1;
                    self.snapshot.metrics.keepalives_received += 1;
                    if let Some(sent_at) = self.pending_heartbeats.remove(&nonce) {
                        self.snapshot.metrics.latency_estimate_ms =
                            Some((sent_at.elapsed().as_millis() / 2) as u32);
                    }
                }
                InboundControl::Stop { reason, wire_bytes } => {
                    self.snapshot.metrics.bytes_received += wire_bytes;
                    self.snapshot.metrics.packets_received += 1;
                    self.set_error(
                        format!("receiver requested stop: {reason}"),
                        StreamTargetHealth::Unreachable,
                    );
                }
                InboundControl::Error {
                    message,
                    wire_bytes,
                } => {
                    self.snapshot.metrics.bytes_received += wire_bytes;
                    self.snapshot.metrics.packets_received += 1;
                    self.set_error(message, StreamTargetHealth::Error);
                }
                InboundControl::Disconnected => {
                    self.set_error(
                        "receiver disconnected unexpectedly".to_string(),
                        StreamTargetHealth::Unreachable,
                    );
                }
            }
        }

        for event in self.network_branch.drain_events() {
            match event {
                NetworkEvent::AudioSent { wire_bytes } => {
                    self.snapshot.metrics.bytes_sent += wire_bytes;
                    self.snapshot.metrics.packets_sent += 1;
                    self.snapshot.metrics.estimated_bitrate_bps =
                        bitrate_bps(self.snapshot.metrics.bytes_sent, self.started_at.elapsed());
                }
                NetworkEvent::HeartbeatSent { wire_bytes } => {
                    self.snapshot.metrics.bytes_sent += wire_bytes;
                    self.snapshot.metrics.keepalives_sent += 1;
                }
                NetworkEvent::StopSent { wire_bytes } => {
                    self.snapshot.metrics.bytes_sent += wire_bytes;
                }
                NetworkEvent::Error(message) => {
                    self.set_error(message, StreamTargetHealth::Unreachable);
                }
            }
        }

        if self.snapshot.state == StreamSessionState::Streaming
            && self.last_heartbeat.elapsed() >= heartbeat_interval
        {
            self.heartbeat_nonce += 1;
            self.pending_heartbeats
                .insert(self.heartbeat_nonce, Instant::now());
            if let Err(error) = self.network_branch.send_heartbeat(self.heartbeat_nonce) {
                self.set_error(error.to_string(), StreamTargetHealth::Unreachable);
            } else {
                self.last_heartbeat = Instant::now();
            }
        }

        self.snapshot.network_buffer = self.network_branch.snapshot();
        self.refresh_health();
        self.snapshot.state == StreamSessionState::Streaming
    }

    fn push_frame(&mut self, frame: FanoutAudioFrame) -> Result<(), TransportError> {
        handle_buffer_push(
            self.network_branch.push_frame(frame),
            &format!("target {}", self.target.receiver_id),
            &mut self.snapshot.network_buffer,
        )?;
        self.snapshot.network_buffer = self.network_branch.snapshot();
        self.refresh_health();
        Ok(())
    }

    fn shutdown_with_stop(mut self, reason: &str) -> StreamTargetSnapshot {
        self.snapshot.state = StreamSessionState::Stopping;
        let _ = self.network_branch.stop_and_shutdown(reason.to_string());
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.snapshot.network_buffer = self.network_branch.snapshot();
        self.snapshot
    }

    fn shutdown_immediate(mut self) -> StreamTargetSnapshot {
        let _ = self.network_branch.shutdown();
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.snapshot.network_buffer = self.network_branch.snapshot();
        self.snapshot
    }

    fn snapshot(&self) -> StreamTargetSnapshot {
        self.snapshot.clone()
    }

    fn set_error(&mut self, message: String, health: StreamTargetHealth) {
        self.snapshot.last_error = Some(message);
        self.snapshot.state = StreamSessionState::Error;
        self.snapshot.health = health;
    }

    fn refresh_health(&mut self) {
        if self.snapshot.state != StreamSessionState::Streaming {
            return;
        }

        self.snapshot.health = if self.snapshot.network_buffer.dropped_packets > 0
            || self.snapshot.metrics.packet_gaps > 0
        {
            StreamTargetHealth::Degraded
        } else {
            StreamTargetHealth::Healthy
        };
    }
}

enum TargetSessionEntry {
    Pending(StreamTargetSnapshot),
    Active(ManagedTargetSession),
    Failed(StreamTargetSnapshot),
}

impl TargetSessionEntry {
    fn snapshot(&self) -> StreamTargetSnapshot {
        match self {
            Self::Pending(snapshot) | Self::Failed(snapshot) => snapshot.clone(),
            Self::Active(session) => session.snapshot(),
        }
    }

    fn state(&self) -> StreamSessionState {
        match self {
            Self::Pending(snapshot) | Self::Failed(snapshot) => snapshot.state,
            Self::Active(session) => session.snapshot.state,
        }
    }
}

struct TargetSessionCollection {
    entries: HashMap<DeviceId, TargetSessionEntry>,
}

impl TargetSessionCollection {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn insert_pending(&mut self, snapshot: StreamTargetSnapshot) {
        self.entries.insert(
            snapshot.receiver_id.clone(),
            TargetSessionEntry::Pending(snapshot),
        );
    }

    fn insert_active(&mut self, session: ManagedTargetSession) {
        self.entries.insert(
            session.target.receiver_id.clone(),
            TargetSessionEntry::Active(session),
        );
    }

    fn insert_failed(&mut self, snapshot: StreamTargetSnapshot) {
        self.entries.insert(
            snapshot.receiver_id.clone(),
            TargetSessionEntry::Failed(snapshot),
        );
    }

    fn remove(&mut self, device_id: &DeviceId) -> Option<TargetSessionEntry> {
        self.entries.remove(device_id)
    }

    fn contains(&self, device_id: &DeviceId) -> bool {
        self.entries.contains_key(device_id)
    }

    fn state_for(&self, device_id: &DeviceId) -> Option<StreamSessionState> {
        self.entries.get(device_id).map(TargetSessionEntry::state)
    }

    fn has_active_targets(&self) -> bool {
        self.entries
            .values()
            .any(|entry| matches!(entry, TargetSessionEntry::Active(_)))
    }

    fn has_pending_targets(&self) -> bool {
        self.entries
            .values()
            .any(|entry| matches!(entry, TargetSessionEntry::Pending(_)))
    }

    fn active_ids(&self) -> Vec<DeviceId> {
        self.entries
            .iter()
            .filter_map(|(device_id, entry)| {
                if matches!(entry, TargetSessionEntry::Active(_)) {
                    Some(device_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn active_mut(&mut self, device_id: &DeviceId) -> Option<&mut ManagedTargetSession> {
        match self.entries.get_mut(device_id) {
            Some(TargetSessionEntry::Active(session)) => Some(session),
            _ => None,
        }
    }

    fn collect_snapshots(&self) -> Vec<StreamTargetSnapshot> {
        let mut snapshots = self
            .entries
            .values()
            .map(TargetSessionEntry::snapshot)
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.receiver_name
                .cmp(&right.receiver_name)
                .then_with(|| left.receiver_id.as_str().cmp(right.receiver_id.as_str()))
        });
        snapshots
    }
}

struct TargetConnectRequest {
    manager_session_id: String,
    sender_name: String,
    stream_config: ReceiverStreamConfig,
    connect_timeout: Duration,
    quality: synchrosonic_core::QualityPreset,
    target_latency_ms: u16,
    heartbeat_interval_ms: u16,
}

enum TargetConnectResult {
    Connected(ManagedTargetSession),
    Failed(StreamTargetSnapshot),
}

struct SenderManager<B> {
    backend: B,
    config: TransportConfig,
    capture_settings: CaptureSettings,
    manager_session_id: String,
    local_mirror_request: PlaybackStartRequest,
    local_mirror_branch: LocalMirrorBranch,
    capture: Option<Box<dyn AudioCapture>>,
    capture_started_unix_ms: Option<u64>,
    snapshot: Arc<Mutex<StreamSessionSnapshot>>,
    shared_snapshot: StreamSessionSnapshot,
    control_rx: Receiver<SenderCommand>,
    connect_tx: Sender<TargetConnectResult>,
    connect_rx: Receiver<TargetConnectResult>,
    connect_request: TargetConnectRequest,
    targets: TargetSessionCollection,
}

impl<B> SenderManager<B>
where
    B: AudioBackend + Send + Sync + 'static,
{
    fn new(
        backend: B,
        playback_engine: Arc<dyn PlaybackEngine>,
        config: TransportConfig,
        capture_settings: CaptureSettings,
        sender_name: String,
        manager_session_id: String,
        local_playback_target_id: Option<String>,
        snapshot: Arc<Mutex<StreamSessionSnapshot>>,
        control_rx: Receiver<SenderCommand>,
    ) -> Self {
        let stream_config = receiver_stream_config(&capture_settings);
        let local_mirror_request = PlaybackStartRequest {
            stream: stream_config.clone(),
            target_id: local_playback_target_id.clone(),
            latency_ms: capture_settings.target_latency_ms,
        };
        let local_mirror_branch = LocalMirrorBranch::new(
            Arc::clone(&playback_engine),
            branch_queue_capacity(
                stream_config.packet_duration(),
                capture_settings.target_latency_ms,
            ),
        );
        let (connect_tx, connect_rx) = flume::unbounded();
        let connect_request = TargetConnectRequest {
            manager_session_id: manager_session_id.clone(),
            sender_name,
            stream_config: stream_config.clone(),
            connect_timeout: Duration::from_millis(config.connect_timeout_ms.max(250) as u64),
            quality: config.quality,
            target_latency_ms: config.target_latency_ms,
            heartbeat_interval_ms: config.heartbeat_interval_ms,
        };

        let mut shared_snapshot = StreamSessionSnapshot::default();
        shared_snapshot.session_id = Some(manager_session_id);
        shared_snapshot.stream = Some(stream_config.clone());
        shared_snapshot.local_mirror.desired_enabled = capture_settings.outputs.local_monitoring;
        shared_snapshot.local_mirror.playback_target_id = local_playback_target_id;
        shared_snapshot.local_mirror.state = if capture_settings.outputs.local_monitoring {
            LocalMirrorState::Idle
        } else {
            LocalMirrorState::Disabled
        };
        shared_snapshot.local_mirror.playback_backend =
            Some(playback_engine.backend_name().to_string());

        Self {
            backend,
            config,
            capture_settings,
            manager_session_id: shared_snapshot
                .session_id
                .clone()
                .unwrap_or_else(|| "sender-session".to_string()),
            local_mirror_request,
            local_mirror_branch,
            capture: None,
            capture_started_unix_ms: None,
            snapshot,
            shared_snapshot,
            control_rx,
            connect_tx,
            connect_rx,
            connect_request,
            targets: TargetSessionCollection::new(),
        }
    }

    fn run(&mut self) {
        loop {
            if self.process_commands() {
                break;
            }

            self.process_connect_results();
            self.process_local_mirror_events();
            self.tick_targets();
            self.capture_and_fan_out();
            self.stop_capture_if_idle();
            self.refresh_snapshot();

            thread::sleep(BRANCH_IDLE_POLL_INTERVAL);
        }

        self.shutdown_all();
    }

    fn queue_target_connect(&mut self, target: SenderTarget) {
        let target_id = target.receiver_id.clone();
        if matches!(
            self.targets.state_for(&target_id),
            Some(StreamSessionState::Connecting)
                | Some(StreamSessionState::Negotiating)
                | Some(StreamSessionState::Streaming)
        ) {
            return;
        }

        let pending_snapshot = pending_target_snapshot(&target);
        self.targets.insert_pending(pending_snapshot);
        let connect_request = self.connect_request.clone();
        let connect_tx = self.connect_tx.clone();
        tracing::info!(
            receiver_id = %target.receiver_id,
            endpoint = %target.endpoint.address,
            "queueing multi-device sender target connection"
        );
        thread::spawn(move || {
            let result = ManagedTargetSession::connect(target, &connect_request);
            let _ = match result {
                Ok(session) => connect_tx.send(TargetConnectResult::Connected(session)),
                Err(snapshot) => connect_tx.send(TargetConnectResult::Failed(snapshot)),
            };
        });
    }

    fn process_commands(&mut self) -> bool {
        while let Ok(command) = self.control_rx.try_recv() {
            match command {
                SenderCommand::AddTarget(target) => {
                    self.queue_target_connect(target);
                }
                SenderCommand::RemoveTarget(device_id) => {
                    tracing::info!(receiver_id = %device_id, "removing sender target");
                    if let Some(entry) = self.targets.remove(&device_id) {
                        if let TargetSessionEntry::Active(session) = entry {
                            let _ = session.shutdown_with_stop("sender removed target");
                        }
                    }
                }
                SenderCommand::SetLocalMirrorEnabled(enabled) => {
                    self.shared_snapshot.local_mirror.desired_enabled = enabled;
                    self.shared_snapshot.local_mirror.last_error = None;
                    if self.capture.is_some() {
                        if enabled {
                            self.shared_snapshot.local_mirror.state = LocalMirrorState::Starting;
                            if let Err(error) = self
                                .local_mirror_branch
                                .start(self.local_mirror_request.clone())
                            {
                                self.shared_snapshot.local_mirror.state = LocalMirrorState::Error;
                                self.shared_snapshot.local_mirror.last_error =
                                    Some(error.to_string());
                            }
                        } else {
                            self.shared_snapshot.local_mirror.state = LocalMirrorState::Stopping;
                            if let Err(error) = self.local_mirror_branch.stop() {
                                self.shared_snapshot.local_mirror.state = LocalMirrorState::Error;
                                self.shared_snapshot.local_mirror.last_error =
                                    Some(error.to_string());
                            }
                        }
                    }
                }
                SenderCommand::SetLocalMirrorTarget(target_id) => {
                    self.local_mirror_request.target_id = target_id.clone();
                    self.shared_snapshot.local_mirror.playback_target_id = target_id;
                    self.shared_snapshot.local_mirror.last_error = None;
                    if self.capture.is_some() && self.shared_snapshot.local_mirror.desired_enabled {
                        self.shared_snapshot.local_mirror.state = LocalMirrorState::Starting;
                        if let Err(error) = self
                            .local_mirror_branch
                            .start(self.local_mirror_request.clone())
                        {
                            self.shared_snapshot.local_mirror.state = LocalMirrorState::Error;
                            self.shared_snapshot.local_mirror.last_error = Some(error.to_string());
                        }
                    }
                }
                SenderCommand::SetQualityPreset(quality) => {
                    self.config.quality = quality;
                    self.connect_request.quality = quality;
                }
                SenderCommand::Shutdown => {
                    self.shared_snapshot.state = StreamSessionState::Stopping;
                    return true;
                }
            }
        }

        false
    }

    fn process_connect_results(&mut self) {
        while let Ok(result) = self.connect_rx.try_recv() {
            match result {
                TargetConnectResult::Connected(session) => {
                    let device_id = session.target.receiver_id.clone();
                    if !self.targets.contains(&device_id) {
                        let _ = session
                            .shutdown_with_stop("sender target was removed before activation");
                        continue;
                    }

                    tracing::info!(
                        receiver_id = %device_id,
                        receiver_name = %session.target.receiver_name,
                        endpoint = %session.target.endpoint.address,
                        "sender target connected"
                    );
                    self.targets.insert_active(session);
                    if let Err(error) = self.ensure_capture_running() {
                        self.shared_snapshot.last_error = Some(error.to_string());
                        self.fail_all_active_targets(error.to_string());
                    }
                }
                TargetConnectResult::Failed(snapshot) => {
                    if !self.targets.contains(&snapshot.receiver_id) {
                        continue;
                    }

                    tracing::warn!(
                        receiver_id = %snapshot.receiver_id,
                        error = %snapshot.last_error.as_deref().unwrap_or("unknown connection error"),
                        "sender target connection failed"
                    );
                    self.shared_snapshot.last_error = snapshot.last_error.clone();
                    self.targets.insert_failed(snapshot);
                }
            }
        }
    }

    fn process_local_mirror_events(&mut self) {
        for event in self.local_mirror_branch.drain_events() {
            match event {
                LocalMirrorEvent::Started {
                    backend_name,
                    target_id,
                } => {
                    self.shared_snapshot.local_mirror.playback_backend = Some(backend_name);
                    self.shared_snapshot.local_mirror.playback_target_id = target_id;
                    self.shared_snapshot.local_mirror.state = LocalMirrorState::Mirroring;
                    self.shared_snapshot.local_mirror.last_error = None;
                }
                LocalMirrorEvent::Played { bytes } => {
                    self.shared_snapshot.local_mirror.packets_played += 1;
                    self.shared_snapshot.local_mirror.bytes_played += bytes as u64;
                }
                LocalMirrorEvent::StateChanged(state) => {
                    self.shared_snapshot.local_mirror.state = state;
                }
                LocalMirrorEvent::Error(message) => {
                    tracing::warn!(error = %message, "local playback mirror reported an error");
                    self.shared_snapshot.local_mirror.last_error = Some(message);
                    self.shared_snapshot.local_mirror.state = LocalMirrorState::Error;
                }
            }
        }
    }

    fn tick_targets(&mut self) {
        let heartbeat_interval =
            Duration::from_millis(self.config.heartbeat_interval_ms.max(250) as u64);
        let mut failed_targets = Vec::new();

        for device_id in self.targets.active_ids() {
            let still_streaming = match self.targets.active_mut(&device_id) {
                Some(session) => session.tick(heartbeat_interval),
                None => continue,
            };
            if !still_streaming {
                failed_targets.push(device_id);
            }
        }

        for device_id in failed_targets {
            if let Some(TargetSessionEntry::Active(session)) = self.targets.remove(&device_id) {
                let snapshot = session.shutdown_immediate();
                self.shared_snapshot.last_error = snapshot.last_error.clone();
                self.targets.insert_failed(snapshot);
            }
        }
    }

    fn capture_and_fan_out(&mut self) {
        let Some(capture) = self.capture.as_mut() else {
            return;
        };

        match capture.try_recv_frame() {
            Ok(Some(frame)) => {
                let captured_at_ms = frame.captured_at.as_millis() as u64;
                let captured_at_unix_ms = self
                    .capture_started_unix_ms
                    .unwrap_or_else(now_unix_ms)
                    .saturating_add(captured_at_ms);
                let base_frame = FanoutAudioFrame {
                    sequence: frame.sequence,
                    captured_at_ms,
                    captured_at_unix_ms,
                    payload: frame.payload,
                };

                if self.shared_snapshot.local_mirror.desired_enabled
                    && matches!(
                        self.shared_snapshot.local_mirror.state,
                        LocalMirrorState::Starting | LocalMirrorState::Mirroring
                    )
                {
                    let _ = handle_buffer_push(
                        self.local_mirror_branch.push_frame(base_frame.clone()),
                        "local playback mirror",
                        &mut self.shared_snapshot.local_mirror.buffer,
                    );
                }

                let mut failed_targets = Vec::new();
                for device_id in self.targets.active_ids() {
                    let push_result = match self.targets.active_mut(&device_id) {
                        Some(session) => session.push_frame(base_frame.clone()),
                        None => continue,
                    };

                    if let Err(error) = push_result {
                        if let Some(session) = self.targets.active_mut(&device_id) {
                            session.set_error(error.to_string(), StreamTargetHealth::Error);
                        }
                        failed_targets.push(device_id);
                    }
                }

                for device_id in failed_targets {
                    if let Some(TargetSessionEntry::Active(session)) =
                        self.targets.remove(&device_id)
                    {
                        let snapshot = session.shutdown_immediate();
                        self.shared_snapshot.last_error = snapshot.last_error.clone();
                        self.targets.insert_failed(snapshot);
                    }
                }
            }
            Ok(None) => {}
            Err(AudioError::CaptureEnded) => {
                self.shared_snapshot.last_error = Some("capture stream ended".to_string());
                self.fail_all_active_targets("capture stream ended".to_string());
            }
            Err(error) => {
                self.shared_snapshot.last_error = Some(error.to_string());
                self.fail_all_active_targets(error.to_string());
            }
        }
    }

    fn ensure_capture_running(&mut self) -> Result<(), TransportError> {
        if self.capture.is_some() {
            return Ok(());
        }

        let capture = self.backend.start_capture(self.capture_settings.clone())?;
        self.capture = Some(capture);
        self.capture_started_unix_ms = Some(now_unix_ms());
        tracing::info!(
            manager_session_id = %self.manager_session_id,
            "multi-device sender capture started"
        );

        if self.shared_snapshot.local_mirror.desired_enabled {
            self.shared_snapshot.local_mirror.state = LocalMirrorState::Starting;
            self.local_mirror_branch
                .start(self.local_mirror_request.clone())?;
        }

        Ok(())
    }

    fn stop_capture_if_idle(&mut self) {
        if self.targets.has_active_targets() || self.targets.has_pending_targets() {
            return;
        }

        if let Some(mut capture) = self.capture.take() {
            let _ = capture.stop();
        }
        self.capture_started_unix_ms = None;

        if matches!(
            self.shared_snapshot.local_mirror.state,
            LocalMirrorState::Starting | LocalMirrorState::Mirroring | LocalMirrorState::Stopping
        ) {
            let _ = self.local_mirror_branch.stop();
        }
    }

    fn fail_all_active_targets(&mut self, message: String) {
        let active_ids = self.targets.active_ids();
        for device_id in active_ids {
            if let Some(session) = self.targets.active_mut(&device_id) {
                session.set_error(message.clone(), StreamTargetHealth::Error);
            }
        }
    }

    fn refresh_snapshot(&mut self) {
        self.shared_snapshot.targets = self.targets.collect_snapshots();
        self.shared_snapshot.metrics = aggregate_metrics(&self.shared_snapshot.targets);
        self.shared_snapshot.state = derive_manager_state(&self.shared_snapshot.targets);
        self.shared_snapshot.local_mirror.buffer = self.local_mirror_branch.snapshot();
        finalize_local_mirror_state(
            &mut self.shared_snapshot.local_mirror,
            self.capture.is_some(),
        );
        sync_snapshot(&self.snapshot, self.shared_snapshot.clone());
    }

    fn shutdown_all(&mut self) {
        if let Some(mut capture) = self.capture.take() {
            let _ = capture.stop();
        }

        let device_ids = self.targets.active_ids();
        for device_id in device_ids {
            if let Some(TargetSessionEntry::Active(session)) = self.targets.remove(&device_id) {
                let _ = session.shutdown_with_stop("sender session manager stopping");
            }
        }

        self.targets.entries.clear();
        let _ = self.local_mirror_branch.shutdown();
        self.shared_snapshot.targets.clear();
        self.shared_snapshot.metrics = StreamMetrics::default();
        self.shared_snapshot.state = StreamSessionState::Idle;
        self.shared_snapshot.last_error = None;
        finalize_local_mirror_state(&mut self.shared_snapshot.local_mirror, false);
        sync_snapshot(&self.snapshot, self.shared_snapshot.clone());
    }
}

impl Clone for TargetConnectRequest {
    fn clone(&self) -> Self {
        Self {
            manager_session_id: self.manager_session_id.clone(),
            sender_name: self.sender_name.clone(),
            stream_config: self.stream_config.clone(),
            connect_timeout: self.connect_timeout,
            quality: self.quality,
            target_latency_ms: self.target_latency_ms,
            heartbeat_interval_ms: self.heartbeat_interval_ms,
        }
    }
}

fn network_writer_loop(
    mut stream: TcpStream,
    control_rx: Receiver<NetworkControl>,
    frame_rx: Receiver<FanoutAudioFrame>,
    event_tx: Sender<NetworkEvent>,
) {
    loop {
        while let Ok(control) = control_rx.try_recv() {
            let result = match control {
                NetworkControl::Heartbeat { nonce } => write_message(
                    &mut stream,
                    FrameKind::Heartbeat,
                    &HeartbeatMessage { nonce },
                    &[],
                )
                .map(|wire_bytes| NetworkEvent::HeartbeatSent {
                    wire_bytes: wire_bytes as u64,
                }),
                NetworkControl::Stop { reason } => {
                    let result =
                        write_message(&mut stream, FrameKind::Stop, &StopMessage { reason }, &[])
                            .map(|wire_bytes| NetworkEvent::StopSent {
                                wire_bytes: wire_bytes as u64,
                            });
                    match result {
                        Ok(event) => {
                            let _ = event_tx.send(event);
                            return;
                        }
                        Err(error) => {
                            let _ = event_tx.send(NetworkEvent::Error(error.to_string()));
                            return;
                        }
                    }
                }
                NetworkControl::Shutdown => return,
            };

            match result {
                Ok(event) => {
                    if event_tx.send(event).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = event_tx.send(NetworkEvent::Error(error.to_string()));
                    return;
                }
            }
        }

        match frame_rx.recv_timeout(BRANCH_IDLE_POLL_INTERVAL) {
            Ok(frame) => {
                let metadata = AudioMessage {
                    sequence: frame.sequence,
                    captured_at_ms: frame.captured_at_ms,
                    captured_at_unix_ms: frame.captured_at_unix_ms,
                };
                match write_message(&mut stream, FrameKind::Audio, &metadata, &frame.payload) {
                    Ok(wire_bytes) => {
                        if event_tx
                            .send(NetworkEvent::AudioSent {
                                wire_bytes: wire_bytes as u64,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = event_tx.send(NetworkEvent::Error(error.to_string()));
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn sender_reader_loop(mut stream: TcpStream, inbound_tx: Sender<InboundControl>) {
    loop {
        match read_frame(&mut stream) {
            Ok(frame) => {
                let wire_bytes = frame.wire_bytes as u64;
                let event = match frame.kind {
                    FrameKind::HeartbeatAck => {
                        let message: HeartbeatMessage = match decode_metadata(&frame) {
                            Ok(message) => message,
                            Err(error) => {
                                let _ = inbound_tx.send(InboundControl::Error {
                                    message: error.to_string(),
                                    wire_bytes,
                                });
                                break;
                            }
                        };
                        InboundControl::HeartbeatAck {
                            nonce: message.nonce,
                            wire_bytes,
                        }
                    }
                    FrameKind::Stop => {
                        let message: StopMessage = match decode_metadata(&frame) {
                            Ok(message) => message,
                            Err(error) => {
                                let _ = inbound_tx.send(InboundControl::Error {
                                    message: error.to_string(),
                                    wire_bytes,
                                });
                                break;
                            }
                        };
                        InboundControl::Stop {
                            reason: message.reason,
                            wire_bytes,
                        }
                    }
                    FrameKind::Error => {
                        let message: ErrorMessage = match decode_metadata(&frame) {
                            Ok(message) => message,
                            Err(error) => {
                                let _ = inbound_tx.send(InboundControl::Error {
                                    message: error.to_string(),
                                    wire_bytes,
                                });
                                break;
                            }
                        };
                        InboundControl::Error {
                            message: format!("{}: {}", message.code, message.message),
                            wire_bytes,
                        }
                    }
                    unexpected => InboundControl::Error {
                        message: format!("unexpected {:?} frame from receiver", unexpected),
                        wire_bytes,
                    },
                };

                if inbound_tx.send(event).is_err() {
                    break;
                }
            }
            Err(TransportError::Io { source, .. })
                if matches!(
                    source.kind(),
                    ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset
                ) =>
            {
                let _ = inbound_tx.send(InboundControl::Disconnected);
                break;
            }
            Err(error) => {
                let _ = inbound_tx.send(InboundControl::Error {
                    message: error.to_string(),
                    wire_bytes: 0,
                });
                break;
            }
        }
    }
}

fn validate_accept(
    accept: &AcceptMessage,
    expected_session_id: &str,
    requested_stream: &ReceiverStreamConfig,
) -> Result<(), TransportError> {
    if accept.protocol_version != synchrosonic_core::STREAM_PROTOCOL_VERSION {
        return Err(TransportError::Negotiation(format!(
            "receiver negotiated unsupported protocol version {}",
            accept.protocol_version
        )));
    }
    if accept.session_id != expected_session_id {
        return Err(TransportError::Negotiation(
            "receiver accepted a different session id".to_string(),
        ));
    }
    if accept.codec != StreamCodec::RawPcm {
        return Err(TransportError::Negotiation(
            "receiver negotiated unsupported codec".to_string(),
        ));
    }
    if &accept.stream != requested_stream {
        return Err(TransportError::Negotiation(
            "receiver changed stream parameters; renegotiation is not yet supported".to_string(),
        ));
    }

    Ok(())
}

fn pending_target_snapshot(target: &SenderTarget) -> StreamTargetSnapshot {
    let mut snapshot = StreamTargetSnapshot::new(
        target.receiver_id.clone(),
        target.receiver_name.clone(),
        target.endpoint.address,
    );
    snapshot.state = StreamSessionState::Connecting;
    snapshot.health = StreamTargetHealth::Pending;
    snapshot
}

fn receiver_stream_config(settings: &CaptureSettings) -> ReceiverStreamConfig {
    ReceiverStreamConfig {
        sample_rate_hz: settings.sample_rate_hz,
        channels: settings.channels,
        sample_format: settings.sample_format,
        frames_per_packet: settings.buffer_frames,
    }
}

fn aggregate_metrics(targets: &[StreamTargetSnapshot]) -> StreamMetrics {
    let mut aggregate = StreamMetrics::default();
    for target in targets {
        aggregate.accumulate(&target.metrics);
    }
    aggregate
}

fn derive_manager_state(targets: &[StreamTargetSnapshot]) -> StreamSessionState {
    if targets.is_empty() {
        return StreamSessionState::Idle;
    }
    if targets
        .iter()
        .any(|target| target.state == StreamSessionState::Streaming)
    {
        return StreamSessionState::Streaming;
    }
    if targets.iter().any(|target| {
        matches!(
            target.state,
            StreamSessionState::Connecting | StreamSessionState::Negotiating
        )
    }) {
        return StreamSessionState::Connecting;
    }
    if targets
        .iter()
        .any(|target| target.state == StreamSessionState::Stopping)
    {
        return StreamSessionState::Stopping;
    }
    if targets
        .iter()
        .all(|target| target.state == StreamSessionState::Error)
    {
        return StreamSessionState::Error;
    }

    StreamSessionState::Idle
}

fn handle_buffer_push(
    outcome: Result<BufferedPushOutcome, TransportError>,
    branch_name: &str,
    snapshot: &mut synchrosonic_core::StreamBranchBufferSnapshot,
) -> Result<(), TransportError> {
    match outcome? {
        BufferedPushOutcome::Enqueued => {}
        BufferedPushOutcome::DroppedOldest => {
            tracing::warn!(
                branch = branch_name,
                dropped_packets = snapshot.dropped_packets + 1,
                "branch queue was full and dropped the oldest buffered packet"
            );
        }
        BufferedPushOutcome::DroppedNewest => {
            tracing::warn!(
                branch = branch_name,
                dropped_packets = snapshot.dropped_packets + 1,
                "branch queue was saturated and dropped the newest packet"
            );
        }
    }
    Ok(())
}

fn sync_snapshot(shared: &Arc<Mutex<StreamSessionSnapshot>>, snapshot: StreamSessionSnapshot) {
    if let Ok(mut shared) = shared.lock() {
        *shared = snapshot;
    }
}

fn bitrate_bps(bytes_sent: u64, elapsed: Duration) -> u64 {
    let millis = elapsed.as_millis() as u64;
    if millis == 0 {
        return 0;
    }

    bytes_sent.saturating_mul(8).saturating_mul(1_000) / millis
}

fn branch_queue_capacity(packet_duration: Duration, target_latency_ms: u16) -> usize {
    let packet_ms = packet_duration.as_millis().max(1) as usize;
    let target_ms = usize::from(target_latency_ms.max(10));
    let packets = div_ceil(target_ms, packet_ms) + BRANCH_QUEUE_HEADROOM_PACKETS;
    packets.clamp(MIN_BRANCH_QUEUE_PACKETS, MAX_BRANCH_QUEUE_PACKETS)
}

fn div_ceil(dividend: usize, divisor: usize) -> usize {
    dividend.saturating_add(divisor.saturating_sub(1)) / divisor.max(1)
}

fn finalize_local_mirror_state(
    snapshot: &mut synchrosonic_core::LocalMirrorSnapshot,
    capture_active: bool,
) {
    if snapshot.state == LocalMirrorState::Error {
        return;
    }

    snapshot.state = if capture_active {
        if snapshot.desired_enabled {
            match snapshot.state {
                LocalMirrorState::Mirroring | LocalMirrorState::Starting => snapshot.state,
                _ => LocalMirrorState::Starting,
            }
        } else {
            LocalMirrorState::Disabled
        }
    } else if snapshot.desired_enabled {
        LocalMirrorState::Idle
    } else {
        LocalMirrorState::Disabled
    };
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
