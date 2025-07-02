use std::fmt::Debug;

use strum::Display;

use crate::python;

#[derive(Debug, Clone, Display)]
pub(crate) enum Shortcut {
    Tab(TabShortcut),
    Quit,
}

impl From<TabShortcut> for Shortcut {
    fn from(shortcut: TabShortcut) -> Self {
        Self::Tab(shortcut)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
pub(crate) enum TabShortcut {
    Create { session: python::Session },
    SwitchToNext,
    SwitchToPrevious,
    SwitchToList,
    SwitchTo { tab_id: u32 },
    SwitchToSession { session: u32 },
    MoveLeft { tab_id: Option<u32> },
    MoveRight { tab_id: Option<u32> },
    Close { tab_id: Option<u32> },
}
