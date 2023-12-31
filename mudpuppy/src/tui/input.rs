use ratatui::layout::Rect;
use ratatui::prelude::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::collections::HashMap;

use crate::client::input as client_input;
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

        let value = match input.echo() {
            client_input::EchoState::Enabled => input.value().to_string(),
            client_input::EchoState::Password => "*".repeat(input.value().len()),
        };

        let width = area.width.max(3) - 3;
        let scroll = input.visual_scroll(width as usize);
        let input_text = Paragraph::new(value)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .style(Style::default().fg(Color::White))
            .scroll((0, u16::try_from(scroll).unwrap_or_default()));

        frame.render_widget(input_text, *area);
        frame.set_cursor(
            area.x
                + u16::try_from(input.visual_cursor().max(scroll) - scroll).unwrap_or_default()
                + 1,
            area.y + 1,
        );
        Ok(())
    }
}

pub const INPUT_SECTION_NAME: &str = "input_area";
