use crossterm::event::{
    KeyCode as CrosstermKeyCode, KeyEvent as CrosstermKeyEvent,
    KeyModifiers as CrosstermKeyModifiers,
};
use pyo3::{pyclass, pymethods, Bound, IntoPyObject, PyResult, Python};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

use crate::error::{ConfigError, KeyBindingError};
use crate::model::{InputMode, KeyCode, KeyEvent, KeyModifiers, Shortcut};

#[derive(Debug, Clone, Default, Eq, PartialEq)]
#[pyclass]
pub struct KeyBindings {
    bindings: BTreeMap<InputMode, BTreeMap<KeyEvent, Shortcut>>,
}

impl KeyBindings {
    /// Parse a TOML configuration file into a `KeyBindings` instance.
    ///
    /// # Errors
    /// If the TOML content is not valid, a `ConfigError` will be returned.
    pub fn from_toml(content: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(content)?;
        let mut bindings = Self::default();

        for bind in raw.bindings {
            let mode = InputMode::try_from(bind.mode.as_str())
                .map_err(|_| KeyBindingError::UnknownMode(bind.mode))?;
            let action = Shortcut::try_from(bind.action.as_str())
                .map_err(|_| KeyBindingError::UnknownShortcut(bind.action))?;
            let key_sequence = KeyEvent::try_from(bind.keys.as_str())?;

            bindings
                .bindings
                .entry(mode)
                .or_default()
                .insert(key_sequence, action);
        }

        Ok(bindings)
    }

    pub fn merge(&mut self, other: KeyBindings) {
        for (mode, mode_bindings) in other.bindings {
            for (seq, action) in mode_bindings {
                self.bindings
                    .entry(mode)
                    .or_default()
                    .entry(seq)
                    .or_insert(action);
            }
        }
    }

    #[must_use]
    pub fn lookup(&self, mode: InputMode, key: &KeyEvent) -> Option<Shortcut> {
        self.bindings
            .get(&mode)
            .and_then(|mode_bindings| mode_bindings.get(key).copied())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn modes(&self) -> impl Iterator<Item = &InputMode> {
        self.bindings.keys()
    }

    pub fn bindings(&self, mode: InputMode) -> impl Iterator<Item = (&KeyEvent, &Shortcut)> {
        self.bindings.get(&mode).into_iter().flat_map(|m| m.iter())
    }

    #[must_use]
    pub fn to_toml(&self) -> String {
        let raw: RawConfig = self.clone().into();
        toml::to_string(&raw).unwrap()
    }
}

#[pymethods]
#[allow(clippy::missing_errors_doc)] // Python APIs documented in stubs.
impl KeyBindings {
    #[pyo3(name = "modes")]
    pub fn py_modes(&self) -> Vec<String> {
        self.bindings.keys().map(ToString::to_string).collect()
    }

    #[pyo3(name = "bindings", signature = (mode=None))]
    pub fn py_bindings<'py>(
        &self,
        py: Python<'py>,
        mode: Option<String>,
    ) -> PyResult<Vec<(Bound<'py, KeyEvent>, Bound<'py, Shortcut>)>> {
        let input_mode = mode
            .and_then(|s| InputMode::from_str(&s).ok())
            .unwrap_or_default();

        Ok(self
            .bindings
            .get(&input_mode)
            .into_iter()
            .flat_map(|m| m.iter())
            .map(|(k, s)| (k.into_pyobject(py).unwrap(), s.into_pyobject(py).unwrap()))
            .collect())
    }

    #[pyo3(signature = (key, mode=None))]
    pub fn shortcut(&self, key: KeyEvent, mode: Option<String>) -> PyResult<Option<Shortcut>> {
        let input_mode = mode
            .and_then(|s| InputMode::from_str(&s).ok())
            .unwrap_or_default();

        Ok(self
            .bindings
            .get(&input_mode)
            .and_then(|bindings| bindings.get(&key).copied()))
    }
}

