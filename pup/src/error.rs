use std::io;

use config as config_crate;
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::PyTracebackMethods;
use pyo3::{PyErr, Python};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not connected")]
    NotConnected,

    #[error("unexpected I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("unexpected internal error: {0}")]
    Internal(String),

    #[error("python error: {error}\n{traceback}")]
    Python {
        #[source]
        error: PyErr,
        traceback: String,
    },

    #[error("no active session")]
    NoActiveSession,

    #[error("session ID {0} does not exist")]
    NoSuchSession(u32),

    #[error("MUD server with name '{0}' does not exist")]
    NoSuchMud(String),

    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("GMCP error: {0}")]
    Gmcp(#[from] GmcpError),

    #[error("invalid regex pattern: {0}")]
    InvalidRegex(#[from] regex::Error),

    #[error("keybinding error: {0}")]
    KeyBinding(#[from] KeyBindingError),
}

// We implement From<PyErr> by hand in order to always collect a traceback.
impl From<PyErr> for Error {
    fn from(error: PyErr) -> Self {
        Python::with_gil(|py| {
            let traceback = error
                .traceback(py)
                .and_then(|t| t.format().ok())
                .unwrap_or_default();
            Error::Python { error, traceback }
        })
    }
}

impl From<Error> for PyErr {
    fn from(err: Error) -> Self {
        // TODO(XXX): Consider concrete exception types per-error variant?
        PyRuntimeError::new_err(err.to_string())
    }
}

impl<T> From<SendError<T>> for Error {
    fn from(value: SendError<T>) -> Self {
        Self::Internal(format!("sending message: {value}"))
    }
}

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum ConfigError {
    #[error("deserializing TOML content: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("{0}")]
    General(#[from] config_crate::ConfigError),

    #[error("invalid MUD server: {0}")]
    InvalidMud(String),
}

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum GmcpError {
    #[error("message had invalid non-utf8 encoding")]
    InvalidEncoding,

    #[error("message was malformed")]
    Malformed,

    #[error("message payload was invalid JSON")]
    InvalidJson,
}

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum KeyBindingError {
    #[error("unknown input mode: {0:?}")]
    UnknownMode(String),

    #[error("unknown shortcut: {0:?}")]
    UnknownShortcut(String),

    #[error("invalid keybinding keys: {0}")]
    InvalidKeys(String),
}
