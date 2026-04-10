use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use synchrosonic_core::{AppConfig, ConfigError, ConfigLoadReport, DiagnosticEvent};

const APP_DIR_NAME: &str = "synchrosonic";
const CONFIG_FILE_NAME: &str = "config.toml";
const PORTABLE_CONFIG_FILE_NAME: &str = "config-export.toml";
const LOG_FILE_NAME: &str = "app-log.jsonl";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub config_path: PathBuf,
    pub portable_config_path: PathBuf,
    pub log_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StartupConfig {
    pub config: AppConfig,
    pub diagnostics: Vec<DiagnosticEvent>,
    pub paths: AppPaths,
}

impl AppPaths {
    pub fn resolve() -> Self {
        let config_dir = env::var_os("SYNCHROSONIC_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(default_config_dir)
            .join(APP_DIR_NAME);
        let state_dir = env::var_os("SYNCHROSONIC_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(default_state_dir)
            .join(APP_DIR_NAME);

        Self {
            config_path: config_dir.join(CONFIG_FILE_NAME),
            portable_config_path: config_dir.join(PORTABLE_CONFIG_FILE_NAME),
            log_path: state_dir.join(LOG_FILE_NAME),
            config_dir,
            state_dir,
        }
    }
}

pub fn load_startup_config(paths: AppPaths) -> StartupConfig {
    let mut diagnostics = Vec::new();

    match AppConfig::load_with_report_from_path(&paths.config_path) {
        Ok(report) => {
            let config = maybe_repair_saved_config(&paths, report, &mut diagnostics);
            StartupConfig {
                config,
                diagnostics,
                paths,
            }
        }
        Err(ConfigError::Read { source, .. }) if source.kind() == ErrorKind::NotFound => {
            let config = AppConfig::default();
            match save_active_config(&paths, &config) {
                Ok(path) => diagnostics.push(DiagnosticEvent::info(
                    "config",
                    format!(
                        "No saved configuration was found; created a default config at {}.",
                        path.display()
                    ),
                )),
                Err(error) => diagnostics.push(DiagnosticEvent::warning(
                    "config",
                    format!(
                        "No saved configuration was found; running with defaults, but saving {} failed: {error}",
                        paths.config_path.display()
                    ),
                )),
            }

            StartupConfig {
                config,
                diagnostics,
                paths,
            }
        }
        Err(error) => {
            let backup_path = backup_invalid_config(&paths.config_path).ok().flatten();
            let config = AppConfig::default();
            let save_result = save_active_config(&paths, &config);

            let backup_suffix = backup_path
                .as_ref()
                .map(|path| format!(" A backup was written to {}.", path.display()))
                .unwrap_or_default();
            diagnostics.push(DiagnosticEvent::warning(
                "config",
                format!(
                    "Saved configuration could not be used ({error}); falling back to defaults.{backup_suffix}"
                ),
            ));

            if let Err(save_error) = save_result {
                diagnostics.push(DiagnosticEvent::warning(
                    "config",
                    format!(
                        "Default configuration could not be re-saved to {}: {save_error}",
                        paths.config_path.display()
                    ),
                ));
            }

            StartupConfig {
                config,
                diagnostics,
                paths,
            }
        }
    }
}

pub fn save_active_config(paths: &AppPaths, config: &AppConfig) -> Result<PathBuf, ConfigError> {
    config.save_to_path(&paths.config_path)?;
    Ok(paths.config_path.clone())
}

pub fn export_config(paths: &AppPaths, config: &AppConfig) -> Result<PathBuf, ConfigError> {
    config.save_to_path(&paths.portable_config_path)?;
    Ok(paths.portable_config_path.clone())
}

pub fn import_config(paths: &AppPaths) -> Result<ConfigLoadReport, ConfigError> {
    AppConfig::load_with_report_from_path(&paths.portable_config_path)
}

fn maybe_repair_saved_config(
    paths: &AppPaths,
    report: ConfigLoadReport,
    diagnostics: &mut Vec<DiagnosticEvent>,
) -> AppConfig {
    let ConfigLoadReport {
        config,
        warnings,
        repaired,
    } = report;

    if repaired {
        diagnostics.push(DiagnosticEvent::warning(
            "config",
            format!(
                "Saved configuration at {} was repaired before use.",
                paths.config_path.display()
            ),
        ));
        for warning in &warnings {
            diagnostics.push(DiagnosticEvent::warning("config", warning.clone()));
        }
        if let Err(error) = save_active_config(paths, &config) {
            diagnostics.push(DiagnosticEvent::warning(
                "config",
                format!(
                    "Repaired configuration could not be written back to {}: {error}",
                    paths.config_path.display()
                ),
            ));
        }
    } else {
        diagnostics.push(DiagnosticEvent::info(
            "config",
            format!("Loaded configuration from {}.", paths.config_path.display()),
        ));
    }

    config
}

fn backup_invalid_config(path: &Path) -> Result<Option<PathBuf>, std::io::Error> {
    if !path.exists() {
        return Ok(None);
    }

    let backup_path = path.with_file_name(format!(
        "config.invalid-{}.toml",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    ));
    fs::rename(path, &backup_path)?;
    Ok(Some(backup_path))
}

fn default_config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_state_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn test_paths(root: &Path) -> AppPaths {
        let config_dir = root.join("config");
        let state_dir = root.join("state");
        AppPaths {
            config_path: config_dir.join(CONFIG_FILE_NAME),
            portable_config_path: config_dir.join(PORTABLE_CONFIG_FILE_NAME),
            log_path: state_dir.join(LOG_FILE_NAME),
            config_dir,
            state_dir,
        }
    }

    #[test]
    fn app_paths_respect_environment_overrides() {
        let _guard = ENV_LOCK.lock().expect("env lock should be available");
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let config_root = temp.path().join("config");
        let state_root = temp.path().join("state");
        let previous_config = env::var_os("SYNCHROSONIC_CONFIG_DIR");
        let previous_state = env::var_os("SYNCHROSONIC_STATE_DIR");
        env::set_var("SYNCHROSONIC_CONFIG_DIR", &config_root);
        env::set_var("SYNCHROSONIC_STATE_DIR", &state_root);

        let paths = AppPaths::resolve();
        assert!(paths.config_path.starts_with(config_root));
        assert!(paths.log_path.starts_with(state_root));

        if let Some(previous_config) = previous_config {
            env::set_var("SYNCHROSONIC_CONFIG_DIR", previous_config);
        } else {
            env::remove_var("SYNCHROSONIC_CONFIG_DIR");
        }
        if let Some(previous_state) = previous_state {
            env::set_var("SYNCHROSONIC_STATE_DIR", previous_state);
        } else {
            env::remove_var("SYNCHROSONIC_STATE_DIR");
        }
    }

    #[test]
    fn load_startup_config_creates_default_when_missing() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let paths = test_paths(temp.path());

        let startup = load_startup_config(paths.clone());

        assert_eq!(startup.config, AppConfig::default());
        assert!(startup.paths.config_path.exists());
        assert!(startup
            .diagnostics
            .iter()
            .any(|event| event.component == "config"
                && event.message.contains("created a default config")));
    }

    #[test]
    fn load_startup_config_backs_up_invalid_config_and_recovers_with_defaults() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let paths = test_paths(temp.path());
        fs::create_dir_all(&paths.config_dir).expect("config dir should exist");
        fs::write(&paths.config_path, "schema_version = 999\n")
            .expect("invalid config fixture should save");

        let startup = load_startup_config(paths.clone());

        assert_eq!(startup.config, AppConfig::default());
        assert!(startup.paths.config_path.exists());
        assert!(startup
            .diagnostics
            .iter()
            .any(|event| event.component == "config"
                && event.message.contains("falling back to defaults")));

        let backup_count = fs::read_dir(&paths.config_dir)
            .expect("config dir should be readable")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("config.invalid-")
            })
            .count();
        assert_eq!(backup_count, 1);
    }
}
