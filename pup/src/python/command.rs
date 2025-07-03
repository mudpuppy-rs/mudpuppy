use std::collections::HashMap;
use std::sync::Arc;

use pyo3::{Py, Python};
use strum::Display;
use tokio::sync::oneshot;
use tracing::{Level, debug, instrument};

use crate::app::{AppData, SlashCommand, TabAction};
use crate::config::Config;
use crate::error::{Error, ErrorKind};
use crate::keyboard::KeyEvent;
use crate::net::connection;
use crate::python::api::Session;
use crate::python::{self, PySlashCommand, Result};
use crate::session::{Alias, Buffer, Character, Input, InputLine, OutputItem, PromptMode, Trigger};
use crate::shortcut::{Shortcut, TabShortcut};

pub(crate) enum Command {
    Config(oneshot::Sender<Py<Config>>),
    ActiveSession(oneshot::Sender<Option<Session>>),
    Sessions(oneshot::Sender<Vec<Session>>),
    Session(u32, oneshot::Sender<Option<Session>>),
    SessionForCharacter(Character, oneshot::Sender<Option<Session>>),
    ConnectionInfo {
        session: u32,
        tx: oneshot::Sender<Option<connection::Info>>,
    },
    NewSession {
        character: Character,
        tx: oneshot::Sender<Session>,
    },
    CloseSession(u32),
    SetActiveSession(u32),
    Connect(u32),
    Disconnect(u32),
    SendLine {
        session: u32,
        line: InputLine,
        skip_aliases: bool,
    },
    SendKey {
        session: u32,
        key: KeyEvent,
    },
    Input {
        session: u32,
        tx: oneshot::Sender<Py<Input>>,
    },
    Slash(Slash),
    AddGlobalEventHandler(python::GlobalHandler),
    AddEventHandler(python::SessionHandler),
    GlobalShortcuts(oneshot::Sender<HashMap<KeyEvent, String>>),
    SetGlobalShortcut(KeyEvent, Shortcut),
    Output {
        session: Option<u32>,
        item: OutputItem,
    },
    Prompt(u32, PromptCommand),
    Telnet(u32, TelnetCommand),
    Gmcp(u32, GmcpCommand),
    Trigger(u32, TriggerCommand),
    Alias(u32, AliasCommand),
    Tab(TabAction),
    Buffer(u32, BufferCommand),
    Quit,
}

