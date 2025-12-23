use std::borrow::Cow;
use std::fmt::Debug;

use ansi_to_tui::IntoText;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style, Text};
use ratatui::text::StyledGrapheme;
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar as ScrollbarWidget, ScrollbarOrientation, ScrollbarState,
};
use tracing::trace;
use unicode_width::UnicodeWidthStr;

use crate::error::{Error, ErrorKind};
use crate::session::{Buffer, BufferConfig, BufferDirection, InputLine, OutputItem, Scrollbar};
use crate::tui::reflow::{LineComposer, LineTruncator, WordWrapper, WrappedLine};

pub fn draw<Filter>(
    f: &mut Frame<'_>,
    buffer: &mut Buffer,
    data_buffer: Option<&mut Buffer>,
    buffer_config: &BufferConfig,
    prompt: Option<&OutputItem>,
    filter: Filter,
    area: Rect,
) -> Result<(), Error>
where
    Filter: Fn(&&OutputItem) -> bool,
{
    if area.height == 0 || area.width == 0 {
        // Don't draw empty buffers.
        return Ok(());
    }

    // Clamp the scroll position within valid range if needed (and update the max_scroll).
    // We do this here because we need to know the size of the area available for rendering
    // to know how many items can be displayed.
    let max_scroll = data_buffer
        .as_ref()
        .unwrap_or(&buffer)
        .len()
        .saturating_sub(area_inside_borders(buffer_config, area, false).height as usize);
    buffer.max_scroll = max_scroll;
    if buffer.scroll_pos > max_scroll {
        trace!("clamping scroll to max_scroll: {}", max_scroll);
        buffer.scroll_to(max_scroll);
    }

    // Draw a framed block with a border (if borders are configured).
    f.render_widget(
        Paragraph::default().block(Block::default().borders(buffer_config.into())),
        area,
    );

    let is_scrolled = buffer.scroll_pos > 0;
    let draw_scrollbar = match buffer_config.scrollbar {
        Scrollbar::IfScrolled => is_scrolled,
        Scrollbar::Never => false,
        Scrollbar::Always => true,
    };

    render_visible(
        f,
        buffer,
        data_buffer,
        buffer_config,
        prompt,
        filter,
        area_inside_borders(buffer_config, area, draw_scrollbar),
    )?;

    if draw_scrollbar {
        let scrollbar = ScrollbarWidget::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        // When Scrollbar::Always is set, ensure we have a non-zero content size for the
        // scrollbar widget, otherwise it won't render. Use the viewport height so the thumb
        // fills the entire track when there's no scrollable content.
        let scrollbar_content_size = if buffer.max_scroll == 0 {
            area_inside_borders(buffer_config, area, draw_scrollbar).height as usize
        } else {
            buffer.max_scroll
        };
        // NB: imprecise - uses unwrapped len.
        let scrollbar_position = buffer.max_scroll - buffer.scroll_pos;

        let mut scrollbar_state =
            ScrollbarState::new(scrollbar_content_size).position(scrollbar_position);

        f.render_stateful_widget(
            scrollbar,
            area_inside_top_borders(buffer_config, area),
            &mut scrollbar_state,
        );
    }

    Ok(())
}

// A hacked up combination of `Paragraph::render_paragraph` and `Paragraph::render_text`.
fn render_visible<Filter>(
    f: &mut Frame<'_>,
    buffer: &mut Buffer,
    data_buffer: Option<&mut Buffer>,
    buffer_config: &BufferConfig,
    prompt: Option<&OutputItem>,
    filter: Filter,
    area: Rect,
) -> Result<(), Error>
where
    Filter: Fn(&&OutputItem) -> bool,
{
    let scroll_pos = buffer.scroll_pos;
    let direction = buffer_config.direction;
    let line_wrap = buffer_config.line_wrap;

    let items =
        HeldPromptIterator::new(data_buffer.unwrap_or(buffer).take_received().iter(), prompt);

    let items: Box<dyn Iterator<Item = &'_ OutputItem> + '_> = match direction {
        BufferDirection::TopToBottom => Box::new(items),
        BufferDirection::BottomToTop => Box::new(items.rev()),
    };

    let items = items.skip(scroll_pos).filter(filter);

    let mut pos = match direction {
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
        let line_composer: &mut dyn LineComposer = if line_wrap {
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
            if direction == BufferDirection::BottomToTop && pos == 0
                || direction == BufferDirection::TopToBottom && pos == area.height
            {
                return Ok(()); // No more space, exit early
            }

            let y = if direction == BufferDirection::BottomToTop {
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

                let (symbol, width) = simplify_complex_emoji(symbol);

                let buf = f.buffer_mut();
                buf[(area.left() + x, area.top() + y)]
                    .set_symbol(&symbol)
                    .set_style(style);
                x += u16::try_from(width).map_err(|e| {
                    ErrorKind::Internal(format!("bad symbol width for {symbol}: {e}"))
                })?;
            }

            pos = match direction {
                BufferDirection::TopToBottom => pos + 1,
                BufferDirection::BottomToTop => pos.saturating_sub(1),
            };
        }
    }

    Ok(()) // Rendered all available lines.
}

