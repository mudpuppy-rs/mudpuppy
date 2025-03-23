use std::fmt::{Display, Formatter};

use async_trait::async_trait;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::types::PyAnyMethods;
use pyo3::{IntoPyObject, Py, PyObject, Python, pyclass, pymethods, pymodule};
use pyo3_async_runtimes::tokio::future_into_py;
use tokio::sync::oneshot;

use crate::app::{AppData, SlashCommand};
use crate::error::Error;
use crate::keyboard::KeyEvent;
use crate::session::{Alias, Character, EchoState, InputLine, PromptMode, Trigger};

use super::{
    APP, AliasCommand, Command, EventType, FutureResult, GmcpCommand, Handler, PromptCommand,
    Result, TelnetCommand, TriggerCommand, require_coroutine,
};

#[derive(Debug, Clone)]
#[pyclass(frozen)]
pub(crate) struct Session {
    #[pyo3(get)]
    pub(crate) id: u32,
    #[pyo3(get)]
    pub(crate) character: Character,
}

#[pymethods]
impl Session {
    pub(crate) fn connect<'py>(&'py self, py: Python<'py>) -> Result {
        dispatch_command(py, Command::Connect(self.id))
    }

    pub(crate) fn disconnect(&self, py: Python<'_>) -> Result {
        dispatch_command(py, Command::Disconnect(self.id))
    }

    pub(crate) fn close(&self, py: Python<'_>) -> Result {
        dispatch_command(py, Command::CloseSession(self.id))
    }

    pub(crate) fn connection_info<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| Command::ConnectionInfo {
            session: self.id,
            tx,
        })
    }

    pub(crate) fn set_active(&self, py: Python<'_>) -> Result {
        dispatch_command(py, Command::SetActiveSession(self.id))
    }

    // line is Union[str, InputLine]
    #[pyo3(signature = (line, skip_aliases = false))]
    #[allow(clippy::needless_pass_by_value)] // TODO(XXX): figure out line: &PyObject
    fn send_line(&self, py: Python<'_>, line: PyObject, skip_aliases: bool) -> Result {
        let line = if let Ok(s) = line.extract::<String>(py) {
            InputLine {
                sent: s,
                original: None,
                echo: EchoState::default(),
                scripted: true,
            }
        } else if let Ok(input) = line.extract::<InputLine>(py) {
            input
        } else {
            return Err(PyTypeError::new_err("line must be a str or InputLine").into());
        };
        dispatch_command(
            py,
            Command::SendLine {
                session: self.id,
                line,
                skip_aliases,
            },
        )
    }

    fn send_key(&self, py: Python<'_>, key: KeyEvent) -> Result {
        dispatch_command(
            py,
            Command::SendKey {
                session: self.id,
                key,
            },
        )
    }

    pub(crate) fn input<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| Command::Input {
            session: self.id,
            tx,
        })
    }

    fn add_event_handler(
        &self,
        py: Python<'_>,
        event_type: EventType,
        awaitable: PyObject,
    ) -> Result {
        dispatch_command(
            py,
            Command::AddEventHandler(Handler::new(py, event_type, Some(self.clone()), awaitable)?),
        )
    }

    fn prompt(&self) -> Prompt {
        self.into()
    }

    fn telnet(&self) -> Telnet {
        self.into()
    }

    fn gmcp(&self) -> Gmcp {
        self.into()
    }

    fn triggers(&self) -> Triggers {
        self.into()
    }

    fn aliases(&self) -> Aliases {
        self.into()
    }

    fn __str__(&self) -> String {
        format!("{}: {}", self.id, self.character)
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.id, self.character)
    }
}

#[derive(Debug, Clone)]
#[pyclass(frozen)]
pub struct Prompt {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Prompt {
    fn flush(&self, py: Python<'_>) -> Result<()> {
        dispatch_command(py, Command::Prompt(self.id, PromptCommand::Flush))
    }