impl Command {
    #[instrument(level = Level::TRACE, skip(self, app), fields(app.active_session))]
    pub(crate) fn exec(self, app: &mut AppData) -> Result<bool> {
        match self {
            Command::Config(tx) => {
                let _ = tx.send(app.config());
            }
            Command::ActiveSession(tx) => {
                let _ = tx.send(app.active_session_py());
            }
            Command::Sessions(tx) => {
                let _ = tx.send(app.sessions_py());
            }
            Command::Session(id, tx) => {
                let _ = tx.send(app.session(id).ok().map(Into::into));
            }
            Command::SessionForCharacter(character, tx) => {
                let _ = tx.send(
                    app.sessions_py()
                        .into_iter()
                        .find(|s| s.character == character),
                );
            }
            Command::NewSession { character, tx } => {
                let _ = tx.send(app.new_session(&character)?);
            }
            Command::CloseSession(id) => {
                let session = app.session_mut(id)?;
                let _ = session.disconnect();
                app.close_session(id)?;
            }
            Command::SetActiveSession(session_id) => {
                app.set_active_session(Some(session_id))?;
            }
            Command::Connect(session) => {
                app.session_mut(session)?.connect()?;
            }
            Command::Disconnect(session) => {
                app.session_mut(session)?.disconnect()?;
            }
            Command::ConnectionInfo { session, tx } => {
                let _ = tx.send(app.session(session)?.connected());
            }
            Command::SendLine {
                session,
                line,
                skip_aliases,
            } => {
                app.session_mut(session)?.send_line(line, skip_aliases)?;
            }
            Command::SendKey { session, key } => {
                app.session_mut(session)?.key_event(&key);
            }
            Command::Input { session, tx } => {
                let input = Python::with_gil(|py| {
                    Ok::<_, Error>(app.session(session)?.input.clone_ref(py))
                })?;
                let _ = tx.send(input);
            }
            Command::Slash(cmd) => {
                cmd.exec(app);
            }
            Command::AddGlobalEventHandler(handler) => {
                app.global_event_handlers.add(handler);
            }
            Command::AddEventHandler(handler) => {
                let session_id =
                    handler
                        .session
                        .as_ref()
                        .map(|s| s.id)
                        .ok_or(ErrorKind::Internal(
                            "session handler missing session".to_string(),
                        ))?;
                app.session_mut(session_id)?.event_handlers.add(handler);
            }
            Command::GlobalShortcuts(tx) => {
                let _ = tx.send(
                    app.shortcuts
                        .iter()
                        .map(|(key_event, shortcut)| (*key_event, shortcut.to_string()))
                        .collect(),
                );
            }
            Command::SetGlobalShortcut(key_event, shortcut) => {
                app.shortcuts.insert(key_event, shortcut);
            }
            Command::Prompt(id, cmd) => {
                cmd.exec(app, id)?;
            }
            Command::Telnet(id, cmd) => {
                cmd.exec(app, id)?;
            }
            Command::Gmcp(id, cmd) => {
                cmd.exec(app, id)?;
            }
            Command::Trigger(id, cmd) => {
                cmd.exec(app, id)?;
            }
            Command::Alias(id, cmd) => {
                cmd.exec(app, id)?;
            }
            Command::Output { session, item } => {
                let session = match session {
                    None => app.active_session_mut(),
                    Some(id) => Some(app.session_mut(id)?),
                };
                if let Some(session) = session {
                    session.output.add(item);
                } else {
                    debug!("No active session to output to");
                }
            }
            Command::Buffer(id, cmd) => {
                cmd.exec(app, id)?;
            }
            Command::Tab(_) => {
                // Special case - handled at a higher level.
            }
            Command::Quit => return Ok(true),
        }

        Ok(false)
    }
}

impl From<TabAction> for Command {
    fn from(action: TabAction) -> Self {
        Self::Tab(action)
    }
}

impl From<TabShortcut> for Command {
    fn from(shortcut: TabShortcut) -> Self {
        Self::from(TabAction::from(shortcut))
    }
}

pub(crate) enum Slash {
    Add(PySlashCommand),
    Remove(String),
}

impl Slash {
    fn exec(self, app: &mut AppData) {
        match self {
            Slash::Add(cmd) => {
                app.slash_commands.insert(cmd.name(), Arc::new(cmd));
            }
            Slash::Remove(name) => {
                app.slash_commands.retain(|c, _| *c != name);
            }
        }
    }
}

pub(crate) enum PromptCommand {
    Flush,
    Get(oneshot::Sender<String>),
    Set {
        prompt: String,
        tx: oneshot::Sender<String>,
    },
    GetMode(oneshot::Sender<PromptMode>),
    SetMode {
        mode: PromptMode,
        tx: oneshot::Sender<PromptMode>,
    },
}

impl PromptCommand {
    fn exec(self, app: &mut AppData, id: u32) -> Result<()> {
        match self {
            PromptCommand::Flush => {
                app.session_mut(id)?.flush_prompt()?;
            }
            PromptCommand::Get(tx) => {
                let _ = tx.send(app.session(id)?.prompt.content().to_string());
            }
            PromptCommand::Set { prompt, tx } => {
                let _ = tx.send(app.session_mut(id)?.prompt.set_content(prompt)?);
            }
            PromptCommand::GetMode(tx) => {
                let _ = tx.send(app.session(id)?.prompt.mode().clone());
            }
            PromptCommand::SetMode { mode, tx } => {
                let _ = tx.send(app.session_mut(id)?.prompt.set_mode(mode)?);
            }
        }

        Ok(())
    }
}

pub(crate) enum TelnetCommand {
    RequestEnableOption(u8),
    RequestDisableOption(u8),
    SendSubnegotiation(u8, Vec<u8>),
}

