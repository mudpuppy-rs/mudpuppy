//! Generic representations of key events/codes/modifiers
//!
//! Allows the Python API and other headless bits to operate without ratatui, or a crossterm
//! dependency.

use pyo3::{pyclass, pymethods};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use crate::error::{Error, ErrorKind, KeyBindingError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[pyclass(frozen)]
pub(crate) struct KeyEvent {
    pub(crate) code: KeyCode,
    pub(crate) modifiers: KeyModifiers,
}

impl KeyEvent {
    // TODO(XXX): reverse arg order.
    #[must_use]
    pub(crate) fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
impl KeyEvent {
    #[new]
    fn py_new(event: &str) -> Result<Self, Error> {
        event.parse().map_err(|e| ErrorKind::from(e).into())
    }

    #[pyo3(name = "code")]
    fn get_code(&self) -> String {
        self.code.to_string()
    }

    #[pyo3(name = "modifiers")]
    fn get_modifiers(&self) -> Vec<String> {
        (&self.modifiers).into()
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
        write!(f, "{}{}", self.modifiers, self.code)
    }
}

impl FromStr for KeyEvent {
    type Err = KeyBindingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut modifiers = KeyModifiers::NONE;
        let mut final_part = None;

        for part in s.split('-') {
            if let Ok(modifier) = KeyModifiers::from_str(part) {
                modifiers.insert(modifier);
            } else {
                final_part = Some(part.to_lowercase());
                break;
            }
        }

        Ok(Self::new(
            final_part.as_deref().unwrap_or_default().parse()?,
            modifiers,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyModifiers(pub(crate) u8);

impl KeyModifiers {
    pub(crate) const NONE: Self = KeyModifiers(0);
    pub(crate) const SHIFT: Self = KeyModifiers(1);
    pub(crate) const CONTROL: Self = KeyModifiers(2);
    pub(crate) const ALT: Self = KeyModifiers(4);
    pub(crate) const META: Self = KeyModifiers(5);

    #[must_use]
    pub(crate) fn contains(self, other: KeyModifiers) -> bool {
        (self.0 & other.0) == other.0
    }

    pub(crate) fn insert(&mut self, other: KeyModifiers) {
        self.0 |= other.0;
    }
}

impl FromStr for KeyModifiers {
    type Err = KeyBindingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "ctrl" => KeyModifiers::CONTROL,
            "shift" => KeyModifiers::SHIFT,
            "alt" => KeyModifiers::ALT,
            "meta" => KeyModifiers::META,
            _ => {
                return Err(KeyBindingError::InvalidKeys(format!(
                    "unknown key modifier: {s}"
                )));
            }
        })
    }
}

impl Display for KeyModifiers {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let str = <&KeyModifiers as Into<Vec<_>>>::into(self).iter().fold(
            String::new(),
            |mut output, m| {
                output.push_str(m);
                output.push('-');
                output
            },
        );
        write!(f, "{str}")
    }
}

impl From<&KeyModifiers> for Vec<String> {
    fn from(modifiers: &KeyModifiers) -> Self {
        let mut parts = Vec::new();

        if modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("ctrl".to_string());
        }
        if modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("shift".to_string());
        }
        if modifiers.contains(KeyModifiers::ALT) {
            parts.push("alt".to_string());
        }

        parts
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

impl FromStr for KeyCode {
    type Err = KeyBindingError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            v if v.starts_with('f') => {
                let num = v[1..]
                    .parse::<u8>()
                    .map_err(|_| KeyBindingError::InvalidKeys(format!("invalid F-key: {v}")))?;
                if (1..=12).contains(&num) {
                    Self::F(num)
                } else {
                    return Err(KeyBindingError::InvalidKeys(format!("invalid F-key: {v}")));
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
            _ => {
                return Err(KeyBindingError::InvalidKeys(format!(
                    "unknown key code: {value:?}"
                )));
            }
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
