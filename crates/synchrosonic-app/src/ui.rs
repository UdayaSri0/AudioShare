use std::{
    cell::RefCell,
    net::SocketAddr,
    rc::Rc,
    sync::{Arc, Mutex},
};

use adw::prelude::*;
use gtk::glib::{self, ControlFlow};
use gtk::{Align, Orientation};
use synchrosonic_audio::LinuxAudioBackend;
use synchrosonic_core::{
    services::{AudioBackend, DiscoveryService, ReceiverService},
    AppConfig, AppState, AudioSourceKind, DeviceId, DiagnosticEvent, DiscoveredDevice,
};
use synchrosonic_discovery::MdnsDiscoveryService;
use synchrosonic_receiver::ReceiverRuntime;
use synchrosonic_transport::{LanReceiverTransportServer, LanSenderSession, SenderTarget};

pub fn build_main_window(app: &adw::Application) {
    let mut config = AppConfig::default();
    config.receiver.enabled = true;
    let mut state = AppState::new(config.clone());
    let audio_backend = LinuxAudioBackend::new();
    let audio_source_summary = match audio_backend.list_sources() {
        Ok(sources) => {
            let source_count = sources.len();
            let selected = sources
                .iter()
                .find(|source| source.is_default && source.kind == AudioSourceKind::Monitor)
                .or_else(|| sources.iter().find(|source| source.is_default))
                .or_else(|| sources.first())
                .map(|source| source.display_name.clone())
                .unwrap_or_else(|| "No source selected".to_string());
            state.set_audio_sources(sources);
            format!("{source_count} capture source(s) available. Selected: {selected}")
        }
        Err(error) => {
            let message = format!("PipeWire source enumeration failed: {error}");
            state
                .diagnostics
                .push(DiagnosticEvent::warning("audio", message.clone()));
            message
        }
    };
    let mut discovery = MdnsDiscoveryService::new(
        config.discovery.clone(),
        config.receiver.advertised_name.clone(),
    );
    let discovery_summary = match discovery.start() {
        Ok(()) => format!("mDNS discovery active on {}", discovery.service_type()),
        Err(error) => {
            let message = format!("mDNS discovery failed to start: {error}");
            state
                .diagnostics
                .push(DiagnosticEvent::warning("discovery", message.clone()));
            message
        }
    };
    state.apply_discovery_snapshot(discovery.snapshot());
    let receiver = Arc::new(Mutex::new(ReceiverRuntime::new(config.receiver.clone())));
    let receiver_transport = Arc::new(Mutex::new(LanReceiverTransportServer::new(
        receiver_bind_addr(&config),
        config.receiver.advertised_name.clone(),
        config.receiver.latency_preset,
        config.transport.heartbeat_interval_ms,
    )));
    let sender = Arc::new(Mutex::new(LanSenderSession::new(config.transport.clone())));
    state.apply_receiver_snapshot(
        receiver
            .lock()
            .expect("receiver runtime mutex")
            .snapshot(),
    );
    state.apply_streaming_snapshot(sender.lock().expect("sender session mutex").snapshot());
    let state = Rc::new(RefCell::new(state));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("SynchroSonic")
        .default_width(1080)
        .default_height(720)
        .build();

    let root = gtk::Box::new(Orientation::Vertical, 0);
    let header = gtk::HeaderBar::new();
    let title = gtk::Label::new(Some("SynchroSonic"));
    title.add_css_class("title-2");
    header.set_title_widget(Some(&title));
    root.append(&header);

    let body = gtk::Box::new(Orientation::Horizontal, 12);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_margin_start(12);
    body.set_margin_end(12);

    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);

    let (dashboard, dashboard_status_label) =
        dashboard_page(&state.borrow(), audio_backend.backend_name(), &audio_source_summary);
    stack.add_titled(&dashboard, Some("dashboard"), "Dashboard");
    let (devices_page, devices_label) = discovery_page(&state.borrow(), &discovery_summary);
    stack.add_titled(
        &devices_page,
        Some("devices"),
        "Devices",
    );
    let (streaming_page, streaming_status_label, receiver_selector) = streaming_page(
        Rc::clone(&state),
        Arc::clone(&sender),
        audio_backend.clone(),
    );
    stack.add_titled(&streaming_page, Some("streaming"), "Streaming");
    let (receiver_page, receiver_status_label) =
        receiver_page(Rc::clone(&state), Arc::clone(&receiver), Arc::clone(&receiver_transport));
    stack.add_titled(&receiver_page, Some("receiver"), "Receiver");
    stack.add_titled(
        &status_page(
            "Audio",
            &format!(
                "{audio_source_summary}\nCapture frames expose sequence, timestamp, PCM payload bytes, peak, and RMS stats for local monitoring and the network encoder."
            ),
        ),
        Some("audio"),
        "Audio",
    );
    stack.add_titled(
        &status_page(
            "Diagnostics",
            state
                .borrow()
                .diagnostics
                .first()
                .map(|event| event.message.as_str())
                .unwrap_or("Diagnostics are ready for future runtime events."),
        ),
        Some("diagnostics"),
        "Diagnostics",
    );
    stack.add_titled(
        &status_page(
            "Settings",
            "Typed default configuration is loaded in memory.",
        ),
        Some("settings"),
        "Settings",
    );
    stack.add_titled(&about_page(), Some("about"), "About");

    let sidebar = gtk::StackSidebar::new();
    sidebar.set_stack(&stack);
    sidebar.set_width_request(220);

    body.append(&sidebar);
    body.append(&stack);
    root.append(&body);

    tracing::info!(
        audio_backend = audio_backend.backend_name(),
        discovery_service = discovery.service_type(),
        receiver_state = ?receiver.lock().expect("receiver runtime mutex").state(),
        sender_state = ?sender.lock().expect("sender session mutex").snapshot().state,
        "SynchroSonic scaffold window activated"
    );

    start_discovery_poll(
        discovery,
        Rc::clone(&state),
        devices_label,
        receiver_selector.clone(),
    );
    start_receiver_poll(
        Arc::clone(&receiver),
        Arc::clone(&receiver_transport),
        Rc::clone(&state),
        receiver_status_label,
        dashboard_status_label.clone(),
        audio_backend.backend_name().to_string(),
        audio_source_summary.clone(),
    );
    start_streaming_poll(
        Arc::clone(&sender),
        Rc::clone(&state),
        streaming_status_label,
        dashboard_status_label,
        receiver_selector,
        audio_backend.backend_name().to_string(),
        audio_source_summary.clone(),
    );

    window.set_content(Some(&root));
    window.present();
}

