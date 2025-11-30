use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::str::Chars;
use std::{iter, mem};

use pyo3::{Py, PyErr, Python, pyclass, pymethods};
use strum::Display;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::keyboard::{KeyCode, KeyEvent};
use crate::python;
use crate::shortcut::InputShortcut;

#[derive(Debug, Clone)]
#[pyclass]
pub(crate) struct Input {
    session: u32,
    line: InputLine,
    echo: EchoState,
    cursor: usize,
    #[pyo3(get, set)]
    markup: Py<Markup>,
    py_event_tx: UnboundedSender<(u32, python::Event)>,
}

impl Input {
    pub(crate) fn new(
        py: Python<'_>,
        session: u32,
        py_event_tx: UnboundedSender<(u32, python::Event)>,
    ) -> Result<Self, PyErr> {
        Ok(Self {
            session,
            line: InputLine::default(),
            echo: EchoState::default(),
            cursor: 0,
            markup: Py::new(py, Markup::default())?,
            py_event_tx,
        })
    }

    pub(crate) fn value_changed(&self) {
        let input = Python::attach(|_| self.clone());
        let _ = self.py_event_tx.send((
            self.session,
            python::Event::InputChanged {
                line: self.line.clone(),
                input,
            },
        ));
    }

    pub(crate) fn key_event(&mut self, key_event: &KeyEvent) {
        let KeyEvent {
            code: KeyCode::Char(c),
            ..
        } = key_event
        else {
            return;
        };
        self.insert(*c);
    }

    pub(crate) fn shortcut(&mut self, shortcut: &InputShortcut) {
        match shortcut {
            InputShortcut::Send => {}
            InputShortcut::CursorLeft => {
                self.cursor_left();
            }
            InputShortcut::CursorRight => {
                self.cursor_right();
            }
            InputShortcut::CursorToStart => {
                self.cursor_start();
            }
            InputShortcut::CursorToEnd => {
                self.cursor_end();
            }
            InputShortcut::CursorWordLeft => {
                self.cursor_word_left();
            }
            InputShortcut::CursorWordRight => {
                self.cursor_word_right();
            }
            InputShortcut::DeletePrev => {
                self.delete_prev();
            }
            InputShortcut::DeleteNext => {
                self.delete_next();
            }
            InputShortcut::CursorDeleteWordLeft => {
                self.delete_word_left();
            }
            InputShortcut::CursorDeleteWordRight => {
                self.delete_word_right();
            }
            InputShortcut::CursorDeleteToEnd => {
                self.delete_to_end();
            }
            InputShortcut::Reset => {
                self.reset();
            }
        }
    }

    fn words_left(&self) -> impl Iterator<Item = char> + '_ {
        self.chars()
            .rev()
            .skip(self.chars().count().max(self.cursor) - self.cursor)
            .skip_while(|c| !c.is_alphanumeric())
            .skip_while(|c| c.is_alphanumeric())
    }

    fn drop_index(&mut self, index: usize) {
        self.line.sent = self
            .chars()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, c)| c)
            .collect();
    }

    fn chars(&self) -> Chars<'_> {
        self.line.sent.chars()
    }
}

