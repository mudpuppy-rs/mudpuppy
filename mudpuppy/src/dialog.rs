use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use pyo3::{Py, PyAny, pyclass, pymethods};
use tracing::{debug, trace};

use crate::keyboard::{KeyCode, KeyEvent};

/// Priority for dialog display. Higher priority dialogs are shown first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[pyclass(frozen, eq, eq_int)]
pub enum DialogPriority {
    Low = 0,
    Normal = 1,
    High = 2,
}

/// Severity level for notification dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[pyclass(frozen, eq, eq_int)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Action to take when a confirmation dialog is confirmed.
#[derive(Clone)]
#[pyclass(frozen)]
pub enum ConfirmAction {
    /// Quit the application.
    Quit {},
    /// Call a Python async callback.
    PyCallback(Py<PyAny>),
}

impl std::fmt::Debug for ConfirmAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfirmAction::Quit {} => write!(f, "Quit"),
            ConfirmAction::PyCallback(_) => write!(f, "PyCallback(<callable>)"),
        }
    }
}

/// Type of dialog to display.
#[derive(Clone)]
#[pyclass]
pub enum DialogKind {
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
}

impl std::fmt::Debug for DialogKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
        }
    }
}

/// A modal dialog overlay.
#[derive(Debug, Clone)]
#[pyclass]
pub struct Dialog {
    #[pyo3(get, set)]
    pub id: String,
    #[pyo3(get, set)]
    pub kind: DialogKind,
    pub expires_at: Option<Instant>,
    #[pyo3(get, set)]
    pub priority: DialogPriority,
}

