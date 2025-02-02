use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use pyo3::types::PyAnyMethods;
use pyo3::{pyclass, pymethods, Py, Python};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge as RatatuiGauge};
use ratatui::Frame;

use crate::error::Error;
use crate::idmap::Identifiable;
use crate::Result;

#[derive(Debug, Clone)]
#[pyclass]
pub struct Gauge {
    #[pyo3(get)]
    pub id: u32,

    #[pyo3(get, set)]
    pub layout_name: String,

    #[pyo3(get, set)]
    pub value: f64,

    #[pyo3(get, set)]
    pub max: f64,

    #[pyo3(get, set)]
    pub title: String,

    pub color: Color,
}

#[pymethods]
impl Gauge {
    fn set_colour(&mut self, r: u8, g: u8, b: u8) {
        self.color = Color::Rgb(r, g, b);
    }

    fn set_color(&mut self, r: u8, g: u8, b: u8) {
        self.set_colour(r, g, b);
    }
}

impl Identifiable for Py<Gauge> {
    fn id(&self) -> u32 {
        Python::with_gil(|py| {
            let gauge: Gauge = self.bind(py).extract().unwrap();
            gauge.id
        })
    }
}

impl Display for Gauge {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Gauge({:?} ({}) {}/{})",
            self.title, self.id, self.value, self.max
        ))
    }
}

pub fn draw_gauge(
    gauge: &Py<Gauge>,
    f: &mut Frame<'_>,
    sections: &HashMap<String, Rect>,
) -> Result<()> {
    Python::with_gil(|py| {
        let gauge: Gauge = gauge.extract(py)?;

        let gauge_area = sections
            .get(&gauge.layout_name)
            .ok_or(Error::LayoutMissing(gauge.layout_name.clone()))?;

        // TODO(XXX): style customization.
        let gauge_block = Block::new()
            .borders(Borders::ALL)
            .title(Line::from(gauge.title.as_str()).alignment(Alignment::Center))
            .fg(Color::White);

        let label = Span::styled(
            format!("{:.1}%", (gauge.value / gauge.max) * 100.0),
            Style::new().italic().bold().fg(Color::Yellow),
        );

        let gauge_widget = RatatuiGauge::default()
            .block(gauge_block)
            .label(label)
            .gauge_style(gauge.color)
            .ratio(safe_ratio(gauge.value, gauge.max));
        f.render_widget(gauge_widget, *gauge_area);

        Ok(())
    })
}

fn safe_ratio(value: f64, max: f64) -> f64 {
    if value.is_nan() || max.is_nan() || max == 0.0 {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    }
}
