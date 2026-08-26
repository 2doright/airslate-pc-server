use std::path::PathBuf;

use thiserror::Error;
#[cfg(windows)]
use windows::core::Error as WindowsError;

use crate::protocol::ProtocolError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("failed to read or write filesystem state: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config at {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize config: {0}")]
    ConfigSerialize(#[from] toml::ser::Error),
    #[error("{0} is not set")]
    MissingConfigBase(&'static str),
    #[error("startup invariant failed: {0}")]
    Startup(&'static str),
    #[error("workspace invariant failed: {0}")]
    Workspace(String),
    #[error("session already active")]
    SessionAlreadyActive,
    #[error(
        "an active wired session must be disconnected before wired connections can be disabled"
    )]
    WiredSessionActive,
    #[error("wired connections are disabled")]
    WiredConnectionDisabled,
    #[error("invalid session id: {0}")]
    InvalidSessionId(&'static str),
    #[error("shared state is poisoned: {0}")]
    StatePoisoned(&'static str),
    #[error("protocol operation failed: {0}")]
    Protocol(#[from] ProtocolError),
    #[cfg(windows)]
    #[error("Windows API call failed: {0}")]
    Windows(#[from] WindowsError),
    #[error("desktop shell failed: {0}")]
    DesktopShell(String),
    #[cfg(target_os = "macos")]
    #[error(
        "macOS Accessibility permission is required to inject input; grant AirSlate PC Server permission and restart the application"
    )]
    MacosInputPermissionDenied,
    #[error("shortcut preset failed: {0}")]
    ShortcutPreset(String),
    #[cfg(target_os = "macos")]
    #[error("shortcut key {key} is not supported on {platform}")]
    UnsupportedShortcutKey {
        platform: &'static str,
        key: &'static str,
    },
}
