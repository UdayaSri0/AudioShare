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
    AppConfig, AppState, AudioSource, AudioSourceKind, ConfigLoadReport, DeviceId, DiagnosticEvent,
    DiagnosticLevel, DiscoveredDevice, PlaybackTarget, PlaybackTargetKind, QualityPreset,
    ReceiverLatencyPreset, StreamTargetSnapshot,
};
use synchrosonic_discovery::MdnsDiscoveryService;
use synchrosonic_receiver::ReceiverRuntime;
use synchrosonic_transport::{LanReceiverTransportServer, LanSenderSession, SenderTarget};

use crate::{
    logging::{LogStore, StructuredLogEntry},
    metadata,
    persistence::{export_config, import_config, save_active_config, AppPaths},
};

const DEFAULT_PLAYBACK_TARGET_ID: &str = "__system_default_playback__";
const SYSTEM_DEFAULT_OUTPUT_LABEL: &str = "System default output";
const QUALITY_LOW_LATENCY_ID: &str = "quality_low_latency";
const QUALITY_BALANCED_ID: &str = "quality_balanced";
const QUALITY_HIGH_QUALITY_ID: &str = "quality_high_quality";
const LATENCY_LOW_ID: &str = "latency_low";
const LATENCY_BALANCED_ID: &str = "latency_balanced";
const LATENCY_STABLE_ID: &str = "latency_stable";

#[derive(Clone)]
pub struct UiLaunchContext {
    pub config: AppConfig,
    pub startup_diagnostics: Vec<DiagnosticEvent>,
    pub paths: AppPaths,
    pub log_store: LogStore,
}

#[derive(Clone)]
struct UiContext {
    audio_backend_name: String,
    discovery_service_type: String,
    discovery_started: bool,
}

#[derive(Clone)]
struct UiController {
    state: Rc<RefCell<AppState>>,
    audio_backend: LinuxAudioBackend,
    sender: Arc<Mutex<LanSenderSession>>,
    receiver: Arc<Mutex<ReceiverRuntime>>,
    receiver_transport: Arc<Mutex<LanReceiverTransportServer>>,
    navigation: adw::ViewStack,
    widgets: UiWidgets,
    context: UiContext,
    paths: AppPaths,
    log_store: LogStore,
}

#[derive(Clone)]
struct UiWidgets {
    dashboard: DashboardWidgets,
    discovery: DiscoveryWidgets,
    casting: CastingWidgets,
    audio: AudioWidgets,
    receiver: ReceiverWidgets,
    diagnostics: DiagnosticsWidgets,
    settings: SettingsWidgets,
}

#[derive(Clone)]
struct DashboardWidgets {
    session_row: adw::ActionRow,
    discovery_row: adw::ActionRow,
    casting_row: adw::ActionRow,
    audio_row: adw::ActionRow,
    detail_label: gtk::Label,
    open_devices_button: gtk::Button,
    open_casting_button: gtk::Button,
    open_audio_button: gtk::Button,
    open_receiver_button: gtk::Button,
}

#[derive(Clone)]
struct DiscoveryWidgets {
    summary_row: adw::ActionRow,
    device_list: gtk::ListBox,
    empty_state: adw::StatusPage,
}

#[derive(Clone)]
struct CastingWidgets {
    receiver_selector: gtk::ComboBoxText,
    add_button: gtk::Button,
    stop_all_button: gtk::Button,
    local_mirror_switch: gtk::Switch,
    local_output_selector: gtk::ComboBoxText,
    session_row: adw::ActionRow,
    mirror_row: adw::ActionRow,
    target_list: gtk::ListBox,
    empty_state: adw::StatusPage,
    detail_label: gtk::Label,
}

#[derive(Clone)]
struct AudioWidgets {
    source_selector: gtk::ComboBoxText,
    current_source_row: adw::ActionRow,
    output_row: adw::ActionRow,
    source_list: gtk::ListBox,
    output_list: gtk::ListBox,
    sources_empty_state: adw::StatusPage,
    outputs_empty_state: adw::StatusPage,
}

#[derive(Clone)]
struct ReceiverWidgets {
    latency_selector: gtk::ComboBoxText,
    output_selector: gtk::ComboBoxText,
    start_button: gtk::Button,
    stop_button: gtk::Button,
    state_row: adw::ActionRow,
    connection_row: adw::ActionRow,
    sync_row: adw::ActionRow,
    detail_label: gtk::Label,
}

#[derive(Clone)]
struct DiagnosticsWidgets {
    summary_row: adw::ActionRow,
    log_summary_row: adw::ActionRow,
    clear_button: gtk::Button,
    list_box: gtk::ListBox,
    empty_state: adw::StatusPage,
    log_list_box: gtk::ListBox,
    log_empty_state: adw::StatusPage,
}

#[derive(Clone)]
struct SettingsWidgets {
    quality_selector: gtk::ComboBoxText,
    theme_switch: gtk::Switch,
    verbose_logging_switch: gtk::Switch,
    receiver_start_on_launch_switch: gtk::Switch,
    summary_row: adw::ActionRow,
    note_row: adw::ActionRow,
    config_dir_row: adw::ActionRow,
    state_dir_row: adw::ActionRow,
    config_path_row: adw::ActionRow,
    portable_config_row: adw::ActionRow,
    log_path_row: adw::ActionRow,
    import_button: gtk::Button,
    export_button: gtk::Button,
}

