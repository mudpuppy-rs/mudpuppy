use std::fmt::Debug;

use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Span, StyledGrapheme, Text};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;
use tracing::trace;
use unicode_width::UnicodeWidthStr;

use crate::client::output;
use crate::error::Error;
use crate::tui::layout::{BufferConfig, BufferDirection};
use crate::tui::reflow::{LineComposer, LineTruncator, WordWrapper, WrappedLine};
use crate::Result;

/// # Errors
/// TODO(XXX): write docs.
pub fn draw<'a, Filter>(
    f: &mut Frame<'_>,
    buffer: &'a mut BufferConfig,
    data_source: impl DoubleEndedIterator<Item = &'a output::Item> + ExactSizeIterator + 'a,
    filter: Filter,
    area: &Rect,
    scrollbar: DrawScrollbar,
) -> Result<()>
where
    Filter: Fn(&&output::Item) -> bool,
{
    if area.height == 0 || area.width == 0 {
        // Don't draw empty buffers.
        return Ok(());
    }

    // Clamp the scroll position within valid range if needed (and update the max_scroll).
    // We do this here because we need to know the size of the area available for rendering
    // to know how many items can be displayed.
    let max_scroll = data_source
        .len()
        .saturating_sub(buffer.area_inside_borders(*area, false).height as usize);
    buffer.max_scroll = max_scroll;
    if buffer.scroll_pos > max_scroll {
        trace!("clamping scroll to max_scroll: {}", max_scroll);
        buffer.scroll_to(max_scroll);
    }

    // Draw a framed block with a border (if borders are configured).
    f.render_widget(
        Paragraph::default().block(Block::default().borders(buffer.borders())),
        *area,
    );

    let is_scrolled = buffer.scroll_pos > 0;
    let draw_scrollbar = match scrollbar {
        DrawScrollbar::IfScrolled => is_scrolled,
        DrawScrollbar::Never => false,
        DrawScrollbar::Always => true,
    };

    render_visible(
        buffer,
        f,
        data_source,
        filter,
        buffer.area_inside_borders(*area, draw_scrollbar),
    )?;

    if draw_scrollbar {
        // Create a scrollbar and position its state.
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        // NB: imprecise - uses unwrapped len.
        let scrollbar_position = buffer.max_scroll - buffer.scroll_pos;
        let mut scrollbar_state =
            ScrollbarState::new(buffer.max_scroll).position(scrollbar_position);

        f.render_stateful_widget(
            scrollbar,
            buffer.area_inside_top_borders(*area),
            &mut scrollbar_state,
        );
    }

    Ok(())
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DrawScrollbar {
    IfScrolled,
    Never,
    Always,
}

/// # Errors
/// TODO(XXX): docs.
// A hacked up combination of `Paragraph::render_paragraph` and `Paragraph::render_text`.
fn render_visible<'a, List, I, Filter>(
    buffer: &'a BufferConfig,
    f: &mut Frame<'_>,
    items: List,
    filter: Filter,
    area: Rect,
) -> Result<()>
where
    List: DoubleEndedIterator<Item = &'a I> + 'a,
    Filter: Fn(&&I) -> bool,
    I: Item + 'a,
{
    let items: Box<dyn Iterator<Item = &'a I> + 'a> = match buffer.direction {
        BufferDirection::TopToBottom => Box::new(items),
        BufferDirection::BottomToTop => Box::new(items.rev()),
    };

    let items = items.skip(buffer.scroll_pos).filter(filter);
    let buf = f.buffer_mut();

    let mut pos = match buffer.direction {
        BufferDirection::TopToBottom => 0,
        BufferDirection::BottomToTop => area.height,
    };

    for item in items {
        // TODO(XXX): Possible optimization, memoization.
        let item = item.to_text()?;

        let styled = item.lines.iter().map(|line| {
            let graphemes = line
                .spans
                .iter()
                .flat_map(|span| span.styled_graphemes(Style::default()));
            let alignment = line.alignment.unwrap_or(Alignment::Left);
            (graphemes, alignment)
        });

        // TODO(XXX): Might be possible to simplify in later rustc versions.
        //            Nightly could build this without the extra let bindings...
        let mut word_wrapper;
        let mut line_truncator;
        let line_composer: &mut dyn LineComposer = if buffer.line_wrap {
            word_wrapper = WordWrapper::new(styled, area.width, false);
            &mut word_wrapper
        } else {
            line_truncator = LineTruncator::new(styled, area.width);
            &mut line_truncator
        };

        let mut lines = Vec::new();
        while let Some(WrappedLine {
            line,
            width,
            alignment,
        }) = line_composer.next_line()
        {
            lines.push((line.to_vec(), width, alignment));
        }
        lines.reverse();

        for (line, width, alignment) in lines {
            if buffer.direction == BufferDirection::BottomToTop && pos == 0
                || buffer.direction == BufferDirection::TopToBottom && pos == area.height
            {
                return Ok(()); // No more space, exit early
            }

            let y = if buffer.direction == BufferDirection::BottomToTop {
                pos - 1
            } else {
                pos
            };
            let mut x = get_line_offset(width, area.width, alignment);
            for StyledGrapheme { symbol, style } in line {
                let width = symbol.width();
                if width == 0 {
                    continue;
                }
                let symbol = if symbol.is_empty() { " " } else { symbol };
                buf.get_mut(area.left() + x, area.top() + y)
                    .set_symbol(symbol)
                    .set_style(style);
                x += u16::try_from(width)
                    .map_err(|e| Error::Internal(format!("bad symbol width for {symbol}: {e}")))?;
            }

            pos = match buffer.direction {
                BufferDirection::TopToBottom => pos + 1,
                BufferDirection::BottomToTop => pos.saturating_sub(1),
            };
        }
    }

    Ok(()) // Rendered all available lines.
}

pub trait Item: Debug + Send + Sync {
    fn icon(&self) -> Option<Vec<Span<'static>>>;

    /// # Errors
    /// If the item can't be converted to text.
    fn to_text(&self) -> Result<Text<'static>>;
}

const fn get_line_offset(line_width: u16, text_area_width: u16, alignment: Alignment) -> u16 {
    match alignment {
        Alignment::Center => (text_area_width / 2).saturating_sub(line_width / 2),
        Alignment::Right => text_area_width.saturating_sub(line_width),
        Alignment::Left => 0,
    }
}
