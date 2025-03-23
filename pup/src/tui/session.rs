use ratatui::layout::Rect;
use ratatui::prelude::{Line, Text};
use ratatui::Frame;

use crate::app::AppData;
use crate::error::Error;
use crate::python;
use crate::tui::Tab;

#[derive(Debug)]
pub(crate) struct Session {
    sesh: python::Session,
}

impl Session {
    pub(crate) fn new(sesh: python::Session) -> Self {
        Self { sesh }
    }
}

impl Tab for Session {
    fn title(&self) -> Line {
        // TODO(XXX): Styling, unread count, etc...
        self.sesh.mud.name.clone().into()
    }

    fn session_id(&self) -> Option<u32> {
        Some(self.sesh.id)
    }

    fn render(
        &mut self,
        _app: &mut AppData,
        f: &mut Frame<'_>,
        tab_content: Rect,
    ) -> Result<(), Error> {
        f.render_widget::<Text>(format!("MUD: {}", self.sesh.mud).into(), tab_content);
        Ok(())
    }
}
