use std::fmt::Debug;

use strum::Display;

use crate::app::TabShortcut;

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
