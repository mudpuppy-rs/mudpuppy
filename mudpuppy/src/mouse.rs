//! Generic representations of mouse events
//!
//! Allows the Python API and other headless bits to operate without ratatui, or a crossterm
//! dependency.

use crate::keyboard::KeyModifiers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MouseEvent {
    pub(crate) kind: MouseEventKind,
    pub(crate) column: u16,
    pub(crate) row: u16,
    pub(crate) modifiers: KeyModifiers,
}

impl MouseEvent {
    #[must_use]
    pub(crate) fn new(
        kind: MouseEventKind,
        column: u16,
        row: u16,
        modifiers: KeyModifiers,
    ) -> Self {
        Self {
            kind,
            column,
            row,
            modifiers,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseEventKind {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollDown,
    ScrollUp,
    ScrollLeft,
    ScrollRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseButton {
    Left,
    Right,
    Middle,
}
