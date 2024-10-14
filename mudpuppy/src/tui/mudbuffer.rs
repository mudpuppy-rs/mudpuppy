use std::collections::HashMap;

use ansi_to_tui::IntoText;
use deref_derive::{Deref, DerefMut};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

use crate::client::{output, Status};
use crate::error::Error;
use crate::model::{InputLine, Mud, SessionId};
use crate::tui::buffer;
use crate::tui::buffer::DrawScrollbar;
use crate::tui::layout::BufferConfig;
use crate::{client, python, Result};

#[derive(Debug, Deref, DerefMut)]
pub(super) struct MudBuffer {
    session_id: SessionId,
    mud: Mud,
    #[deref]
    buff: BufferConfig,
}

impl MudBuffer {
    pub(super) fn new(mud: Mud, session_id: SessionId) -> Result<Self> {
        let mut buff = BufferConfig::new(OUTPUT_SECTION_NAME.to_string())?;
        buff.line_wrap = !mud.no_line_wrap;
        Ok(Self {
            session_id,
            mud,
            buff,
        })
    }

    pub(super) fn reload_config(&mut self, mud: Mud) {
        self.buff.line_wrap = !mud.no_line_wrap;
        self.mud = mud;
    }

    pub(super) fn draw_buffer(
        &mut self,
        session: &mut client::Client,
        event_tx: &UnboundedSender<python::Event>,
        f: &mut Frame<'_>,
        sections: &HashMap<String, Rect>,
    ) -> Result<()> {
        let area = sections
            .get(OUTPUT_SECTION_NAME)
            .ok_or(Error::LayoutMissing(OUTPUT_SECTION_NAME.to_string()))?;

        // Handle sending a resize event if the area we're rendering into
        // has changed size since the last render.
        let current_dimensions = (area.width, area.height);
        if session.buffer_dimensions != current_dimensions {
            session.buffer_dimensions = current_dimensions;
            info!(
                "session buffer resized to {}x{}",
                current_dimensions.0, current_dimensions.1
            );
            event_tx.send(python::Event::BufferResized {
                id: self.session_id,
                dimensions: current_dimensions,
            })?;
        }

        // We may display a held prompt at the bottom of all the normal output.
        let prompt = if self.mud.hold_prompt {
            session
                .prompt
                .as_ref()
                .map(|prompt| output::Item::HeldPrompt {
                    prompt: prompt.clone(),
                })
        } else {
            None
        };
        // This is accomplished using a special iterator that wraps the session's received data.
        let buff_iter =
            HeldPromptIterator::new(session.output.read_received().iter(), prompt.as_ref());

        buffer::draw(
            f,
            &mut self.buff,
            buff_iter,
            |item| {
                match item {
                    // Hide gagged MUD items
                    // TODO(XXX): Offer a way to disable gagging for troubleshooting?
                    output::Item::Mud { line } if line.gag => false,
                    // Hide input items when echo_input is disabled. The user doesn't want to see
                    // their own input displayed in the output buff.
                    output::Item::Input { .. } if !self.mud.echo_input => false,
                    // Hide prompt items when hold_prompt is true. The held prompt will supersede
                    // historic prompts.
                    output::Item::Prompt { .. } if self.mud.hold_prompt => false,
                    // Hide gagged prompts.
                    output::Item::Prompt { prompt } if prompt.gag => false,
                    output::Item::HeldPrompt { prompt } if prompt.gag => false,
                    _ => true,
                }
            },
            area,
            DrawScrollbar::Never,
        )
    }
}

// HeldPromptIterator is a double ended iterator that wraps _another_ double ended
// iterator, augmenting it with one extra OutputItem at the end. This is useful for
// displaying a held prompt at a fixed position after the contents of a buffer of
// OutputItems.
struct HeldPromptIterator<'a, DI>
where
    DI: 'a + DoubleEndedIterator<Item = &'a output::Item>,
{
    data_iter: DI,
    held_prompt: Option<&'a output::Item>,
}

impl<'a, DI> HeldPromptIterator<'a, DI>
where
    DI: 'a + DoubleEndedIterator<Item = &'a output::Item>,
{
    fn new(data_iter: DI, held_prompt: Option<&'a output::Item>) -> Self {
        Self {
            data_iter,
            held_prompt,
        }
    }
}

impl<'a, DI> Iterator for HeldPromptIterator<'a, DI>
where
    DI: 'a + DoubleEndedIterator<Item = &'a output::Item>,
{
    type Item = &'a output::Item;

    fn next(&mut self) -> Option<Self::Item> {
        // In the forward iteration direction we yield from the data iterator until
        // it's out of elements, and then yield the held item.
        if let Some(data) = self.data_iter.next() {
            return Some(data);
        }
        self.held_prompt.take()
    }
}

