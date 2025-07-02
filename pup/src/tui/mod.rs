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

use crate::app::{AppData, TabAction, TabShortcut};
use crate::config::{CRATE_NAME, Config};
use crate::error::{Error, ErrorKind};
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::session::OutputItem;
use crate::{cli, python};
pub(super) use char_menu::CharacterMenu;
pub(super) use chrome::{Chrome, Tab};
use crossterm::ExecutableCommand;
use crossterm::event::{
    Event as CrosstermEvent, EventStream as CrosstermEventStream, KeyCode as CrosstermKeyCode,
    KeyEvent as CrosstermKeyEvent, KeyModifiers as CrosstermKeyModifiers,
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
use crate::shortcut::Shortcut;

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
                self.terminal.draw(|f|self.chrome.render(app, f).unwrap()).map_err(ErrorKind::from)?;
                Ok(())
            }
            // Terminal event.
            Some(Ok(event)) = self.event_stream.next().fuse() => {
                let Some(tab_action) = self.crossterm_event(app, &event)? else {
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

    fn crossterm_event(
        &mut self,
        app: &mut AppData,
        event: &CrosstermEvent,
    ) -> Result<Option<TabAction>, Error> {
        // TODO(XXX): noisy logging per keystroke
        trace!(event=?event);

        // Key events are checked against shortcuts.
        if let CrosstermEvent::Key(key_event) = event {
            if let Ok(key_event) = KeyEvent::try_from(key_event) {
                // Try to execute the key event as a global shortcut first.
                if let Some(shortcut) = app.shortcuts.get(&key_event).cloned() {
                    trace!(shortcut = shortcut.to_string(), "global shortcut matched");
                    self.handle_shortcut(app, shortcut, key_event)?;
                    return Ok(None)
                }
            }
        }

        // If it didn't match a shortcut, forward the event to the active tab.
        self.chrome.active_tab().crossterm_event(app, event)
    }

    pub(crate) fn handle_shortcut(&mut self, app: &mut AppData, shortcut: Shortcut, event: KeyEvent) -> Result<(), Error> {
        match shortcut {
            Shortcut::Tab(tab_action) => self.handle_tab_action(app, tab_action.into()),
            Shortcut::Quit => {
                app.should_quit = true;
                Ok(())
            }
        }
    }

    pub(crate) async fn handle_input_send(
        &mut self,
        app: &mut AppData,
    ) -> Result<Option<TabAction>, Error> {
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
        Ok(match cmd.execute(app, remaining.to_string()).await {
            Ok(Some(tab_action)) => Some(tab_action),
            Err(e) => {
                let message = format!("error executing slash command {cmd_name}: {e}");
                let session = app.active_session_mut().unwrap();
                error!(message);
                session.output.add(OutputItem::CommandResult {
                    error: true,
                    message,
                });
                None
            }
            _ => None,
        })
    }

    pub(crate) fn handle_tab_action(
        &mut self,
        app: &mut AppData,
        tab_action: TabAction,
    ) -> Result<(), Error> {
        match tab_action {
                TabAction::Shortcut(tab_shortcut) => return self.handle_tab_shortcut(app, tab_shortcut),
                TabAction::Layout { tab_id, tx } => {
                    let section = self.chrome.get_tab(tab_id)?.layout();
                    let _ = tx.send(section);
                }
                TabAction::Title { tab_id, tx } => {
                    _ = tx.send(self.chrome.get_tab(tab_id)?.title(app));
                }
                TabAction::SetTitle { tab_id, title } => {
                    let tab_id = tab_id.unwrap_or(self.chrome.active_tab_id());
                    self.chrome
                        .get_tab_mut(tab_id)?
                        .set_title(app, title.as_str())?;
                }

                TabAction::TabForSession { session_id, tx } => {
                    let session_id = session_id.unwrap_or(app.active_session.ok_or(ErrorKind::NoActiveSession)?);
                    let tab_info = self
                        .chrome
                        .tab_for_session(session_id)
                        .ok_or(ErrorKind::NoSuchSession(session_id))?;
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

    pub(crate) fn handle_tab_shortcut(
        &mut self,
        app: &mut AppData,
        tab_shortcut: TabShortcut,
    ) -> Result<(), Error> {
        match tab_shortcut {
            TabShortcut::Create { session } => {
                info!(name = session.character.name, "creating session tab");
                self.chrome.new_tab(Character::new(session));
            }
            TabShortcut::SwitchToNext => {
                info!("switching to next tab");
                self.chrome.next_tab();
                app.set_active_session(self.chrome.active_tab().session_id())?;
            }
            TabShortcut::SwitchToPrevious => {
                info!("switching to previous tab");
                self.chrome.previous_tab();
                app.set_active_session(self.chrome.active_tab().session_id())?;
            }
            TabShortcut::Close { tab_id } => {
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
            TabShortcut::SwitchToSession { session } => {
                info!(session, "switching to session tab");
                self.chrome.switch_to_session(session)?;
            }
            TabShortcut::SwitchToList => {
                info!("switching to character list");
                self.chrome.switch_to_list();
            }
            TabShortcut::SwitchTo { tab_id } => {
                info!(tab_id, "switching to tab");
                if let Err(err) = self.chrome.switch_to(tab_id) {
                    warn!(?err, "failed to switch to tab");
                }
            }
            TabShortcut::ProcessInput { tab_id } => {
                /*
                let tab_id = tab_id.unwrap_or(self.chrome.active_tab_id());
                let Some(session_id) = self.chrome.get_tab(tab_id)?.session_id() else {
                    break;
                };
                let session = app.session(session_id)?;
                if Python::with_gil(|py| session.input.borrow(py).value().sent.starts_with('/'))
                {
                    self.handle_input_send(app).await?
                } else {
                    None
                }*/
            }
            TabShortcut::MoveLeft { tab_id } => {
                let tab_id = tab_id.unwrap_or(self.chrome.active_tab_id());
                self.chrome.move_tab_left(tab_id)?;
            }
            TabShortcut::MoveRight { tab_id } => {
                let tab_id = tab_id.unwrap_or(self.chrome.active_tab_id());
                self.chrome.move_tab_right(tab_id)?;
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
        return Err(ErrorKind::Cli(msg).into());
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

impl TryFrom<&CrosstermKeyEvent> for KeyEvent {
    type Error = &'static str;

    fn try_from(value: &CrosstermKeyEvent) -> Result<Self, Self::Error> {
        Ok(KeyEvent {
            code: match value.code {
                CrosstermKeyCode::Backspace => KeyCode::Backspace,
                CrosstermKeyCode::Enter => KeyCode::Enter,
                CrosstermKeyCode::Left => KeyCode::Left,
                CrosstermKeyCode::Right => KeyCode::Right,
                CrosstermKeyCode::Up => KeyCode::Up,
                CrosstermKeyCode::Down => KeyCode::Down,
                CrosstermKeyCode::Home => KeyCode::Home,
                CrosstermKeyCode::End => KeyCode::End,
                CrosstermKeyCode::PageUp => KeyCode::PageUp,
                CrosstermKeyCode::PageDown => KeyCode::PageDown,
                CrosstermKeyCode::Tab | CrosstermKeyCode::BackTab => KeyCode::Tab,
                CrosstermKeyCode::Delete => KeyCode::Delete,
                CrosstermKeyCode::Insert => KeyCode::Insert,
                CrosstermKeyCode::F(code) => KeyCode::F(code),
                CrosstermKeyCode::Char(char) => KeyCode::Char(char),
                CrosstermKeyCode::Null => return Err("Null key unsupported"),
                CrosstermKeyCode::Esc => KeyCode::Esc,
                CrosstermKeyCode::CapsLock => return Err("CapsLock key unsupported"),
                CrosstermKeyCode::ScrollLock => return Err("ScrollLock key unsupported"),
                CrosstermKeyCode::NumLock => return Err("NumLock key unsupported"),
                CrosstermKeyCode::PrintScreen => return Err("PrintScreen key unsupported"),
                CrosstermKeyCode::Pause => return Err("Pause key unsupported"),
                CrosstermKeyCode::Menu => return Err("Menu key unsupported"),
                CrosstermKeyCode::KeypadBegin => return Err("KeypadBegin key unsupported"),
                CrosstermKeyCode::Media(_) => return Err("Media key unsupported"),
                CrosstermKeyCode::Modifier(_) => return Err("Modifier key unsupported"),
            },
            modifiers: value.modifiers.into(),
        })
    }
}

impl From<CrosstermKeyModifiers> for KeyModifiers {
    fn from(value: CrosstermKeyModifiers) -> Self {
        KeyModifiers(value.bits())
    }
}