impl Dialog {
    /// Update the occurrence count for a notification dialog.
    pub fn increment_count(&mut self) {
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

/// Tracking information for error deduplication.
#[derive(Debug)]
struct ErrorTracker {
    count: usize,
    last_shown: Instant,
    expires_at: Instant,
}

/// Manages modal dialogs and notifications.
#[derive(Debug)]
#[pyclass]
pub struct DialogManager {
    /// Active dialogs, sorted by priority (highest first).
    active: VecDeque<Dialog>,

    /// Error deduplication tracking.
    recent_errors: HashMap<u64, ErrorTracker>,

    /// Minimum time between showing the same error.
    error_cooldown: Duration,

    /// Maximum number of dialogs to keep in queue.
    max_concurrent: usize,
}

impl Default for DialogManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DialogManager {
    /// Create a new dialog manager with default settings.
    pub fn new() -> Self {
        Self {
            active: VecDeque::new(),
            recent_errors: HashMap::new(),
            error_cooldown: Duration::from_secs(5),
            max_concurrent: 3,
        }
    }

    /// Get the currently active (topmost) dialog.
    pub fn get_active(&self) -> Option<&Dialog> {
        self.active.front()
    }

    /// Get a mutable reference to the currently active dialog.
    pub fn get_active_mut(&mut self) -> Option<&mut Dialog> {
        self.active.front_mut()
    }

    /// Check for expired dialogs and clean up old error tracking.
    pub fn tick(&mut self) {
        let now = Instant::now();

        // Remove expired dialogs
        self.active.retain(|dialog| {
            let Some(expires_at) = dialog.expires_at else {
                return true;
            };

            if expires_at < now {
                debug!(id = ?dialog.id, "dialog expired");
                return false;
            }

            true
        });

        // Clean up expired error tracking
        self.recent_errors
            .retain(|_, tracker| tracker.expires_at >= now);
    }

    /// Handle a key event. Returns Some(action) if a confirmation was accepted, or None.
    /// Also returns a bool indicating if the event was consumed.
    pub(crate) fn handle_key(&mut self, key: &KeyEvent) -> (bool, Option<ConfirmAction>) {
        let KeyCode::Char(key_char) = key.code else {
            return (false, None);
        };
        let Some(dialog) = self.get_active_mut() else {
            return (false, None);
        };

        match &mut dialog.kind {
            DialogKind::Confirmation { confirm_key, .. } => {
                if key_char == *confirm_key {
                    debug!(id = ?dialog.id, "confirmation accepted");
                    // Take the dialog to get the action
                    let dialog = self.active.pop_front().unwrap();
                    if let DialogKind::Confirmation { action, .. } = dialog.kind {
                        return (true, Some(action));
                    }
                } else {
                    debug!(id = ?dialog.id, ?key, "confirmation cancelled");
                    self.active.pop_front();
                }
                (true, None)
            }
            DialogKind::Notification { dismissible, .. } => {
                if *dismissible {
                    debug!(id = ?dialog.id, "notification dismissed by key press");
                    self.active.pop_front();
                    (true, None)
                } else {
                    // Non-dismissible notifications ignore key presses
                    (false, None)
                }
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
    /// Add a dialog to the manager.
    pub fn add_dialog(&mut self, dialog: Dialog) {
        debug!(id = ?dialog.id, priority = ?dialog.priority, "adding dialog");

        // Find the insertion point to maintain priority order
        let insert_pos = self
            .active
            .iter()
            .position(|d| d.priority < dialog.priority)
            .unwrap_or(self.active.len());

        self.active.insert(insert_pos, dialog);

        // Trim if we exceed max concurrent
        while self.active.len() > self.max_concurrent {
            if let Some(removed) = self.active.pop_back() {
                debug!(id = ?removed.id, "removing dialog due to queue overflow");
            }
        }
    }

    /// Show an error notification with deduplication.
    pub fn show_error(&mut self, message: String) {
        let hash = Self::calculate_hash(&message);

        // Check if we've seen this error recently
        if let Some(tracker) = self.recent_errors.get_mut(&hash) {
            if tracker.last_shown.elapsed() < self.error_cooldown {
                // Still in cooldown - increment count
                tracker.count += 1;

                // Update existing dialog message to show count
                if let Some(dialog) = self.active.iter_mut().find(|d| {
                    matches!(
                        &d.kind,
                        DialogKind::Notification {
                            severity: Severity::Error,
                            ..
                        }
                    ) && d.id == format!("error_{hash}")
                }) {
                    dialog.increment_count();
                    trace!(
                        hash,
                        count = tracker.count,
                        "updated error occurrence count"
                    );
                }

                return;
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

        self.add_dialog(dialog);

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
    }

    /// Show a warning notification.
    pub fn show_warning(&mut self, message: String) {
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

        self.add_dialog(dialog);
    }

    /// Show an info notification.
    pub fn show_info(&mut self, message: String) {
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

        self.add_dialog(dialog);
    }

    /// Show a confirmation dialog.
    pub fn show_confirmation(
        &mut self,
        message: String,
        confirm_key: char,
        action: ConfirmAction,
        timeout: Option<Duration>,
    ) {
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

        self.add_dialog(dialog);
    }

    /// Dismiss a specific dialog by ID.
    pub fn dismiss(&mut self, id: &str) {
        if let Some(pos) = self.active.iter().position(|d| d.id == id) {
            let removed = self.active.remove(pos).unwrap();
            debug!(id = ?removed.id, "dismissed dialog");
        }
    }

    /// Clear all dialogs.
    pub fn clear(&mut self) {
        debug!(count = self.active.len(), "clearing all dialogs");
        self.active.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_priority_ordering() {
        let mut dm = DialogManager::new();

        dm.add_dialog(Dialog {
            id: "low".to_string(),
            kind: DialogKind::Notification {
                message: "Low priority".to_string(),
                severity: Severity::Info,
                dismissible: true,
                occurrence_count: 1,
            },
            expires_at: None,
            priority: DialogPriority::Low,
        });

        dm.add_dialog(Dialog {
            id: "high".to_string(),
            kind: DialogKind::Notification {
                message: "High priority".to_string(),
                severity: Severity::Error,
                dismissible: true,
                occurrence_count: 1,
            },
            expires_at: None,
            priority: DialogPriority::High,
        });

        // High priority should be shown first
        assert_eq!(dm.get_active().unwrap().id, "high");
    }

    #[test]
    fn test_error_deduplication() {
        let mut dm = DialogManager::new();

        dm.show_error("Test error".to_string());
        assert_eq!(dm.active.len(), 1);

        // Same error immediately - should not add new dialog
        dm.show_error("Test error".to_string());
        assert_eq!(dm.active.len(), 1);

        // Different error - should add
        dm.show_error("Different error".to_string());
        assert_eq!(dm.active.len(), 2);
    }
}
