mod buffer;
mod char_menu;
mod chrome;
mod commandline;
mod custom_tab;
mod layout;
mod reflow;
mod session;

use crossterm::ExecutableCommand;
use crossterm::event::{
    Event as CrosstermEvent, EventStream as CrosstermEventStream, KeyCode as CrosstermKeyCode,
    KeyEvent as CrosstermKeyEvent, KeyEventKind, KeyModifiers as CrosstermKeyModifiers,
    MouseButton as CrosstermMouseButton, MouseEvent as CrosstermMouseEvent,
    MouseEventKind as CrosstermMouseEventKind,
};
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
use futures::{FutureExt, StreamExt};
use pyo3::{Py, Python};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Layout, Rect};
use std::fmt::Debug;
use std::io::{IsTerminal, Stdout, stdout};
use std::num::NonZeroUsize;
use std::panic;
use std::time::Duration;
use tokio::select;
use tokio::time::{Interval, MissedTickBehavior, interval};
use tracing::{debug, error, info, trace, warn};

use crate::app::{AppData, TabAction};
use crate::config::{CRATE_NAME, Config};
use crate::error::{Error, ErrorKind};
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::mouse::{MouseButton, MouseEvent, MouseEventKind};
use crate::shortcut::{Shortcut, TabShortcut};
use crate::{cli, dialog, python};

pub(super) use char_menu::CharacterMenu;
pub(super) use chrome::{Chrome, Tab, TabKind};
pub(super) use custom_tab::CustomTab;
pub(super) use dialog::{Dialog, DialogManager, DialogPriority};
pub(super) use layout::{Constraint, Direction, Section};
pub(super) use session::Character;

#[derive(Debug)]
pub(crate) struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    draw_interval: Interval,
    event_stream: CrosstermEventStream,
    pub(super) chrome: Chrome,
}

