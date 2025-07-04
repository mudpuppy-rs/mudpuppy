use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader, Lines, Stdin, stdin};
use tracing::{debug, error, trace, warn};

use crate::app::{AppData, SlashCommand};
use crate::error::{Error, ErrorKind};
use crate::session::{EchoState, InputLine};
use crate::slash_command;

#[derive(Debug)]
pub(super) struct Headless {
    stdin: Lines<BufReader<Stdin>>,
    slash_commands: HashMap<String, Arc<dyn SlashCommand>>,
}

impl Headless {
    pub(super) fn new() -> Self {
        trace!("configuring headless mode stdin reader");
        Self {
            stdin: BufReader::new(stdin()).lines(),
            slash_commands: slash_command::builtin(),
        }
    }

    pub(super) async fn run(&mut self, app: &mut AppData) -> Result<(), Error> {
        let Some(line) = self.stdin.next_line().await.map_err(ErrorKind::from)? else {
            return Ok(());
        };
        if let Err(e) = self.stdin(app, line).await {
            error!("stdin error: {e}");
            eprintln!("{e}");
        }
        Ok(())
    }

    async fn stdin(&mut self, app: &mut AppData, line: String) -> Result<(), Error> {
        if let Some(line) = line.strip_prefix('/') {
            let mut parts = line.splitn(2, ' ');
            let cmd_name = parts.next().unwrap_or_default();
            let remaining = parts.next().unwrap_or_default();

            let Some(cmd) = self.slash_commands.get(cmd_name).cloned() else {
                warn!("unknown slash command: {cmd_name}");
                eprintln!("unknown slash command: {cmd_name}");
                return Ok(());
            };

            debug!("executing slash command: {cmd_name} {remaining}");
            if let Some(action) = cmd.execute(app, remaining.to_string()).await? {
                warn!("ignoring tab action in headless mode: {action:?}");
                eprintln!("ignoring tab action in headless mode: {action:?}");
            }
        } else if let Some(active_session) = app.active_session {
            debug!("sending stdin line: {line}");
            app.session_mut(active_session)?.send_line(
                InputLine {
                    sent: line,
                    original: None,
                    echo: EchoState::default(),
                    scripted: false,
                },
                false,
            )?;
        } else {
            warn!("no active session to send line to");
            eprintln!("no active session to send line to");
        }

        Ok(())
    }
}
