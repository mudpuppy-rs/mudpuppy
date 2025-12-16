use std::collections::HashMap;

use pyo3::{Py, Python};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::app::AppData;
use crate::error::Error;
use crate::session::Buffer;
use crate::tui::chrome::{TabData, TabKind};
use crate::tui::{Constraint, Section, Tab, buffer};

#[derive(Debug)]
pub(crate) struct CustomTab {
    pub(crate) buffers: HashMap<String, Py<Buffer>>,
}

impl CustomTab {
    pub(crate) fn new_tab(
        py: Python<'_>,
        title: String,
        layout: Option<Py<Section>>,
        buffers: Vec<Py<Buffer>>,
    ) -> Result<Tab, Error> {
        let layout = if let Some(layout) = layout {
            layout
        } else {
            let mut root = Section::new(py, format!("{title}_TabRoot"));
            root.append_child(py, Constraint::Min(1), Section::new(py, title.clone()))?;
            Py::new(py, root)?
        };
        let buffers = buffers
            .into_iter()
            .map(|buff| {
                let name = buff.borrow(py).name.clone();
                (name, buff)
            })
            .collect();
        Ok(Tab {
            data: TabData::new(title, layout, None),
            kind: TabKind::Custom(Box::new(Self { buffers })),
        })
    }

    pub(crate) fn render(
        &mut self,
        _: &mut AppData,
        f: &mut Frame<'_>,
        sections: &HashMap<String, Rect>,
    ) -> Result<(), Error> {
        // Render each buffer to its corresponding section
        for (name, buffer) in &mut self.buffers {
            let Some(area) = sections.get(name) else {
                // TODO(XXX): fuse some kind of warning/error
                continue;
            };
            Python::attach(|py| {
                let mut buffer = buffer.borrow_mut(py);
                let buffer_config = buffer
                    .config
                    .as_ref()
                    .map(|buffer_config| buffer_config.borrow(py).clone())
                    .unwrap_or_default();
                // TODO(XXX): filtering settings.
                buffer::draw(f, &mut buffer, None, &buffer_config, None, |_| true, *area)
            })?;
        }
        Ok(())
    }
}
