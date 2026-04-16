use std::{
    backtrace::Backtrace,
    collections::VecDeque,
    fs,
    io::{self, Read, Write},
    panic::{self, PanicHookInfo},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, Once, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use flate2::{write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use synchrosonic_core::{AppConfig, AppState, DiagnosticEvent, DiagnosticLevel};
use tar::{Builder, Header};

use crate::{
    logging::{read_log_tail, LogStore, StructuredLogEntry},
    metadata,
    persistence::AppPaths,
};

const RECOVERY_LOG_LIMIT: usize = 200;
const SNAPSHOT_PERSIST_INTERVAL_MS: u64 = 2_000;
const MAX_RECENT_WARNINGS_ERRORS: usize = 12;

static PANIC_RUNTIME: OnceLock<DiagnosticsRuntime> = OnceLock::new();
static PANIC_HOOK_ONCE: Once = Once::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedConfigSummary {
    pub schema_version: u32,
    pub audio: RedactedAudioConfigSummary,
    pub discovery: RedactedDiscoveryConfigSummary,
    pub transport: RedactedTransportConfigSummary,
    pub receiver: RedactedReceiverConfigSummary,
    pub ui: RedactedUiConfigSummary,
    pub diagnostics: RedactedDiagnosticsConfigSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedAudioConfigSummary {
    pub has_preferred_source_id: bool,
    pub local_playback_enabled: bool,
    pub has_local_playback_target_id: bool,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sample_format: String,
    pub capture_buffer_frames: u32,
    pub capture_latency_ms: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedDiscoveryConfigSummary {
    pub enabled: bool,
    pub service_type: String,
    pub stale_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedTransportConfigSummary {
    pub bind_host: String,
    pub stream_port: u16,
    pub quality_preset: String,
    pub target_latency_ms: u16,
    pub connect_timeout_ms: u16,
    pub heartbeat_interval_ms: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedReceiverConfigSummary {
    pub enabled: bool,
    pub start_on_launch: bool,
    pub has_advertised_name: bool,
    pub bind_host: String,
    pub listen_port: u16,
    pub has_playback_target_id: bool,
    pub latency_preset: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedUiConfigSummary {
    pub prefer_dark_theme: bool,
    pub last_view_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedDiagnosticsConfigSummary {
    pub verbose_logging: bool,
}

impl From<&AppConfig> for RedactedConfigSummary {
    fn from(config: &AppConfig) -> Self {
        Self {
            schema_version: config.schema_version,
            audio: RedactedAudioConfigSummary {
                has_preferred_source_id: config.audio.preferred_source_id.is_some(),
                local_playback_enabled: config.audio.local_playback_enabled,
                has_local_playback_target_id: config.audio.local_playback_target_id.is_some(),
                sample_rate_hz: config.audio.sample_rate_hz,
                channels: config.audio.channels,
                sample_format: format!("{:?}", config.audio.sample_format),
                capture_buffer_frames: config.audio.capture_buffer_frames,
                capture_latency_ms: config.audio.capture_latency_ms,
            },
            discovery: RedactedDiscoveryConfigSummary {
                enabled: config.discovery.enabled,
                service_type: config.discovery.service_type.clone(),
                stale_timeout_secs: config.discovery.stale_timeout_secs,
            },
            transport: RedactedTransportConfigSummary {
                bind_host: config.transport.bind_host.clone(),
                stream_port: config.transport.stream_port,
                quality_preset: format!("{:?}", config.transport.quality),
                target_latency_ms: config.transport.target_latency_ms,
                connect_timeout_ms: config.transport.connect_timeout_ms,
                heartbeat_interval_ms: config.transport.heartbeat_interval_ms,
            },
            receiver: RedactedReceiverConfigSummary {
                enabled: config.receiver.enabled,
                start_on_launch: config.receiver.start_on_launch,
                has_advertised_name: !config.receiver.advertised_name.trim().is_empty(),
                bind_host: config.receiver.bind_host.clone(),
                listen_port: config.receiver.listen_port,
                has_playback_target_id: config.receiver.playback_target_id.is_some(),
                latency_preset: format!("{:?}", config.receiver.latency_preset),
            },
            ui: RedactedUiConfigSummary {
                prefer_dark_theme: config.ui.prefer_dark_theme,
                last_view_name: config.ui.last_view_name.clone(),
            },
            diagnostics: RedactedDiagnosticsConfigSummary {
                verbose_logging: config.diagnostics.verbose_logging,
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollSnapshot {
    pub interval_secs: u64,
    pub runs: u64,
    pub skips: u64,
    pub last_started_at_unix_ms: Option<u64>,
    pub last_duration_ms: Option<u64>,
    pub last_changes_applied: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSubsystemSnapshot {
    pub source_poll: PollSnapshot,
    pub playback_target_poll: PollSnapshot,
    pub source_count: usize,
    pub playback_target_count: usize,
    pub active_source_id: Option<String>,
    pub local_playback_target_id: Option<String>,
    pub receiver_playback_target_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownDeviceSnapshot {
    pub id: String,
    pub display_name: String,
    pub status: String,
    pub availability: String,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverySubsystemSnapshot {
    pub poll: PollSnapshot,
    pub service_state: String,
    pub discovered_device_count: usize,
    pub local_device_id: Option<String>,
    pub advertisement_state: String,
    pub advertised_endpoint: Option<String>,
    pub last_advertised_unix_ms: Option<u64>,
    pub last_advertisement_error: Option<String>,
    pub known_devices: Vec<KnownDeviceSnapshot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverSubsystemSnapshot {
    pub poll: PollSnapshot,
    pub runtime_state: String,
    pub listener_active: bool,
    pub listener_bind_addr: Option<String>,
    pub listener_port: Option<u16>,
    pub last_runtime_error: Option<String>,
    pub last_transport_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingSubsystemSnapshot {
    pub poll: PollSnapshot,
    pub state: String,
    pub target_session_count: usize,
    pub active_target_count: usize,
    pub local_mirror_state: String,
    pub last_transport_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSubsystemSnapshot {
    pub current_view_name: String,
    pub refresh_requests: u64,
    pub refresh_applied: u64,
    pub refresh_skipped: u64,
    pub list_rebuilds: u64,
    pub list_skips: u64,
    pub last_rendered_row_count: usize,
    pub browser_join_active: bool,
    pub browser_join_url: Option<String>,
    pub browser_join_requests_served: u64,
    pub browser_join_last_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsystemSnapshot {
    pub updated_at_unix_ms: u64,
    pub last_crash_recovery_unix_ms: Option<u64>,
    pub audio: AudioSubsystemSnapshot,
    pub discovery: DiscoverySubsystemSnapshot,
    pub receiver: ReceiverSubsystemSnapshot,
    pub streaming: StreamingSubsystemSnapshot,
    pub ui: UiSubsystemSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollKind {
    Discovery,
    Receiver,
    Streaming,
    AudioSources,
    PlaybackTargets,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollObservation {
    pub interval_secs: u64,
    pub duration_ms: u64,
    pub skipped: bool,
    pub changes_applied: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SessionMarker {
    pub app_version: String,
    pub release_channel: String,
    pub pid: u32,
    pub started_at_unix_ms: u64,
    pub os: String,
    pub kernel: String,
    pub config_path: String,
    pub state_path: String,
}

#[derive(Debug, Clone)]
pub struct DiagnosticsRuntime {
    inner: Arc<DiagnosticsRuntimeInner>,
}

#[derive(Debug)]
struct DiagnosticsRuntimeInner {
    paths: AppPaths,
    log_store: LogStore,
    config_summary: Mutex<RedactedConfigSummary>,
    pending_events: Mutex<VecDeque<DiagnosticEvent>>,
    snapshot_state: Mutex<SnapshotPersistenceState>,
}

#[derive(Debug, Clone)]
struct SnapshotPersistenceState {
    current: SubsystemSnapshot,
    last_persisted: Option<SubsystemSnapshot>,
    last_persisted_at_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub struct RuntimeStateView {
    pub current_view_name: String,
    pub discovery_started: bool,
}

#[derive(Debug, Clone)]
pub struct CrashReportHandle {
    pub path: PathBuf,
    pub summary: String,
}

impl DiagnosticsRuntime {
    pub fn bootstrap(
        paths: &AppPaths,
        config: &AppConfig,
        log_store: LogStore,
    ) -> Result<(Self, Vec<DiagnosticEvent>), io::Error> {
        paths.ensure_state_layout()?;

        let mut startup_events = Vec::new();
        let mut snapshot = load_snapshot(&paths.subsystem_snapshot_path).unwrap_or_default();
        let config_summary = RedactedConfigSummary::from(config);
        let recovered_report = recover_previous_session(paths, &config_summary)?;
        if let Some(report) = recovered_report {
            snapshot.last_crash_recovery_unix_ms = Some(now_unix_ms());
            startup_events.push(DiagnosticEvent::warning(
                "diagnostics",
                format!(
                    "Recovered an abnormal termination report at {}.",
                    report.path.display()
                ),
            ));
            tracing::warn!(
                crash_report = %report.path.display(),
                summary = %report.summary,
                "recovered abnormal termination details from a stale session marker"
            );
        }

        let runtime = Self {
            inner: Arc::new(DiagnosticsRuntimeInner {
                paths: paths.clone(),
                log_store,
                config_summary: Mutex::new(config_summary),
                pending_events: Mutex::new(VecDeque::new()),
                snapshot_state: Mutex::new(SnapshotPersistenceState {
                    current: snapshot,
                    last_persisted: None,
                    last_persisted_at_unix_ms: 0,
                }),
            }),
        };
        runtime.persist_snapshot(true)?;
        runtime.start_session()?;

        Ok((runtime, startup_events))
    }

    pub fn install_panic_hook(&self) {
        let runtime = self.clone();
        let _ = PANIC_RUNTIME.set(runtime.clone());
        PANIC_HOOK_ONCE.call_once(move || {
            let default_hook = panic::take_hook();
            panic::set_hook(Box::new(move |panic_info| {
                if let Some(runtime) = PANIC_RUNTIME.get() {
                    if let Err(error) = runtime.write_panic_report(panic_info) {
                        eprintln!("failed to write SynchroSonic panic report: {error}");
                    }
                }
                default_hook(panic_info);
            }));
        });
    }

    pub fn update_config_summary(&self, config: &AppConfig) {
        if let Ok(mut summary) = self.inner.config_summary.lock() {
            *summary = RedactedConfigSummary::from(config);
        }
    }

    pub fn update_snapshot_from_state(&self, state: &AppState, view: &RuntimeStateView) {
        if let Ok(mut snapshot_state) = self.inner.snapshot_state.lock() {
            snapshot_state.current.updated_at_unix_ms = now_unix_ms();
            snapshot_state.current.audio.source_count = state.audio_sources.len();
            snapshot_state.current.audio.playback_target_count = state.playback_targets.len();
            snapshot_state.current.audio.active_source_id = state.selected_audio_source_id.clone();
            snapshot_state.current.audio.local_playback_target_id =
                state.config.audio.local_playback_target_id.clone();
            snapshot_state.current.audio.receiver_playback_target_id =
                state.config.receiver.playback_target_id.clone();
            snapshot_state.current.discovery.service_state = if view.discovery_started {
                "running".to_string()
            } else {
                "failed".to_string()
            };
            snapshot_state.current.discovery.discovered_device_count =
                state.discovered_devices.len();
            snapshot_state.current.discovery.known_devices = state
                .discovered_devices
                .iter()
                .map(|device| KnownDeviceSnapshot {
                    id: device.id.to_string(),
                    display_name: device.display_name.clone(),
                    status: format!("{:?}", device.status),
                    availability: format!("{:?}", device.availability),
                    endpoint: device
                        .endpoint
                        .as_ref()
                        .map(|endpoint| endpoint.address.to_string()),
                })
                .collect();
            snapshot_state.current.receiver.runtime_state = format!("{:?}", state.receiver.state);
            snapshot_state.current.receiver.last_runtime_error = state.receiver.last_error.clone();
            snapshot_state.current.streaming.state = format!("{:?}", state.streaming.state);
            snapshot_state.current.streaming.target_session_count = state.streaming.targets.len();
            snapshot_state.current.streaming.active_target_count =
                state.streaming.active_target_count();
            snapshot_state.current.streaming.local_mirror_state =
                format!("{:?}", state.streaming.local_mirror.state);
            snapshot_state.current.streaming.last_transport_error =
                state.streaming.last_error.clone();
            snapshot_state.current.ui.current_view_name = view.current_view_name.clone();
        }

        if let Err(error) = self.persist_snapshot(false) {
            tracing::warn!(error = %error, "failed to persist subsystem snapshot");
        }
    }

    pub fn update_receiver_listener_snapshot(
        &self,
        bind_addr: String,
        listener_active: bool,
        last_transport_error: Option<String>,
    ) {
        if let Ok(mut snapshot_state) = self.inner.snapshot_state.lock() {
            let port = bind_addr
                .rsplit_once(':')
                .and_then(|(_, port)| port.parse::<u16>().ok());
            snapshot_state.current.updated_at_unix_ms = now_unix_ms();
            snapshot_state.current.receiver.listener_bind_addr = Some(bind_addr);
            snapshot_state.current.receiver.listener_port = port;
            snapshot_state.current.receiver.listener_active = listener_active;
            snapshot_state.current.receiver.last_transport_error = last_transport_error;
        }

        if let Err(error) = self.persist_snapshot(false) {
            tracing::warn!(error = %error, "failed to persist receiver listener snapshot");
        }
    }

    pub fn update_discovery_local_snapshot(
        &self,
        service_state: String,
        local_device_id: Option<String>,
        advertisement_state: String,
        advertised_endpoint: Option<String>,
        last_advertised_unix_ms: Option<u64>,
        last_advertisement_error: Option<String>,
    ) {
        if let Ok(mut snapshot_state) = self.inner.snapshot_state.lock() {
            snapshot_state.current.updated_at_unix_ms = now_unix_ms();
            snapshot_state.current.discovery.service_state = service_state;
            snapshot_state.current.discovery.local_device_id = local_device_id;
            snapshot_state.current.discovery.advertisement_state = advertisement_state;
            snapshot_state.current.discovery.advertised_endpoint = advertised_endpoint;
            snapshot_state.current.discovery.last_advertised_unix_ms = last_advertised_unix_ms;
            snapshot_state.current.discovery.last_advertisement_error = last_advertisement_error;
        }

        if let Err(error) = self.persist_snapshot(false) {
            tracing::warn!(error = %error, "failed to persist discovery local snapshot");
        }
    }

    pub fn note_poll(&self, poll_kind: PollKind, observation: PollObservation) {
        if let Ok(mut snapshot_state) = self.inner.snapshot_state.lock() {
            let poll = match poll_kind {
                PollKind::Discovery => &mut snapshot_state.current.discovery.poll,
                PollKind::Receiver => &mut snapshot_state.current.receiver.poll,
                PollKind::Streaming => &mut snapshot_state.current.streaming.poll,
                PollKind::AudioSources => &mut snapshot_state.current.audio.source_poll,
                PollKind::PlaybackTargets => &mut snapshot_state.current.audio.playback_target_poll,
            };
            poll.interval_secs = observation.interval_secs;
            poll.last_started_at_unix_ms =
                Some(now_unix_ms().saturating_sub(observation.duration_ms));
            poll.last_duration_ms = Some(observation.duration_ms);
            poll.last_changes_applied = observation.changes_applied;
            poll.last_error = observation.error.clone();
            if observation.skipped {
                poll.skips = poll.skips.saturating_add(1);
            } else {
                poll.runs = poll.runs.saturating_add(1);
            }
        }

        if let Err(error) = self.persist_snapshot(false) {
            tracing::warn!(error = %error, "failed to persist poll snapshot");
        }
    }

    pub fn note_ui_refresh(&self, applied: bool) {
        if let Ok(mut snapshot_state) = self.inner.snapshot_state.lock() {
            snapshot_state.current.updated_at_unix_ms = now_unix_ms();
            snapshot_state.current.ui.refresh_requests =
                snapshot_state.current.ui.refresh_requests.saturating_add(1);
            if applied {
                snapshot_state.current.ui.refresh_applied =
                    snapshot_state.current.ui.refresh_applied.saturating_add(1);
            } else {
                snapshot_state.current.ui.refresh_skipped =
                    snapshot_state.current.ui.refresh_skipped.saturating_add(1);
            }
        }

        if let Err(error) = self.persist_snapshot(false) {
            tracing::warn!(error = %error, "failed to persist ui refresh metrics");
        }
    }

    pub fn note_ui_list_render(&self, rebuilt: bool, row_count: usize) {
        if let Ok(mut snapshot_state) = self.inner.snapshot_state.lock() {
            snapshot_state.current.updated_at_unix_ms = now_unix_ms();
            snapshot_state.current.ui.last_rendered_row_count = row_count;
            if rebuilt {
                snapshot_state.current.ui.list_rebuilds =
                    snapshot_state.current.ui.list_rebuilds.saturating_add(1);
            } else {
                snapshot_state.current.ui.list_skips =
                    snapshot_state.current.ui.list_skips.saturating_add(1);
            }
        }

        if let Err(error) = self.persist_snapshot(false) {
            tracing::warn!(error = %error, "failed to persist ui list render metrics");
        }
    }

    pub fn update_browser_join_snapshot(
        &self,
        active: bool,
        join_url: Option<String>,
        requests_served: u64,
        last_error: Option<String>,
    ) {
        if let Ok(mut snapshot_state) = self.inner.snapshot_state.lock() {
            snapshot_state.current.updated_at_unix_ms = now_unix_ms();
            snapshot_state.current.ui.browser_join_active = active;
            snapshot_state.current.ui.browser_join_url = join_url;
            snapshot_state.current.ui.browser_join_requests_served = requests_served;
            snapshot_state.current.ui.browser_join_last_error = last_error;
        }

        if let Err(error) = self.persist_snapshot(false) {
            tracing::warn!(error = %error, "failed to persist browser join snapshot");
        }
    }

    pub fn queue_diagnostic_event(&self, event: DiagnosticEvent) {
        if let Ok(mut events) = self.inner.pending_events.lock() {
            events.push_back(event);
        }
    }

    pub fn drain_pending_events(&self) -> Vec<DiagnosticEvent> {
        self.inner
            .pending_events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn snapshot(&self) -> SubsystemSnapshot {
        self.inner
            .snapshot_state
            .lock()
            .map(|state| state.current.clone())
            .unwrap_or_default()
    }

    pub fn build_compact_diagnostics_report(&self, state: &AppState, paths: &AppPaths) -> String {
        let snapshot = self.snapshot();
        let config_summary = self
            .inner
            .config_summary
            .lock()
            .map(|summary| summary.clone())
            .unwrap_or_else(|_| RedactedConfigSummary::from(&state.config));
        let recent_warnings_errors = state
            .diagnostics
            .iter()
            .rev()
            .filter(|event| event.level != DiagnosticLevel::Info)
            .take(MAX_RECENT_WARNINGS_ERRORS)
            .map(format_diagnostic_event)
            .collect::<Vec<_>>();
        let known_devices = if snapshot.discovery.known_devices.is_empty() {
            "none".to_string()
        } else {
            snapshot
                .discovery
                .known_devices
                .iter()
                .map(|device| {
                    format!(
                        "{} [{} / {}] {}",
                        device.display_name,
                        device.status,
                        device.availability,
                        device.endpoint.as_deref().unwrap_or("unresolved")
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        };

        let maintainer = metadata::authors_display();

        format!(
            concat!(
                "App: {} {} ({})\n",
                "Developer / Maintainer: {}\n",
                "Repository: {}\n",
                "Issues: {}\n",
                "Releases: {}\n",
                "OS: {} / {}\n",
                "Config path: {}\n",
                "Log path: {}\n",
                "Crash reports path: {}\n",
                "Discovery status: {} ({} device(s))\n",
                "Discovery advertisement: {} on {}\n",
                "UI refreshes: requested={} applied={} skipped={} list_rebuilds={} list_skips={}\n",
                "Browser join prototype: active={} url={} served={} error={}\n",
                "Receiver status: {} on {}\n",
                "Sender status: {} with {} active target(s)\n",
                "Active capture source: {}\n",
                "Active local playback target: {}\n",
                "Active receiver playback target: {}\n",
                "Known discovered devices: {}\n",
                "Recent warnings/errors: {}\n",
                "Safe config summary: {}"
            ),
            metadata::APP_NAME,
            metadata::APP_VERSION,
            metadata::release_channel_label(),
            maintainer,
            metadata::APP_REPOSITORY,
            metadata::APP_BUG_TRACKER,
            metadata::APP_RELEASES,
            std::env::consts::OS,
            system_kernel_label(),
            paths.config_path.display(),
            paths.log_path.display(),
            paths.crash_reports_dir.display(),
            snapshot.discovery.service_state,
            snapshot.discovery.discovered_device_count,
            snapshot.discovery.advertisement_state,
            snapshot
                .discovery
                .advertised_endpoint
                .as_deref()
                .unwrap_or("not advertised"),
            snapshot.ui.refresh_requests,
            snapshot.ui.refresh_applied,
            snapshot.ui.refresh_skipped,
            snapshot.ui.list_rebuilds,
            snapshot.ui.list_skips,
            snapshot.ui.browser_join_active,
            snapshot
                .ui
                .browser_join_url
                .as_deref()
                .unwrap_or("not generated"),
            snapshot.ui.browser_join_requests_served,
            snapshot
                .ui
                .browser_join_last_error
                .as_deref()
                .unwrap_or("none"),
            snapshot.receiver.runtime_state,
            snapshot
                .receiver
                .listener_bind_addr
                .as_deref()
                .unwrap_or("not listening"),
            snapshot.streaming.state,
            snapshot.streaming.active_target_count,
            snapshot.audio.active_source_id.as_deref().unwrap_or("none"),
            snapshot
                .audio
                .local_playback_target_id
                .as_deref()
                .unwrap_or("system default"),
            snapshot
                .audio
                .receiver_playback_target_id
                .as_deref()
                .unwrap_or("system default"),
            known_devices,
            if recent_warnings_errors.is_empty() {
                "none".to_string()
            } else {
                recent_warnings_errors.join(" | ")
            },
            serde_json::to_string(&config_summary).unwrap_or_else(|_| "{}".to_string())
        )
    }

    // The export bundle is intentionally limited to tar.gz so we can stay in-process
    // without shelling out to external archivers or adding a heavier dependency set.
    pub fn export_diagnostic_bundle(&self, state: &AppState) -> Result<PathBuf, io::Error> {
        self.persist_snapshot(true)?;
        let bundle_path = self.inner.paths.diagnostic_bundles_dir.join(format!(
            "diagnostic-bundle-{}.tar.gz",
            now_unix_ms() / 1_000
        ));
        let file = fs::File::create(&bundle_path)?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let snapshot = self.snapshot();
        let config_summary = self
            .inner
            .config_summary
            .lock()
            .map(|summary| summary.clone())
            .unwrap_or_else(|_| RedactedConfigSummary::from(&state.config));
        let logs = read_log_tail(&self.inner.paths.log_path, RECOVERY_LOG_LIMIT)?;
        let latest_crash_report = self.latest_crash_report_path().and_then(read_text_file);
        let environment_summary = serde_json::json!({
            "app_name": metadata::APP_NAME,
            "app_version": metadata::APP_VERSION,
            "release_channel": metadata::release_channel_label(),
            "developer": metadata::authors_display(),
            "repository_url": metadata::APP_REPOSITORY,
            "issues_url": metadata::APP_BUG_TRACKER,
            "releases_url": metadata::APP_RELEASES,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "kernel": system_kernel_label(),
            "config_path": self.inner.paths.config_path,
            "state_path": self.inner.paths.state_dir,
            "log_path": self.inner.paths.log_path,
            "crash_reports_dir": self.inner.paths.crash_reports_dir,
            "diagnostics_dir": self.inner.paths.diagnostics_dir,
        });

        append_bytes(
            &mut builder,
            "environment-summary.json",
            serde_json::to_vec_pretty(&environment_summary)?,
        )?;
        append_bytes(
            &mut builder,
            "config-summary.json",
            serde_json::to_vec_pretty(&config_summary)?,
        )?;
        append_bytes(
            &mut builder,
            "diagnostics-events.json",
            serde_json::to_vec_pretty(&state.diagnostics)?,
        )?;
        append_bytes(
            &mut builder,
            "subsystem-snapshot.json",
            serde_json::to_vec_pretty(&snapshot)?,
        )?;
        append_bytes(
            &mut builder,
            "structured-logs.json",
            serde_json::to_vec_pretty(&logs)?,
        )?;
        append_bytes(
            &mut builder,
            "diagnostics.txt",
            self.build_compact_diagnostics_report(state, &self.inner.paths)
                .into_bytes(),
        )?;
        if let Some(latest_crash_report) = latest_crash_report {
            append_bytes(
                &mut builder,
                "latest-crash-report.txt",
                latest_crash_report.into_bytes(),
            )?;
        }

        let encoder = builder.into_inner()?;
        encoder.finish()?;
        Ok(bundle_path)
    }

    pub fn latest_crash_report_path(&self) -> Option<PathBuf> {
        latest_file_in_dir(&self.inner.paths.crash_reports_dir)
    }

    pub fn latest_crash_report_text(&self) -> io::Result<Option<String>> {
        Ok(self.latest_crash_report_path().and_then(read_text_file))
    }

    pub fn mark_clean_shutdown(&self) -> Result<(), io::Error> {
        self.persist_snapshot(true)?;
        match fs::remove_file(&self.inner.paths.session_marker_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn start_session(&self) -> Result<(), io::Error> {
        let marker = SessionMarker {
            app_version: metadata::APP_VERSION.to_string(),
            release_channel: metadata::release_channel_label().to_string(),
            pid: std::process::id(),
            started_at_unix_ms: now_unix_ms(),
            os: std::env::consts::OS.to_string(),
            kernel: system_kernel_label(),
            config_path: self.inner.paths.config_path.display().to_string(),
            state_path: self.inner.paths.state_dir.display().to_string(),
        };
        write_json_atomic(&self.inner.paths.session_marker_path, &marker)
    }

    fn persist_snapshot(&self, force: bool) -> Result<(), io::Error> {
        let mut guard = self
            .inner
            .snapshot_state
            .lock()
            .map_err(|_| io::Error::other("snapshot mutex poisoned"))?;
        let now = now_unix_ms();
        let changed = guard.last_persisted.as_ref() != Some(&guard.current);
        let due = force
            || changed
            || now.saturating_sub(guard.last_persisted_at_unix_ms) >= SNAPSHOT_PERSIST_INTERVAL_MS;
        if !due {
            return Ok(());
        }
        write_json_atomic(&self.inner.paths.subsystem_snapshot_path, &guard.current)?;
        guard.last_persisted = Some(guard.current.clone());
        guard.last_persisted_at_unix_ms = now;
        Ok(())
    }

    fn write_panic_report(&self, panic_info: &PanicHookInfo<'_>) -> Result<PathBuf, io::Error> {
        let message = panic_payload_message(panic_info);
        let location = panic_info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown".to_string());
        let thread_name = std::thread::current()
            .name()
            .map(ToString::to_string)
            .unwrap_or_else(|| "unnamed".to_string());
        let snapshot = self.snapshot();
        let logs = self
            .inner
            .log_store
            .snapshot_recent_chronological(RECOVERY_LOG_LIMIT);
        let config_summary = self
            .inner
            .config_summary
            .lock()
            .map(|summary| summary.clone())
            .unwrap_or_else(|_| RedactedConfigSummary::from(&AppConfig::default()));
        let report_body = render_panic_report(
            &message,
            &location,
            &thread_name,
            &snapshot,
            &config_summary,
            &logs,
        );
        let path = self
            .inner
            .paths
            .crash_reports_dir
            .join(format!("crash-report-{}-panic.txt", now_unix_ms() / 1_000));
        write_text_atomic(&path, &report_body)?;
        self.queue_diagnostic_event(DiagnosticEvent::error(
            "panic",
            format!(
                "A panic was captured on thread {thread_name}; crash report written to {}.",
                path.display()
            ),
        ));
        Ok(path)
    }
}

fn recover_previous_session(
    paths: &AppPaths,
    config_summary: &RedactedConfigSummary,
) -> Result<Option<CrashReportHandle>, io::Error> {
    if !paths.session_marker_path.exists() {
        return Ok(None);
    }

    let marker_bytes = fs::read(&paths.session_marker_path)?;
    let marker = serde_json::from_slice::<SessionMarker>(&marker_bytes).ok();
    let snapshot = load_snapshot(&paths.subsystem_snapshot_path).unwrap_or_default();
    let logs = read_log_tail(&paths.log_path, RECOVERY_LOG_LIMIT)?;
    let report =
        render_abnormal_termination_report(marker.as_ref(), config_summary, &snapshot, &logs);
    let path = paths.crash_reports_dir.join(format!(
        "crash-report-{}-recovered.txt",
        now_unix_ms() / 1_000
    ));
    write_text_atomic(&path, &report)?;
    fs::remove_file(&paths.session_marker_path)?;

    Ok(Some(CrashReportHandle {
        summary: marker
            .as_ref()
            .map(|marker| {
                format!(
                    "Recovered stale session marker from PID {} started at {}.",
                    marker.pid, marker.started_at_unix_ms
                )
            })
            .unwrap_or_else(|| {
                "Recovered a stale session marker, but its contents were unreadable.".to_string()
            }),
        path,
    }))
}

fn load_snapshot(path: &Path) -> Option<SubsystemSnapshot> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SubsystemSnapshot>(&bytes).ok())
}

fn render_abnormal_termination_report(
    marker: Option<&SessionMarker>,
    config_summary: &RedactedConfigSummary,
    snapshot: &SubsystemSnapshot,
    logs: &[StructuredLogEntry],
) -> String {
    let marker_summary = marker
        .map(|marker| {
            format!(
                "Previous session\n  PID: {}\n  Started: {}\n  OS: {}\n  Kernel: {}\n  Config path: {}\n  State path: {}\n",
                marker.pid,
                marker.started_at_unix_ms,
                marker.os,
                marker.kernel,
                marker.config_path,
                marker.state_path
            )
        })
        .unwrap_or_else(|| {
            "Previous session\n  Marker contents were unreadable, but the session did not close cleanly.\n".to_string()
        });

    format!(
        concat!(
            "SynchroSonic abnormal termination recovered\n",
            "Generated: {}\n",
            "App version: {} ({})\n",
            "Developer / Maintainer: {}\n",
            "Repository: {}\n",
            "Issues: {}\n",
            "Build target: {} / {}\n\n",
            "{}\n",
            "Last known subsystem snapshot\n{}\n\n",
            "Safe config summary\n{}\n\n",
            "Recent structured logs\n{}"
        ),
        now_unix_ms(),
        metadata::APP_VERSION,
        metadata::release_channel_label(),
        metadata::authors_display(),
        metadata::APP_REPOSITORY,
        metadata::APP_BUG_TRACKER,
        std::env::consts::OS,
        std::env::consts::ARCH,
        marker_summary,
        serde_json::to_string_pretty(snapshot).unwrap_or_else(|_| "{}".to_string()),
        serde_json::to_string_pretty(config_summary).unwrap_or_else(|_| "{}".to_string()),
        format_logs(logs)
    )
}

fn render_panic_report(
    message: &str,
    location: &str,
    thread_name: &str,
    snapshot: &SubsystemSnapshot,
    config_summary: &RedactedConfigSummary,
    logs: &[StructuredLogEntry],
) -> String {
    format!(
        concat!(
            "SynchroSonic panic report\n",
            "Generated: {}\n",
            "App version: {} ({})\n",
            "Developer / Maintainer: {}\n",
            "Repository: {}\n",
            "Issues: {}\n",
            "Thread: {}\n",
            "Location: {}\n",
            "Message: {}\n",
            "Backtrace:\n{}\n\n",
            "Recent subsystem snapshot\n{}\n\n",
            "Safe config summary\n{}\n\n",
            "Recent structured logs\n{}"
        ),
        now_unix_ms(),
        metadata::APP_VERSION,
        metadata::release_channel_label(),
        metadata::authors_display(),
        metadata::APP_REPOSITORY,
        metadata::APP_BUG_TRACKER,
        thread_name,
        location,
        message,
        Backtrace::force_capture(),
        serde_json::to_string_pretty(snapshot).unwrap_or_else(|_| "{}".to_string()),
        serde_json::to_string_pretty(config_summary).unwrap_or_else(|_| "{}".to_string()),
        format_logs(logs)
    )
}

fn append_bytes(
    builder: &mut Builder<GzEncoder<fs::File>>,
    path: &str,
    bytes: Vec<u8>,
) -> Result<(), io::Error> {
    let mut header = Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, path, bytes.as_slice())?;
    Ok(())
}

fn latest_file_in_dir(path: &Path) -> Option<PathBuf> {
    let mut entries = fs::read_dir(path)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(system_time_to_unix_ms);
            (modified.unwrap_or(0), entry.path())
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries.pop().map(|(_, path)| path)
}

fn read_text_file(path: PathBuf) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    Some(contents)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), io::Error> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| io::Error::other(error.to_string()))?;
    write_bytes_atomic(path, &bytes)
}

fn write_text_atomic(path: &Path, contents: &str) -> Result<(), io::Error> {
    write_bytes_atomic(path, contents.as_bytes())
}

fn write_bytes_atomic(path: &Path, contents: &[u8]) -> Result<(), io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("write")
    ));
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, path)?;
    Ok(())
}

fn panic_payload_message(info: &PanicHookInfo<'_>) -> String {
    if let Some(message) = info.payload().downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn format_logs(logs: &[StructuredLogEntry]) -> String {
    if logs.is_empty() {
        return "(no structured logs were available)".to_string();
    }

    logs.iter()
        .map(|entry| {
            format!(
                "- {} {} {} {} {}",
                entry.timestamp_unix_ms,
                entry.level,
                entry.target,
                entry.message,
                if entry.fields.is_empty() {
                    "".to_string()
                } else {
                    serde_json::to_string(&entry.fields).unwrap_or_else(|_| "{}".to_string())
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_diagnostic_event(event: &DiagnosticEvent) -> String {
    format!(
        "{} {} {}",
        event.timestamp_unix_ms, event.component, event.message
    )
}

fn system_kernel_label() -> String {
    let ostype = fs::read_to_string("/proc/sys/kernel/ostype")
        .ok()
        .map(|value| value.trim().to_string());
    let osrelease = fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|value| value.trim().to_string());
    match (ostype, osrelease) {
        (Some(ostype), Some(osrelease)) => format!("{ostype} {osrelease}"),
        _ => Command::new("uname")
            .arg("-sr")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unknown kernel".to_string()),
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn system_time_to_unix_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(root: &Path) -> AppPaths {
        AppPaths {
            config_dir: root.join("config"),
            state_dir: root.join("state"),
            config_path: root.join("config").join("config.toml"),
            portable_config_path: root.join("config").join("config-export.toml"),
            log_path: root.join("state").join("app-log.jsonl"),
            diagnostics_dir: root.join("state").join("diagnostics"),
            crash_reports_dir: root.join("state").join("diagnostics").join("crash-reports"),
            diagnostic_bundles_dir: root.join("state").join("diagnostics").join("bundles"),
            session_marker_path: root
                .join("state")
                .join("diagnostics")
                .join("session-marker.json"),
            subsystem_snapshot_path: root
                .join("state")
                .join("diagnostics")
                .join("subsystem-snapshot.json"),
        }
    }

    fn sample_log(message: &str) -> StructuredLogEntry {
        StructuredLogEntry {
            timestamp_unix_ms: 1,
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: message.to_string(),
            fields: Default::default(),
            file: None,
            line: None,
        }
    }

    #[test]
    fn redacted_config_summary_omits_sensitive_ids() {
        let mut config = AppConfig::default();
        config.audio.preferred_source_id = Some("alsa_input.private".to_string());
        config.audio.local_playback_target_id = Some("bluez.private".to_string());
        config.receiver.playback_target_id = Some("speaker.private".to_string());
        config.receiver.advertised_name = "Office Receiver".to_string();

        let summary = RedactedConfigSummary::from(&config);

        assert!(summary.audio.has_preferred_source_id);
        assert!(summary.audio.has_local_playback_target_id);
        assert!(summary.receiver.has_playback_target_id);
        assert!(summary.receiver.has_advertised_name);
        let encoded = serde_json::to_string(&summary).expect("summary should serialize");
        assert!(!encoded.contains("alsa_input.private"));
        assert!(!encoded.contains("bluez.private"));
        assert!(!encoded.contains("speaker.private"));
        assert!(!encoded.contains("Office Receiver"));
    }

    #[test]
    fn session_marker_lifecycle_recovers_unclean_exit() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let paths = test_paths(temp.path());
        paths
            .ensure_state_layout()
            .expect("state layout should exist");
        let config = AppConfig::default();
        write_json_atomic(
            &paths.session_marker_path,
            &SessionMarker {
                app_version: metadata::APP_VERSION.to_string(),
                release_channel: metadata::release_channel_label().to_string(),
                pid: 42,
                started_at_unix_ms: 7,
                os: "linux".to_string(),
                kernel: "Linux 6.x".to_string(),
                config_path: paths.config_path.display().to_string(),
                state_path: paths.state_dir.display().to_string(),
            },
        )
        .expect("marker should write");
        write_text_atomic(
            &paths.log_path,
            &serde_json::to_string(&sample_log("before crash")).expect("log should serialize"),
        )
        .expect("log file should write");

        let recovered = recover_previous_session(&paths, &RedactedConfigSummary::from(&config))
            .expect("recovery should succeed")
            .expect("report should exist");

        assert!(!paths.session_marker_path.exists());
        let report = fs::read_to_string(recovered.path).expect("report should read");
        assert!(report.contains("abnormal termination recovered"));
        assert!(report.contains("before crash"));
    }

    #[test]
    fn panic_report_generation_writes_context() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let paths = test_paths(temp.path());
        let runtime =
            DiagnosticsRuntime::bootstrap(&paths, &AppConfig::default(), LogStore::new(16))
                .expect("runtime should initialize")
                .0;
        runtime.update_snapshot_from_state(
            &AppState::new(AppConfig::default()),
            &RuntimeStateView {
                current_view_name: "diagnostics".to_string(),
                discovery_started: true,
            },
        );

        let panic = panic::catch_unwind(|| panic!("boom"));
        assert!(panic.is_err());
        let payload = panic.unwrap_err();
        let message = if let Some(message) = payload.downcast_ref::<&str>() {
            (*message).to_string()
        } else {
            "boom".to_string()
        };
        let report_path = runtime.inner.paths.crash_reports_dir.join("panic-test.txt");
        write_text_atomic(
            &report_path,
            &render_panic_report(
                &message,
                "tests.rs:1",
                "main",
                &runtime.snapshot(),
                &RedactedConfigSummary::from(&AppConfig::default()),
                &[sample_log("panic trail")],
            ),
        )
        .expect("panic report should write");

        let report = fs::read_to_string(report_path).expect("panic report should read");
        assert!(report.contains("panic report"));
        assert!(report.contains("boom"));
        assert!(report.contains("panic trail"));
    }

    #[test]
    fn diagnostic_bundle_export_includes_expected_artifacts() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let paths = test_paths(temp.path());
        let store = LogStore::new(16);
        let runtime = DiagnosticsRuntime::bootstrap(&paths, &AppConfig::default(), store)
            .expect("runtime should initialize")
            .0;
        write_text_atomic(
            &paths.log_path,
            &serde_json::to_string(&sample_log("bundle log")).expect("log should serialize"),
        )
        .expect("log should write");
        write_text_atomic(
            &paths.crash_reports_dir.join("latest.txt"),
            "latest crash report",
        )
        .expect("crash report should write");
        let mut state = AppState::new(AppConfig::default());
        state
            .diagnostics
            .push(DiagnosticEvent::warning("tests", "bundle diagnostic"));

        let bundle_path = runtime
            .export_diagnostic_bundle(&state)
            .expect("bundle should export");

        let bundle_file = fs::File::open(bundle_path).expect("bundle should open");
        let decoder = flate2::read::GzDecoder::new(bundle_file);
        let mut archive = tar::Archive::new(decoder);
        let entries = archive
            .entries()
            .expect("archive entries should read")
            .filter_map(|entry| entry.ok())
            .map(|entry| {
                entry
                    .path()
                    .expect("path should exist")
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>();

        assert!(entries
            .iter()
            .any(|path| path == "environment-summary.json"));
        assert!(entries.iter().any(|path| path == "config-summary.json"));
        assert!(entries.iter().any(|path| path == "diagnostics-events.json"));
        assert!(entries.iter().any(|path| path == "structured-logs.json"));
        assert!(entries.iter().any(|path| path == "latest-crash-report.txt"));
    }
}
