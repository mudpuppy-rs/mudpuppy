use std::fmt::{Display, Formatter};

use pyo3::exceptions::PyRuntimeError;
use pyo3::{pyclass, pymethods, pymodule, IntoPyObject, Py, PyObject, Python};
use pyo3_async_runtimes::tokio::future_into_py;
use tokio::sync::oneshot;

use crate::session::{Mud, PromptMode, Trigger};

use super::{
    Command, EventType, FutureResult, GmcpCommand, Handler, PromptCommand, Result, TelnetCommand,
    TriggerCommand, APP,
};

#[derive(Debug, Clone)]
#[pyclass(frozen)]
pub(crate) struct Session {
    #[pyo3(get)]
    pub(crate) id: u32,
    #[pyo3(get)]
    pub(crate) mud: Mud,
}

#[pymethods]
impl Session {
    fn connect<'py>(&'py self, py: Python<'py>) -> Result {
        dispatch_command(py, Command::Connect(self.id))
    }

    fn disconnect(&self, py: Python<'_>) -> Result {
        dispatch_command(py, Command::Disconnect(self.id))
    }

    fn close(&self, py: Python<'_>) -> Result {
        dispatch_command(py, Command::CloseSession(self.id))
    }

    fn connection_info<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| Command::ConnectionInfo {
            session: self.id,
            tx,
        })
    }

    fn set_active(&self, py: Python<'_>) -> Result {
        dispatch_command(py, Command::SetActiveSession(self.id))
    }

    fn send_line(&self, py: Python<'_>, line: String) -> Result {
        dispatch_command(
            py,
            Command::SendLine {
                session: self.id,
                line,
            },
        )
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
        Prompt { id: self.id }
    }

    fn telnet(&self) -> Telnet {
        Telnet { id: self.id }
    }

    fn gmcp(&self) -> Gmcp {
        Gmcp { id: self.id }
    }

    fn triggers(&self) -> Triggers {
        Triggers { id: self.id }
    }

    fn __str__(&self) -> String {
        format!("{}: {}", self.id, self.mud)
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.id, self.mud)
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

    use pyo3::{pyfunction, PyObject, Python};

    use super::{dispatch_async_command, dispatch_command, Command, FutureResult, Result};
    use crate::python::Handler;

    #[pymodule_export]
    use super::{Gmcp, Prompt, Session, Telnet};
    #[pymodule_export]
    use crate::keyboard::KeyEvent;
    #[pymodule_export]
    use crate::python::{Event, EventType, GlobalEvent, GlobalEventType};
    #[pymodule_export]
    use crate::session::{
        EchoState, InputLine, Markup, Mud, MudLine, PromptMode, PromptSignal, Tls, Trigger,
    };

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
    fn new_session(py: Python<'_>, mud: Mud) -> FutureResult {
        dispatch_async_command(py, |tx| Command::NewSession { mud, tx })
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
}
