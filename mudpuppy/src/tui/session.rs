use std::collections::HashMap;

use pyo3::{Py, Python};
use ratatui::Frame;
use ratatui::layout::{Direction, Layout, Margin, Rect};
use ratatui::prelude::Line;
use ratatui::widgets::Clear;
use tracing::{debug, error, warn};

use crate::app::{AppData, TabAction};
use crate::error::Error;
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::python::{self, Event};
use crate::session::{Buffer, InputLine, OUTPUT_BUFFER_NAME, OutputItem};
use crate::shortcut::{InputShortcut, ScrollShortcut, Shortcut};
use crate::tui::chrome::{TabData, TabKind};
use crate::tui::{Constraint, Section, Tab, buffer, commandline};

#[derive(Debug)]
pub(crate) struct Character {
    pub(crate) sesh: python::Session,
}

impl Character {
    pub(crate) fn new_tab(sesh: python::Session) -> Tab {
        Tab {
            data: TabData::new(
                sesh.character.name.clone(),
                initial_layout(),
                Some(default_shortcuts()),
            ),
            kind: TabKind::Session(Box::new(Self { sesh })),
        }
    }

    pub(crate) fn render_title(&self, app: &AppData, tab_data: &TabData) -> Line<'_> {
        let Ok(sesh) = app.session(self.sesh.id) else {
            return Line::from(tab_data.title.clone());
        };

        if app.active_session == Some(self.sesh.id) {
            return Line::from(tab_data.title.clone());
        }

        let new_data = sesh.output.new_data();
        let unread = if new_data > 0 {
            format!(" ({new_data})")
        } else {
            String::new()
        };

        format!("{}{}", tab_data.title, unread).into()
    }

    pub(crate) fn render(
        &mut self,
        app: &mut AppData,
        f: &mut Frame<'_>,
        sections: &HashMap<String, Rect>,
    ) -> Result<(), Error> {
        let session = app.session_mut(self.sesh.id)?;

        // TODO(XXX): spammy, remove after debugging:
        /*trace!("\n\n");
        for (name, rect) in &sections {
            trace!("{name} -> {rect}");
        }*/

        // Safety: we unconditionally create this section in layout init.
        let output_section = sections.get(OUTPUT_BUFFER_NAME).unwrap();

        let output_dimensions = (output_section.width, output_section.height);
        if session.output.dimensions != output_dimensions {
            session
                .event_handlers
                .session_event(&Event::BufferResized {
                    name: OUTPUT_BUFFER_NAME.to_string(),
                    from: session.output.dimensions.into(),
                    to: output_dimensions.into(),
                })?;
            session.output.dimensions = output_dimensions;
        }

        // If we're scrolling and there was new data received, move the scroll position
        // up by the amount of new data so that the scroll window remains at the same
        // point relative to where it was before the new data was received.
        //
        // We do this _before_ drawing the output buffer because the act of draining the
        // new data to draw will clear the new data count.
        let new_data = session.output.new_data();
        if session.scrollback.scroll_pos != 0 && new_data > 0 {
            session
                .scrollback
                .scroll_up(u16::try_from(new_data).unwrap_or(u16::MAX));
        }

        // TODO(XXX): held prompt setting.
        let prompt = if session.prompt.content().is_empty() {
            None
        } else {
            Some(OutputItem::HeldPrompt {
                prompt: session.prompt.content().into(),
            })
        };

        buffer::draw(
            f,
            &mut session.output,
            None,
            prompt.as_ref(),
            // TODO(XXX): filtering settings.
            |item| !matches!(item, OutputItem::Prompt { .. }),
            *output_section,
        )?;

        if session.scrollback.scroll_pos != 0 {
            draw_scrollback(
                f,
                *output_section,
                &mut session.scrollback,
                &mut session.output,
            )?;
        }

        commandline::draw(f, &session.input, sections)?;

        for (name, buffer) in &session.extra_buffers {
            let Some(output_section) = sections.get(name) else {
                // TODO(XXX): fuse some kind of warning/error
                continue;
            };
            Python::attach(|py| {
                let mut buffer = buffer.borrow_mut(py);
                // TODO(XXX): filtering settings.
                buffer::draw(f, &mut buffer, None, None, |_| true, *output_section)
            })?;
        }

        Ok(())
    }

    pub(crate) async fn shortcut(
        &mut self,
        app: &mut AppData,
        shortcut: &Shortcut,
    ) -> Result<Option<TabAction>, Error> {
        match shortcut {
            Shortcut::SessionInput(InputShortcut::Send) => {}
            Shortcut::SessionInput(shortcut) => {
                return Python::attach(|py| {
                    let mut input = app.session_mut(self.sesh.id)?.input.borrow_mut(py);
                    input.shortcut(shortcut);
                    Ok(None)
                });
            }
            Shortcut::Scroll(shortcut) => {
                scroll_shortcut(&mut app.session_mut(self.sesh.id)?.scrollback, shortcut);
                return Ok(None);
            }
            _ => return Ok(None),
        }

        // Pop whatever input has been queued.
        let input = Python::attach(|py| {
            let session = app.active_session_mut().unwrap();
            session.input.borrow_mut(py).pop().unwrap_or_default()
        });

        // If the input line has the command prefix, dispatch the input line as a command.
        // TODO(XXX): configurable prefix.
        if let Some(cmd_name) = input.sent.strip_prefix('/') {
            return dispatch_command(app, &input, cmd_name).await;
        }

        // Otherwise, send the input line to the session (if connected).
        let session = app.active_session_mut().unwrap();
        if session.connected().is_some() {
            session.send_line(input, false).map(|()| None)
        } else {
            session.output.add(OutputItem::CommandResult {
                error: true,
                message: "Not connected".to_string(),
            });
            Ok(None)
        }
    }

    pub(crate) fn key_event(
        &mut self,
        app: &mut AppData,
        key_event: &KeyEvent,
    ) -> Result<Option<TabAction>, Error> {
        Python::attach(|py| {
            let mut input = app.session_mut(self.sesh.id)?.input.borrow_mut(py);
            input.key_event(key_event);
            Ok(None)
        })
    }
}

