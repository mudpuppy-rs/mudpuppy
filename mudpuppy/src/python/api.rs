use std::fmt::{Display, Formatter};

use super::{
    APP, AliasCommand, BufferCommand, Command, EventType, FutureResult, GmcpCommand, Handler,
    PromptCommand, Result, SessionCommand, Slash, TelnetCommand, TriggerCommand, require_coroutine,
};
use crate::app::{AppData, SlashCommand, TabAction};
use crate::error::{Error, ErrorKind};
use crate::keyboard::KeyEvent;
use crate::session::{Alias, Buffer, EchoState, InputLine, OutputItem, PromptMode, Trigger};
use crate::shortcut::{PythonShortcut, Shortcut, TabShortcut};
use async_trait::async_trait;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::types::{PyAnyMethods, PyList, PyListMethods};
use pyo3::{Bound, IntoPyObject, Py, PyAny, Python, pyclass, pymethods, pymodule};
use pyo3_async_runtimes::tokio::future_into_py;
use tokio::sync::oneshot;
use tracing::{error, trace};

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[pyclass(frozen, eq, hash)]
pub(crate) struct Session {
    #[pyo3(get)]
    pub(crate) id: u32,
    #[pyo3(get)]
    pub(crate) character: String,
}

impl From<Session> for u32 {
    fn from(sesh: Session) -> Self {
        sesh.id
    }
}

#[pymethods]
impl Session {
    pub(crate) fn connect<'py>(&'py self, py: Python<'py>) -> Result {
        dispatch_command(
            py,
            SessionCommand::Connect {
                session_id: self.id,
            },
        )
    }

    pub(crate) fn disconnect(&self, py: Python<'_>) -> Result {
        dispatch_command(
            py,
            SessionCommand::Disconnect {
                session_id: self.id,
            },
        )
    }

    pub(crate) fn close(&self, py: Python<'_>) -> Result {
        dispatch_command(
            py,
            SessionCommand::CloseSession {
                session_id: self.id,
            },
        )
    }

    pub(crate) fn character_config<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::CharacterInfo {
                character: self.character.clone(),
                tx,
            }
            .into()
        })
    }

    pub(crate) fn mud_config<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::MudInfo {
                character: self.character.clone(),
                tx,
            }
            .into()
        })
    }

    pub(crate) fn connection_info<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::ConnectionInfo {
                session: self.id,
                tx,
            }
            .into()
        })
    }

    pub(crate) fn set_active(&self, py: Python<'_>) -> Result {
        dispatch_command(
            py,
            SessionCommand::SetActiveSession {
                session_id: self.id,
            },
        )
    }

    // line is Union[str, InputLine]
    #[pyo3(signature = (line, skip_aliases = false))]
    #[allow(clippy::needless_pass_by_value)] // TODO(XXX): figure out line: &PyObject
    fn send_line(&self, py: Python<'_>, line: Py<PyAny>, skip_aliases: bool) -> Result {
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
            SessionCommand::SendLine {
                session_id: self.id,
                line,
                skip_aliases,
            },
        )
    }

    fn send_key(&self, py: Python<'_>, key: KeyEvent) -> Result {
        dispatch_command(
            py,
            SessionCommand::SendKey {
                session_id: self.id,
                key,
            },
        )
    }

    pub(crate) fn output(&self, py: Python<'_>, items: &Bound<'_, PyAny>) -> Result {
        trace!("output called with {items:?}");
        let output_items = match items.cast::<PyList>() {
            Ok(output_items) => output_items
                .iter()
                .map(|item| convert_to_output_item(&item))
                .collect::<Result<Vec<_>>>()?,
            Err(_) => vec![convert_to_output_item(items)?],
        };

        dispatch_command(
            py,
            SessionCommand::Output {
                session_id: Some(self.id),
                items: output_items,
            },
        )
    }

    pub(crate) fn input<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::Input {
                session_id: self.id,
                tx,
            }
            .into()
        })
    }

    fn add_event_handler(
        &self,
        py: Python<'_>,
        event_type: EventType,
        awaitable: Py<PyAny>,
    ) -> Result {
        dispatch_command(
            py,
            SessionCommand::AddEventHandler(Handler::new(py, event_type, self.clone(), awaitable)?),
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

    /// Get the per-session dialog manager.
    pub(crate) fn dialog_manager<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::DialogManager {
                session_id: self.id,
                tx,
            }
            .into()
        })
    }

    fn tab<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            Command::Tab(TabAction::TabForSession {
                session_id: Some(self.id),
                tx,
            })
        })
    }

    fn add_slash_command(&self, py: Python<'_>, name: String, callback: Py<PyAny>) -> Result {
        dispatch_command(
            py,
            SessionCommand::Slash {
                session_id: self.id,
                cmd: Slash::Add(PySlashCommand::new(py, name, callback)?),
            },
        )
    }

    fn slash_command_exists<'py>(&'py self, py: Python<'py>, name: String) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::Slash {
                session_id: self.id,
                cmd: Slash::Exists(name, tx),
            }
            .into()
        })
    }

    fn remove_slash_command<'py>(&'py self, py: Python<'py>, name: String) -> Result {
        dispatch_command(
            py,
            SessionCommand::Slash {
                session_id: self.id,
                cmd: Slash::Remove(name),
            },
        )
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

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[pyclass(frozen, eq, hash)]
pub struct Prompt {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Prompt {
    fn flush(&self, py: Python<'_>) -> Result<()> {
        dispatch_command(
            py,
            SessionCommand::Prompt {
                session_id: self.id,
                cmd: PromptCommand::Flush,
            },
        )
    }

