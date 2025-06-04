mod buffer;
mod char_menu;
mod chrome;
mod commandline;
mod layout;
mod reflow;
mod session;

use std::fmt::Debug;
use std::io::{IsTerminal, Stdout, stdout};
use std::num::NonZeroUsize;
use std::panic;

use crate::app::{AppData, TabAction};
use crate::config::{CRATE_NAME, Config};
use crate::error::Error;
use crate::session::OutputItem;
use crate::{cli, python};
pub(super) use char_menu::CharacterMenu;
pub(super) use chrome::{Chrome, Tab};
use crossterm::ExecutableCommand;
use crossterm::event::{
    Event as CrosstermEvent, EventStream as CrosstermEventStream, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
use futures::{FutureExt, StreamExt};
pub(super) use layout::{Constraint, Direction, Section};
use pyo3::Python;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Layout;
pub(super) use session::Character;
use tokio::select;
use tokio::time::{Interval, MissedTickBehavior, interval};
use tracing::{debug, error, info, trace, warn};

#[derive(Debug)]
pub(super) struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    draw_interval: Interval,
    event_stream: CrosstermEventStream,
    pub(super) chrome: Chrome,
}

impl Tui {
    pub(super) fn new(args: &cli::Args, config: &Config) -> Result<Self, Error> {
        let terminal = init_tui_terminal(config.mouse_enabled)?;
        let mut draw_interval = interval(args.frame_rate_duration()?);
        trace!(draw_interval=?draw_interval.period(), "configuring TUI frame rate");
        draw_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        Ok(Self {
            terminal,
            draw_interval,
            event_stream: CrosstermEventStream::new(),
            chrome: Chrome::new(config),
        })
    }

    pub(super) async fn run(&mut self, app: &mut AppData) -> Result<(), Error> {
        select! {
            // TUI drawing.
            _ = self.draw_interval.tick() => {
                self.terminal.draw(|f|self.chrome.render(app, f).unwrap())?;
                Ok(())
            }
            // Terminal event.
            Some(Ok(event)) = self.event_stream.next().fuse() => {
                let Some(tab_action) = self.crossterm_event(app, &event).await? else {
                    return Ok(())
                };
                self.handle_tab_action(app, tab_action)
            }
        }
    }

    pub(super) fn exit(&mut self) {
        trace!("restoring terminal settings");
        self.terminal
            .backend_mut()
            .execute(crossterm::event::DisableMouseCapture)
            .unwrap();
        ratatui::restore();
    }

    pub(super) fn config_reloaded(&mut self, config: &Config) -> Result<(), Error> {
        self.chrome.config_reloaded(config)
    }

