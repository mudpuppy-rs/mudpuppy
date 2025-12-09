use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use pyo3::Python;

use crate::app::{AppData, TabAction};
use crate::error::{Error, ErrorKind};
use crate::session::OutputItem;
use crate::shortcut::TabShortcut;

#[async_trait]
pub(super) trait SlashCommand: Debug + Send + Sync {
    fn name(&self) -> String;

    async fn execute(&self, app: &mut AppData, line: String) -> Result<Option<TabAction>, Error>;
}

pub(super) fn builtin() -> HashMap<String, Arc<dyn SlashCommand>> {
    let mut commands: HashMap<String, Arc<dyn SlashCommand>> = HashMap::new();

    let cmds = [
        Arc::new(QuitCommand) as Arc<dyn SlashCommand>,
        Arc::new(NewSession),
        Arc::new(Connect),
        Arc::new(Disconnect),
        Arc::new(Close),
        Arc::new(Session),
    ];

    for cmd in cmds {
        let name = cmd.name();
        assert!(
            !commands.contains_key(&name),
            "duplicate slash command: {name}"
        );
        commands.insert(name, cmd);
    }

    commands
}

#[derive(Debug)]
struct QuitCommand;

#[async_trait]
impl SlashCommand for QuitCommand {
    fn name(&self) -> String {
        "quit".to_string()
    }

    async fn execute(&self, app: &mut AppData, _line: String) -> Result<Option<TabAction>, Error> {
        app.should_quit = true;
        if let Some(active_session) = app.active_session_mut() {
            active_session.output.add(OutputItem::CommandResult {
                error: false,
                message: "Quitting...".to_string(),
            });
        }
        Ok(None)
    }
}

#[derive(Debug)]
struct NewSession;

#[async_trait]
impl SlashCommand for NewSession {
    fn name(&self) -> String {
        "new".to_string()
    }

    async fn execute(&self, app: &mut AppData, line: String) -> Result<Option<TabAction>, Error> {
        let session = app.new_session(line.clone())?;
        app.set_active_session(Some(session.id))?;

        let active_sess = app.active_session_mut().unwrap();
        active_sess.output.add(OutputItem::CommandResult {
            error: false,
            message: format!("created session {id} for {line}", id = session.id),
        });
        active_sess.connect()?;
        Ok(Some(TabAction::CreateSessionTab { session }))
    }
}

#[derive(Debug)]
struct Connect;

#[async_trait]
impl SlashCommand for Connect {
    fn name(&self) -> String {
        "connect".to_string()
    }

    async fn execute(&self, app: &mut AppData, _: String) -> Result<Option<TabAction>, Error> {
        let Some(active) = app.active_session_py() else {
            return Err(ErrorKind::NoActiveSession.into());
        };

        Python::attach(|py| active.connect(py))?;
        Ok(None)
    }
}

#[derive(Debug)]
struct Disconnect;

#[async_trait]
impl SlashCommand for Disconnect {
    fn name(&self) -> String {
        "disconnect".to_string()
    }

    async fn execute(&self, app: &mut AppData, _: String) -> Result<Option<TabAction>, Error> {
        let Some(active) = app.active_session_py() else {
            return Err(ErrorKind::NoActiveSession.into());
        };

        Python::attach(|py| active.disconnect(py))?;
        Ok(None)
    }
}

// TODO(XXX): make more general tab command: tab close, tab left, tab right, etc.
#[derive(Debug)]
struct Close;

#[async_trait]
impl SlashCommand for Close {
    fn name(&self) -> String {
        "close".to_string()
    }

    async fn execute(&self, app: &mut AppData, _: String) -> Result<Option<TabAction>, Error> {
        if app.active_session.is_none() {
            return Err(ErrorKind::NoActiveSession.into());
        }

        // TODO(XXX): parse an optional tab id argument to use below.
        Ok(Some(
            TabShortcut::Close {
                tab_id: None, // active tab
            }
            .into(),
        ))
    }
}

#[derive(Debug)]
struct Session;

impl Session {
    fn display(app: &mut AppData) -> Result<(), Error> {
        let sessions = app.sessions();
        if sessions.is_empty() {
            return Err(ErrorKind::NoActiveSession.into());
        }

        let active_id = app.active_session().map(|s| s.id);

        let mut lines = Vec::new();
        for sesh in sessions {
            let character = &sesh.character;
            let info = sesh.connected();
            let is_active = if Some(sesh.id) == active_id {
                "(*) "
            } else {
                ""
            };

            let message = match info {
                None => format!(
                    "{is_active}session {id}: {character} - not connected",
                    id = sesh.id,
                ),
                Some(info) => format!(
                    "{is_active}session {id}: {character} - connected {info}",
                    id = sesh.id,
                ),
            };
            lines.push(OutputItem::CommandResult {
                error: false,
                message,
            });
        }

        app.active_session_mut().unwrap().output.add_multiple(lines);

        Ok(())
    }
}

#[async_trait]
impl SlashCommand for Session {
    fn name(&self) -> String {
        "session".to_string()
    }

    async fn execute(&self, app: &mut AppData, line: String) -> Result<Option<TabAction>, Error> {
        if line.is_empty() {
            Session::display(app)?;
            return Ok(None);
        }

        let Ok(session) = line.parse::<u32>() else {
            // TODO(XXX): better error type?
            return Err(ErrorKind::Internal(format!("invalid session ID: {line}")).into());
        };

        app.set_active_session(Some(session))?;
        Ok(Some(TabShortcut::SwitchToSession { session }.into()))
    }
}
