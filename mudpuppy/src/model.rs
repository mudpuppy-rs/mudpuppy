use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::time::Duration;

use crossterm::event::{
    MouseButton, MouseEvent as TermMouseEvent, MouseEventKind as TermMouseEventKind,
};
use pyo3::{pyclass, pymethods, Py, PyAny, PyObject, PyRef, Python};
use regex::Regex;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::sync::watch;
use tokio_util::bytes::Bytes;

use crate::client::input::EchoState;
use crate::error::{AliasError, Error, KeyBindingError, TriggerError};
use crate::idmap::{self};
use crate::net::telnet;

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[pyclass]
pub struct SessionInfo {
    #[pyo3(get)]
    pub id: u32,
    #[pyo3(get)]
    pub mud_name: String,
}

#[pymethods]
impl SessionInfo {
    fn __str__(&self) -> String {
        format!("{self}")
    }
}

impl Display for SessionInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Session({}, {})", self.id, self.mud_name)
    }
}

impl idmap::Identifiable for SessionInfo {
    fn id(&self) -> u32 {
        self.id
    }
}

/// MUD configuration.
///
/// Identified by a unique `name`. This type holds both information required to connect to a
/// MUD server (`host`, `port`, `tls`) alongside other per-session settings like whether to
/// hold prompt lines at the bottom of the screen, or to disable text wrapping.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[pyclass]
#[allow(clippy::unsafe_derive_deserialize)] // No constructor invariants to uphold.
#[allow(clippy::struct_excessive_bools)] // It's Fine.
pub struct Mud {
    #[pyo3(get)]
    pub name: String,

    #[pyo3(get)]
    pub host: String,

    #[pyo3(get)]
    pub port: u16,

    /// Whether TLS was used for the connection. See `Tls`.
    #[pyo3(get)]
    pub tls: Tls,

    /// Whether TCP keepalives are configured.
    #[serde(default = "default::no_tcp_keepalive")]
    #[pyo3(get)]
    pub no_tcp_keepalive: bool,

    /// Whether to hold the most recent prompt line at the bottom of the output buffer.
    ///
    /// You may want to disable this if prompt detection is not working correctly, or if
    /// you prefer prompts to be treated just like all other output.
    #[serde(default = "default::hold_prompt")]
    #[pyo3(get)]
    pub hold_prompt: bool,

    /// Whether input sent to the MUD is echoed in the output buffer.
    #[serde(default = "default::echo_input")]
    #[pyo3(get)]
    pub echo_input: bool,

    /// Whether output lines are wrapped when they would exceed the width of the output buffer.
    ///
    /// You may want to disable this if you prefer to see truncated, but accurately rendered,
    /// output (e.g. textual table information on a small screen).
    #[serde(default = "default::no_line_wrap")]
    #[pyo3(get)]
    pub no_line_wrap: bool,

    /// Whether to output received GMCP messages in the output buffer.
    #[serde(default = "default::debug_gmcp")]
    pub debug_gmcp: bool,

    /// The percentage of the screen to use for the "split view" for scrolling output history.
    #[serde(default = "default::splitview_percentage")]
    pub splitview_percentage: u16,

    /// The number of columns to use as margin on the sides of the "split view" for scrolling
    /// output history.
    #[serde(default = "default::splitview_margin_horizontal")]
    pub splitview_margin_horizontal: u16,

    /// The number of rows to use as margin on the top and bottom of the "split view" for scrolling
    /// output history.
    #[serde(default = "default::splitview_margin_vertical")]
    pub splitview_margin_vertical: u16,

    /// The command separator to use when sending multiple commands in a single message.
    #[serde(default = "default::command_separator")]
    pub command_separator: Option<String>,
}

impl Display for Mud {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}:{})", self.name, self.host, self.port)
    }
}

