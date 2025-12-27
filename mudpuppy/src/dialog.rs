use std::collections::{HashMap, VecDeque};
use std::fmt::{self, Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use pyo3::{Py, PyAny, Python, pyclass, pymethods};
use strum::Display;
use tracing::{debug, trace};

use crate::keyboard::{KeyCode, KeyEvent};
use crate::mouse::{MouseButton, MouseEvent, MouseEventKind};
use crate::session::Buffer;

/// Manages modal dialogs and notifications.
#[derive(Debug)]
#[pyclass]
pub struct DialogManager {
    /// Active dialogs, sorted by priority (highest first).
    active: VecDeque<Py<Dialog>>,

    /// Error deduplication tracking.
    recent_errors: HashMap<u64, ErrorTracker>,

    /// Minimum time between showing the same error.
    error_cooldown: Duration,

    /// Maximum number of dialogs to keep in queue.
    #[pyo3(get, set)]
    max_concurrent: usize,

    /// Current drag operation state.
    drag_state: Option<DragState>,
}

impl DialogManager {
    /// Create a new dialog manager with default settings.
    pub(crate) fn new() -> Self {
        Self {
            active: VecDeque::new(),
            recent_errors: HashMap::new(),
            error_cooldown: Duration::from_secs(5),
            max_concurrent: 3,
            drag_state: None,
        }
    }

    /// Get the currently active (topmost) dialog.
    pub(crate) fn get_active(&self) -> Option<&Py<Dialog>> {
        self.active.front()
    }

    /// Get all active dialogs (for rendering).
    pub(crate) fn get_all_active(&self) -> impl DoubleEndedIterator<Item = &Py<Dialog>> {
        self.active.iter()
    }

    /// Check for expired dialogs and clean up old error tracking.
    pub(crate) fn tick(&mut self) {
        let now = Instant::now();

        // Remove expired dialogs
        self.active.retain(|py_dialog| {
            Python::attach(|py| {
                let dialog = py_dialog.borrow(py);
                let Some(expires_at) = dialog.expires_at else {
                    return true;
                };

                if expires_at < now {
                    debug!(id = ?dialog.id, "dialog expired");
                    return false;
                }

                true
            })
        });

        // Clean up expired error tracking
        self.recent_errors
            .retain(|_, tracker| tracker.expires_at >= now);
    }

    /// Handle a key event. Returns Some(action) if a confirmation was accepted, or None.
    /// Also returns a bool indicating if the event was consumed.
    pub(crate) fn handle_key(&mut self, key: &KeyEvent) -> (bool, Option<ConfirmAction>) {
        // Only the topmost dialog (highest priority, front of queue) gets keyboard events
        let py_dialog = Python::attach(|py| self.active.front().map(|d| d.clone_ref(py)));

        let Some(py_dialog) = py_dialog else {
            trace!("handle_key: no active dialogs");
            return (false, None);
        };

        Python::attach(|py| {
            let dialog = py_dialog.borrow(py);
            trace!(id = ?dialog.id, kind = ?dialog.kind, priority = ?dialog.priority, "handle_key: checking topmost dialog");

            match &dialog.kind {
                DialogKind::Confirmation { confirm_key, .. } => {
                    // Check if this is the confirm key (must be a char)
                    let is_confirm = matches!(key.code, KeyCode::Char(c) if c == *confirm_key);

                    if is_confirm {
                        debug!(id = ?dialog.id, "confirmation accepted");
                        let py_dialog = self.active.pop_front().unwrap();
                        if let DialogKind::Confirmation { action, .. } = &py_dialog.borrow(py).kind
                        {
                            (true, Some(action.clone()))
                        } else {
                            (true, None)
                        }
                    } else {
                        debug!(id = ?dialog.id, ?key, "confirmation cancelled");
                        self.active.pop_front();
                        (true, None)
                    }
                }
                DialogKind::Notification { dismissible, .. } => {
                    if *dismissible {
                        // Only dismiss on char keys to avoid issues with special keys
                        if matches!(key.code, KeyCode::Char(_)) {
                            debug!(id = ?dialog.id, kind = ?dialog.kind, "dialog dismissed by key press");
                            self.active.pop_front();
                            (true, None)
                        } else {
                            // Non-char keys don't dismiss notifications
                            (false, None)
                        }
                    } else {
                        // Non-dismissible dialogs ignore key presses
                        (false, None)
                    }
                }
                DialogKind::FloatingWindow { .. } => {
                    // Floating windows don't respond to keyboard events - they're mouse-only
                    // Event is not consumed, so it falls through to the app
                    (false, None)
                }
            }
        })
    }

    /// Handle a mouse event for dragging floating windows.
    /// Takes the mouse event and a list of (dialog_index, rect) pairs for hit testing.
    /// Returns true if the event was consumed.
    pub(crate) fn handle_mouse(
        &mut self,
        py: Python<'_>,
        mouse: &MouseEvent,
        window_rects: &[(usize, (u16, u16, u16, u16))],
    ) -> bool {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if we clicked on a floating window
                for &(dialog_idx, (x, y, width, height)) in window_rects {
                    if mouse.column >= x
                        && mouse.column < x + width
                        && mouse.row >= y
                        && mouse.row < y + height
                    {
                        // Start dragging this window
                        if let Some(py_dialog) = self.active.get(dialog_idx) {
                            let dialog = py_dialog.borrow(py);
                            if let DialogKind::FloatingWindow { window, .. } = &dialog.kind {
                                let mut window = window.borrow_mut(py);

                                // Convert percentage position to absolute if needed
                                let (abs_x, abs_y) = match window.position {
                                    Position::Absolute { x, y } => (x, y),
                                    Position::Percent { .. } => {
                                        // Already calculated in window_rects
                                        (x, y)
                                    }
                                };

                                // Update to absolute positioning
                                window.position = Position::Absolute { x: abs_x, y: abs_y };

                                self.drag_state = Some(DragState {
                                    dialog_index: dialog_idx,
                                    start_mouse_x: mouse.column,
                                    start_mouse_y: mouse.row,
                                    start_window_x: abs_x,
                                    start_window_y: abs_y,
                                });

                                trace!(
                                    id = ?dialog.id,
                                    x = abs_x,
                                    y = abs_y,
                                    "started dragging window"
                                );
                                return true;
                            }
                        }
                    }
                }
                false
            }
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                // Update window position if we're dragging
                if let Some(drag_state) = &self.drag_state {
                    if let Some(py_dialog) = self.active.get(drag_state.dialog_index) {
                        let dialog = py_dialog.borrow(py);
                        if let DialogKind::FloatingWindow { window, .. } = &dialog.kind {
                            let mut window = window.borrow_mut(py);

                            // Calculate new position based on mouse movement
                            let delta_x = mouse.column as i32 - drag_state.start_mouse_x as i32;
                            let delta_y = mouse.row as i32 - drag_state.start_mouse_y as i32;

                            let new_x = (drag_state.start_window_x as i32 + delta_x).max(0) as u16;
                            let new_y = (drag_state.start_window_y as i32 + delta_y).max(0) as u16;

                            window.position = Position::Absolute { x: new_x, y: new_y };

                            trace!(
                                id = ?dialog.id,
                                x = new_x,
                                y = new_y,
                                "dragging window"
                            );
                            return true;
                        }
                    }
                }
                false
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // End dragging
                if self.drag_state.is_some() {
                    trace!("ended dragging window");
                    self.drag_state = None;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Add a dialog to the manager.
    fn add_dialog(&mut self, py: Python<'_>, dialog: &Py<Dialog>) {
        let d = dialog.borrow(py);
        debug!(id = ?d.id, priority = ?d.priority, "adding dialog");

        // Find the insertion point to maintain priority order
        let insert_pos = self
            .active
            .iter()
            .position(|py_d| py_d.borrow(py).priority < d.priority)
            .unwrap_or(self.active.len());

        self.active.insert(insert_pos, dialog.clone_ref(py));

        // Trim if we exceed max concurrent
        while self.active.len() > self.max_concurrent {
            if let Some(removed) = self.active.pop_back() {
                debug!(id = ?removed.borrow(py).id, "removing dialog due to queue overflow");
            }
        }
    }

    /// Calculate a hash for deduplication.
    fn calculate_hash(s: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }
}

#[pymethods]
impl DialogManager {
    /// Show an error notification with deduplication.
    pub(crate) fn show_error(&mut self, py: Python<'_>, message: String) -> Py<Dialog> {
        let hash = Self::calculate_hash(&message);

        // Check if we've seen this error recently
        if let Some(tracker) = self.recent_errors.get_mut(&hash) {
            if tracker.last_shown.elapsed() < self.error_cooldown {
                // Still in cooldown - increment count
                tracker.count += 1;

                // Update existing dialog message to show count
                // TODO(XXX): this sucks.
                let target_id = format!("error_{hash}");
                let py_dialog = self
                    .active
                    .iter()
                    .find(|d| {
                        let dialog = d.borrow(py);
                        matches!(
                            &dialog.kind,
                            DialogKind::Notification {
                                severity: Severity::Error,
                                ..
                            }
                        ) && dialog.id == target_id
                    })
                    .unwrap();
                py_dialog.borrow_mut(py).increment_count();
                trace!(
                    hash,
                    count = tracker.count,
                    "updated error occurrence count"
                );

                return py_dialog.clone_ref(py);
            }
        }

        // Not in cooldown or first occurrence - show dialog
        let dialog = Dialog {
            id: format!("error_{hash}"),
            kind: DialogKind::Notification {
                message,
                severity: Severity::Error,
                dismissible: true,
                occurrence_count: 1,
            },
            expires_at: Some(Instant::now() + Duration::from_secs(15)),
            priority: DialogPriority::Normal,
        };

        let py_dialog = Py::new(py, dialog).unwrap();
        self.add_dialog(py, &py_dialog);

        // Track this error
        let now = Instant::now();
        self.recent_errors.insert(
            hash,
            ErrorTracker {
                count: 1,
                last_shown: now,
                expires_at: now + self.error_cooldown + Duration::from_secs(5),
            },
        );

        py_dialog
    }

    /// Show a warning notification.
    pub(crate) fn show_warning(&mut self, py: Python<'_>, message: String) -> Py<Dialog> {
        let dialog = Dialog {
            id: format!("warning_{}", Self::calculate_hash(&message)),
            kind: DialogKind::Notification {
                message,
                severity: Severity::Warning,
                dismissible: true,
                occurrence_count: 1,
            },
            expires_at: Some(Instant::now() + Duration::from_secs(10)),
            priority: DialogPriority::Normal,
        };

        let py_dialog = Py::new(py, dialog).unwrap();
        self.add_dialog(py, &py_dialog);
        py_dialog
    }

    /// Show an info notification.
    pub(crate) fn show_info(&mut self, py: Python<'_>, message: String) -> Py<Dialog> {
        let dialog = Dialog {
            id: format!("info_{}", Self::calculate_hash(&message)),
            kind: DialogKind::Notification {
                message,
                severity: Severity::Info,
                dismissible: true,
                occurrence_count: 1,
            },
            expires_at: Some(Instant::now() + Duration::from_secs(5)),
            priority: DialogPriority::Low,
        };

        let py_dialog = Py::new(py, dialog).unwrap();
        self.add_dialog(py, &py_dialog);
        py_dialog
    }

    /// Show a confirmation dialog.
    pub(crate) fn show_confirmation(
        &mut self,
        py: Python<'_>,
        message: String,
        confirm_key: char,
        action: ConfirmAction,
        timeout: Option<Duration>,
    ) -> Py<Dialog> {
        let dialog = Dialog {
            id: format!("confirm_{}", Self::calculate_hash(&message)),
            kind: DialogKind::Confirmation {
                message,
                confirm_key,
                action,
            },
            expires_at: timeout.map(|d| Instant::now() + d),
            priority: DialogPriority::High,
        };

        let py_dialog = Py::new(py, dialog).unwrap();
        self.add_dialog(py, &py_dialog);
        py_dialog
    }

    /// Show a floating window. Returns the dialog so Python can hold onto it and mutate it.
    #[pyo3(signature = (window, *, id=None, dismissible=true, priority=DialogPriority::Low, timeout=None))]
    pub(crate) fn show_floating_window(
        &mut self,
        py: Python<'_>,
        window: FloatingWindow,
        id: Option<String>,
        dismissible: bool,
        priority: DialogPriority,
        timeout: Option<Duration>,
    ) -> Py<Dialog> {
        let id = id.unwrap_or_else(|| {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            format!(
                "floating-window-{}",
                COUNTER.fetch_add(1, Ordering::Relaxed)
            )
        });
        let py_window = Py::new(py, window).unwrap();
        let dialog = Dialog {
            id,
            kind: DialogKind::FloatingWindow {
                window: py_window,
                dismissible,
            },
            expires_at: timeout.map(|d| Instant::now() + d),
            priority,
        };

        let py_dialog = Py::new(py, dialog).unwrap();
        self.add_dialog(py, &py_dialog);
        py_dialog
    }

    /// Dismiss a specific dialog by ID.
    pub(crate) fn dismiss(&mut self, py: Python<'_>, id: &str) {
        let Some(pos) = self.active.iter().position(|d| d.borrow(py).id == id) else {
            return;
        };
        let removed = self.active.remove(pos).unwrap();
        let removed = removed.borrow(py);
        debug!(id = ?removed.id, "dismissed dialog");
    }

    /// Clear all dialogs.
    pub(crate) fn clear(&mut self) {
        debug!(count = self.active.len(), "clearing all dialogs");
        self.active.clear();
    }
}

impl Default for DialogManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Type of dialog to display.
#[derive(Clone)]
#[pyclass]
pub(crate) enum DialogKind {
    /// Confirmation dialog: requires specific key to confirm, any other key cancels.
    Confirmation {
        message: String,
        confirm_key: char,
        action: ConfirmAction,
    },

    /// Notification: auto-dismiss or any-key-dismiss.
    Notification {
        message: String,
        severity: Severity,
        dismissible: bool,
        occurrence_count: usize,
    },

    /// Floating window: a buffer-backed window with optional title and positioning.
    FloatingWindow {
        window: Py<FloatingWindow>,
        dismissible: bool,
    },
}

impl Debug for DialogKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DialogKind::Confirmation {
                message,
                confirm_key,
                ..
            } => f
                .debug_struct("Confirmation")
                .field("message", message)
                .field("confirm_key", confirm_key)
                .finish(),
            DialogKind::Notification {
                message, severity, ..
            } => f
                .debug_struct("Notification")
                .field("message", message)
                .field("severity", severity)
                .finish(),
            DialogKind::FloatingWindow {
                window,
                dismissible,
            } => Python::attach(|py| {
                let w = window.borrow(py);
                f.debug_struct("FloatingWindow")
                    .field("window", &*w)
                    .field("dismissible", dismissible)
                    .finish()
            }),
        }
    }
}

