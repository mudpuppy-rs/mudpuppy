use std::collections::HashMap;

use ansi_to_tui::IntoText;
use pyo3::{Py, Python};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::prelude::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::error::{Error, ErrorKind};
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::session::Input;

/// The input area for a character session
///
/// Allows the user to enter buffered text, to be processed/transmitted when the
/// enter key is pressed.
// TODO(XXX): border/style customization.
pub(crate) fn draw(
    frame: &mut Frame<'_>,
    input: &Py<Input>,
    sections: &HashMap<String, Rect>,
) -> Result<(), Error> {
    // Safety: not possible to remove sections, and we initialize this one ourselves and know
    // it exists.
    let area = sections.get(SECTION_NAME).unwrap();
    let width = area.width.max(3) - 3;

    let (scroll, cursor, input_text) = Python::with_gil(|py| {
        let input = input.borrow(py);
        let scroll = input.visual_scroll(width as usize);
        let cursor = input.visual_cursor();
        let input_text = input
            .decorated_value(py)
            .into_text()
            .map_err(ErrorKind::from)?;
        Ok::<_, Error>((scroll, cursor, input_text))
    })?;

    let input_text = Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .style(Style::default().fg(Color::White))
        .scroll((0, u16::try_from(scroll).unwrap_or_default()));

    frame.render_widget(input_text, *area);

    let cursor_x = area.x + u16::try_from(cursor.max(scroll) - scroll).unwrap_or_default() + 1;

    frame.set_cursor_position(Position::from((cursor_x, area.y + 1)));

    Ok(())
}

impl TryFrom<crossterm::event::KeyEvent> for KeyEvent {
    type Error = String;

    fn try_from(event: crossterm::event::KeyEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            code: event.code.try_into()?,
            modifiers: event.modifiers.into(),
        })
    }
}

impl TryFrom<crossterm::event::KeyCode> for KeyCode {
    type Error = String;

    fn try_from(code: crossterm::event::KeyCode) -> Result<Self, Self::Error> {
        use crossterm::event::KeyCode;
        Ok(match code {
            KeyCode::Char(c) => Self::Char(c),
            KeyCode::F(n) => Self::F(n),
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Enter => Self::Enter,
            KeyCode::Left => Self::Left,
            KeyCode::Right => Self::Right,
            KeyCode::Up => Self::Up,
            KeyCode::Down => Self::Down,
            KeyCode::Home => Self::Home,
            KeyCode::End => Self::End,
            KeyCode::PageUp => Self::PageUp,
            KeyCode::PageDown => Self::PageDown,
            KeyCode::Tab => Self::Tab,
            KeyCode::Delete => Self::Delete,
            KeyCode::Insert => Self::Insert,
            KeyCode::Esc => Self::Esc,
            c => return Err(format!("unknown key code: {c:?}")),
        })
    }
}

impl From<crossterm::event::KeyModifiers> for KeyModifiers {
    fn from(mods: crossterm::event::KeyModifiers) -> Self {
        use crossterm::event::KeyModifiers;

        let mut result = Self::NONE;
        if mods.contains(KeyModifiers::SHIFT) {
            result.insert(Self::SHIFT);
        }
        if mods.contains(KeyModifiers::CONTROL) {
            result.insert(Self::CONTROL);
        }
        if mods.contains(KeyModifiers::ALT) {
            result.insert(Self::ALT);
        }
        result
    }
}

pub(super) const SECTION_NAME: &str = "commandline";
