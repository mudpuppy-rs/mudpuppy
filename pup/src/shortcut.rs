use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use crate::error::Error;
use crate::keyboard::KeyEvent;
use crate::python;
use crate::python::require_coroutine;
use pyo3::types::PyAnyMethods;
use pyo3::{PyObject, Python, pyclass, pymethods};
use strum::Display;
use tracing::error;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
#[pyclass(frozen, eq, hash)]
pub(crate) enum Shortcut {
    #[strum(to_string = "Tab({0})")]
    Tab(TabShortcut),
    #[strum(to_string = "Menu({0})")]
    Menu(MenuShortcut),
    #[strum(to_string = "SessionInput({0})")]
    SessionInput(InputShortcut),
    #[strum(to_string = "PythonShortcut")] // TODO(XXX): improve PythonShortcut to_string
    Python(PythonShortcut),
    Quit {},
}

impl From<TabShortcut> for Shortcut {
    fn from(shortcut: TabShortcut) -> Self {
        Self::Tab(shortcut)
    }
}

impl From<MenuShortcut> for Shortcut {
    fn from(shortcut: MenuShortcut) -> Self {
        Self::Menu(shortcut)
    }
}

impl From<InputShortcut> for Shortcut {
    fn from(shortcut: InputShortcut) -> Self {
        Self::SessionInput(shortcut)
    }
}

impl From<PythonShortcut> for Shortcut {
    fn from(shortcut: PythonShortcut) -> Self {
        Self::Python(shortcut)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
#[pyclass(frozen, eq, hash)]
pub(crate) enum TabShortcut {
    SwitchToNext {},
    SwitchToPrevious {},
    SwitchToList {},
    SwitchTo { tab_id: u32 },
    SwitchToSession { session: u32 },
    MoveLeft { tab_id: Option<u32> },
    MoveRight { tab_id: Option<u32> },
    Close { tab_id: Option<u32> },
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
#[pyclass(frozen, eq, hash)]
pub(crate) enum MenuShortcut {
    Up,
    Down,
    Connect,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
#[pyclass(frozen, eq, hash)]
pub(crate) enum InputShortcut {
    Send,
    CursorLeft,
    CursorRight,
    CursorToStart,
    CursorToEnd,
    CursorWordLeft,
    CursorWordRight,
    DeletePrev,
    DeleteNext,
    CursorDeleteWordLeft,
    CursorDeleteWordRight,
    CursorDeleteToEnd,
    Reset,
}

#[derive(Debug, Clone)]
#[pyclass(frozen, eq, hash)]
pub(crate) struct PythonShortcut {
    awaitable: PyObject,
}

impl PythonShortcut {
    pub(crate) fn execute(
        &self,
        active_tab: python::Tab,
        active_sesh: Option<python::Session>,
        key_event: &KeyEvent,
    ) -> Result<(), Error> {
        let active_tab_id = active_tab.id;
        let active_sesh_id = active_sesh.as_ref().map(|s| s.id);

        let future = Python::with_gil(|py| {
            let awaitable = self
                .awaitable
                .bind(py)
                .call1((active_tab, active_sesh, *key_event))?;
            pyo3_async_runtimes::tokio::into_future(awaitable)
        })?;

        let key_event = *key_event;
        tokio::spawn(async move {
            if let Err(err) = future.await {
                // Note: Error::from() to collect backtrace from PyErr.
                error!(
                    key_event = key_event.to_string(),
                    active_tab_id,
                    active_sesh_id,
                    "shortcut callback error: {}",
                    Error::from(err)
                );
            }
        });

        Ok(())
    }
}

#[pymethods]
impl PythonShortcut {
    #[new]
    fn new(py: Python<'_>, awaitable: PyObject) -> Result<Self, Error> {
        require_coroutine(py, "PythonShortcut", &awaitable)?;
        Ok(Self { awaitable })
    }
}

impl PartialEq for PythonShortcut {
    fn eq(&self, other: &Self) -> bool {
        self.awaitable.as_ptr() == other.awaitable.as_ptr()
    }
}

impl Eq for PythonShortcut {}

impl Hash for PythonShortcut {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.awaitable.as_ptr().hash(state);
    }
}
