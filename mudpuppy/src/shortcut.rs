use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};

use pyo3::types::PyAnyMethods;
use pyo3::{Py, PyAny, Python, pyclass, pymethods};
use strum::Display;
use tracing::error;

use crate::error::Error;
use crate::keyboard::KeyEvent;
use crate::python;
use crate::python::{label_for_coroutine, require_coroutine};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
#[pyclass(frozen, eq, hash)]
pub(crate) enum Shortcut {
    #[strum(to_string = "Tab({0})")]
    Tab(TabShortcut),
    #[strum(to_string = "Menu({0})")]
    Menu(MenuShortcut),
    #[strum(to_string = "SessionInput({0})")]
    SessionInput(InputShortcut),
    #[strum(to_string = "Scroll({0})")]
    Scroll(ScrollShortcut),
    #[strum(to_string = "Settings({0})")]
    Settings(SettingsShortcut),
    #[strum(to_string = "PythonShortcut({0})")] // TODO(XXX): improve PythonShortcut to_string
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

impl From<ScrollShortcut> for Shortcut {
    fn from(shortcut: ScrollShortcut) -> Self {
        Self::Scroll(shortcut)
    }
}

impl From<SettingsShortcut> for Shortcut {
    fn from(shortcut: SettingsShortcut) -> Self {
        Self::Settings(shortcut)
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

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
#[pyclass(frozen, eq, hash)]
pub(crate) enum ScrollShortcut {
    Up,
    Down,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
#[pyclass(frozen, eq, hash)]
pub(crate) enum SettingsShortcut {
    ToggleGmcpDebug,
}

#[derive(Debug, Clone)]
#[pyclass(frozen, eq, hash)]
pub(crate) struct PythonShortcut {
    label: String,
    // async def example(key_event: KeyEvent, active_sesh: Optional[Session], active_tab: Tab):
    //   pass
    awaitable: Py<PyAny>,
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

        let future = Python::attach(|py| {
            let awaitable = self
                .awaitable
                .bind(py)
                .call1((*key_event, active_sesh, active_tab))?;
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
    pub(crate) fn new(py: Python<'_>, awaitable: Py<PyAny>) -> Result<Self, Error> {
        require_coroutine(py, "PythonShortcut", &awaitable)?;
        Ok(Self {
            label: label_for_coroutine(py, &awaitable).unwrap_or("unknown".to_string()),
            awaitable,
        })
    }
}

impl Display for PythonShortcut {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
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
