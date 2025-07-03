use std::fmt::Debug;

use pyo3::pyclass;
use strum::Display;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
#[pyclass(frozen, eq, hash)]
pub(crate) enum Shortcut {
    #[strum(to_string = "Tab({0})")]
    Tab(TabShortcut),
    #[strum(to_string = "Menu({0})")]
    Menu(MenuShortcut),
    #[strum(to_string = "SessionInput({0})")]
    SessionInput(InputShortcut),
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