fn dashboard_page(
    state: &AppState,
    audio_backend_name: &str,
    audio_source_summary: &str,
) -> (gtk::Box, gtk::Label) {
    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title = gtk::Label::new(Some("Ready for implementation milestones"));
    title.add_css_class("title-1");
    title.set_halign(Align::Start);
    page.append(&title);

    let summary = gtk::Label::new(Some(
        "PipeWire source enumeration is active. Cast controls stay disabled until transport and receiver playback are wired to verified end-to-end session behavior.",
    ));
    summary.set_wrap(true);
    summary.set_halign(Align::Start);
    page.append(&summary);

    let status = gtk::Label::new(Some(&format_dashboard_status(
        state,
        audio_backend_name,
        audio_source_summary,
    )));
    status.set_halign(Align::Start);
    status.set_selectable(true);
    page.append(&status);

    let start_button = gtk::Button::with_label("Start Casting");
    start_button.set_sensitive(false);
    start_button.set_tooltip_text(Some(
        "Disabled until the capture session is connected to transport and receiver playback.",
    ));
    start_button.set_halign(Align::Start);
    page.append(&start_button);

    (page, status)
}

fn status_page(title: &str, message: &str) -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("title-1");
    heading.set_halign(Align::Start);
    page.append(&heading);

    let body = gtk::Label::new(Some(message));
    body.set_wrap(true);
    body.set_halign(Align::Start);
    page.append(&body);

    page
}

