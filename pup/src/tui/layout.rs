use std::collections::HashMap;

use pyo3::types::{PyAnyMethods, PyDict, PyList, PyListMethods, PyTuple};
use pyo3::{Bound, Py, PyResult, Python, pyclass, pymethods};
use ratatui::layout::{Layout, Rect};
use strum::Display;

use crate::error::Error;

// TODO(XXX): padding, borders, etc.
#[derive(Debug, Clone)]
#[pyclass]
pub struct Section {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub direction: Direction,
    #[pyo3(get, set)]
    pub margin: u16,
    #[pyo3(get, set)]
    pub children: Py<PyList>, // List[Tuple[Constraint, Section]]
}

impl Section {
    pub(crate) fn partition_by_name(
        &self,
        py: Python<'_>,
        parent: Rect,
    ) -> Result<HashMap<String, Rect>, Error> {
        let mut result = HashMap::new();
        self.partition_by_name_inner(py, parent, &mut result)?;
        Ok(result)
    }

    fn partition_by_name_inner(
        &self,
        py: Python<'_>,
        parent: Rect,
        result: &mut HashMap<String, Rect>,
    ) -> Result<(), Error> {
        let rects = self.partition(py, parent)?;

        let children = self.children.bind(py);
        for (idx, rect) in rects.into_iter().enumerate() {
            let child_tuple = children.get_item(idx)?;
            let child_tuple: &Bound<'_, PyTuple> = child_tuple.downcast()?;

            let child_section = child_tuple.get_item(1)?.extract::<Section>()?;

            if result.contains_key(&child_section.name) {
                return Err(Error::DuplicateLayoutSection(child_section.name.clone()));
            }
            result.insert(child_section.name.clone(), rect);
            child_section.partition_by_name_inner(py, rect, result)?;
        }

        Ok(())
    }

    fn partition(&self, py: Python<'_>, parent: Rect) -> Result<Vec<Rect>, Error> {
        let children = self.children.bind(py);

        let mut constraints: Vec<ratatui::layout::Constraint> = vec![];
        for child in children {
            let tuple: &Bound<'_, PyTuple> = child.downcast()?;
            let constraint = tuple.get_item(0)?.extract::<Constraint>()?;
            constraints.push(constraint.into());
        }

        Ok(Layout::default()
            .direction(self.direction.into())
            .margin(self.margin)
            .constraints(constraints)
            .split(parent)
            .to_vec())
    }

    fn all_children_inner(&self, py: Python<'_>, result: &Bound<PyDict>) -> Result<(), Error> {
        let children = self.children.bind(py);
        for child in children {
            let child_tuple: &Bound<'_, PyTuple> = child.downcast()?;
            let child_section = child_tuple.get_item(1)?.extract::<Section>()?;

            if result.get_item(&child_section.name).is_ok() {
                return Err(Error::DuplicateLayoutSection(child_section.name.clone()));
            }
            result.set_item(child_section.name.clone(), child_tuple)?;
            child_section.all_children_inner(py, result)?;
        }

        Ok(())
    }
}

#[pymethods]
impl Section {
    #[must_use]
    #[new]
    pub(crate) fn new(py: Python<'_>, name: String) -> Self {
        Self {
            name,
            direction: Direction::default(),
            margin: 0,
            children: Py::from(PyList::empty(py)),
        }
    }

    pub(crate) fn add_child(
        &mut self,
        py: Python<'_>,
        constraint: Constraint,
        section: Section,
    ) -> PyResult<()> {
        let children = self.children.bind(py);
        children.append((constraint, section))
    }

    fn find_child<'py>(
        &self,
        py: Python<'py>,
        name: &str,
    ) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let children = self.children.bind(py);

        for child in children {
            let tuple: Bound<'_, PyTuple> = child.downcast_into()?;
            let section: Section = tuple.get_item(1)?.extract()?;

            if section.name == name {
                return Ok(Some(tuple));
            }

            if let Some(found) = section.find_child(py, name)? {
                return Ok(Some(found));
            }
        }

        Ok(None)
    }

    fn all_layouts<'py>(&self, py: Python<'py>) -> Result<Bound<'py, PyDict>, Error> {
        let result = PyDict::new(py);
        self.all_children_inner(py, &result)?;
        Ok(result)
    }

    fn __str__(&self, py: Python<'_>) -> String {
        let children = self.children.bind(py);
        format!("Section({:?}, {} children)", self.name, children.len())
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

/// `Constraint` is a wrapper for the Ratatui `ratatui::layout::Constraint` enum, making it
/// `PyO3` compatible.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Display)]
#[pyclass]
pub enum Constraint {
    Min(u16),
    Max(u16),
    Length(u16),
    Percentage(u16),
    Ratio(u32, u32),
    Fill(u16),
}

#[pymethods]
impl Constraint {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        self.to_string()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl From<Constraint> for ratatui::layout::Constraint {
    fn from(value: Constraint) -> Self {
        match value {
            Constraint::Min(value) => Self::Min(value),
            Constraint::Max(value) => Self::Max(value),
            Constraint::Length(value) => Self::Length(value),
            Constraint::Percentage(value) => Self::Percentage(value),
            Constraint::Ratio(num, den) => Self::Ratio(num, den),
            Constraint::Fill(value) => Self::Fill(value),
        }
    }
}

/// `Direction` is a wrapper for the Ratatui `ratatui::layout::Direction` enum, making it
/// `PyO3` compatible.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Display)]
#[pyclass]
pub enum Direction {
    Horizontal,
    #[default]
    Vertical,
}

#[pymethods]
impl Direction {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        self.to_string()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl From<Direction> for ratatui::layout::Direction {
    fn from(value: Direction) -> Self {
        match value {
            Direction::Horizontal => Self::Horizontal,
            Direction::Vertical => Self::Vertical,
        }
    }
}
