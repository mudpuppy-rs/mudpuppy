use std::collections::HashMap;
use std::fmt::Write;

use pyo3::types::{PyAnyMethods, PyDict, PyList, PyListMethods, PyTuple};
use pyo3::{Bound, Py, PyAny, PyResult, Python, pyclass, pymethods};
use ratatui::layout::{Layout, Rect};
use strum::Display;

use crate::error::{Error, ErrorKind};

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
    // Generic tree collection - restructured to avoid recursive closure passing
    fn collect_from_tree(
        &self,
        py: Python<'_>,
        parent_rect: Rect,
    ) -> Result<Vec<(String, Rect)>, Error> {
        let mut results = Vec::new();
        let rects = self.partition(py, parent_rect)?;

        for (idx, child) in self.children.bind(py).iter().enumerate() {
            let (_, section) = Self::extract_child_tuple(&child)?;
            let rect = rects[idx];

            results.push((section.name.clone(), rect));

            let mut child_results = section.collect_from_tree(py, rect)?;
            results.append(&mut child_results);
        }

        Ok(results)
    }

    pub(crate) fn partition_by_name(
        &self,
        py: Python<'_>,
        parent: Rect,
    ) -> Result<HashMap<String, Rect>, Error> {
        let items = self.collect_from_tree(py, parent)?;

        let mut result = HashMap::with_capacity(items.len());
        for (name, rect) in items {
            if result.insert(name.clone(), rect).is_some() {
                return Err(ErrorKind::DuplicateLayoutSection(name).into());
            }
        }
        Ok(result)
    }

    fn partition(&self, py: Python<'_>, parent: Rect) -> Result<Vec<Rect>, Error> {
        let constraints: Vec<_> = self
            .children
            .bind(py)
            .iter()
            .map(|child| Self::extract_child_tuple(&child).map(|(c, _)| c.into()))
            .collect::<Result<_, _>>()?;

        Ok(Layout::default()
            .direction(self.direction.into())
            .margin(self.margin)
            .constraints::<Vec<ratatui::layout::Constraint>>(constraints)
            .split(parent)
            .to_vec())
    }

    fn tree_string_inner(&self, py: Python<'_>, depth: usize) -> PyResult<String> {
        let mut result = match depth == 0 {
            true => format!("{}:\n", self.name),
            false => String::new(),
        };

        self.for_each_child(py, |_, constraint, section| {
            writeln!(
                result,
                "{}├─ {} - {}",
                "  ".repeat(depth),
                constraint,
                section.name
            )
            .unwrap();
            write!(result, "{}", section.tree_string_inner(py, depth + 1)?).unwrap();
            Ok(())
        })?;

        Ok(result)
    }

    // Generic section modification by name - restructured to avoid recursive closure passing
    fn modify_section_by_name(
        &mut self,
        py: Python<'_>,
        target_name: &str,
        constraint: Constraint,
    ) -> Result<bool, Error> {
        let children = self.children.bind(py);

        // Check direct children first
        for (idx, child) in children.iter().enumerate() {
            let (_, section) = Self::extract_child_tuple(&child)?;
            if section.name == target_name {
                children.set_item(idx, (constraint, section))?;
                return Ok(true);
            }
        }

        // Recurse into children
        for child in children.iter() {
            let (_, mut section) = Self::extract_child_tuple(&child)?;
            if section.modify_section_by_name(py, target_name, constraint)? {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn find_in_tree<T, F>(
        &self,
        py: Python<'_>,
        name: &str,
        extractor: F,
    ) -> Result<Option<T>, Error>
    where
        F: Fn(&Section, &Constraint) -> Option<T> + Copy,
    {
        let mut found = None;
        self.for_each_child(py, |_, constraint, section| {
            if section.name == name {
                if let Some(result) = extractor(section, &constraint) {
                    found = Some(result);
                    return Ok(());
                }
            }

            if let Some(result) = section.find_in_tree(py, name, extractor)? {
                found = Some(result);
            }
            Ok(())
        })?;
        Ok(found)
    }

    fn for_each_child<F>(&self, py: Python<'_>, mut f: F) -> Result<(), Error>
    where
        F: FnMut(usize, Constraint, &Section) -> Result<(), Error>,
    {
        for (idx, child) in self.children.bind(py).iter().enumerate() {
            let (constraint, section) = Self::extract_child_tuple(&child)?;
            f(idx, constraint, &section)?;
        }
        Ok(())
    }

    fn extract_child_tuple(child: &Bound<PyAny>) -> Result<(Constraint, Section), Error> {
        let tuple: &Bound<PyTuple> = child.cast().map_err(ErrorKind::from)?;
        Ok((
            tuple
                .get_item(0)?
                .extract::<Constraint>()
                .map_err(ErrorKind::from)?,
            tuple
                .get_item(1)?
                .extract::<Section>()
                .map_err(ErrorKind::from)?,
        ))
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

    pub(crate) fn insert_child(
        &mut self,
        py: Python<'_>,
        position: usize,
        constraint: Constraint,
        section: Section,
    ) -> PyResult<()> {
        let children = self.children.bind(py);
        children.insert(position, (constraint, section))
    }

    pub(crate) fn append_child(
        &mut self,
        py: Python<'_>,
        constraint: Constraint,
        section: Section,
    ) -> PyResult<()> {
        let children = self.children.bind(py);
        children.append((constraint, section))
    }

    // Cleaner alias for append_child
    fn add_child(
        &mut self,
        py: Python<'_>,
        constraint: Constraint,
        section: Section,
    ) -> PyResult<()> {
        self.append_child(py, constraint, section)
    }

    fn get_section(&self, py: Python<'_>, name: &str) -> Result<Option<Section>, Error> {
        if self.name == name {
            return Ok(Some(self.clone()));
        }

        self.find_in_tree(py, name, |section, _| Some(section.clone()))
    }

    fn get_constraint(&self, py: Python<'_>, name: &str) -> Result<Option<Constraint>, Error> {
        self.find_in_tree(py, name, |_, constraint| Some(*constraint))
    }

    fn get_parent(&self, py: Python<'_>, target_name: &str) -> Result<Option<Section>, Error> {
        // Check if any direct child matches
        for child in self.children.bind(py).iter() {
            let (_, section) = Self::extract_child_tuple(&child)?;
            if section.name == target_name {
                return Ok(Some(self.clone()));
            }
        }

        // If not found in direct children, recurse
        for child in self.children.bind(py).iter() {
            let (_, section) = Self::extract_child_tuple(&child)?;
            if let Some(parent) = section.get_parent(py, target_name)? {
                return Ok(Some(parent));
            }
        }

        Ok(None)
    }

    fn set_constraint(
        &mut self,
        py: Python<'_>,
        name: &str,
        constraint: Constraint,
    ) -> Result<(), Error> {
        if !self.modify_section_by_name(py, name, constraint)? {
            return Err(ErrorKind::UnknownLayoutSection(name.to_string()).into());
        }
        Ok(())
    }

    fn children<'py>(&'py self, py: Python<'py>) -> &'py Bound<'py, PyList> {
        self.children.bind(py)
    }

    // Simplified all_layouts using direct tree traversal
    fn all_layouts<'py>(&self, py: Python<'py>) -> Result<Bound<'py, PyDict>, Error> {
        fn collect_layouts(
            section: &Section,
            py: Python<'_>,
            result: &Bound<'_, PyDict>,
        ) -> Result<(), Error> {
            section.for_each_child(py, |idx, _, child_section| {
                if result.contains(&child_section.name)? {
                    return Err(
                        ErrorKind::DuplicateLayoutSection(child_section.name.clone()).into(),
                    );
                }

                let child_tuple = section.children.bind(py).get_item(idx)?;
                result.set_item(&child_section.name, child_tuple)?;

                collect_layouts(child_section, py, result)?;
                Ok(())
            })
        }

        let result = PyDict::new(py);
        collect_layouts(self, py, &result)?;
        Ok(result)
    }

    fn __str__(&self, py: Python<'_>) -> String {
        self.tree_string_inner(py, 0).unwrap()
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
    #[strum(to_string = "Min({0})")]
    Min(u16),
    #[strum(to_string = "Max({0})")]
    Max(u16),
    #[strum(to_string = "Length({0})")]
    Length(u16),
    #[strum(to_string = "Percentage({0})")]
    Percentage(u16),
    #[strum(to_string = "Ratio({0},{1})")]
    Ratio(u32, u32),
    #[strum(to_string = "Fill({0})")]
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
