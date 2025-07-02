use std::collections::HashMap;

use async_trait::async_trait;
use pyo3::{Py, PyRef, Python};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Line;
use tracing::{debug, error, warn};

use crate::app::{AppData, TabAction};
use crate::error::Error;
use crate::keyboard::KeyEvent;
use crate::python::{self, Event};
use crate::session::{OUTPUT_BUFFER_NAME, OutputItem};
use crate::shortcut::{InputShortcut, Shortcut};
use crate::tui::{Constraint, Section, Tab, buffer, commandline};

#[derive(Debug)]
pub(crate) struct Character {
    sesh: python::Session,
    tab_title: Option<String>,
    layout: Py<Section>,
}

impl Character {
    pub(crate) fn new(sesh: python::Session) -> Self {
        Self {
            sesh,
            tab_title: None,
            layout: initial_layout(),
        }
    }
}

#[async_trait]
impl Tab for Character {
    fn title(&self, app: &AppData) -> String {
        if let Some(title) = &self.tab_title {
            title.clone()
        } else {
            let Ok(sesh) = app.session(self.sesh.id) else {
                return "Unknown".to_string();
            };
            sesh.character.name.clone()
        }
    }

    // TODO(XXX): Styling
    fn rendered_title(&self, app: &AppData) -> Line<'_> {
        let Ok(sesh) = app.session(self.sesh.id) else {
            return Line::from(self.title(app));
        };

        if app.active_session == Some(self.sesh.id) {
            return Line::from(self.title(app));
        }

        let new_data = sesh.output.new_data();
        let unread = if new_data > 0 {
            format!(" ({new_data})")
        } else {
            String::new()
        };

        format!("{}{}", self.title(app), unread).into()
    }

    fn set_title(&mut self, _: &AppData, title: &str) -> Result<(), Error> {
        self.tab_title = Some(title.to_string());
        Ok(())
    }

    fn render(
        &mut self,
        app: &mut AppData,
        f: &mut Frame<'_>,
        tab_content: Rect,
    ) -> Result<(), Error> {
        let session = app.session_mut(self.sesh.id)?;

        let sections = Python::with_gil(|py| {
            let layout: PyRef<'_, Section> = self.layout.extract(py)?;
            layout.partition_by_name(py, tab_content)
        })?;

        // TODO(XXX): spammy, remove after debugging:
        /*trace!("\n\n");
        for (name, rect) in &sections {
            trace!("{name} -> {rect}");
        }*/

        // Safety: we unconditionally create this section in layout init.
        let output_section = sections.get(OUTPUT_BUFFER_NAME).unwrap();

        let output_dimensions = (output_section.width, output_section.height);
        if session.output.dimensions != output_dimensions {
            session.event_handlers.session_event(
                session.id,
                &Event::BufferResized {
                    name: OUTPUT_BUFFER_NAME.to_string(),
                    from: session.output.dimensions.into(),
                    to: output_dimensions.into(),
                },
            )?;
            session.output.dimensions = output_dimensions;
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
            prompt.as_ref(),
            // TODO(XXX): filtering settings.
            |item| !matches!(item, OutputItem::Prompt { .. }),
            *output_section,
        )?;

        commandline::draw(f, &session.input, &sections)?;

        for (name, buffer) in &session.extra_buffers {
            let Some(output_section) = sections.get(name) else {
                // TODO(XXX): fuse some kind of warning/error
                continue;
            };
            Python::with_gil(|py| {
                let mut buffer = buffer.borrow_mut(py);
                // TODO(XXX): filtering settings.
                buffer::draw(f, &mut buffer, None, |_| true, *output_section)
            })?;
        }

        Ok(())
    }

    fn session_id(&self) -> Option<u32> {
        Some(self.sesh.id)
    }

    fn layout(&self) -> Py<Section> {
        Python::with_gil(|py| self.layout.clone_ref(py))
    }

    fn all_shortcuts(&self, app: &AppData) -> Result<HashMap<KeyEvent, Shortcut>, Error> {
        Ok(app.session(self.sesh.id)?.shortcuts.clone())
    }

    fn lookup_shortcut(
        &self,
        app: &AppData,
        key_event: &KeyEvent,
    ) -> Result<Option<Shortcut>, Error> {
        Ok(app.session(self.sesh.id)?.shortcuts.get(key_event).cloned())
    }

    async fn shortcut(
        &mut self,
        app: &mut AppData,
        shortcut: &Shortcut,
    ) -> Result<Option<TabAction>, Error> {
        match shortcut {
            Shortcut::SessionInput(InputShortcut::Send) => {}
            Shortcut::SessionInput(shortcut) => {
                return Python::with_gil(|py| {
                    let mut input = app.session_mut(self.sesh.id)?.input.borrow_mut(py);
                    input.shortcut(shortcut);
                    Ok(None)
                });
            }
            _ => return Ok(None),
        }

        // Pop whatever input has been queued.
        let input = Python::with_gil(|py| {
            let session = app.active_session_mut().unwrap();
            session.input.borrow_mut(py).pop().unwrap_or_default()
        });

        // If the input line has the command prefix, dispatch the input line as a command.
        // TODO(XXX): configurable prefix.
        if let Some(line) = input.sent.strip_prefix('/') {
            return dispatch_command(app, line).await;
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

    async fn key_event(
        &mut self,
        app: &mut AppData,
        key_event: &KeyEvent,
    ) -> Result<Option<TabAction>, Error> {
        Python::with_gil(|py| {
            let mut input = app.session_mut(self.sesh.id)?.input.borrow_mut(py);
            input.key_event(key_event);
            Ok(None)
        })
    }
}

async fn dispatch_command(app: &mut AppData, input: &str) -> Result<Option<TabAction>, Error> {
    let mut parts = input.splitn(2, ' ');
    let cmd_name = parts.next().unwrap_or_default();
    let remaining = parts.next().unwrap_or_default();

    let Some(cmd) = app.slash_commands.get(cmd_name).cloned() else {
        let message = format!("unknown slash command: {cmd_name}");
        let session = app.active_session_mut().unwrap();
        warn!(message);
        session.output.add(OutputItem::CommandResult {
            error: true,
            message,
        });
        return Ok(None);
    };

    debug!("executing slash command: {cmd_name} {remaining}");
    Ok(match cmd.execute(app, remaining.to_string()).await {
        Ok(Some(tab_action)) => Some(tab_action),
        Ok(None) => None,
        Err(e) => {
            let message = format!("error executing slash command {cmd_name}: {e}");
            let session = app.active_session_mut().unwrap();
            error!(message);
            session.output.add(OutputItem::CommandResult {
                error: true,
                message,
            });
            None
        }
    })
}

fn initial_layout() -> Py<Section> {
    Python::with_gil(|py| {
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

const ROOT_SECTION_NAME: &str = "root";
const SESSION_SECTION_NAME: &str = "session";
