use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use crate::error::Error;
use crate::idmap::Identifiable;
use crate::Result;
use pyo3::types::PyAnyMethods;
use pyo3::{pyclass, pymethods, Py, Python};
use ratatui::layout::Rect;
use ratatui::prelude::Color;
use ratatui::style::Style;
use ratatui::Frame;
use tracing::warn;

#[derive(Debug, Clone)]
#[pyclass]
pub struct Annotation {
    #[pyo3(get)]
    pub id: u32,

    #[pyo3(get, set)]
    pub layout_name: String,

    #[pyo3(get, set)]
    pub enabled: bool,

    #[pyo3(get, set)]
    pub row: u16,

    #[pyo3(get, set)]
    pub column: u16,

    #[pyo3(get, set)]
    pub text: String,

    pub color: Color,
}

#[pymethods]
impl Annotation {
    fn set_colour(&mut self, r: u8, g: u8, b: u8) {
        self.color = Color::Rgb(r, g, b);
    }

    fn set_color(&mut self, r: u8, g: u8, b: u8) {
        self.set_colour(r, g, b);
    }
}

impl Identifiable for Py<Annotation> {
    fn id(&self) -> u32 {
        Python::with_gil(|py| self.bind(py).extract::<Annotation>().unwrap().id)
    }
}

impl Display for Annotation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Annotation({}): {}", self.id, self.text))
    }
}

pub fn draw_annotation(
    annotation: &Py<Annotation>,
    f: &mut Frame<'_>,
    sections: &HashMap<String, Rect>,
) -> Result<()> {
    Python::with_gil(|py| {
        let annotation = annotation.borrow_mut(py);
        if !annotation.enabled {
            return Ok(());
        }

        let area = sections
            .get(&annotation.layout_name)
            .ok_or_else(|| Error::LayoutMissing(annotation.layout_name.clone()))?;

        let row = annotation.row.saturating_sub(1); // Always one row up
        let Ok(text_width) = u16::try_from(annotation.text.len()) else {
            warn!(
                "annotation {} text too long for u16: {}",
                annotation.id,
                annotation.text.len()
            );
            return Ok(());
        };

        let start_col = annotation.column.saturating_sub(text_width / 2);
        f.buffer_mut().set_string(
            area.x + start_col,
            area.y + row,
            &annotation.text,
            Style::default().fg(annotation.color),
        );

        Ok(())
    })
}