/// A modal dialog overlay.
#[derive(Clone)]
#[pyclass]
pub(crate) struct Dialog {
    #[pyo3(get, set)]
    pub(crate) id: String,
    #[pyo3(get, set)]
    pub(crate) kind: DialogKind,
    pub(crate) expires_at: Option<Instant>,
    #[pyo3(get, set)]
    pub(crate) priority: DialogPriority,
}

impl Debug for Dialog {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dialog")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("expires_at", &self.expires_at)
            .field("priority", &self.priority)
            .finish()
    }
}

#[pymethods]
impl Dialog {
    #[new]
    pub(crate) fn new(id: String, kind: DialogKind, priority: DialogPriority) -> Self {
        Self {
            id,
            kind,
            expires_at: None,
            priority,
        }
    }

    /// Update the occurrence count for a notification dialog.
    pub(crate) fn increment_count(&mut self) {
        let DialogKind::Notification {
            occurrence_count,
            message,
            ..
        } = &mut self.kind
        else {
            return;
        };

        *occurrence_count += 1;

        // TODO(XXX): This is stupid:
        // Update message to include count
        if message.contains("(occurred") {
            // Extract base message and update count
            if let Some(idx) = message.find(" (occurred") {
                let base_message = &message[..idx];
                *message = format!("{base_message} (occurred {occurrence_count} times)");
            }
        } else {
            let base_message = message.clone();
            *message = format!("{base_message} (occurred {occurrence_count} times)");
        }
    }
}

