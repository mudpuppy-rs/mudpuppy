use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use pyo3::Python;

use crate::app::AppData;
use crate::error::Error;
use crate::session::OutputItem;

#[async_trait]
pub(super) trait SlashCommand: Debug + Send + Sync {
    fn name(&self) -> String;

    async fn execute(&self, app: &mut AppData, line: String) -> Result<(), Error>;
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

    async fn execute(&self, app: &mut AppData, _line: String) -> Result<(), Error> {
        app.should_quit = true;
        if let Some(active_session) = app.active_session_mut() {
            active_session.output.add(OutputItem::CommandResult {
                error: false,
                message: "Quitting...".to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Debug)]
struct NewSession;

#[async_trait]
impl SlashCommand for NewSession {
    fn name(&self) -> String {
        "new".to_string()
    }

    async fn execute(&self, app: &mut AppData, line: String) -> Result<(), Error> {
        let Some(character) = Python::with_gil(|py| {
            app.config()
                .borrow(py)
                .characters
                .iter()
                .find(|m| m.name == line)
                .cloned()
        }) else {
            return Err(Error::NoSuchMud(line));
        };

        let sesh = app.new_session(&character)?;
        app.set_active_session(Some(sesh.id))?;

        let active_sess = app.active_session_mut().unwrap();
        active_sess.output.add(OutputItem::CommandResult {
            error: false,
            message: format!("Creating session for {character}"),
        });
        Ok(())
    }
}

#[derive(Debug)]
struct Connect;

#[async_trait]
impl SlashCommand for Connect {
    fn name(&self) -> String {
        "connect".to_string()
    }

    async fn execute(&self, app: &mut AppData, _: String) -> Result<(), Error> {
        let Some(active) = app.active_session_py() else {
            return Err(Error::NoActiveSession);
        };

        Python::with_gil(|py| active.connect(py))?;
        Ok(())
    }
}

#[derive(Debug)]
struct Disconnect;

#[async_trait]
impl SlashCommand for Disconnect {
    fn name(&self) -> String {
        "disconnect".to_string()
    }

    async fn execute(&self, app: &mut AppData, _: String) -> Result<(), Error> {
        let Some(active) = app.active_session_py() else {
            return Err(Error::NoActiveSession);
        };

        Python::with_gil(|py| active.disconnect(py))?;
        Ok(())
    }
}

#[derive(Debug)]
struct Close;

#[async_trait]
impl SlashCommand for Close {
    fn name(&self) -> String {
        "close".to_string()
    }

    async fn execute(&self, app: &mut AppData, _: String) -> Result<(), Error> {
        let Some(active) = app.active_session_py() else {
            return Err(Error::NoActiveSession);
        };

        Python::with_gil(|py| active.close(py))?;
        Ok(())
    }
}

#[derive(Debug)]
struct Session;

impl Session {
    // TODO(XXX): replace with output creation.
    fn display(app: &AppData) -> Result<(), Error> {
        let sessions = app.sessions();
        if sessions.is_empty() {
            return Err(Error::NoActiveSession);
        }

        let active_id = app.active_session().map(|s| s.id);

        for sesh in sessions {
            let character = &sesh.character;
            let info = sesh.connected();
            let is_active = if Some(sesh.id) == active_id {
                "(*) "
            } else {
                ""
            };

            match info {
                None => {
                    println!(
                        "{is_active}session {}: {} - not connected",
                        sesh.id, character.name
                    );
                }
                Some(info) => {
                    println!(
                        "{is_active}session {}: {} - connected {}",
                        sesh.id, character.name, info
                    );
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl SlashCommand for Session {
    fn name(&self) -> String {
        "session".to_string()
    }

    async fn execute(&self, app: &mut AppData, line: String) -> Result<(), Error> {
        if line.is_empty() {
            return Session::display(app);
        }

        let Ok(sesh_id) = line.parse::<u32>() else {
            // TODO(XXX): better error type?
            return Err(Error::Internal(format!("invalid session ID: {line}")));
        };

        app.set_active_session(Some(sesh_id))
    }
}
