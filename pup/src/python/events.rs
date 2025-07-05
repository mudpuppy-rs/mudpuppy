use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::marker::PhantomData;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use pyo3::types::PyAnyMethods;
use pyo3::{Py, PyObject, Python, pyclass, pymethods};

use strum::{Display, VariantArray};
use tracing::{error, trace};

use crate::config::Config;
use crate::error::{Error, ErrorKind};
use crate::net::connection;
use crate::python::{self, label_for_coroutine, require_coroutine};
use crate::session::{Input, InputLine, MudLine, PromptMode};

#[derive(Debug)]
pub(crate) struct NewSessionHandler {
    label: String,
    awaitable: PyObject,
}

impl NewSessionHandler {
    pub(super) fn new(py: Python<'_>, awaitable: PyObject) -> python::Result<Self> {
        require_coroutine(py, "NewSessionHandler", &awaitable)?;
        Ok(Self {
            label: label_for_coroutine(py, &awaitable).unwrap_or("unknown".to_string()),
            awaitable,
        })
    }

    pub(crate) fn execute(&self, sesh: python::Session) -> Result<(), Error> {
        let future = Python::with_gil(|py| {
            let awaitable = self.awaitable.bind(py).call1((sesh,))?;
            pyo3_async_runtimes::tokio::into_future(awaitable)
        })?;

        let label = self.label.clone();
        tokio::spawn(async move {
            if let Err(err) = future.await {
                // Note: Error::from() to collect backtrace from PyErr.
                error!(
                    "NewSessionHandler {label} callback error: {}",
                    Error::from(err)
                );
            }
            Ok::<_, Error>(())
        });

        Ok(())
    }
}

#[derive(Debug, Clone, Display)]
#[pyclass]
pub(crate) enum Event {
    #[strum(to_string = "config reloaded")]
    ConfigReloaded { config: Py<Config> },
    #[strum(to_string = "closed")]
    SessionClosed {},
    #[strum(to_string = "connecting")]
    SessionConnecting {},
    #[strum(to_string = "connected: {info}")]
    SessionConnected { info: connection::Info },
    #[strum(to_string = "disconnected")]
    SessionDisconnected {},
    #[strum(to_string = "active session changed from {changed_from:?} to {changed_to:?}")]
    ActiveSessionChanged {
        changed_from: Option<python::Session>,
        changed_to: Option<python::Session>,
    },
    #[strum(to_string = "enabled telnet option {option}")]
    TelnetOptionEnabled { option: u8 },
    #[strum(to_string = "disabled telnet option {option}")]
    TelnetOptionDisabled { option: u8 },
    #[strum(to_string = "received telnet IAC command {command}")]
    TelnetIacCommand { command: u8 },
    #[strum(to_string = "received telnet subnegotiation {option}")]
    TelnetSubnegotiation { option: u8, data: Vec<u8> },
    #[strum(to_string = "prompt changed from '{from}' to '{to}'")]
    PromptChanged { from: String, to: String },
    #[strum(to_string = "prompt mode changed from {from} to {to}")]
    PromptModeChanged { from: PromptMode, to: PromptMode },
    #[strum(to_string = "received line: {line}")]
    Line { line: MudLine },
    #[strum(to_string = "buffered input line changed:{line}")]
    InputChanged { line: InputLine, input: Input },
    #[strum(to_string = "sent line: {line}")]
    InputLine { line: InputLine },
    #[strum(to_string = "buffer {name} resized from {from} to {to}")]
    BufferResized {
        name: String,
        from: Dimensions,
        to: Dimensions,
    },
    #[strum(to_string = "now GMCP enabled")]
    GmcpEnabled {},
    #[strum(to_string = "no longer GMCP enabled")]
    GmcpDisabled {},
    #[strum(to_string = "received GMCP message for package {package}: {json}")]
    GmcpMessage { package: String, json: String },
}

#[pymethods]
impl Event {
    pub(crate) fn r#type(&self) -> EventType {
        match self {
            Event::ConfigReloaded { .. } => EventType::ConfigReloaded,
            Event::SessionClosed { .. } => EventType::SessionClosed,
            Event::SessionConnecting { .. } => EventType::SessionConnecting,
            Event::SessionConnected { .. } => EventType::SessionConnected,
            Event::SessionDisconnected { .. } => EventType::SessionDisconnected,
            Event::ActiveSessionChanged { .. } => EventType::ActiveSessionChanged,
            Event::TelnetOptionEnabled { .. } => EventType::TelnetOptionEnabled,
            Event::TelnetOptionDisabled { .. } => EventType::TelnetOptionDisabled,
            Event::TelnetIacCommand { .. } => EventType::TelnetIacCommand,
            Event::TelnetSubnegotiation { .. } => EventType::TelnetSubnegotiation,
            Event::PromptChanged { .. } => EventType::PromptChanged,
            Event::PromptModeChanged { .. } => EventType::PromptModeChanged,
            Event::Line { .. } => EventType::Line,
            Event::InputChanged { .. } => EventType::InputChanged,
            Event::InputLine { .. } => EventType::InputLine,
            Event::BufferResized { .. } => EventType::BufferResized,
            Event::GmcpEnabled { .. } => EventType::GmcpEnabled,
            Event::GmcpDisabled { .. } => EventType::GmcpDisabled,
            Event::GmcpMessage { .. } => EventType::GmcpMessage,
        }
    }

    fn __str__(&self) -> String {
        self.to_string()
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[pyclass]
pub(crate) struct Dimensions(pub(crate) u16, pub(crate) u16);

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)]
impl Dimensions {
    fn width(&self) -> u16 {
        self.0
    }

