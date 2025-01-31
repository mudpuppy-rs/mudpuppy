use ratatui::layout::{Position, Rect};
use ratatui::prelude::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::collections::HashMap;

use crate::client::input as client_input;
use crate::client::input::EchoState;
use crate::error::Error;
use crate::Result;

#[derive(Debug, Default)]
pub(super) struct Input {}

impl Input {
    pub fn draw(
        input: &mut client_input::Input,
        frame: &mut Frame<'_>,
        sections: &HashMap<String, Rect>,
    ) -> Result<()> {
        let area = sections
            .get(INPUT_SECTION_NAME)
            .ok_or(Error::LayoutMissing(INPUT_SECTION_NAME.to_string()))?;

        let content = input.value();
        let mut content_str = content.sent;
        if content_str.is_empty() && content.original.is_some() {
            content_str = content.original.unwrap();
        }

        if content.echo == EchoState::Password {
            content_str = "*".repeat(content_str.len());
        }

        let width = area.width.max(3) - 3;
        let scroll = input.visual_scroll(width as usize);
        let input_text = Paragraph::new(content_str.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .style(Style::default().fg(Color::White))
            .scroll((0, u16::try_from(scroll).unwrap_or_default()));

        frame.render_widget(input_text, *area);

        let cursor_x = area.x
            + u16::try_from(input.visual_cursor().max(scroll) - scroll).unwrap_or_default()
            + 1;

        frame.set_cursor_position(Position::from((cursor_x, area.y + 1)));
        Ok(())
    }
}

pub const INPUT_SECTION_NAME: &str = "input_area";
