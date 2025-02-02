use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};

use pyo3::{pyclass, pymethods, Py, PyRef, Python};
use ratatui::layout::Rect;
use ratatui::widgets::Clear;
use ratatui::Frame;

use crate::client::output::Output;
use crate::error::Error;
use crate::idmap;
use crate::tui::buffer::{self, BufferConfig, DrawScrollbar};

/// An "Extra Buffer" is a user created buffer that can be used to display
/// arbitrary content.
///
/// It's similar to `MudBuffer` and `ScrollWindow`, but without being tailored
/// to one of those two specific tasks.
#[derive(Debug, Clone)]
#[pyclass]
pub struct ExtraBuffer {
    #[pyo3(get)]
    pub id: u32,

    #[pyo3(get)]
    pub config: Py<BufferConfig>,
}

impl ExtraBuffer {
    /// # Errors
    /// TODO(XXX): docs.
    pub fn draw_buffer(
        &mut self,
        f: &mut Frame<'_>,
        sections: &HashMap<String, Rect>,
    ) -> crate::Result<()> {
        Python::with_gil(|py| {
            let mut config: BufferConfig = self.config.extract(py)?;
            let buffer_area = sections
                .get(&config.layout_name)
                .ok_or(Error::LayoutMissing(config.layout_name.clone()))?;

            let mut output: Output = config.output.extract(py)?;

            // Make sure to clear the viewport first - we might be drawing on top of already
            // rendered content.
            f.render_widget(Clear, *buffer_area);

            buffer::draw(
                f,
                &mut config,
                output.read_received().iter(),
                |_| true,
                buffer_area,
                // TODO(XXX): config for scroll bar render...
                DrawScrollbar::Always,
            )
        })
    }
}

#[pymethods]
impl ExtraBuffer {
    fn __str__(&self, py: Python<'_>) -> String {
        let config: PyRef<'_, BufferConfig> = self.config.extract(py).unwrap();
        format!("Buffer({}) config: {}", self.id, *config)
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for ExtraBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Buffer({})", self.id)
    }
}

impl idmap::Identifiable for ExtraBuffer {
    fn id(&self) -> u32 {
        self.id
    }
}