const fn get_line_offset(line_width: u16, text_area_width: u16, alignment: Alignment) -> u16 {
    match alignment {
        Alignment::Center => (text_area_width / 2).saturating_sub(line_width / 2),
        Alignment::Right => text_area_width.saturating_sub(line_width),
        Alignment::Left => 0,
    }
}

/// Simplify symbols that are a sequence of emoji
///
/// Presently Ratatui doesn't handle these correctly, resulting in buffer corruption when
/// the cells they occupy are redrawn.
///
/// In most cases we can hack around this by just extracting the first emoji from the overall
/// grapheme symbol. We can't use unicode segmentation for this because it's _already_ produced
/// this `symbol`.
///
/// For most cases we drop everything but the first emoji. For regional indicator flags
/// we preserve both characters, since that seems to render OK and returning just the first
/// symbol makes the content unreadable.
///
/// This only picks up subsequent emoji in the supplementary plane (leading 0xF0) but in
/// practice that seems sufficient for this hacky workaround.
// TODO(XXX): revisit as upstream support evolves.
fn simplify_complex_emoji(symbol: &str) -> (Cow<'_, str>, usize) {
    // Too small to be of interest.
    if symbol.len() <= 3 {
        return (symbol.into(), symbol.width());
    }

    let mut chars = symbol.chars();
    let is_regional = |c: char| {
        let cp = c as u32;
        (0x1F1E6..=0x1F1FF).contains(&cp)
    };
    if let Some(first_char) = chars.next() {
        if is_regional(first_char) {
            if let Some(second_char) = chars.next() {
                if is_regional(second_char) {
                    let mut flag_str = String::with_capacity(8);
                    flag_str.push(first_char);
                    flag_str.push(second_char);
                    let width = flag_str.width();
                    return (Cow::Owned(flag_str), width);
                }
            }
        }
    }

    let bytes = symbol.as_bytes();
    let first_part_end = bytes
        .iter()
        .enumerate()
        .skip(3)
        .find(|(_, b)| **b == 0xF0)
        .map_or(bytes.len(), |(i, _)| i);

    let symbol = String::from_utf8_lossy(&bytes[0..first_part_end]);
    let width = symbol.width();
    (symbol, width)
}

pub trait BufferItem: Debug + Send + Sync {
    fn icon(&self) -> Option<Vec<Span<'static>>>;

    fn to_text(&self) -> Result<Text<'static>, Error>;
}

// Describes how to style OutputItems into Text for a buffer to display.
// TODO(XXX): Optimization: cache the calculated Text?
impl BufferItem for OutputItem {
    fn icon(&self) -> Option<Vec<Span<'static>>> {
        match self {
            Self::Mud { .. } | Self::Prompt { .. } | Self::HeldPrompt { .. } => None,
            Self::Debug { .. } => Some(vec![Span::styled(
                " 🐛 ",
                Style::default().fg(Color::Green),
            )]),
            Self::Input { .. } => Some(vec![Span::styled(
                " ↳ ",
                Style::default().fg(Color::LightBlue),
            )]),
            Self::ConnectionEvent { .. } => Some(vec![Span::styled(
                " 💻 ",
                Style::default().fg(Color::LightBlue),
            )]),
            Self::CommandResult { error: true, .. } | Self::Error { .. } => {
                Some(vec![Span::styled(
                    " ✗ ",
                    Style::default().fg(Color::LightRed),
                )])
            }
            Self::CommandResult { error: false, .. } => Some(vec![Span::styled(
                " ℹ ",
                Style::default().fg(Color::LightBlue),
            )]),
        }
    }

    fn to_text(&self) -> Result<Text<'static>, Error> {
        Ok(match self {
            Self::Mud { line: text } | Self::Prompt { prompt: text } => {
                String::from_utf8_lossy(&text.raw)
                    .to_string()
                    .clean()
                    .into_text()
                    .map_err(ErrorKind::from)?
            }
            Self::HeldPrompt { prompt } => prompt.into_text().map_err(ErrorKind::from)?,
            Self::Input { line, .. } => {
                vec![Line::from([self.icon().unwrap(), line.into()].concat())].into()
            }
            Self::ConnectionEvent { message, info } => {
                // TODO(XXX): revisit connection event rendering
                vec![Line::from(
                    [
                        self.icon().unwrap(),
                        vec![Span::styled(
                            format!(
                                "{}{}",
                                message,
                                info.as_ref()
                                    .map(|info| format!(" {info}"))
                                    .unwrap_or_default()
                            ),
                            Style::default().fg(Color::LightBlue),
                        )],
                    ]
                    .concat(),
                )]
                .into()
            }
            Self::Debug { line } => {
                let mut content = line.clean().into_text().map_err(ErrorKind::from)?;
                for line in &mut content.lines {
                    line.style = Style::default().add_modifier(Modifier::DIM);
                    if let Some(spans) = self.icon() {
                        line.spans.splice(0..0, spans);
                    }
                }
                content
            }
            Self::Error { message } => {
                let mut content = message.clean().into_text().map_err(ErrorKind::from)?;
                for line in &mut content.lines {
                    line.style = Style::default().fg(Color::LightRed);
                    if let Some(spans) = self.icon() {
                        line.spans.splice(0..0, spans);
                    }
                }
                content
            }
            Self::CommandResult { error, message } => {
                let mut content = message.clean().into_text().map_err(ErrorKind::from)?;
                for line in &mut content.lines {
                    line.style = Style::default().fg(match error {
                        true => Color::LightRed,
                        false => line.style.fg.unwrap_or(Color::LightBlue),
                    });
                    if let Some(spans) = self.icon() {
                        line.spans.splice(0..0, spans);
                    }
                }
                content
            }
        })
    }
}