/// Priority for dialog display. Higher priority dialogs are shown first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Display)]
#[pyclass(frozen, eq, eq_int)]
pub(crate) enum DialogPriority {
    Low = 0,
    Normal = 1,
    High = 2,
}

/// Severity level for notification dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[pyclass(frozen, eq, eq_int)]
pub(crate) enum Severity {
    Info,
    Warning,
    Error,
}

/// Action to take when a confirmation dialog is confirmed.
#[derive(Clone)]
#[pyclass(frozen)]
pub(crate) enum ConfirmAction {
    /// Quit the application.
    Quit {},
    /// Call a Python async callback.
    PyCallback(Py<PyAny>),
}

impl Debug for ConfirmAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ConfirmAction::Quit {} => write!(f, "Quit"),
            ConfirmAction::PyCallback(_) => write!(f, "PyCallback(<callable>)"),
        }
    }
}

/// A floating window with a buffer and optional title.
#[derive(Clone)]
#[pyclass]
pub(crate) struct FloatingWindow {
    #[pyo3(get, set)]
    pub(crate) title: Option<String>,
    #[pyo3(get, set)]
    pub(crate) position: Position,
    #[pyo3(get, set)]
    pub(crate) size: Size,
    #[pyo3(get)]
    pub(crate) buffer: Py<Buffer>,
}

