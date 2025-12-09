use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};

use pyo3::{pyclass, pymethods};
use tokio_util::bytes::Bytes;

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
    pub fn to_str(&self) -> Cow<'_, str> {
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