trait CleanText {
    fn clean(&self) -> String;
}

impl CleanText for String {
    fn clean(&self) -> String {
        self.replace('\t', "    ")
            .chars()
            .filter(|c| !c.is_control() || *c == '\x1B')
            .collect()
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

// HeldPromptIterator is a double ended iterator that wraps _another_ double ended
// iterator, augmenting it with one extra OutputItem at the end. This is useful for
// displaying a held prompt at a fixed position after the contents of a buffer of
// OutputItems.
struct HeldPromptIterator<'a, DI>
where
    DI: 'a + DoubleEndedIterator<Item = &'a OutputItem>,
{
    data_iter: DI,
    held_prompt: Option<&'a OutputItem>,
}

impl<'a, DI> HeldPromptIterator<'a, DI>
where
    DI: 'a + DoubleEndedIterator<Item = &'a OutputItem>,
{
    fn new(data_iter: DI, held_prompt: Option<&'a OutputItem>) -> Self {
        Self {
            data_iter,
            held_prompt,
        }
    }
}

impl<'a, DI> Iterator for HeldPromptIterator<'a, DI>
where
    DI: 'a + DoubleEndedIterator<Item = &'a OutputItem>,
{
    type Item = &'a OutputItem;

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
    DI: 'a + DoubleEndedIterator<Item = &'a OutputItem>,
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
    DI: 'a + DoubleEndedIterator<Item = &'a OutputItem> + ExactSizeIterator,
{
    fn len(&self) -> usize {
        // The length of the iterator is the length of the data iterator plus one if
        // there is a held item.
        self.data_iter.len() + usize::from(self.held_prompt.is_some())
    }
}

fn area_inside_borders(buffer_config: &BufferConfig, mut area: Rect, with_scrollbar: bool) -> Rect {
    if buffer_config.border_top {
        area.height = area.height.saturating_sub(1);
        area.y = area.y.saturating_add(1);
    }
    if buffer_config.border_bottom {
        area.height = area.height.saturating_sub(1);
    }
    if buffer_config.border_left {
        area.width = area.width.saturating_sub(1);
        area.x = area.x.saturating_add(1);
    }
    if buffer_config.border_right {
        area.width = area.width.saturating_sub(1);
    }
    if with_scrollbar {
        area.width = area.width.saturating_sub(1);
    }
    area
}

fn area_inside_top_borders(buffer_config: &BufferConfig, mut area: Rect) -> Rect {
    if buffer_config.border_top {
        area.height = area.height.saturating_sub(1);
        area.y = area.y.saturating_add(1);
    }
    if buffer_config.border_bottom {
        area.height = area.height.saturating_sub(1);
    }
    area
}

impl From<&BufferConfig> for Borders {
    fn from(buff: &BufferConfig) -> Borders {
        let mut borders = Borders::empty();
        if buff.border_top {
            borders |= Borders::TOP;
        }
        if buff.border_bottom {
            borders |= Borders::BOTTOM;
        }
        if buff.border_left {
            borders |= Borders::LEFT;
        }
        if buff.border_right {
            borders |= Borders::RIGHT;
        }
        borders
    }
}