#[pymethods]
impl FloatingWindow {
    #[new]
    pub(crate) fn new(
        buffer: Py<Buffer>,
        position: Position,
        size: Size,
        title: Option<String>,
    ) -> Self {
        Self {
            title,
            position,
            size,
            buffer,
        }
    }
}

impl Debug for FloatingWindow {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("FloatingWindow")
            .field("title", &self.title)
            .field("position", &self.position)
            .field("size", &self.size)
            .field("buffer", &"<Buffer>")
            .finish()
    }
}

/// Position for floating windows (percentage or absolute).
#[derive(Debug, Clone, Copy, PartialEq)]
#[pyclass]
pub(crate) enum Position {
    /// Percentage of the screen (0-100).
    Percent { x: u16, y: u16 },
    /// Absolute position in cells.
    Absolute { x: u16, y: u16 },
}

#[pymethods]
impl Position {
    #[staticmethod]
    pub(crate) fn percent(x: u16, y: u16) -> Self {
        Self::Percent { x, y }
    }

    #[staticmethod]
    pub(crate) fn absolute(x: u16, y: u16) -> Self {
        Self::Absolute { x, y }
    }
}

/// Size for floating windows (percentage or absolute).
#[derive(Debug, Clone, Copy, PartialEq)]
#[pyclass]
pub(crate) enum Size {
    /// Percentage of the screen (0-100).
    Percent { width: u16, height: u16 },
    /// Absolute size in cells.
    Absolute { width: u16, height: u16 },
}

