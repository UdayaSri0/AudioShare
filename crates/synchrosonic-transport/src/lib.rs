mod fanout;
mod protocol;
mod receiver;
mod sender;

pub use receiver::{LanReceiverTransportServer, ReceiverServerSnapshot};
pub use sender::{LanSenderSession, SenderTarget};

#[cfg(test)]
mod tests {
    use std::{
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        thread,
        time::{Duration, Instant},
    };

    use synchrosonic_audio::{PlaybackEngine, PlaybackSink, PlaybackStartRequest};
    use synchrosonic_core::{
        services::{AudioBackend, AudioCapture, ReceiverService},
        AudioError, AudioFrame, AudioSource, CaptureSettings, CaptureStats, PlaybackTarget,
        ReceiverError, ReceiverTransportEvent, StreamTargetFailureKind,
    };
    use synchrosonic_receiver::ReceiverRuntime;

    use crate::{LanReceiverTransportServer, LanSenderSession, SenderTarget};

    struct MockPlaybackEngine {
        bytes_written: Arc<AtomicUsize>,
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
                bytes_written: Arc::clone(&self.bytes_written),
            }))
        }
    }

    struct MockPlaybackSink {
        bytes_written: Arc<AtomicUsize>,
    }

    impl PlaybackSink for MockPlaybackSink {
        fn write(&mut self, payload: &[u8]) -> Result<(), AudioError> {
            self.bytes_written
                .fetch_add(payload.len(), Ordering::SeqCst);
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioError> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct MockAudioBackend {
        frames: Arc<Mutex<Vec<AudioFrame>>>,
    }

    impl AudioBackend for MockAudioBackend {
        fn backend_name(&self) -> &'static str {
            "mock-audio"
        }

        fn list_sources(&self) -> Result<Vec<AudioSource>, AudioError> {
            Ok(Vec::new())
        }

        fn list_playback_targets(&self) -> Result<Vec<PlaybackTarget>, AudioError> {
            Ok(Vec::new())
        }

        fn start_capture(
            &self,
            _settings: CaptureSettings,
        ) -> Result<Box<dyn AudioCapture>, AudioError> {
            Ok(Box::new(MockCapture {
                frames: self.frames.lock().expect("frames mutex").clone(),
                index: 0,
            }))
        }
    }

    struct MockCapture {
        frames: Vec<AudioFrame>,
        index: usize,
    }

    impl AudioCapture for MockCapture {
        fn recv_frame(&mut self) -> Result<AudioFrame, AudioError> {
            self.try_recv_frame()?.ok_or(AudioError::CaptureEnded)
        }

        fn try_recv_frame(&mut self) -> Result<Option<AudioFrame>, AudioError> {
            if self.frames.is_empty() {
                return Ok(None);
            }

            let mut frame = self.frames[self.index % self.frames.len()].clone();
            frame.sequence = self.index as u64;
            frame.captured_at = Duration::from_millis((self.index as u64) * 10);
            self.index += 1;
            Ok(Some(frame))
        }

        fn stats(&self) -> CaptureStats {
            CaptureStats::default()
        }

        fn stop(&mut self) -> Result<(), AudioError> {
            Ok(())
        }
    }

    #[test]
    fn sender_streams_audio_to_receiver_runtime_over_tcp() {
        let receiver_bytes_written = Arc::new(AtomicUsize::new(0));
        let local_mirror_bytes_written = Arc::new(AtomicUsize::new(0));
        let playback_engine = Arc::new(MockPlaybackEngine {
            bytes_written: Arc::clone(&receiver_bytes_written),
        });
        let receiver_config = synchrosonic_core::config::ReceiverConfig {
            enabled: true,
            listen_port: 0,
            ..synchrosonic_core::config::ReceiverConfig::default()
        };
        let receiver_runtime = Arc::new(Mutex::new(ReceiverRuntime::with_playback_engine(
            receiver_config.clone(),
            playback_engine,
        )));
        receiver_runtime
            .lock()
            .expect("receiver runtime mutex")
            .start()
            .expect("receiver runtime should start");

        let mut server = LanReceiverTransportServer::new(
            std::net::SocketAddr::from(([127, 0, 0, 1], receiver_config.listen_port)),
            receiver_config.advertised_name.clone(),
            receiver_config.latency_preset,
            500,
        );
        {
            let receiver_runtime_for_events = Arc::clone(&receiver_runtime);
            match server.start(move |event: ReceiverTransportEvent| {
                receiver_runtime_for_events
                    .lock()
                    .map_err(|_| ReceiverError::ThreadJoin)?
                    .submit_transport_event(event)
            }) {
                Ok(()) => {}
                Err(synchrosonic_core::TransportError::Bind { source, .. })
                    if source.kind() == std::io::ErrorKind::PermissionDenied =>
                {
                    receiver_runtime
                        .lock()
                        .expect("receiver runtime mutex")
                        .stop()
                        .expect("receiver runtime should stop after skipped bind");
                    return;
                }
                Err(error) => panic!("receiver server should start: {error}"),
            }
        }
        let listen_addr = server.snapshot().bind_addr;

        let frames = vec![
            AudioFrame::from_payload(
                0,
                Duration::from_millis(0),
                &CaptureSettings::default(),
                vec![0; 1_920],
            ),
            AudioFrame::from_payload(
                1,
                Duration::from_millis(10),
                &CaptureSettings::default(),
                vec![0; 1_920],
            ),
            AudioFrame::from_payload(
                2,
                Duration::from_millis(20),
                &CaptureSettings::default(),
                vec![0; 1_920],
            ),
            AudioFrame::from_payload(
                3,
                Duration::from_millis(30),
                &CaptureSettings::default(),
                vec![0; 1_920],
            ),
        ];

        let backend = MockAudioBackend {
            frames: Arc::new(Mutex::new(frames)),
        };
        let mut sender = LanSenderSession::with_playback_engine(
            synchrosonic_core::config::TransportConfig::default(),
            Arc::new(MockPlaybackEngine {
                bytes_written: Arc::clone(&local_mirror_bytes_written),
            }),
        );
        sender
            .start(
                backend,
                CaptureSettings::default(),
                SenderTarget::new(
                    synchrosonic_core::DeviceId::new("receiver-1"),
                    "Receiver",
                    synchrosonic_core::TransportEndpoint {
                        device_id: synchrosonic_core::DeviceId::new("receiver-1"),
                        address: listen_addr,
                    },
                ),
                "Sender",
            )
            .expect("sender should start");

        let streaming_established =
            wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
                let sender_snapshot = sender.snapshot();
                let receiver_snapshot = receiver_runtime
                    .lock()
                    .expect("receiver runtime mutex")
                    .snapshot();

                sender_snapshot.state == synchrosonic_core::StreamSessionState::Streaming
                    && sender_snapshot.metrics.packets_sent >= 4
                    && sender_snapshot.local_mirror.packets_played >= 4
                    && receiver_snapshot.metrics.packets_received >= 4
                    && receiver_bytes_written.load(Ordering::SeqCst) > 0
                    && local_mirror_bytes_written.load(Ordering::SeqCst) > 0
            });

        let sender_snapshot = sender.snapshot();
        let receiver_snapshot = receiver_runtime
            .lock()
            .expect("receiver runtime mutex")
            .snapshot();

        assert!(
            streaming_established,
            "sender/receiver pair did not reach stable streaming; sender_snapshot={sender_snapshot:?}, receiver_snapshot={receiver_snapshot:?}, receiver_bytes_written={}, local_mirror_bytes_written={}",
            receiver_bytes_written.load(Ordering::SeqCst),
            local_mirror_bytes_written.load(Ordering::SeqCst),
        );
        assert_eq!(
            sender_snapshot.state,
            synchrosonic_core::StreamSessionState::Streaming
        );
        assert!(sender_snapshot.metrics.packets_sent >= 4);
        assert!(sender_snapshot.local_mirror.packets_played >= 4);
        assert!(receiver_snapshot.metrics.packets_received >= 4);
        assert!(receiver_bytes_written.load(Ordering::SeqCst) > 0);
        assert!(local_mirror_bytes_written.load(Ordering::SeqCst) > 0);

        sender.stop().expect("sender should stop");
        server.stop().expect("receiver server should stop");
        receiver_runtime
            .lock()
            .expect("receiver runtime mutex")
            .stop()
            .expect("receiver runtime should stop");
    }

    #[test]
    fn sender_can_stream_to_multiple_targets_and_remove_one_without_stopping_the_other() {
        let receiver_one_bytes = Arc::new(AtomicUsize::new(0));
        let receiver_two_bytes = Arc::new(AtomicUsize::new(0));

        let Some(receiver_one) = spawn_test_receiver(Arc::new(MockPlaybackEngine {
            bytes_written: Arc::clone(&receiver_one_bytes),
        })) else {
            return;
        };
        let Some(receiver_two) = spawn_test_receiver(Arc::new(MockPlaybackEngine {
            bytes_written: Arc::clone(&receiver_two_bytes),
        })) else {
            return;
        };

        let frames = vec![
            AudioFrame::from_payload(
                0,
                Duration::from_millis(0),
                &CaptureSettings::default(),
                vec![0; 1_920],
            ),
            AudioFrame::from_payload(
                1,
                Duration::from_millis(10),
                &CaptureSettings::default(),
                vec![0; 1_920],
            ),
        ];
        let backend = MockAudioBackend {
            frames: Arc::new(Mutex::new(frames)),
        };
        let mut sender = LanSenderSession::with_playback_engine(
            synchrosonic_core::config::TransportConfig::default(),
            Arc::new(MockPlaybackEngine {
                bytes_written: Arc::new(AtomicUsize::new(0)),
            }),
        );

        sender
            .start(
                backend.clone(),
                CaptureSettings::default(),
                SenderTarget::new(
                    synchrosonic_core::DeviceId::new("receiver-1"),
                    "Receiver One",
                    synchrosonic_core::TransportEndpoint {
                        device_id: synchrosonic_core::DeviceId::new("receiver-1"),
                        address: receiver_one.listen_addr,
                    },
                ),
                "Sender",
            )
            .expect("first target should start");

        sender
            .start(
                backend,
                CaptureSettings::default(),
                SenderTarget::new(
                    synchrosonic_core::DeviceId::new("receiver-2"),
                    "Receiver Two",
                    synchrosonic_core::TransportEndpoint {
                        device_id: synchrosonic_core::DeviceId::new("receiver-2"),
                        address: receiver_two.listen_addr,
                    },
                ),
                "Sender",
            )
            .expect("second target should join active manager");
        let both_targets_streaming =
            wait_until(Duration::from_secs(2), Duration::from_millis(25), || {
                let snapshot = sender.snapshot();
                snapshot.state == synchrosonic_core::StreamSessionState::Streaming
                    && snapshot.targets.len() == 2
                    && snapshot.targets.iter().all(|target| {
                        target.state == synchrosonic_core::StreamSessionState::Streaming
                    })
                    && receiver_one_bytes.load(Ordering::SeqCst) > 0
                    && receiver_two_bytes.load(Ordering::SeqCst) > 0
            });
        let snapshot = sender.snapshot();
        assert!(
            both_targets_streaming,
            "sender did not reach stable two-target streaming; receiver_one_bytes={}, receiver_two_bytes={}, sender_state={:?}, sender_snapshot={:?}",
            receiver_one_bytes.load(Ordering::SeqCst),
            receiver_two_bytes.load(Ordering::SeqCst),
            snapshot.state,
            snapshot
        );
        assert_eq!(
            snapshot.state,
            synchrosonic_core::StreamSessionState::Streaming
        );
        assert_eq!(snapshot.targets.len(), 2);
        assert!(snapshot
            .targets
            .iter()
            .all(|target| target.state == synchrosonic_core::StreamSessionState::Streaming));
        assert!(receiver_one_bytes.load(Ordering::SeqCst) > 0);
        assert!(receiver_two_bytes.load(Ordering::SeqCst) > 0);

        let receiver_two_before = receiver_two_bytes.load(Ordering::SeqCst);
        let receiver_two_packets_before = receiver_two
            .runtime
            .lock()
            .expect("receiver runtime mutex")
            .snapshot()
            .metrics
            .packets_received;
        sender
            .stop_target(&synchrosonic_core::DeviceId::new("receiver-1"))
            .expect("first target should stop independently");
        let receiver_two_progressed_after_removal =
            wait_until(Duration::from_secs(4), Duration::from_millis(25), || {
                let snapshot = sender.snapshot();
                let receiver_two_packets_after = receiver_two
                    .runtime
                    .lock()
                    .expect("receiver runtime mutex")
                    .snapshot()
                    .metrics
                    .packets_received;
                snapshot.state == synchrosonic_core::StreamSessionState::Streaming
                    && snapshot.targets.len() == 1
                    && snapshot.targets[0].receiver_id.as_str() == "receiver-2"
                    && (receiver_two_bytes.load(Ordering::SeqCst) > receiver_two_before
                        || receiver_two_packets_after > receiver_two_packets_before)
            });

        let snapshot_after = sender.snapshot();
        assert!(
            receiver_two_progressed_after_removal,
            "receiver-2 did not continue receiving after receiver-1 was removed; before={}, after={}, sender_state={:?}, sender_snapshot={:?}, active_targets={:?}",
            receiver_two_before,
            receiver_two_bytes.load(Ordering::SeqCst),
            snapshot_after.state,
            snapshot_after,
            snapshot_after.targets
        );
        assert_eq!(
            snapshot_after.state,
            synchrosonic_core::StreamSessionState::Streaming
        );
        assert_eq!(snapshot_after.targets.len(), 1);
        assert_eq!(snapshot_after.targets[0].receiver_id.as_str(), "receiver-2");
        let receiver_two_after = receiver_two_bytes.load(Ordering::SeqCst);
        let receiver_two_packets_after = receiver_two
            .runtime
            .lock()
            .expect("receiver runtime mutex")
            .snapshot()
            .metrics
            .packets_received;
        assert!(
            receiver_two_after > receiver_two_before
                || receiver_two_packets_after > receiver_two_packets_before,
            "receiver-2 stopped progressing after receiver-1 was removed; bytes_before={}, bytes_after={}, packets_before={}, packets_after={}, sender_state={:?}, sender_snapshot={:?}",
            receiver_two_before,
            receiver_two_after,
            receiver_two_packets_before,
            receiver_two_packets_after,
            snapshot_after.state,
            snapshot_after
        );

        let mut receiver_one_last = receiver_one_bytes.load(Ordering::SeqCst);
        let receiver_one_stabilized =
            wait_until(Duration::from_secs(1), Duration::from_millis(25), || {
                let current = receiver_one_bytes.load(Ordering::SeqCst);
                let stabilized = current == receiver_one_last;
                receiver_one_last = current;
                stabilized
            });
        let receiver_one_before = receiver_one_bytes.load(Ordering::SeqCst);
        let receiver_one_continued = wait_until(
            Duration::from_millis(250),
            Duration::from_millis(25),
            || receiver_one_bytes.load(Ordering::SeqCst) > receiver_one_before,
        );
        let receiver_one_after = receiver_one_bytes.load(Ordering::SeqCst);
        let snapshot_after_receiver_one_stop = sender.snapshot();
        assert!(
            receiver_one_stabilized,
            "receiver-1 never stabilized after removal; before={}, after={}, sender_state={:?}, sender_snapshot={:?}, active_targets={:?}",
            receiver_one_before,
            receiver_one_after,
            snapshot_after_receiver_one_stop.state,
            snapshot_after_receiver_one_stop,
            snapshot_after_receiver_one_stop.targets
        );
        assert!(
            !receiver_one_continued,
            "receiver-1 continued receiving after removal; before={}, after={}, sender_state={:?}, sender_snapshot={:?}, active_targets={:?}",
            receiver_one_before,
            receiver_one_after,
            snapshot_after_receiver_one_stop.state,
            snapshot_after_receiver_one_stop,
            snapshot_after_receiver_one_stop.targets
        );

        sender.stop().expect("sender manager should stop");
        receiver_one.shutdown();
        receiver_two.shutdown();
    }

    #[test]
    fn receiver_server_snapshot_starts_inactive() {
        let server = LanReceiverTransportServer::new(
            std::net::SocketAddr::from(([127, 0, 0, 1], 51_700)),
            "Receiver",
            synchrosonic_core::ReceiverLatencyPreset::Balanced,
            1_000,
        );

        assert!(!server.snapshot().active);
    }

    #[test]
    fn sender_session_starts_idle() {
        let sender = LanSenderSession::new(synchrosonic_core::config::TransportConfig::default());

        assert_eq!(
            sender.snapshot().state,
            synchrosonic_core::StreamSessionState::Idle
        );
    }

    #[test]
    fn sender_snapshot_tracks_local_mirror_output_selection() {
        let mut sender =
            LanSenderSession::new(synchrosonic_core::config::TransportConfig::default());

        sender
            .set_local_playback_target(Some("bluez_output.11_22_33_44_55_66.a2dp-sink".to_string()))
            .expect("local mirror target should update");

        assert_eq!(
            sender.snapshot().local_mirror.playback_target_id.as_deref(),
            Some("bluez_output.11_22_33_44_55_66.a2dp-sink")
        );
    }

    #[test]
    fn sender_allows_quality_preset_changes_before_start() {
        let mut sender =
            LanSenderSession::new(synchrosonic_core::config::TransportConfig::default());

        sender
            .set_quality_preset(synchrosonic_core::QualityPreset::HighQuality)
            .expect("quality preset should update");

        assert_eq!(
            sender.snapshot().state,
            synchrosonic_core::StreamSessionState::Idle
        );
    }

    #[test]
    fn sender_blocks_self_target_connections() {
        let mut sender =
            LanSenderSession::new(synchrosonic_core::config::TransportConfig::default());
        sender.set_local_device_id(synchrosonic_core::DeviceId::new("self-device"));

        sender
            .start(
                MockAudioBackend {
                    frames: Arc::new(Mutex::new(Vec::new())),
                },
                CaptureSettings::default(),
                SenderTarget::new(
                    synchrosonic_core::DeviceId::new("self-device"),
                    "Self",
                    synchrosonic_core::TransportEndpoint {
                        device_id: synchrosonic_core::DeviceId::new("self-device"),
                        address: std::net::SocketAddr::from(([127, 0, 0, 1], 51_700)),
                    },
                ),
                "Sender",
            )
            .expect("sender manager should start");

        let blocked = wait_until(Duration::from_secs(1), Duration::from_millis(25), || {
            let snapshot = sender.snapshot();
            snapshot.targets.first().is_some_and(|target| {
                target.last_error_kind == Some(StreamTargetFailureKind::SelfTargetBlocked)
                    && target.attempt_count == 1
                    && target.state == synchrosonic_core::StreamSessionState::Error
            })
        });

        assert!(
            blocked,
            "sender did not mark self-target as blocked: {:?}",
            sender.snapshot()
        );
        sender.stop().expect("sender should stop");
    }

    #[test]
    fn sender_retries_refused_connections_with_bounded_backoff() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("ephemeral listener should bind");
        let unused_port = listener.local_addr().expect("listener addr").port();
        drop(listener);

        let mut sender =
            LanSenderSession::new(synchrosonic_core::config::TransportConfig::default());
        sender
            .start(
                MockAudioBackend {
                    frames: Arc::new(Mutex::new(Vec::new())),
                },
                CaptureSettings::default(),
                SenderTarget::new(
                    synchrosonic_core::DeviceId::new("receiver-refused"),
                    "Receiver Refused",
                    synchrosonic_core::TransportEndpoint {
                        device_id: synchrosonic_core::DeviceId::new("receiver-refused"),
                        address: std::net::SocketAddr::from(([127, 0, 0, 1], unused_port)),
                    },
                ),
                "Sender",
            )
            .expect("sender manager should start");

        let exhausted = wait_until(Duration::from_secs(6), Duration::from_millis(50), || {
            let snapshot = sender.snapshot();
            snapshot.targets.first().is_some_and(|target| {
                target.last_error_kind == Some(StreamTargetFailureKind::Refused)
                    && target.attempt_count >= 3
                    && target.state == synchrosonic_core::StreamSessionState::Error
                    && target.next_retry_at_unix_ms.is_none()
            })
        });

        assert!(
            exhausted,
            "sender did not stop retrying after bounded refused attempts: {:?}",
            sender.snapshot()
        );
        sender.stop().expect("sender should stop");
    }

    struct SpawnedReceiver {
        runtime: Arc<Mutex<ReceiverRuntime>>,
        server: LanReceiverTransportServer,
        listen_addr: std::net::SocketAddr,
    }

    impl SpawnedReceiver {
        fn shutdown(mut self) {
            self.server.stop().expect("receiver server should stop");
            self.runtime
                .lock()
                .expect("receiver runtime mutex")
                .stop()
                .expect("receiver runtime should stop");
        }
    }

    fn spawn_test_receiver(playback_engine: Arc<dyn PlaybackEngine>) -> Option<SpawnedReceiver> {
        let receiver_config = synchrosonic_core::config::ReceiverConfig {
            enabled: true,
            listen_port: 0,
            ..synchrosonic_core::config::ReceiverConfig::default()
        };
        let runtime = Arc::new(Mutex::new(ReceiverRuntime::with_playback_engine(
            receiver_config.clone(),
            playback_engine,
        )));
        runtime
            .lock()
            .expect("receiver runtime mutex")
            .start()
            .expect("receiver runtime should start");

        let mut server = LanReceiverTransportServer::new(
            std::net::SocketAddr::from(([127, 0, 0, 1], receiver_config.listen_port)),
            receiver_config.advertised_name.clone(),
            receiver_config.latency_preset,
            500,
        );
        {
            let runtime_for_events = Arc::clone(&runtime);
            match server.start(move |event: ReceiverTransportEvent| {
                runtime_for_events
                    .lock()
                    .map_err(|_| ReceiverError::ThreadJoin)?
                    .submit_transport_event(event)
            }) {
                Ok(()) => {}
                Err(synchrosonic_core::TransportError::Bind { source, .. })
                    if source.kind() == std::io::ErrorKind::PermissionDenied =>
                {
                    runtime
                        .lock()
                        .expect("receiver runtime mutex")
                        .stop()
                        .expect("receiver runtime should stop after skipped bind");
                    return None;
                }
                Err(error) => panic!("receiver server should start: {error}"),
            }
        }

        let listen_addr = server.snapshot().bind_addr;
        Some(SpawnedReceiver {
            runtime,
            server,
            listen_addr,
        })
    }

    // Polling on real state keeps transport tests deterministic on slower CI runners
    // where a fixed sleep can miss the exact moment work finishes.
    fn wait_until(
        timeout: Duration,
        interval: Duration,
        mut predicate: impl FnMut() -> bool,
    ) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if predicate() {
                return true;
            }
            thread::sleep(interval);
        }
        predicate()
    }
}
