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
    #[error("audio operation is not active in this build phase: {0}")]
    NotActive(String),
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
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

