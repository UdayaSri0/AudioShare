use adw::prelude::*;
use gtk::{Align, Orientation};
use synchrosonic_audio::LinuxAudioBackend;
use synchrosonic_core::{
    services::{AudioBackend, DiscoveryService},
    AppConfig, AppState, AudioSourceKind, DiagnosticEvent,
};
use synchrosonic_discovery::MdnsDiscoveryService;
use synchrosonic_receiver::ReceiverRuntime;
use synchrosonic_transport::LanTransportService;

pub fn build_main_window(app: &adw::Application) {
    let config = AppConfig::default();
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
    let discovery = MdnsDiscoveryService::new(
        config.discovery.clone(),
        config.receiver.advertised_name.clone(),
    );
    let transport = LanTransportService::new(None);
    let receiver = ReceiverRuntime::new(config.receiver.clone());

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

    stack.add_titled(
        &dashboard_page(&state, audio_backend.backend_name(), &audio_source_summary),
        Some("dashboard"),
        "Dashboard",
    );
    stack.add_titled(
        &status_page("Devices", "LAN discovery is not active yet."),
        Some("devices"),
        "Devices",
    );
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
        receiver_state = ?receiver.state(),
        transport_state = ?transport.state(),
        "SynchroSonic scaffold window activated"
    );

    window.set_content(Some(&root));
    window.present();
}

fn dashboard_page(
    state: &AppState,
    audio_backend_name: &str,
    audio_source_summary: &str,
) -> gtk::Box {
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

    let status = gtk::Label::new(Some(&format!(
        "Session: {:?}\nCapture: {:?}\nAudio backend: {}\n{}\nDefault stream port: {}\nLocal playback default: {}",
        state.cast_session,
        state.capture_state,
        audio_backend_name,
        audio_source_summary,
        state.config.transport.stream_port,
        if state.config.audio.local_playback_enabled {
            "enabled"
        } else {
            "disabled"
        }
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

    page
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

fn about_page() -> gtk::Box {
    status_page(
        "About",
        "SynchroSonic is a Linux-first Rust desktop app for LAN audio streaming. Bluetooth is deferred and will be designed as a later output/backend capability.",
    )
}
