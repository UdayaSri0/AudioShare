use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub level: DiagnosticLevel,
    pub component: String,
    pub message: String,
}

impl DiagnosticEvent {
    pub fn info(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Info,
            component: component.into(),
            message: message.into(),
        }
    }

    pub fn warning(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            component: component.into(),
            message: message.into(),
        }
    }

    pub fn error(component: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            component: component.into(),
            message: message.into(),
        }
    }
}

