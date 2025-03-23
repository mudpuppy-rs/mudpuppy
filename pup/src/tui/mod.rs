mod char_menu;
mod chrome;
mod commandline;
mod layout;
mod output_buffer;
mod session;

use std::fmt::Debug;
use std::io::{IsTerminal, Stdout, stdout};
use std::num::NonZeroUsize;
use std::panic;

use crossterm::ExecutableCommand;
use crossterm::event::{EventStream as CrosstermEventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
use futures::{FutureExt, StreamExt};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Layout;
use tokio::select;
use tokio::time::{Interval, MissedTickBehavior, interval};
use tracing::{debug, error, info, trace};

use crate::app::AppData;
use crate::config::{CRATE_NAME, Config};
use crate::error::Error;
use crate::{cli, python};
pub(super) use char_menu::CharacterMenu;
pub(super) use chrome::{Chrome, Tab};
pub(super) use layout::{Constraint, Direction, Section};
pub(super) use session::Character;

#[derive(Debug)]
pub(super) struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    draw_interval: Interval,
    event_stream: CrosstermEventStream,
    chrome: Chrome,
}

impl Tui {
    pub(super) fn new(args: &cli::Args, config: &Config) -> Result<Self, Error> {
        let terminal = init_tui_terminal(config.mouse_enabled)?;
        let mut draw_interval = interval(args.frame_rate_duration()?);
        trace!(draw_interval=?draw_interval, "configuring TUI frame rate");
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
        event: &crossterm::event::Event,
    ) -> Result<Option<TabAction>, Error> {
        trace!(event=?event, "crossterm event");

        match event {
            crossterm::event::Event::Key(
                crossterm::event::KeyEvent {
                    code: KeyCode::Esc,
                    kind: KeyEventKind::Press,
                    ..
                }
                | crossterm::event::KeyEvent {
                    code: KeyCode::Char('c'),
                    kind: KeyEventKind::Press,
                    modifiers: KeyModifiers::CONTROL,
                    ..
                },
            ) => {
                app.should_quit = true;
                return Ok(None);
            }
            crossterm::event::Event::Key(crossterm::event::KeyEvent {
                code: KeyCode::Char('n'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => return Ok(Some(TabAction::Next)),
            crossterm::event::Event::Key(crossterm::event::KeyEvent {
                code: KeyCode::Char('p'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => return Ok(Some(TabAction::Previous)),
            crossterm::event::Event::Key(crossterm::event::KeyEvent {
                code: KeyCode::Char('x'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => return Ok(Some(TabAction::Close)),
            _ => {}
        }

        self.chrome.active_tab().crossterm_event(app, event)
    }

    // TODO(XXX): consider getting rid of tabaction, figuring out whether NewSession justifies it.
    fn handle_tab_action(&mut self, app: &mut AppData, tab_action: TabAction) -> Result<(), Error> {
        match tab_action {
            TabAction::Create(sesh) => {
                info!(sesh=?sesh, "creating session");
                app.new_session(&sesh.character)?;
                self.chrome.new_tab(Character::new(sesh));
            }
            TabAction::Next => {
                info!("switching to next tab");
                self.chrome.next_tab();
                app.set_active_session(self.chrome.active_tab().session_id())?;
            }
            TabAction::Previous => {
                info!("switching to previous tab");
                self.chrome.previous_tab();
                app.set_active_session(self.chrome.active_tab().session_id())?;
            }
            TabAction::Close => {
                info!("closing active tab");
                let (_, Some(removed)) = self.chrome.close_active_tab() else {
                    app.should_quit = true;
                    return Ok(());
                };
                if let Some(session) = removed.session_id() {
                    info!(session, "closing session");
                    app.close_session(session)?;
                }
                app.set_active_session(self.chrome.active_tab().session_id())?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(super) enum TabAction {
    Create(python::Session),
    Next,
    Previous,
    Close,
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
