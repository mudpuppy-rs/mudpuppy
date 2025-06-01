use std::collections::VecDeque;

use pyo3::{pyclass, pymethods};
use strum::Display;

use crate::net::connection;
use crate::session::{InputLine, MudLine};

#[derive(Debug, Default)]
pub(crate) struct Output {
    pub(crate) new_data: usize,
    received: VecDeque<OutputItem>,
}

impl Output {
    pub(crate) fn take_received(&mut self) -> &VecDeque<OutputItem> {
        // assume all new data will be read by the caller.
        self.new_data = 0;
        &self.received
    }

    pub(crate) fn add(&mut self, item: OutputItem) {
        self.new_data = self.new_data.saturating_add(1);
        self.received.push_back(item);
    }
}

#[derive(Debug, Clone, Display)]
#[pyclass]
pub(crate) enum OutputItem {
    /// An item of output, usually from the MUD server.
    Mud { line: MudLine },

    /// A line of input, usually from the player.
    Input { line: InputLine },

    /// A line of output that was detected as a prompt.
    Prompt { prompt: MudLine },

    /// An item of output related to the connection status changing.
    ConnectionEvent {
        message: String,
        info: Option<connection::Info>,
    },

    /// A line of output produced as a result of executing a mudpuppy command.
    CommandResult { error: bool, message: String },

    /// A line of debug data
    Debug { line: String },
}

#[pymethods]
impl OutputItem {
    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    #[staticmethod]
    fn mud(line: MudLine) -> Self {
        Self::Mud { line }
    }

    #[staticmethod]
    fn input(line: InputLine) -> Self {
        Self::Input { line }
    }

    #[staticmethod]
    fn command_result(message: String) -> Self {
        Self::CommandResult {
            error: false,
            message,
        }
    }

    #[staticmethod]
    fn failed_command_result(message: String) -> Self {
        Self::CommandResult {
            error: true,
            message,
        }
    }

    #[staticmethod]
    fn debug(line: String) -> Self {
        Self::Debug { line }
    }
}

impl From<InputLine> for OutputItem {
    fn from(line: InputLine) -> Self {
        Self::Input { line }
    }
}