pub fn build_main_window(app: &adw::Application, launch: UiLaunchContext) {
    let config = launch.config.clone();
    apply_color_scheme(config.ui.prefer_dark_theme);

    let audio_backend = LinuxAudioBackend::new();
    let mut state = AppState::new(config.clone());
    state.diagnostics.extend(launch.startup_diagnostics.clone());
    refresh_audio_inventory(&audio_backend, &mut state);

    let mut discovery = MdnsDiscoveryService::new(
        config.discovery.clone(),
        config.receiver.advertised_name.clone(),
    );
    let discovery_started = match discovery.start() {
        Ok(()) => true,
        Err(error) => {
            state.diagnostics.push(DiagnosticEvent::warning(
                "discovery",
                format!("mDNS discovery failed to start: {error}"),
            ));
            false
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
    {
        let mut sender = sender.lock().expect("sender session mutex");
        let _ = sender.set_local_playback_target(config.audio.local_playback_target_id.clone());
        let _ = sender.set_quality_preset(config.transport.quality);
    }
    state.apply_receiver_snapshot(receiver.lock().expect("receiver runtime mutex").snapshot());
    state.apply_streaming_snapshot(sender.lock().expect("sender session mutex").snapshot());
    let state = Rc::new(RefCell::new(state));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(metadata::APP_NAME)
        .default_width(1240)
        .default_height(860)
        .build();

    let header = gtk::HeaderBar::new();
    let brand = gtk::Box::new(Orientation::Vertical, 0);
    let brand_title = gtk::Label::new(Some(metadata::APP_NAME));
    brand_title.add_css_class("title-4");
    brand_title.set_halign(Align::Start);
    let brand_subtitle = gtk::Label::new(Some("Linux-native LAN audio casting"));
    brand_subtitle.add_css_class("dim-label");
    brand_subtitle.set_halign(Align::Start);
    brand.append(&brand_title);
    brand.append(&brand_subtitle);
    header.pack_start(&brand);

    let navigation = adw::ViewStack::new();
    navigation.set_hexpand(true);
    navigation.set_vexpand(true);

    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(&navigation));
    header.set_title_widget(Some(&switcher));
    let switcher_bar = adw::ViewSwitcherBar::new();
    switcher_bar.set_stack(Some(&navigation));
    switcher_bar.set_reveal(true);

    let root = gtk::Box::new(Orientation::Vertical, 0);
    root.append(&header);
    root.append(&navigation);
    root.append(&switcher_bar);

    let (dashboard_page, dashboard) = dashboard_page();
    let dashboard_stack_page =
        navigation.add_titled(&dashboard_page, Some("dashboard"), "Dashboard");
    dashboard_stack_page.set_icon_name(Some("go-home-symbolic"));
    let (discovery_page, discovery_widgets) = discovery_page();
    let discovery_stack_page = navigation.add_titled(&discovery_page, Some("devices"), "Devices");
    discovery_stack_page.set_icon_name(Some("network-workgroup-symbolic"));
    let (casting_page, casting_widgets) = casting_page();
    let casting_stack_page = navigation.add_titled(&casting_page, Some("casting"), "Casting");
    casting_stack_page.set_icon_name(Some("media-playback-start-symbolic"));
    let (audio_page, audio_widgets) = audio_page();
    let audio_stack_page = navigation.add_titled(&audio_page, Some("audio"), "Audio");
    audio_stack_page.set_icon_name(Some("audio-card-symbolic"));
    let (receiver_page, receiver_widgets) = receiver_page();
    let receiver_stack_page = navigation.add_titled(&receiver_page, Some("receiver"), "Receiver");
    receiver_stack_page.set_icon_name(Some("audio-speakers-symbolic"));
    let (diagnostics_page, diagnostics_widgets) = diagnostics_page();
    let diagnostics_stack_page =
        navigation.add_titled(&diagnostics_page, Some("diagnostics"), "Diagnostics");
    diagnostics_stack_page.set_icon_name(Some("utilities-terminal-symbolic"));
    let (settings_page, settings_widgets) = settings_page();
    let settings_stack_page = navigation.add_titled(&settings_page, Some("settings"), "Settings");
    settings_stack_page.set_icon_name(Some("emblem-system-symbolic"));
    let about_page = about_page();
    let about_stack_page = navigation.add_titled(&about_page, Some("about"), "About");
    about_stack_page.set_icon_name(Some("help-about-symbolic"));

    let widgets = UiWidgets {
        dashboard,
        discovery: discovery_widgets,
        casting: casting_widgets,
        audio: audio_widgets,
        receiver: receiver_widgets,
        diagnostics: diagnostics_widgets,
        settings: settings_widgets,
    };

    let controller = UiController {
        state: Rc::clone(&state),
        audio_backend: audio_backend.clone(),
        sender: Arc::clone(&sender),
        receiver: Arc::clone(&receiver),
        receiver_transport: Arc::clone(&receiver_transport),
        navigation: navigation.clone(),
        widgets,
        context: UiContext {
            audio_backend_name: audio_backend.backend_name().to_string(),
            discovery_service_type: discovery.service_type().to_string(),
            discovery_started,
        },
        paths: launch.paths.clone(),
        log_store: launch.log_store.clone(),
    };

    refresh_ui(&controller);
    connect_navigation_buttons(&controller);
    connect_navigation_persistence(&controller);
    connect_casting_controls(&controller);
    connect_audio_controls(&controller);
    connect_receiver_controls(&controller);
    connect_settings_controls(&controller);
    connect_diagnostics_controls(&controller);

    restore_visible_page(&controller.navigation, &config.ui.last_view_name);

    tracing::info!(
        audio_backend = audio_backend.backend_name(),
        discovery_service = discovery.service_type(),
        receiver_state = ?receiver.lock().expect("receiver runtime mutex").state(),
        sender_state = ?sender.lock().expect("sender session mutex").snapshot().state,
        "SynchroSonic window activated"
    );

    start_discovery_poll(discovery, controller.clone());
    start_receiver_poll(controller.clone());
    start_streaming_poll(controller.clone());
    start_audio_inventory_poll(controller.clone());

    if config.receiver.start_on_launch {
        start_receiver_mode(&controller);
    }

    window.set_content(Some(&root));
    window.present();
}

fn connect_navigation_buttons(controller: &UiController) {
    connect_navigation_button(
        &controller.widgets.dashboard.open_devices_button,
        &controller.navigation,
        "devices",
    );
    connect_navigation_button(
        &controller.widgets.dashboard.open_casting_button,
        &controller.navigation,
        "casting",
    );
    connect_navigation_button(
        &controller.widgets.dashboard.open_audio_button,
        &controller.navigation,
        "audio",
    );
    connect_navigation_button(
        &controller.widgets.dashboard.open_receiver_button,
        &controller.navigation,
        "receiver",
    );
}

fn connect_navigation_button(
    button: &gtk::Button,
    navigation: &adw::ViewStack,
    page: &'static str,
) {
    let navigation = navigation.clone();
    button.connect_clicked(move |_| {
        navigation.set_visible_child_name(page);
    });
}

fn connect_navigation_persistence(controller: &UiController) {
    let controller = controller.clone();
    let navigation = controller.navigation.clone();
    navigation.connect_visible_child_name_notify(move |stack| {
        let Some(visible_child_name) = stack.visible_child_name() else {
            return;
        };
        if controller.state.borrow().config.ui.last_view_name == visible_child_name {
            return;
        }

        controller
            .state
            .borrow_mut()
            .set_last_view_name(visible_child_name.as_str());
        persist_current_config(&controller, "settings", None);
    });
}

fn restore_visible_page(navigation: &adw::ViewStack, page_name: &str) {
    if is_known_view_name(page_name) {
        navigation.set_visible_child_name(page_name);
    } else {
        navigation.set_visible_child_name("dashboard");
    }
}

fn is_known_view_name(page_name: &str) -> bool {
    matches!(
        page_name,
        "dashboard"
            | "devices"
            | "casting"
            | "audio"
            | "receiver"
            | "diagnostics"
            | "settings"
            | "about"
    )
}

fn persist_current_config(
    controller: &UiController,
    component: &str,
    success_message: Option<String>,
) {
    let config = controller.state.borrow().config.clone();
    match save_active_config(&controller.paths, &config) {
        Ok(path) => {
            if let Some(success_message) = success_message {
                controller
                    .state
                    .borrow_mut()
                    .diagnostics
                    .push(DiagnosticEvent::info(
                        component,
                        format!("{success_message} Saved to {}.", path.display()),
                    ));
            }
        }
        Err(error) => controller
            .state
            .borrow_mut()
            .diagnostics
            .push(DiagnosticEvent::warning(
                component,
                format!(
                    "Configuration could not be saved to {}: {error}",
                    controller.paths.config_path.display()
                ),
            )),
    }
}

fn connect_casting_controls(controller: &UiController) {
    {
        let controller = controller.clone();
        let receiver_selector = controller.widgets.casting.receiver_selector.clone();
        receiver_selector.connect_changed(move |selector| {
            if let Some(active_id) = selector.active_id() {
                let _ = controller
                    .state
                    .borrow_mut()
                    .select_receiver_device(DeviceId::new(active_id.as_str()));
                refresh_ui(&controller);
            }
        });
    }

    {
        let controller = controller.clone();
        let local_mirror_switch = controller.widgets.casting.local_mirror_switch.clone();
        local_mirror_switch.connect_active_notify(move |toggle| {
            let enabled = toggle.is_active();
            let (result, snapshot) = {
                let sender = controller.sender.lock().expect("sender session mutex");
                (
                    sender.set_local_playback_enabled(enabled),
                    sender.snapshot(),
                )
            };

            {
                let mut state = controller.state.borrow_mut();
                state.set_local_playback_enabled(enabled);
                state.apply_streaming_snapshot(snapshot);
                match result {
                    Ok(()) => {
                        let scope = if state.streaming.state
                            == synchrosonic_core::StreamSessionState::Idle
                        {
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
                    Err(error) => state.diagnostics.push(DiagnosticEvent::error(
                        "streaming",
                        format!("Failed to update local playback mirror: {error}"),
                    )),
                }
            }

            persist_current_config(&controller, "streaming", None);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let local_output_selector = controller.widgets.casting.local_output_selector.clone();
        local_output_selector.connect_changed(move |selector| {
            let target_id = selector
                .active_id()
                .and_then(|id| playback_target_id_from_selection(id.as_str()));
            if controller
                .state
                .borrow()
                .config
                .audio
                .local_playback_target_id
                == target_id
            {
                return;
            }

            let (result, snapshot) = {
                let mut sender = controller.sender.lock().expect("sender session mutex");
                let result = sender.set_local_playback_target(target_id.clone());
                let snapshot = sender.snapshot();
                (result, snapshot)
            };

            {
                let mut state = controller.state.borrow_mut();
                state.select_local_playback_target(target_id);
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
                    Err(error) => state.diagnostics.push(DiagnosticEvent::warning(
                        "streaming",
                        format!("Failed to update local mirror output target: {error}"),
                    )),
                }
            }

            persist_current_config(&controller, "streaming", None);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let add_button = controller.widgets.casting.add_button.clone();
        add_button.connect_clicked(move |_| queue_selected_receiver_for_cast(&controller));
    }

    {
        let controller = controller.clone();
        let stop_all_button = controller.widgets.casting.stop_all_button.clone();
        stop_all_button.connect_clicked(move |_| stop_all_casting(&controller));
    }
}

fn connect_audio_controls(controller: &UiController) {
    let controller = controller.clone();
    let source_selector = controller.widgets.audio.source_selector.clone();
    source_selector.connect_changed(move |selector| {
        let Some(active_id) = selector.active_id() else {
            return;
        };

        if controller
            .state
            .borrow()
            .selected_audio_source_id
            .as_deref()
            == Some(active_id.as_str())
        {
            return;
        }

        let changed = controller
            .state
            .borrow_mut()
            .select_audio_source(active_id.as_str());

        if !changed {
            return;
        }

        {
            let mut state = controller.state.borrow_mut();
            let selected_source = format_selected_audio_source(&state);
            let scope = if state.streaming.state == synchrosonic_core::StreamSessionState::Idle {
                "next cast session"
            } else {
                "a future cast after the current session stops"
            };
            state.diagnostics.push(DiagnosticEvent::info(
                "audio",
                format!("Capture source set to {selected_source} for {scope}."),
            ));
        }

        persist_current_config(&controller, "audio", None);
        refresh_ui(&controller);
    });
}

fn connect_receiver_controls(controller: &UiController) {
    {
        let controller = controller.clone();
        let output_selector = controller.widgets.receiver.output_selector.clone();
        output_selector.connect_changed(move |selector| {
            let target_id = selector
                .active_id()
                .and_then(|id| playback_target_id_from_selection(id.as_str()));
            if controller.state.borrow().config.receiver.playback_target_id == target_id {
                return;
            }

            let (result, snapshot) = {
                let mut receiver = controller.receiver.lock().expect("receiver runtime mutex");
                let result = receiver.set_playback_target(target_id.clone());
                let snapshot = receiver.snapshot();
                (result, snapshot)
            };

            {
                let mut state = controller.state.borrow_mut();
                state.select_receiver_playback_target(target_id);
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
                            format!(
                                "Receiver output target set to {selected_target} for the {scope}."
                            ),
                        ));
                    }
                    Err(error) => state.diagnostics.push(DiagnosticEvent::warning(
                        "receiver",
                        format!("Failed to update receiver playback target: {error}"),
                    )),
                }
            }

            persist_current_config(&controller, "receiver", None);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let latency_selector = controller.widgets.receiver.latency_selector.clone();
        latency_selector.connect_changed(move |selector| {
            let Some(active_id) = selector.active_id() else {
                return;
            };
            let Some(preset) = latency_preset_from_id(active_id.as_str()) else {
                return;
            };
            if controller.state.borrow().config.receiver.latency_preset == preset {
                return;
            }

            let result = controller
                .receiver
                .lock()
                .expect("receiver runtime mutex")
                .set_latency_preset(preset);
            let snapshot = controller
                .receiver
                .lock()
                .expect("receiver runtime mutex")
                .snapshot();

            {
                let mut state = controller.state.borrow_mut();
                state.set_receiver_latency_preset(preset);
                state.apply_receiver_snapshot(snapshot);
                match result {
                    Ok(()) => state.diagnostics.push(DiagnosticEvent::info(
                        "receiver",
                        format!(
                            "Receiver latency preset set to {}.",
                            format_latency_preset(preset)
                        ),
                    )),
                    Err(error) => state.diagnostics.push(DiagnosticEvent::warning(
                        "receiver",
                        format!("Failed to change receiver latency preset: {error}"),
                    )),
                }
            }

            persist_current_config(&controller, "receiver", None);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let start_button = controller.widgets.receiver.start_button.clone();
        start_button.connect_clicked(move |_| start_receiver_mode(&controller));
    }

    {
        let controller = controller.clone();
        let stop_button = controller.widgets.receiver.stop_button.clone();
        stop_button.connect_clicked(move |_| stop_receiver_mode(&controller));
    }
}

fn connect_settings_controls(controller: &UiController) {
    {
        let controller = controller.clone();
        let quality_selector = controller.widgets.settings.quality_selector.clone();
        quality_selector.connect_changed(move |selector| {
            let Some(active_id) = selector.active_id() else {
                return;
            };
            let Some(preset) = quality_preset_from_id(active_id.as_str()) else {
                return;
            };
            if controller.state.borrow().config.transport.quality == preset {
                return;
            }

            let result = controller
                .sender
                .lock()
                .expect("sender session mutex")
                .set_quality_preset(preset);
            {
                let mut state = controller.state.borrow_mut();
                state.set_transport_quality(preset);
                match result {
                    Ok(()) => {
                        let scope = if state.streaming.state
                            == synchrosonic_core::StreamSessionState::Idle
                        {
                            "next cast session"
                        } else {
                            "newly added targets and future cast sessions"
                        };
                        state.diagnostics.push(DiagnosticEvent::info(
                            "settings",
                            format!(
                                "Sender quality preset set to {} for {scope}.",
                                format_quality_preset(preset)
                            ),
                        ));
                    }
                    Err(error) => state.diagnostics.push(DiagnosticEvent::warning(
                        "settings",
                        format!("Failed to change sender quality preset: {error}"),
                    )),
                }
            }

            persist_current_config(&controller, "settings", None);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let theme_switch = controller.widgets.settings.theme_switch.clone();
        theme_switch.connect_active_notify(move |toggle| {
            let prefer_dark_theme = toggle.is_active();
            apply_color_scheme(prefer_dark_theme);
            {
                let mut state = controller.state.borrow_mut();
                state.set_prefer_dark_theme(prefer_dark_theme);
                state.diagnostics.push(DiagnosticEvent::info(
                    "settings",
                    format!(
                        "Theme preference set to {}.",
                        if prefer_dark_theme {
                            "prefer dark"
                        } else {
                            "follow system default"
                        }
                    ),
                ));
            }
            persist_current_config(&controller, "settings", None);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let verbose_logging_switch = controller.widgets.settings.verbose_logging_switch.clone();
        verbose_logging_switch.connect_active_notify(move |toggle| {
            let verbose_logging = toggle.is_active();
            {
                let mut state = controller.state.borrow_mut();
                state.set_verbose_logging(verbose_logging);
                state.diagnostics.push(DiagnosticEvent::info(
                    "settings",
                    format!(
                        "Verbose logging was set to {} and will apply on the next app launch.",
                        if verbose_logging {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                ));
            }
            persist_current_config(&controller, "settings", None);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let receiver_start_on_launch_switch = controller
            .widgets
            .settings
            .receiver_start_on_launch_switch
            .clone();
        receiver_start_on_launch_switch.connect_active_notify(move |toggle| {
            let start_on_launch = toggle.is_active();
            {
                let mut state = controller.state.borrow_mut();
                state.set_receiver_start_on_launch(start_on_launch);
                state.diagnostics.push(DiagnosticEvent::info(
                    "settings",
                    format!(
                        "Receiver auto-start on launch is now {}.",
                        if start_on_launch {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                ));
            }
            persist_current_config(&controller, "settings", None);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let export_button = controller.widgets.settings.export_button.clone();
        export_button.connect_clicked(move |_| {
            let config = controller.state.borrow().config.clone();
            let message = match export_config(&controller.paths, &config) {
                Ok(path) => DiagnosticEvent::info(
                    "settings",
                    format!(
                        "Exported a portable configuration snapshot to {}.",
                        path.display()
                    ),
                ),
                Err(error) => DiagnosticEvent::warning(
                    "settings",
                    format!(
                        "Portable configuration export to {} failed: {error}",
                        controller.paths.portable_config_path.display()
                    ),
                ),
            };
            controller.state.borrow_mut().diagnostics.push(message);
            refresh_ui(&controller);
        });
    }

    {
        let controller = controller.clone();
        let import_button = controller.widgets.settings.import_button.clone();
        import_button.connect_clicked(move |_| match import_config(&controller.paths) {
            Ok(report) => apply_imported_config(&controller, report),
            Err(error) => {
                controller
                    .state
                    .borrow_mut()
                    .diagnostics
                    .push(DiagnosticEvent::warning(
                        "settings",
                        format!(
                            "Portable configuration import from {} failed: {error}",
                            controller.paths.portable_config_path.display()
                        ),
                    ));
                refresh_ui(&controller);
            }
        });
    }
}

fn connect_diagnostics_controls(controller: &UiController) {
    let controller = controller.clone();
    let clear_button = controller.widgets.diagnostics.clear_button.clone();
    clear_button.connect_clicked(move |_| {
        controller.state.borrow_mut().clear_diagnostics();
        refresh_ui(&controller);
    });
}

fn refresh_ui(controller: &UiController) {
    let state = controller.state.borrow();
    refresh_dashboard(controller, &state);
    refresh_discovery_page(controller, &state);
    refresh_casting_page(controller, &state);
    refresh_audio_page(controller, &state);
    refresh_receiver_page(controller, &state);
    refresh_diagnostics_page(controller, &state);
    refresh_settings_page(controller, &state);
}

fn refresh_dashboard(controller: &UiController, state: &AppState) {
    let receiver_capable = state
        .discovered_devices
        .iter()
        .filter(|device| device.capabilities.supports_receiver)
        .count();
    let bluetooth_outputs = state
        .playback_targets
        .iter()
        .filter(|target| target.is_bluetooth())
        .count();

    controller
        .widgets
        .dashboard
        .session_row
        .set_subtitle(&format!(
            "Cast session {:?}, {} active target(s), local mirror {:?}.",
            state.cast_session,
            state.streaming.active_target_count(),
            state.streaming.local_mirror.state
        ));
    controller
        .widgets
        .dashboard
        .discovery_row
        .set_subtitle(&format!(
            "{} device(s) discovered, {} receiver-capable, discovery {} on {}.",
            state.discovered_devices.len(),
            receiver_capable,
            if controller.context.discovery_started {
                "running"
            } else {
                "failed"
            },
            controller.context.discovery_service_type
        ));
    controller
        .widgets
        .dashboard
        .casting_row
        .set_subtitle(&format!(
            "Selected receiver: {}. Healthy targets: {} of {}.",
            selected_receiver_label(state),
            state.streaming.healthy_target_count(),
            state.streaming.active_target_count()
        ));
    controller
        .widgets
        .dashboard
        .audio_row
        .set_subtitle(&format!(
            "Capture source: {}. Playback outputs: {} total, {} Bluetooth.",
            format_selected_audio_source(state),
            state.playback_targets.len(),
            bluetooth_outputs
        ));
    controller
        .widgets
        .dashboard
        .detail_label
        .set_text(&format_dashboard_status(
            state,
            &controller.context.audio_backend_name,
        ));
}

fn refresh_discovery_page(controller: &UiController, state: &AppState) {
    let summary = if controller.context.discovery_started {
        format!(
            "Listening for {} advertisements. Receiver-capable devices can be queued directly from this page.",
            controller.context.discovery_service_type
        )
    } else {
        "Discovery startup failed earlier. Existing cached devices can still be shown, and the diagnostics page has the startup error.".to_string()
    };
    controller
        .widgets
        .discovery
        .summary_row
        .set_subtitle(&summary);

    clear_list_box(&controller.widgets.discovery.device_list);
    let has_devices = !state.discovered_devices.is_empty();
    controller
        .widgets
        .discovery
        .empty_state
        .set_visible(!has_devices);
    controller
        .widgets
        .discovery
        .device_list
        .set_visible(has_devices);
    if !has_devices {
        return;
    }

    for device in &state.discovered_devices {
        let row = adw::ActionRow::new();
        row.set_title(&device.display_name);
        row.set_subtitle(&format_device_row_subtitle(state, device));

        let status_icon = gtk::Image::from_icon_name(status_icon_name(device.status));
        row.add_prefix(&status_icon);

        if device.capabilities.supports_receiver && device.endpoint.is_some() {
            let button = gtk::Button::with_label(if state.streaming.target(&device.id).is_some() {
                "Queued"
            } else {
                "Add to Cast"
            });
            let can_queue = state.streaming.target(&device.id).is_none();
            button.set_sensitive(can_queue);
            if can_queue {
                button.add_css_class("suggested-action");
                let controller = controller.clone();
                let device = device.clone();
                button.connect_clicked(move |_| {
                    queue_receiver_device_for_cast(&controller, device.clone())
                });
            }
            row.add_suffix(&button);
        }

        controller.widgets.discovery.device_list.append(&row);
    }
}

fn refresh_casting_page(controller: &UiController, state: &AppState) {
    refresh_receiver_selector(&controller.widgets.casting.receiver_selector, state);
    refresh_playback_target_selector(
        &controller.widgets.casting.local_output_selector,
        state,
        state.config.audio.local_playback_target_id.as_deref(),
        "Local mirror output: system default",
    );

    controller
        .widgets
        .casting
        .local_mirror_switch
        .set_active(state.config.audio.local_playback_enabled);
    controller
        .widgets
        .casting
        .session_row
        .set_subtitle(&format!(
            "{:?} with {} active target(s). Selected receiver: {}.",
            state.streaming.state,
            state.streaming.active_target_count(),
            selected_receiver_label(state)
        ));
    controller.widgets.casting.mirror_row.set_subtitle(&format!(
        "Local mirror {} on {} (available={}).",
        if state.streaming.local_mirror.desired_enabled {
            format!("desired as {:?}", state.streaming.local_mirror.state)
        } else {
            "disabled".to_string()
        },
        format_selected_playback_target(
            state,
            state.config.audio.local_playback_target_id.as_deref()
        ),
        state.local_playback_target_available()
    ));
    controller
        .widgets
        .casting
        .add_button
        .set_sensitive(!state.discovered_devices.is_empty());
    controller
        .widgets
        .casting
        .stop_all_button
        .set_sensitive(state.streaming.active_target_count() > 0);

    clear_list_box(&controller.widgets.casting.target_list);
    let has_targets = !state.streaming.targets.is_empty();
    controller
        .widgets
        .casting
        .empty_state
        .set_visible(!has_targets);
    controller
        .widgets
        .casting
        .target_list
        .set_visible(has_targets);
    for target in &state.streaming.targets {
        let row = adw::ActionRow::new();
        row.set_title(&target.receiver_name);
        row.set_subtitle(&format_target_row_subtitle(target));

        let remove_button = gtk::Button::with_label("Remove");
        let controller_for_button = controller.clone();
        let device_id = target.receiver_id.clone();
        remove_button
            .connect_clicked(move |_| remove_target_from_cast(&controller_for_button, &device_id));
        row.add_suffix(&remove_button);

        controller.widgets.casting.target_list.append(&row);
    }

    controller
        .widgets
        .casting
        .detail_label
        .set_text(&format_streaming_status(state));
}

fn refresh_audio_page(controller: &UiController, state: &AppState) {
    refresh_audio_source_selector(&controller.widgets.audio.source_selector, state);
    controller
        .widgets
        .audio
        .source_selector
        .set_sensitive(state.streaming.state == synchrosonic_core::StreamSessionState::Idle);
    controller
        .widgets
        .audio
        .current_source_row
        .set_subtitle(&format!(
            "{}. Changes require a fresh cast session once streaming has already started.",
            format_selected_audio_source(state)
        ));
    controller.widgets.audio.output_row.set_subtitle(&format!(
        "Sender mirror uses {}. Receiver mode uses {}.",
        format_selected_playback_target(
            state,
            state.config.audio.local_playback_target_id.as_deref()
        ),
        format_selected_playback_target(state, state.config.receiver.playback_target_id.as_deref())
    ));

    clear_list_box(&controller.widgets.audio.source_list);
    let has_sources = !state.audio_sources.is_empty();
    controller
        .widgets
        .audio
        .sources_empty_state
        .set_visible(!has_sources);
    controller
        .widgets
        .audio
        .source_list
        .set_visible(has_sources);
    for source in &state.audio_sources {
        let row = adw::ActionRow::new();
        row.set_title(&source.display_name);
        row.set_subtitle(&format_audio_source_row_subtitle(state, source));
        let icon = gtk::Image::from_icon_name(audio_source_icon_name(source.kind));
        row.add_prefix(&icon);
        controller.widgets.audio.source_list.append(&row);
    }

    clear_list_box(&controller.widgets.audio.output_list);
    let has_outputs = !state.playback_targets.is_empty();
    controller
        .widgets
        .audio
        .outputs_empty_state
        .set_visible(!has_outputs);
    controller
        .widgets
        .audio
        .output_list
        .set_visible(has_outputs);
    for target in &state.playback_targets {
        let row = adw::ActionRow::new();
        row.set_title(&target.display_name);
        row.set_subtitle(&format_playback_target_inventory_subtitle(state, target));
        let icon = gtk::Image::from_icon_name(playback_target_icon_name(target));
        row.add_prefix(&icon);
        controller.widgets.audio.output_list.append(&row);
    }
}

fn refresh_receiver_page(controller: &UiController, state: &AppState) {
    refresh_playback_target_selector(
        &controller.widgets.receiver.output_selector,
        state,
        state.config.receiver.playback_target_id.as_deref(),
        "Receiver output: system default",
    );
    refresh_latency_selector(
        &controller.widgets.receiver.latency_selector,
        state.config.receiver.latency_preset,
    );

    let receiver_active = state.receiver.state != synchrosonic_core::ReceiverServiceState::Idle;
    controller
        .widgets
        .receiver
        .latency_selector
        .set_sensitive(!receiver_active);
    controller
        .widgets
        .receiver
        .start_button
        .set_sensitive(!receiver_active);
    controller
        .widgets
        .receiver
        .stop_button
        .set_sensitive(receiver_active);

    controller.widgets.receiver.state_row.set_subtitle(&format!(
        "{:?} on {}:{} with {} latency.",
        state.receiver.state,
        state.receiver.bind_host,
        state.receiver.listen_port,
        format_latency_preset(state.receiver.latency_preset)
    ));
    controller.widgets.receiver.connection_row.set_subtitle(
        &state
            .receiver
            .connection
            .as_ref()
            .map(|connection| {
                format!(
                    "Session {} from {} at {} Hz / {} ch, requested {} ms.",
                    connection.session_id,
                    connection
                        .remote_addr
                        .map(|address| address.to_string())
                        .unwrap_or_else(|| "unresolved peer".to_string()),
                    connection.stream.sample_rate_hz,
                    connection.stream.channels,
                    connection.requested_latency_ms
                )
            })
            .unwrap_or_else(|| "No active sender connection yet.".to_string()),
    );
    controller.widgets.receiver.sync_row.set_subtitle(&format!(
        "{:?}, queued {} ms, target {} ms, late drops {}, sync resets {}.",
        state.receiver.sync.state,
        state.receiver.sync.queued_audio_ms,
        state.receiver.sync.expected_output_latency_ms,
        state.receiver.sync.late_packet_drops,
        state.receiver.sync.sync_resets
    ));
    controller
        .widgets
        .receiver
        .detail_label
        .set_text(&format_receiver_status(state));
}

fn refresh_diagnostics_page(controller: &UiController, state: &AppState) {
    let info_count = state
        .diagnostics
        .iter()
        .filter(|event| event.level == DiagnosticLevel::Info)
        .count();
    let warning_count = state
        .diagnostics
        .iter()
        .filter(|event| event.level == DiagnosticLevel::Warning)
        .count();
    let error_count = state
        .diagnostics
        .iter()
        .filter(|event| event.level == DiagnosticLevel::Error)
        .count();
    controller
        .widgets
        .diagnostics
        .summary_row
        .set_subtitle(&format!(
            "{} info, {} warning, {} error event(s). Newest events appear first.",
            info_count, warning_count, error_count
        ));
    let recent_logs = controller.log_store.snapshot_recent(60);
    controller
        .widgets
        .diagnostics
        .log_summary_row
        .set_subtitle(&format!(
            "{} structured log entr{} in memory. Log file: {}.",
            recent_logs.len(),
            if recent_logs.len() == 1 { "y" } else { "ies" },
            controller.paths.log_path.display()
        ));

    clear_list_box(&controller.widgets.diagnostics.list_box);
    let has_events = !state.diagnostics.is_empty();
    controller
        .widgets
        .diagnostics
        .empty_state
        .set_visible(!has_events);
    controller
        .widgets
        .diagnostics
        .list_box
        .set_visible(has_events);
    if has_events {
        for event in state.diagnostics.iter().rev().take(60) {
            let row = adw::ActionRow::new();
            row.set_title(&format!(
                "{} · {}",
                diagnostic_level_label(event.level),
                event.component
            ));
            row.set_subtitle(&format!(
                "{} · {}",
                format_unix_ms(event.timestamp_unix_ms),
                event.message
            ));

            let icon = gtk::Image::from_icon_name(diagnostic_icon_name(event.level));
            row.add_prefix(&icon);

            controller.widgets.diagnostics.list_box.append(&row);
        }
    }

    clear_list_box(&controller.widgets.diagnostics.log_list_box);
    let has_logs = !recent_logs.is_empty();
    controller
        .widgets
        .diagnostics
        .log_empty_state
        .set_visible(!has_logs);
    controller
        .widgets
        .diagnostics
        .log_list_box
        .set_visible(has_logs);
    if has_logs {
        for entry in recent_logs {
            let row = adw::ActionRow::new();
            row.set_title(&format!("{} · {}", entry.level, entry.target));
            row.set_subtitle(&format_log_entry_subtitle(&entry));
            let icon = gtk::Image::from_icon_name(log_icon_name(entry.level.as_str()));
            row.add_prefix(&icon);
            controller.widgets.diagnostics.log_list_box.append(&row);
        }
    }
}

fn refresh_settings_page(controller: &UiController, state: &AppState) {
    refresh_quality_selector(
        &controller.widgets.settings.quality_selector,
        state.config.transport.quality,
    );
    controller
        .widgets
        .settings
        .theme_switch
        .set_active(state.config.ui.prefer_dark_theme);
    controller
        .widgets
        .settings
        .verbose_logging_switch
        .set_active(state.config.diagnostics.verbose_logging);
    controller
        .widgets
        .settings
        .receiver_start_on_launch_switch
        .set_active(state.config.receiver.start_on_launch);

    controller
        .widgets
        .settings
        .summary_row
        .set_subtitle(&format!(
            "Schema v{}. Quality preset: {}. Theme: {}. Verbose logging next launch: {}. Receiver auto-start: {}.",
            state.config.schema_version,
            format_quality_preset(state.config.transport.quality),
            if state.config.ui.prefer_dark_theme {
                "prefer dark"
            } else {
                "follow system"
            },
            if state.config.diagnostics.verbose_logging {
                "enabled"
            } else {
                "disabled"
            },
            if state.config.receiver.start_on_launch {
                "enabled"
            } else {
                "disabled"
            }
        ));
    controller
        .widgets
        .settings
        .config_dir_row
        .set_subtitle(&format!(
            "{} (base directory, override with SYNCHROSONIC_CONFIG_DIR)",
            controller.paths.config_dir.display()
        ));
    controller
        .widgets
        .settings
        .state_dir_row
        .set_subtitle(&format!(
            "{} (base directory, override with SYNCHROSONIC_STATE_DIR)",
            controller.paths.state_dir.display()
        ));
    controller
        .widgets
        .settings
        .config_path_row
        .set_subtitle(&format!(
            "{} (active settings, schema v{})",
            controller.paths.config_path.display(),
            state.config.schema_version
        ));
    controller
        .widgets
        .settings
        .portable_config_row
        .set_subtitle(&format!(
            "{} (import/export target)",
            controller.paths.portable_config_path.display()
        ));
    controller
        .widgets
        .settings
        .log_path_row
        .set_subtitle(&format!(
            "{} (structured JSON lines)",
            controller.paths.log_path.display()
        ));
    controller.widgets.settings.note_row.set_subtitle(
        "Settings save automatically. Theme changes apply immediately, audio/cast preferences apply safely to the current or next session as supported, and verbose logging changes take effect on the next launch.",
    );
}

fn queue_selected_receiver_for_cast(controller: &UiController) {
    let Some(active_id) = controller.widgets.casting.receiver_selector.active_id() else {
        let mut state = controller.state.borrow_mut();
        state.diagnostics.push(DiagnosticEvent::warning(
            "streaming",
            "Select a receiver before adding it to the cast.",
        ));
        drop(state);
        refresh_ui(controller);
        return;
    };

    let device = {
        let state = controller.state.borrow();
        find_receiver_device(&state, active_id.as_str())
    };
    match device {
        Some(device) => queue_receiver_device_for_cast(controller, device),
        None => {
            let mut state = controller.state.borrow_mut();
            state.diagnostics.push(DiagnosticEvent::warning(
                "streaming",
                "The selected receiver is no longer available with a resolved transport endpoint.",
            ));
            drop(state);
            refresh_ui(controller);
        }
    }
}

fn queue_receiver_device_for_cast(controller: &UiController, device: DiscoveredDevice) {
    let Some(endpoint) = device.endpoint.clone() else {
        let mut state = controller.state.borrow_mut();
        state.diagnostics.push(DiagnosticEvent::warning(
            "streaming",
            "Selected receiver does not expose a transport endpoint yet.",
        ));
        drop(state);
        refresh_ui(controller);
        return;
    };

    let target = SenderTarget::new(device.id.clone(), device.display_name.clone(), endpoint);
    let (capture_settings, sender_name) = {
        let state = controller.state.borrow();
        (
            state.config.audio.capture_settings(),
            state.receiver.advertised_name.clone(),
        )
    };

    let result = controller
        .sender
        .lock()
        .expect("sender session mutex")
        .start(
            controller.audio_backend.clone(),
            capture_settings,
            target,
            sender_name,
        );
    let snapshot = controller
        .sender
        .lock()
        .expect("sender session mutex")
        .snapshot();

    {
        let mut state = controller.state.borrow_mut();
        let _ = state.select_receiver_device(device.id.clone());
        state.apply_streaming_snapshot(snapshot.clone());
        match result {
            Ok(()) => state.diagnostics.push(DiagnosticEvent::info(
                "streaming",
                format!(
                    "Queued receiver target {} at {}. Active targets: {}.",
                    device.display_name,
                    device
                        .endpoint
                        .as_ref()
                        .map(|endpoint| endpoint.address.to_string())
                        .unwrap_or_else(|| "unresolved endpoint".to_string()),
                    snapshot.active_target_count()
                ),
            )),
            Err(error) => state.diagnostics.push(DiagnosticEvent::error(
                "streaming",
                format!("Failed to start or extend the cast session: {error}"),
            )),
        }
    }

    refresh_ui(controller);
}

fn remove_target_from_cast(controller: &UiController, device_id: &DeviceId) {
    let result = controller
        .sender
        .lock()
        .expect("sender session mutex")
        .stop_target(device_id);
    let snapshot = controller
        .sender
        .lock()
        .expect("sender session mutex")
        .snapshot();

    {
        let mut state = controller.state.borrow_mut();
        state.apply_streaming_snapshot(snapshot);
        match result {
            Ok(()) => state.diagnostics.push(DiagnosticEvent::info(
                "streaming",
                format!(
                    "Removed receiver target {} from the active cast.",
                    device_id
                ),
            )),
            Err(error) => state.diagnostics.push(DiagnosticEvent::error(
                "streaming",
                format!("Failed to remove receiver target {device_id}: {error}"),
            )),
        }
    }

    refresh_ui(controller);
}

fn stop_all_casting(controller: &UiController) {
    let result = controller
        .sender
        .lock()
        .expect("sender session mutex")
        .stop();
    let snapshot = controller
        .sender
        .lock()
        .expect("sender session mutex")
        .snapshot();

    {
        let mut state = controller.state.borrow_mut();
        state.apply_streaming_snapshot(snapshot);
        match result {
            Ok(()) => state.diagnostics.push(DiagnosticEvent::info(
                "streaming",
                "Stopped the sender session manager and released all cast targets.",
            )),
            Err(error) => state.diagnostics.push(DiagnosticEvent::error(
                "streaming",
                format!("Failed to stop the sender session manager: {error}"),
            )),
        }
    }

    refresh_ui(controller);
}

fn start_receiver_mode(controller: &UiController) {
    let runtime_result = controller
        .receiver
        .lock()
        .expect("receiver runtime mutex")
        .start();

    match runtime_result {
        Ok(()) => {
            let receiver_for_events = Arc::clone(&controller.receiver);
            let start_transport = controller
                .receiver_transport
                .lock()
                .expect("receiver transport mutex")
                .start(move |event| {
                    receiver_for_events
                        .lock()
                        .map_err(|_| synchrosonic_core::ReceiverError::ThreadJoin)?
                        .submit_transport_event(event)
                });

            if let Err(error) = start_transport {
                let _ = controller
                    .receiver
                    .lock()
                    .expect("receiver runtime mutex")
                    .stop();
                let mut state = controller.state.borrow_mut();
                state.diagnostics.push(DiagnosticEvent::error(
                    "receiver",
                    format!("Receiver transport listener failed to start: {error}"),
                ));
                drop(state);
                refresh_ui(controller);
                return;
            }

            let snapshot = controller
                .receiver
                .lock()
                .expect("receiver runtime mutex")
                .snapshot();
            {
                let mut state = controller.state.borrow_mut();
                state.apply_receiver_snapshot(snapshot.clone());
                state.diagnostics.push(DiagnosticEvent::info(
                    "receiver",
                    format!(
                        "Receiver mode listening on {}:{} with {} latency preset.",
                        snapshot.bind_host,
                        snapshot.listen_port,
                        format_latency_preset(snapshot.latency_preset)
                    ),
                ));
            }
        }
        Err(error) => {
            let mut state = controller.state.borrow_mut();
            state.diagnostics.push(DiagnosticEvent::error(
                "receiver",
                format!("Failed to start receiver mode: {error}"),
            ));
        }
    }

    refresh_ui(controller);
}

fn stop_receiver_mode(controller: &UiController) {
    let stop_transport = controller
        .receiver_transport
        .lock()
        .expect("receiver transport mutex")
        .stop();
    let result = controller
        .receiver
        .lock()
        .expect("receiver runtime mutex")
        .stop();
    let snapshot = controller
        .receiver
        .lock()
        .expect("receiver runtime mutex")
        .snapshot();

    {
        let mut state = controller.state.borrow_mut();
        state.apply_receiver_snapshot(snapshot);
        if let Err(error) = stop_transport {
            state.diagnostics.push(DiagnosticEvent::warning(
                "receiver",
                format!("Receiver transport stop reported: {error}"),
            ));
        }
        match result {
            Ok(()) => state.diagnostics.push(DiagnosticEvent::info(
                "receiver",
                "Receiver mode stopped and released its playback and transport resources.",
            )),
            Err(error) => state.diagnostics.push(DiagnosticEvent::error(
                "receiver",
                format!("Failed to stop receiver mode: {error}"),
            )),
        }
    }

    refresh_ui(controller);
}

fn apply_imported_config(controller: &UiController, report: ConfigLoadReport) {
    let ConfigLoadReport {
        config: imported_config,
        warnings,
        ..
    } = report;

    apply_color_scheme(imported_config.ui.prefer_dark_theme);

    let (quality_result, local_target_result, local_enabled_result, sender_snapshot) = {
        let mut sender = controller.sender.lock().expect("sender session mutex");
        let quality_result = sender.set_quality_preset(imported_config.transport.quality);
        let local_target_result = sender
            .set_local_playback_target(imported_config.audio.local_playback_target_id.clone());
        let local_enabled_result =
            sender.set_local_playback_enabled(imported_config.audio.local_playback_enabled);
        let sender_snapshot = sender.snapshot();
        (
            quality_result,
            local_target_result,
            local_enabled_result,
            sender_snapshot,
        )
    };

    let (receiver_target_result, receiver_latency_result, receiver_snapshot) = {
        let mut receiver = controller.receiver.lock().expect("receiver runtime mutex");
        let receiver_target_result =
            receiver.set_playback_target(imported_config.receiver.playback_target_id.clone());
        let receiver_latency_result =
            receiver.set_latency_preset(imported_config.receiver.latency_preset);
        let receiver_snapshot = receiver.snapshot();
        (
            receiver_target_result,
            receiver_latency_result,
            receiver_snapshot,
        )
    };

    {
        let mut state = controller.state.borrow_mut();
        let audio_sources = state.audio_sources.clone();
        state.config = imported_config.clone();
        state.selected_audio_source_id = imported_config.audio.preferred_source_id.clone();
        state.set_audio_sources(audio_sources);
        state.apply_streaming_snapshot(sender_snapshot);
        state.apply_receiver_snapshot(receiver_snapshot);
        state.set_receiver_start_on_launch(imported_config.receiver.start_on_launch);
        state.set_last_view_name(imported_config.ui.last_view_name.clone());

        state.diagnostics.push(DiagnosticEvent::info(
            "settings",
            format!(
                "Imported portable configuration from {}.",
                controller.paths.portable_config_path.display()
            ),
        ));
        for warning in warnings {
            state
                .diagnostics
                .push(DiagnosticEvent::warning("settings", warning));
        }
        if let Err(error) = quality_result {
            state.diagnostics.push(DiagnosticEvent::warning(
                "settings",
                format!("Imported sender quality preset could not be applied immediately: {error}"),
            ));
        }
        if let Err(error) = local_target_result {
            state.diagnostics.push(DiagnosticEvent::warning(
                "settings",
                format!("Imported local playback target could not be applied immediately: {error}"),
            ));
        }
        if let Err(error) = local_enabled_result {
            state.diagnostics.push(DiagnosticEvent::warning(
                "settings",
                format!("Imported local mirror state could not be applied immediately: {error}"),
            ));
        }
        if let Err(error) = receiver_target_result {
            state.diagnostics.push(DiagnosticEvent::warning(
                "settings",
                format!(
                    "Imported receiver playback target could not be applied immediately: {error}"
                ),
            ));
        }
        if let Err(error) = receiver_latency_result {
            state.diagnostics.push(DiagnosticEvent::warning(
                "settings",
                format!(
                    "Imported receiver latency preset will apply on a later receiver start: {error}"
                ),
            ));
        }
    }

    restore_visible_page(&controller.navigation, &imported_config.ui.last_view_name);
    persist_current_config(
        controller,
        "settings",
        Some("Portable configuration became the active saved configuration.".to_string()),
    );
    refresh_ui(controller);
}

fn dashboard_page() -> (gtk::ScrolledWindow, DashboardWidgets) {
    let (page, content) = page_shell(
        "Home",
        "Manage discovery, casting, audio routing, receiver mode, and diagnostics from a single Linux-native control surface.",
    );

    let quick_actions = section_box("Quick actions");
    let action_buttons = gtk::Box::new(Orientation::Vertical, 12);
    let open_devices_button = gtk::Button::with_label("Open discovered devices");
    open_devices_button.add_css_class("suggested-action");
    let open_casting_button = gtk::Button::with_label("Open casting controls");
    let open_audio_button = gtk::Button::with_label("Open audio routing");
    let open_receiver_button = gtk::Button::with_label("Open receiver mode");
    action_buttons.append(&open_devices_button);
    action_buttons.append(&open_casting_button);
    action_buttons.append(&open_audio_button);
    action_buttons.append(&open_receiver_button);
    quick_actions.append(&action_buttons);
    content.append(&quick_actions);

    let overview_group = preferences_group(
        "Overview",
        Some("A live summary of the current application state."),
    );
    let session_row = summary_row("Session");
    let discovery_row = summary_row("Discovery");
    let casting_row = summary_row("Casting");
    let audio_row = summary_row("Audio");
    overview_group.add(&session_row);
    overview_group.add(&discovery_row);
    overview_group.add(&casting_row);
    overview_group.add(&audio_row);
    content.append(&overview_group);

    let detail_group = preferences_group(
        "Detailed status",
        Some("This mirrors the internal session snapshots in a debuggable text view."),
    );
    let detail_label = detail_label();
    detail_group.add(&label_row(&detail_label));
    content.append(&detail_group);

    (
        page,
        DashboardWidgets {
            session_row,
            discovery_row,
            casting_row,
            audio_row,
            detail_label,
            open_devices_button,
            open_casting_button,
            open_audio_button,
            open_receiver_button,
        },
    )
}

fn discovery_page() -> (gtk::ScrolledWindow, DiscoveryWidgets) {
    let (page, content) = page_shell(
        "Discovered devices",
        "Receiver-capable devices appear here as they are advertised over mDNS. Queue them into a cast without leaving the page.",
    );

    let summary_group = preferences_group(
        "Discovery status",
        Some("Use this page to confirm visibility and add receivers quickly."),
    );
    let summary_row = summary_row("Current discovery status");
    summary_group.add(&summary_row);
    content.append(&summary_group);

    let device_section = section_box("Available devices");
    let empty_state = empty_state(
        "No devices discovered yet",
        "Waiting for SynchroSonic devices to appear on the local network.",
        "network-workgroup-symbolic",
    );
    let device_list = boxed_list();
    device_section.append(&empty_state);
    device_section.append(&device_list);
    content.append(&device_section);

    (
        page,
        DiscoveryWidgets {
            summary_row,
            device_list,
            empty_state,
        },
    )
}

fn casting_page() -> (gtk::ScrolledWindow, CastingWidgets) {
    let (page, content) = page_shell(
        "Active casting sessions",
        "Add one or more discovered receivers, keep a local mirror if needed, and watch target health without burying the transport logic inside the UI.",
    );

    let controls_group = preferences_group(
        "Cast controls",
        Some("Receiver selection and local mirror routing for the sender side."),
    );
    let receiver_selector = gtk::ComboBoxText::new();
    receiver_selector.set_hexpand(true);
    let receiver_row = control_row(
        "Receiver target",
        "Choose a discovered receiver, then add it to the cast session.",
        &receiver_selector,
    );
    let local_mirror_switch = gtk::Switch::new();
    let mirror_switch_row = control_row(
        "Mirror locally while casting",
        "Keep playback on the sender while the network fan-out is active.",
        &local_mirror_switch,
    );
    let local_output_selector = gtk::ComboBoxText::new();
    local_output_selector.set_hexpand(true);
    let mirror_output_row = control_row(
        "Local mirror output",
        "Choose a local playback sink for the optional sender-side mirror.",
        &local_output_selector,
    );
    controls_group.add(&receiver_row);
    controls_group.add(&mirror_switch_row);
    controls_group.add(&mirror_output_row);
    content.append(&controls_group);

    let buttons = gtk::Box::new(Orientation::Horizontal, 12);
    let add_button = gtk::Button::with_label("Add selected receiver");
    add_button.add_css_class("suggested-action");
    let stop_all_button = gtk::Button::with_label("Stop all casting");
    buttons.append(&add_button);
    buttons.append(&stop_all_button);
    content.append(&buttons);

    let summary_group = preferences_group(
        "Session health",
        Some("A concise summary of the sender session and local mirror branch."),
    );
    let session_row = summary_row("Cast session");
    let mirror_row = summary_row("Local mirror");
    summary_group.add(&session_row);
    summary_group.add(&mirror_row);
    content.append(&summary_group);

    let targets_section = section_box("Queued and active targets");
    let empty_state = empty_state(
        "No active casting targets",
        "Choose a receiver and add it to start a cast. You can queue multiple receivers one after another.",
        "media-playback-start-symbolic",
    );
    let target_list = boxed_list();
    targets_section.append(&empty_state);
    targets_section.append(&target_list);
    content.append(&targets_section);

    let detail_group = preferences_group(
        "Detailed sender state",
        Some("Low-level transport, queue, and local mirror diagnostics."),
    );
    let detail_label = detail_label();
    detail_group.add(&label_row(&detail_label));
    content.append(&detail_group);

    (
        page,
        CastingWidgets {
            receiver_selector,
            add_button,
            stop_all_button,
            local_mirror_switch,
            local_output_selector,
            session_row,
            mirror_row,
            target_list,
            empty_state,
            detail_label,
        },
    )
}

fn audio_page() -> (gtk::ScrolledWindow, AudioWidgets) {
    let (page, content) = page_shell(
        "Audio source and output selection",
        "Choose the capture source used for sender sessions and inspect the playback outputs that local mirror and receiver mode can target.",
    );

    let selection_group = preferences_group(
        "Current routing",
        Some("Source selection is explicit and output targeting stays visible."),
    );
    let source_selector = gtk::ComboBoxText::new();
    source_selector.set_hexpand(true);
    let source_selector_row = control_row(
        "Capture source",
        "Choose the PipeWire source for future sender sessions.",
        &source_selector,
    );
    let current_source_row = summary_row("Selected source");
    let output_row = summary_row("Selected outputs");
    selection_group.add(&source_selector_row);
    selection_group.add(&current_source_row);
    selection_group.add(&output_row);
    content.append(&selection_group);

    let sources_section = section_box("Available capture sources");
    let sources_empty_state = empty_state(
        "No capture sources found",
        "PipeWire did not return any readable capture sources.",
        "audio-input-microphone-symbolic",
    );
    let source_list = boxed_list();
    sources_section.append(&sources_empty_state);
    sources_section.append(&source_list);
    content.append(&sources_section);

    let outputs_section = section_box("Available playback outputs");
    let outputs_empty_state = empty_state(
        "No playback outputs found",
        "PipeWire did not return any playback sinks for local mirror or receiver mode.",
        "audio-speakers-symbolic",
    );
    let output_list = boxed_list();
    outputs_section.append(&outputs_empty_state);
    outputs_section.append(&output_list);
    content.append(&outputs_section);

    (
        page,
        AudioWidgets {
            source_selector,
            current_source_row,
            output_row,
            source_list,
            output_list,
            sources_empty_state,
            outputs_empty_state,
        },
    )
}

fn receiver_page() -> (gtk::ScrolledWindow, ReceiverWidgets) {
    let (page, content) = page_shell(
        "Receiver mode",
        "Run this machine as a LAN receiver, choose its playback output, and inspect sync and buffering health without losing the exact debug details.",
    );

    let controls_group = preferences_group(
        "Receiver controls",
        Some("Latency preset changes apply before starting receiver mode. Output changes can be requested at runtime."),
    );
    let latency_selector = gtk::ComboBoxText::new();
    let latency_row = control_row(
        "Latency preset",
        "Low latency, balanced, or stable buffering behavior for receiver playback.",
        &latency_selector,
    );
    let output_selector = gtk::ComboBoxText::new();
    output_selector.set_hexpand(true);
    let output_row = control_row(
        "Playback output",
        "Choose the system default sink or a specific local output, including Bluetooth when available.",
        &output_selector,
    );
    controls_group.add(&latency_row);
    controls_group.add(&output_row);
    content.append(&controls_group);

    let buttons = gtk::Box::new(Orientation::Horizontal, 12);
    let start_button = gtk::Button::with_label("Start receiver mode");
    start_button.add_css_class("suggested-action");
    let stop_button = gtk::Button::with_label("Stop receiver mode");
    buttons.append(&start_button);
    buttons.append(&stop_button);
    content.append(&buttons);

    let summary_group = preferences_group(
        "Receiver health",
        Some("State, connection, and sync information that stays readable at a glance."),
    );
    let state_row = summary_row("Receiver state");
    let connection_row = summary_row("Connection");
    let sync_row = summary_row("Sync");
    summary_group.add(&state_row);
    summary_group.add(&connection_row);
    summary_group.add(&sync_row);
    content.append(&summary_group);

    let detail_group = preferences_group(
        "Detailed receiver state",
        Some("Buffered audio, sync timing, metrics, and transport details."),
    );
    let detail_label = detail_label();
    detail_group.add(&label_row(&detail_label));
    content.append(&detail_group);

    (
        page,
        ReceiverWidgets {
            latency_selector,
            output_selector,
            start_button,
            stop_button,
            state_row,
            connection_row,
            sync_row,
            detail_label,
        },
    )
}

fn diagnostics_page() -> (gtk::ScrolledWindow, DiagnosticsWidgets) {
    let (page, content) = page_shell(
        "Diagnostics and logs",
        "Warnings, errors, and status transitions are surfaced here so transport and timing behavior stay understandable during early phases.",
    );

    let summary_group = preferences_group(
        "Diagnostic summary",
        Some("Diagnostics capture state transitions; structured logs show traced application events."),
    );
    let diagnostic_summary_row = summary_row("Recent diagnostic volume");
    let log_summary_row = summary_row("Structured log viewer");
    summary_group.add(&diagnostic_summary_row);
    summary_group.add(&log_summary_row);
    content.append(&summary_group);

    let clear_button = gtk::Button::with_label("Clear diagnostics");
    content.append(&clear_button);

    let diagnostics_section = section_box("Recent events");
    let diagnostics_empty_state = empty_state(
        "No diagnostics recorded",
        "Live state changes and warnings will appear here once the app does work.",
        "dialog-information-symbolic",
    );
    let list_box = boxed_list();
    diagnostics_section.append(&diagnostics_empty_state);
    diagnostics_section.append(&list_box);
    content.append(&diagnostics_section);

    let logs_section = section_box("Recent structured logs");
    let log_empty_state = empty_state(
        "No structured logs captured yet",
        "Tracing events from this app run will appear here, and the JSON-lines log file will keep them on disk.",
        "text-x-log-symbolic",
    );
    let log_list_box = boxed_list();
    logs_section.append(&log_empty_state);
    logs_section.append(&log_list_box);
    content.append(&logs_section);

    (
        page,
        DiagnosticsWidgets {
            summary_row: diagnostic_summary_row,
            log_summary_row,
            clear_button,
            list_box,
            empty_state: diagnostics_empty_state,
            log_list_box,
            log_empty_state,
        },
    )
}

fn settings_page() -> (gtk::ScrolledWindow, SettingsWidgets) {
    let (page, content) = page_shell(
        "Settings",
        "Operational preferences are kept visible and explicit. Runtime behavior stays controlled instead of hiding state inside widgets.",
    );

    let settings_group = preferences_group(
        "Run-time preferences",
        Some(
            "These controls are persisted to disk and restored on restart where the runtime can safely apply them.",
        ),
    );
    let quality_selector = gtk::ComboBoxText::new();
    let quality_row = control_row(
        "Sender quality preset",
        "Choose low latency, balanced, or high quality for future target negotiation.",
        &quality_selector,
    );
    let theme_switch = gtk::Switch::new();
    let theme_row = control_row(
        "Prefer dark theme",
        "Use a darker application appearance when possible.",
        &theme_switch,
    );
    let verbose_logging_switch = gtk::Switch::new();
    let logging_row = control_row(
        "Verbose logging",
        "Persist a debug-level logging preference for the next app launch.",
        &verbose_logging_switch,
    );
    let receiver_start_on_launch_switch = gtk::Switch::new();
    let receiver_start_on_launch_row = control_row(
        "Auto-start receiver on launch",
        "Start receiver mode automatically after the app restores its saved configuration.",
        &receiver_start_on_launch_switch,
    );
    let summary_status_row = summary_row("Current configuration");
    let config_dir_row = summary_row("Config directory");
    let state_dir_row = summary_row("State directory");
    let config_path_row = summary_row("Active config file");
    let portable_config_row = summary_row("Portable config file");
    let log_path_row = summary_row("Structured log file");
    let note_status_row = summary_row("Persistence status");
    let import_export_buttons = gtk::Box::new(Orientation::Horizontal, 12);
    let import_button = gtk::Button::with_label("Import portable config");
    let export_button = gtk::Button::with_label("Export portable config");
    import_export_buttons.append(&import_button);
    import_export_buttons.append(&export_button);
    settings_group.add(&quality_row);
    settings_group.add(&theme_row);
    settings_group.add(&logging_row);
    settings_group.add(&receiver_start_on_launch_row);
    settings_group.add(&summary_status_row);
    settings_group.add(&config_dir_row);
    settings_group.add(&state_dir_row);
    settings_group.add(&config_path_row);
    settings_group.add(&portable_config_row);
    settings_group.add(&log_path_row);
    settings_group.add(&note_status_row);
    content.append(&settings_group);
    content.append(&import_export_buttons);

    (
        page,
        SettingsWidgets {
            quality_selector,
            theme_switch,
            verbose_logging_switch,
            receiver_start_on_launch_switch,
            summary_row: summary_status_row,
            note_row: note_status_row,
            config_dir_row,
            state_dir_row,
            config_path_row,
            portable_config_row,
            log_path_row,
            import_button,
            export_button,
        },
    )
}

fn about_page() -> gtk::ScrolledWindow {
    let (page, content) = page_shell(
        "About",
        "Release metadata, support paths, and current scope are surfaced here so packaged Linux builds stay self-describing.",
    );

    let about_group = preferences_group(
        "Project",
        Some("Core metadata, release identity, and scope for the current implementation."),
    );
    let project_row = summary_row("Project");
    project_row.set_subtitle(metadata::APP_SUMMARY);
    let version_row = summary_row("Version");
    version_row.set_subtitle(&format!(
        "{} · application id {} · binary {} · icon {}",
        metadata::APP_VERSION,
        metadata::APP_ID,
        metadata::APP_BINARY_NAME,
        metadata::APP_ICON_NAME
    ));
    let developer_row = summary_row("Developers");
    developer_row.set_subtitle(&metadata::authors_display());
    let source_row = summary_row("Source and license");
    source_row.set_subtitle(&format!(
        "{} · repo {}",
        metadata::APP_LICENSE,
        metadata::APP_REPOSITORY
    ));
    let support_row = summary_row("Support");
    support_row.set_subtitle(&format!(
        "Issues: {} · Security policy: {} · Contributing guide: {}",
        metadata::APP_BUG_TRACKER,
        metadata::APP_SECURITY_POLICY_PATH,
        metadata::APP_CONTRIBUTING_PATH
    ));
    let homepage_row = summary_row("Homepage");
    homepage_row.set_subtitle(metadata::APP_HOMEPAGE);
    let scope_row = summary_row("Current Bluetooth scope");
    scope_row.set_subtitle("Bluetooth is treated as a local playback-output choice on Linux, not as a separate streaming transport or receiver discovery path.");
    about_group.add(&project_row);
    about_group.add(&version_row);
    about_group.add(&developer_row);
    about_group.add(&source_row);
    about_group.add(&support_row);
    about_group.add(&homepage_row);
    about_group.add(&scope_row);
    content.append(&about_group);

    page
}

fn page_shell(title: &str, subtitle: &str) -> (gtk::ScrolledWindow, gtk::Box) {
    let body = gtk::Box::new(Orientation::Vertical, 18);
    body.set_margin_top(24);
    body.set_margin_bottom(24);
    body.set_margin_start(24);
    body.set_margin_end(24);

    let header = gtk::Box::new(Orientation::Vertical, 6);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("title-1");
    title_label.set_halign(Align::Start);
    let subtitle_label = gtk::Label::new(Some(subtitle));
    subtitle_label.add_css_class("dim-label");
    subtitle_label.set_wrap(true);
    subtitle_label.set_halign(Align::Start);
    header.append(&title_label);
    header.append(&subtitle_label);
    body.append(&header);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_hexpand(true);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&body));

    (scrolled, body)
}

fn preferences_group(title: &str, description: Option<&str>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title(title);
    group.set_description(description);
    group
}

fn summary_row(title: &str) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_subtitle("Waiting for state updates.");
    row
}

fn control_row(title: &str, subtitle: &str, widget: &impl IsA<gtk::Widget>) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.add_suffix(widget);
    row
}

fn detail_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_wrap(true);
    label.set_selectable(true);
    label.set_halign(Align::Start);
    label.set_xalign(0.0);
    label
}

fn label_row(label: &gtk::Label) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title("Snapshot");
    row.set_subtitle("");
    row.add_suffix(label);
    row
}

fn section_box(title: &str) -> gtk::Box {
    let section = gtk::Box::new(Orientation::Vertical, 12);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("title-4");
    heading.set_halign(Align::Start);
    section.append(&heading);
    section
}

fn boxed_list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list
}

fn empty_state(title: &str, description: &str, icon_name: &str) -> adw::StatusPage {
    let page = adw::StatusPage::new();
    page.set_title(title);
    page.set_description(Some(description));
    page.set_icon_name(Some(icon_name));
    page
}

fn start_discovery_poll(mut discovery: MdnsDiscoveryService, controller: UiController) {
    glib::timeout_add_seconds_local(1, move || {
        let previous_selected_receiver = controller
            .state
            .borrow()
            .selected_receiver_device_id
            .clone();

        loop {
            match discovery.poll_event() {
                Ok(Some(event)) => controller.state.borrow_mut().apply_discovery_event(event),
                Ok(None) => break,
                Err(error) => {
                    controller
                        .state
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
                    controller.state.borrow_mut().apply_discovery_event(event);
                }
            }
            Err(error) => controller
                .state
                .borrow_mut()
                .diagnostics
                .push(DiagnosticEvent::warning(
                    "discovery",
                    format!("mDNS stale pruning failed: {error}"),
                )),
        }

        controller
            .state
            .borrow_mut()
            .apply_discovery_snapshot(discovery.snapshot());
        let current_selected_receiver = controller
            .state
            .borrow()
            .selected_receiver_device_id
            .clone();
        if previous_selected_receiver != current_selected_receiver {
            let message = match (previous_selected_receiver, current_selected_receiver) {
                (Some(previous), Some(current)) => Some(DiagnosticEvent::warning(
                    "discovery",
                    format!(
                        "Selected receiver {} disappeared; switched selection to {}.",
                        previous, current
                    ),
                )),
                (Some(previous), None) => Some(DiagnosticEvent::warning(
                    "discovery",
                    format!(
                        "Selected receiver {} disappeared and no replacement receiver is available.",
                        previous
                    ),
                )),
                _ => None,
            };
            if let Some(message) = message {
                controller.state.borrow_mut().diagnostics.push(message);
            }
        }
        refresh_ui(&controller);
        ControlFlow::Continue
    });
}

fn start_receiver_poll(controller: UiController) {
    glib::timeout_add_seconds_local(1, move || {
        let snapshot = controller
            .receiver
            .lock()
            .expect("receiver runtime mutex")
            .snapshot();
        let transport_snapshot = controller
            .receiver_transport
            .lock()
            .expect("receiver transport mutex")
            .snapshot();

        {
            let mut state = controller.state.borrow_mut();
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
        }

        refresh_ui(&controller);
        ControlFlow::Continue
    });
}

fn start_streaming_poll(controller: UiController) {
    glib::timeout_add_seconds_local(1, move || {
        let snapshot = controller
            .sender
            .lock()
            .expect("sender session mutex")
            .snapshot();
        let previous_snapshot = controller.state.borrow().streaming.clone();
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
            let mut state = controller.state.borrow_mut();
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
        }

        refresh_ui(&controller);
        ControlFlow::Continue
    });
}

fn start_audio_inventory_poll(controller: UiController) {
    let mut last_source_error = None::<String>;
    let mut last_playback_error = None::<String>;

    glib::timeout_add_seconds_local(1, move || {
        match controller.audio_backend.list_sources() {
            Ok(sources) => {
                last_source_error = None;
                let mut source_change_message = None::<DiagnosticEvent>;
                let mut source_selection_changed = false;
                {
                    let mut state = controller.state.borrow_mut();
                    let previous_selected = state.selected_audio_source_id.clone();
                    let previous_sources = state.audio_sources.len();
                    state.set_audio_sources(sources);
                    if previous_selected != state.selected_audio_source_id {
                        source_selection_changed = true;
                        source_change_message = Some(DiagnosticEvent::warning(
                            "audio",
                            format!(
                                "Capture source inventory changed; selected source is now {}.",
                                format_selected_audio_source(&state)
                            ),
                        ));
                    } else if previous_sources == 0 && !state.audio_sources.is_empty() {
                        source_change_message = Some(DiagnosticEvent::info(
                            "audio",
                            format!(
                                "Detected {} capture source(s) from {}.",
                                state.audio_sources.len(),
                                controller.context.audio_backend_name
                            ),
                        ));
                    }
                    if let Some(message) = source_change_message.take() {
                        state.diagnostics.push(message);
                    }
                }
                if source_selection_changed {
                    persist_current_config(&controller, "audio", None);
                }
            }
            Err(error) => {
                let message = format!("PipeWire source enumeration failed: {error}");
                if last_source_error.as_deref() != Some(message.as_str()) {
                    controller
                        .state
                        .borrow_mut()
                        .diagnostics
                        .push(DiagnosticEvent::warning("audio", message.clone()));
                }
                last_source_error = Some(message);
            }
        }

        match controller.audio_backend.list_playback_targets() {
            Ok(targets) => {
                last_playback_error = None;

                let mut local_retry_target = None::<Option<String>>;
                let mut local_transition_message = None::<DiagnosticEvent>;
                let mut receiver_transition_message = None::<DiagnosticEvent>;

                {
                    let mut state = controller.state.borrow_mut();
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

                    if let Some(message) = local_transition_message.take() {
                        state.diagnostics.push(message);
                    }
                    if let Some(message) = receiver_transition_message.take() {
                        state.diagnostics.push(message);
                    }

                    if !targets_changed
                        && previous_local_available == local_available
                        && previous_receiver_available == receiver_available
                    {
                        // No-op; we still refresh the UI below in case source inventory changed.
                    }
                }

                if let Some(target_id) = local_retry_target.flatten() {
                    let retry_result = controller
                        .sender
                        .lock()
                        .expect("sender session mutex")
                        .set_local_playback_target(Some(target_id.clone()));
                    let snapshot = controller
                        .sender
                        .lock()
                        .expect("sender session mutex")
                        .snapshot();
                    let mut state = controller.state.borrow_mut();
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
            }
            Err(error) => {
                let message = format!("PipeWire playback target enumeration failed: {error}");
                if last_playback_error.as_deref() != Some(message.as_str()) {
                    controller
                        .state
                        .borrow_mut()
                        .diagnostics
                        .push(DiagnosticEvent::warning("audio", message.clone()));
                }
                last_playback_error = Some(message);
            }
        }

        refresh_ui(&controller);
        ControlFlow::Continue
    });
}

fn refresh_audio_inventory(audio_backend: &LinuxAudioBackend, state: &mut AppState) {
    match audio_backend.list_sources() {
        Ok(sources) => state.set_audio_sources(sources),
        Err(error) => state.diagnostics.push(DiagnosticEvent::warning(
            "audio",
            format!("PipeWire source enumeration failed: {error}"),
        )),
    }

    match audio_backend.list_playback_targets() {
        Ok(targets) => state.set_playback_targets(targets),
        Err(error) => state.diagnostics.push(DiagnosticEvent::warning(
            "audio",
            format!("PipeWire playback target enumeration failed: {error}"),
        )),
    }
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

fn refresh_audio_source_selector(selector: &gtk::ComboBoxText, state: &AppState) {
    selector.remove_all();
    for source in &state.audio_sources {
        selector.append(Some(source.id.as_str()), &source.display_name);
    }

    if let Some(selected_id) = state.selected_audio_source_id.as_deref() {
        selector.set_active_id(Some(selected_id));
    } else if !state.audio_sources.is_empty() {
        selector.set_active(Some(0));
    }
}

fn refresh_quality_selector(selector: &gtk::ComboBoxText, preset: QualityPreset) {
    selector.remove_all();
    selector.append(Some(QUALITY_LOW_LATENCY_ID), "Low latency");
    selector.append(Some(QUALITY_BALANCED_ID), "Balanced");
    selector.append(Some(QUALITY_HIGH_QUALITY_ID), "High quality");
    selector.set_active_id(Some(quality_preset_id(preset)));
}

fn refresh_latency_selector(selector: &gtk::ComboBoxText, preset: ReceiverLatencyPreset) {
    selector.remove_all();
    selector.append(Some(LATENCY_LOW_ID), "Low latency");
    selector.append(Some(LATENCY_BALANCED_ID), "Balanced");
    selector.append(Some(LATENCY_STABLE_ID), "Stable");
    selector.set_active_id(Some(latency_preset_id(preset)));
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

fn format_selected_audio_source(state: &AppState) -> String {
    state
        .selected_audio_source_id
        .as_deref()
        .and_then(|id| state.audio_sources.iter().find(|source| source.id == id))
        .map(|source| {
            format!(
                "{} ({})",
                source.display_name,
                audio_source_kind_label(source.kind)
            )
        })
        .unwrap_or_else(|| "No capture source selected".to_string())
}

fn format_device_row_subtitle(state: &AppState, device: &DiscoveredDevice) -> String {
    let selected_suffix = if state.selected_receiver_device_id.as_ref() == Some(&device.id) {
        " Selected for manual cast actions."
    } else {
        ""
    };
    let active_suffix = if state.streaming.target(&device.id).is_some() {
        " Currently queued in the active cast."
    } else {
        ""
    };
    format!(
        "Status: {:?}, availability: {:?}, endpoint: {}, receiver={}, local_output={}, bluetooth={}.{}{}",
        device.status,
        device.availability,
        device
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.address.to_string())
            .unwrap_or_else(|| "unresolved".to_string()),
        device.capabilities.supports_receiver,
        device.capabilities.supports_local_output,
        device.capabilities.supports_bluetooth_output,
        selected_suffix,
        active_suffix
    )
}

fn format_target_row_subtitle(target: &StreamTargetSnapshot) -> String {
    format!(
        "{:?} with {:?} health, endpoint {}, latency {:?} ms, buffer {}%, dropped {} packet(s).",
        target.state,
        target.health,
        target.endpoint,
        target.metrics.latency_estimate_ms,
        target.network_buffer.fill_percent(),
        target.network_buffer.dropped_packets
    )
}

fn format_audio_source_row_subtitle(state: &AppState, source: &AudioSource) -> String {
    let selected = if state.selected_audio_source_id.as_deref() == Some(source.id.as_str()) {
        "selected"
    } else {
        "available"
    };
    let default = if source.is_default { ", default" } else { "" };
    format!(
        "{} source, {}{}",
        audio_source_kind_label(source.kind),
        selected,
        default
    )
}

fn format_playback_target_inventory_subtitle(state: &AppState, target: &PlaybackTarget) -> String {
    let selected_local =
        state.config.audio.local_playback_target_id.as_deref() == Some(target.id.as_str());
    let selected_receiver =
        state.config.receiver.playback_target_id.as_deref() == Some(target.id.as_str());
    let selected_suffix = match (selected_local, selected_receiver) {
        (true, true) => "Selected for local mirror and receiver mode.",
        (true, false) => "Selected for local mirror.",
        (false, true) => "Selected for receiver mode.",
        (false, false) => "Not currently selected.",
    };
    format!(
        "{} output, availability {:?}, default={}, {}",
        if target.is_bluetooth() {
            "Bluetooth"
        } else {
            "Standard"
        },
        target.availability,
        target.is_default,
        selected_suffix
    )
}

fn format_dashboard_status(state: &AppState, audio_backend_name: &str) -> String {
    let healthy_targets = state.streaming.healthy_target_count();
    let bluetooth_outputs = state
        .playback_targets
        .iter()
        .filter(|target| target.is_bluetooth())
        .count();
    format!(
        "Session: {:?}\nCapture: {:?}\nStreaming: {:?}\nSelected receiver: {}\nTarget sessions: total={} healthy={}\nPlayback outputs: total={} bluetooth={}\nSelected source: {}\nLocal mirror output: {} (available={})\nReceiver output: {} (available={})\nLocal mirror: desired={} state={:?} buffer={}% ({} / {} packets, dropped={}) played packets={} bytes={} error={}\nSender aggregate metrics: packets sent={} bytes sent={} bitrate={}bps latency={:?}ms gaps={} keepalives={}/{}\nReceiver: {:?}\nReceiver buffer: {}% ({} / {} packets)\nReceiver metrics: packets in={} played={} underruns={} overruns={} reconnects={}\nAudio backend: {}\nDefault stream port: {}\nSender quality preset: {}\nReceiver latency preset: {}\nLocal playback default: {}",
        state.cast_session,
        state.capture_state,
        state.streaming.state,
        selected_receiver_label(state),
        state.streaming.active_target_count(),
        healthy_targets,
        state.playback_targets.len(),
        bluetooth_outputs,
        format_selected_audio_source(state),
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
        state.config.transport.stream_port,
        format_quality_preset(state.config.transport.quality),
        format_latency_preset(state.config.receiver.latency_preset),
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
    let selected_device = selected_receiver_label(state);
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
        "State: {:?}\nSelected receiver: {}\nSender session id: {}\nCapture source: {}\nStream: {}\nActive target count: {}\nTargets:\n{}\nLocal mirror output: {} (available={})\nLocal mirror: desired={} state={:?} backend={} target={} buffer={}% ({} / {} packets, dropped={}) played packets={} bytes={} error={}\nAggregate metrics: packets sent={} packets received={} bytes sent={} bytes received={} bitrate={}bps latency estimate={:?}ms packet gaps={} keepalives sent={} keepalives received={}\nSender quality preset: {}\nLast error: {}\nSplit-stream path: PipeWire capture -> explicit branch fan-out -> [per-target bounded network queue -> per-target TCP framed transport -> receiver runtime -> PipeWire playback] + [bounded local mirror queue -> sender-side PipeWire playback].",
        stream.state,
        selected_device,
        stream.session_id.as_deref().unwrap_or("not started"),
        format_selected_audio_source(state),
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
        format_quality_preset(state.config.transport.quality),
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
    } else if !state
        .discovered_devices
        .iter()
        .filter(|device| device.capabilities.supports_receiver)
        .collect::<Vec<_>>()
        .is_empty()
    {
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
        "State: {:?}\nAdvertised name: {}\nListen address: {}:{}\nLatency preset: {}\nPlayback backend: {}\nPlayback target: {} (available={})\nConnection: {}\nBuffer: {} packet(s), {} frame(s), {} ms queued, target {} ms, max {} ms, {}% full\nSync: state={:?} expected={} ms requested={} queued={} ms buffer delta={} ms schedule error={} ms late drops={} resets={} last sender ts={} last sender unix={}\nMetrics: packets in={} frames in={} bytes in={} packets out={} frames out={} bytes out={} underruns={} overruns={} reconnect attempts={}\nLast error: {}\nInternal app flow: use the Start/Stop buttons here; the TCP receiver listener already submits Connected, AudioPacket, KeepAlive, and Disconnected events into the runtime.",
        receiver.state,
        receiver.advertised_name,
        receiver.bind_host,
        receiver.listen_port,
        format_latency_preset(receiver.latency_preset),
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

fn quality_preset_id(preset: QualityPreset) -> &'static str {
    match preset {
        QualityPreset::LowLatency => QUALITY_LOW_LATENCY_ID,
        QualityPreset::Balanced => QUALITY_BALANCED_ID,
        QualityPreset::HighQuality => QUALITY_HIGH_QUALITY_ID,
    }
}

fn quality_preset_from_id(value: &str) -> Option<QualityPreset> {
    match value {
        QUALITY_LOW_LATENCY_ID => Some(QualityPreset::LowLatency),
        QUALITY_BALANCED_ID => Some(QualityPreset::Balanced),
        QUALITY_HIGH_QUALITY_ID => Some(QualityPreset::HighQuality),
        _ => None,
    }
}

fn latency_preset_id(preset: ReceiverLatencyPreset) -> &'static str {
    match preset {
        ReceiverLatencyPreset::LowLatency => LATENCY_LOW_ID,
        ReceiverLatencyPreset::Balanced => LATENCY_BALANCED_ID,
        ReceiverLatencyPreset::Stable => LATENCY_STABLE_ID,
    }
}

fn latency_preset_from_id(value: &str) -> Option<ReceiverLatencyPreset> {
    match value {
        LATENCY_LOW_ID => Some(ReceiverLatencyPreset::LowLatency),
        LATENCY_BALANCED_ID => Some(ReceiverLatencyPreset::Balanced),
        LATENCY_STABLE_ID => Some(ReceiverLatencyPreset::Stable),
        _ => None,
    }
}

fn format_quality_preset(preset: QualityPreset) -> &'static str {
    match preset {
        QualityPreset::LowLatency => "Low latency",
        QualityPreset::Balanced => "Balanced",
        QualityPreset::HighQuality => "High quality",
    }
}

fn format_latency_preset(preset: ReceiverLatencyPreset) -> &'static str {
    match preset {
        ReceiverLatencyPreset::LowLatency => "Low latency",
        ReceiverLatencyPreset::Balanced => "Balanced",
        ReceiverLatencyPreset::Stable => "Stable",
    }
}

fn selected_receiver_label(state: &AppState) -> String {
    state
        .selected_receiver_device_id
        .as_ref()
        .and_then(|selected_id| {
            state
                .discovered_devices
                .iter()
                .find(|device| &device.id == selected_id)
        })
        .map(|device| device.display_name.clone())
        .or_else(|| {
            state
                .selected_receiver_device_id
                .as_ref()
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "none".to_string())
}

fn audio_source_kind_label(kind: AudioSourceKind) -> &'static str {
    match kind {
        AudioSourceKind::Monitor => "Monitor",
        AudioSourceKind::Microphone => "Microphone",
        AudioSourceKind::Application => "Application",
    }
}

fn audio_source_icon_name(kind: AudioSourceKind) -> &'static str {
    match kind {
        AudioSourceKind::Monitor => "audio-card-symbolic",
        AudioSourceKind::Microphone => "audio-input-microphone-symbolic",
        AudioSourceKind::Application => "application-x-executable-symbolic",
    }
}

fn playback_target_icon_name(target: &PlaybackTarget) -> &'static str {
    if target.is_bluetooth() {
        "bluetooth-active-symbolic"
    } else {
        "audio-speakers-symbolic"
    }
}

fn status_icon_name(status: synchrosonic_core::DeviceStatus) -> &'static str {
    match status {
        synchrosonic_core::DeviceStatus::Discovered => "network-wireless-signal-excellent-symbolic",
        synchrosonic_core::DeviceStatus::Connecting => "network-transmit-receive-symbolic",
        synchrosonic_core::DeviceStatus::Connected => "object-select-symbolic",
        synchrosonic_core::DeviceStatus::Unavailable => "dialog-warning-symbolic",
    }
}

fn diagnostic_icon_name(level: DiagnosticLevel) -> &'static str {
    match level {
        DiagnosticLevel::Info => "dialog-information-symbolic",
        DiagnosticLevel::Warning => "dialog-warning-symbolic",
        DiagnosticLevel::Error => "dialog-error-symbolic",
    }
}

fn diagnostic_level_label(level: DiagnosticLevel) -> &'static str {
    match level {
        DiagnosticLevel::Info => "Info",
        DiagnosticLevel::Warning => "Warning",
        DiagnosticLevel::Error => "Error",
    }
}

fn log_icon_name(level: &str) -> &'static str {
    match level {
        "ERROR" => "dialog-error-symbolic",
        "WARN" => "dialog-warning-symbolic",
        _ => "text-x-log-symbolic",
    }
}

fn format_log_entry_subtitle(entry: &StructuredLogEntry) -> String {
    let field_summary = if entry.fields.is_empty() {
        String::new()
    } else {
        let summary = entry
            .fields
            .iter()
            .filter(|(key, _)| key.as_str() != "message")
            .take(3)
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ");
        if summary.is_empty() {
            String::new()
        } else {
            format!(" [{summary}]")
        }
    };

    format!(
        "{} · {}{}",
        format_unix_ms(entry.timestamp_unix_ms),
        entry.message,
        field_summary
    )
}

fn format_unix_ms(timestamp_unix_ms: u64) -> String {
    format!("{timestamp_unix_ms} ms")
}

fn clear_list_box(list_box: &gtk::ListBox) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
}

fn apply_color_scheme(prefer_dark_theme: bool) {
    let style_manager = adw::StyleManager::default();
    style_manager.set_color_scheme(if prefer_dark_theme {
        adw::ColorScheme::PreferDark
    } else {
        adw::ColorScheme::Default
    });
}
