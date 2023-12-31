use std::collections::VecDeque;
use std::fmt::{Display, Formatter};

use pyo3::{pyclass, pymethods};

use crate::client::Status;
use crate::model::{InputLine, MudLine};

#[derive(Debug, Clone, Default)]
#[pyclass]
pub struct Output {
    pub new_data: usize,
    received: VecDeque<Item>,
}

impl Output {
    #[must_use]
    pub fn new() -> Self {
        Output::default()
    }

    pub fn read_received(&mut self) -> &VecDeque<Item> {
        // assume all new data will be read by the caller.
        self.new_data = 0;
        &self.received
    }

    pub fn extend(&mut self, items: impl IntoIterator<Item = Item> + ExactSizeIterator) {
        self.new_data = self.new_data.saturating_add(items.len());
        self.received.extend(items);
    }

    pub fn set(
        &mut self,
        items: impl IntoIterator<Item = Item> + ExactSizeIterator,
        changed: bool,
    ) {
        self.received.clear();
        self.extend(items);
        if !changed {
            self.new_data = 0;
        }
    }
}

#[pymethods]
impl Output {
    #[must_use]
    pub fn len(&self) -> usize {
        self.received.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.received.is_empty()
    }

    pub fn push(&mut self, item: Item) {
        //trace!("adding item {item:?}");
        self.received.push_back(item);
        self.new_data = self.new_data.saturating_add(1);
    }

    #[pyo3(name = "set")]
    pub fn set_py(&mut self, items: Vec<Item>) {
        self.received.clear();
        self.extend(items.into_iter());
    }
}

#[derive(Debug, Clone)]
#[pyclass(name = "OutputItem")]
pub enum Item {
    /// An item of output, usually from the MUD server.
    Mud { line: MudLine },

    /// A line of input, usually from the player.
    Input { line: InputLine },

    /// A line of output that was detected as a prompt.
    Prompt { prompt: MudLine },

    /// A line of output that was detected as a prompt, and should be held in-place in the output.
    HeldPrompt { prompt: MudLine },

    /// An item of output related to the connection status changing.
    ConnectionEvent { status: Status },

    /// A line of output produced as a result of executing a mudpuppy command.
    CommandResult { error: bool, message: String },

    /// A line of output from a previous session.
    PreviousSession { line: MudLine },

    /// A line of debug data
    Debug { line: String },
}

#[pymethods]
impl Item {
    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    #[staticmethod]
    fn mud(line: MudLine) -> Self {
        Item::Mud { line }
    }

    #[staticmethod]
    fn input(line: InputLine) -> Self {
        Item::Input { line }
    }

    #[staticmethod]
    fn prompt(prompt: MudLine) -> Self {
        Item::Prompt { prompt }
    }

    #[staticmethod]
    fn held_prompt(prompt: MudLine) -> Self {
        Item::HeldPrompt { prompt }
    }

    #[staticmethod]
    fn connection_event(status: Status) -> Self {
        Item::ConnectionEvent { status }
    }

    #[staticmethod]
    fn command_result(message: String) -> Self {
        Item::CommandResult {
            error: false,
            message,
        }
    }

    #[staticmethod]
    fn failed_command_result(message: String) -> Self {
        Item::CommandResult {
            error: true,
            message,
        }
    }

    #[staticmethod]
    fn previous_session(line: MudLine) -> Self {
        Item::PreviousSession { line }
    }

    #[staticmethod]
    fn debug(line: String) -> Self {
        Item::Debug { line }
    }
}

impl Display for Item {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Item::Mud { line } => write!(f, "Line: {line}"),
            Item::Input { line, .. } => write!(f, "Input Line: {line}"),
            Item::Prompt { prompt } => write!(f, "Prompt: {prompt}"),
            Item::HeldPrompt { prompt } => write!(f, "Held Prompt: {prompt}"),
            Item::ConnectionEvent { status } => write!(f, "Connection Event: {status}"),
            Item::CommandResult {
                error: true,
                message,
            } => {
                write!(f, "Command Result Error: {message}")
            }
            Item::CommandResult { message, .. } => {
                write!(f, "Command Result: {message}")
            }
            Item::PreviousSession { line } => {
                write!(f, "Line (Previous session): {line}")
            }
            Item::Debug { line } => {
                write!(f, "Debug: {line}")
            }
        }
    }
}
