use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub timestamp_unix_ms: u64,
    pub level: DiagnosticLevel,
    pub component: String,
    pub message: String,
}

impl DiagnosticEvent {
    pub fn info(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            timestamp_unix_ms: current_unix_ms(),
            level: DiagnosticLevel::Info,
            component: component.into(),
            message: message.into(),
        }
    }

    pub fn warning(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            timestamp_unix_ms: current_unix_ms(),
            level: DiagnosticLevel::Warning,
            component: component.into(),
            message: message.into(),
        }
    }

    pub fn error(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            timestamp_unix_ms: current_unix_ms(),
            level: DiagnosticLevel::Error,
            component: component.into(),
            message: message.into(),
        }
    }
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