impl TelnetCommand {
    fn exec(self, app: &mut AppData, id: u32) -> Result<()> {
        match self {
            TelnetCommand::RequestEnableOption(option) => {
                app.session_mut(id)?.request_enable_option(option)?;
            }
            TelnetCommand::RequestDisableOption(option) => {
                app.session_mut(id)?.request_disable_option(option)?;
            }
            TelnetCommand::SendSubnegotiation(option, data) => {
                app.session_mut(id)?.send_subnegotiation(option, data)?;
            }
        }

        Ok(())
    }
}

pub(crate) enum GmcpCommand {
    Register(String),
    Unregister(String),
    Send(String, serde_json::Value),
}

impl GmcpCommand {
    fn exec(&self, app: &mut AppData, id: u32) -> Result<()> {
        let session = app.session_mut(id)?;

        match self {
            GmcpCommand::Register(module) => {
                session.register_gmcp_package(module.clone())?;
            }
            GmcpCommand::Unregister(module) => {
                session.unregister_gmcp_package(module.clone())?;
            }
            GmcpCommand::Send(package, value) => {
                session.send_gmcp_message(package, value)?;
            }
        }

        Ok(())
    }
}

pub(crate) enum TriggerCommand {
    Add(Py<Trigger>),
    Remove(Py<Trigger>),
    Get(oneshot::Sender<Vec<Py<Trigger>>>),
}

impl TriggerCommand {
    fn exec(self, app: &mut AppData, id: u32) -> Result<()> {
        let session = app.session_mut(id)?;

        match self {
            TriggerCommand::Add(trigger) => {
                debug!("Adding trigger: {:?}", trigger);
                session.triggers.push(trigger);
            }
            TriggerCommand::Remove(trigger) => {
                let triggers = &mut session.triggers;
                Python::with_gil(|py| {
                    let trigger = trigger.borrow(py);
                    triggers.retain(|t| t.borrow(py).name != trigger.name.as_str());
                });
            }
            TriggerCommand::Get(tx) => {
                let triggers = Python::with_gil(|_| session.triggers.clone());
                let _ = tx.send(triggers);
            }
        }

        Ok(())
    }
}

pub(crate) enum AliasCommand {
    Add(Py<Alias>),
    Remove(Py<Alias>),
    Get(oneshot::Sender<Vec<Py<Alias>>>),
}

impl AliasCommand {
    fn exec(self, app: &mut AppData, id: u32) -> Result<()> {
        let session = app.session_mut(id)?;

        match self {
            AliasCommand::Add(alias) => {
                Python::with_gil(|py| {
                    let alias = alias.borrow(py);
                    debug!("Adding alias: {:?}", alias);
                });
                session.aliases.push(alias);
            }
            AliasCommand::Remove(alias) => {
                let aliases = &mut session.aliases;
                Python::with_gil(|py| {
                    let alias = alias.borrow(py);
                    aliases.retain(|t| t.borrow(py).name != alias.name.as_str());
                });
            }
            AliasCommand::Get(tx) => {
                let aliases = Python::with_gil(|_| session.aliases.clone());
                let _ = tx.send(aliases);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Display)]
pub(crate) enum BufferCommand {
    Add(Py<Buffer>),
    Get {
        name: String,
        tx: oneshot::Sender<Py<Buffer>>,
    },
    GetAll(oneshot::Sender<Vec<Py<Buffer>>>),
}

impl BufferCommand {
    fn exec(self, app: &mut AppData, id: u32) -> Result<()> {
        let session = app.session_mut(id)?;

        match self {
            BufferCommand::Add(buffer) => {
                let name = Python::with_gil(|py| {
                    let buff = buffer.borrow(py);
                    buff.name.clone()
                });
                session.extra_buffers.insert(name, buffer);
            }
            BufferCommand::Get { name, tx } => {
                let buffer = Python::with_gil(|py| {
                    let buffer = session
                        .extra_buffers
                        .get(&name)
                        .ok_or(ErrorKind::NoSuchBufferName(id, name))?;
                    Ok::<_, Error>(buffer.clone_ref(py))
                })?;
                let _ = tx.send(buffer);
            }
            BufferCommand::GetAll(tx) => {
                let buffers = Python::with_gil(|py| {
                    session
                        .extra_buffers
                        .values()
                        .map(|buff| buff.clone_ref(py))
                        .collect::<Vec<_>>()
                });
                let _ = tx.send(buffers);
            }
        }

        Ok(())
    }
}