    fn get<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::Prompt {
                session_id: self.id,
                cmd: PromptCommand::Get(tx),
            }
            .into()
        })
    }

    fn set<'py>(&'py self, py: Python<'py>, prompt: String) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::Prompt {
                session_id: self.id,
                cmd: PromptCommand::Set { prompt, tx },
            }
            .into()
        })
    }

    fn mode<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::Prompt {
                session_id: self.id,
                cmd: PromptCommand::GetMode(tx),
            }
            .into()
        })
    }

    fn set_mode<'py>(&'py self, py: Python<'py>, mode: PromptMode) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::Prompt {
                session_id: self.id,
                cmd: PromptCommand::SetMode { mode, tx },
            }
            .into()
        })
    }
}

impl From<&Session> for Prompt {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[pyclass(frozen, eq, hash)]
pub struct Telnet {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Telnet {
    fn request_enable_option(&self, py: Python<'_>, option: u8) -> Result {
        dispatch_command(
            py,
            SessionCommand::Telnet {
                session_id: self.id,
                cmd: TelnetCommand::RequestEnableOption(option),
            },
        )
    }

    fn request_disable_option(&self, py: Python<'_>, option: u8) -> Result {
        dispatch_command(
            py,
            SessionCommand::Telnet {
                session_id: self.id,
                cmd: TelnetCommand::RequestDisableOption(option),
            },
        )
    }

    fn send_subnegotiation(&self, py: Python<'_>, option: u8, data: Vec<u8>) -> Result {
        dispatch_command(
            py,
            SessionCommand::Telnet {
                session_id: self.id,
                cmd: TelnetCommand::SendSubnegotiation(option, data),
            },
        )
    }
}

impl From<&Session> for Telnet {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[pyclass(frozen, eq, hash)]
pub struct Gmcp {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Gmcp {
    fn register(&self, py: Python<'_>, module: String) -> Result {
        dispatch_command(
            py,
            SessionCommand::Gmcp {
                session_id: self.id,
                cmd: GmcpCommand::Register(module),
            },
        )
    }

    fn unregister(&self, py: Python<'_>, module: String) -> Result {
        dispatch_command(
            py,
            SessionCommand::Gmcp {
                session_id: self.id,
                cmd: GmcpCommand::Unregister(module),
            },
        )
    }

    fn send(&self, py: Python<'_>, package: String, json: String) -> Result {
        dispatch_command(
            py,
            SessionCommand::Gmcp {
                session_id: self.id,
                cmd: GmcpCommand::Send(package, serde_json::Value::String(json)),
            },
        )
    }
}

impl From<&Session> for Gmcp {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[pyclass(frozen, eq, hash)]
pub struct Triggers {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Triggers {
    fn add(&self, py: Python<'_>, trigger: Py<Trigger>) -> Result {
        dispatch_command(
            py,
            SessionCommand::Trigger {
                session_id: self.id,
                cmd: TriggerCommand::Add(trigger),
            },
        )
    }

    fn remove(&self, py: Python<'_>, trigger: Py<Trigger>) -> Result {
        dispatch_command(
            py,
            SessionCommand::Trigger {
                session_id: self.id,
                cmd: TriggerCommand::Remove(trigger),
            },
        )
    }