fn scroll_shortcut(scrollback: &mut Buffer, shortcut: &ScrollShortcut) {
    let scroll_lines = 5; // TODO(XXX): Setting for scroll lines
    match shortcut {
        ScrollShortcut::Up => {
            scrollback.scroll_up(scroll_lines);
        }
        ScrollShortcut::Down => {
            scrollback.scroll_down(scroll_lines);
        }
        ScrollShortcut::Top => {
            scrollback.scroll_max();
        }
        ScrollShortcut::Bottom => {
            scrollback.scroll_bottom();
        }
    }
}

async fn dispatch_command(
    app: &mut AppData,
    line: &InputLine,
    name: &str,
) -> Result<Option<TabAction>, Error> {
    let mut parts = name.splitn(2, ' ');
    let cmd_name = parts.next().unwrap_or_default();
    let remaining = parts.next().unwrap_or_default();

    let cmd = {
        let Some(active_session) = app.active_session_mut() else {
            return Ok(None);
        };

        let _ = active_session
            .event_handlers
            .session_event(&Event::InputLine { line: line.clone() });
        active_session
            .output
            .add(OutputItem::Input { line: line.clone() });

        let Some(cmd) = active_session.slash_commands.get(cmd_name).cloned() else {
            let message = format!("unknown slash command: {cmd_name}");
            warn!(message);
            active_session.output.add(OutputItem::CommandResult {
                error: true,
                message,
            });
            return Ok(None);
        };
        cmd
    };

    debug!("executing slash command: {cmd_name} {remaining}");
    let res = cmd.execute(app, remaining.to_string()).await;

    Ok(match res {
        Ok(Some(tab_action)) => Some(tab_action),
        Ok(None) => None,
        Err(e) => {
            let message = format!("error executing slash command {cmd_name}: {e}");
            error!(message);
            let output = app.active_session_mut().map(|sesh| &mut sesh.output);
            if let Some(output) = output {
                output.add(OutputItem::CommandResult {
                    error: true,
                    message,
                });
            }
            None
        }
    })
}

fn draw_scrollback(
    f: &mut Frame,
    output_area: Rect,
    scrollback: &mut Buffer,
    output: &mut Buffer,
) -> Result<(), Error> {
    // Create a sub area of the overall buffer area where we can draw the scroll window.
    // We don't create this as a fixed layout section because we want it sized relative
    // to the existing fixed `MudBuffer` output section.
    let area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Percentage(70), // TODO(XXX): scrollback percentage setting.
            ratatui::layout::Constraint::Min(1),
        ])
        .split(output_area)[0];

    // Render the scrollback content and the scrollbar inside a viewport offset within the
    // overall area.
    let viewport = area.inner(Margin {
        vertical: 0,   // TODO(XXX): scrollback margin vertical setting.
        horizontal: 6, // TODO(XXX): scrollback margin horizontal setting.
    });
    // Make sure to clear the viewport first - we're drawing on top of the already rendered
    // normal buffer content.
    f.render_widget(Clear, viewport);

    buffer::draw(
        f,
        scrollback,
        Some(output),
        None,
        |_| true, // TODO(XXX): filtering
        viewport,
    )
}

