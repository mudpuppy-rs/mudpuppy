use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use pyo3::types::PyAnyMethods;
use pyo3::{pyclass, Py, PyAny, Python};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use ratatui::Frame;
use tui_framework_experiment::button as tui_button;

use crate::error::Error;
use crate::idmap::Identifiable;
use crate::Result;

#[derive(Debug, Clone)]
#[pyclass]
pub struct Button {
    #[pyo3(get)]
    pub id: u32,

    #[pyo3(get, set)]
    pub layout_name: String,

    #[pyo3(get, set)]
    pub label: String,

    #[pyo3(get, set)]
    pub callback: Py<PyAny>, // Must be async. No return.

    pub(crate) toggle_press: bool,
}

impl Identifiable for Py<Button> {
    fn id(&self) -> u32 {
        Python::with_gil(|py| self.bind(py).extract::<Button>().unwrap().id)
    }
}

impl Display for Button {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Button({}): {}", self.id, self.label))
    }
}

pub fn draw_button(
    button: &Py<Button>,
    f: &mut Frame<'_>,
    sections: &HashMap<String, Rect>,
) -> Result<Rect> {
    Python::with_gil(|py| {
        let mut py_button = button.borrow_mut(py);
        let button_area = *sections
            .get(&py_button.layout_name)
            .ok_or(Error::LayoutMissing(py_button.layout_name.clone()))?;

        if button_area.is_empty() {
            return Ok(button_area);
        }

        // TODO(XXX): styling
        let mut button =
            tui_button::Button::new(py_button.label.clone()).with_theme(tui_button::themes::GREEN);

        let toggled = py_button.toggle_press;
        py_button.toggle_press = false;
        if toggled {
            button.toggle_press();
        }

        button.render(button_area, f.buffer_mut());
        Ok(button_area)
    })
}