impl<'a, DI> DoubleEndedIterator for HeldPromptIterator<'a, DI>
where
    DI: 'a + DoubleEndedIterator<Item = &'a output::Item>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        // In the reverse iteration direction we consume the held item first and then
        // iterate backwards through the data iterator.
        if let Some(held_item) = self.held_prompt.take() {
            return Some(held_item);
        }
        self.data_iter.next_back()
    }
}

impl<'a, DI> ExactSizeIterator for HeldPromptIterator<'a, DI>
where
    DI: 'a + DoubleEndedIterator<Item = &'a output::Item> + ExactSizeIterator,
{
    fn len(&self) -> usize {
        // The length of the iterator is the length of the data iterator plus one if
        // there is a held item.
        self.data_iter.len() + usize::from(self.held_prompt.is_some())
    }
}

// Describes how to style OutputItems into Text for a buffer to display.
// TODO(XXX): Optimization: cache the calculated Text?
impl buffer::Item for output::Item {
    fn icon(&self) -> Option<Vec<Span<'static>>> {
        match self {
            Self::Mud { .. }
            | Self::Prompt { .. }
            | Self::HeldPrompt { .. }
            | Self::PreviousSession { .. } => None,
            Self::Debug { .. } => Some(vec![Span::styled(
                " 🐛 ",
                Style::default().fg(Color::Green),
            )]),
            Self::Input { .. } => Some(vec![Span::styled(
                " ↳ ",
                Style::default().fg(Color::LightBlue),
            )]),
            Self::ConnectionEvent { status } => Some(vec![Span::styled(
                match status {
                    Status::Connected { .. } => " ✓ ",
                    Status::Connecting { .. } => " ⚙ ",
                    Status::Disconnected { .. } => " ✗ ",
                },
                Style::default().fg(status.into()),
            )]),
            Self::CommandResult { error: true, .. } => Some(vec![Span::styled(
                " ✗ ",
                Style::default().fg(Color::LightRed),
            )]),
            Self::CommandResult { error: false, .. } => Some(vec![Span::styled(
                " ℹ ",
                Style::default().fg(Color::LightBlue),
            )]),
        }
    }

    fn to_text(&self) -> Result<Text<'static>> {
        Ok(match self {
            Self::Mud { line: text }
            | Self::Prompt { prompt: text }
            | Self::HeldPrompt { prompt: text } => String::from_utf8_lossy(&text.raw)
                .to_string()
                .replace('\t', "    ")
                .into_text()?,
            Self::PreviousSession { line, .. } => String::from_utf8_lossy(&line.raw)
                .to_string()
                .replace('\t', "    ")
                .into_text()?
                .style(Style::default().add_modifier(Modifier::DIM)),
            Self::Input { line, .. } => {
                vec![Line::from([self.icon().unwrap(), line.into()].concat())].into()
            }
            Self::ConnectionEvent { status } => {
                vec![Line::from([self.icon().unwrap(), status.into()].concat())].into()
            }
            Self::Debug { line } => vec![Line::from(
                [
                    self.icon().unwrap(),
                    vec![Span::styled(
                        line.clone(),
                        Style::default().fg(Color::Gray).add_modifier(Modifier::DIM),
                    )],
                ]
                .concat(),
            )]
            .into(),
            Self::CommandResult { error, message } => vec![Line::from(
                [
                    self.icon().unwrap(),
                    vec![Span::styled(
                        message.clone(),
                        Style::default().fg(match error {
                            true => Color::LightRed,
                            false => Color::LightBlue,
                        }),
                    )],
                ]
                .concat(),
            )]
            .into(),
        })
    }
}

impl From<&Status> for Vec<Span<'static>> {
    fn from(status: &Status) -> Self {
        vec![Span::styled(
            format!("{status}"),
            Style::default().fg(status.into()),
        )]
    }
}

impl From<&Status> for Color {
    fn from(status: &Status) -> Self {
        match status {
            Status::Connected { .. } => Color::LightBlue,
            Status::Connecting { .. } => Color::LightYellow,
            Status::Disconnected { .. } => Color::LightRed,
        }
    }
}

impl From<&InputLine> for Vec<Span<'static>> {
    fn from(line: &InputLine) -> Self {
        vec![
            Span::styled(format!("{line}"), Style::default().fg(Color::Gray)),
            line.original
                .as_ref()
                .map(|orig| {
                    Span::styled(format!(" ({orig})"), Style::default().fg(Color::DarkGray))
                })
                .unwrap_or_default(),
        ]
    }
}

pub const OUTPUT_SECTION_NAME: &str = "output_area";
