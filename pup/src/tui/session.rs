use crossterm::event::Event;
use pyo3::{Py, PyRef, Python};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::{Line, Text};
use tracing::debug;

use crate::app::AppData;
use crate::error::Error;
use crate::python;
use crate::tui::{Constraint, Section, Tab, TabAction, commandline, output_buffer};

#[derive(Debug)]
pub(crate) struct Character {
    sesh: python::Session,
    layout: Py<Section>,
}

impl Character {
    pub(crate) fn new(sesh: python::Session) -> Self {
        Self {
            sesh,
            layout: initial_layout(),
        }
    }
}

impl Tab for Character {
    fn title(&self) -> Line {
        // TODO(XXX): Styling, unread count, etc...
        self.sesh.character.name.clone().into()
    }

    fn session_id(&self) -> Option<u32> {
        Some(self.sesh.id)
    }

    fn render(
        &mut self,
        app: &mut AppData,
        f: &mut Frame<'_>,
        tab_content: Rect,
    ) -> Result<(), Error> {
        let session = app.session(self.sesh.id)?;

        let sections = Python::with_gil(|py| {
            let layout: PyRef<'_, Section> = self.layout.extract(py)?;
            layout.partition_by_name(py, tab_content)
        })?;

        commandline::draw(f, &session.input, &sections)?;

        f.render_widget::<Text>(
            format!("MUD: {}", self.sesh.character.mud).into(),
            *sections.get(output_buffer::SECTION_NAME).unwrap(),
        );
        Ok(())
    }

    fn crossterm_event(
        &mut self,
        app: &mut AppData,
        event: &Event,
    ) -> Result<Option<TabAction>, Error> {
        let session = app.session_mut(self.sesh.id)?;

        let Event::Key(key_event) = event else {
            return Ok(None);
        };

        Python::with_gil(|py| {
            if let &crossterm::event::KeyEvent {
                kind: crossterm::event::KeyEventKind::Press,
                code: crossterm::event::KeyCode::Enter,
                modifiers,
                ..
            } = key_event
            {
                if modifiers.is_empty() {
                    let input = {
                        let mut input = session.input.borrow_mut(py);
                        input.pop().unwrap_or_default()
                    };
                    session.send_line(input, false)?;
                }
                return Ok(None);
            }

            let mut input = session.input.borrow_mut(py);
            let key_event = (*key_event).try_into().map_err(Error::Internal)?;
            input.key_event(&key_event);
            Ok(None)
        })
    }
}

fn initial_layout() -> Py<Section> {
    Python::with_gil(|py| {
        debug!("configuring initial layout");
        let output = Section::new(py, output_buffer::SECTION_NAME.to_string());
        let commandline = Section::new(py, commandline::SECTION_NAME.to_string());
        let mut session = Section::new(py, SESSION_SECTION_NAME.to_string());
        session.add_child(py, Constraint::Percentage(95), output)?;
        session.add_child(py, Constraint::Min(3), commandline)?;
        let mut root = Section::new(py, ROOT_SECTION_NAME.to_string());
        root.add_child(py, Constraint::Percentage(100), session)?;
        Py::new(py, root)
    })
    .unwrap() // Safety: no chance for duplicate sections.
}

const ROOT_SECTION_NAME: &str = "root";
const SESSION_SECTION_NAME: &str = "session";
