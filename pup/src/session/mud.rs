use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};

use pyo3::{pyclass, pymethods};
use serde::{Deserialize, Serialize};
use tokio_util::bytes::Bytes;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[pyclass(frozen)]
#[allow(clippy::unsafe_derive_deserialize)] // No constructor invariants to uphold.
pub(crate) struct Mud {
    #[pyo3(get)]
    pub(crate) name: String,

    #[pyo3(get)]
    pub(crate) host: String,

    #[pyo3(get)]
    pub(crate) port: u16,

    /// Whether TLS was used for the connection. See `Tls`.
    #[pyo3(get)]
    #[serde(default)]
    pub(crate) tls: Tls,

    /// Whether TCP keepalives are configured.
    #[pyo3(get)]
    #[serde(default)]
    pub(crate) no_tcp_keepalive: bool,

    /// The command separator to use when sending multiple commands in a single message.
    #[serde(default = "default::command_separator")]
    #[pyo3(get)]
    pub command_separator: Option<String>,
}

#[pymethods]
impl Mud {
    #[new]
    #[pyo3(signature = (name, host, port, tls = None, no_tcp_keepalive = None, command_separator = None))]
    fn new(
        name: String,
        host: String,
        port: u16,
        tls: Option<Tls>,
        no_tcp_keepalive: Option<bool>,
        command_separator: Option<String>,
    ) -> Self {
        Self {
            name,
            host,
            port,
            tls: tls.unwrap_or_default(),
            no_tcp_keepalive: no_tcp_keepalive.unwrap_or_default(),
            command_separator: command_separator.or(default::command_separator()),
        }
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }
}

impl Display for Mud {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}:{})", self.name, self.host, self.port)
    }
}

/// Possible TLS states for a `MUD`.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[pyclass(eq, eq_int)]
#[allow(clippy::unsafe_derive_deserialize)] // No constructor invariants to uphold.
pub enum Tls {
    #[default]
    Disabled,
    Enabled,
    InsecureSkipVerify,
}

#[pyclass]
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub(crate) struct MudLine {
    pub raw: Bytes,

    // TODO(XXX): optimization opportunity: compact flags repr.
    #[pyo3(get, set)]
    pub prompt: bool,

    #[pyo3(get, set)]
    pub gag: bool,
}

impl MudLine {
    pub fn to_str(&self) -> Cow<str> {
        String::from_utf8_lossy(&self.raw)
    }
}

#[pymethods]
impl MudLine {
    pub(crate) fn stripped(&self) -> String {
        strip_ansi_escapes::strip_str(self.to_str())
    }

    pub(crate) fn set(&mut self, value: &str) {
        self.raw = Bytes::copy_from_slice(value.as_bytes());
    }

    #[new]
    fn new(value: &[u8]) -> Self {
        Self {
            raw: Bytes::copy_from_slice(value),
            prompt: false,
            gag: false,
        }
    }

    fn __str__(&self) -> String {
        self.to_str().to_string()
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    fn raw(&self) -> Vec<u8> {
        self.raw.to_vec()
    }
}

impl Display for MudLine {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_str())
    }
}

impl From<&Bytes> for MudLine {
    fn from(value: &Bytes) -> Self {
        Self::new(value)
    }
}

// 🤷 https://github.com/serde-rs/serde/issues/368
mod default {
    #[allow(clippy::unnecessary_wraps)] // Matching config field.
    pub(super) fn command_separator() -> Option<String> {
        Some(";;".to_string())
    }
}