    fn get<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| Command::Prompt(self.id, PromptCommand::Get(tx)))
    }

    fn set<'py>(&'py self, py: Python<'py>, prompt: String) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            Command::Prompt(self.id, PromptCommand::Set { prompt, tx })
        })
    }

    fn mode<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            Command::Prompt(self.id, PromptCommand::GetMode(tx))
        })
    }

    fn set_mode<'py>(&'py self, py: Python<'py>, mode: PromptMode) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            Command::Prompt(self.id, PromptCommand::SetMode { mode, tx })
        })
    }
}

impl From<&Session> for Prompt {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone)]
#[pyclass(frozen)]
pub struct Telnet {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Telnet {
    fn request_enable_option(&self, py: Python<'_>, option: u8) -> Result {
        dispatch_command(
            py,
            Command::Telnet(self.id, TelnetCommand::RequestEnableOption(option)),
        )
    }

    fn request_disable_option(&self, py: Python<'_>, option: u8) -> Result {
        dispatch_command(
            py,
            Command::Telnet(self.id, TelnetCommand::RequestDisableOption(option)),
        )
    }

    fn send_subnegotiation(&self, py: Python<'_>, option: u8, data: Vec<u8>) -> Result {
        dispatch_command(
            py,
            Command::Telnet(self.id, TelnetCommand::SendSubnegotiation(option, data)),
        )
    }
}

impl From<&Session> for Telnet {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone)]
#[pyclass(frozen)]
pub struct Gmcp {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Gmcp {
    fn register(&self, py: Python<'_>, module: String) -> Result {
        dispatch_command(py, Command::Gmcp(self.id, GmcpCommand::Register(module)))
    }

    fn unregister(&self, py: Python<'_>, module: String) -> Result {
        dispatch_command(py, Command::Gmcp(self.id, GmcpCommand::Unregister(module)))
    }

    fn send(&self, py: Python<'_>, package: String, json: String) -> Result {
        dispatch_command(
            py,
            Command::Gmcp(
                self.id,
                GmcpCommand::Send(package, serde_json::Value::String(json)),
            ),
        )
    }
}

impl From<&Session> for Gmcp {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone)]
#[pyclass(frozen)]
pub struct Triggers {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Triggers {
    fn add(&self, py: Python<'_>, trigger: Py<Trigger>) -> Result {
        dispatch_command(py, Command::Trigger(self.id, TriggerCommand::Add(trigger)))
    }

    fn remove(&self, py: Python<'_>, trigger: Py<Trigger>) -> Result {
        dispatch_command(
            py,
            Command::Trigger(self.id, TriggerCommand::Remove(trigger)),
        )
    }

    fn get<'py>(&self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| Command::Trigger(self.id, TriggerCommand::Get(tx)))
    }
}

impl From<&Session> for Triggers {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone)]
#[pyclass(frozen)]
pub struct Aliases {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Aliases {
    fn add(&self, py: Python<'_>, alias: Py<Alias>) -> Result {
        dispatch_command(py, Command::Alias(self.id, AliasCommand::Add(alias)))
    }

    fn remove(&self, py: Python<'_>, trigger: Py<Alias>) -> Result {
        dispatch_command(py, Command::Alias(self.id, AliasCommand::Remove(trigger)))
    }

    fn get<'py>(&self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| Command::Alias(self.id, AliasCommand::Get(tx)))
    }
}

impl From<&Session> for Aliases {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug)]
pub(crate) struct PySlashCommand {
    name: String,
    callback: PyObject,
}

impl PySlashCommand {
    pub(super) fn new(py: Python<'_>, name: String, callback: PyObject) -> Result<Self> {
        require_coroutine(py, "slash command", &callback)?;
        Ok(Self { name, callback })
    }
}

#[async_trait]
impl SlashCommand for PySlashCommand {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn execute(&self, app: &mut AppData, line: String) -> Result<()> {
        let current_session = app.active_session_py();

