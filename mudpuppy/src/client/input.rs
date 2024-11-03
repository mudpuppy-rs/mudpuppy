use std::fmt::{Display, Formatter};
use std::iter;

use pyo3::{pyclass, pymethods};
use ratatui::crossterm::event::KeyCode::{Backspace, Char, Delete, End, Home, Left, Right};
use ratatui::crossterm::event::{KeyEvent, KeyEventKind, KeyModifiers};
use tracing::info;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

// Adapted from tui-input crate. Mainly this version:
//  * Tidies a few small style issues.
//  * Splits the model representation from any TUI details.
//  * Removes the InputRequest passing interface in favour of direct methods.
//  * Removes the InputResponse - callers can handle this themselves if needed.
//  * Removes unsafe block in favour of a Result unwrap.
//  * Uses pyo3's pyclass macros to be FFI friendly with python.
#[derive(Debug, Default, Clone)]
#[pyclass]
pub struct Input {
    line: String,
    cursor: usize,
    echo: EchoState,
}

impl Input {
    pub fn handle_key_event(&mut self, key_event: &KeyEvent) {
        let KeyEvent {
            code,
            modifiers,
            kind,
            ..
        } = key_event;
        if *kind != KeyEventKind::Press {
            return;
        }

        match (code, *modifiers) {
            (Backspace, KeyModifiers::NONE) | (Char('h'), KeyModifiers::CONTROL) => {
                self.delete_prev();
            }
            (Delete, KeyModifiers::NONE) => self.delete_next(),
            (Left, KeyModifiers::NONE) | (Char('b'), KeyModifiers::CONTROL) => self.cursor_left(),
            (Left, KeyModifiers::CONTROL) | (Char('b'), KeyModifiers::META) => {
                self.cursor_word_left();
            }
            (Right, KeyModifiers::NONE) | (Char('f'), KeyModifiers::CONTROL) => self.cursor_right(),
            (Right, KeyModifiers::CONTROL) | (Char('f'), KeyModifiers::META) => {
                self.cursor_word_right();
            }
            (Char('u'), KeyModifiers::CONTROL) => self.reset(),

            (Char('w'), KeyModifiers::CONTROL) | (Char('d') | Backspace, KeyModifiers::META) => {
                self.delete_word_left();
            }

            (Delete, KeyModifiers::CONTROL) => self.delete_word_right(),
            (Char('k'), KeyModifiers::CONTROL) => self.delete_to_end(),
            (Char('a'), KeyModifiers::CONTROL) | (Home, KeyModifiers::NONE) => self.cursor_start(),
            (Char('e'), KeyModifiers::CONTROL) | (End, KeyModifiers::NONE) => self.cursor_end(),
            (Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => self.insert(*c),
            (_, _) => {}
        }
    }

    pub fn paste(&mut self, data: &str) {
        for c in data.chars() {
            self.insert(c);
        }
    }

    fn words_left(&self) -> impl Iterator<Item = char> + '_ {
        self.chars()
            .rev()
            .skip(self.chars().count().max(self.cursor) - self.cursor)
            .skip_while(|c| !c.is_alphanumeric())
            .skip_while(|c| c.is_alphanumeric())
    }

    fn chars(&self) -> std::str::Chars {
        self.line.chars()
    }
}

#[pymethods]
impl Input {
    #[must_use]
    #[new]
    pub fn new() -> Self {
        Input::default()
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.line
    }

    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub fn visual_cursor(&self) -> usize {
        if self.cursor == 0 {
            return 0;
        }

        // Unwrap safe because the end index will always be within bounds
        UnicodeWidthStr::width(
            self.line
                .get(
                    0..self
                        .line
                        .char_indices()
                        .nth(self.cursor)
                        .map_or_else(|| self.line.len(), |(index, _)| index),
                )
                .unwrap(),
        )
    }

    #[must_use]
    pub fn visual_scroll(&self, width: usize) -> usize {
        let mut uscroll = 0;
        while uscroll < self.visual_cursor().max(width) - width {
            match self.chars().next() {
                Some(c) => {
                    uscroll += UnicodeWidthChar::width(c).unwrap_or(0);
                }
                None => break,
            }
        }
        uscroll
    }

    #[must_use]
    pub fn echo(&self) -> EchoState {
        self.echo
    }

    pub fn reset(&mut self) {
        self.line.clear();
        self.cursor = 0;
        self.echo = EchoState::default();
    }

    pub fn pop(&mut self) -> Option<String> {
        match self.line.is_empty() {
            false => {
                let line = self.line.clone();
                self.line.clear();
                self.cursor = 0;
                Some(line)
            }
            true => None,
        }
    }

    pub fn set_value(&mut self, value: &str) {
        self.line = value.to_string();
        self.cursor = self.chars().count();
    }

