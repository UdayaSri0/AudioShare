use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Receiver(#[from] ReceiverError),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config at {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to parse config at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to serialize config: {0}")]
    Serialize(toml::ser::Error),
    #[error("failed to write config at {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("audio command is unavailable: {0}")]
    CommandUnavailable(String),
    #[error("audio command `{command}` failed: {stderr}")]
    CommandFailed { command: String, stderr: String },
    #[error("failed to start audio process `{command}`: {source}")]
    ProcessStart { command: String, source: io::Error },
    #[error("audio process I/O failed while {context}: {source}")]
    ProcessIo { context: String, source: io::Error },
    #[error("invalid capture settings: {0}")]
    InvalidSettings(String),
    #[error("capture stream ended")]
    CaptureEnded,
    #[error("audio operation is not active in this build phase: {0}")]
    NotActive(String),
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("failed to start discovery daemon: {0}")]
    Daemon(String),
    #[error("failed to build mDNS service info: {0}")]
    ServiceInfo(String),
    #[error("failed to register mDNS service: {0}")]
    Register(String),
    #[error("failed to browse mDNS service: {0}")]
    Browse(String),
    #[error("failed to stop discovery service: {0}")]
    Stop(String),
    #[error("failed to process discovery event: {0}")]
    Event(String),
    #[error("discovery is not active in this build phase: {0}")]
    NotActive(String),
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport is not active in this build phase: {0}")]
    NotActive(String),
}

#[derive(Debug, Error)]
pub enum ReceiverError {
    #[error("receiver mode is not active in this build phase: {0}")]
    NotActive(String),
}
