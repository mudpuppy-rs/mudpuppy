use std::collections::VecDeque;
use std::fmt;
use std::fmt::{Display, Formatter};

use pyo3::{pyclass, pymethods};
use strum::Display;
use tracing::trace;

use crate::error::{Error, ErrorKind};
use crate::net::connection;
use crate::session::{InputLine, MudLine};

#[derive(Debug, Clone)]
#[pyclass]
#[allow(clippy::struct_excessive_bools)] // TODO(XXX): Consider.
pub(crate) struct Buffer {
    #[pyo3(get, set)]
    pub(crate) name: String,

    #[pyo3(get, set)]
    pub(crate) line_wrap: bool,

    #[pyo3(get, set)]
    pub(crate) border_top: bool,

    #[pyo3(get, set)]
    pub(crate) border_bottom: bool,

    #[pyo3(get, set)]
    pub(crate) border_left: bool,

    #[pyo3(get, set)]
    pub(crate) border_right: bool,

    #[pyo3(get, set)]
    pub(crate) direction: BufferDirection,

    #[pyo3(get, set)]
    pub(crate) scrollbar: Scrollbar,

    #[pyo3(get)]
    pub(crate) scroll_pos: usize,

    #[pyo3(get)]
    pub(crate) max_scroll: usize,

    #[pyo3(get)]
    pub(crate) dimensions: (u16, u16),

    data: TrackedOutput,
}

impl Buffer {
    pub(crate) fn take_received(&mut self) -> &VecDeque<OutputItem> {
        self.data.take_received()
    }
}

#[pymethods]
impl Buffer {
    #[new]
    pub(crate) fn new(name: String) -> Result<Self, Error> {
        if name.is_empty() {
            return Err(ErrorKind::NameRequired.into());
        }
        Ok(Self {
            name,
            line_wrap: false,
            border_top: false,
            border_bottom: false,
            border_left: false,
            border_right: false,
            direction: BufferDirection::default(),
            scrollbar: Scrollbar::default(),
            scroll_pos: 0,
            max_scroll: 0,
            dimensions: (0, 0),
            data: TrackedOutput::default(),
        })
    }

    pub(crate) fn new_data(&self) -> usize {
        self.data.new_data
    }

    pub(crate) fn len(&self) -> usize {
        self.data.received.len()
    }

    pub(crate) fn add(&mut self, item: OutputItem) {
        self.data.add(item);
    }

    pub(crate) fn add_multiple(&mut self, items: Vec<OutputItem>) {
        self.data.add_multiple(items);
    }

    #[must_use]
    pub(crate) fn scroll(&self) -> usize {
        self.scroll_pos
    }

    pub(crate) fn scroll_up(&mut self, lines: u16) {
        trace!("scrolling up: scroll-pos: {}", self.scroll_pos);
        self.scroll_pos = self
            .scroll_pos
            .checked_add(lines as usize)
            .unwrap_or(self.scroll_pos);
        trace!("scrolling up: scroll-pos now {}", self.scroll_pos);
    }

    pub(crate) fn scroll_down(&mut self, lines: u16) {
        trace!("scrolling down: scroll-pos: {}", self.scroll_pos);
        self.scroll_pos = self.scroll_pos.saturating_sub(lines as usize);
        trace!("scrolling down: scroll-pos now {}", self.scroll_pos);
    }

    pub(crate) fn scroll_bottom(&mut self) {
        trace!("scrolling to bottom: scroll-pos: {}", self.scroll_pos);
        self.scroll_pos = 1;
        trace!("scrolling to bottom: scroll-pos now {}", self.scroll_pos);
    }

    pub(crate) fn scroll_to(&mut self, scroll: usize) {
        trace!(
            "scrolling to pos: scroll-pos {}: {}",
            scroll, self.scroll_pos
        );
        self.scroll_pos = scroll;
        trace!(
            "scrolling to pos: scroll-pos {} now: {}",
            scroll, self.scroll_pos
        );
    }

    pub(crate) fn scroll_max(&mut self) {
        trace!("scrolling to max: scroll-pos: {}", self.max_scroll);
        self.scroll_pos = self.max_scroll;
        trace!("scrolling to max: scroll-pos now: {}", self.scroll_pos);
    }

    fn __str__(&self) -> String {
        // TODO(XXX): nicer str format
        format!("{self:?}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
#[pyclass(eq, eq_int)]
pub enum BufferDirection {
    TopToBottom,
    #[default]
    BottomToTop,
}

#[pymethods]
impl BufferDirection {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        format!("{self}")
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for BufferDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BufferDirection::TopToBottom => write!(f, "top to bottom"),
            BufferDirection::BottomToTop => write!(f, "bottom to top"),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct TrackedOutput {
    new_data: usize,
    received: VecDeque<OutputItem>,
}

impl TrackedOutput {
    fn take_received(&mut self) -> &VecDeque<OutputItem> {
        // assume all new data will be read by the caller.
        self.new_data = 0;
        &self.received
    }

    fn add(&mut self, item: OutputItem) {
        self.new_data = self.new_data.saturating_add(1);
        self.received.push_back(item);
    }

    fn add_multiple(&mut self, items: Vec<OutputItem>) {
        let count = items.len();
        self.new_data = self.new_data.saturating_add(count);
        self.received.extend(items);
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

    /// A line of output that was detected as a prompt and should be displayed in a held
    /// position at the bottom of the output.
    // TODO(XXX): maybe better named LastPrompt, or folding into the existing Prompt item.
    HeldPrompt { prompt: String },

    /// An item of output related to the connection status changing.
    // TODO(XXX): revisit.
    ConnectionEvent {
        message: String,
        info: Option<connection::Info>,
    },

    /// A line of output produced as a result of executing a mudpuppy command.
    CommandResult { error: bool, message: String },

    /// A line of debug data
    Debug { line: String },

    /// A runtime error
    Error { message: String },
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Display)]
#[pyclass]
pub(crate) enum Scrollbar {
    #[default]
    IfScrolled,
    Never,
    Always,
}
