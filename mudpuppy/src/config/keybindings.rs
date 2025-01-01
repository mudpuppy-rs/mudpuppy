use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::Shortcut;

#[derive(Debug, Clone, Default)]
pub struct KeyBindings(pub(crate) HashMap<String, HashMap<Vec<KeyEvent>, Shortcut>>);

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> crate::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed_map = HashMap::<String, HashMap<String, Shortcut>>::deserialize(deserializer)?;

        let keybindings = parsed_map
            .into_iter()
            .map(|(input_mode, inner_map)| {
                let converted_inner_map = inner_map
                    .into_iter()
                    .map(|(key_str, cmd)| {
                        (
                            parse_key_sequence(&key_str).unwrap_or_else(|_| {
                                std::panic!("invalid config keyboard sequence: {key_str}")
                            }),
                            cmd,
                        )
                    })
                    .collect();
                (input_mode.to_lowercase(), converted_inner_map)
            })
            .collect();

        Ok(Self(keybindings))
    }
}

impl Serialize for KeyBindings {
    fn serialize<S>(&self, serializer: S) -> crate::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        fn code_to_string(code: KeyCode) -> String {
            match code {
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
                KeyCode::BackTab => "backtab".to_string(),
                KeyCode::Delete => "delete".to_string(),
                KeyCode::Insert => "insert".to_string(),
                KeyCode::F(code) => format!("f{code}"),
                KeyCode::Esc => "esc".to_string(),
                KeyCode::Char(' ') => "space".to_string(),
                KeyCode::Char('-') => "hyphen".to_string(),
                KeyCode::Char(c) => c.to_string(),
                _ => std::panic!("unknown key code: {code:?}"),
            }
        }

        fn key_to_string(event: KeyEvent) -> String {
            if event.modifiers.is_empty() {
                return code_to_string(event.code);
            }
            let mut key = code_to_string(event.code);
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                key = format!("ctrl-{key}");
            }
            if event.modifiers.contains(KeyModifiers::ALT) {
                key = format!("alt-{key}");
            }
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                key = format!("shift-{key}");
            }
            format!("<{key}>")
        }

        let mut raw_map = HashMap::<String, HashMap<String, Shortcut>>::default();

        for (tab, bindings) in &self.0 {
            let mut inner_map = HashMap::default();
            for (key, cmd) in bindings {
                inner_map.insert(
                    key.iter()
                        .map(|key| key_to_string(*key))
                        .collect::<Vec<_>>()
                        .join("><"),
                    cmd.clone(),
                );
            }
            raw_map.insert(tab.clone(), inner_map);
        }

        raw_map.serialize(serializer)
    }
}

fn parse_key_event(raw: &str) -> crate::Result<KeyEvent, String> {
    parse_key_code_with_modifiers(extract_modifiers(&raw.to_ascii_lowercase()))
}

fn extract_modifiers(raw: &str) -> (&str, KeyModifiers) {
    let mut modifiers = KeyModifiers::empty();
    let mut current = raw;

    loop {
        if let Some(rest) = current.strip_prefix("ctrl-") {
            modifiers.insert(KeyModifiers::CONTROL);
            current = rest;
        } else if let Some(rest) = current.strip_prefix("alt-") {
            modifiers.insert(KeyModifiers::ALT);
            current = rest;
        } else if let Some(rest) = current.strip_prefix("shift-") {
            modifiers.insert(KeyModifiers::SHIFT);
            current = rest;
        } else {
            break;
        }
    }

    (current, modifiers)
}

fn parse_key_code_with_modifiers(
    (raw, mut modifiers): (&str, KeyModifiers),
) -> crate::Result<KeyEvent, String> {
    #[allow(clippy::match_same_arms)]
    let c = match raw {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "backtab" => {
            modifiers.insert(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        "space" => KeyCode::Char(' '),
        "hyphen" => KeyCode::Char('-'),
        "minus" => KeyCode::Char('-'),
        "tab" => KeyCode::Tab,
        c if c.len() == 1 => {
            let mut c = c.chars().next().unwrap();
            if modifiers.contains(KeyModifiers::SHIFT) {
                c = c.to_ascii_uppercase();
            }
            KeyCode::Char(c)
        }
        _ => return Err(format!("Unable to parse {raw}")),
    };
    Ok(KeyEvent::new(c, modifiers))
}

fn parse_key_sequence(raw: &str) -> crate::Result<Vec<KeyEvent>, String> {
    if raw.chars().filter(|c| *c == '>').count() != raw.chars().filter(|c| *c == '<').count() {
        return Err(format!("Unable to parse `{raw}`"));
    }
    let raw = if raw.contains("><") {
        raw
    } else {
        let raw = raw.strip_prefix('<').unwrap_or(raw);
        let raw = raw.strip_prefix('>').unwrap_or(raw);
        raw
    };
    let sequences = raw
        .split("><")
        .map(|seq| {
            if let Some(s) = seq.strip_prefix('<') {
                s
            } else if let Some(s) = seq.strip_suffix('>') {
                s
            } else {
                seq
            }
        })
        .collect::<Vec<_>>();

    sequences.into_iter().map(parse_key_event).collect()
}