impl From<KeyBindings> for RawConfig {
    fn from(bindings: KeyBindings) -> Self {
        let mut raw_bindings = Vec::new();

        for (mode, mode_bindings) in bindings.bindings {
            for (event, action) in mode_bindings {
                let mut key_parts = event.modifiers.modifiers();
                key_parts.push(event.code.to_string());

                raw_bindings.push(RawKeyBinding {
                    mode: mode.to_string(),
                    keys: key_parts.join("-"),
                    action: action.to_string(),
                });
            }
        }

        RawConfig {
            bindings: raw_bindings,
        }
    }
}

impl Serialize for KeyBindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let raw: RawConfig = self.clone().into();
        raw.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawConfig::deserialize(deserializer)?;
        Self::from_toml(&toml::to_string(&raw).map_err(serde::de::Error::custom)?)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawConfig {
    #[serde(default, rename = "binding")]
    bindings: Vec<RawKeyBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawKeyBinding {
    #[serde(default = "default::input_mode")]
    mode: String,
    keys: String,
    action: String,
}

mod default {
    pub(super) fn input_mode() -> String {
        super::InputMode::default().to_string()
    }
}

impl TryFrom<CrosstermKeyEvent> for KeyEvent {
    type Error = String;

    fn try_from(event: CrosstermKeyEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            code: event.code.try_into()?,
            modifiers: event.modifiers.into(),
        })
    }
}

impl From<KeyEvent> for CrosstermKeyEvent {
    fn from(event: KeyEvent) -> Self {
        CrosstermKeyEvent::new(event.code.into(), event.modifiers.into())
    }
}

impl TryFrom<CrosstermKeyCode> for KeyCode {
    type Error = String;

    fn try_from(code: CrosstermKeyCode) -> Result<Self, Self::Error> {
        Ok(match code {
            CrosstermKeyCode::Char(c) => KeyCode::Char(c),
            CrosstermKeyCode::F(n) => KeyCode::F(n),
            CrosstermKeyCode::Backspace => KeyCode::Backspace,
            CrosstermKeyCode::Enter => KeyCode::Enter,
            CrosstermKeyCode::Left => KeyCode::Left,
            CrosstermKeyCode::Right => KeyCode::Right,
            CrosstermKeyCode::Up => KeyCode::Up,
            CrosstermKeyCode::Down => KeyCode::Down,
            CrosstermKeyCode::Home => KeyCode::Home,
            CrosstermKeyCode::End => KeyCode::End,
            CrosstermKeyCode::PageUp => KeyCode::PageUp,
            CrosstermKeyCode::PageDown => KeyCode::PageDown,
            CrosstermKeyCode::Tab => KeyCode::Tab,
            CrosstermKeyCode::Delete => KeyCode::Delete,
            CrosstermKeyCode::Insert => KeyCode::Insert,
            CrosstermKeyCode::Esc => KeyCode::Esc,
            c => return Err(format!("unknown key code: {c:?}")),
        })
    }
}

impl From<KeyCode> for CrosstermKeyCode {
    fn from(code: KeyCode) -> Self {
        match code {
            KeyCode::Char(c) => CrosstermKeyCode::Char(c),
            KeyCode::F(n) => CrosstermKeyCode::F(n),
            KeyCode::Backspace => CrosstermKeyCode::Backspace,
            KeyCode::Enter => CrosstermKeyCode::Enter,
            KeyCode::Left => CrosstermKeyCode::Left,
            KeyCode::Right => CrosstermKeyCode::Right,
            KeyCode::Up => CrosstermKeyCode::Up,
            KeyCode::Down => CrosstermKeyCode::Down,
            KeyCode::Home => CrosstermKeyCode::Home,
            KeyCode::End => CrosstermKeyCode::End,
            KeyCode::PageUp => CrosstermKeyCode::PageUp,
            KeyCode::PageDown => CrosstermKeyCode::PageDown,
            KeyCode::Tab => CrosstermKeyCode::Tab,
            KeyCode::Delete => CrosstermKeyCode::Delete,
            KeyCode::Insert => CrosstermKeyCode::Insert,
            KeyCode::Esc => CrosstermKeyCode::Esc,
        }
    }
}