fn discovery_page(state: &AppState, status: &str) -> (gtk::Box, gtk::Label) {
    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let heading = gtk::Label::new(Some("Devices"));
    heading.add_css_class("title-1");
    heading.set_halign(Align::Start);
    page.append(&heading);

    let status_label = gtk::Label::new(Some(status));
    status_label.set_wrap(true);
    status_label.set_halign(Align::Start);
    page.append(&status_label);

    let devices_label = gtk::Label::new(Some(&format_discovery_devices(state)));
    devices_label.set_wrap(true);
    devices_label.set_selectable(true);
    devices_label.set_halign(Align::Start);
    page.append(&devices_label);

    (page, devices_label)
}

fn streaming_page(
    state: Rc<RefCell<AppState>>,
    sender: Arc<Mutex<LanSenderSession>>,
    audio_backend: LinuxAudioBackend,
) -> (gtk::Box, gtk::Label, gtk::ComboBoxText) {
    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let heading = gtk::Label::new(Some("Streaming"));
    heading.add_css_class("title-1");
    heading.set_halign(Align::Start);
    page.append(&heading);

    let summary = gtk::Label::new(Some(
        "The sender uses a TCP session with a framed SynchroSonic protocol and raw PCM payloads. Pick one discovered receiver, negotiate the session, then push captured PipeWire frames straight into the receiver playback pipeline.",
    ));
    summary.set_wrap(true);
    summary.set_halign(Align::Start);
    page.append(&summary);

    let receiver_selector = gtk::ComboBoxText::new();
    refresh_receiver_selector(&receiver_selector, &state.borrow());
    {
        let state = Rc::clone(&state);
        receiver_selector.connect_changed(move |selector| {
            if let Some(active_id) = selector.active_id() {
                let _ = state
                    .borrow_mut()
                    .select_receiver_device(DeviceId::new(active_id.as_str()));
            }
        });
    }
    page.append(&receiver_selector);

    let controls = gtk::Box::new(Orientation::Horizontal, 12);
    let start_button = gtk::Button::with_label("Start Stream");
    let stop_button = gtk::Button::with_label("Stop Stream");
    controls.append(&start_button);
    controls.append(&stop_button);
    page.append(&controls);

    let status_label = gtk::Label::new(Some(&format_streaming_status(&state.borrow())));
    status_label.set_wrap(true);
    status_label.set_selectable(true);
    status_label.set_halign(Align::Start);
    page.append(&status_label);

    {
        let state = Rc::clone(&state);
        let sender = Arc::clone(&sender);
        let status_label = status_label.clone();
        let receiver_selector = receiver_selector.clone();
        let audio_backend = audio_backend.clone();
        start_button.connect_clicked(move |_| {
            let target = {
                let active_id = receiver_selector.active_id();
                let state = state.borrow();
                active_id
                    .as_ref()
                    .and_then(|id| find_receiver_device(&state, id.as_str()))
            };

            let Some(device) = target else {
                let mut state = state.borrow_mut();
                state.diagnostics.push(DiagnosticEvent::warning(
                    "streaming",
                    "No receiver with a resolved transport endpoint is currently selected.",
                ));
                status_label.set_text(&format_streaming_status(&state));
                return;
            };

            let Some(endpoint) = device.endpoint.clone() else {
                let mut state = state.borrow_mut();
                state.diagnostics.push(DiagnosticEvent::warning(
                    "streaming",
                    "Selected receiver does not expose a transport endpoint yet.",
                ));
                status_label.set_text(&format_streaming_status(&state));
                return;
            };

            let target = SenderTarget::new(device.id.clone(), device.display_name.clone(), endpoint);
            let capture_settings = state.borrow().config.audio.capture_settings();
            let sender_name = state.borrow().receiver.advertised_name.clone();

            match sender
                .lock()
                .expect("sender session mutex")
                .start(audio_backend.clone(), capture_settings, target, sender_name)
            {
                Ok(()) => {
                    let snapshot = sender.lock().expect("sender session mutex").snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_streaming_snapshot(snapshot.clone());
                    state.diagnostics.push(DiagnosticEvent::info(
                        "streaming",
                        format!(
                            "Started LAN stream toward {} at {}.",
                            snapshot
                                .receiver_name
                                .as_deref()
                                .unwrap_or("receiver"),
                            snapshot
                                .endpoint
                                .map(|endpoint| endpoint.to_string())
                                .unwrap_or_else(|| "unknown endpoint".to_string())
                        ),
                    ));
                    status_label.set_text(&format_streaming_status(&state));
                }
                Err(error) => {
                    let mut state = state.borrow_mut();
                    state.diagnostics.push(DiagnosticEvent::error(
                        "streaming",
                        format!("Failed to start stream: {error}"),
                    ));
                    status_label.set_text(&format_streaming_status(&state));
                }
            }
        });
    }

    {
        let state = Rc::clone(&state);
        let sender = Arc::clone(&sender);
        let status_label = status_label.clone();
        stop_button.connect_clicked(move |_| {
            match sender.lock().expect("sender session mutex").stop() {
                Ok(()) => {
                    let snapshot = sender.lock().expect("sender session mutex").snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_streaming_snapshot(snapshot);
                    state.diagnostics.push(DiagnosticEvent::info(
                        "streaming",
                        "Sender stream stopped and transport resources were released.",
                    ));
                    status_label.set_text(&format_streaming_status(&state));
                }
                Err(error) => {
                    let mut state = state.borrow_mut();
                    state.diagnostics.push(DiagnosticEvent::error(
                        "streaming",
                        format!("Failed to stop stream: {error}"),
                    ));
                    status_label.set_text(&format_streaming_status(&state));
                }
            }
        });
    }

    (page, status_label, receiver_selector)
}

