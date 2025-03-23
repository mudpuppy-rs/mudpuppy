use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::mem;
use pyo3::{pyclass, pymethods, Py, PyErr, Python};
use strum::Display;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone)]
#[pyclass]
pub(crate) struct Input {
    line: Py<InputLine>,
    echo: EchoState,
    cursor: usize,
    #[pyo3(get, set)]
    markup: Py<Markup>,
}

impl Input {
    pub(crate) fn new(py: Python<'_>) -> Result<Self, PyErr> {
        Ok(Self {
            line: Py::new(py, InputLine::default())?,
            echo: EchoState::default(),
            cursor: 0,
            markup: Py::new(py, Markup::default())?,
        })
    }

    /*
    error[E0515]: cannot return value referencing temporary value

    fn words_left<'py>(&'py self, py: Python<'py>) -> impl Iterator<Item = char> + '_ {
        let count = self.line.borrow(py).sent.chars().count().max(self.cursor);
        let chars = self.line.borrow(py).sent.chars().to_owned();

        chars
            .rev()
            .skip(count - self.cursor)
            .skip_while(|c| !c.is_alphanumeric())
            .skip_while(|c| c.is_alphanumeric())
    }*/
}

#[pymethods]
impl Input {
    #[must_use]
    pub(crate) fn value(&self, py: Python<'_>) -> Py<InputLine> {
        self.line.clone_ref(py)
    }

    #[must_use]
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub(crate) fn visual_cursor(&self, py: Python<'_>) -> usize {
        if self.cursor == 0 {
            return 0;
        }

        let s = &self.line.borrow(py).sent;

        // Unwrap safe because the end index is internal, and kept within bounds
        UnicodeWidthStr::width(
            s.get(
                0..s.char_indices()
                    .nth(self.cursor)
                    .map_or_else(|| s.len(), |(index, _)| index),
            )
                .unwrap(),
        )
    }

    #[must_use]
    pub(crate) fn visual_scroll(&self, py: Python<'_>, width: usize) -> usize {
        let mut uscroll = 0;

        let sent = &self.line.borrow(py).sent;
        while uscroll < self.visual_cursor(py).max(width) - width {
            match sent.chars().next() {
                Some(c) => {
                    uscroll += UnicodeWidthChar::width(c).unwrap_or(0);
                }
                None => break,
            }
        }
        uscroll
    }

    #[must_use]
    pub(crate) fn echo(&self) -> EchoState {
        self.echo
    }

    pub(crate) fn reset(&mut self, py: Python<'_>) {
        let mut line = self.line.borrow_mut(py);
        line.sent.clear();
        line.original = None;
        line.echo = EchoState::default();
        self.cursor = 0;
    }

    pub(crate) fn pop(&mut self, py: Python<'_>) -> Option<InputLine> {
        let mut line = self.line.borrow_mut(py);
        if line.sent.is_empty() {
            return None;
        }

        self.cursor = 0;

        Some(InputLine {
            sent: mem::take(&mut line.sent),
            // Reset the current echo state back to the telnet-level state.
            echo: mem::replace(&mut line.echo, self.echo),
            original: None,
            scripted: false,
        })
    }
}

impl Display for Input {
    // TODO(XXX): apply markup to line?
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.echo {
            EchoState::Password => {
                let len = Python::with_gil(|py|{
                    self.line.borrow(py).sent.len()
                });
                f.write_str(&"*".repeat(len))
            }
            EchoState::Normal => {
                f.write_str(&self.line.to_string())
            }
        }
    }
}

#[pyclass]
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub(crate) struct InputLine {
    #[pyo3(get, set)]
    pub(crate) sent: String,

    #[pyo3(get, set)]
    pub(crate) original: Option<String>,

    #[pyo3(get, set)]
    pub(crate) echo: EchoState,

    #[pyo3(get, set)]
    pub(crate) scripted: bool,
}

#[pymethods]
impl InputLine {
    #[new]
    #[pyo3(signature = (sent, original = None, echo = None, scripted = false))]
    #[must_use]
    pub(crate) fn new(
        sent: String,
        original: Option<String>,
        echo: Option<EchoState>,
        scripted: bool,
    ) -> Self {
        Self {
            sent,
            original,
            echo: echo.unwrap_or_default(),
            scripted,
        }
    }

    pub(crate) fn empty(&self) -> bool {
        self.sent.trim().is_empty()
    }

    pub(crate) fn split(&self, sep: &str) -> Vec<Self> {
        self.sent
            .split(sep)
            .filter_map(|fragment| {
                if fragment.trim().is_empty() {
                    return None;
                }
                Some(Self {
                    sent: fragment.to_string(),
                    original: None,
                    echo: self.echo,
                    scripted: self.scripted,
                })
            })
            .collect()
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for InputLine {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("> ")?;

        if self.echo == EchoState::Password {
            return f.write_str(&"*".repeat(self.sent.len()));
        }

        if let Some(original) = &self.original {
            write!(f, "{} ({})", &self.sent, original)
        } else {
            f.write_str(&self.sent)
        }
    }
}

#[pyclass(eq, eq_int)]
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, Display)]
pub(crate) enum EchoState {
    #[default]
    #[strum(to_string = "echo state: normal")]
    Normal,
    #[strum(to_string = "echo state: password")]
    Password,
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
impl EchoState {
    fn __str__(&self) -> String {
        self.to_string()
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Debug, Clone, Default)]
#[pyclass]
pub(crate) struct Markup {
    tokens: BTreeMap<usize, String>,
}

#[pymethods]
impl Markup {
    fn add(&mut self, pos: usize, token: String) {
        self.tokens.insert(pos, token);
    }

    fn remove(&mut self, pos: usize) {
        self.tokens.remove(&pos);
    }

    fn clear(&mut self) {
        self.tokens.clear();
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}