    fn get<'py>(&self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::Trigger {
                session_id: self.id,
                cmd: TriggerCommand::Get(tx),
            }
            .into()
        })
    }
}

impl From<&Session> for Triggers {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[pyclass(frozen, eq, hash)]
pub struct Aliases {
    #[pyo3(get)]
    pub id: u32,
}

#[pymethods]
impl Aliases {
    fn add(&self, py: Python<'_>, alias: Py<Alias>) -> Result {
        dispatch_command(
            py,
            SessionCommand::Alias {
                session_id: self.id,
                cmd: AliasCommand::Add(alias),
            },
        )
    }

    fn remove(&self, py: Python<'_>, trigger: Py<Alias>) -> Result {
        dispatch_command(
            py,
            SessionCommand::Alias {
                session_id: self.id,
                cmd: AliasCommand::Remove(trigger),
            },
        )
    }

    fn get<'py>(&self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            SessionCommand::Alias {
                session_id: self.id,
                cmd: AliasCommand::Get(tx),
            }
            .into()
        })
    }
}

impl From<&Session> for Aliases {
    fn from(sesh: &Session) -> Self {
        Self { id: sesh.id }
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[pyclass(frozen, eq, hash)]
pub(crate) struct Tab {
    #[pyo3(get)]
    pub(crate) id: u32,
}

#[pymethods]
impl Tab {
    fn set_active(&self, py: Python<'_>) -> Result {
        dispatch_command(py, TabShortcut::SwitchTo { tab_id: self.id })
    }

    fn layout<'py>(&self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            TabAction::Layout {
                tab_id: self.id,
                tx,
            }
            .into()
        })
    }

    fn title<'py>(&self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            TabAction::Title {
                tab_id: self.id,
                tx,
            }
            .into()
        })
    }

    fn shortcuts<'py>(&self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            TabAction::AllShortcuts {
                tab_id: Some(self.id),
                tx,
            }
            .into()
        })
    }

    #[expect(clippy::needless_pass_by_value)] // Making key_event & doesn't compile.
    fn set_shortcut(
        &self,
        py: Python<'_>,
        key_event: Py<PyAny>,
        shortcut: Option<Py<PyAny>>,
    ) -> Result {
        let key_event = if let Ok(ke) = key_event.extract::<KeyEvent>(py) {
            ke
        } else if let Ok(s) = key_event.extract::<String>(py) {
            KeyEvent::py_new(&s)?
        } else {
            return Err(PyTypeError::new_err("key_event must be a KeyEvent or str").into());
        };

        let shortcut = match shortcut {
            Some(s) => {
                if let Ok(sc) = s.extract::<Shortcut>(py) {
                    Some(sc)
                } else {
                    Some(Shortcut::Python(PythonShortcut::new(py, s)?))
                }
            }
            None => None,
        };

        dispatch_command(
            py,
            TabAction::SetShortcut {
                tab_id: Some(self.id),
                key_event,
                shortcut,
            },
        )
    }

    fn add_buffer(&self, py: Python<'_>, buff: Py<Buffer>) -> Result {
        dispatch_command(
            py,
            TabAction::Buffer {
                tab_id: self.id,
                cmd: BufferCommand::Add(buff),
            },
        )
    }

    fn get_buffer<'py>(&'py self, py: Python<'py>, name: String) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            TabAction::Buffer {
                tab_id: self.id,
                cmd: BufferCommand::Get { name, tx },
            }
            .into()
        })
    }

    fn get_buffers<'py>(&'py self, py: Python<'py>) -> FutureResult<'py> {
        dispatch_async_command(py, |tx| {
            TabAction::Buffer {
                tab_id: self.id,
                cmd: BufferCommand::GetAll(tx),
            }
            .into()
        })
    }

    fn set_title(&self, py: Python<'_>, title: String) -> Result {
        dispatch_command(
            py,
            TabAction::SetTitle {
                tab_id: Some(self.id),
                title,
            },
        )
    }

    #[allow(clippy::unused_self)]
    fn switch_next(&self, py: Python<'_>) -> Result {
        dispatch_command(py, TabShortcut::SwitchToNext {})
    }

    #[allow(clippy::unused_self)]
    fn switch_previous(&self, py: Python<'_>) -> Result {
        dispatch_command(py, TabShortcut::SwitchToPrevious {})
    }

    #[allow(clippy::unused_self)]
    fn switch_to_list(&self, py: Python<'_>) -> Result {
        dispatch_command(py, TabShortcut::SwitchToList {})
    }

    fn move_left(&self, py: Python<'_>) -> Result {
        dispatch_command(
            py,
            TabShortcut::MoveLeft {
                tab_id: Some(self.id),
            },
        )
    }

    fn move_right(&self, py: Python<'_>) -> Result {
        dispatch_command(
            py,
            TabShortcut::MoveRight {
                tab_id: Some(self.id),
            },
        )
    }

    fn close(&self, py: Python<'_>) -> Result {
        dispatch_command(
            py,
            TabShortcut::Close {
                tab_id: Some(self.id),
            },
        )
    }
}