/// Possible TLS states for a `MUD`.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize,
)]
#[pyclass(eq, eq_int)]
#[allow(clippy::unsafe_derive_deserialize)] // No constructor invariants to uphold.
pub enum Tls {
    #[default]
    Disabled,
    Enabled,
    InsecureSkipVerify,
}

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, EnumString, Display,
)]
#[strum(ascii_case_insensitive)]
pub enum InputMode {
    MudList,
    #[default]
    MudSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[pyclass]
pub struct KeyEvent {
    pub(crate) code: KeyCode,
    pub(crate) modifiers: KeyModifiers,
}

impl KeyEvent {
    #[must_use]
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
impl KeyEvent {
    #[pyo3(name = "code")]
    fn get_code(&self) -> String {
        self.code.to_string()
    }

    #[pyo3(name = "modifiers")]
    fn get_modifiers(&self) -> Vec<String> {
        self.modifiers.modifiers()
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for KeyEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            self.modifiers
                .modifiers()
                .iter()
                .fold(String::new(), |mut output, m| {
                    output.push_str(m);
                    output.push('-');
                    output
                }),
            self.code
        )
    }
}

impl TryFrom<&str> for KeyEvent {
    type Error = KeyBindingError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let mut parts = s.split('-');
        let mut modifiers = KeyModifiers::NONE;
        let mut final_part = None;

        for part in parts.by_ref() {
            if let Some(modifier) = KeyModifiers::from_string(part) {
                modifiers.insert(modifier);
            } else {
                final_part = Some(part.to_lowercase());
                break;
            }
        }

        let code = KeyCode::try_from(final_part.as_deref().unwrap_or_default())
            .map_err(KeyBindingError::InvalidKeys)?;

        Ok(Self::new(code, modifiers))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    pub const NONE: Self = KeyModifiers(0);
    pub const SHIFT: Self = KeyModifiers(1);
    pub const CONTROL: Self = KeyModifiers(2);
    pub const ALT: Self = KeyModifiers(4);

    #[must_use]
    pub fn bits(&self) -> u8 {
        self.0
    }

    #[must_use]
    pub fn contains(&self, other: KeyModifiers) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn insert(&mut self, other: KeyModifiers) {
        self.0 |= other.0;
    }

    #[must_use]
    pub fn modifiers(&self) -> Vec<String> {
        let mut modifiers = Vec::new();

        if self.contains(Self::CONTROL) {
            modifiers.push("ctrl".to_string());
        }
        if self.contains(Self::SHIFT) {
            modifiers.push("shift".to_string());
        }
        if self.contains(Self::ALT) {
            modifiers.push("alt".to_string());
        }

        modifiers
    }

    #[must_use]
    pub fn from_string(str: &str) -> Option<KeyModifiers> {
        Some(match str.to_lowercase().as_str() {
            "ctrl" => KeyModifiers::CONTROL,
            "shift" => KeyModifiers::SHIFT,
            "alt" => KeyModifiers::ALT,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyCode {
    Char(char),
    F(u8),
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    Delete,
    Insert,
    Esc,
}

impl TryFrom<&str> for KeyCode {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(match value {
            v if v.starts_with('f') => {
                let num = v[1..]
                    .parse::<u8>()
                    .map_err(|_| format!("invalid F-key: {v}"))?;
                if (1..=12).contains(&num) {
                    Self::F(num)
                } else {
                    return Err(format!("invalid F-key: {v}"));
                }
            }
            "backspace" => Self::Backspace,
            "enter" => Self::Enter,
            "left" => Self::Left,
            "right" => Self::Right,
            "up" => Self::Up,
            "down" => Self::Down,
            "home" => Self::Home,
            "end" => Self::End,
            "pageup" => Self::PageUp,
            "pagedown" => Self::PageDown,
            "tab" => Self::Tab,
            "delete" => Self::Delete,
            "insert" => Self::Insert,
            "esc" => Self::Esc,
            c if c.len() == 1 => Self::Char(c.chars().next().unwrap()),
            _ => return Err(format!("unknown key code: {value:?}")),
        })
    }
}

impl Display for KeyCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                KeyCode::Char(c) => c.to_string(),
                KeyCode::F(n) => format!("f{n}"),
                KeyCode::Backspace => "backspace".to_string(),
                KeyCode::Enter => "enter".to_string(),
                KeyCode::Left => "left".to_string(),
                KeyCode::Right => "right".to_string(),
                KeyCode::Up => "up".to_string(),
                KeyCode::Down => "down".to_string(),
                KeyCode::Home => "home".to_string(),
                KeyCode::End => "end".to_string(),
                KeyCode::PageUp => "pageup".to_string(),
                KeyCode::PageDown => "pagedown".to_string(),
                KeyCode::Tab => "tab".to_string(),
                KeyCode::Delete => "delete".to_string(),
                KeyCode::Insert => "insert".to_string(),
                KeyCode::Esc => "esc".to_string(),
            }
        )
    }
}

