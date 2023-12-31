use std::io;
use std::string;

use config as config_crate;
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::PyTracebackMethods;
use pyo3::{PyErr, Python};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

use crate::model::{AliasId, SessionId, TimerId, TriggerId};

#[derive(Debug, Error)]
pub enum Error {
    #[error("not connected")]
    NotConnected,

    #[error("unexpected I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("unexpected internal error: {0}")]
    Internal(String),

    #[error("CLI error: {0}")]
    Cli(String),

    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("styling text: {0}")]
    Text(#[from] ansi_to_tui::Error),

    #[error("trigger error: {0}")]
    Trigger(#[from] TriggerError),

    #[error("alias error: {0}")]
    Alias(#[from] AliasError),

    #[error("timer error: {0}")]
    Timer(#[from] TimerError),

    #[error("layout missing required section named {0:?}")]
    LayoutMissing(String),

    #[error("layout section names must be unique, found {0:?} more than once")]
    DuplicateLayout(String),

    #[error("invalid layout section")]
    BadLayout,

    #[error("gmcp error: {0}")]
    Gmcp(#[from] GmcpError),

    #[error("python error: {error}\n{traceback}")]
    Python {
        #[source]
        error: PyErr,
        traceback: String,
    },

    #[error("unknown session: {0}")]
    UnknownSession(SessionId),
}

impl Error {
    /// Returns true if the application should terminate after displaying the error.
    #[must_use]
    pub fn fatal(&self) -> bool {
        // Fatal error examples:
        //   Internal errors.
        matches!(self, Self::Internal(_))
    }
}

// We implement From<PyErr> by hand in order to always collect a traceback.
impl From<PyErr> for Error {
    fn from(error: PyErr) -> Self {
        Python::with_gil(|py| {
            let traceback = error
                .traceback_bound(py)
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

impl From<SessionId> for Error {
    fn from(id: SessionId) -> Self {
        Error::UnknownSession(id)
    }
}

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum ConfigError {
    #[error("watching configuration file for changes: {0}")]
    Watch(#[from] notify::Error),

    #[error("deserializing configuration TOML content: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("serializing configuration TOML content: {0}")]
    TomlEdit(#[from] toml_edit::TomlError),

    #[error("{0}")]
    General(#[from] config_crate::ConfigError),

    #[error("invalid MUD server: {0}")]
    InvalidMud(String),

    #[error("config for MUD server {0} missing from config file")]
    MissingMud(String),

    #[error("configuring logging: {0}")]
    Logging(String),
}

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum TriggerError {
    #[error("invalid trigger regex pattern: {0}")]
    Pattern(#[from] regex::Error),

    #[error("unknown trigger ID: {0}")]
    UnknownId(TriggerId),
}

impl From<TriggerId> for TriggerError {
    fn from(id: TriggerId) -> Self {
        TriggerError::UnknownId(id)
    }
}

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum AliasError {
    #[error("invalid alias regex pattern: {0}")]
    Pattern(#[from] regex::Error),

    #[error("unknown alias ID: {0}")]
    UnknownId(AliasId),
}

impl From<AliasId> for AliasError {
    fn from(id: AliasId) -> Self {
        AliasError::UnknownId(id)
    }
}

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum TimerError {
    #[error("unknown timer ID: {0}")]
    UnknownId(TimerId),
}

impl From<TimerId> for TimerError {
    fn from(id: TimerId) -> Self {
        TimerError::UnknownId(id)
    }
}

#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum GmcpError {
    #[error("bad GMCP subnegotiation data: {0}")]
    BadEncoding(#[from] string::FromUtf8Error),

    #[error("bad GMCP subnegotiation data: {0}")]
    BadData(String),

    #[error("error encoding or decoding JSON GMCP data: {0}")]
    BadJson(#[from] serde_json::Error),

    #[error("GMCP is not negotiated as ready yet")]
    NotReady,
}