#[derive(Debug)]
pub(crate) struct PySlashCommand {
    name: String,
    callback: Py<PyAny>,
}

impl PySlashCommand {
    pub(super) fn new(py: Python<'_>, name: String, callback: Py<PyAny>) -> Result<Self> {
        require_coroutine(py, "slash command", &callback)?;
        Ok(Self { name, callback })
    }
}

#[async_trait]
impl SlashCommand for PySlashCommand {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn execute(&self, app: &mut AppData, line: String) -> Result<Option<TabAction>> {
        let Some(current_session) = app.active_session_py() else {
            return Err(ErrorKind::NoActiveSession.into());
        };
        let session_id = current_session.id;

        let future = Python::attach(|py| {
            let callback = self
                .callback
                .bind(py)
                .call1((line.clone(), current_session))?;
            Ok::<_, Error>(pyo3_async_runtimes::tokio::into_future(callback)?)
        })?;

        let command_name = self.name.clone();
        tokio::spawn(async move {
            if let Err(err) = future.await {
                // Note: Error::from() to collect backtrace from PyErr.
                let message = format!(
                    "slash command '{command_name}' callback error: {}",
                    Error::from(err)
                );
                error!("{}", message);
                let _ = Python::attach(|py| {
                    dispatch_command(
                        py,
                        SessionCommand::Output {
                            session_id: Some(session_id),
                            items: message
                                .lines()
                                .map(|line| OutputItem::CommandResult {
                                    error: true,
                                    message: line.to_string(),
                                })
                                .collect(),
                        },
                    )
                });
            }
        });

        Ok(None)
    }
}

fn dispatch_async_command<T>(
    py: Python<'_>,
    cmd: impl FnOnce(oneshot::Sender<T>) -> Command,
) -> FutureResult<'_>
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

pub(crate) fn dispatch_command(py: Python<'_>, cmd: impl Into<Command>) -> Result {
    Ok(APP
        .get(py)
        .unwrap()
        .send(cmd.into())
        .map_err(ErrorKind::from)?)
}

fn convert_to_output_item(item: &Bound<'_, PyAny>) -> Result<OutputItem> {
    // Try to extract as OutputItem first
    if let Ok(output_item) = item.extract::<OutputItem>() {
        return Ok(output_item);
    }

    // If that fails, try to extract as string and convert to Debug
    if let Ok(text) = item.extract::<String>() {
        return Ok(OutputItem::Debug { line: text });
    }

    Err(PyTypeError::new_err("items must be OutputItem or str objects").into())
}

#[pymodule]
pub(crate) mod pup {
    use std::path::PathBuf;

    use pyo3::types::{PyAnyMethods, PyStringMethods, PyTuple};
    use pyo3::{Bound, Py, PyAny, Python, pyfunction};

    use super::{Command, FutureResult, Result, dispatch_async_command, dispatch_command};
    use crate::app::TabAction;
    use crate::error::ErrorKind;
    use crate::python::{ERROR_TX, NewSessionHandler, SessionCommand};

    #[pymodule_export]
    use super::{Gmcp, Prompt, Session, Tab, Telnet};
    #[pymodule_export]
    use crate::config::{Character, Config, Mud, Settings, SettingsOverlay, Tls};
    #[pymodule_export]
    use crate::keyboard::KeyEvent;
    #[pymodule_export]
    use crate::python::{Dimensions, Event, EventType};
    #[pymodule_export]
    use crate::session::{
        Alias, Buffer, BufferDirection, EchoState, Input, InputLine, Markup, MudLine, OutputItem,
        PromptMode, PromptSignal, Scrollbar, Timer, Trigger,
    };

