use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use flume::{Receiver, RecvTimeoutError, Sender, TryRecvError, TrySendError};
use synchrosonic_audio::{PlaybackEngine, PlaybackSink, PlaybackStartRequest};
use synchrosonic_core::{LocalMirrorState, StreamBranchBufferSnapshot, TransportError};

const LOCAL_MIRROR_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanoutAudioFrame {
    pub sequence: u64,
    pub captured_at_ms: u64,
    pub captured_at_unix_ms: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferedPushOutcome {
    Enqueued,
    DroppedOldest,
    DroppedNewest,
}

pub struct BufferedBranchQueue<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
    capacity: usize,
    dropped_packets: Arc<AtomicU64>,
}

impl<T> BufferedBranchQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let (sender, receiver) = flume::bounded(capacity);

        Self {
            sender,
            receiver,
            capacity,
            dropped_packets: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn receiver(&self) -> Receiver<T> {
        self.receiver.clone()
    }

    pub fn snapshot(&self) -> StreamBranchBufferSnapshot {
        StreamBranchBufferSnapshot {
            queued_packets: self.receiver.len().min(self.capacity) as u32,
            max_packets: self.capacity as u32,
            dropped_packets: self.dropped_packets.load(Ordering::Relaxed),
        }
    }

    pub fn clear(&self) {
        while self.receiver.try_recv().is_ok() {}
    }

    pub fn push(&self, item: T) -> Result<BufferedPushOutcome, TransportError> {
        match self.sender.try_send(item) {
            Ok(()) => Ok(BufferedPushOutcome::Enqueued),
            Err(TrySendError::Disconnected(_)) => Err(TransportError::ChannelClosed),
            Err(TrySendError::Full(item)) => {
                if self.receiver.try_recv().is_ok() {
                    self.dropped_packets.fetch_add(1, Ordering::Relaxed);
                    match self.sender.try_send(item) {
                        Ok(()) => Ok(BufferedPushOutcome::DroppedOldest),
                        Err(TrySendError::Disconnected(_)) => Err(TransportError::ChannelClosed),
                        Err(TrySendError::Full(_)) => {
                            self.dropped_packets.fetch_add(1, Ordering::Relaxed);
                            Ok(BufferedPushOutcome::DroppedNewest)
                        }
                    }
                } else {
                    self.dropped_packets.fetch_add(1, Ordering::Relaxed);
                    Ok(BufferedPushOutcome::DroppedNewest)
                }
            }
        }
    }
}

enum LocalMirrorControl {
    Start(PlaybackStartRequest),
    Stop,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalMirrorEvent {
    Started {
        backend_name: String,
        target_id: Option<String>,
    },
    Played {
        bytes: usize,
    },
    StateChanged(LocalMirrorState),
    Error(String),
}

pub struct LocalMirrorBranch {
    queue: BufferedBranchQueue<FanoutAudioFrame>,
    control_tx: Sender<LocalMirrorControl>,
    event_rx: Receiver<LocalMirrorEvent>,
    worker: Option<JoinHandle<()>>,
}

impl LocalMirrorBranch {
    pub fn new(playback_engine: Arc<dyn PlaybackEngine>, queue_capacity: usize) -> Self {
        let queue = BufferedBranchQueue::new(queue_capacity);
        let frame_rx = queue.receiver();
        let (control_tx, control_rx) = flume::unbounded();
        let (event_tx, event_rx) = flume::unbounded();
        let worker = thread::spawn(move || {
            local_mirror_worker_loop(playback_engine, control_rx, frame_rx, event_tx);
        });

        Self {
            queue,
            control_tx,
            event_rx,
            worker: Some(worker),
        }
    }

    pub fn push_frame(
        &self,
        frame: FanoutAudioFrame,
    ) -> Result<BufferedPushOutcome, TransportError> {
        self.queue.push(frame)
    }

    pub fn snapshot(&self) -> StreamBranchBufferSnapshot {
        self.queue.snapshot()
    }

    pub fn start(&self, request: PlaybackStartRequest) -> Result<(), TransportError> {
        self.control_tx
            .send(LocalMirrorControl::Start(request))
            .map_err(|_| TransportError::ChannelClosed)
    }

    pub fn stop(&self) -> Result<(), TransportError> {
        self.control_tx
            .send(LocalMirrorControl::Stop)
            .map_err(|_| TransportError::ChannelClosed)
    }

    pub fn drain_events(&self) -> Vec<LocalMirrorEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    pub fn shutdown(&mut self) -> Result<(), TransportError> {
        let _ = self.control_tx.send(LocalMirrorControl::Shutdown);
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| TransportError::ThreadJoin)?;
        }
        self.queue.clear();
        Ok(())
    }
}

