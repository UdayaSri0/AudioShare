use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use flume::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use synchrosonic_audio::{LinuxPlaybackEngine, PlaybackEngine, PlaybackStartRequest};
use synchrosonic_core::{
    config::TransportConfig,
    services::AudioBackend,
    AudioError, CaptureSettings, DeviceId, LocalMirrorState, ReceiverStreamConfig,
    StreamSessionSnapshot, StreamSessionState, TransportEndpoint, TransportError,
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
        if self.control_tx.is_some() {
            return Err(TransportError::AlreadyRunning);
        }

        let sender_name = sender_name.into();
        let desired_local_mirror = capture_settings.outputs.local_monitoring;
        let (control_tx, control_rx) = flume::unbounded();
        let config = self.config.clone();
        let snapshot = Arc::clone(&self.snapshot);
        let playback_engine = Arc::clone(&self.playback_engine);
        let mut target_snapshot = StreamSessionSnapshot::with_target(
            target.receiver_id.clone(),
            target.receiver_name.clone(),
            target.endpoint.address,
        );
        target_snapshot.state = StreamSessionState::Connecting;
        target_snapshot.local_mirror.desired_enabled = desired_local_mirror;
        target_snapshot.local_mirror.state = if desired_local_mirror {
            LocalMirrorState::Starting
        } else {
            LocalMirrorState::Disabled
        };
        target_snapshot.local_mirror.playback_backend =
            Some(self.playback_engine.backend_name().to_string());
        if let Ok(mut shared) = self.snapshot.lock() {
            *shared = target_snapshot;
        }

        self.worker = Some(thread::spawn(move || {
            if let Err(error) = sender_worker_loop(
                backend,
                playback_engine,
                config,
                capture_settings,
                target,
                sender_name,
                Arc::clone(&snapshot),
                control_rx,
            ) {
                tracing::error!(error = %error, "sender session ended with error");
                if let Ok(mut shared) = snapshot.lock() {
                    shared.state = StreamSessionState::Error;
                    shared.last_error = Some(error.to_string());
                    finalize_local_mirror_state(&mut shared);
                }
            }
        }));
        self.control_tx = Some(control_tx);

        Ok(())
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

    pub fn stop(&mut self) -> Result<(), TransportError> {
        if let Some(control_tx) = self.control_tx.take() {
            let _ = control_tx.send(SenderCommand::Stop);
        }

        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| TransportError::ThreadJoin)?;
        }

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.state = StreamSessionState::Idle;
            finalize_local_mirror_state(&mut snapshot);
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
    Stop,
    SetLocalMirrorEnabled(bool),
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
        let worker = thread::spawn(move || network_writer_loop(stream, control_rx, frame_rx, event_tx));

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