fn receiver_page(
    state: Rc<RefCell<AppState>>,
    receiver: Arc<Mutex<ReceiverRuntime>>,
    receiver_transport: Arc<Mutex<LanReceiverTransportServer>>,
) -> (gtk::Box, gtk::Label) {
    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let heading = gtk::Label::new(Some("Receiver Mode"));
    heading.add_css_class("title-1");
    heading.set_halign(Align::Start);
    page.append(&heading);

    let summary = gtk::Label::new(Some(
        "Receiver mode now owns a listening/runtime lifecycle, explicit packet buffering, Linux PipeWire playback, and transport-event metrics. The TCP receiver listener feeds connect/audio/keepalive/disconnect events into this runtime.",
    ));
    summary.set_wrap(true);
    summary.set_halign(Align::Start);
    page.append(&summary);

    let controls = gtk::Box::new(Orientation::Horizontal, 12);
    let start_button = gtk::Button::with_label("Start Receiver");
    let stop_button = gtk::Button::with_label("Stop Receiver");
    controls.append(&start_button);
    controls.append(&stop_button);
    page.append(&controls);

    let status_label = gtk::Label::new(Some(&format_receiver_status(&state.borrow())));
    status_label.set_wrap(true);
    status_label.set_selectable(true);
    status_label.set_halign(Align::Start);
    page.append(&status_label);

    {
        let state = Rc::clone(&state);
        let receiver = Arc::clone(&receiver);
        let receiver_transport = Arc::clone(&receiver_transport);
        let status_label = status_label.clone();
        start_button.connect_clicked(move |_| {
            let runtime_result = receiver.lock().expect("receiver runtime mutex").start();
            match runtime_result {
                Ok(()) => {
                    let start_server = {
                        let receiver_for_events = Arc::clone(&receiver);
                        receiver_transport.lock().expect("receiver transport mutex").start(
                            move |event| {
                                receiver_for_events
                                    .lock()
                                    .map_err(|_| synchrosonic_core::ReceiverError::ThreadJoin)?
                                    .submit_transport_event(event)
                            },
                        )
                    };

                    if let Err(error) = start_server {
                        let _ = receiver.lock().expect("receiver runtime mutex").stop();
                        let mut state = state.borrow_mut();
                        state.diagnostics.push(DiagnosticEvent::error(
                            "receiver",
                            format!("Receiver transport listener failed to start: {error}"),
                        ));
                        status_label.set_text(&format_receiver_status(&state));
                        return;
                    }

                    let snapshot = receiver.lock().expect("receiver runtime mutex").snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_receiver_snapshot(snapshot.clone());
                    state.diagnostics.push(DiagnosticEvent::info(
                        "receiver",
                        format!(
                            "Receiver mode listening on {}:{} with {:?} latency preset and TCP transport ready.",
                            snapshot.bind_host, snapshot.listen_port, snapshot.latency_preset
                        ),
                    ));
                    status_label.set_text(&format_receiver_status(&state));
                }
                Err(error) => {
                    let mut state = state.borrow_mut();
                    state.diagnostics.push(DiagnosticEvent::error(
                        "receiver",
                        format!("Failed to start receiver mode: {error}"),
                    ));
                    status_label.set_text(&format_receiver_status(&state));
                }
            }
        });
    }

    {
        let state = Rc::clone(&state);
        let receiver = Arc::clone(&receiver);
        let receiver_transport = Arc::clone(&receiver_transport);
        let status_label = status_label.clone();
        stop_button.connect_clicked(move |_| {
            let stop_transport = receiver_transport.lock().expect("receiver transport mutex").stop();
            let result = receiver.lock().expect("receiver runtime mutex").stop();
            match result {
                Ok(()) => {
                    let snapshot = receiver.lock().expect("receiver runtime mutex").snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_receiver_snapshot(snapshot);
                    if let Err(error) = stop_transport {
                        state.diagnostics.push(DiagnosticEvent::warning(
                            "receiver",
                            format!("Receiver transport stop reported: {error}"),
                        ));
                    }
                    state.diagnostics.push(DiagnosticEvent::info("receiver", "Receiver mode stopped and playback/transport resources were released."));
                    status_label.set_text(&format_receiver_status(&state));
                }
                Err(error) => {
                    let mut state = state.borrow_mut();
                    state.diagnostics.push(DiagnosticEvent::error(
                        "receiver",
                        format!("Failed to stop receiver mode: {error}"),
                    ));
                    status_label.set_text(&format_receiver_status(&state));
                }
            }
        });
    }

    (page, status_label)
}

