use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use flume::{Receiver, TryRecvError};
use synchrosonic_core::{
    receiver::{ReceiverConnectionInfo, ReceiverLatencyPreset, ReceiverTransportEvent},
    ReceiverError, TransportError,
};

use crate::protocol::{
    decode_metadata, read_frame, write_message, AcceptMessage, AudioMessage, ErrorMessage,
    FrameKind, HeartbeatMessage, HelloMessage, StopMessage,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverServerSnapshot {
    pub active: bool,
    pub bind_addr: SocketAddr,
    pub session_id: Option<String>,
    pub connected_peer: Option<SocketAddr>,
    pub last_error: Option<String>,
}

impl ReceiverServerSnapshot {
    fn new(bind_addr: SocketAddr) -> Self {
        Self {
            active: false,
            bind_addr,
            session_id: None,
            connected_peer: None,
            last_error: None,
        }
    }
}

pub struct LanReceiverTransportServer {
    bind_addr: SocketAddr,
    receiver_name: String,
    latency_preset: ReceiverLatencyPreset,
    heartbeat_interval_ms: u16,
    snapshot: Arc<Mutex<ReceiverServerSnapshot>>,
    active_stream: Arc<Mutex<Option<TcpStream>>>,
    stop_tx: Option<flume::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl LanReceiverTransportServer {
    pub fn new(
        bind_addr: SocketAddr,
        receiver_name: impl Into<String>,
        latency_preset: ReceiverLatencyPreset,
        heartbeat_interval_ms: u16,
    ) -> Self {
        Self {
            bind_addr,
            receiver_name: receiver_name.into(),
            latency_preset,
            heartbeat_interval_ms,
            snapshot: Arc::new(Mutex::new(ReceiverServerSnapshot::new(bind_addr))),
            active_stream: Arc::new(Mutex::new(None)),
            stop_tx: None,
            worker: None,
        }
    }

    pub fn snapshot(&self) -> ReceiverServerSnapshot {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_else(|_| ReceiverServerSnapshot::new(self.bind_addr))
    }

    pub fn start<F>(&mut self, on_event: F) -> Result<(), TransportError>
    where
        F: Fn(ReceiverTransportEvent) -> Result<(), ReceiverError> + Send + Sync + 'static,
    {
        if self.stop_tx.is_some() {
            return Ok(());
        }

        let listener =
            TcpListener::bind(self.bind_addr).map_err(|source| TransportError::Bind {
                address: self.bind_addr.to_string(),
                source,
            })?;
        let bound_addr = listener.local_addr().map_err(|source| TransportError::Io {
            context: "reading receiver listener local address".to_string(),
            source,
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|source| TransportError::Io {
                context: format!("setting listener {} nonblocking", bound_addr),
                source,
            })?;
        self.bind_addr = bound_addr;

        let (stop_tx, stop_rx) = flume::unbounded();
        let snapshot = Arc::clone(&self.snapshot);
        let active_stream = Arc::clone(&self.active_stream);
        let receiver_name = self.receiver_name.clone();
        let latency_preset = self.latency_preset;
        let heartbeat_interval_ms = self.heartbeat_interval_ms;
        let on_event = Arc::new(on_event);

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.active = true;
            snapshot.bind_addr = bound_addr;
            snapshot.last_error = None;
        }

        self.worker = Some(thread::spawn(move || {
            receiver_listener_loop(
                listener,
                receiver_name,
                latency_preset,
                heartbeat_interval_ms,
                snapshot,
                active_stream,
                stop_rx,
                on_event,
            );
        }));
        self.stop_tx = Some(stop_tx);

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), TransportError> {
        if let Some(stop_tx) = self.stop_tx.take() {
            stop_tx
                .send(())
                .map_err(|_| TransportError::ChannelClosed)?;
        }

        if let Ok(mut stream_slot) = self.active_stream.lock() {
            if let Some(stream) = stream_slot.take() {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }

        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| TransportError::ThreadJoin)?;
        }

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.active = false;
            snapshot.session_id = None;
            snapshot.connected_peer = None;
        }

        Ok(())
    }
}