fn sender_worker_loop<B>(
    backend: B,
    playback_engine: Arc<dyn PlaybackEngine>,
    config: TransportConfig,
    capture_settings: CaptureSettings,
    target: SenderTarget,
    sender_name: String,
    snapshot: Arc<Mutex<StreamSessionSnapshot>>,
    control_rx: Receiver<SenderCommand>,
) -> Result<(), TransportError>
where
    B: AudioBackend + Send + Sync + 'static,
{
    let connect_timeout = Duration::from_millis(config.connect_timeout_ms.max(250) as u64);
    let heartbeat_interval = Duration::from_millis(config.heartbeat_interval_ms.max(250) as u64);
    let stream_config = ReceiverStreamConfig {
        sample_rate_hz: capture_settings.sample_rate_hz,
        channels: capture_settings.channels,
        sample_format: capture_settings.sample_format,
        frames_per_packet: capture_settings.buffer_frames,
    };
    let session_id = format!("stream-{}", now_unix_ms());
    let local_mirror_request = PlaybackStartRequest {
        stream: stream_config.clone(),
        target_id: None,
        latency_ms: capture_settings.target_latency_ms,
    };

    let mut shared_snapshot = StreamSessionSnapshot::with_target(
        target.receiver_id.clone(),
        target.receiver_name.clone(),
        target.endpoint.address,
    );
    shared_snapshot.state = StreamSessionState::Connecting;
    shared_snapshot.local_mirror.desired_enabled = capture_settings.outputs.local_monitoring;
    shared_snapshot.local_mirror.state = if capture_settings.outputs.local_monitoring {
        LocalMirrorState::Starting
    } else {
        LocalMirrorState::Disabled
    };
    shared_snapshot.local_mirror.playback_backend = Some(playback_engine.backend_name().to_string());
    shared_snapshot.local_mirror.playback_target_id = local_mirror_request.target_id.clone();
    sync_snapshot(&snapshot, shared_snapshot.clone());

    let mut stream = TcpStream::connect_timeout(&target.endpoint.address, connect_timeout)
        .map_err(|source| TransportError::Connect {
            address: target.endpoint.address.to_string(),
            source,
        })?;
    stream
        .set_nodelay(true)
        .map_err(|source| TransportError::Io {
            context: format!("enabling TCP_NODELAY for {}", target.endpoint.address),
            source,
        })?;

    shared_snapshot.state = StreamSessionState::Negotiating;
    sync_snapshot(&snapshot, shared_snapshot.clone());

    let hello = HelloMessage::new(
        session_id.clone(),
        sender_name,
        stream_config.clone(),
        config.quality,
        config.target_latency_ms,
        config.heartbeat_interval_ms,
    );
    shared_snapshot.metrics.bytes_sent +=
        write_message(&mut stream, FrameKind::Hello, &hello, &[])? as u64;

    let accept_frame = read_frame(&mut stream)?;
    shared_snapshot.metrics.bytes_received += accept_frame.wire_bytes as u64;
    shared_snapshot.metrics.packets_received += 1;
    if accept_frame.kind != FrameKind::Accept {
        return Err(TransportError::Negotiation(
            "receiver did not respond with Accept".to_string(),
        ));
    }

    let accept: AcceptMessage = decode_metadata(&accept_frame)?;
    validate_accept(&accept, &session_id, &stream_config)?;
    shared_snapshot.session_id = Some(session_id.clone());
    shared_snapshot.codec = Some(accept.codec);
    shared_snapshot.stream = Some(accept.stream.clone());
    shared_snapshot.state = StreamSessionState::Streaming;
    shared_snapshot.last_error = None;

    let network_queue_packets =
        branch_queue_capacity(accept.stream.packet_duration(), config.target_latency_ms);
    let local_mirror_queue_packets = branch_queue_capacity(
        accept.stream.packet_duration(),
        capture_settings.target_latency_ms,
    );
    let mut network_branch = NetworkBranch::new(
        stream.try_clone().map_err(|source| TransportError::Io {
            context: format!("cloning TCP stream for {}", target.endpoint.address),
            source,
        })?,
        network_queue_packets,
    );
    let mut local_mirror_branch =
        LocalMirrorBranch::new(Arc::clone(&playback_engine), local_mirror_queue_packets);
    shared_snapshot.network_buffer = network_branch.snapshot();
    shared_snapshot.local_mirror.buffer = local_mirror_branch.snapshot();
    if shared_snapshot.local_mirror.desired_enabled {
        shared_snapshot.local_mirror.state = LocalMirrorState::Starting;
        local_mirror_branch.start(local_mirror_request.clone())?;
    }
    sync_snapshot(&snapshot, shared_snapshot.clone());

    let read_stream = stream.try_clone().map_err(|source| TransportError::Io {
        context: format!("cloning TCP stream for {}", target.endpoint.address),
        source,
    })?;
    let (inbound_tx, inbound_rx) = flume::unbounded();
    let reader = thread::spawn(move || sender_reader_loop(read_stream, inbound_tx));

    let mut capture = backend.start_capture(capture_settings)?;
    let started_at = Instant::now();
    let mut last_sequence = None::<u64>;
    let mut heartbeat_nonce = 0_u64;
    let mut pending_heartbeats = HashMap::<u64, Instant>::new();
    let mut last_heartbeat = Instant::now();
    let mut stopping = false;

    loop {
        let mut should_break = false;

        match control_rx.try_recv() {
            Ok(SenderCommand::Stop) => {
                shared_snapshot.state = StreamSessionState::Stopping;
                sync_snapshot(&snapshot, shared_snapshot.clone());
                let _ = network_branch.send_stop("sender requested stop");
                stopping = true;
                should_break = true;
            }
            Ok(SenderCommand::SetLocalMirrorEnabled(enabled)) => {
                shared_snapshot.local_mirror.desired_enabled = enabled;
                shared_snapshot.local_mirror.last_error = None;
                if enabled {
                    shared_snapshot.local_mirror.state = LocalMirrorState::Starting;
                    local_mirror_branch.start(local_mirror_request.clone())?;
                    tracing::info!("local playback mirror enabled for active sender session");
                } else {
                    shared_snapshot.local_mirror.state = LocalMirrorState::Stopping;
                    local_mirror_branch.stop()?;
                    tracing::info!("local playback mirror disabled for active sender session");
                }
            }
            Err(TryRecvError::Disconnected) => {
                stopping = true;
                should_break = true;
            }
            Err(TryRecvError::Empty) => {}
        }

        while let Ok(inbound) = inbound_rx.try_recv() {
            match inbound {
                InboundControl::HeartbeatAck { nonce, wire_bytes } => {
                    shared_snapshot.metrics.bytes_received += wire_bytes;
                    shared_snapshot.metrics.packets_received += 1;
                    shared_snapshot.metrics.keepalives_received += 1;
                    if let Some(sent_at) = pending_heartbeats.remove(&nonce) {
                        shared_snapshot.metrics.latency_estimate_ms =
                            Some((sent_at.elapsed().as_millis() / 2) as u32);
                    }
                }
                InboundControl::Stop { reason, wire_bytes } => {
                    shared_snapshot.metrics.bytes_received += wire_bytes;
                    shared_snapshot.metrics.packets_received += 1;
                    shared_snapshot.last_error = Some(reason);
                    shared_snapshot.state = StreamSessionState::Idle;
                    should_break = true;
                    stopping = true;
                }
                InboundControl::Error { message, wire_bytes } => {
                    shared_snapshot.metrics.bytes_received += wire_bytes;
                    shared_snapshot.metrics.packets_received += 1;
                    shared_snapshot.last_error = Some(message);
                    shared_snapshot.state = StreamSessionState::Error;
                    should_break = true;
                }
                InboundControl::Disconnected => {
                    shared_snapshot.last_error =
                        Some("receiver disconnected unexpectedly".to_string());
                    shared_snapshot.state = StreamSessionState::Error;
                    should_break = true;
                }
            }
        }

        for event in network_branch.drain_events() {
            match event {
                NetworkEvent::AudioSent { wire_bytes } => {
                    shared_snapshot.metrics.bytes_sent += wire_bytes;
                    shared_snapshot.metrics.packets_sent += 1;
                    shared_snapshot.metrics.estimated_bitrate_bps =
                        bitrate_bps(shared_snapshot.metrics.bytes_sent, started_at.elapsed());
                }
                NetworkEvent::HeartbeatSent { wire_bytes } => {
                    shared_snapshot.metrics.bytes_sent += wire_bytes;
                    shared_snapshot.metrics.keepalives_sent += 1;
                }
                NetworkEvent::StopSent { wire_bytes } => {
                    shared_snapshot.metrics.bytes_sent += wire_bytes;
                }
                NetworkEvent::Error(message) => {
                    shared_snapshot.last_error = Some(message);
                    shared_snapshot.state = StreamSessionState::Error;
                    should_break = true;
                }
            }
        }

        for event in local_mirror_branch.drain_events() {
            match event {
                LocalMirrorEvent::Started {
                    backend_name,
                    target_id,
                } => {
                    shared_snapshot.local_mirror.playback_backend = Some(backend_name);
                    shared_snapshot.local_mirror.playback_target_id = target_id;
                    shared_snapshot.local_mirror.state = LocalMirrorState::Mirroring;
                    shared_snapshot.local_mirror.last_error = None;
                }
                LocalMirrorEvent::Played { bytes } => {
                    shared_snapshot.local_mirror.packets_played += 1;
                    shared_snapshot.local_mirror.bytes_played += bytes as u64;
                }
                LocalMirrorEvent::StateChanged(state) => {
                    shared_snapshot.local_mirror.state = state;
                }
                LocalMirrorEvent::Error(message) => {
                    shared_snapshot.local_mirror.last_error = Some(message.clone());
                    shared_snapshot.local_mirror.state = LocalMirrorState::Error;
                    tracing::warn!(error = %message, "local playback mirror reported an error");
                }
            }
        }

        if should_break {
            break;
        }

        match capture.try_recv_frame() {
            Ok(Some(frame)) => {
                if let Some(previous) = last_sequence {
                    if frame.sequence > previous + 1 {
                        shared_snapshot.metrics.packet_gaps += frame.sequence - previous - 1;
                    }
                }
                last_sequence = Some(frame.sequence);

                let branch_frame = FanoutAudioFrame {
                    sequence: frame.sequence,
                    captured_at_ms: frame.captured_at.as_millis() as u64,
                    payload: frame.payload,
                };
                let local_mirror_frame = branch_frame.clone();

                handle_buffer_push(
                    network_branch.push_frame(branch_frame),
                    "network sender",
                    &mut shared_snapshot.network_buffer,
                )?;

                if shared_snapshot.local_mirror.desired_enabled
                    && matches!(
                        shared_snapshot.local_mirror.state,
                        LocalMirrorState::Starting | LocalMirrorState::Mirroring
                    )
                {
                    handle_buffer_push(
                        local_mirror_branch.push_frame(local_mirror_frame),
                        "local playback mirror",
                        &mut shared_snapshot.local_mirror.buffer,
                    )?;
                }
            }
            Ok(None) => {}
            Err(AudioError::CaptureEnded) => {
                shared_snapshot.last_error = Some("capture stream ended".to_string());
                shared_snapshot.state = StreamSessionState::Error;
                break;
            }
            Err(error) => return Err(error.into()),
        }

        if last_heartbeat.elapsed() >= heartbeat_interval {
            heartbeat_nonce += 1;
            pending_heartbeats.insert(heartbeat_nonce, Instant::now());
            network_branch.send_heartbeat(heartbeat_nonce)?;
            last_heartbeat = Instant::now();
        }

        shared_snapshot.network_buffer = network_branch.snapshot();
        shared_snapshot.local_mirror.buffer = local_mirror_branch.snapshot();
        sync_snapshot(&snapshot, shared_snapshot.clone());

        if !stopping {
            thread::sleep(BRANCH_IDLE_POLL_INTERVAL);
        }
    }

    let _ = capture.stop();
    let _ = network_branch.shutdown();
    let _ = local_mirror_branch.shutdown();
    let _ = stream.shutdown(Shutdown::Both);
    let _ = reader.join();

    if shared_snapshot.state != StreamSessionState::Error {
        shared_snapshot.state = StreamSessionState::Idle;
    }
    shared_snapshot.network_buffer = network_branch.snapshot();
    shared_snapshot.local_mirror.buffer = local_mirror_branch.snapshot();
    finalize_local_mirror_state(&mut shared_snapshot);
    sync_snapshot(&snapshot, shared_snapshot);

    Ok(())
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
                    let result = write_message(
                        &mut stream,
                        FrameKind::Stop,
                        &StopMessage { reason },
                        &[],
                    )
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
                if matches!(source.kind(), ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset) =>
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
    if accept.codec != synchrosonic_core::StreamCodec::RawPcm {
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

fn finalize_local_mirror_state(snapshot: &mut StreamSessionSnapshot) {
    if snapshot.local_mirror.state == LocalMirrorState::Error {
        return;
    }

    snapshot.local_mirror.state = if snapshot.local_mirror.desired_enabled {
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