fn start_discovery_poll(
    mut discovery: MdnsDiscoveryService,
    state: Rc<RefCell<AppState>>,
    devices_label: gtk::Label,
    receiver_selector: gtk::ComboBoxText,
) {
    glib::timeout_add_seconds_local(1, move || {
        loop {
            match discovery.poll_event() {
                Ok(Some(event)) => state.borrow_mut().apply_discovery_event(event),
                Ok(None) => break,
                Err(error) => {
                    state.borrow_mut().diagnostics.push(DiagnosticEvent::warning(
                        "discovery",
                        format!("mDNS discovery event error: {error}"),
                    ));
                    break;
                }
            }
        }

        match discovery.prune_stale() {
            Ok(events) => {
                for event in events {
                    state.borrow_mut().apply_discovery_event(event);
                }
            }
            Err(error) => state.borrow_mut().diagnostics.push(DiagnosticEvent::warning(
                "discovery",
                format!("mDNS stale pruning failed: {error}"),
            )),
        }

        state
            .borrow_mut()
            .apply_discovery_snapshot(discovery.snapshot());
        devices_label.set_text(&format_discovery_devices(&state.borrow()));
        refresh_receiver_selector(&receiver_selector, &state.borrow());
        ControlFlow::Continue
    });
}

fn start_receiver_poll(
    receiver: Arc<Mutex<ReceiverRuntime>>,
    receiver_transport: Arc<Mutex<LanReceiverTransportServer>>,
    state: Rc<RefCell<AppState>>,
    receiver_status_label: gtk::Label,
    dashboard_status_label: gtk::Label,
    audio_backend_name: String,
    audio_source_summary: String,
) {
    glib::timeout_add_seconds_local(1, move || {
        let snapshot = receiver.lock().expect("receiver runtime mutex").snapshot();
        let transport_snapshot = receiver_transport
            .lock()
            .expect("receiver transport mutex")
            .snapshot();
        {
            let mut state = state.borrow_mut();
            let state_changed = state.receiver.state != snapshot.state;
            let last_error_changed = state.receiver.last_error != snapshot.last_error;
            let state_message = if state_changed {
                Some(format!("Receiver state is now {:?}", snapshot.state))
            } else {
                None
            };
            let error_message = if last_error_changed {
                snapshot
                    .last_error
                    .as_ref()
                    .map(|error| format!("Receiver runtime reported: {error}"))
            } else {
                None
            };
            state.apply_receiver_snapshot(snapshot);

            if let Some(message) = state_message {
                state.diagnostics.push(DiagnosticEvent::info(
                    "receiver",
                    message,
                ));
            }
            if let Some(message) = error_message {
                state
                    .diagnostics
                    .push(DiagnosticEvent::warning("receiver", message));
            }
            if let Some(error) = &transport_snapshot.last_error {
                if state
                    .diagnostics
                    .last()
                    .map(|event| event.message.as_str())
                    != Some(error.as_str())
                {
                    state.diagnostics.push(DiagnosticEvent::warning(
                        "receiver-transport",
                        format!("Receiver TCP listener reported: {error}"),
                    ));
                }
            }

            receiver_status_label.set_text(&format_receiver_status(&state));
            dashboard_status_label.set_text(&format_dashboard_status(
                &state,
                &audio_backend_name,
                &audio_source_summary,
            ));
        }

        ControlFlow::Continue
    });
}