    #[pymodule_export]
    use crate::shortcut::{InputShortcut, MenuShortcut, PythonShortcut, Shortcut, TabShortcut};
    #[pymodule_export]
    use crate::tui::{
        Constraint, Dialog, DialogKind, DialogManager, DialogPriority, Direction, Section,
    };
    #[pymodule_export]
    use crate::dialog::{ConfirmAction, FloatingWindow, Position, Severity, Size};

    #[pyfunction]
    fn config(py: Python<'_>) -> FutureResult<'_> {
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
    fn show_error(py: Python<'_>, message: String) -> Result {
        if let Some(error_tx) = ERROR_TX.get(py) {
            error_tx.send(message).map_err(|_| {
                crate::error::Error::from(ErrorKind::Internal(
                    "Failed to send error message".to_owned(),
                ))
            })?;
        }
        Ok(())
    }

    #[pyfunction]
    fn new_session(py: Python<'_>, character: String) -> FutureResult<'_> {
        dispatch_async_command(py, |tx| SessionCommand::NewSession { character, tx }.into())
    }

    #[pyfunction]
    fn active_session(py: Python<'_>) -> FutureResult<'_> {
        dispatch_async_command(py, |tx| SessionCommand::ActiveSession(tx).into())
    }

    #[pyfunction]
    fn sessions(py: Python<'_>) -> FutureResult<'_> {
        dispatch_async_command(py, |tx| SessionCommand::Sessions(tx).into())
    }

    #[pyfunction]
    fn session(py: Python<'_>, session_id: u32) -> FutureResult<'_> {
        dispatch_async_command(py, |tx| SessionCommand::Session { session_id, tx }.into())
    }

    #[pyfunction]
    fn session_for_character(py: Python<'_>, character: String) -> FutureResult<'_> {
        dispatch_async_command(py, |tx| {
            SessionCommand::SessionForCharacter { character, tx }.into()
        })
    }

    /// Get the global dialog manager.
    #[pyfunction]
    fn dialog_manager(py: Python<'_>) -> FutureResult<'_> {
        dispatch_async_command(py, Command::DialogManager)
    }

    #[pyfunction]
    fn new_session_handler(py: Python<'_>, awaitable: Py<PyAny>) -> Result {
        dispatch_command(
            py,
            Command::AddNewSessionHandler(NewSessionHandler::new(py, awaitable)?),
        )
    }

    #[pyfunction]
    fn tabs(py: Python<'_>) -> FutureResult<'_> {
        dispatch_async_command(py, |tx| Command::Tab(TabAction::AllTabs { tx }))
    }

    #[pyfunction]
    fn global_shortcuts(py: Python<'_>) -> FutureResult<'_> {
        dispatch_async_command(py, Command::GlobalShortcuts)
    }

    #[pyfunction]
    fn set_global_shortcut(py: Python<'_>, key_event: KeyEvent, shortcut: Shortcut) -> Result {
        dispatch_command(py, Command::SetGlobalShortcut(key_event, shortcut))
    }

    #[pyfunction]
    #[pyo3(signature = (title, *, layout = None, buffers = None))]
    fn create_tab(
        py: Python<'_>,
        title: String,
        layout: Option<Py<Section>>,
        buffers: Option<Vec<Py<Buffer>>>,
    ) -> FutureResult<'_> {
        dispatch_async_command(py, |tx| {
            Command::Tab(TabAction::CreateCustomTab {
                title,
                layout,
                buffers: buffers.unwrap_or_default(),
                tx,
            })
        })
    }

    #[pyfunction(signature = (*args, sep=None, end=None))]
    fn print<'py>(
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        sep: Option<&str>,
        end: Option<&str>,
    ) -> Result {
        let sep = sep.unwrap_or(" ");
        let end = end.unwrap_or("\n");
        let mut output = String::new();
        for (i, arg) in args.try_iter()?.enumerate() {
            if i > 0 {
                output.push_str(sep);
            }
            let arg_str = arg?.str()?;
            let arg_str = arg_str.to_str()?;
            output.push_str(arg_str);
        }
        output.push_str(end);

        // Then convert each line of the output into a debug item.
        dispatch_command(
            py,
            SessionCommand::Output {
                session_id: None,
                items: output
                    .lines()
                    .map(|i| OutputItem::Debug {
                        line: i.to_string(),
                    })
                    .collect(),
            },
        )?;

        Ok(())
    }
}