    fn height(&self) -> u16 {
        self.0
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    fn __str__(&self) -> String {
        self.to_string()
    }
}

impl Display for Dimensions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}×{}", self.0, self.1)
    }
}

impl From<(u16, u16)> for Dimensions {
    fn from(dims: (u16, u16)) -> Self {
        Self(dims.0, dims.1)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Display, VariantArray)]
#[pyclass(eq, eq_int)]
pub(crate) enum EventType {
    All,
    ConfigReloaded,
    SessionClosed,
    SessionConnecting,
    SessionConnected,
    SessionDisconnected,
    ActiveSessionChanged,
    TelnetOptionEnabled,
    TelnetOptionDisabled,
    TelnetIacCommand,
    TelnetSubnegotiation,
    PromptChanged,
    PromptModeChanged,
    Line,
    InputChanged,
    InputLine,
    BufferResized,
    GmcpEnabled,
    GmcpDisabled,
    GmcpMessage,
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)]
impl EventType {
    fn __str__(&self) -> String {
        self.to_string()
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    #[staticmethod]
    fn all() -> HashMap<String, EventType> {
        Self::VARIANTS
            .iter()
            .map(|typ| (typ.to_string(), *typ))
            .collect()
    }
}

#[derive(Debug)]
pub(crate) struct Handler<Event, EventType> {
    pub(crate) r#type: EventType,
    pub(crate) session: Option<python::Session>,
    pub(crate) awaitable: PyObject,
    _phantom: PhantomData<Event>,
}

impl<Event, EventType> Handler<Event, EventType>
where
    EventType: Display,
{
    pub(super) fn new(
        py: Python<'_>,
        r#type: EventType,
        session: Option<python::Session>,
        awaitable: PyObject,
    ) -> python::Result<Self> {
        require_coroutine(py, r#type.to_string(), &awaitable)?;
        Ok(Handler {
            r#type,
            session,
            awaitable,
            _phantom: PhantomData,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Handlers<Event, EventType>
where
    EventType: Eq + Hash + Clone,
{
    handlers: HashMap<EventType, Vec<Handler<Event, EventType>>>,
    _phantom: PhantomData<Event>,
}

impl<Event, EventType> Default for Handlers<Event, EventType>
where
    EventType: Eq + Hash + Clone,
{
    fn default() -> Self {
        Handlers {
            handlers: HashMap::new(),
            _phantom: PhantomData,
        }
    }
}

impl<Event, EventType> Handlers<Event, EventType>
where
    Event: Clone,
    EventType: AllType + Display + Eq + Hash + Clone + 'static,
{
    pub(super) fn add(&mut self, handler: Handler<Event, EventType>) {
        self.handlers
            .entry(handler.r#type.clone())
            .or_default()
            .push(handler);
    }

    pub(crate) fn emit<F>(&self, event_type: &EventType, event: &Event, invoke: F) -> python::Result
    where
        F: Fn(&Handler<Event, EventType>, &Event) -> python::Result<python::PyFuture>,
    {
        let mut futures = FuturesUnordered::new();

        if let Some(type_handlers) = self.handlers.get(event_type) {
            for handler in type_handlers {
                futures.push(invoke(handler, event)?);
            }
        }

        if let Some(all_handlers) = self.handlers.get(EventType::all()) {
            for handler in all_handlers {
                futures.push(invoke(handler, event)?);
            }
        }

        let event_type_name = event_type.to_string();
        tokio::spawn(async move {
            while let Some(result) = futures.next().await {
                if let Err(err) = result {
                    // Note: Error::from() to collect backtrace from PyErr.
                    error!(
                        "event type {event_type_name} callback error: {}",
                        Error::from(err)
                    );
                }
            }
        });

        Ok(())
    }
}

pub(crate) trait AllType {
    fn all() -> &'static Self;
}

impl AllType for EventType {
    fn all() -> &'static Self {
        &Self::All
    }
}

pub(crate) type SessionHandler = Handler<Event, EventType>;
pub(crate) type SessionHandlers = Handlers<Event, EventType>;

impl SessionHandlers {
    pub(crate) fn session_event(&self, session_id: u32, event: &Event) -> python::Result {
        if event.r#type() != EventType::Line && event.r#type() != EventType::InputChanged {
            trace!(session_id, event=?event);
        }
        self.emit(&event.r#type(), event, |handler, event| {
            let event = Python::with_gil(|_| event.clone());
            let session = handler.session.clone().ok_or(ErrorKind::Internal(
                "event handler missing session".to_string(),
            ))?;
            let future = Python::with_gil(|py| {
                let awaitable = handler.awaitable.bind(py).call1((session, event))?;
                pyo3_async_runtimes::tokio::into_future(awaitable)
            })?;
            Ok(Box::pin(future))
        })
    }
}
