use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use flume::{Receiver, Sender, TryRecvError};
use synchrosonic_core::{
    config::TransportConfig,
    services::AudioBackend,
    AudioError, CaptureSettings, DeviceId, ReceiverStreamConfig, StreamSessionSnapshot,
    StreamSessionState, TransportEndpoint, TransportError,
};

use crate::protocol::{
    decode_metadata, read_frame, write_message, AcceptMessage, AudioMessage, ErrorMessage,
    FrameKind, HeartbeatMessage, HelloMessage, StopMessage,
};

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
    snapshot: Arc<Mutex<StreamSessionSnapshot>>,
    control_tx: Option<Sender<SenderCommand>>,
    worker: Option<JoinHandle<()>>,
}

impl LanSenderSession {
    pub fn new(config: TransportConfig) -> Self {
        Self {
            config,
            snapshot: Arc::new(Mutex::new(StreamSessionSnapshot::default())),
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
        let (control_tx, control_rx) = flume::unbounded();
        let config = self.config.clone();
        let snapshot = Arc::clone(&self.snapshot);
        let target_snapshot = StreamSessionSnapshot::with_target(
            target.receiver_id.clone(),
            target.receiver_name.clone(),
            target.endpoint.address,
        );
        if let Ok(mut shared) = self.snapshot.lock() {
            *shared = target_snapshot;
            shared.state = StreamSessionState::Connecting;
        }

        self.worker = Some(thread::spawn(move || {
            if let Err(error) = sender_worker_loop(
                backend,
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
                }
            }
        }));
        self.control_tx = Some(control_tx);

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
}

enum InboundControl {
    HeartbeatAck { nonce: u64, wire_bytes: u64 },
    Stop { reason: String, wire_bytes: u64 },
    Error { message: String, wire_bytes: u64 },
    Disconnected,
}

fn sender_worker_loop<B>(
    backend: B,
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

    let mut shared_snapshot = StreamSessionSnapshot::with_target(
        target.receiver_id.clone(),
        target.receiver_name.clone(),
        target.endpoint.address,
    );
    shared_snapshot.state = StreamSessionState::Connecting;
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

    loop {
        match control_rx.try_recv() {
            Ok(SenderCommand::Stop) => {
                shared_snapshot.state = StreamSessionState::Stopping;
                sync_snapshot(&snapshot, shared_snapshot.clone());
                let _ = write_message(
                    &mut stream,
                    FrameKind::Stop,
                    &StopMessage {
                        reason: "sender requested stop".to_string(),
                    },
                    &[],
                )
                .map(|wire_bytes| {
                    shared_snapshot.metrics.bytes_sent += wire_bytes as u64;
                });
                break;
            }
            Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        while let Ok(inbound) = inbound_rx.try_recv() {
            match inbound {
                InboundControl::HeartbeatAck { nonce, wire_bytes } => {
                    shared_snapshot.metrics.bytes_received += wire_bytes;
                    shared_snapshot.metrics.keepalives_received += 1;
                    if let Some(sent_at) = pending_heartbeats.remove(&nonce) {
                        shared_snapshot.metrics.latency_estimate_ms =
                            Some((sent_at.elapsed().as_millis() / 2) as u32);
                    }
                }
                InboundControl::Stop { reason, wire_bytes } => {
                    shared_snapshot.metrics.bytes_received += wire_bytes;
                    shared_snapshot.last_error = Some(reason);
                    shared_snapshot.state = StreamSessionState::Idle;
                    sync_snapshot(&snapshot, shared_snapshot.clone());
                    let _ = stream.shutdown(Shutdown::Both);
                    let _ = capture.stop();
                    let _ = reader.join();
                    return Ok(());
                }
                InboundControl::Error { message, wire_bytes } => {
                    shared_snapshot.metrics.bytes_received += wire_bytes;
                    shared_snapshot.last_error = Some(message);
                    shared_snapshot.state = StreamSessionState::Error;
                    sync_snapshot(&snapshot, shared_snapshot.clone());
                    let _ = stream.shutdown(Shutdown::Both);
                    let _ = capture.stop();
                    let _ = reader.join();
                    return Ok(());
                }
                InboundControl::Disconnected => {
                    shared_snapshot.last_error =
                        Some("receiver disconnected unexpectedly".to_string());
                    shared_snapshot.state = StreamSessionState::Error;
                    sync_snapshot(&snapshot, shared_snapshot.clone());
                    let _ = capture.stop();
                    let _ = reader.join();
                    return Ok(());
                }
            }
        }

        match capture.try_recv_frame() {
            Ok(Some(frame)) => {
                if let Some(previous) = last_sequence {
                    if frame.sequence > previous + 1 {
                        shared_snapshot.metrics.packet_gaps += frame.sequence - previous - 1;
                    }
                }
                last_sequence = Some(frame.sequence);

                let metadata = AudioMessage {
                    sequence: frame.sequence,
                    captured_at_ms: frame.captured_at.as_millis() as u64,
                };
                let wire_bytes =
                    write_message(&mut stream, FrameKind::Audio, &metadata, &frame.payload)?;
                shared_snapshot.metrics.bytes_sent += wire_bytes as u64;
                shared_snapshot.metrics.packets_sent += 1;
                shared_snapshot.metrics.estimated_bitrate_bps =
                    bitrate_bps(shared_snapshot.metrics.bytes_sent, started_at.elapsed());
            }
            Ok(None) => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(AudioError::CaptureEnded) => {
                shared_snapshot.last_error = Some("capture stream ended".to_string());
                break;
            }
            Err(error) => return Err(error.into()),
        }

        if last_heartbeat.elapsed() >= heartbeat_interval {
            heartbeat_nonce += 1;
            pending_heartbeats.insert(heartbeat_nonce, Instant::now());
            let wire_bytes = write_message(
                &mut stream,
                FrameKind::Heartbeat,
                &HeartbeatMessage {
                    nonce: heartbeat_nonce,
                },
                &[],
            )?;
            shared_snapshot.metrics.bytes_sent += wire_bytes as u64;
            shared_snapshot.metrics.keepalives_sent += 1;
            last_heartbeat = Instant::now();
        }

        sync_snapshot(&snapshot, shared_snapshot.clone());
    }

    let _ = capture.stop();
    let _ = stream.shutdown(Shutdown::Both);
    let _ = reader.join();
    shared_snapshot.state = StreamSessionState::Idle;
    sync_snapshot(&snapshot, shared_snapshot);

    Ok(())
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

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
