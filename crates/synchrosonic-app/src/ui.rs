use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::glib::{self, ControlFlow};
use gtk::{Align, Orientation};
use synchrosonic_audio::LinuxAudioBackend;
use synchrosonic_core::{
    services::{AudioBackend, DiscoveryService, ReceiverService},
    AppConfig, AppState, AudioSourceKind, DiagnosticEvent,
};
use synchrosonic_discovery::MdnsDiscoveryService;
use synchrosonic_receiver::ReceiverRuntime;
use synchrosonic_transport::LanTransportService;

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
    let receiver = Rc::new(RefCell::new(ReceiverRuntime::new(config.receiver.clone())));
    state.apply_receiver_snapshot(receiver.borrow().snapshot());
    let state = Rc::new(RefCell::new(state));
    let transport = LanTransportService::new(None);

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
    let (receiver_page, receiver_status_label) =
        receiver_page(Rc::clone(&state), Rc::clone(&receiver));
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
        receiver_state = ?receiver.borrow().state(),
        transport_state = ?transport.state(),
        "SynchroSonic scaffold window activated"
    );

    start_discovery_poll(discovery, Rc::clone(&state), devices_label);
    start_receiver_poll(
        Rc::clone(&receiver),
        Rc::clone(&state),
        receiver_status_label,
        dashboard_status_label,
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

fn receiver_page(
    state: Rc<RefCell<AppState>>,
    receiver: Rc<RefCell<ReceiverRuntime>>,
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
        "Receiver mode now owns a listening/runtime lifecycle, explicit packet buffering, Linux PipeWire playback, and transport-event metrics. Prompt 5 can attach the future LAN stream by submitting connect/audio/keepalive/disconnect events into this runtime.",
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
        let receiver = Rc::clone(&receiver);
        let status_label = status_label.clone();
        start_button.connect_clicked(move |_| {
            let result = receiver.borrow_mut().start();
            match result {
                Ok(()) => {
                    let snapshot = receiver.borrow().snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_receiver_snapshot(snapshot.clone());
                    state.diagnostics.push(DiagnosticEvent::info(
                        "receiver",
                        format!(
                            "Receiver mode listening on {}:{} with {:?} latency preset.",
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
        let receiver = Rc::clone(&receiver);
        let status_label = status_label.clone();
        stop_button.connect_clicked(move |_| {
            let result = receiver.borrow_mut().stop();
            match result {
                Ok(()) => {
                    let snapshot = receiver.borrow().snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_receiver_snapshot(snapshot);
                    state.diagnostics.push(DiagnosticEvent::info(
                        "receiver",
                        "Receiver mode stopped and playback resources were released.",
                    ));
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
        ControlFlow::Continue
    });
}

fn start_receiver_poll(
    receiver: Rc<RefCell<ReceiverRuntime>>,
    state: Rc<RefCell<AppState>>,
    receiver_status_label: gtk::Label,
    dashboard_status_label: gtk::Label,
    audio_backend_name: String,
    audio_source_summary: String,
) {
    glib::timeout_add_seconds_local(1, move || {
        let snapshot = receiver.borrow().snapshot();
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
        "Session: {:?}\nCapture: {:?}\nReceiver: {:?}\nReceiver buffer: {}% ({} / {} packets)\nReceiver metrics: packets in={} played={} underruns={} overruns={} reconnects={}\nAudio backend: {}\n{}\nDefault stream port: {}\nLocal playback default: {}",
        state.cast_session,
        state.capture_state,
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
        "State: {:?}\nAdvertised name: {}\nListen address: {}:{}\nLatency preset: {:?}\nPlayback backend: {}\nPlayback target: {}\nConnection: {}\nBuffer: {} packet(s), {} frame(s), {}% full\nMetrics: packets in={} frames in={} bytes in={} packets out={} frames out={} bytes out={} underruns={} overruns={} reconnect attempts={}\nLast error: {}\nInternal app flow: use the Start/Stop buttons here; future transport code can submit Connected, AudioPacket, KeepAlive, and Disconnected events into the runtime.",
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
