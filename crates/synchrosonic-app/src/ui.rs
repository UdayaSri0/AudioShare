use std::{
    cell::RefCell,
    collections::HashMap,
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
    PlaybackTarget, PlaybackTargetKind, StreamTargetSnapshot,
};
use synchrosonic_discovery::MdnsDiscoveryService;
use synchrosonic_receiver::ReceiverRuntime;
use synchrosonic_transport::{LanReceiverTransportServer, LanSenderSession, SenderTarget};

const DEFAULT_PLAYBACK_TARGET_ID: &str = "__system_default_playback__";
const SYSTEM_DEFAULT_OUTPUT_LABEL: &str = "System default output";

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
    let playback_target_summary = match audio_backend.list_playback_targets() {
        Ok(targets) => {
            let target_count = targets.len();
            let bluetooth_count = targets
                .iter()
                .filter(|target| target.is_bluetooth())
                .count();
            let default_target = targets
                .iter()
                .find(|target| target.is_default)
                .map(|target| target.display_name.clone())
                .unwrap_or_else(|| SYSTEM_DEFAULT_OUTPUT_LABEL.to_string());
            state.set_playback_targets(targets);
            format!(
                "{target_count} playback output(s) available, {bluetooth_count} Bluetooth. Default target: {default_target}"
            )
        }
        Err(error) => {
            let message = format!("PipeWire playback target enumeration failed: {error}");
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
    let _ = sender
        .lock()
        .expect("sender session mutex")
        .set_local_playback_target(config.audio.local_playback_target_id.clone());
    state.apply_receiver_snapshot(receiver.lock().expect("receiver runtime mutex").snapshot());
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

    let (dashboard, dashboard_status_label) = dashboard_page(
        &state.borrow(),
        audio_backend.backend_name(),
        &audio_source_summary,
    );
    stack.add_titled(&dashboard, Some("dashboard"), "Dashboard");
    let (devices_page, devices_label) = discovery_page(&state.borrow(), &discovery_summary);
    stack.add_titled(&devices_page, Some("devices"), "Devices");
    let (streaming_page, streaming_status_label, receiver_selector, local_output_selector) =
        streaming_page(
            Rc::clone(&state),
            Arc::clone(&sender),
            audio_backend.clone(),
        );
    stack.add_titled(&streaming_page, Some("streaming"), "Streaming");
    let (receiver_page, receiver_status_label, receiver_output_selector) = receiver_page(
        Rc::clone(&state),
        Arc::clone(&receiver),
        Arc::clone(&receiver_transport),
    );
    stack.add_titled(&receiver_page, Some("receiver"), "Receiver");
    stack.add_titled(
        &status_page(
            "Audio",
            &format!(
                "{audio_source_summary}\n{playback_target_summary}\nCapture frames expose sequence, timestamp, PCM payload bytes, peak, and RMS stats for local monitoring and the network encoder."
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
        receiver_status_label.clone(),
        dashboard_status_label.clone(),
        audio_backend.backend_name().to_string(),
        audio_source_summary.clone(),
    );
    start_streaming_poll(
        Arc::clone(&sender),
        Rc::clone(&state),
        streaming_status_label.clone(),
        dashboard_status_label.clone(),
        receiver_selector,
        audio_backend.backend_name().to_string(),
        audio_source_summary.clone(),
    );
    start_playback_target_poll(
        audio_backend.clone(),
        Arc::clone(&sender),
        Arc::clone(&receiver),
        Rc::clone(&state),
        local_output_selector,
        receiver_output_selector,
        streaming_status_label,
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
        "PipeWire source enumeration is active and the sender/receiver pipeline is live. Use the Streaming page to start a cast session and decide whether the captured stream also mirrors to a local playback branch.",
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
        "Use the dedicated Streaming page controls for live cast and local mirror management.",
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
) -> (gtk::Box, gtk::Label, gtk::ComboBoxText, gtk::ComboBoxText) {
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
        "The sender uses one capture stream with an explicit fan-out: each receiver gets its own bounded network branch, and an optional bounded local branch mirrors playback on the sender. Add or remove the selected receiver without collapsing the rest of the cast.",
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

    let mirror_row = gtk::Box::new(Orientation::Horizontal, 12);
    let mirror_label = gtk::Label::new(Some("Mirror locally while casting"));
    mirror_label.set_halign(Align::Start);
    let mirror_switch = gtk::Switch::new();
    mirror_switch.set_active(state.borrow().config.audio.local_playback_enabled);
    mirror_row.append(&mirror_label);
    mirror_row.append(&mirror_switch);
    page.append(&mirror_row);

    let mirror_target_selector = gtk::ComboBoxText::new();
    refresh_playback_target_selector(
        &mirror_target_selector,
        &state.borrow(),
        state
            .borrow()
            .config
            .audio
            .local_playback_target_id
            .as_deref(),
        "Local mirror output: system default",
    );
    page.append(&mirror_target_selector);

    let controls = gtk::Box::new(Orientation::Horizontal, 12);
    let start_button = gtk::Button::with_label("Add Selected Target");
    let stop_button = gtk::Button::with_label("Remove Selected Target");
    let stop_all_button = gtk::Button::with_label("Stop All");
    controls.append(&start_button);
    controls.append(&stop_button);
    controls.append(&stop_all_button);
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
        mirror_switch.connect_active_notify(move |toggle| {
            let enabled = toggle.is_active();
            let (result, snapshot) = {
                let sender = sender.lock().expect("sender session mutex");
                (
                    sender.set_local_playback_enabled(enabled),
                    sender.snapshot(),
                )
            };

            let mut state = state.borrow_mut();
            state.set_local_playback_enabled(enabled);
            state.apply_streaming_snapshot(snapshot);

            match result {
                Ok(()) => {
                    let scope =
                        if state.streaming.state == synchrosonic_core::StreamSessionState::Idle {
                            "next cast session"
                        } else {
                            "active cast session"
                        };
                    state.diagnostics.push(DiagnosticEvent::info(
                        "streaming",
                        format!(
                            "Local playback mirror {} for the {scope}.",
                            if enabled { "enabled" } else { "disabled" }
                        ),
                    ));
                }
                Err(error) => {
                    state.diagnostics.push(DiagnosticEvent::error(
                        "streaming",
                        format!("Failed to update local playback mirror: {error}"),
                    ));
                }
            }

            status_label.set_text(&format_streaming_status(&state));
        });
    }

    {
        let state = Rc::clone(&state);
        let sender = Arc::clone(&sender);
        let status_label = status_label.clone();
        mirror_target_selector.connect_changed(move |selector| {
            let target_id = selector
                .active_id()
                .and_then(|id| playback_target_id_from_selection(id.as_str()));
            if state.borrow().config.audio.local_playback_target_id == target_id {
                return;
            }

            let result = {
                let mut sender = sender.lock().expect("sender session mutex");
                sender.set_local_playback_target(target_id.clone())
            };

            let snapshot = sender.lock().expect("sender session mutex").snapshot();
            let mut state = state.borrow_mut();
            state.select_local_playback_target(target_id.clone());
            state.apply_streaming_snapshot(snapshot);

            match result {
                Ok(()) => {
                    let selected_target = format_selected_playback_target(
                        &state,
                        state.config.audio.local_playback_target_id.as_deref(),
                    );
                    state.diagnostics.push(DiagnosticEvent::info(
                        "streaming",
                        format!("Local mirror output target set to {selected_target}."),
                    ));
                }
                Err(error) => {
                    state.diagnostics.push(DiagnosticEvent::warning(
                        "streaming",
                        format!("Failed to update local mirror output target: {error}"),
                    ));
                }
            }

            status_label.set_text(&format_streaming_status(&state));
        });
    }

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

            let target =
                SenderTarget::new(device.id.clone(), device.display_name.clone(), endpoint);
            let capture_settings = state.borrow().config.audio.capture_settings();
            let sender_name = state.borrow().receiver.advertised_name.clone();

            match sender.lock().expect("sender session mutex").start(
                audio_backend.clone(),
                capture_settings,
                target,
                sender_name,
            ) {
                Ok(()) => {
                    let snapshot = sender.lock().expect("sender session mutex").snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_streaming_snapshot(snapshot.clone());
                    state.diagnostics.push(DiagnosticEvent::info(
                        "streaming",
                        format!(
                            "Queued receiver target {} at {} with local mirror {}.",
                            snapshot
                                .target(&device.id)
                                .map(|target| target.receiver_name.as_str())
                                .unwrap_or(device.display_name.as_str()),
                            device
                                .endpoint
                                .as_ref()
                                .map(|endpoint| endpoint.address.to_string())
                                .unwrap_or_else(|| "unknown endpoint".to_string()),
                            if snapshot.local_mirror.desired_enabled {
                                "enabled"
                            } else {
                                "disabled"
                            }
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
            let selected_device_id = state.borrow().selected_receiver_device_id.clone();
            let Some(device_id) = selected_device_id else {
                let mut state = state.borrow_mut();
                state.diagnostics.push(DiagnosticEvent::warning(
                    "streaming",
                    "Select a receiver target before trying to remove it from the cast.",
                ));
                status_label.set_text(&format_streaming_status(&state));
                return;
            };

            match sender
                .lock()
                .expect("sender session mutex")
                .stop_target(&device_id)
            {
                Ok(()) => {
                    let snapshot = sender.lock().expect("sender session mutex").snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_streaming_snapshot(snapshot);
                    state.diagnostics.push(DiagnosticEvent::info(
                        "streaming",
                        format!(
                            "Removed receiver target {} from the active cast.",
                            device_id
                        ),
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

    {
        let state = Rc::clone(&state);
        let sender = Arc::clone(&sender);
        let status_label = status_label.clone();
        stop_all_button.connect_clicked(move |_| {
            match sender.lock().expect("sender session mutex").stop() {
                Ok(()) => {
                    let snapshot = sender.lock().expect("sender session mutex").snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_streaming_snapshot(snapshot);
                    state.diagnostics.push(DiagnosticEvent::info(
                        "streaming",
                        "Stopped the sender session manager and released all target transports.",
                    ));
                    status_label.set_text(&format_streaming_status(&state));
                }
                Err(error) => {
                    let mut state = state.borrow_mut();
                    state.diagnostics.push(DiagnosticEvent::error(
                        "streaming",
                        format!("Failed to stop all sender targets: {error}"),
                    ));
                    status_label.set_text(&format_streaming_status(&state));
                }
            }
        });
    }

    (
        page,
        status_label,
        receiver_selector,
        mirror_target_selector,
    )
}

fn receiver_page(
    state: Rc<RefCell<AppState>>,
    receiver: Arc<Mutex<ReceiverRuntime>>,
    receiver_transport: Arc<Mutex<LanReceiverTransportServer>>,
) -> (gtk::Box, gtk::Label, gtk::ComboBoxText) {
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

    let receiver_output_selector = gtk::ComboBoxText::new();
    refresh_playback_target_selector(
        &receiver_output_selector,
        &state.borrow(),
        state.borrow().config.receiver.playback_target_id.as_deref(),
        "Receiver output: system default",
    );
    page.append(&receiver_output_selector);

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
        let status_label = status_label.clone();
        receiver_output_selector.connect_changed(move |selector| {
            let target_id = selector
                .active_id()
                .and_then(|id| playback_target_id_from_selection(id.as_str()));
            if state.borrow().config.receiver.playback_target_id == target_id {
                return;
            }

            let result = receiver
                .lock()
                .expect("receiver runtime mutex")
                .set_playback_target(target_id.clone());
            let snapshot = receiver.lock().expect("receiver runtime mutex").snapshot();
            let mut state = state.borrow_mut();
            state.select_receiver_playback_target(target_id.clone());
            state.apply_receiver_snapshot(snapshot);

            match result {
                Ok(()) => {
                    let scope = if matches!(
                        state.receiver.state,
                        synchrosonic_core::ReceiverServiceState::Connected
                            | synchrosonic_core::ReceiverServiceState::Buffering
                            | synchrosonic_core::ReceiverServiceState::Playing
                    ) {
                        "active receiver playback"
                    } else {
                        "next receiver playback session"
                    };
                    let selected_target = format_selected_playback_target(
                        &state,
                        state.config.receiver.playback_target_id.as_deref(),
                    );
                    state.diagnostics.push(DiagnosticEvent::info(
                        "receiver",
                        format!("Receiver output target set to {selected_target} for the {scope}."),
                    ));
                }
                Err(error) => {
                    state.diagnostics.push(DiagnosticEvent::warning(
                        "receiver",
                        format!("Failed to update receiver playback target: {error}"),
                    ));
                }
            }

            status_label.set_text(&format_receiver_status(&state));
        });
    }

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
            let stop_transport = receiver_transport
                .lock()
                .expect("receiver transport mutex")
                .stop();
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
                    state.diagnostics.push(DiagnosticEvent::info(
                        "receiver",
                        "Receiver mode stopped and playback/transport resources were released.",
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

    (page, status_label, receiver_output_selector)
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
                    state
                        .borrow_mut()
                        .diagnostics
                        .push(DiagnosticEvent::warning(
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
            Err(error) => state
                .borrow_mut()
                .diagnostics
                .push(DiagnosticEvent::warning(
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
            let sync_state_changed = state.receiver.sync.state != snapshot.sync.state;
            let late_drop_delta = snapshot
                .sync
                .late_packet_drops
                .saturating_sub(state.receiver.sync.late_packet_drops);
            let sync_state = snapshot.sync.state;
            let sync_buffer_delta_ms = snapshot.sync.buffer_delta_ms;
            let sync_schedule_error_ms = snapshot.sync.schedule_error_ms;
            let sync_expected_latency_ms = snapshot.sync.expected_output_latency_ms;
            let sync_queued_audio_ms = snapshot.sync.queued_audio_ms;
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
                state
                    .diagnostics
                    .push(DiagnosticEvent::info("receiver", message));
            }
            if sync_state_changed {
                let sync_event = match sync_state {
                    synchrosonic_core::ReceiverSyncState::Late
                    | synchrosonic_core::ReceiverSyncState::Recovering => DiagnosticEvent::warning(
                        "receiver-sync",
                        format!(
                            "Receiver sync is now {:?} (buffer delta {} ms, schedule error {} ms).",
                            sync_state, sync_buffer_delta_ms, sync_schedule_error_ms
                        ),
                    ),
                    _ => DiagnosticEvent::info(
                        "receiver-sync",
                        format!(
                            "Receiver sync is now {:?} (expected {} ms, queued {} ms).",
                            sync_state, sync_expected_latency_ms, sync_queued_audio_ms
                        ),
                    ),
                };
                state.diagnostics.push(sync_event);
            }
            if late_drop_delta > 0 {
                state.diagnostics.push(DiagnosticEvent::warning(
                    "receiver-sync",
                    format!("Receiver dropped {late_drop_delta} stale packet(s) to recover sync."),
                ));
            }
            if let Some(message) = error_message {
                state
                    .diagnostics
                    .push(DiagnosticEvent::warning("receiver", message));
            }
            if let Some(error) = &transport_snapshot.last_error {
                if state.diagnostics.last().map(|event| event.message.as_str())
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
        let previous_snapshot = state.borrow().streaming.clone();
        let state_message = if previous_snapshot.state != snapshot.state {
            Some(format!("Sender stream state is now {:?}", snapshot.state))
        } else {
            None
        };
        let local_mirror_state_message =
            if previous_snapshot.local_mirror.state != snapshot.local_mirror.state {
                Some(format!(
                    "Local playback mirror is now {:?}",
                    snapshot.local_mirror.state
                ))
            } else {
                None
            };
        let error_message = if previous_snapshot.last_error != snapshot.last_error {
            snapshot
                .last_error
                .as_ref()
                .map(|error| format!("Sender stream reported: {error}"))
        } else {
            None
        };
        let local_mirror_error_message =
            if previous_snapshot.local_mirror.last_error != snapshot.local_mirror.last_error {
                snapshot
                    .local_mirror
                    .last_error
                    .as_ref()
                    .map(|error| format!("Local playback mirror reported: {error}"))
            } else {
                None
            };
        let target_messages =
            stream_target_transition_messages(&previous_snapshot.targets, &snapshot.targets);

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
            if let Some(message) = local_mirror_state_message {
                state
                    .diagnostics
                    .push(DiagnosticEvent::info("streaming", message));
            }
            if let Some(message) = local_mirror_error_message {
                state
                    .diagnostics
                    .push(DiagnosticEvent::warning("streaming", message));
            }
            for message in target_messages {
                state.diagnostics.push(message);
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

fn start_playback_target_poll(
    audio_backend: LinuxAudioBackend,
    sender: Arc<Mutex<LanSenderSession>>,
    receiver: Arc<Mutex<ReceiverRuntime>>,
    state: Rc<RefCell<AppState>>,
    local_output_selector: gtk::ComboBoxText,
    receiver_output_selector: gtk::ComboBoxText,
    streaming_status_label: gtk::Label,
    receiver_status_label: gtk::Label,
    dashboard_status_label: gtk::Label,
    audio_backend_name: String,
    audio_source_summary: String,
) {
    let mut last_error = None::<String>;

    glib::timeout_add_seconds_local(1, move || {
        match audio_backend.list_playback_targets() {
            Ok(targets) => {
                last_error = None;

                let mut local_retry_target = None::<Option<String>>;
                let mut local_transition_message = None::<DiagnosticEvent>;
                let mut receiver_transition_message = None::<DiagnosticEvent>;

                {
                    let mut state = state.borrow_mut();
                    let targets_changed = state.playback_targets != targets;
                    let previous_local_available = state.local_playback_target_available();
                    let previous_receiver_available = state.receiver_playback_target_available();
                    let previous_local_target_id =
                        state.config.audio.local_playback_target_id.clone();
                    let previous_receiver_target_id =
                        state.config.receiver.playback_target_id.clone();

                    state.set_playback_targets(targets);

                    let local_available = state.local_playback_target_available();
                    let receiver_available = state.receiver_playback_target_available();

                    if previous_local_available != local_available {
                        local_transition_message = Some(playback_target_transition_diagnostic(
                            &state,
                            "streaming",
                            "Local mirror output",
                            previous_local_target_id.as_deref(),
                            local_available,
                        ));
                        if local_available
                            && state.streaming.state != synchrosonic_core::StreamSessionState::Idle
                            && state.streaming.local_mirror.desired_enabled
                            && state.streaming.local_mirror.state
                                == synchrosonic_core::LocalMirrorState::Error
                        {
                            local_retry_target = Some(previous_local_target_id);
                        }
                    }

                    if previous_receiver_available != receiver_available {
                        receiver_transition_message = Some(playback_target_transition_diagnostic(
                            &state,
                            "receiver",
                            "Receiver output",
                            previous_receiver_target_id.as_deref(),
                            receiver_available,
                        ));
                    }

                    if targets_changed
                        || previous_local_available != local_available
                        || previous_receiver_available != receiver_available
                    {
                        refresh_playback_target_selector(
                            &local_output_selector,
                            &state,
                            state.config.audio.local_playback_target_id.as_deref(),
                            "Local mirror output: system default",
                        );
                        refresh_playback_target_selector(
                            &receiver_output_selector,
                            &state,
                            state.config.receiver.playback_target_id.as_deref(),
                            "Receiver output: system default",
                        );
                    }
                }

                if let Some(target_id) = local_retry_target.flatten() {
                    let retry_result = sender
                        .lock()
                        .expect("sender session mutex")
                        .set_local_playback_target(Some(target_id.clone()));
                    let snapshot = sender.lock().expect("sender session mutex").snapshot();
                    let mut state = state.borrow_mut();
                    state.apply_streaming_snapshot(snapshot);
                    match retry_result {
                        Ok(()) => {
                            let selected_target = format_selected_playback_target(
                                &state,
                                state.config.audio.local_playback_target_id.as_deref(),
                            );
                            state.diagnostics.push(DiagnosticEvent::info(
                                "streaming",
                                format!(
                                    "Local mirror output {target_id} is available again; retrying playback on {selected_target}."
                                ),
                            ));
                        }
                        Err(error) => state.diagnostics.push(DiagnosticEvent::warning(
                            "streaming",
                            format!(
                                "Local mirror output became available again, but restart failed: {error}"
                            ),
                        )),
                    }
                }

                let snapshot = receiver.lock().expect("receiver runtime mutex").snapshot();
                {
                    let mut state = state.borrow_mut();
                    state.apply_receiver_snapshot(snapshot);
                    if let Some(message) = local_transition_message {
                        state.diagnostics.push(message);
                    }
                    if let Some(message) = receiver_transition_message {
                        state.diagnostics.push(message);
                    }
                    streaming_status_label.set_text(&format_streaming_status(&state));
                    receiver_status_label.set_text(&format_receiver_status(&state));
                    dashboard_status_label.set_text(&format_dashboard_status(
                        &state,
                        &audio_backend_name,
                        &audio_source_summary,
                    ));
                }
            }
            Err(error) => {
                let message = format!("PipeWire playback target enumeration failed: {error}");
                if last_error.as_deref() != Some(message.as_str()) {
                    state
                        .borrow_mut()
                        .diagnostics
                        .push(DiagnosticEvent::warning("audio", message.clone()));
                }
                last_error = Some(message);
            }
        }

        ControlFlow::Continue
    });
}

fn playback_target_transition_diagnostic(
    state: &AppState,
    component: &str,
    prefix: &str,
    target_id: Option<&str>,
    available: bool,
) -> DiagnosticEvent {
    let message = format!(
        "{prefix} is {}: {}.",
        if available {
            "available"
        } else {
            "currently unavailable"
        },
        format_selected_playback_target(state, target_id)
    );

    if available {
        DiagnosticEvent::info(component, message)
    } else {
        DiagnosticEvent::warning(component, message)
    }
}

fn playback_target_id_from_selection(selection_id: &str) -> Option<String> {
    if selection_id == DEFAULT_PLAYBACK_TARGET_ID {
        None
    } else {
        Some(selection_id.to_string())
    }
}

fn refresh_playback_target_selector(
    selector: &gtk::ComboBoxText,
    state: &AppState,
    selected_target_id: Option<&str>,
    default_label: &str,
) {
    selector.remove_all();
    selector.append(Some(DEFAULT_PLAYBACK_TARGET_ID), default_label);

    for target in &state.playback_targets {
        let label = format_playback_target_option(target);
        selector.append(Some(target.id.as_str()), &label);
    }

    if let Some(selected_target_id) = selected_target_id {
        if state.playback_target(selected_target_id).is_none() {
            selector.append(
                Some(selected_target_id),
                &format!("{selected_target_id} (currently unavailable)"),
            );
        }
        selector.set_active_id(Some(selected_target_id));
    } else {
        selector.set_active_id(Some(DEFAULT_PLAYBACK_TARGET_ID));
    }
}

fn format_playback_target_option(target: &PlaybackTarget) -> String {
    let prefix = match target.kind {
        PlaybackTargetKind::Bluetooth => "Bluetooth",
        PlaybackTargetKind::Standard => "Output",
    };
    let default_suffix = if target.is_default { " [default]" } else { "" };
    format!("{prefix}: {}{default_suffix}", target.display_name)
}

fn format_selected_playback_target(state: &AppState, target_id: Option<&str>) -> String {
    let Some(target_id) = target_id else {
        return SYSTEM_DEFAULT_OUTPUT_LABEL.to_string();
    };

    match state.playback_target(target_id) {
        Some(target) => match target.kind {
            PlaybackTargetKind::Bluetooth => format!(
                "Bluetooth output {}{}",
                target.display_name,
                target
                    .bluetooth_address
                    .as_deref()
                    .map(|address| format!(" ({address})"))
                    .unwrap_or_default()
            ),
            PlaybackTargetKind::Standard => format!("output {}", target.display_name),
        },
        None => format!("output {target_id} (currently unavailable)"),
    }
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
        "SynchroSonic is a Linux-first Rust desktop app for LAN audio streaming. Bluetooth support is modeled as a local playback-output capability on the sender or receiver device, not as a separate transport path.",
    )
}

fn format_dashboard_status(
    state: &AppState,
    audio_backend_name: &str,
    audio_source_summary: &str,
) -> String {
    let healthy_targets = state.streaming.healthy_target_count();
    let bluetooth_outputs = state
        .playback_targets
        .iter()
        .filter(|target| target.is_bluetooth())
        .count();
    format!(
        "Session: {:?}\nCapture: {:?}\nStreaming: {:?}\nSelected receiver: {}\nTarget sessions: total={} healthy={}\nPlayback outputs: total={} bluetooth={}\nLocal mirror output: {} (available={})\nReceiver output: {} (available={})\nLocal mirror: desired={} state={:?} buffer={}% ({} / {} packets, dropped={}) played packets={} bytes={} error={}\nSender aggregate metrics: packets sent={} bytes sent={} bitrate={}bps latency={:?}ms gaps={} keepalives={}/{}\nReceiver: {:?}\nReceiver buffer: {}% ({} / {} packets)\nReceiver metrics: packets in={} played={} underruns={} overruns={} reconnects={}\nAudio backend: {}\n{}\nDefault stream port: {}\nLocal playback default: {}",
        state.cast_session,
        state.capture_state,
        state.streaming.state,
        state
            .selected_receiver_device_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "none".to_string()),
        state.streaming.active_target_count(),
        healthy_targets,
        state.playback_targets.len(),
        bluetooth_outputs,
        format_selected_playback_target(state, state.config.audio.local_playback_target_id.as_deref()),
        state.local_playback_target_available(),
        format_selected_playback_target(state, state.config.receiver.playback_target_id.as_deref()),
        state.receiver_playback_target_available(),
        state.streaming.local_mirror.desired_enabled,
        state.streaming.local_mirror.state,
        state.streaming.local_mirror.buffer.fill_percent(),
        state.streaming.local_mirror.buffer.queued_packets,
        state.streaming.local_mirror.buffer.max_packets,
        state.streaming.local_mirror.buffer.dropped_packets,
        state.streaming.local_mirror.packets_played,
        state.streaming.local_mirror.bytes_played,
        state
            .streaming
            .local_mirror
            .last_error
            .as_deref()
            .unwrap_or("none"),
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
    let local_mirror = &stream.local_mirror;
    let selected_device = state
        .selected_receiver_device_id
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "none".to_string());
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
    let target_status = format_stream_target_statuses(&stream.targets);

    format!(
        "State: {:?}\nSelected receiver id: {}\nSender session id: {}\nStream: {}\nActive target count: {}\nTargets:\n{}\nLocal mirror output: {} (available={})\nLocal mirror: desired={} state={:?} backend={} target={} buffer={}% ({} / {} packets, dropped={}) played packets={} bytes={} error={}\nAggregate metrics: packets sent={} packets received={} bytes sent={} bytes received={} bitrate={}bps latency estimate={:?}ms packet gaps={} keepalives sent={} keepalives received={}\nLast error: {}\nSplit-stream path: PipeWire capture -> explicit branch fan-out -> [per-target bounded network queue -> per-target TCP framed transport -> receiver runtime -> PipeWire playback] + [bounded local mirror queue -> sender-side PipeWire playback].",
        stream.state,
        selected_device,
        stream.session_id.as_deref().unwrap_or("not started"),
        stream_shape,
        stream.active_target_count(),
        target_status,
        format_selected_playback_target(state, state.config.audio.local_playback_target_id.as_deref()),
        state.local_playback_target_available(),
        local_mirror.desired_enabled,
        local_mirror.state,
        local_mirror
            .playback_backend
            .as_deref()
            .unwrap_or("not configured"),
        format_selected_playback_target(state, local_mirror.playback_target_id.as_deref()),
        local_mirror.buffer.fill_percent(),
        local_mirror.buffer.queued_packets,
        local_mirror.buffer.max_packets,
        local_mirror.buffer.dropped_packets,
        local_mirror.packets_played,
        local_mirror.bytes_played,
        local_mirror.last_error.as_deref().unwrap_or("none"),
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

fn format_stream_target_statuses(targets: &[StreamTargetSnapshot]) -> String {
    if targets.is_empty() {
        return "none".to_string();
    }

    targets
        .iter()
        .map(|target| {
            format!(
                "- {} [{}] {:?} {:?} endpoint={} session={} latency={:?}ms buffer={}% ({} / {} packets, dropped={}) bitrate={}bps bytes={} last_error={}",
                target.receiver_name,
                target.receiver_id,
                target.state,
                target.health,
                target.endpoint,
                target.session_id.as_deref().unwrap_or("not negotiated"),
                target.metrics.latency_estimate_ms,
                target.network_buffer.fill_percent(),
                target.network_buffer.queued_packets,
                target.network_buffer.max_packets,
                target.network_buffer.dropped_packets,
                target.metrics.estimated_bitrate_bps,
                target.metrics.bytes_sent,
                target.last_error.as_deref().unwrap_or("none"),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn stream_target_transition_messages(
    previous: &[StreamTargetSnapshot],
    next: &[StreamTargetSnapshot],
) -> Vec<DiagnosticEvent> {
    let previous_by_id = previous
        .iter()
        .map(|target| (target.receiver_id.clone(), target))
        .collect::<HashMap<_, _>>();
    let next_by_id = next
        .iter()
        .map(|target| (target.receiver_id.clone(), target))
        .collect::<HashMap<_, _>>();
    let mut messages = Vec::new();

    for target in next {
        match previous_by_id.get(&target.receiver_id) {
            None => messages.push(DiagnosticEvent::info(
                "streaming",
                format!(
                    "Target {} joined the cast collection with state {:?}.",
                    target.receiver_name, target.state
                ),
            )),
            Some(previous_target) => {
                if previous_target.state != target.state || previous_target.health != target.health
                {
                    messages.push(DiagnosticEvent::info(
                        "streaming",
                        format!(
                            "Target {} is now {:?} with {:?} health.",
                            target.receiver_name, target.state, target.health
                        ),
                    ));
                }
                if previous_target.last_error != target.last_error {
                    if let Some(error) = &target.last_error {
                        messages.push(DiagnosticEvent::warning(
                            "streaming",
                            format!("Target {} reported: {error}", target.receiver_name),
                        ));
                    }
                }
            }
        }
    }

    for previous_target in previous {
        if !next_by_id.contains_key(&previous_target.receiver_id) {
            messages.push(DiagnosticEvent::info(
                "streaming",
                format!(
                    "Target {} was removed from the cast collection.",
                    previous_target.receiver_name
                ),
            ));
        }
    }

    messages
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
    format!(
        "{}:{}",
        config.receiver.bind_host, config.receiver.listen_port
    )
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
                "session={} remote={} stream={}Hz/{}ch/{:?}/{}fpp requested={}ms",
                connection.session_id,
                connection
                    .remote_addr
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|| "unresolved".to_string()),
                connection.stream.sample_rate_hz,
                connection.stream.channels,
                connection.stream.sample_format,
                connection.stream.frames_per_packet,
                connection.requested_latency_ms
            )
        })
        .unwrap_or_else(|| "none".to_string());
    let last_error = receiver.last_error.as_deref().unwrap_or("none");

    format!(
        "State: {:?}\nAdvertised name: {}\nListen address: {}:{}\nLatency preset: {:?}\nPlayback backend: {}\nPlayback target: {} (available={})\nConnection: {}\nBuffer: {} packet(s), {} frame(s), {} ms queued, target {} ms, max {} ms, {}% full\nSync: state={:?} expected={} ms requested={} queued={} ms buffer delta={} ms schedule error={} ms late drops={} resets={} last sender ts={} last sender unix={}\nMetrics: packets in={} frames in={} bytes in={} packets out={} frames out={} bytes out={} underruns={} overruns={} reconnect attempts={}\nLast error: {}\nInternal app flow: use the Start/Stop buttons here; the TCP receiver listener already submits Connected, AudioPacket, KeepAlive, and Disconnected events into the runtime.",
        receiver.state,
        receiver.advertised_name,
        receiver.bind_host,
        receiver.listen_port,
        receiver.latency_preset,
        receiver
            .playback_backend
            .as_deref()
            .unwrap_or("not configured"),
        format_selected_playback_target(state, receiver.playback_target_id.as_deref()),
        state.receiver_playback_target_available(),
        connection,
        receiver.buffer.queued_packets,
        receiver.buffer.queued_frames,
        receiver.buffer.queued_audio_ms,
        receiver.buffer.target_buffer_ms,
        receiver.buffer.max_buffer_ms,
        receiver.buffer.fill_percent(),
        receiver.sync.state,
        receiver.sync.expected_output_latency_ms,
        receiver
            .sync
            .requested_latency_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        receiver.sync.queued_audio_ms,
        receiver.sync.buffer_delta_ms,
        receiver.sync.schedule_error_ms,
        receiver.sync.late_packet_drops,
        receiver.sync.sync_resets,
        receiver
            .sync
            .last_sender_timestamp_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        receiver
            .sync
            .last_sender_capture_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
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
