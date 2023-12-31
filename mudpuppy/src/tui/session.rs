use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use pyo3::{Py, PyErr, PyRef, Python};
use ratatui::crossterm::event::Event as TermEvent;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::Frame;
use tracing::{debug, warn};

use crate::app::{State, Tab, TabAction, TabKind};
use crate::client::output;
use crate::config::{edit_mud, GlobalConfig};
use crate::error::Error;
use crate::model::{SessionInfo, Shortcut};
use crate::tui::input::{self, Input};
use crate::tui::layout::{LayoutNode, PyConstraint};
use crate::tui::mudbuffer::{self, MudBuffer};
use crate::tui::splitview::ScrollWindow;
use crate::{python, Result};

#[derive(Debug)]
pub struct Widget {
    config: GlobalConfig,
    session: Arc<SessionInfo>,

    mud_buffer: MudBuffer,
    scroll_window: ScrollWindow,
}

impl Widget {
    pub fn new(config: GlobalConfig, session: Arc<SessionInfo>) -> Result<Self> {
        let mud = config.must_lookup_mud(&session.mud_name)?;
        let mud_buffer = MudBuffer::new(mud.clone(), session.id)?;
        let scroll_window = ScrollWindow::new(mud.clone())?;
        Ok(Self {
            config,
            session,
            mud_buffer,
            scroll_window,
        })
    }
}

#[async_trait]
impl Tab for Widget {
    fn kind(&self) -> TabKind {
        TabKind::Session {
            session: self.session.clone(),
        }
    }

    // TODO(XXX): Text styling.
    fn title(&self) -> Line {
        self.session.mud_name.clone().into()
    }

    fn reload_config(&mut self) -> Result<(), Error> {
        let mud = self.config.must_lookup_mud(&self.session.mud_name)?;
        self.mud_buffer.reload_config(mud.clone());
        self.scroll_window.reload_config(mud);
        Ok(())
    }

    async fn shortcut(
        &mut self,
        state: &mut State,
        shortcut: Shortcut,
    ) -> Result<Option<TabAction>, Error> {
        let mud = self.config.must_lookup_mud(&self.session.mud_name)?;
        let client = state
            .client_for_id_mut(self.session.id)
            .ok_or(Error::UnknownSession(self.session.id))?;

        match shortcut {
            Shortcut::ToggleLineWrap => {
                let no_line_wrap = !mud.no_line_wrap;
                edit_mud(&mud.name, "no_line_wrap", no_line_wrap)?;
                client.output.push(output::Item::CommandResult {
                    error: false,
                    message: format!(
                        "line wrapping {}",
                        if no_line_wrap { "disabled" } else { "enabled" }
                    ),
                });
            }
            Shortcut::ToggleInputEcho => {
                let echo_input = !mud.echo_input;
                edit_mud(&mud.name, "echo_input", echo_input)?;
                client.output.push(output::Item::CommandResult {
                    error: false,
                    message: format!(
                        "input echo {}",
                        if echo_input { "enabled" } else { "disabled" }
                    ),
                });
            }
            _ => {}
        }

        self.scroll_window.handle_shortcut(&shortcut);

        state.event_tx.send(python::Event::Shortcut {
            id: self.session.id,
            shortcut,
        })?;
        Ok(None)
    }

    fn term_event(
        &mut self,
        state: &mut State,
        futures: &mut FuturesUnordered<python::PyFuture>,
        event: &TermEvent,
    ) -> Result<Option<TabAction>, Error> {
        let Some(client) = state.client_for_id_mut(self.session.id) else {
            warn!("missing client for session tab: {}", self.session);
            return Ok(None);
        };

        let TermEvent::Key(key_event) = event else {
            return Ok(None);
        };

        client.key_event(futures, key_event).map(|()| None)
    }

    fn draw(&mut self, state: &mut State, frame: &mut Frame<'_>, area: Rect) -> Result<()> {
        let event_tx = state.event_tx.clone();
        // Retrieve the client for the session.
        let Some(client) = state.client_for_id_mut(self.session.id) else {
            warn!("missing client for session tab: {}", self.session);
            return Ok(());
        };

        // Extract a table of section name -> layout area.
        let sections = Python::with_gil(|py| {
            let layout: PyRef<'_, LayoutNode> = client.layout.extract(py)?;
            Ok::<_, PyErr>(layout.all_sections_rects(py, area)?)
        })?;

        // If we're scrolling and there was new data received, move the scroll position
        // up by the amount of new data so that the scroll window remains at the same
        // point relative to where it was before the new data was received.
        //
        // We do this _before_ drawing the output buffer because the act of draining the
        // new data to draw will clear the new data count.
        if self.scroll_window.scroll_pos != 0 && client.output.new_data > 0 {
            self.scroll_window
                .scroll_up(u16::try_from(client.output.new_data).unwrap_or(u16::MAX));
        }

        // Draw the input area.
        Input::draw(&mut client.input, frame, &sections)?;

        // Draw the main output buffer.
        self.mud_buffer
            .draw_buffer(client, &event_tx, frame, &sections)?;

        // Draw the scroll window if applicable.
        if self.scroll_window.scroll_pos != 0 {
            self.scroll_window.draw_buffer(client, frame, &sections)?;
        }

        // Draw any extra buffers.
        for (_, buf) in &mut client.extra_buffers {
            buf.draw_buffer(frame, &sections)?;
        }

        Ok(())
    }
}

pub fn initial_layout() -> Py<LayoutNode> {
    Python::with_gil(|py| {
        debug!("configuring initial layout");
        let output = LayoutNode::new(py, mudbuffer::OUTPUT_SECTION_NAME);
        let input = LayoutNode::new(py, input::INPUT_SECTION_NAME);
        let mut session = LayoutNode::new(py, SESSION_SECTION_NAME);
        session.add_section(py, output, PyConstraint::with_percentage(95))?;
        session.add_section(py, input, PyConstraint::with_min(3))?;
        let mut root = LayoutNode::new(py, "");
        root.add_section(py, session, PyConstraint::with_percentage(100))?;
        Py::new(py, root)
    })
    .unwrap() // Safety: no chance for duplicate sections.
}

pub const SESSION_SECTION_NAME: &str = "session";
