use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};

use pyo3::types::{PyAnyMethods, PyDict, PyList, PyListMethods, PyTuple};
use pyo3::{pyclass, pymethods, Bound, Py, PyRef, Python};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Borders, Clear};
use ratatui::Frame;
use tracing::trace;

use crate::client::output::Output;
use crate::error::Error;
use crate::tui::buffer;
use crate::tui::buffer::DrawScrollbar;
use crate::{idmap, Result};

// TODO(XXX): Rename?
#[derive(Debug, Clone)]
#[pyclass]
#[allow(clippy::module_name_repetitions)]
pub struct LayoutNode {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub direction: PyDirection,
    #[pyo3(get, set)]
    pub margin: u16,
    #[pyo3(get, set)]
    pub sections: Py<PyList>,
}

impl LayoutNode {
    /// # Errors
    /// If the layout contains invalid types, or duplicate section names.
    pub fn all_sections_rects(
        &self,
        py: Python<'_>,
        parent: Rect,
    ) -> Result<HashMap<String, Rect>> {
        let mut result = HashMap::new();
        self.collect_named_section_rects(py, parent, &mut result)?;
        Ok(result)
    }

    fn section_rects(&self, py: Python<'_>, parent: Rect) -> Result<Vec<Rect>> {
        let sections = self.sections.bind(py);

        let mut constraints: Vec<Constraint> = vec![];
        for section in sections {
            let tuple: &Bound<'_, PyTuple> = section.downcast().map_err(|_| Error::BadLayout)?;
            let py_constraint: PyConstraint = tuple.get_item(0)?.extract()?;
            constraints.push(py_constraint.to_constraint());
        }

        Ok(Layout::default()
            .direction(self.direction.into())
            .margin(self.margin)
            .constraints(constraints)
            .split(parent)
            .to_vec())
    }

    fn collect_named_section_rects(
        &self,
        py: Python<'_>,
        parent: Rect,
        result: &mut HashMap<String, Rect>,
    ) -> Result<()> {
        let rects = self.section_rects(py, parent)?;

        let sections = self.sections.bind(py);

        for (idx, rect) in rects.into_iter().enumerate() {
            let section_tuple = sections.get_item(idx).unwrap();
            let section_tuple: &Bound<'_, PyTuple> =
                section_tuple.downcast().map_err(|_| Error::BadLayout)?;

            let layout: LayoutNode = section_tuple.get_item(1)?.extract()?;

            if result.contains_key(&layout.name) {
                return Err(Error::DuplicateLayout(layout.name.clone()));
            }
            result.insert(layout.name.clone(), rect);
            layout.collect_named_section_rects(py, rect, result)?;
        }

        Ok(())
    }

    fn collect_all_layouts(&self, py: Python<'_>, result: &Bound<PyDict>) -> Result<()> {
        let sections = self.sections.bind(py);
        for section in sections {
            let tuple: Bound<'_, PyTuple> =
                section.downcast_into().map_err(|_| Error::BadLayout)?;
            let layout: LayoutNode = tuple.get_item(1)?.extract()?;

            if result.get_item(&layout.name).is_ok() {
                return Err(Error::DuplicateLayout(layout.name.clone()));
            }
            result.set_item(layout.name.clone(), tuple)?;

            layout.collect_all_layouts(py, result)?;
        }

        Ok(())
    }
}

#[pymethods]
impl LayoutNode {
    #[must_use]
    #[new]
    pub fn new(py: Python<'_>, name: &str) -> Self {
        Self {
            name: name.to_string(),
            direction: PyDirection::default(),
            margin: 0,
            sections: Py::from(PyList::empty(py)),
        }
    }

    /// # Errors
    /// If the Python environment can't append a list item.
    pub fn add_section(
        &mut self,
        py: Python<'_>,
        node: LayoutNode,
        constraint: PyConstraint,
    ) -> Result<()> {
        let sections_list = self.sections.bind(py);
        sections_list.append((constraint, node)).map_err(Into::into)
    }

