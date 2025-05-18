use tokio::io::{AsyncBufReadExt, BufReader, Lines, Stdin, stdin};
use tracing::{debug, error, trace, warn};

use crate::app::AppData;
use crate::error::Error;
use crate::session::{EchoState, InputLine};

#[derive(Debug)]
pub(super) struct Headless {
    stdin: Lines<BufReader<Stdin>>,
}

impl Headless {
    pub(super) fn new() -> Self {
        trace!("configuring headless mode stdin reader");
        Self {
            stdin: BufReader::new(stdin()).lines(),
        }
    }

    pub(super) async fn run(&mut self, app: &mut AppData) -> Result<(), Error> {
        let Some(line) = self.stdin.next_line().await? else {
            return Ok(());
        };
        if let Err(e) = self.stdin(app, line).await {
            error!("stdin error: {e}");
        }
        Ok(())
    }

    async fn stdin(&mut self, app: &mut AppData, line: String) -> Result<(), Error> {
        if let Some(line) = line.strip_prefix('/') {
            let mut parts = line.splitn(2, ' ');
            let cmd_name = parts.next().unwrap_or_default();
            let remaining = parts.next().unwrap_or_default();

            let Some(cmd) = app.slash_commands.get(cmd_name).cloned() else {
                warn!("unknown slash command: {cmd_name}");
                return Ok(());
            };

            debug!("executing slash command: {cmd_name} {remaining}");
            cmd.execute(app, remaining.to_string()).await?;
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
        }

        Ok(())
    }
}