fn start_streaming_poll(
    sender: Arc<Mutex<LanSenderSession>>,
    state: Rc<RefCell<AppState>>,
    streaming_status_label: gtk::Label,
    dashboard_status_label: gtk::Label,
    receiver_selector: gtk::ComboBoxText,
    audio_backend_name: String,
    audio_source_summary: String,
) {
    glib::timeout_add_seconds_local(1, move || {
        let snapshot = sender.lock().expect("sender session mutex").snapshot();
        let state_message = {
            let state = state.borrow();
            if state.streaming.state != snapshot.state {
                Some(format!("Sender stream state is now {:?}", snapshot.state))
            } else {
                None
            }
        };
        let error_message = {
            let state = state.borrow();
            if state.streaming.last_error != snapshot.last_error {
                snapshot
                    .last_error
                    .as_ref()
                    .map(|error| format!("Sender stream reported: {error}"))
            } else {
                None
            }
        };

        {
            let mut state = state.borrow_mut();
            state.apply_streaming_snapshot(snapshot);
            if let Some(message) = state_message {
                state
                    .diagnostics
                    .push(DiagnosticEvent::info("streaming", message));
            }
            if let Some(message) = error_message {
                state
                    .diagnostics
                    .push(DiagnosticEvent::warning("streaming", message));
            }

            streaming_status_label.set_text(&format_streaming_status(&state));
            dashboard_status_label.set_text(&format_dashboard_status(
                &state,
                &audio_backend_name,
                &audio_source_summary,
            ));
            refresh_receiver_selector(&receiver_selector, &state);
        }

        ControlFlow::Continue
    });
}

