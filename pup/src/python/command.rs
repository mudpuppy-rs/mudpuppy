use pyo3::{Py, Python};
use tokio::sync::oneshot;
use tracing::debug;

use crate::app::App;
use crate::config::Config;
use crate::error::Error;
use crate::keyboard::KeyEvent;
use crate::net::connection;
use crate::python::api::Session;
use crate::python::{self, GlobalEvent, Result};
use crate::session::{Mud, PromptMode, Trigger};

pub(crate) enum Command {
    Config(oneshot::Sender<Py<Config>>),
    ActiveSession(oneshot::Sender<Option<Session>>),
    Sessions(oneshot::Sender<Vec<Session>>),
    Session(u32, oneshot::Sender<Option<Session>>),
    SessionForMud(Mud, oneshot::Sender<Option<Session>>),
    ConnectionInfo {
        session: u32,
        tx: oneshot::Sender<Option<connection::Info>>,
    },
    NewSession {
        mud: Mud,
        tx: oneshot::Sender<Session>,
    },
    CloseSession(u32),
    SetActiveSession(u32),
    Connect(u32),
    Disconnect(u32),
    SendLine {
        session: u32,
        line: String,
    },
    SendKey {
        session: u32,
        key: KeyEvent,
    },
    AddGlobalEventHandler(python::GlobalHandler),
    AddEventHandler(python::SessionHandler),
    Prompt(u32, PromptCommand),
    Telnet(u32, TelnetCommand),
    Gmcp(u32, GmcpCommand),
    Trigger(u32, TriggerCommand),
    Quit,
}

impl Command {
    pub(crate) fn exec(self, app: &mut App) -> Result<bool> {
        match self {
            Command::Config(tx) => {
                let _ = tx.send(app.config());
            }
            Command::ActiveSession(tx) => {
                let _ = tx.send(app.active_session());
            }
            Command::Sessions(tx) => {
                let _ = tx.send(app.sessions());
            }
            Command::Session(id, tx) => {
                let _ = tx.send(app.session(id).ok().map(Into::into));
            }
            Command::SessionForMud(mud, tx) => {
                let _ = tx.send(app.sessions().into_iter().find(|s| s.mud == mud));
            }
            Command::NewSession { mud, tx } => {
                let new_sesh = app.new_session(&mud)?;
                let new_id = new_sesh.id;
                app.global_event_handlers
                    .global_event(&GlobalEvent::NewSession {
                        session: new_sesh.clone(),
                    })?;
                let _ = tx.send(new_sesh);
                if app.active_session().is_none() {
                    app.set_active_session(Some(new_id))?;
                }
            }
            Command::CloseSession(id) => {
                let session = app.session_mut(id)?;
                session.disconnect()?;
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
            Command::SendLine { session, line } => {
                app.session(session)?.send_line(&line)?;
            }
            Command::SendKey { session, key } => {
                app.session_mut(session)?.key_event(&key);
            }
            Command::AddGlobalEventHandler(handler) => {
                app.global_event_handlers.add(handler);
            }
            Command::AddEventHandler(handler) => {
                let session_id = handler
                    .session
                    .as_ref()
                    .map(|s| s.id)
                    .ok_or(Error::Internal(
                        "session handler missing session".to_string(),
                    ))?;
                app.session_mut(session_id)?.event_handlers.add(handler);
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
            Command::Quit => return Ok(true),
        }

        Ok(false)
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
    fn exec(self, app: &mut App, id: u32) -> Result<()> {
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
    fn exec(self, app: &mut App, id: u32) -> Result<()> {
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
    fn exec(&self, app: &mut App, id: u32) -> Result<()> {
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
    fn exec(self, app: &mut App, id: u32) -> Result<()> {
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