        Python::with_gil(|py| {
            let callback = self
                .callback
                .bind(py)
                .call1((line.clone(), current_session))?;
            Ok::<_, Error>(pyo3_async_runtimes::tokio::into_future(callback)?)
        })?
        .await?;

        Ok(())
    }
}

fn dispatch_async_command<T>(
    py: Python<'_>,
    cmd: impl FnOnce(oneshot::Sender<T>) -> Command,
) -> FutureResult
where
    T: for<'py> IntoPyObject<'py> + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    dispatch_command(py, cmd(tx))?;
    future_into_py(py, async move {
        rx.await
            .map_err(|err| PyRuntimeError::new_err(format!("error receiving result: {err}")))
    })
}

fn dispatch_command(py: Python<'_>, cmd: Command) -> Result {
    APP.get(py).unwrap().send(cmd).map_err(Into::into)
}

#[pymodule]
pub(crate) mod pup {
    use std::path::PathBuf;

    use pyo3::{PyObject, Python, pyfunction};

    use super::{
        Command, FutureResult, PySlashCommand, Result, dispatch_async_command, dispatch_command,
    };
    use crate::python::{Handler, Slash};

    #[pymodule_export]
    use super::{Gmcp, Prompt, Session, Telnet};
    #[pymodule_export]
    use crate::keyboard::KeyEvent;
    #[pymodule_export]
    use crate::python::{Event, EventType, GlobalEvent, GlobalEventType};
    #[pymodule_export]
    use crate::session::{
        Alias, Character, EchoState, InputLine, Markup, Mud, MudLine, PromptMode, PromptSignal,
        Tls, Trigger,
    };
    #[pymodule_export]
    use crate::tui::{Constraint, Direction, Section};

    #[pyfunction]
    fn config(py: Python<'_>) -> FutureResult {
        dispatch_async_command(py, Command::Config)
    }

    #[pyfunction]
    fn config_dir() -> PathBuf {
        crate::config::config_dir().to_owned()
    }

    #[pyfunction]
    fn data_dir() -> PathBuf {
        crate::config::data_dir().to_owned()
    }

    #[pyfunction]
    fn quit(py: Python<'_>) -> Result {
        dispatch_command(py, Command::Quit)
    }

    #[pyfunction]
    fn new_session(py: Python<'_>, character: Character) -> FutureResult {
        dispatch_async_command(py, |tx| Command::NewSession { character, tx })
    }

    #[pyfunction]
    fn active_session(py: Python<'_>) -> FutureResult {
        dispatch_async_command(py, Command::ActiveSession)
    }

    #[pyfunction]
    fn sessions(py: Python<'_>) -> FutureResult {
        dispatch_async_command(py, Command::Sessions)
    }

    #[pyfunction]
    fn session(py: Python<'_>, id: u32) -> FutureResult {
        dispatch_async_command(py, |tx| Command::Session(id, tx))
    }

    #[pyfunction]
    fn session_for_mud(py: Python<'_>, character: Character) -> FutureResult {
        dispatch_async_command(py, |tx| Command::SessionForCharacter(character, tx))
    }

    #[pyfunction]
    fn add_global_event_handler(
        py: Python<'_>,
        event_type: GlobalEventType,
        awaitable: PyObject,
    ) -> Result {
        dispatch_command(
            py,
            Command::AddGlobalEventHandler(Handler::new(py, event_type, None, awaitable)?),
        )
    }

    #[pyfunction]
    fn add_slash_command(py: Python<'_>, name: String, callback: PyObject) -> Result {
        dispatch_command(
            py,
            Command::Slash(Slash::Add(PySlashCommand::new(py, name, callback)?)),
        )
    }

    #[pyfunction]
    fn remove_slash_command(py: Python<'_>, name: String) -> Result {
        dispatch_command(py, Command::Slash(Slash::Remove(name)))
    }
}