fn format_discovery_devices(state: &AppState) -> String {
    if state.discovered_devices.is_empty() {
        return "No SynchroSonic devices discovered yet.".to_string();
    }

    state
        .discovered_devices
        .iter()
        .map(|device| {
            format!(
                "{}\n  id: {}\n  version: {}\n  availability: {:?}\n  capabilities: sender={}, receiver={}, local_output={}, bluetooth={}\n  endpoint: {}",
                device.display_name,
                device.id,
                device.app_version,
                device.availability,
                device.capabilities.supports_sender,
                device.capabilities.supports_receiver,
                device.capabilities.supports_local_output,
                device.capabilities.supports_bluetooth_output,
                device
                    .endpoint
                    .as_ref()
                    .map(|endpoint| endpoint.address.to_string())
                    .unwrap_or_else(|| "unresolved".to_string())
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn about_page() -> gtk::Box {
    status_page(
        "About",
        "SynchroSonic is a Linux-first Rust desktop app for LAN audio streaming. Bluetooth is deferred and will be designed as a later output/backend capability.",
    )
}

fn format_dashboard_status(
    state: &AppState,
    audio_backend_name: &str,
    audio_source_summary: &str,
) -> String {
    format!(
        "Session: {:?}\nCapture: {:?}\nStreaming: {:?}\nSelected receiver: {}\nSender metrics: packets sent={} bytes sent={} bitrate={}bps latency={:?}ms gaps={} keepalives={}/{}\nReceiver: {:?}\nReceiver buffer: {}% ({} / {} packets)\nReceiver metrics: packets in={} played={} underruns={} overruns={} reconnects={}\nAudio backend: {}\n{}\nDefault stream port: {}\nLocal playback default: {}",
        state.cast_session,
        state.capture_state,
        state.streaming.state,
        state
            .selected_receiver_device_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "none".to_string()),
        state.streaming.metrics.packets_sent,
        state.streaming.metrics.bytes_sent,
        state.streaming.metrics.estimated_bitrate_bps,
        state.streaming.metrics.latency_estimate_ms,
        state.streaming.metrics.packet_gaps,
        state.streaming.metrics.keepalives_sent,
        state.streaming.metrics.keepalives_received,
        state.receiver.state,
        state.receiver.buffer.fill_percent(),
        state.receiver.buffer.queued_packets,
        state.receiver.buffer.max_packets,
        state.receiver.metrics.packets_received,
        state.receiver.metrics.packets_played,
        state.receiver.metrics.underruns,
        state.receiver.metrics.overruns,
        state.receiver.metrics.reconnect_attempts,
        audio_backend_name,
        audio_source_summary,
        state.config.transport.stream_port,
        if state.config.audio.local_playback_enabled {
            "enabled"
        } else {
            "disabled"
        }
    )
}

fn format_streaming_status(state: &AppState) -> String {
    let stream = &state.streaming;
    let selected_device = state
        .selected_receiver_device_id
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "none".to_string());
    let receiver = stream
        .receiver_name
        .as_deref()
        .unwrap_or("none");
    let endpoint = stream
        .endpoint
        .map(|endpoint| endpoint.to_string())
        .unwrap_or_else(|| "unresolved".to_string());
    let stream_shape = stream
        .stream
        .as_ref()
        .map(|stream| {
            format!(
                "{}Hz/{}ch/{:?}/{}fpp",
                stream.sample_rate_hz,
                stream.channels,
                stream.sample_format,
                stream.frames_per_packet
            )
        })
        .unwrap_or_else(|| "not negotiated".to_string());
    let last_error = stream.last_error.as_deref().unwrap_or("none");

    format!(
        "State: {:?}\nSelected receiver id: {}\nNegotiated receiver: {}\nEndpoint: {}\nSession id: {}\nCodec: {}\nStream: {}\nMetrics: packets sent={} packets received={} bytes sent={} bytes received={} bitrate={}bps latency estimate={:?}ms packet gaps={} keepalives sent={} keepalives received={}\nLast error: {}\nTransport path: TCP session -> framed control/audio messages -> receiver transport server -> receiver runtime buffer -> PipeWire playback.",
        stream.state,
        selected_device,
        receiver,
        endpoint,
        stream.session_id.as_deref().unwrap_or("not started"),
        stream
            .codec
            .map(|codec| format!("{codec:?}"))
            .unwrap_or_else(|| "not negotiated".to_string()),
        stream_shape,
        stream.metrics.packets_sent,
        stream.metrics.packets_received,
        stream.metrics.bytes_sent,
        stream.metrics.bytes_received,
        stream.metrics.estimated_bitrate_bps,
        stream.metrics.latency_estimate_ms,
        stream.metrics.packet_gaps,
        stream.metrics.keepalives_sent,
        stream.metrics.keepalives_received,
        last_error
    )
}

fn refresh_receiver_selector(selector: &gtk::ComboBoxText, state: &AppState) {
    let active_id = state
        .selected_receiver_device_id
        .as_ref()
        .map(ToString::to_string);
    selector.remove_all();

    for device in state
        .discovered_devices
        .iter()
        .filter(|device| device.capabilities.supports_receiver)
    {
        let label = format!(
            "{} ({})",
            device.display_name,
            device
                .endpoint
                .as_ref()
                .map(|endpoint| endpoint.address.to_string())
                .unwrap_or_else(|| "unresolved".to_string())
        );
        selector.append(Some(device.id.as_str()), &label);
    }

    if let Some(active_id) = active_id {
        selector.set_active_id(Some(&active_id));
    } else if !state.discovered_devices.is_empty() {
        selector.set_active(Some(0));
    }
}

fn find_receiver_device(state: &AppState, device_id: &str) -> Option<DiscoveredDevice> {
    state
        .discovered_devices
        .iter()
        .find(|device| device.id.as_str() == device_id && device.capabilities.supports_receiver)
        .cloned()
}

fn receiver_bind_addr(config: &AppConfig) -> SocketAddr {
    format!("{}:{}", config.receiver.bind_host, config.receiver.listen_port)
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], config.receiver.listen_port)))
}