    pub fn set_echo(&mut self, echo: EchoState) {
        info!("set {echo}");
        self.echo = echo;
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.chars().count());
    }

    pub fn insert(&mut self, c: char) {
        if self.cursor == self.chars().count() {
            self.line.push(c);
        } else {
            self.line = self
                .chars()
                .take(self.cursor)
                .chain(iter::once(c).chain(self.chars().skip(self.cursor)))
                .collect();
        }
        self.cursor += 1;
    }

    pub fn delete_prev(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.drop_index(self.cursor);
    }

    pub fn delete_next(&mut self) {
        if self.cursor == self.chars().count() {
            return;
        }
        self.drop_index(self.cursor);
    }

    pub fn delete_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let rev = self.words_left().collect::<Vec<_>>();
        let rev_len = rev.len();
        self.line = rev
            .into_iter()
            .rev()
            .chain(self.chars().skip(self.cursor))
            .collect();
        self.cursor = rev_len;
    }

    pub fn delete_word_right(&mut self) {
        if self.cursor == self.chars().count() {
            return;
        }
        self.line = self
            .chars()
            .take(self.cursor)
            .chain(
                self.chars()
                    .skip(self.cursor)
                    .skip_while(|c| c.is_alphanumeric())
                    .skip_while(|c| !c.is_alphanumeric()),
            )
            .collect();
    }

    pub fn delete_to_end(&mut self) {
        self.line = self.chars().take(self.cursor).collect();
    }

    pub fn cursor_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
    }

    pub fn cursor_right(&mut self) {
        if self.cursor == self.chars().count() {
            return;
        }
        self.cursor += 1;
    }

    pub fn cursor_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.words_left().count();
    }

    pub fn cursor_word_right(&mut self) {
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

    pub fn cursor_start(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor = self.chars().count();
    }

    fn drop_index(&mut self, index: usize) {
        self.line = self
            .chars()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, c)| c)
            .collect();
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.line)
    }
}

impl From<String> for Input {
    fn from(value: String) -> Self {
        let cursor = value.chars().count();
        Self {
            line: value,
            cursor,
            echo: EchoState::default(),
        }
    }
}

impl From<&str> for Input {
    fn from(value: &str) -> Self {
        Self {
            line: value.to_string(),
            cursor: value.chars().count(),
            echo: EchoState::default(),
        }
    }
}

#[pyclass(eq, eq_int)]
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq)]
pub enum EchoState {
    #[default]
    Enabled,
    Password,
}

impl Display for EchoState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled => write!(f, "echo state: enabled"),
            Self::Password => write!(f, "echo state: password"),
        }
    }
}

impl From<EchoState> for bool {
    fn from(state: EchoState) -> Self {
        match state {
            EchoState::Enabled => true,
            EchoState::Password => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format() {
        let input: Input = TEXT.into();
        println!("{input}");
    }

    #[test]
    fn set_cursor() {
        let mut input: Input = TEXT.into();

        input.set_cursor(3);
        assert_eq!(input.value(), "first second, third.");
        assert_eq!(input.cursor(), 3);

        input.set_cursor(30);
        assert_eq!(input.cursor(), TEXT.chars().count());

        input.set_cursor(TEXT.chars().count());
        assert_eq!(input.cursor(), TEXT.chars().count());
    }

    #[test]
    fn insert_char() {
        let mut input: Input = TEXT.into();

        input.insert('x');
        assert_eq!(input.value(), "first second, third.x");
        assert_eq!(input.cursor(), TEXT.chars().count() + 1);
        input.insert('x');
        assert_eq!(input.value(), "first second, third.xx");
        assert_eq!(input.cursor(), TEXT.chars().count() + 2);

        input.set_cursor(3);
        input.insert('x');
        assert_eq!(input.value(), "firxst second, third.xx");
        assert_eq!(input.cursor(), 4);

        input.insert('x');
        assert_eq!(input.value(), "firxxst second, third.xx");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn go_to_prev_char() {
        let mut input: Input = TEXT.into();

        input.cursor_left();
        assert_eq!(input.value(), "first second, third.");
        assert_eq!(input.cursor(), TEXT.chars().count() - 1);

        input.set_cursor(3);
        input.cursor_left();
        assert_eq!(input.value(), "first second, third.");
        assert_eq!(input.cursor(), 2);

        input.cursor_left();
        assert_eq!(input.value(), "first second, third.");
        assert_eq!(input.cursor(), 1);
    }

    #[test]
    fn remove_unicode_chars() {
        let mut input: Input = "¡test¡".into();

        input.delete_prev();
        assert_eq!(input.value(), "¡test");
        assert_eq!(input.cursor(), 5);

        input.cursor_start();
        input.delete_next();
        assert_eq!(input.value(), "test");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn insert_unicode_chars() {
        let mut input = Input::from("¡test¡");
        input.set_cursor(5);

        input.insert('☆');
        assert_eq!(input.value(), "¡test☆¡");
        assert_eq!(input.cursor(), 6);

        input.cursor_start();
        input.cursor_right();
        input.insert('☆');
        assert_eq!(input.value(), "¡☆test☆¡");
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn multispace_characters() {
        let input: Input = "Ｈｅｌｌｏ, ｗｏｒｌｄ!".into();
        assert_eq!(input.cursor(), 13);
        assert_eq!(input.visual_cursor(), 23);
        assert_eq!(input.visual_scroll(6), 18);
    }

    const TEXT: &str = "first second, third.";
}