impl Drop for LocalMirrorBranch {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn local_mirror_worker_loop(
    playback_engine: Arc<dyn PlaybackEngine>,
    control_rx: Receiver<LocalMirrorControl>,
    frame_rx: Receiver<FanoutAudioFrame>,
    event_tx: Sender<LocalMirrorEvent>,
) {
    let backend_name = playback_engine.backend_name().to_string();
    let mut enabled = false;
    let mut sink = None::<Box<dyn PlaybackSink>>;

    loop {
        while let Ok(control) = control_rx.try_recv() {
            match control {
                LocalMirrorControl::Start(request) => {
                    if let Some(mut previous_sink) = sink.take() {
                        let _ = previous_sink.stop();
                    }

                    match playback_engine.start_stream(request.clone()) {
                        Ok(new_sink) => {
                            sink = Some(new_sink);
                            enabled = true;
                            let _ = event_tx.send(LocalMirrorEvent::Started {
                                backend_name: backend_name.clone(),
                                target_id: request.target_id.clone(),
                            });
                            let _ = event_tx
                                .send(LocalMirrorEvent::StateChanged(LocalMirrorState::Mirroring));
                        }
                        Err(error) => {
                            enabled = false;
                            clear_stale_frames(&frame_rx);
                            let _ = event_tx.send(LocalMirrorEvent::Error(error.to_string()));
                        }
                    }
                }
                LocalMirrorControl::Stop => {
                    enabled = false;
                    clear_stale_frames(&frame_rx);
                    if let Some(mut active_sink) = sink.take() {
                        if let Err(error) = active_sink.stop() {
                            let _ = event_tx.send(LocalMirrorEvent::Error(error.to_string()));
                        }
                    }
                    let _ =
                        event_tx.send(LocalMirrorEvent::StateChanged(LocalMirrorState::Disabled));
                }
                LocalMirrorControl::Shutdown => {
                    if let Some(mut active_sink) = sink.take() {
                        let _ = active_sink.stop();
                    }
                    return;
                }
            }
        }

        if !enabled {
            match control_rx.recv_timeout(LOCAL_MIRROR_IDLE_POLL_INTERVAL) {
                Ok(control) => match control {
                    LocalMirrorControl::Start(request) => {
                        match playback_engine.start_stream(request.clone()) {
                            Ok(new_sink) => {
                                sink = Some(new_sink);
                                enabled = true;
                                let _ = event_tx.send(LocalMirrorEvent::Started {
                                    backend_name: backend_name.clone(),
                                    target_id: request.target_id.clone(),
                                });
                                let _ = event_tx.send(LocalMirrorEvent::StateChanged(
                                    LocalMirrorState::Mirroring,
                                ));
                            }
                            Err(error) => {
                                clear_stale_frames(&frame_rx);
                                let _ = event_tx.send(LocalMirrorEvent::Error(error.to_string()));
                            }
                        }
                    }
                    LocalMirrorControl::Stop => {
                        let _ = event_tx
                            .send(LocalMirrorEvent::StateChanged(LocalMirrorState::Disabled));
                    }
                    LocalMirrorControl::Shutdown => return,
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            continue;
        }

        match frame_rx.recv_timeout(LOCAL_MIRROR_IDLE_POLL_INTERVAL) {
            Ok(frame) => {
                let Some(active_sink) = sink.as_mut() else {
                    enabled = false;
                    clear_stale_frames(&frame_rx);
                    let _ = event_tx.send(LocalMirrorEvent::Error(
                        "local mirror sink was unavailable during playback".to_string(),
                    ));
                    continue;
                };

                if let Err(error) = active_sink.write(&frame.payload) {
                    enabled = false;
                    clear_stale_frames(&frame_rx);
                    sink.take();
                    let _ = event_tx.send(LocalMirrorEvent::Error(error.to_string()));
                    continue;
                }

                let _ = event_tx.send(LocalMirrorEvent::Played {
                    bytes: frame.payload.len(),
                });
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn clear_stale_frames(frame_rx: &Receiver<FanoutAudioFrame>) {
    loop {
        match frame_rx.try_recv() {
            Ok(_) => {}
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffered_branch_queue_drops_oldest_when_full() {
        let queue = BufferedBranchQueue::new(2);

        assert_eq!(
            queue
                .push(FanoutAudioFrame {
                    sequence: 1,
                    captured_at_ms: 1,
                    captured_at_unix_ms: 1_001,
                    payload: vec![1],
                })
                .expect("first frame should enqueue"),
            BufferedPushOutcome::Enqueued
        );
        assert_eq!(
            queue
                .push(FanoutAudioFrame {
                    sequence: 2,
                    captured_at_ms: 2,
                    captured_at_unix_ms: 1_002,
                    payload: vec![2],
                })
                .expect("second frame should enqueue"),
            BufferedPushOutcome::Enqueued
        );
        assert_eq!(
            queue
                .push(FanoutAudioFrame {
                    sequence: 3,
                    captured_at_ms: 3,
                    captured_at_unix_ms: 1_003,
                    payload: vec![3],
                })
                .expect("full queue should drop the oldest frame"),
            BufferedPushOutcome::DroppedOldest
        );

        let snapshot = queue.snapshot();
        assert_eq!(snapshot.queued_packets, 2);
        assert_eq!(snapshot.dropped_packets, 1);
    }
}