fn initial_layout() -> Py<Section> {
    Python::attach(|py| {
        debug!("configuring initial layout");
        let output = Section::new(py, OUTPUT_BUFFER_NAME.to_string());
        let commandline = Section::new(py, commandline::SECTION_NAME.to_string());
        let mut session = Section::new(py, SESSION_SECTION_NAME.to_string());
        session.append_child(py, Constraint::Percentage(95), output)?;
        session.append_child(py, Constraint::Min(3), commandline)?;
        let mut root = Section::new(py, ROOT_SECTION_NAME.to_string());
        root.append_child(py, Constraint::Percentage(100), session)?;
        Py::new(py, root)
    })
    .unwrap() // Safety: no chance for duplicate sections.
}

#[allow(clippy::too_many_lines)]
pub(crate) fn default_shortcuts() -> HashMap<KeyEvent, Shortcut> {
    HashMap::from([
        // ENTER -> Send input
        (
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::Send.into(),
        ),
        // BACKSPACE or Ctrl-h -> Delete prev char
        (
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::DeletePrev.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('h'),
            },
            InputShortcut::DeletePrev.into(),
        ),
        // DELETE -> Delete next char
        (
            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::DeleteNext.into(),
        ),
        // LEFT or Ctrl-b -> Cursor left
        (
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::CursorLeft.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('b'),
            },
            InputShortcut::CursorLeft.into(),
        ),
        // Ctrl-LEFT or Alt-b -> Cursor word left
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Left,
            },
            InputShortcut::CursorWordLeft.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::ALT,
                code: KeyCode::Char('b'),
            },
            InputShortcut::CursorWordLeft.into(),
        ),
        // RIGHT or Ctrl-f -> Cursor right
        (
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::CursorRight.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('f'),
            },
            InputShortcut::CursorRight.into(),
        ),
        // Ctrl-RIGHT or Alt-f -> Cursor word right
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Right,
            },
            InputShortcut::CursorWordRight.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::ALT,
                code: KeyCode::Char('f'),
            },
            InputShortcut::CursorWordRight.into(),
        ),
        // CTRL-u -> Reset
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('u'),
            },
            InputShortcut::Reset.into(),
        ),
        // Alt-BACKSPACE or CTRL-w -> Delete word left
        (
            KeyEvent {
                modifiers: KeyModifiers::ALT,
                code: KeyCode::Backspace,
            },
            InputShortcut::CursorDeleteWordLeft.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('w'),
            },
            InputShortcut::CursorDeleteWordLeft.into(),
        ),
        // Ctrl-DELETE -> Delete word right
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Delete,
            },
            InputShortcut::CursorDeleteWordRight.into(),
        ),
        // Ctrl-k -> Delete to end
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('k'),
            },
            InputShortcut::CursorDeleteToEnd.into(),
        ),
        // HOME or Ctrl-a -> Cursor start
        (
            KeyEvent {
                code: KeyCode::Home,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::CursorToStart.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('a'),
            },
            InputShortcut::CursorToStart.into(),
        ),
        // END or Ctrl-e -> Cursor end
        (
            KeyEvent {
                code: KeyCode::End,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::CursorToEnd.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('e'),
            },
            InputShortcut::CursorToEnd.into(),
        ),
        // PAGE-UP -> Scroll up
        (
            KeyEvent {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
            },
            ScrollShortcut::Up.into(),
        ),
        // PAGE-DOWN -> Scroll down
        (
            KeyEvent {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
            },
            ScrollShortcut::Down.into(),
        ),
        // SHIFT-HOME -> Scroll top
        (
            KeyEvent {
                modifiers: KeyModifiers::SHIFT,
                code: KeyCode::Home,
            },
            ScrollShortcut::Top.into(),
        ),
        // SHIFT-END -> Scroll bottom
        (
            KeyEvent {
                modifiers: KeyModifiers::SHIFT,
                code: KeyCode::End,
            },
            ScrollShortcut::Bottom.into(),
        ),
    ])
}

const ROOT_SECTION_NAME: &str = "root";
const SESSION_SECTION_NAME: &str = "session";
