use std::io;

use config as config_crate;
use pyo3::exceptions::PyRuntimeError;
use pyo3::pyclass::PyClassGuardError;
use pyo3::types::PyTracebackMethods;
use pyo3::{CastError, PyErr, Python};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tracing_error::SpanTrace;

#[derive(Debug, Error)]
#[error("error: {kind}\n{span}")]
pub struct Error {
    pub(crate) kind: Box<ErrorKind>,
    pub(crate) span: SpanTrace,
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::from(ErrorKind::from(err))
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
            kind: kind.into(),
            span: SpanTrace::capture(),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum ErrorKind {
    #[error("not connected")]
    NotConnected,

    #[error("unexpected I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("unexpected internal error: {0}")]
    Internal(String),

    #[error("invalid CLI arg: {0}")]
    Cli(String),

    #[error("python error: {error}\n{traceback}")]
    Python {
        #[source]
        error: PyErr,
        traceback: String,
    },

    #[error("python downcast error: {0}")]
    Downcast(String),

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

    #[error("unknown layout section name: {0}")]
    UnknownLayoutSection(String),

    #[error("duplicate layout section name: {0}")]
    DuplicateLayoutSection(String),

    #[error("invalid UTF-8 ANSI text: {0}")]
    Ansi(#[from] ansi_to_tui::Error),

    #[error("a non-empty name is required")]
    NameRequired,

    #[error("invalid tab id: {0}")]
    InvalidTabId(u32),

    #[error("tab ID {0} does not have a buffer with name {1}")]
    NoSuchBufferName(u32, String),

    #[error("invalid alias: {0}")]
    InvalidAlias(String),

    #[error("invalid trigger: {0}")]
    InvalidTrigger(String),

    #[error("invalid timer: {0}")]
    InvalidTimer(String),
}

// We implement From<PyErr> by hand in order to always collect a traceback.
impl From<PyErr> for Error {
    fn from(error: PyErr) -> Self {
        Python::attach(|py| {
            let traceback = error
                .traceback(py)
                .and_then(|t| t.format().ok())
                .unwrap_or_default();
            ErrorKind::Python { error, traceback }.into()
        })
    }
}

impl From<CastError<'_, '_>> for ErrorKind {
    fn from(value: CastError) -> Self {
        Self::Downcast(value.to_string())
    }
}

impl From<PyClassGuardError<'_, '_>> for ErrorKind {
    fn from(value: PyClassGuardError<'_, '_>) -> Self {
        Self::Downcast(value.to_string())
    }
}

impl From<Error> for PyErr {
    fn from(err: Error) -> Self {
        // TODO(XXX): Consider concrete exception types per-error variant?
        PyRuntimeError::new_err(err.to_string())
    }
}

impl<T> From<SendError<T>> for ErrorKind {
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

    #[error("invalid character in config: {0}")]
    InvalidCharacter(String),

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
