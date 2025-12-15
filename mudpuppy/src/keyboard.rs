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
    pub(crate) fn py_new(event: &str) -> Result<Self, Error> {
        event.parse().map_err(|e| ErrorKind::from(e).into())
    }

    #[getter(code)]
    fn get_code(&self) -> String {
        self.code.to_string()
    }

    #[getter(modifiers)]
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
            v if v.len() > 1
                && (v.starts_with('f') || v.starts_with('F'))
                && v[1..].chars().all(|c| c.is_ascii_digit()) =>
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_alt_f_not_fkey() {
        let key: KeyEvent = "Alt-f".parse().unwrap();
        assert_eq!(key.code, KeyCode::Char('f'));
        assert!(key.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn test_modifiers_none() {
        let mods = KeyModifiers::NONE;
        assert!(!mods.contains(KeyModifiers::SHIFT));
        assert!(!mods.contains(KeyModifiers::CONTROL));
        assert!(!mods.contains(KeyModifiers::ALT));
    }

    #[test]
    fn test_modifiers_insert() {
        let mut mods = KeyModifiers::NONE;
        mods.insert(KeyModifiers::CONTROL);
        assert!(mods.contains(KeyModifiers::CONTROL));
        assert!(!mods.contains(KeyModifiers::SHIFT));

        mods.insert(KeyModifiers::SHIFT);
        assert!(mods.contains(KeyModifiers::CONTROL));
        assert!(mods.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn test_modifiers_parse() {
        assert_eq!(
            "ctrl".parse::<KeyModifiers>().unwrap(),
            KeyModifiers::CONTROL
        );
        assert_eq!(
            "Ctrl".parse::<KeyModifiers>().unwrap(),
            KeyModifiers::CONTROL
        );
        assert_eq!(
            "CTRL".parse::<KeyModifiers>().unwrap(),
            KeyModifiers::CONTROL
        );
        assert_eq!(
            "shift".parse::<KeyModifiers>().unwrap(),
            KeyModifiers::SHIFT
        );
        assert_eq!("alt".parse::<KeyModifiers>().unwrap(), KeyModifiers::ALT);
        assert_eq!("meta".parse::<KeyModifiers>().unwrap(), KeyModifiers::META);
    }

    #[test]
    fn test_modifiers_parse_invalid() {
        assert!("super".parse::<KeyModifiers>().is_err());
        assert!("command".parse::<KeyModifiers>().is_err());
        assert!("".parse::<KeyModifiers>().is_err());
        assert!("ctrl-shift".parse::<KeyModifiers>().is_err());
    }

    #[test]
    fn test_modifiers_display() {
        let mut mods = KeyModifiers::NONE;
        assert_eq!(mods.to_string(), "");

        mods.insert(KeyModifiers::CONTROL);
        assert_eq!(mods.to_string(), "ctrl-");

        mods.insert(KeyModifiers::SHIFT);
        assert_eq!(mods.to_string(), "ctrl-shift-");

        mods.insert(KeyModifiers::ALT);
        assert_eq!(mods.to_string(), "ctrl-shift-alt-");
    }

    #[test]
    fn test_modifiers_to_vec() {
        let mut mods = KeyModifiers::NONE;
        mods.insert(KeyModifiers::CONTROL);
        mods.insert(KeyModifiers::ALT);

        let vec: Vec<String> = (&mods).into();
        assert_eq!(vec, vec!["ctrl", "alt"]);
    }

    #[test]
    fn test_keycode_special_keys() {
        assert_eq!("backspace".parse::<KeyCode>().unwrap(), KeyCode::Backspace);
        assert_eq!("enter".parse::<KeyCode>().unwrap(), KeyCode::Enter);
        assert_eq!("left".parse::<KeyCode>().unwrap(), KeyCode::Left);
        assert_eq!("right".parse::<KeyCode>().unwrap(), KeyCode::Right);
        assert_eq!("up".parse::<KeyCode>().unwrap(), KeyCode::Up);
        assert_eq!("down".parse::<KeyCode>().unwrap(), KeyCode::Down);
        assert_eq!("home".parse::<KeyCode>().unwrap(), KeyCode::Home);
        assert_eq!("end".parse::<KeyCode>().unwrap(), KeyCode::End);
        assert_eq!("pageup".parse::<KeyCode>().unwrap(), KeyCode::PageUp);
        assert_eq!("pagedown".parse::<KeyCode>().unwrap(), KeyCode::PageDown);
        assert_eq!("tab".parse::<KeyCode>().unwrap(), KeyCode::Tab);
        assert_eq!("delete".parse::<KeyCode>().unwrap(), KeyCode::Delete);
        assert_eq!("insert".parse::<KeyCode>().unwrap(), KeyCode::Insert);
        assert_eq!("esc".parse::<KeyCode>().unwrap(), KeyCode::Esc);
    }

    #[test]
    fn test_keycode_chars() {
        assert_eq!("a".parse::<KeyCode>().unwrap(), KeyCode::Char('a'));
        assert_eq!("Z".parse::<KeyCode>().unwrap(), KeyCode::Char('Z'));
        assert_eq!("1".parse::<KeyCode>().unwrap(), KeyCode::Char('1'));
        assert_eq!("!".parse::<KeyCode>().unwrap(), KeyCode::Char('!'));
        assert_eq!(" ".parse::<KeyCode>().unwrap(), KeyCode::Char(' '));
    }

    #[test]
    fn test_keycode_f_keys_all() {
        for i in 1..=12 {
            let key_str = format!("f{i}");
            assert_eq!(key_str.parse::<KeyCode>().unwrap(), KeyCode::F(i));

            let key_str_upper = format!("F{i}");
            assert_eq!(key_str_upper.parse::<KeyCode>().unwrap(), KeyCode::F(i));
        }
    }

    #[test]
    fn test_keycode_f_keys_invalid() {
        assert!("f0".parse::<KeyCode>().is_err());
        assert!("f13".parse::<KeyCode>().is_err());
        assert!("f99".parse::<KeyCode>().is_err());
        assert!("f1a".parse::<KeyCode>().is_err());
        assert!("fa".parse::<KeyCode>().is_err());
    }

    #[test]
    fn test_keycode_invalid() {
        assert!("".parse::<KeyCode>().is_err());
        assert!("unknown".parse::<KeyCode>().is_err());
        assert!("ctrl-a".parse::<KeyCode>().is_err());
        assert!("page up".parse::<KeyCode>().is_err());
    }

    #[test]
    fn test_keycode_display_roundtrip() {
        let codes = vec![
            KeyCode::Char('a'),
            KeyCode::F(5),
            KeyCode::Backspace,
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Esc,
        ];

        for code in codes {
            let s = code.to_string();
            let parsed: KeyCode = s.parse().unwrap();
            assert_eq!(code, parsed, "Failed roundtrip for {code:?}");
        }
    }

    #[test]
    fn test_keyevent_multiple_modifiers() {
        let key: KeyEvent = "Ctrl-Shift-a".parse().unwrap();
        assert_eq!(key.code, KeyCode::Char('a'));
        assert!(key.modifiers.contains(KeyModifiers::CONTROL));
        assert!(key.modifiers.contains(KeyModifiers::SHIFT));
        assert!(!key.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn test_keyevent_all_modifiers() {
        let key: KeyEvent = "Ctrl-Shift-Alt-x".parse().unwrap();
        assert_eq!(key.code, KeyCode::Char('x'));
        assert!(key.modifiers.contains(KeyModifiers::CONTROL));
        assert!(key.modifiers.contains(KeyModifiers::SHIFT));
        assert!(key.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn test_keyevent_modifier_order() {
        // Order shouldn't matter
        let key1: KeyEvent = "Ctrl-Alt-a".parse().unwrap();
        let key2: KeyEvent = "Alt-Ctrl-a".parse().unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_keyevent_with_special_keys() {
        let key: KeyEvent = "Ctrl-enter".parse().unwrap();
        assert_eq!(key.code, KeyCode::Enter);
        assert!(key.modifiers.contains(KeyModifiers::CONTROL));

        let key: KeyEvent = "Shift-tab".parse().unwrap();
        assert_eq!(key.code, KeyCode::Tab);
        assert!(key.modifiers.contains(KeyModifiers::SHIFT));

        let key: KeyEvent = "Alt-f5".parse().unwrap();
        assert_eq!(key.code, KeyCode::F(5));
        assert!(key.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn test_keyevent_display() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(key.to_string(), "ctrl-a");

        let mut mods = KeyModifiers::NONE;
        mods.insert(KeyModifiers::CONTROL);
        mods.insert(KeyModifiers::SHIFT);
        let key = KeyEvent::new(KeyCode::Enter, mods);
        assert_eq!(key.to_string(), "ctrl-shift-enter");
    }

    #[test]
    fn test_keyevent_display_roundtrip() {
        let events = vec![
            "a",
            "ctrl-a",
            "shift-b",
            "alt-f1",
            "ctrl-shift-enter",
            "ctrl-alt-delete",
        ];

        for event_str in events {
            let parsed: KeyEvent = event_str.parse().unwrap();
            let displayed = parsed.to_string();
            let reparsed: KeyEvent = displayed.parse().unwrap();
            assert_eq!(parsed, reparsed, "Failed roundtrip for {event_str}");
        }
    }

    #[test]
    fn test_keyevent_no_key() {
        // Just modifiers without a key should fail
        assert!("Ctrl-".parse::<KeyEvent>().is_err());
        assert!("Ctrl-Shift-".parse::<KeyEvent>().is_err());
    }

    #[test]
    fn test_keyevent_plain_key() {
        let key: KeyEvent = "a".parse().unwrap();
        assert_eq!(key.code, KeyCode::Char('a'));
        assert_eq!(key.modifiers, KeyModifiers::NONE);

        let key: KeyEvent = "enter".parse().unwrap();
        assert_eq!(key.code, KeyCode::Enter);
        assert_eq!(key.modifiers, KeyModifiers::NONE);
    }

    #[test]
    fn test_keyevent_case_normalization() {
        // Modifiers are case-insensitive
        let key1: KeyEvent = "ctrl-a".parse().unwrap();
        let key2: KeyEvent = "Ctrl-a".parse().unwrap();
        let key3: KeyEvent = "CTRL-a".parse().unwrap();
        assert_eq!(key1, key2);
        assert_eq!(key2, key3);

        // Keys are lowercased (except uppercase chars become lowercase)
        let key1: KeyEvent = "alt-ENTER".parse().unwrap();
        let key2: KeyEvent = "Alt-enter".parse().unwrap();
        assert_eq!(key1, key2);
    }
}