#[pymethods]
impl Input {
    #[must_use]
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub(crate) fn visual_cursor(&self) -> usize {
        if self.cursor == 0 {
            return 0;
        }

        // Unwrap safe because the end index is internal, and kept within bounds
        let s = &self.line.sent;
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
    pub(crate) fn visual_scroll(&self, width: usize) -> usize {
        let mut uscroll = 0;

        let sent = &self.line.sent;
        while uscroll < self.visual_cursor().max(width) - width {
            match sent.chars().next() {
                Some(c) => {
                    uscroll += UnicodeWidthChar::width(c).unwrap_or(0);
                }
                None => break,
            }
        }
        uscroll
    }

    pub(crate) fn reset(&mut self) {
        self.line.sent.clear();
        self.line.original = None;
        self.line.echo = EchoState::default();
        self.cursor = 0;
        self.value_changed();
    }

    pub(crate) fn pop(&mut self) -> Option<InputLine> {
        if self.line.sent.is_empty() {
            return None;
        }

        self.cursor = 0;

        let result = Some(InputLine {
            sent: mem::take(&mut self.line.sent),
            // Reset the current echo state back to the telnet-level state.
            echo: mem::replace(&mut self.line.echo, self.echo),
            original: None,
            scripted: false,
        });
        self.value_changed();
        result
    }

    #[must_use]
    pub(crate) fn echo(&self) -> EchoState {
        self.echo
    }

    pub(crate) fn value(&self) -> InputLine {
        self.line.clone()
    }

    pub(crate) fn set_value(&mut self, value: InputLine) {
        self.line = value;
        self.cursor = self.line.sent.chars().count();
        self.value_changed();
    }

    pub(crate) fn set_telnet_echo(&mut self, echo: EchoState) {
        info!("set {echo}");
        // Save the telnet echo state.
        self.echo = echo;
        // and update the in-progress line to match.
        self.line.echo = echo;
    }

    pub(crate) fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.chars().count());
    }

    pub(crate) fn insert(&mut self, c: char) {
        if self.cursor == self.chars().count() {
            self.line.sent.push(c);
        } else {
            self.line.sent = self
                .chars()
                .take(self.cursor)
                .chain(iter::once(c).chain(self.chars().skip(self.cursor)))
                .collect();
        }
        self.cursor += 1;
        self.value_changed();
    }

    pub(crate) fn delete_prev(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.drop_index(self.cursor);
        self.value_changed();
    }

    pub(crate) fn delete_next(&mut self) {
        if self.cursor == self.chars().count() {
            return;
        }
        self.drop_index(self.cursor);
        self.value_changed();
    }

    pub(crate) fn delete_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let rev = self.words_left().collect::<Vec<_>>();
        let rev_len = rev.len();
        self.line.sent = rev
            .into_iter()
            .rev()
            .chain(self.chars().skip(self.cursor))
            .collect();
        self.cursor = rev_len;
        self.value_changed();
    }

    pub(crate) fn delete_word_right(&mut self) {
        if self.cursor == self.chars().count() {
            return;
        }
        self.line.sent = self
            .chars()
            .take(self.cursor)
            .chain(
                self.chars()
                    .skip(self.cursor)
                    .skip_while(|c| c.is_alphanumeric())
                    .skip_while(|c| !c.is_alphanumeric()),
            )
            .collect();
        self.value_changed();
    }

    pub(crate) fn delete_to_end(&mut self) {
        self.line.sent = self.chars().take(self.cursor).collect();
        self.value_changed();
    }

    pub(crate) fn cursor_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
    }

    pub(crate) fn cursor_right(&mut self) {
        if self.cursor == self.chars().count() {
            return;
        }
        self.cursor += 1;
    }

    pub(crate) fn cursor_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.words_left().count();
    }

    pub(crate) fn cursor_word_right(&mut self) {
        if self.cursor == self.chars().count() {
            return;
        }
        self.cursor = self
            .chars()
            .enumerate()
            .skip(self.cursor)
            .skip_while(|(_, c)| c.is_alphanumeric())
            .find(|(_, c)| c.is_alphanumeric())
            .map_or_else(|| self.chars().count(), |(i, _)| i);
    }

    pub(crate) fn cursor_start(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn cursor_end(&mut self) {
        self.cursor = self.chars().count();
    }

    pub(crate) fn markup(&self, py: Python<'_>) -> Py<Markup> {
        self.markup.clone_ref(py)
    }

    #[must_use]
    pub(crate) fn decorated_value(&self, py: Python<'_>) -> String {
        let content_str = match (&self.line.sent, &self.line.original) {
            (s, _) if !s.is_empty() => s,
            (_, Some(orig)) => orig,
            _ => return String::new(),
        };

        let content_str = match self.line.echo {
            EchoState::Password => "*".repeat(content_str.chars().count()),
            EchoState::Normal => content_str.clone(),
        };

        let char_count = content_str.chars().count();

        let markup = self.markup.borrow(py);
        if markup.tokens.is_empty() {
            return content_str;
        }

        let total_tokens_len = markup.tokens.values().map(String::len).sum::<usize>();
        let mut result = String::with_capacity(char_count + total_tokens_len);

        let mut chars = content_str.chars();
        let mut current_pos = 0;

        for (pos, token) in &markup.tokens {
            let pos = *pos;
            if pos > char_count {
                break;
            }

            while current_pos < pos {
                if let Some(c) = chars.next() {
                    result.push(c);
                }
                current_pos += 1;
            }
            result.push_str(token);
        }

        result.extend(chars);
        result
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }
}

impl Display for Input {
    // TODO(XXX): apply markup to line?
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.echo {
            EchoState::Password => f.write_str(&"*".repeat(self.line.sent.len())),
            EchoState::Normal => f.write_str(&self.line.to_string()),
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
                    original: Some(self.sent.clone()),
                    echo: self.echo,
                    scripted: self.scripted,
                })
            })
            .collect()
    }

    // Clones the InputLine, but replaces sent with original.
    fn clone_with_original(&self) -> Self {
        Self {
            sent: self.original.clone().unwrap_or_default(),
            original: Some(self.sent.clone()),
            echo: self.echo,
            scripted: self.scripted,
        }
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
        if self.echo == EchoState::Password {
            return f.write_str(&"*".repeat(self.sent.len()));
        }

        write!(f, "{}", &self.sent)
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