fn format_receiver_status(state: &AppState) -> String {
    let receiver = &state.receiver;
    let connection = receiver
        .connection
        .as_ref()
        .map(|connection| {
            format!(
                "session={} remote={} stream={}Hz/{}ch/{:?}/{}fpp",
                connection.session_id,
                connection
                    .remote_addr
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|| "unresolved".to_string()),
                connection.stream.sample_rate_hz,
                connection.stream.channels,
                connection.stream.sample_format,
                connection.stream.frames_per_packet
            )
        })
        .unwrap_or_else(|| "none".to_string());
    let last_error = receiver
        .last_error
        .as_deref()
        .unwrap_or("none");

    format!(
        "State: {:?}\nAdvertised name: {}\nListen address: {}:{}\nLatency preset: {:?}\nPlayback backend: {}\nPlayback target: {}\nConnection: {}\nBuffer: {} packet(s), {} frame(s), {}% full\nMetrics: packets in={} frames in={} bytes in={} packets out={} frames out={} bytes out={} underruns={} overruns={} reconnect attempts={}\nLast error: {}\nInternal app flow: use the Start/Stop buttons here; the TCP receiver listener already submits Connected, AudioPacket, KeepAlive, and Disconnected events into the runtime.",
        receiver.state,
        receiver.advertised_name,
        receiver.bind_host,
        receiver.listen_port,
        receiver.latency_preset,
        receiver
            .playback_backend
            .as_deref()
            .unwrap_or("not configured"),
        receiver
            .playback_target_id
            .as_deref()
            .unwrap_or("default PipeWire sink"),
        connection,
        receiver.buffer.queued_packets,
        receiver.buffer.queued_frames,
        receiver.buffer.fill_percent(),
        receiver.metrics.packets_received,
        receiver.metrics.frames_received,
        receiver.metrics.bytes_received,
        receiver.metrics.packets_played,
        receiver.metrics.frames_played,
        receiver.metrics.bytes_played,
        receiver.metrics.underruns,
        receiver.metrics.overruns,
        receiver.metrics.reconnect_attempts,
        last_error
    )
}