impl From<CrosstermKeyModifiers> for KeyModifiers {
    fn from(mods: CrosstermKeyModifiers) -> Self {
        let mut result = KeyModifiers::NONE;
        if mods.contains(CrosstermKeyModifiers::SHIFT) {
            result.insert(KeyModifiers::SHIFT);
        }
        if mods.contains(CrosstermKeyModifiers::CONTROL) {
            result.insert(KeyModifiers::CONTROL);
        }
        if mods.contains(CrosstermKeyModifiers::ALT) {
            result.insert(KeyModifiers::ALT);
        }
        result
    }
}

impl From<KeyModifiers> for CrosstermKeyModifiers {
    fn from(mods: KeyModifiers) -> Self {
        let mut result = CrosstermKeyModifiers::empty();
        if mods.contains(KeyModifiers::SHIFT) {
            result.insert(CrosstermKeyModifiers::SHIFT);
        }
        if mods.contains(KeyModifiers::CONTROL) {
            result.insert(CrosstermKeyModifiers::CONTROL);
        }
        if mods.contains(KeyModifiers::ALT) {
            result.insert(CrosstermKeyModifiers::ALT);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONFIG: &str = r#"
[[binding]]
keys = "ctrl-p"
action = "tabnext"

[[binding]]
mode = "mudlist"
keys = "enter"
action = "MudListConnect"
"#;

    #[test]
    fn test_valid_config() {
        assert_eq!(
            KeyBindings::from_toml(TEST_CONFIG).unwrap().lookup(
                InputMode::MudSession,
                &KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)
            ),
            Some(Shortcut::TabNext)
        );
    }

    #[test]
    fn test_roundtrip() {
        let bindings = KeyBindings::from_toml(TEST_CONFIG).unwrap();
        assert_eq!(
            bindings,
            KeyBindings::from_toml(&bindings.to_toml()).unwrap()
        );
    }

    #[test]
    fn test_invalid_mode() {
        let invalid_config = r#"
[[binding]]
mode = "invalid"
keys = "ctrl-p"
action = "search"
"#;

        assert_eq!(
            KeyBindings::from_toml(invalid_config)
                .unwrap_err()
                .to_string(),
            "unknown input mode: \"invalid\""
        );
    }

    #[test]
    fn test_invalid_shortcut() {
        let invalid_config = r#"
[[binding]]
keys = "ctrl-p"
action = "search"
"#;

        assert_eq!(
            KeyBindings::from_toml(invalid_config)
                .unwrap_err()
                .to_string(),
            "unknown shortcut: \"search\""
        );
    }

    #[test]
    fn test_merge() {
        let default_config = r#"
[[binding]]
keys = "ctrl-p"
action = "tabnext"
"#;

        let user_config = r#"
[[binding]]
keys = "ctrl-p"
action = "quit"
"#;

        let defaults = KeyBindings::from_toml(default_config).unwrap();
        let mut user = KeyBindings::from_toml(user_config).unwrap();
        user.merge(defaults);

        assert_eq!(
            user.lookup(
                InputMode::MudSession,
                &KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)
            ),
            Some(Shortcut::Quit)
        );
    }

    #[test]
    fn test_default_mode() {
        let config = r#"
[[binding]]
keys = "ctrl-q"
action = "quit"
"#;
        let bindings = KeyBindings::from_toml(config).unwrap();
        assert_eq!(
            bindings.lookup(
                InputMode::MudSession,
                &KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)
            ),
            Some(Shortcut::Quit)
        );
    }
}
