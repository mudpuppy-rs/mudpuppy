use std::collections::HashMap;
use std::sync::Arc;

use pyo3::{Py, Python};
use strum::Display;
use tokio::sync::oneshot;
use tracing::{Level, debug, instrument, warn};

use crate::app::{AppData, Frontend, SlashCommand, TabAction};
use crate::config::Config;
use crate::error::{Error, ErrorKind};
use crate::keyboard::KeyEvent;
use crate::net::connection;
use crate::python::api::Session;
use crate::python::{self, PySlashCommand, Result};
use crate::session::{Alias, Buffer, Character, Input, InputLine, OutputItem, PromptMode, Trigger};
use crate::shortcut::{Shortcut, TabShortcut};
use crate::tui;
use crate::tui::TabKind;

pub(crate) enum Command {
    Config(oneshot::Sender<Py<Config>>),
    Session(SessionCommand),
    AddNewSessionHandler(python::NewSessionHandler),
    GlobalShortcuts(oneshot::Sender<HashMap<KeyEvent, String>>),
    SetGlobalShortcut(KeyEvent, Shortcut),
    Tab(TabAction),
    Quit,
}

impl Command {
    #[instrument(level = Level::TRACE, skip(self, fe, app), fields(app.active_session))]
    pub(crate) fn exec(self, fe: &mut Frontend, app: &mut AppData) -> Result<bool> {
        match self {
            Command::Config(tx) => {
                let _ = tx.send(app.config());
            }
            Command::Session(cmd) => {
                cmd.exec(app)?;
            }
            Command::AddNewSessionHandler(handler) => {
                app.new_session_handlers.push(handler);
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
            Command::Tab(tab_action) => {
                fe.tab_action(app, tab_action)?;
            }
            Command::Quit => return Ok(true),
        }

        Ok(false)
    }
}

impl From<SessionCommand> for Command {
    fn from(cmd: SessionCommand) -> Self {
        Self::Session(cmd)
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

pub(crate) enum SessionCommand {
    ActiveSession(oneshot::Sender<Option<Session>>),
    Sessions(oneshot::Sender<Vec<Session>>),
    Session {
        session_id: u32,
        tx: oneshot::Sender<Option<Session>>,
    },
    SessionForCharacter(Character, oneshot::Sender<Option<Session>>),
    ConnectionInfo {
        session: u32,
        tx: oneshot::Sender<Option<connection::Info>>,
    },
    NewSession {
        character: Character,
        tx: oneshot::Sender<Session>,
    },
    AddEventHandler(python::Handler),
    CloseSession {
        session_id: u32,
    },
    SetActiveSession {
        session_id: u32,
    },
    Connect {
        session_id: u32,
    },
    Disconnect {
        session_id: u32,
    },
    SendLine {
        session_id: u32,
        line: InputLine,
        skip_aliases: bool,
    },
    SendKey {
        session_id: u32,
        key: KeyEvent,
    },
    Input {
        session_id: u32,
        tx: oneshot::Sender<Py<Input>>,
    },
    Slash {
        session_id: u32,
        cmd: Slash,
    },
    Output {
        session_id: Option<u32>,
        items: Vec<OutputItem>,
    },
    Prompt {
        session_id: u32,
        cmd: PromptCommand,
    },
    Telnet {
        session_id: u32,
        cmd: TelnetCommand,
    },
    Gmcp {
        session_id: u32,
        cmd: GmcpCommand,
    },
    Trigger {
        session_id: u32,
        cmd: TriggerCommand,
    },
    Alias {
        session_id: u32,
        cmd: AliasCommand,
    },
}

#[allow(clippy::too_many_lines)] // It's fine ¯\_(ツ)_/¯
impl SessionCommand {
    pub(crate) fn exec(self, app: &mut AppData) -> Result<()> {
        match self {
            Self::ActiveSession(tx) => {
                let _ = tx.send(app.active_session_py());
            }
            Self::Sessions(tx) => {
                let _ = tx.send(app.sessions_py());
            }
            Self::Session { session_id, tx } => {
                let _ = tx.send(app.session(session_id).ok().map(Into::into));
            }
            Self::SessionForCharacter(character, tx) => {
                let _ = tx.send(
                    app.sessions_py()
                        .into_iter()
                        .find(|s| s.character == character),
                );
            }
            Self::NewSession { character, tx } => {
                let _ = tx.send(app.new_session(&character)?);
            }
            Self::CloseSession { session_id } => {
                let _ = app.session_mut(session_id)?.disconnect();
                app.close_session(session_id)?;
            }
            Self::SetActiveSession { session_id } => {
                app.set_active_session(Some(session_id))?;
            }
            Self::AddEventHandler(handler) => {
                app.session_mut(handler.session.id)?
                    .event_handlers
                    .add(handler);
            }
            Self::Connect { session_id } => {
                app.session_mut(session_id)?.connect()?;
            }
            Self::Disconnect { session_id } => {
                app.session_mut(session_id)?.disconnect()?;
            }
            Self::ConnectionInfo { session, tx } => {
                let _ = tx.send(app.session(session)?.connected());
            }
            Self::SendLine {
                session_id,
                line,
                skip_aliases,
            } => {
                app.session_mut(session_id)?.send_line(line, skip_aliases)?;
            }
            Self::SendKey { session_id, key } => {
                app.session_mut(session_id)?.key_event(&key);
            }
            Self::Input { session_id, tx } => {
                let input = Python::attach(|py| {
                    Ok::<_, Error>(app.session(session_id)?.input.clone_ref(py))
                })?;
                let _ = tx.send(input);
            }
            Self::Slash { session_id, cmd } => {
                cmd.exec(session_id, app)?;
            }
            Self::Prompt { session_id, cmd } => {
                cmd.exec(app, session_id)?;
            }
            Self::Telnet { session_id, cmd } => {
                cmd.exec(app, session_id)?;
            }
            Self::Gmcp { session_id, cmd } => {
                cmd.exec(app, session_id)?;
            }
            Self::Trigger { session_id, cmd } => {
                cmd.exec(app, session_id)?;
            }
            Self::Alias { session_id, cmd } => {
                cmd.exec(app, session_id)?;
            }
            Self::Output { session_id, items } => {
                let session = match session_id {
                    None => app.active_session_mut(),
                    Some(id) => Some(app.session_mut(id)?),
                };
                if let Some(session) = session {
                    session.output.add_multiple(items);
                } else {
                    debug!("No active session to output to");
                }
            }
        }
        Ok(())
    }
}

pub(crate) enum Slash {
    Add(PySlashCommand),
    Remove(String),
    Exists(String, oneshot::Sender<bool>),
}

impl Slash {
    fn exec(self, session: u32, app: &mut AppData) -> Result {
        let slash_commands = &mut app.session_mut(session)?.slash_commands;
        match self {
            Slash::Add(cmd) => {
                slash_commands.insert(cmd.name(), Arc::new(cmd));
            }
            Slash::Remove(name) => {
                slash_commands.retain(|c, _| *c != name);
            }
            Slash::Exists(name, tx) => {
                let _ = tx.send(slash_commands.contains_key(&name));
            }
        }
        Ok(())
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
                session.unregister_gmcp_package(module)?;
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
                Python::attach(|py| {
                    let trigger = trigger.borrow(py);
                    triggers.retain(|t| t.borrow(py).name != trigger.name.as_str());
                });
            }
            TriggerCommand::Get(tx) => {
                let triggers = Python::attach(|_| session.triggers.clone());
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
                Python::attach(|py| {
                    let alias = alias.borrow(py);
                    debug!("Adding alias: {:?}", alias);
                });
                session.aliases.push(alias);
            }
            AliasCommand::Remove(alias) => {
                let aliases = &mut session.aliases;
                Python::attach(|py| {
                    let alias = alias.borrow(py);
                    aliases.retain(|t| t.borrow(py).name != alias.name.as_str());
                });
            }
            AliasCommand::Get(tx) => {
                let aliases = Python::attach(|_| session.aliases.clone());
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
    pub(crate) fn exec(self, tui: &mut tui::Tui, app: &mut AppData, tab_id: u32) -> Result<()> {
        let tab = tui.chrome.get_tab_mut(tab_id)?;

        let buffers = match &mut tab.kind {
            TabKind::Menu(_) => {
                warn!("cannot manipulate buffers for character menu tab");
                return Ok(());
            }
            TabKind::Session(character) => &mut app.session_mut(character.sesh.id)?.extra_buffers,
            TabKind::Custom(custom) => &mut custom.buffers,
        };

        match self {
            BufferCommand::Add(buffer) => {
                let name = Python::attach(|py| buffer.borrow(py).name.clone());
                buffers.insert(name, buffer);
            }
            BufferCommand::Get { name, tx } => {
                let buffer = Python::attach(|py| {
                    let buffer = buffers
                        .get(&name)
                        .ok_or(ErrorKind::NoSuchBufferName(tab_id, name))?;
                    Ok::<_, Error>(buffer.clone_ref(py))
                })?;
                let _ = tx.send(buffer);
            }
            BufferCommand::GetAll(tx) => {
                let buffers = Python::attach(|py| {
                    buffers
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
