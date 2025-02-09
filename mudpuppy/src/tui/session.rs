use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;

use async_trait::async_trait;
use crossterm::event::MouseEvent;
use futures::stream::FuturesUnordered;
use pyo3::{Py, PyErr, PyRef, Python};
use ratatui::crossterm::event::{
    Event as TermEvent, MouseButton, MouseEventKind as TermMouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::Frame;
use tracing::{debug, trace, warn};

use crate::app::{State, Tab, TabAction, TabKind};
use crate::client::output;
use crate::config::{edit_global, edit_mud, GlobalConfig};
use crate::error::Error;
use crate::model::{InputMode, SessionInfo, Shortcut};
use crate::tui::annotation::draw_annotation;
use crate::tui::button::draw_button;
use crate::tui::gauge::draw_gauge;
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
    button_areas: HashMap<u32, Rect>,
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
            button_areas: HashMap::new(),
        })
    }

    fn handle_button_click(
        &mut self,
        state: &mut State,
        futures: &mut FuturesUnordered<python::PyFuture>,
        mouse_event: MouseEvent,
    ) -> Result<ControlFlow<()>> {
        // For now, we only react to left mouse button press events for buttons.
        if mouse_event.kind != TermMouseEventKind::Down(MouseButton::Left) {
            return Ok(ControlFlow::Continue(()));
        }

        let row = mouse_event.row;
        let column = mouse_event.column;
        for (id, area) in &self.button_areas {
            let area_contains_click = area.left() <= column
                && column < area.right()
                && area.top() <= row
                && row < area.bottom();
            if !area_contains_click {
                continue;
            }

            debug!("button ID {id} was clicked.");

            let Some(client) = state.client_for_id_mut(self.session.id) else {
                warn!("missing client for session tab: {}", self.session);
                return Ok(ControlFlow::Continue(()));
            };

            Python::with_gil(|py| {
                let mut btn = client.buttons.get(*id).unwrap().borrow_mut(py);
                btn.toggle_press = true;

                trace!("preparing callback future for button {id}");
                futures.push(Box::pin(pyo3_async_runtimes::tokio::into_future(
                    btn.callback
                        .call1(py, (self.session.id, btn.id))?
                        .into_bound(py),
                )?));

                Ok::<_, Error>(())
            })?;

            return Ok(ControlFlow::Break(()));
        }

        Ok(ControlFlow::Continue(()))
    }
}

#[async_trait]
impl Tab for Widget {
    fn kind(&self) -> TabKind {
        TabKind::Session {
            session: self.session.clone(),
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::MudSession
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
            Shortcut::ToggleMouseMode => {
                let mouse_enabled = !self.config.lookup(|c| c.mouse_enabled, false);
                edit_global("mouse_enabled", mouse_enabled)?;
                client.output.push(output::Item::CommandResult {
                    error: false,
                    message: format!(
                        "mouse support {}",
                        if mouse_enabled { "enabled" } else { "disabled" }
                    ),
                });
            }
            _ => {}
        }

        self.scroll_window.handle_shortcut(shortcut);

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
        match event {
            TermEvent::Key(key_event) => {
                let Some(client) = state.client_for_id_mut(self.session.id) else {
                    warn!("missing client for session tab: {}", self.session);
                    return Ok(None);
                };
                client.key_event(futures, key_event)?;
            }
            TermEvent::Mouse(mouse_event) => {
                // Check left click events against button areas, pushing a callback future
                // if a button was hit by the click.
                if self.handle_button_click(state, futures, *mouse_event)? == ControlFlow::Break(())
                {
                    return Ok(None); // buton was clicked, don't send mouse event.
                }

                // Non-button click mouse events are passed onward.
                let Some(client) = state.client_for_id_mut(self.session.id) else {
                    warn!("missing client for session tab: {}", self.session);
                    return Ok(None);
                };
                client.mouse_event(futures, mouse_event)?;
            }
            _ => {}
        }

        Ok(None)
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
        Python::with_gil(|py| Input::draw(&mut client.input.borrow_mut(py), frame, &sections))?;

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

        for (_, gauge) in &client.gauges {
            draw_gauge(gauge, frame, &sections)?;
        }

        for (i, button) in &client.buttons {
            // Keep track of where each button drew itself for event handling.
            let area = draw_button(button, frame, &sections)?;
            self.button_areas.insert(*i, area);
        }

        for (_, annotation) in &client.annotations {
            draw_annotation(annotation, frame, &sections)?;
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
