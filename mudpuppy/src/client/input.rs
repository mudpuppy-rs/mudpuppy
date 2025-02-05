use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::{iter, mem};

use pyo3::{pyclass, pymethods};
use ratatui::crossterm::event::KeyCode::{Backspace, Char, Delete, End, Home, Left, Right};
use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
use tracing::info;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::model::InputLine;

// Adapted from tui-input crate. Mainly this version:
//  * Tidies a few small style issues.
//  * Splits the model representation from any TUI details.
//  * Removes the InputRequest passing interface in favour of direct methods.
//  * Removes the InputResponse - callers can handle this themselves if needed.
//  * Removes unsafe block in favour of a Result unwrap.
//  * Uses pyo3's pyclass macros to be FFI friendly with python.
//  * Adapts state to InputLine.
//  * Maintains a separate EchoState.
//
// We want to track EchoState both per-line and at the telnet level so that
// items can be masked when loaded from history when we're back in normal
// echo mode.
#[derive(Debug, Default, Clone)]
#[pyclass]
pub struct Input {
    line: InputLine,
    telnet_echo: EchoState,
    markup: Markup,
    cursor: usize,
}

impl Input {
    pub fn handle_key_event(&mut self, key_event: &KeyEvent) {
        let KeyEvent {
            code, modifiers, ..
        } = key_event;

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
        self.line.sent.chars()
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
    pub fn value(&self) -> InputLine {
        self.line.clone()
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

        let s = &self.line.sent;

        // Unwrap safe because the end index will always be within bounds
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
    pub fn telnet_echo(&self) -> EchoState {
        self.telnet_echo
    }

    pub fn reset(&mut self) {
        self.line.sent.clear();
        self.line.original = None;
        self.line.echo = EchoState::default();
        self.cursor = 0;
    }

    pub fn pop(&mut self) -> Option<InputLine> {
        if self.line.sent.is_empty() {
            return None;
        }

        self.cursor = 0;

        Some(InputLine {
            sent: mem::take(&mut self.line.sent),
            // Reset the current echo state back to the telnet-level state.
            echo: mem::replace(&mut self.line.echo, self.telnet_echo),
            original: None,
            scripted: false,
        })
    }

    pub fn set_value(&mut self, value: InputLine) {
        self.line = value;
        self.cursor = self.line.sent.chars().count();
    }

    pub fn set_telnet_echo(&mut self, echo: EchoState) {
        info!("set {echo}");
        // Save the telnet echo state.
        self.telnet_echo = echo;
        // and update the in-progress line to match.
        self.line.echo = echo;
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.chars().count());
    }

    pub fn insert(&mut self, c: char) {
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
        self.line.sent = rev
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
    }

    pub fn delete_to_end(&mut self) {
        self.line.sent = self.chars().take(self.cursor).collect();
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
        self.line.sent = self
            .chars()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, c)| c)
            .collect();
    }

    pub fn add_markup(&mut self, pos: usize, token: &str) {
        if pos <= self.chars().count() {
            self.markup.add(pos, token.to_string());
        }
    }

    pub fn remove_markup(&mut self, pos: usize) {
        self.markup.remove(pos);
    }

    pub fn clear_markup(&mut self) {
        self.markup.clear_all();
    }

    #[must_use]
    pub fn markup(&self) -> &BTreeMap<usize, String> {
        &self.markup.tokens
    }

    #[must_use]
    pub fn decorated_value(&self) -> String {
        let content_str = match (&self.line.sent, &self.line.original) {
            (s, _) if !s.is_empty() => s,
            (_, Some(orig)) => orig,
            _ => return String::new(),
        };

        let content_str = match self.line.echo {
            EchoState::Password => "*".repeat(content_str.chars().count()),
            EchoState::Enabled => content_str.to_string(),
        };

        let char_count = content_str.chars().count();

        if self.markup.tokens.is_empty() {
            return content_str;
        }

        let total_tokens_len: usize = self.markup.tokens.values().map(String::len).sum();
        let mut result = String::with_capacity(char_count + total_tokens_len);

        let mut chars = content_str.chars();
        let mut current_pos = 0;

        for (pos, token) in &self.markup.tokens {
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
}

impl Display for Input {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.line.sent)
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

#[derive(Debug, Clone, Default)]
struct Markup {
    tokens: BTreeMap<usize, String>,
}

impl Markup {
    fn add(&mut self, pos: usize, token: String) {
        self.tokens.insert(pos, token);
    }

    fn remove(&mut self, pos: usize) {
        self.tokens.remove(&pos);
    }

    fn clear_all(&mut self) {
        self.tokens.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format() {
        let mut input = Input::default();
        input.set_value(InputLine::new(TEXT.to_owned(), true, false));
        println!("{input}");
    }

    #[test]
    fn set_cursor() {
        let line = InputLine::new(TEXT.to_owned(), true, false);
        let mut input = Input::default();
        input.set_value(line.clone());

        input.set_cursor(3);
        assert_eq!(input.value(), line);
        assert_eq!(input.cursor(), 3);

        input.set_cursor(30);
        assert_eq!(input.cursor(), TEXT.chars().count());

        input.set_cursor(TEXT.chars().count());
        assert_eq!(input.cursor(), TEXT.chars().count());
    }

    #[test]
    fn insert_char() {
        let line = InputLine::new(TEXT.to_owned(), true, false);
        let mut input = Input::default();
        input.set_value(line);

        input.insert('x');
        assert_eq!(input.value().sent, "first second, third.x");
        assert_eq!(input.cursor(), TEXT.chars().count() + 1);
        input.insert('x');
        assert_eq!(input.value().sent, "first second, third.xx");
        assert_eq!(input.cursor(), TEXT.chars().count() + 2);

        input.set_cursor(3);
        input.insert('x');
        assert_eq!(input.value().sent, "firxst second, third.xx");
        assert_eq!(input.cursor(), 4);

        input.insert('x');
        assert_eq!(input.value().sent, "firxxst second, third.xx");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn go_to_prev_char() {
        let line = InputLine::new(TEXT.to_owned(), true, false);
        let mut input = Input::default();
        input.set_value(line);

        input.cursor_left();
        assert_eq!(input.cursor(), TEXT.chars().count() - 1);

        input.set_cursor(3);
        input.cursor_left();
        assert_eq!(input.cursor(), 2);

        input.cursor_left();
        assert_eq!(input.cursor(), 1);
    }

    #[test]
    fn remove_unicode_chars() {
        let line = InputLine::new("¡test¡".to_owned(), true, false);
        let mut input = Input::default();
        input.set_value(line);

        input.delete_prev();
        assert_eq!(input.value().sent, "¡test");
        assert_eq!(input.cursor(), 5);

        input.cursor_start();
        input.delete_next();
        assert_eq!(input.value().sent, "test");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn insert_unicode_chars() {
        let line = InputLine::new("¡test¡".to_owned(), true, false);
        let mut input = Input::default();
        input.set_value(line);

        input.set_cursor(5);

        input.insert('☆');
        assert_eq!(input.value().sent, "¡test☆¡");
        assert_eq!(input.cursor(), 6);

        input.cursor_start();
        input.cursor_right();
        input.insert('☆');
        assert_eq!(input.value().sent, "¡☆test☆¡");
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn multispace_characters() {
        let line = InputLine::new("Ｈｅｌｌｏ, ｗｏｒｌｄ!".to_owned(), true, false);
        let mut input = Input::default();
        input.set_value(line);

        assert_eq!(input.cursor(), 13);
        assert_eq!(input.visual_cursor(), 23);
        assert_eq!(input.visual_scroll(6), 18);
    }

    const TEXT: &str = "first second, third.";
}
