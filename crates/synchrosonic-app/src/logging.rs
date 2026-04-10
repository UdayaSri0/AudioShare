use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
use tracing_subscriber::{layer::Context, prelude::*, registry::LookupSpan, EnvFilter, Layer};

const DEFAULT_LOG_STORE_CAPACITY: usize = 400;
const DEFAULT_LOG_FILE_TAIL: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredLogEntry {
    pub timestamp_unix_ms: u64,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct LogStore {
    entries: Arc<Mutex<VecDeque<StructuredLogEntry>>>,
    capacity: usize,
}

#[derive(Debug, Clone)]
pub struct LoggingInit {
    pub store: LogStore,
    pub warnings: Vec<String>,
}

impl LogStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    pub fn snapshot_recent(&self, limit: usize) -> Vec<StructuredLogEntry> {
        self.entries
            .lock()
            .map(|entries| {
                entries
                    .iter()
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    fn push(&self, entry: StructuredLogEntry) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.push_back(entry);
            while entries.len() > self.capacity {
                entries.pop_front();
            }
        }
    }

    fn hydrate_from_file(&self, path: &Path, limit: usize) -> Result<(), std::io::Error> {
        if !path.exists() {
            return Ok(());
        }

        let reader = BufReader::new(File::open(path)?);
        let mut tail = VecDeque::with_capacity(limit);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<StructuredLogEntry>(&line) {
                tail.push_back(entry);
                while tail.len() > limit {
                    tail.pop_front();
                }
            }
        }

        if let Ok(mut entries) = self.entries.lock() {
            *entries = tail;
        }

        Ok(())
    }
}

impl Default for LogStore {
    fn default() -> Self {
        Self::new(DEFAULT_LOG_STORE_CAPACITY)
    }
}

pub fn init_logging(verbose_logging: bool, log_path: &Path) -> LoggingInit {
    let mut warnings = Vec::new();
    let store = LogStore::default();

    if let Err(error) = store.hydrate_from_file(log_path, DEFAULT_LOG_FILE_TAIL) {
        warnings.push(format!(
            "Previous structured logs at {} could not be loaded: {error}",
            log_path.display()
        ));
    }

    let log_file = match open_log_file(log_path) {
        Ok(file) => Some(Arc::new(Mutex::new(file))),
        Err(error) => {
            warnings.push(format!(
                "Structured log file {} could not be opened: {error}",
                log_path.display()
            ));
            None
        }
    };

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(if verbose_logging { "debug" } else { "info" }));
    let stdout_layer = tracing_subscriber::fmt::layer().with_target(true).compact();
    let structured_layer = StructuredLogLayer::new(store.clone(), log_file);
    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(structured_layer);

    if let Err(error) = subscriber.try_init() {
        warnings.push(format!(
            "Tracing subscriber could not be initialized: {error}"
        ));
    }

    LoggingInit { store, warnings }
}

fn open_log_file(path: &Path) -> Result<File, std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    OpenOptions::new().create(true).append(true).open(path)
}

#[derive(Clone)]
struct StructuredLogLayer {
    store: LogStore,
    log_file: Option<Arc<Mutex<File>>>,
}

impl StructuredLogLayer {
    fn new(store: LogStore, log_file: Option<Arc<Mutex<File>>>) -> Self {
        Self { store, log_file }
    }
}

impl<S> Layer<S> for StructuredLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = EventFieldVisitor::default();
        event.record(&mut visitor);

        let message = visitor
            .message
            .clone()
            .or_else(|| visitor.fields.get("message").cloned())
            .unwrap_or_else(|| metadata.name().to_string());
        let entry = StructuredLogEntry {
            timestamp_unix_ms: current_unix_ms(),
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message,
            fields: visitor.fields,
            file: metadata.file().map(ToString::to_string),
            line: metadata.line(),
        };

        self.store.push(entry.clone());
        if let Some(log_file) = &self.log_file {
            if let Ok(mut log_file) = log_file.lock() {
                if serde_json::to_writer(&mut *log_file, &entry).is_ok() {
                    let _ = log_file.write_all(b"\n");
                    let _ = log_file.flush();
                }
            }
        }
    }
}

#[derive(Default)]
struct EventFieldVisitor {
    message: Option<String>,
    fields: BTreeMap<String, String>,
}

impl EventFieldVisitor {
    fn record_value(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value.clone());
        }
        self.fields.insert(field.name().to_string(), value);
    }
}

impl Visit for EventFieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{value:?}"));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_value(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_value(field, value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_value(field, value.to_string());
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, value.to_string());
    }
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_store_retains_recent_entries() {
        let store = LogStore::new(2);
        store.push(StructuredLogEntry {
            timestamp_unix_ms: 1,
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "first".to_string(),
            fields: BTreeMap::new(),
            file: None,
            line: None,
        });
        store.push(StructuredLogEntry {
            timestamp_unix_ms: 2,
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "second".to_string(),
            fields: BTreeMap::new(),
            file: None,
            line: None,
        });
        store.push(StructuredLogEntry {
            timestamp_unix_ms: 3,
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "third".to_string(),
            fields: BTreeMap::new(),
            file: None,
            line: None,
        });

        let recent = store.snapshot_recent(10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].message, "third");
        assert_eq!(recent[1].message, "second");
    }
}