impl Drop for LanReceiverTransportServer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn receiver_listener_loop(
    listener: TcpListener,
    receiver_name: String,
    latency_preset: ReceiverLatencyPreset,
    heartbeat_interval_ms: u16,
    snapshot: Arc<Mutex<ReceiverServerSnapshot>>,
    active_stream: Arc<Mutex<Option<TcpStream>>>,
    stop_rx: Receiver<()>,
    on_event: Arc<dyn Fn(ReceiverTransportEvent) -> Result<(), ReceiverError> + Send + Sync>,
) {
    loop {
        match stop_rx.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        match listener.accept() {
            Ok((stream, peer_addr)) => {
                if let Ok(clone) = stream.try_clone() {
                    if let Ok(mut slot) = active_stream.lock() {
                        *slot = Some(clone);
                    }
                }

                let result = handle_receiver_session(
                    stream,
                    peer_addr,
                    &receiver_name,
                    latency_preset,
                    heartbeat_interval_ms,
                    Arc::clone(&on_event),
                    Arc::clone(&snapshot),
                    &stop_rx,
                );

                if let Ok(mut slot) = active_stream.lock() {
                    *slot = None;
                }

                if let Err(error) = result {
                    tracing::warn!(error = %error, "receiver transport session ended with error");
                    if let Ok(mut snapshot) = snapshot.lock() {
                        snapshot.last_error = Some(error.to_string());
                        snapshot.session_id = None;
                        snapshot.connected_peer = None;
                    }
                }
            }
            Err(source) if source.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(source) => {
                let error = TransportError::Io {
                    context: format!(
                        "accepting connection on {}",
                        listener
                            .local_addr()
                            .ok()
                            .map(|addr| addr.to_string())
                            .unwrap_or_else(|| "receiver-listener".to_string())
                    ),
                    source,
                };
                tracing::error!(error = %error, "receiver listener failed");
                if let Ok(mut snapshot) = snapshot.lock() {
                    snapshot.last_error = Some(error.to_string());
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.active = false;
        snapshot.session_id = None;
        snapshot.connected_peer = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_receiver_session(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    receiver_name: &str,
    latency_preset: ReceiverLatencyPreset,
    heartbeat_interval_ms: u16,
    on_event: Arc<dyn Fn(ReceiverTransportEvent) -> Result<(), ReceiverError> + Send + Sync>,
    snapshot: Arc<Mutex<ReceiverServerSnapshot>>,
    stop_rx: &Receiver<()>,
) -> Result<(), TransportError> {
    stream
        .set_nodelay(true)
        .map_err(|source| TransportError::Io {
            context: format!("enabling TCP_NODELAY for sender {peer_addr}"),
            source,
        })?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|source| TransportError::Io {
            context: format!("setting read timeout for sender {peer_addr}"),
            source,
        })?;

    let frame = read_frame(&mut stream)?;
    if frame.kind != FrameKind::Hello {
        send_protocol_error(&mut stream, "unexpected-frame", "first frame must be Hello")?;
        return Err(TransportError::InvalidProtocol(
            "receiver expected Hello as the first frame".to_string(),
        ));
    }

    let hello: HelloMessage = decode_metadata(&frame)?;
    validate_hello(&hello)?;
    let keepalive_interval_ms = heartbeat_interval_ms.min(hello.keepalive_interval_ms.max(250));
    let accept = AcceptMessage {
        protocol_version: hello.protocol_version,
        session_id: hello.session_id.clone(),
        receiver_name: receiver_name.to_string(),
        codec: hello.desired_codec,
        stream: hello.stream.clone(),
        keepalive_interval_ms,
        receiver_latency_ms: latency_preset.profile().expected_output_latency_ms(),
    };
    write_message(&mut stream, FrameKind::Accept, &accept, &[])?;

    (on_event)(ReceiverTransportEvent::Connected(ReceiverConnectionInfo {
        session_id: hello.session_id.clone(),
        remote_addr: Some(peer_addr),
        stream: hello.stream.clone(),
        requested_latency_ms: hello.target_latency_ms,
    }))
    .map_err(|error| TransportError::ReceiverCallback(error.to_string()))?;

    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.session_id = Some(hello.session_id.clone());
        snapshot.connected_peer = Some(peer_addr);
        snapshot.last_error = None;
    }

    let mut last_sequence = None::<u64>;
    loop {
        match stop_rx.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => {
                let _ = write_message(
                    &mut stream,
                    FrameKind::Stop,
                    &StopMessage {
                        reason: "receiver transport stopped".to_string(),
                    },
                    &[],
                );
                break;
            }
            Err(TryRecvError::Empty) => {}
        }

        match read_frame(&mut stream) {
            Ok(frame) => match frame.kind {
                FrameKind::Audio => {
                    let metadata: AudioMessage = decode_metadata(&frame)?;
                    if let Some(previous) = last_sequence {
                        if metadata.sequence > previous + 1 {
                            tracing::warn!(
                                previous_sequence = previous,
                                current_sequence = metadata.sequence,
                                missing_packets = metadata.sequence - previous - 1,
                                "receiver observed transport sequence gap"
                            );
                        }
                    }
                    last_sequence = Some(metadata.sequence);

                    (on_event)(ReceiverTransportEvent::AudioPacket(
                        synchrosonic_core::ReceiverAudioPacket {
                            sequence: metadata.sequence,
                            captured_at_ms: metadata.captured_at_ms,
                            captured_at_unix_ms: metadata.captured_at_unix_ms,
                            payload: frame.payload,
                        },
                    ))
                    .map_err(|error| TransportError::ReceiverCallback(error.to_string()))?;
                }
                FrameKind::Heartbeat => {
                    let heartbeat: HeartbeatMessage = decode_metadata(&frame)?;
                    (on_event)(ReceiverTransportEvent::KeepAlive)
                        .map_err(|error| TransportError::ReceiverCallback(error.to_string()))?;
                    write_message(&mut stream, FrameKind::HeartbeatAck, &heartbeat, &[])?;
                }
                FrameKind::Stop => {
                    let stop: StopMessage = decode_metadata(&frame)?;
                    (on_event)(ReceiverTransportEvent::Disconnected {
                        reason: stop.reason,
                        reconnect_suggested: false,
                    })
                    .map_err(|error| TransportError::ReceiverCallback(error.to_string()))?;
                    break;
                }
                FrameKind::Error => {
                    let error: ErrorMessage = decode_metadata(&frame)?;
                    (on_event)(ReceiverTransportEvent::Error {
                        message: format!("sender reported {}: {}", error.code, error.message),
                    })
                    .map_err(|error| TransportError::ReceiverCallback(error.to_string()))?;
                    break;
                }
                unexpected => {
                    send_protocol_error(
                        &mut stream,
                        "unexpected-frame",
                        &format!(
                            "receiver does not accept {:?} during active streaming",
                            unexpected
                        ),
                    )?;
                    return Err(TransportError::InvalidProtocol(format!(
                        "unexpected {:?} frame during active streaming",
                        unexpected
                    )));
                }
            },
            Err(TransportError::Io { source, .. })
                if matches!(
                    source.kind(),
                    ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset
                ) =>
            {
                (on_event)(ReceiverTransportEvent::Disconnected {
                    reason: "sender disconnected unexpectedly".to_string(),
                    reconnect_suggested: true,
                })
                .map_err(|error| TransportError::ReceiverCallback(error.to_string()))?;
                break;
            }
            Err(TransportError::Io { source, .. })
                if matches!(source.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                continue;
            }
            Err(error) => {
                let _ = (on_event)(ReceiverTransportEvent::Error {
                    message: error.to_string(),
                });
                let _ = send_protocol_error(&mut stream, "read-failure", &error.to_string());
                return Err(error);
            }
        }
    }

    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.session_id = None;
        snapshot.connected_peer = None;
    }

    Ok(())
}

fn validate_hello(hello: &HelloMessage) -> Result<(), TransportError> {
    if hello.protocol_version != synchrosonic_core::STREAM_PROTOCOL_VERSION {
        return Err(TransportError::Negotiation(format!(
            "protocol version {} is unsupported",
            hello.protocol_version
        )));
    }
    if !hello.supported_codecs.contains(&hello.desired_codec) {
        return Err(TransportError::Negotiation(
            "sender requested a codec it did not advertise".to_string(),
        ));
    }
    if hello.desired_codec != synchrosonic_core::StreamCodec::RawPcm {
        return Err(TransportError::Negotiation(
            "receiver currently only supports raw PCM".to_string(),
        ));
    }

    Ok(())
}

fn send_protocol_error(
    stream: &mut TcpStream,
    code: &str,
    message: &str,
) -> Result<(), TransportError> {
    write_message(
        stream,
        FrameKind::Error,
        &ErrorMessage {
            code: code.to_string(),
            message: message.to_string(),
        },
        &[],
    )?;
    Ok(())
}