#[pyclass]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    #[pyo3(get)]
    pub kind: MouseEventKind,

    #[pyo3(get)]
    pub column: u16,

    #[pyo3(get)]
    pub row: u16,

    pub modifiers: KeyModifiers,
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
impl MouseEvent {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    #[pyo3(name = "modifiers")]
    fn get_modifiers(&self) -> Vec<String> {
        self.modifiers.modifiers()
    }
}

impl TryFrom<TermMouseEvent> for MouseEvent {
    type Error = ();

    fn try_from(term_event: TermMouseEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: term_event.kind.try_into()?,
            column: term_event.column,
            row: term_event.row,
            modifiers: term_event.modifiers.into(),
        })
    }
}

impl Display for MouseEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let modifiers = if self.modifiers.0 == 0 {
            "no modifiers".to_string()
        } else {
            format!("modifiers: {}", self.modifiers.modifiers().join(", "))
        };
        write!(
            f,
            "{kind} at ({column}, {row}) with {modifiers}",
            kind = self.kind,
            column = self.column,
            row = self.row,
        )
    }
}

#[pyclass(eq, eq_int)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display)]
#[strum(ascii_case_insensitive)]
pub enum MouseEventKind {
    LeftButtonDown,
    RightButtonDown,
    MiddleButtonDown,
    Moved,
    ScrollDown,
    ScrollUp,
    ScrollLeft,
    ScrollRight,
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
impl MouseEventKind {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl TryFrom<TermMouseEventKind> for MouseEventKind {
    type Error = ();

    fn try_from(term_kind: TermMouseEventKind) -> Result<Self, Self::Error> {
        Ok(match term_kind {
            TermMouseEventKind::Down(MouseButton::Left) => Self::LeftButtonDown,
            TermMouseEventKind::Down(MouseButton::Right) => Self::RightButtonDown,
            TermMouseEventKind::Down(MouseButton::Middle) => Self::MiddleButtonDown,
            TermMouseEventKind::Up(_) | TermMouseEventKind::Drag(_) => return Err(()),
            TermMouseEventKind::Moved => Self::Moved,
            TermMouseEventKind::ScrollDown => Self::ScrollDown,
            TermMouseEventKind::ScrollUp => Self::ScrollUp,
            TermMouseEventKind::ScrollLeft => Self::ScrollLeft,
            TermMouseEventKind::ScrollRight => Self::ScrollRight,
        })
    }
}

#[pyclass(eq, eq_int)]
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display)]
#[strum(ascii_case_insensitive)]
pub enum Shortcut {
    Quit,

    TabNext,
    TabPrev,
    TabClose,
    TabSwapLeft,
    TabSwapRight,

    MudListNext,
    MudListPrev,
    MudListConnect,

    ToggleLineWrap,
    ToggleInputEcho,
    ToggleMouseMode,

    HistoryNext,
    HistoryPrevious,

    ScrollUp,
    ScrollDown,
    ScrollTop,
    ScrollBottom,
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
impl Shortcut {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl TryFrom<TermMouseEvent> for Shortcut {
    type Error = ();