impl Tui {
    pub(super) fn new(args: &cli::Args, config: &Py<Config>) -> Result<Self, Error> {
        let mouse_enabled = Python::attach(|py| config.borrow(py).mouse_enabled);
        let terminal = init_tui_terminal(mouse_enabled)?;
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

    async fn crossterm_event(
        &mut self,
        app: &mut AppData,
        event: &CrosstermEvent,
    ) -> Result<Option<TabAction>, Error> {
        // Uncomment for VERY VERBOSE logging :)
        //trace!(event=?event);

        // TODO(XXX): Bracketed paste.
        // TODO(XXX): Focus gained/lost ?

        match event {
            CrosstermEvent::Key(key_event) => {
                // We don't do anything special with release/repeat.
                if key_event.kind != KeyEventKind::Press {
                    return Ok(None);
                }

                // Not all native crossterm key events can be translated to our Python-friendly
                // domain repr.
                let Ok(key_event) = KeyEvent::try_from(key_event) else {
                    return Ok(None);
                };

                // Check dialogs first - they have the highest priority
                // Dialogs must be checked before shortcuts so they can intercept keys like Esc
                if Tab::dialog_key_consumed(app, &key_event) {
                    return Ok(None);
                }

                // Handle app-level shortcuts, these aren't specific to the active tab and so we
                // either process them or discard them. They aren't forwarded to the active tab.
                // Note: taking the GIL to allow cloning the PyObject in PythonShortcut.
                let shortcut = Python::attach(|_| app.shortcuts.get(&key_event).cloned());
                if let Some(shortcut) = shortcut {
                    trace!(
                        key_event = key_event.to_string(),
                        shortcut = shortcut.to_string(),
                        "global shortcut matched"
                    );
                    return self
                        .process_shortcut(app, shortcut, &key_event, false)
                        .await;
                }

                // Handle tab-level shortcuts. These are forwarded to the active tab if we don't handle
                // them ourselves (e.g. quit, python shortcuts).
                let active_tab = self.chrome.active_tab_mut();
                let shortcut = active_tab.lookup_shortcut(&key_event);
                if let Some(shortcut) = shortcut {
                    trace!(
                        key_event = key_event.to_string(),
                        shortcut = shortcut.to_string(),
                        active_tab = active_tab.data.title,
                        "tab shortcut matched"
                    );
                    return self.process_shortcut(app, shortcut, &key_event, true).await;
                }

                // Otherwise, forward the crossterm event to the active tab.
                active_tab.key_event(app, &key_event)
            }
            CrosstermEvent::Mouse(mouse_event) => {
                // Translate crossterm mouse event to our domain type
                let mouse_event = MouseEvent::from(mouse_event);

                // TODO(XXX): sus.
                // Calculate the tab content area (excluding the 3-line tab bar)
                // This matches the layout in Chrome::render
                let size = self.terminal.size()?;
                let full_area = Rect {
                    x: 0,
                    y: 0,
                    width: size.width,
                    height: size.height,
                };
                let [_tab_bar, tab_content] = Layout::vertical([
                    ratatui::layout::Constraint::Length(3),
                    ratatui::layout::Constraint::Fill(0),
                ])
                .areas(full_area);

                Ok(Tab::mouse_event(app, mouse_event, tab_content))
            }
            _ => Ok(None),
        }
    }

    pub(crate) async fn process_shortcut(
        &mut self,
        app: &mut AppData,
        shortcut: Shortcut,
        key_event: &KeyEvent,
        forward_to_tab: bool,
    ) -> Result<Option<TabAction>, Error> {
        Ok(match shortcut {
            Shortcut::Quit {} => {
                let confirm_quit = Python::attach(|py| app.config.borrow(py).confirm_quit);
                if confirm_quit {
                    app.dialog_manager.show_confirmation(
                        "Are you sure you want to quit?".to_string(),
                        'q',
                        dialog::ConfirmAction::Quit {},
                        Some(Duration::from_secs(15)),
                    );
                } else {
                    app.should_quit = true;
                }
                None
            }
            Shortcut::Python(shortcut) => {
                let session = match &self.chrome.active_tab().kind {
                    TabKind::Session(char) => Some(char.sesh.clone()),
                    _ => None,
                };
                shortcut.execute(
                    python::Tab {
                        id: self.chrome.active_tab_id(),
                    },
                    session,
                    key_event,
                )?;
                None
            }
            Shortcut::Tab(tab_shortcut) => Some(tab_shortcut.into()),
            _ if forward_to_tab => {
                return self.chrome.active_tab_mut().shortcut(app, &shortcut).await;
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
            TabAction::Shortcut(tab_shortcut) => {
                return self.handle_tab_shortcut(app, &tab_shortcut);
            }
            TabAction::CreateCustomTab {
                title,
                layout,
                buffers,
                tx,
            } => {
                info!(title = title, "creating custom tab");
                let custom_tab =
                    Python::attach(|py| CustomTab::new_tab(py, title, layout, buffers))?;
                let tab_id = self.chrome.new_tab(custom_tab);
                let _ = tx.send(python::Tab { id: tab_id });
            }
            TabAction::CreateSessionTab { session } => {
                info!(name = session.character, "creating session tab");
                let config = Python::attach(|py| app.config.clone_ref(py));
                self.chrome.new_tab(Character::new_tab(session, config));
            }
            TabAction::Layout { tab_id, tx } => {
                let section = self.chrome.get_tab(tab_id)?.data.layout();
                let _ = tx.send(section);
            }
            TabAction::Title { tab_id, tx } => {
                _ = tx.send(self.chrome.get_tab(tab_id)?.data.title.clone());
            }
            TabAction::SetTitle { tab_id, title } => {
                let tab_id = tab_id.unwrap_or(self.chrome.active_tab_id());
                self.chrome.get_tab_mut(tab_id)?.data.title = title;
            }
            TabAction::AllShortcuts { tab_id, tx } => {
                let tab_id = tab_id.unwrap_or(self.chrome.active_tab_id());
                let _ = tx.send(self.chrome.get_tab(tab_id)?.all_shortcuts());
            }
            TabAction::SetShortcut {
                tab_id,
                key_event,
                shortcut,
            } => {
                let tab_id = tab_id.unwrap_or(self.chrome.active_tab_id());
                let tab = self.chrome.get_tab_mut(tab_id)?;
                tab.set_shortcut(key_event, shortcut);
            }
            TabAction::Buffer { tab_id, cmd } => {
                cmd.exec(self, app, tab_id)?;
            }
            TabAction::TabForSession { session_id, tx } => {
                let session_id =
                    session_id.unwrap_or(app.active_session.ok_or(ErrorKind::NoActiveSession)?);
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
        tab_shortcut: &TabShortcut,
    ) -> Result<(), Error> {
        match tab_shortcut {
            TabShortcut::SwitchToNext {} => {
                info!("switching to next tab");
                self.chrome.next_tab();
                app.set_active_session(self.chrome.active_tab().session().map(|s| s.id))?;
            }
            TabShortcut::SwitchToPrevious {} => {
                info!("switching to previous tab");
                self.chrome.previous_tab();
                app.set_active_session(self.chrome.active_tab().session().map(|s| s.id))?;
            }
            TabShortcut::Close { tab_id } => {
                let id = match tab_id {
                    None => {
                        info!("closing active tab");
                        self.chrome.active_tab_id()
                    }
                    Some(tab_id) => {
                        info!(tab_id, "closing specific tab");
                        *tab_id
                    }
                };
                let (_, Some(removed)) = self.chrome.close_tab(id) else {
                    let confirm_quit = Python::attach(|py| app.config.borrow(py).confirm_quit);
                    if confirm_quit {
                        app.dialog_manager.show_confirmation(
                            "Are you sure you want to quit?".to_string(),
                            'q',
                            dialog::ConfirmAction::Quit {},
                            Some(Duration::from_secs(25)),
                        );
                    } else {
                        app.should_quit = true;
                    }
                    return Ok(());
                };
                if let Some(session) = removed.session().map(|s| s.id) {
                    info!(session, "closing session");
                    app.close_session(session)?;
                }
                for session in app.sessions.values() {
                    let _ = session
                        .event_handlers
                        .session_event(&python::Event::TabClosed {
                            title: removed.data.title.clone(),
                            tab_id: id,
                        });
                }
                app.set_active_session(self.chrome.active_tab().session().map(|s| s.id))?;
            }
            TabShortcut::SwitchToSession { session } => {
                info!(session, "switching to session tab");
                self.chrome.switch_to_session(*session)?;
            }
            TabShortcut::SwitchToList {} => {
                info!("switching to character list");
                self.chrome.switch_to_list();
            }
            TabShortcut::SwitchTo { tab_id } => {
                info!(tab_id, "switching to tab");
                if let Err(err) = self.chrome.switch_to(*tab_id) {
                    warn!(?err, "failed to switch to tab");
                }
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

impl From<&CrosstermMouseEvent> for MouseEvent {
    fn from(value: &CrosstermMouseEvent) -> Self {
        MouseEvent {
            kind: (&value.kind).into(),
            column: value.column,
            row: value.row,
            modifiers: value.modifiers.into(),
        }
    }
}

impl From<&CrosstermMouseEventKind> for MouseEventKind {
    fn from(value: &CrosstermMouseEventKind) -> Self {
        match value {
            CrosstermMouseEventKind::Down(button) => MouseEventKind::Down(button.into()),
            CrosstermMouseEventKind::Up(button) => MouseEventKind::Up(button.into()),
            CrosstermMouseEventKind::Drag(button) => MouseEventKind::Drag(button.into()),
            CrosstermMouseEventKind::Moved => MouseEventKind::Moved,
            CrosstermMouseEventKind::ScrollDown => MouseEventKind::ScrollDown,
            CrosstermMouseEventKind::ScrollUp => MouseEventKind::ScrollUp,
            CrosstermMouseEventKind::ScrollLeft => MouseEventKind::ScrollLeft,
            CrosstermMouseEventKind::ScrollRight => MouseEventKind::ScrollRight,
        }
    }
}

impl From<&CrosstermMouseButton> for MouseButton {
    fn from(value: &CrosstermMouseButton) -> Self {
        match value {
            CrosstermMouseButton::Left => MouseButton::Left,
            CrosstermMouseButton::Right => MouseButton::Right,
            CrosstermMouseButton::Middle => MouseButton::Middle,
        }
    }
}
