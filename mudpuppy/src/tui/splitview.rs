use deref_derive::{Deref, DerefMut};
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::widgets::Clear;
use ratatui::Frame;
use std::collections::HashMap;

use crate::client::output;
use crate::error::Error;
use crate::model::{Mud, Shortcut};
use crate::tui::buffer;
use crate::tui::buffer::DrawScrollbar;
use crate::tui::layout::BufferConfig;
use crate::tui::mudbuffer::OUTPUT_SECTION_NAME;
use crate::{client, Result};

#[derive(Debug, Deref, DerefMut)]
pub(super) struct ScrollWindow {
    mud: Mud,
    #[deref]
    buff: BufferConfig,
}

impl ScrollWindow {
    pub(super) fn new(mud: Mud) -> Result<Self> {
        let mut buff = BufferConfig::new("split_view".to_string())?;
        buff.line_wrap = !mud.no_line_wrap;
        buff.border_left = true;
        buff.border_right = true;
        buff.border_bottom = true;

        Ok(Self { mud, buff })
    }

    pub(super) fn reload_config(&mut self, mud: Mud) {
        self.buff.line_wrap = !mud.no_line_wrap;
        self.mud = mud;
    }

    pub(super) fn draw_buffer(
        &mut self,
        session: &mut client::Client,
        f: &mut Frame<'_>,
        sections: &HashMap<String, Rect>,
    ) -> Result<()> {
        let area = sections
            .get(OUTPUT_SECTION_NAME)
            .ok_or(Error::LayoutMissing(OUTPUT_SECTION_NAME.to_string()))?;

        // Create a sub area of the overall buffer area where we can draw the scroll window.
        // We don't create this as a fixed layout section because we want it sized relative
        // to the existing fixed `MudBuffer` output section.
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(self.mud.splitview_percentage),
                Constraint::Min(1),
            ])
            .split(*area)[0];

        // Render the scrollback content and the scrollbar inside a viewport offset within the
        // overall area.
        let viewport = area.inner(Margin {
            vertical: self.mud.splitview_margin_horizontal,
            horizontal: self.mud.splitview_margin_vertical,
        });
        // Make sure to clear the viewport first - we're drawing on top of the already rendered
        // normal buffer content.
        f.render_widget(Clear, viewport);

        // We don't use a HeldPromptIterator here because we don't want to hold a prompt in
        // the scrollback buffer.
        let items = session.output.read_received().iter();

        buffer::draw(
            f,
            &mut self.buff,
            items,
            |item| filter_item(item, self.mud.echo_input),
            &viewport,
            DrawScrollbar::Always,
        )
    }

    pub(super) fn handle_shortcut(&mut self, shortcut: &Shortcut) {
        // TODO(XXX): look up scroll line config.
        let scroll_lines = default::SCROLL_LINES;

        match shortcut {
            Shortcut::ScrollUp => {
                self.buff.scroll_up(scroll_lines);
            }
            Shortcut::ScrollDown => {
                self.buff.scroll_down(scroll_lines);
            }
            Shortcut::ScrollTop => {
                self.buff.scroll_max();
            }
            Shortcut::ScrollBottom => {
                self.buff.scroll_bottom();
            }
            _ => {}
        }
    }
}

fn filter_item(item: &output::Item, echo_input: bool) -> bool {
    match item {
        // Hide gagged MUD items
        // TODO(XXX): Offer a way to disable gagging for troubleshooting?
        output::Item::Mud { line } if line.gag => false,

        // Hide input items when echo_input is disabled. The user doesn't want to see their own
        // input displayed in the output buff.
        output::Item::Input { .. } if !echo_input => false,

        // When viewing history we want to see the normal prompt items, but not the held prompt.
        output::Item::HeldPrompt { .. } => false,

        // Show everything else.
        _ => true,
    }
}

mod default {
    pub(super) const SCROLL_LINES: u16 = 5;
}