    #[allow(clippy::too_many_lines)] // TODO(XXX): pull out some helpers.
    async fn crossterm_event(
        &mut self,
        app: &mut AppData,
        event: &CrosstermEvent,
    ) -> Result<Option<TabAction>, Error> {
        trace!(event=?event);

        match event {
            CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::NONE,
                ..
            }) if app.active_session.is_some() => {
                // First, with an immutabe ref, check if the pending input is a cmd.
                let is_cmd = Python::with_gil(|py| {
                    // Safety: guard condition in match.
                    let session = app.active_session().unwrap();
                    session.input.borrow(py).value().sent.starts_with('/')
                });
                if is_cmd {
                    let input = Python::with_gil(|py| {
                        let session = app.active_session_mut().unwrap();
                        // Safety: we know it must be Some("/") at least.
                        session.input.borrow_mut(py).pop().unwrap()
                    });
                    // Safety: we know it starts with '/'.
                    let line = input.sent.strip_prefix('/').unwrap();
                    let mut parts = line.splitn(2, ' ');
                    let cmd_name = parts.next().unwrap_or_default();
                    let remaining = parts.next().unwrap_or_default();

                    let Some(cmd) = app.slash_commands.get(cmd_name).cloned() else {
                        let message = format!("unknown slash command: {cmd_name}");
                        let session = app.active_session_mut().unwrap();
                        warn!(message);
                        session.output.add(OutputItem::CommandResult {
                            error: true,
                            message,
                        });
                        return Ok(None);
                    };

                    debug!("executing slash command: {cmd_name} {remaining}");
                    match cmd.execute(app, remaining.to_string()).await {
                        Ok(Some(tab_action)) => {
                            self.handle_tab_action(app, tab_action)?;
                        }
                        Err(e) => {
                            let message = format!("error executing slash command {cmd_name}: {e}");
                            let session = app.active_session_mut().unwrap();
                            error!(message);
                            session.output.add(OutputItem::CommandResult {
                                error: true,
                                message,
                            });
                        }
                        _ => {}
                    }

                    return Ok(None);
                }
            }
            CrosstermEvent::Key(
                KeyEvent {
                    code: KeyCode::Esc,
                    kind: KeyEventKind::Press,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Char('c'),
                    kind: KeyEventKind::Press,
                    modifiers: KeyModifiers::CONTROL,
                    ..
                },
            ) => {
                app.should_quit = true;
                return Ok(None);
            }
            CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Char('n'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::ALT,
                ..
            }) => {
                return Ok(Some(TabAction::MoveRight {
                    tab_id: self.chrome.active_tab_id(),
                }));
            }
            CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Char('p'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::ALT,
                ..
            }) => {
                return Ok(Some(TabAction::MoveLeft {
                    tab_id: self.chrome.active_tab_id(),
                }));
            }
            CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Char('n'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => return Ok(Some(TabAction::Next {})),
            CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Char('p'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => return Ok(Some(TabAction::Previous {})),
            CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Char('x'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => {
                return Ok(Some(TabAction::Close {
                    tab_id: None, // active tab.
                }));
            }
            _ => {}
        }

        self.chrome.active_tab().crossterm_event(app, event)
    }

    // TODO(XXX): consider getting rid of tabaction, figuring out whether NewSession justifies it.
    pub(crate) fn handle_tab_action(
        &mut self,
        app: &mut AppData,
        tab_action: TabAction,
    ) -> Result<(), Error> {
        match tab_action {
            TabAction::Create { session } => {
                info!(name = session.character.name, "creating session tab");
                self.chrome.new_tab(Character::new(session));
            }
            TabAction::Next {} => {
                info!("switching to next tab");
                self.chrome.next_tab();
                app.set_active_session(self.chrome.active_tab().session_id())?;
            }
            TabAction::Previous {} => {
                info!("switching to previous tab");
                self.chrome.previous_tab();
                app.set_active_session(self.chrome.active_tab().session_id())?;
            }
            TabAction::Close { tab_id } => {
                let id = match tab_id {
                    None => {
                        info!("closing active tab");
                        self.chrome.active_tab_id()
                    }
                    Some(tab_id) => {
                        info!(tab_id, "closing specific tab");
                        tab_id
                    }
                };
                let (_, Some(removed)) = self.chrome.close_tab(id) else {
                    app.should_quit = true;
                    return Ok(());
                };
                if let Some(session) = removed.session_id() {
                    info!(session, "closing session");
                    app.close_session(session)?;
                }
                app.set_active_session(self.chrome.active_tab().session_id())?;
            }
            TabAction::SwitchToSession { session: Some(id) } => {
                info!(id, "switching to session tab");
                self.chrome.switch_to_session(id)?;
            }
            TabAction::SwitchToSession { session: None } => {
                info!("switching to character list");
                self.chrome.switch_to_list();
            }
            TabAction::SwitchToTab { tab_id } => {
                info!(tab_id, "switching to tab");
                if let Err(err) = self.chrome.switch_to(tab_id) {
                    warn!(?err, "failed to switch to tab");
                }
            }
            TabAction::Layout { tab_id, tx } => {
                let section = self.chrome.get_tab(tab_id)?.layout();
                let _ = tx.send(section);
            }
            TabAction::Title { tab_id, tx } => {
                _ = tx.send(self.chrome.get_tab(tab_id)?.title(app));
            }
            TabAction::SetTitle { tab_id, title } => {
                self.chrome
                    .get_tab_mut(tab_id)?
                    .set_title(app, title.as_str())?;
            }
            TabAction::MoveLeft { tab_id } => {
                self.chrome.move_tab_left(tab_id)?;
            }
            TabAction::MoveRight { tab_id } => {
                self.chrome.move_tab_right(tab_id)?;
            }
            TabAction::IdForSession { session_id, tx } => {
                let tab_info = self
                    .chrome
                    .tab_for_session(session_id)
                    .ok_or(Error::NoSuchSession(session_id))?;
                let _ = tx.send(python::Tab { id: tab_info.id });
            }
            TabAction::AllTabs { tx } => {
                let tabs = self
                    .chrome
                    .tabs()
                    .iter()
                    .map(|tab_info| python::Tab { id: tab_info.id })
                    .collect();
                let _ = tx.send(tabs);
            }
        }
        Ok(())
    }
}

fn init_tui_terminal(mouse_enabled: bool) -> Result<Terminal<CrosstermBackend<Stdout>>, Error> {
    let mut out = stdout();

    if !IsTerminal::is_terminal(&out) {
        let msg = format!(
            "{CRATE_NAME} without --headless is a TUI application that can only be run when STDOUT is a regular terminal."
        );
        error!("{msg}");
        return Err(Error::Cli(msg));
    }

    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if mouse_enabled {
            stdout()
                .execute(crossterm::event::DisableMouseCapture)
                .unwrap();
        }
        ratatui::restore();
        hook(info);
    }));

    enable_raw_mode()?;

    out.execute(EnterAlternateScreen)?;

    if mouse_enabled {
        debug!("enabling mouse capture");
        out.execute(crossterm::event::EnableMouseCapture)?;
    }

    // TODO(XXX): should support bracketed paste here w/ EnableBracketedPaste.

    // increase the cache size to avoid flickering for indeterminate layouts
    Layout::init_cache(NonZeroUsize::new(100).unwrap());
    Terminal::new(CrosstermBackend::new(out)).map_err(Into::into)
}
