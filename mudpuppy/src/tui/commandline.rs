use std::collections::HashMap;

use ansi_to_tui::IntoText;
use pyo3::{Py, Python};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::prelude::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::error::{Error, ErrorKind};
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

    let (scroll, cursor, input_text) = Python::attach(|py| {
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

pub(super) const SECTION_NAME: &str = "commandline";