    /// # Errors
    /// If the section can't be found.
    pub fn find_section<'py>(&self, py: Python<'py>, name: &str) -> Result<Bound<'py, PyTuple>> {
        let sections = self.sections.bind(py);

        for section in sections {
            let tuple: Bound<'_, PyTuple> =
                section.downcast_into().map_err(|_| Error::BadLayout)?;
            let layout: LayoutNode = tuple.get_item(1)?.extract()?;

            if layout.name == name {
                return Ok(tuple);
            } else if let Ok(found) = layout.find_section(py, name) {
                return Ok(found);
            }
        }

        Err(Error::LayoutMissing(name.to_string()))
    }

    /// # Errors
    /// If the layout contains invalid types, or duplicate section names.
    pub fn all_layouts<'py>(&self, py: Python<'py>) -> Result<Bound<'py, PyDict>, Error> {
        let result = PyDict::new(py);
        self.collect_all_layouts(py, &result)?;
        Ok(result)
    }

    fn __str__(&self, py: Python<'_>) -> String {
        let sections = self.sections.bind(py);
        format!("Layout({:?}, {} sections)", self.name, sections.len())
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
#[pyclass(name = "Direction", eq, eq_int)]
pub enum PyDirection {
    Horizontal,
    #[default]
    Vertical,
}

impl From<PyDirection> for Direction {
    fn from(dir: PyDirection) -> Self {
        match dir {
            PyDirection::Horizontal => Direction::Horizontal,
            PyDirection::Vertical => Direction::Vertical,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
#[pyclass(name = "Constraint")]
pub struct PyConstraint {
    #[pyo3(get)]
    pub percentage: Option<u16>,
    #[pyo3(get)]
    pub ratio: Option<(u32, u32)>,
    #[pyo3(get)]
    pub length: Option<u16>,
    #[pyo3(get)]
    pub max: Option<u16>,
    #[pyo3(get)]
    pub min: Option<u16>,
}

impl PyConstraint {
    fn to_constraint(self) -> Constraint {
        if let Some(percentage) = self.percentage {
            Constraint::Percentage(percentage)
        } else if let Some((a, b)) = self.ratio {
            Constraint::Ratio(a, b)
        } else if let Some(length) = self.length {
            Constraint::Length(length)
        } else if let Some(max) = self.max {
            Constraint::Max(max)
        } else if let Some(min) = self.min {
            Constraint::Min(min)
        } else {
            Constraint::Min(0)
        }
    }
}

#[pymethods]
impl PyConstraint {
    #[staticmethod]
    #[must_use]
    pub fn with_percentage(percentage: u16) -> Self {
        Self {
            percentage: Some(percentage),
            ratio: None,
            length: None,
            max: None,
            min: None,
        }
    }

    pub fn set_percentage(&mut self, percentage: u16) {
        self.percentage = Some(percentage);
        self.ratio = None;
        self.length = None;
        self.max = None;
        self.min = None;
    }

    #[staticmethod]
    #[must_use]
    pub fn with_ratio(ratio: (u32, u32)) -> Self {
        Self {
            percentage: None,
            ratio: Some(ratio),
            length: None,
            max: None,
            min: None,
        }
    }

    pub fn set_ratio(&mut self, ratio: (u32, u32)) {
        self.percentage = None;
        self.ratio = Some(ratio);
        self.length = None;
        self.max = None;
        self.min = None;
    }

    #[staticmethod]
    #[must_use]
    pub fn with_length(length: u16) -> Self {
        Self {
            percentage: None,
            ratio: None,
            length: Some(length),
            max: None,
            min: None,
        }
    }

    pub fn set_length(&mut self, length: u16) {
        self.percentage = None;
        self.ratio = None;
        self.length = Some(length);
        self.max = None;
        self.min = None;
    }

    #[staticmethod]
    #[must_use]
    pub fn with_max(max: u16) -> Self {
        Self {
            percentage: None,
            ratio: None,
            length: None,
            max: Some(max),
            min: None,
        }
    }

    pub fn set_max(&mut self, max: u16) {
        self.percentage = None;
        self.ratio = None;
        self.length = None;
        self.max = Some(max);
        self.min = None;
    }

    #[staticmethod]
    #[must_use]
    pub fn with_min(min: u16) -> Self {
        Self {
            percentage: None,
            ratio: None,
            length: None,
            max: None,
            min: Some(min),
        }
    }

    pub fn set_min(&mut self, min: u16) {
        self.percentage = None;
        self.ratio = None;
        self.length = None;
        self.max = None;
        self.min = Some(min);
    }

    pub fn set_from(&mut self, other: PyConstraint) {
        self.percentage = other.percentage;
        self.ratio = other.ratio;
        self.length = other.length;
        self.max = other.max;
        self.min = other.min;
    }

    fn __str__(&self) -> String {
        if let Some(max) = self.max {
            format!("Max({max})")
        } else if let Some(min) = self.min {
            format!("Min({min})")
        } else if let Some(length) = self.length {
            format!("Length({length})")
        } else if let Some((a, b)) = self.ratio {
            format!("Ratio({a},{b})")
        } else if let Some(percentage) = self.percentage {
            format!("Percentage({percentage})")
        } else {
            "Unknown".to_string()
        }
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

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
    ) -> Result<()> {
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

#[derive(Debug, Clone)]
#[pyclass]
#[allow(clippy::struct_excessive_bools)] // TODO(XXX): Consider.
pub struct BufferConfig {
    #[pyo3(get, set)]
    pub layout_name: String,

    #[pyo3(get, set)]
    pub line_wrap: bool,

    #[pyo3(get, set)]
    pub border_top: bool,

    #[pyo3(get, set)]
    pub border_bottom: bool,

    #[pyo3(get, set)]
    pub border_left: bool,

    #[pyo3(get, set)]
    pub border_right: bool,

    #[pyo3(get, set)]
    pub direction: BufferDirection,

    #[pyo3(get)]
    pub output: Py<Output>,

    #[pyo3(get)]
    pub scroll_pos: usize,

    #[pyo3(get)]
    pub max_scroll: usize,
}

impl BufferConfig {
    #[must_use]
    pub fn area_inside_borders(&self, mut area: Rect, scrollbar: bool) -> Rect {
        if self.border_top {
            area.height = area.height.saturating_sub(1);
            area.y = area.y.saturating_add(1);
        }
        if self.border_bottom {
            area.height = area.height.saturating_sub(1);
        }
        if self.border_left {
            area.width = area.width.saturating_sub(1);
            area.x = area.x.saturating_add(1);
        }
        if self.border_right {
            area.width = area.width.saturating_sub(1);
        }
        if scrollbar {
            area.width = area.width.saturating_sub(1);
        }
        area
    }

    #[must_use]
    pub fn area_inside_top_borders(&self, mut area: Rect) -> Rect {
        if self.border_top {
            area.height = area.height.saturating_sub(1);
            area.y = area.y.saturating_add(1);
        }
        if self.border_bottom {
            area.height = area.height.saturating_sub(1);
        }
        area
    }

    #[must_use]
    pub fn borders(&self) -> Borders {
        let mut borders = Borders::empty();
        if self.border_top {
            borders |= Borders::TOP;
        }
        if self.border_bottom {
            borders |= Borders::BOTTOM;
        }
        if self.border_left {
            borders |= Borders::LEFT;
        }
        if self.border_right {
            borders |= Borders::RIGHT;
        }
        borders
    }
}

#[pymethods]
impl BufferConfig {
    /// # Errors
    /// If the layout name is empty
    #[new]
    pub fn new(layout_name: String) -> Result<Self> {
        if layout_name.is_empty() {
            return Err(Error::BadLayout);
        }
        let output = Python::with_gil(|py| Py::new(py, Output::new()))?;
        Ok(Self {
            layout_name,
            line_wrap: false,
            output,
            border_top: false,
            border_bottom: false,
            border_left: false,
            border_right: false,
            direction: BufferDirection::default(),
            scroll_pos: 0,
            max_scroll: 0,
        })
    }

    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll_pos
    }

    pub fn scroll_up(&mut self, lines: u16) {
        trace!("scrolling up: scroll-pos: {}", self.scroll_pos);
        self.scroll_pos = self
            .scroll_pos
            .checked_add(lines as usize)
            .unwrap_or(self.scroll_pos);
        trace!("scrolling up: scroll-pos now {}", self.scroll_pos);
    }

    pub fn scroll_down(&mut self, lines: u16) {
        trace!("scrolling down: scroll-pos: {}", self.scroll_pos);
        self.scroll_pos = self.scroll_pos.saturating_sub(lines as usize);
        trace!("scrolling down: scroll-pos now {}", self.scroll_pos);
    }

    pub fn scroll_bottom(&mut self) {
        trace!("scrolling to bottom: scroll-pos: {}", self.scroll_pos);
        self.scroll_pos = 1;
        trace!("scrolling to bottom: scroll-pos now {}", self.scroll_pos);
    }

    pub fn scroll_to(&mut self, scroll: usize) {
        trace!(
            "scrolling to pos: scroll-pos {}: {}",
            scroll,
            self.scroll_pos
        );
        self.scroll_pos = scroll;
        trace!(
            "scrolling to pos: scroll-pos {} now: {}",
            scroll,
            self.scroll_pos
        );
    }

    pub fn scroll_max(&mut self) {
        trace!("scrolling to max: scroll-pos: {}", self.max_scroll);
        self.scroll_pos = self.max_scroll;
        trace!("scrolling to max: scroll-pos now: {}", self.scroll_pos);
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        // TODO(XXX): nicer str format
        format!("{self:?}")
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for BufferConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "BufferConfig({})", self.layout_name)
    }
}

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
#[pyclass(eq, eq_int)]
pub enum BufferDirection {
    TopToBottom,
    #[default]
    BottomToTop,
}

#[pymethods]
impl BufferDirection {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        format!("{self}")
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for BufferDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BufferDirection::TopToBottom => write!(f, "top to bottom"),
            BufferDirection::BottomToTop => write!(f, "bottom to top"),
        }
    }
}