    fn try_from(event: TermMouseEvent) -> Result<Self, Self::Error> {
        Ok(match event {
            TermMouseEvent {
                kind: TermMouseEventKind::ScrollUp,
                ..
            } => Self::ScrollUp,
            TermMouseEvent {
                kind: TermMouseEventKind::ScrollDown,
                ..
            } => Self::ScrollDown,
            _ => return Err(()),
        })
    }
}

#[pyclass]
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct MudLine {
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

impl Display for MudLine {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_str())
    }
}

impl From<Bytes> for MudLine {
    fn from(value: Bytes) -> Self {
        Self::new(&value)
    }
}

#[pymethods]
impl MudLine {
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

    fn raw(&self) -> Vec<u8> {
        self.raw.to_vec()
    }

    pub fn stripped(&self) -> String {
        strip_ansi_escapes::strip_str(self.to_str())
    }

    pub fn set(&mut self, value: &str) {
        self.raw = Bytes::copy_from_slice(value.as_bytes());
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[pyclass]
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct InputLine {
    #[pyo3(get)]
    pub sent: String,

    #[pyo3(get)]
    pub original: Option<String>,

    // TODO(XXX): compact flags repr
    #[pyo3(get)]
    pub echo: EchoState,

    #[pyo3(get)]
    pub scripted: bool,
}

#[pymethods]
impl InputLine {
    #[new]
    #[must_use]
    pub fn new(sent: String, echo: bool, scripted: bool) -> Self {
        Self {
            sent,
            original: None,
            echo: match echo {
                true => EchoState::Enabled,
                false => EchoState::Password,
            },
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.echo {
            EchoState::Enabled => f.write_str(&self.sent),
            EchoState::Password => f.write_str(&"*".repeat(self.sent.len())),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[pyclass]
pub enum PromptMode {
    // When dealing with a MUD that doesn't terminate prompts in some way we can end up with
    // data in the buffer after deframing that may or may not be a prompt.
    //
    // If it isn't a prompt, we expect to receive more data that will have a line ending Soon(TM).
    // If it is a prompt, we won't get anything else; the game sent something like "Enter username: "
    // and is expecting the player to act before it will send any more data. There's no way to tell
    // the two apart definitively so in this mode we use a heuristic: if we don't receive more data
    // and deframe a line before the Duration expires, consider what's in the buffer a prompt and flush
    // it as a received prompt line.
    Unsignalled { timeout: Duration },

    // Used for a MUD that signals prompts using EOR or GA.
    Signalled { signal: PromptSignal },
}

#[pymethods]
impl PromptMode {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    #[must_use]
    pub fn signal(&self) -> Option<PromptSignal> {
        match self {
            PromptMode::Unsignalled { .. } => None,
            PromptMode::Signalled { signal } => Some(*signal),
        }
    }
}

impl Display for PromptMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsignalled { timeout } => {
                write!(f, "unterminated prompt mode ({timeout:?} timeout)")
            }
            Self::Signalled { signal } => write!(f, "terminated prompt mode ({signal})"),
        }
    }
}

impl Default for PromptMode {
    fn default() -> Self {
        Self::Unsignalled {
            timeout: Duration::from_millis(200),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[pyclass(eq, eq_int)]
pub enum PromptSignal {
    EndOfRecord,
    GoAhead,
}

impl From<PromptSignal> for u8 {
    fn from(value: PromptSignal) -> Self {
        use telnet::command;
        match value {
            PromptSignal::EndOfRecord => command::EOR,
            PromptSignal::GoAhead => command::GA,
        }
    }
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
impl PromptSignal {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for PromptSignal {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EndOfRecord => "end of record (EoR)",
            Self::GoAhead => "go ahead (GA)",
        })
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct Trigger {
    #[pyo3(get)]
    pub id: u32,

    #[pyo3(get)]
    pub enabled: bool,

    #[pyo3(get)]
    pub module: String,

    #[pyo3(get)]
    pub config: Py<TriggerConfig>,
}

#[pymethods]
impl Trigger {
    fn __str__(&self, py: Python<'_>) -> String {
        let config: PyRef<'_, TriggerConfig> = self.config.extract(py).unwrap();
        format!(
            "Trigger({}) - enabled: {} config: {}",
            self.id, self.enabled, *config
        )
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Trigger {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Trigger({})", self.id)
    }
}

impl idmap::Identifiable for Trigger {
    fn id(&self) -> u32 {
        self.id
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct TriggerConfig {
    #[pyo3(get)]
    pub name: String,

    #[pyo3(get, set)]
    pub strip_ansi: bool,

    #[pyo3(get, set)]
    pub prompt: bool,

    #[pyo3(get, set)]
    pub gag: bool,

    #[pyo3(get, set)]
    pub callback: Option<Py<PyAny>>, // Must be async. No return.

    #[pyo3(get, set)]
    pub highlight: Option<Py<PyAny>>, // Must _not_ be async. Must return MudLine.

    #[pyo3(get, set)]
    pub expansion: Option<String>, // TODO(XXX): Rename to reaction?

    #[pyo3(get)]
    pub hit_count: u64,

    pub regex: Regex,
}

impl TriggerConfig {
    /// Check if the input matches the trigger pattern, and return matches if it does.
    ///
    /// # Panics
    /// TODO: It shouldn't...
    ///
    // TODO(XXX): Tidy, remove unwrap.
    #[must_use]
    pub fn matches(&self, line: &MudLine) -> (bool, Option<Vec<String>>) {
        if !line.prompt && self.prompt {
            return (false, None);
        }
        // TODO(XXX): Cleanup with MSRV 1.81 lifetime coolness
        let stripped_haystack;
        let haystack;
        let line = if self.strip_ansi {
            stripped_haystack = line.stripped();
            stripped_haystack.as_str()
        } else {
            haystack = line.to_str();
            &haystack
        };
        self.regex.captures(line).map_or((false, None), |matches| {
            let captures = matches
                .iter()
                .skip(1)
                .map(|m| m.unwrap().as_str().to_owned())
                .collect();
            (true, Some(captures))
        })
    }
}

#[pymethods]
impl TriggerConfig {
    /// Construct a new trigger configuration for a given regex pattern.
    ///
    /// # Errors
    ///
    /// If the regex pattern can't be compiled.
    #[new]
    #[pyo3(signature = (pattern, name, *, strip_ansi=false, prompt=false, gag=false, callback=None, highlight=None, expansion=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pattern: &str,
        name: String,
        strip_ansi: bool,
        prompt: bool,
        gag: bool,
        callback: Option<PyObject>,
        highlight: Option<PyObject>,
        expansion: Option<String>,
    ) -> Result<Self, Error> {
        let regex = Regex::new(pattern).map_err(TriggerError::Pattern)?;
        Ok(Self {
            name,
            strip_ansi,
            prompt,
            gag,
            callback,
            highlight,
            expansion,
            hit_count: 0,
            regex,
        })
    }

    #[must_use]
    pub fn pattern(&self) -> &str {
        self.regex.as_str()
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for TriggerConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.regex)
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct Alias {
    #[pyo3(get)]
    pub id: u32,

    #[pyo3(get)]
    pub enabled: bool,

    #[pyo3(get)]
    pub module: String,

    #[pyo3(get)]
    pub config: Py<AliasConfig>,
}

#[pymethods]
impl Alias {
    fn __str__(&self, py: Python<'_>) -> String {
        let config: PyRef<'_, AliasConfig> = self.config.extract(py).unwrap();
        format!(
            "Alias({}) - enabled: {} config: {}",
            self.id, self.enabled, *config
        )
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Alias {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Alias({})", self.id)
    }
}

impl idmap::Identifiable for Alias {
    fn id(&self) -> u32 {
        self.id
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct AliasConfig {
    #[pyo3(get)]
    pub name: String,

    #[pyo3(get, set)]
    pub callback: Option<Py<PyAny>>, // Must be async. No return.

    #[pyo3(get, set)]
    pub expansion: Option<String>, // TODO(XXX): Rename to reaction?

    #[pyo3(get)]
    pub hit_count: u64,

    pub regex: Regex,
}

impl AliasConfig {
    /// Check if the input matches the alias pattern, and return matches if it does.
    ///
    /// # Panics
    /// TODO: It shouldn't...
    ///
    // TODO(XXX): Tidy, remove unwrap.
    #[must_use]
    pub fn matches(&self, input: &str) -> (bool, Option<Vec<String>>) {
        match self.regex.captures(input) {
            Some(matches) => {
                let captures = matches
                    .iter()
                    .skip(1)
                    .map(|m| m.unwrap().as_str().to_owned())
                    .collect();
                (true, Some(captures))
            }
            None => (false, None),
        }
    }
}

#[pymethods]
impl AliasConfig {
    /// Construct a new alias configuration for a given regex pattern.
    ///
    /// # Errors
    ///
    /// If the regex pattern can't be compiled.
    #[new]
    #[pyo3(signature = (pattern, name, *, callback=None, expansion=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pattern: &str,
        name: String,
        callback: Option<Py<PyAny>>,
        expansion: Option<String>,
    ) -> Result<Self, Error> {
        let regex = Regex::new(pattern).map_err(AliasError::Pattern)?;
        Ok(Self {
            name,
            callback,
            expansion,
            hit_count: 0,
            regex,
        })
    }

    #[must_use]
    pub fn pattern(&self) -> &str {
        self.regex.as_str()
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for AliasConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.regex)
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct Timer {
    #[pyo3(get)]
    pub id: u32,

    #[pyo3(get)]
    pub running: bool,

    pub stop_tx: watch::Sender<bool>,

    #[pyo3(get)]
    pub module: String,

    #[pyo3(get)]
    pub config: Py<TimerConfig>,
}

#[pymethods]
impl Timer {
    fn __str__(&self, py: Python<'_>) -> String {
        let config: PyRef<'_, TimerConfig> = self.config.extract(py).unwrap();
        format!("Timer({}) - config: {}", self.id, *config)
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Timer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Timer({})", self.id)
    }
}

impl idmap::Identifiable for Timer {
    fn id(&self) -> u32 {
        self.id
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct TimerConfig {
    #[pyo3(get)]
    pub name: String,

    #[pyo3(get, set)]
    pub session_id: Option<u32>,

    #[pyo3(get)]
    pub duration: Duration,

    #[pyo3(get, set)]
    pub callback: Py<PyAny>, // Must be async. No return.

    #[pyo3(get, set)]
    pub max_ticks: u64,
}

#[pymethods]
impl TimerConfig {
    /// Construct a new timer configuration for a given duration pattern.
    ///
    /// # Errors
    ///
    /// If the duration pattern can't be recognized.
    #[new]
    #[pyo3(signature = (name, duration_ms, callback, session_id=None))]
    pub fn new(
        name: String,
        duration_ms: u64,
        callback: PyObject,
        session_id: Option<u32>,
    ) -> Result<Self, Error> {
        let duration = Duration::from_millis(duration_ms);
        Ok(Self {
            name,
            session_id,
            callback,
            max_ticks: 0,
            duration,
        })
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for TimerConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.name, self.duration)
    }
}

// 🤷 https://github.com/serde-rs/serde/issues/368
mod default {
    pub(super) fn hold_prompt() -> bool {
        true
    }

    pub(super) fn echo_input() -> bool {
        true
    }

    pub(super) fn no_line_wrap() -> bool {
        false
    }

    pub(super) fn debug_gmcp() -> bool {
        false
    }

    pub(super) fn splitview_percentage() -> u16 {
        70
    }

    pub(super) fn splitview_margin_horizontal() -> u16 {
        6
    }

    pub(super) fn splitview_margin_vertical() -> u16 {
        0
    }

    pub(super) fn no_tcp_keepalive() -> bool {
        false
    }

    #[allow(clippy::unnecessary_wraps)] // Matching config field.
    pub(super) fn command_separator() -> Option<String> {
        Some(";;".to_string())
    }
}