#[pymethods]
impl Size {
    #[staticmethod]
    pub(crate) fn percent(width: u16, height: u16) -> Self {
        Self::Percent { width, height }
    }

    #[staticmethod]
    pub(crate) fn absolute(width: u16, height: u16) -> Self {
        Self::Absolute { width, height }
    }
}

/// Tracking information for error deduplication.
#[derive(Debug)]
struct ErrorTracker {
    count: usize,
    last_shown: Instant,
    expires_at: Instant,
}

/// Tracking information for window drag operations.
#[derive(Debug, Clone)]
struct DragState {
    /// Index of the dialog being dragged in the active queue.
    dialog_index: usize,
    /// Mouse position where drag started.
    start_mouse_x: u16,
    start_mouse_y: u16,
    /// Window position when drag started (always absolute).
    start_window_x: u16,
    start_window_y: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_priority_ordering() {
        Python::initialize();
        Python::attach(|py| {
            let mut dm = DialogManager::new();

            let low_dialog = Dialog {
                id: "low".to_string(),
                kind: DialogKind::Notification {
                    message: "Low priority".to_string(),
                    severity: Severity::Info,
                    dismissible: true,
                    occurrence_count: 1,
                },
                expires_at: None,
                priority: DialogPriority::Low,
            };
            dm.add_dialog(py, &Py::new(py, low_dialog).unwrap());

            let high_dialog = Dialog {
                id: "high".to_string(),
                kind: DialogKind::Notification {
                    message: "High priority".to_string(),
                    severity: Severity::Error,
                    dismissible: true,
                    occurrence_count: 1,
                },
                expires_at: None,
                priority: DialogPriority::High,
            };
            dm.add_dialog(py, &Py::new(py, high_dialog).unwrap());

            // High priority should be shown first
            let active = dm.get_active().unwrap();
            assert_eq!(active.borrow(py).id, "high");
        });
    }

    #[test]
    fn test_error_deduplication() {
        Python::initialize();
        Python::attach(|py| {
            let mut dm = DialogManager::new();

            dm.show_error(py, "Test error".to_string());
            assert_eq!(dm.active.len(), 1);

            // Same error immediately - should not add new dialog
            dm.show_error(py, "Test error".to_string());
            assert_eq!(dm.active.len(), 1);

            // Different error - should add
            dm.show_error(py, "Different error".to_string());
            assert_eq!(dm.active.len(), 2);
        });
    }
}
