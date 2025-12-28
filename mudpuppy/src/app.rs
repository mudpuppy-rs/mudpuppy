use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicU32, Ordering};

use futures::{StreamExt, future};
use notify::event::Event as NotifyEvent;
use pyo3::{Py, Python};
use strum::Display;
use tokio::select;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{Level, debug, error, info, instrument, trace, warn};

use crate::config::{self, Config};
use crate::dialog::DialogManager;
use crate::error::{Error, ErrorKind};
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::net::connection::{self};
use crate::python::{BufferCommand, Command, DialogCommand, NewSessionHandler, SessionCommand};
use crate::session::{Buffer, Session};
use crate::shortcut::{Shortcut, TabShortcut};
pub(crate) use crate::slash_command::SlashCommand;
use crate::tui::{Section, Tui};
use crate::{cli, python};

#[derive(Debug)]
pub(super) struct App {
    data: AppData,
    tui: Tui,
}

impl App {
    pub(super) fn new(args: cli::Args, config: &Py<Config>) -> Result<Self, Error> {
        Ok(Self {
            tui: Tui::new(&args, config)?,
            data: AppData::new(args, config),
        })
    }

    #[instrument(level = Level::TRACE, skip(self, py_rx))]
    pub(super) async fn run(
        &mut self,
        mut py_rx: UnboundedReceiver<python::Command>,
    ) -> Result<(), Error> {
        let (_watcher, mut config_event_rx) =
            config::reload_watcher().map_err(|e| ErrorKind::Internal(e.to_string()))?;

        let (conn_event_tx, mut conn_event_rx) = unbounded_channel();
        let (python_event_tx, mut python_event_rx) = unbounded_channel();

        self.data.conn_event_tx = Some(conn_event_tx);
        self.data.python_event_tx = Some(python_event_tx);

        python::init_python_env().await?;

        python::run_user_setup(&self.data.config, &mut self.data.dialog_manager).await;

        // Drain Python command queue to ensure new session handlers are registered
        // before auto-connect runs (handlers are registered via commands sent during init)
        while let Ok(py_cmd) = py_rx.try_recv() {
            if let Err(err) = py_cmd.exec(&mut self.tui, &mut self.data).await {
                error!("python command during init failed: {err}");
                self.data
                    .dialog_manager
                    .show_error(format!("Python command during init failed: {err}"));
            }
        }
        debug!(
            handler_count = self.data.new_session_handlers.len(),
            "new session handlers registered"
        );

        self.data.auto_connect(&mut self.tui)?;

        let result = loop {
            self.data.dialog_manager.tick();

            // Tick per-session dialog managers
            for session in self.data.sessions.values_mut() {
                session.dialog_manager.tick();
            }

            if self.data.should_quit {
                info!("quitting. Goodbye!");
                break Ok(());
            }

            select! {
                // Configuration reload
                Some(event) = config_event_rx.next() => {
                    if let Ok(event) = event {
                        self.data.config_reloaded(&event);
                    }
                }
                // Connection event dispatch
                Some(connection::Event{session_id, event}) = conn_event_rx.recv() => {
                    let Ok(session) = self.data.session_mut(session_id) else {
                        warn!("dropping event for missing session {session_id}: {event:?}");
                        continue;
                    };
                    if let Err(err) = session.handle_event(&event) {
                        error!("session event error: {err}");
                        self.data.dialog_manager.show_error(format!("Session event error: {err}"));
                    }
                }
                // Python event dispatch (sending events from app -> Python)
                Some((id, event)) = python_event_rx.recv() => {
                    let Ok(session) = self.data.session(id) else {
                        warn!("dropping python event for missing session {id}: {event:?}");
                        continue;
                    };
                    if let Err(err) = session.event_handlers.session_event(&event) {
                        error!("python event dispatch error: {err}");
                        self.data.dialog_manager.show_error(
                                format!("Python event dispatch error: {err}"));
                    }
                }
                // Python API command dispatch (processing reqs from Python -> app)
                Some(py_cmd) = py_rx.recv() => {
                    match py_cmd.exec(&mut self.tui, &mut self.data).await {
                        Ok(true) => {
                            info!("quitting");
                            break Ok(());
                        }
                        Err(err) => {
                            error!("python error: {err}");
                            self.data.dialog_manager.show_error(
                                    format!("Python error: {err}"));
                        }
                        _ => {}
                    }
                }
                // Frontend processing
                result = self.tui.run(&mut self.data) => {
                    if let Err(err) = result {
                        error!("app error: {err}");
                        break Err(err);
                    }
                }
            }
        };

        self.tui.exit();
        result
    }
}

#[derive(Debug)]
pub(super) struct AppData {
    pub(super) should_quit: bool,
    pub(super) dialog_manager: DialogManager,
    pub(super) args: cli::Args,
    pub(super) config: Py<Config>,
    pub(super) active_session: Option<u32>,
    pub(super) sessions: HashMap<u32, Session>,
    pub(super) new_session_handlers: Vec<NewSessionHandler>,
    pub(super) shortcuts: HashMap<KeyEvent, Shortcut>,

    conn_event_tx: Option<UnboundedSender<connection::Event>>,
    python_event_tx: Option<UnboundedSender<(u32, python::Event)>>,
}

impl AppData {
    fn new(args: cli::Args, config: &Py<Config>) -> Self {
        Self {
            should_quit: false,
            dialog_manager: DialogManager::new(),
            args,
            config: Python::attach(|py| config.clone_ref(py)),
            active_session: None,
            sessions: HashMap::new(),
            new_session_handlers: Vec::new(),
            shortcuts: Self::default_shortcuts(),
            conn_event_tx: None,
            python_event_tx: None,
        }
    }

    fn default_shortcuts() -> HashMap<KeyEvent, Shortcut> {
        let mut shortcuts = HashMap::new();
        shortcuts.insert(
            KeyEvent::new(KeyModifiers::CONTROL, KeyCode::Char('c')),
            Shortcut::Quit {},
        );
        shortcuts.insert(
            KeyEvent::new(KeyModifiers::NONE, KeyCode::Esc),
            Shortcut::Quit {},
        );
        shortcuts.insert(
            KeyEvent::new(KeyModifiers::CONTROL, KeyCode::Char('n')),
            TabShortcut::SwitchToNext {}.into(),
        );
        shortcuts.insert(
            KeyEvent::new(KeyModifiers::CONTROL, KeyCode::Char('p')),
            TabShortcut::SwitchToPrevious {}.into(),
        );
        shortcuts.insert(
            KeyEvent::new(KeyModifiers::ALT, KeyCode::Char('n')),
            TabShortcut::MoveRight { tab_id: None }.into(),
        );
        shortcuts.insert(
            KeyEvent::new(KeyModifiers::ALT, KeyCode::Char('p')),
            TabShortcut::MoveLeft { tab_id: None }.into(),
        );
        shortcuts.insert(
            KeyEvent::new(KeyModifiers::CONTROL, KeyCode::Char('x')),
            TabShortcut::Close { tab_id: None }.into(),
        );
        shortcuts
    }

    pub(crate) fn session(&self, session_id: u32) -> Result<&Session, Error> {
        self.sessions
            .get(&session_id)
            .ok_or(ErrorKind::NoSuchSession(session_id).into())
    }

    pub(crate) fn session_mut(&mut self, session_id: u32) -> Result<&mut Session, Error> {
        self.sessions
            .get_mut(&session_id)
            .ok_or(ErrorKind::NoSuchSession(session_id).into())
    }

    pub(crate) fn sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_unstable_by_key(|s| s.id);
        sessions
    }

    pub(crate) fn sessions_py(&self) -> Vec<python::Session> {
        let mut sessions: Vec<python::Session> = self.sessions.values().map(Into::into).collect();
        sessions.sort_unstable_by_key(|s| s.id);
        sessions
    }

    pub(crate) fn new_session(
        &mut self,
        character: &str,
    ) -> Result<(python::Session, Vec<JoinHandle<()>>), Error> {
        let (Some(conn_event_tx), Some(py_event_tx)) = (&self.conn_event_tx, &self.python_event_tx)
        else {
            return Err(ErrorKind::Internal("App not running".to_owned()).into());
        };

        let new_id = SESSION_ID.fetch_add(1, Ordering::SeqCst);
        let session = Session::new(
            new_id,
            character.to_owned(),
            &self.config,
            conn_event_tx.clone(),
            py_event_tx.clone(),
        )?;

        self.sessions.insert(new_id, session);

        let new_sesh = python::Session {
            id: new_id,
            character: character.to_owned(),
        };

        let mut handles = Vec::new();
        for handler in &self.new_session_handlers {
            handles.push(handler.execute(new_sesh.clone())?);
        }
        trace!(
            session_id = new_id,
            character,
            handler_count = handles.len(),
            "spawned new session handler tasks"
        );

        if self.active_session_py().is_none() {
            self.set_active_session(Some(new_id))?;
        }

        Ok((new_sesh, handles))
    }

    pub(crate) fn active_session(&self) -> Option<&Session> {
        self.active_session.and_then(|id| self.sessions.get(&id))
    }

    pub(crate) fn active_session_mut(&mut self) -> Option<&mut Session> {
        self.active_session
            .and_then(|id| self.sessions.get_mut(&id))
    }

    pub(crate) fn active_session_py(&self) -> Option<python::Session> {
        self.session(self.active_session?).ok().map(Into::into)
    }

    pub(crate) fn set_active_session(&mut self, session_id: Option<u32>) -> Result<(), Error> {
        if self.active_session == session_id {
            return Ok(());
        }

        if let Some(new_id) = session_id {
            if !self.sessions.contains_key(&new_id) {
                return Err(ErrorKind::NoSuchSession(new_id).into());
            }
        }

        let from = self.active_session.take();
        self.active_session = session_id;

        let mk_session =
            |maybe_id: Option<u32>| maybe_id.and_then(|id| self.session(id).ok().map(Into::into));

        for sesh in self.sessions.values() {
            sesh.event_handlers
                .session_event(&python::Event::ActiveSessionChanged {
                    changed_from: mk_session(from),
                    changed_to: mk_session(session_id),
                })?;
        }

        Ok(())
    }

    pub(crate) fn close_session(&mut self, session_id: u32) -> Result<(), Error> {
        if self.active_session == Some(session_id) {
            self.set_active_session(None)?;
        }

        self.session(session_id)?
            .event_handlers
            .session_event(&python::Event::SessionClosed {})?;

        self.sessions.remove(&session_id);
        Ok(())
    }

    fn auto_connect(&mut self, tui: &mut Tui) -> Result<(), Error> {
        let auto_connect = self.args.connect.clone();

        trace!(?auto_connect, "auto-connecting");
        let mut sessions = Vec::with_capacity(auto_connect.len());
        let mut new_session_handlers = Vec::new();
        for character in &auto_connect {
            info!(character, "auto-connecting");
            let (sesh, handles) = self.new_session(character)?;
            sessions.push(sesh.clone());
            new_session_handlers.extend(handles);
            // Create tab immediately so handlers can access it
            tui.handle_tab_action(self, TabAction::CreateSessionTab { session: sesh })?;
        }

        tokio::spawn(async move {
            join_all(new_session_handlers, "new session handler task panicked").await;
            // Try to connect each session individually, don't stop on first failure
            for sesh in &sessions {
                Python::attach(|py| {
                    if let Err(e) = sesh.connect(py) {
                        error!("auto-connect failed for '{}': {e}", sesh.character);
                        let _ = python::dispatch_command(
                            py,
                            Command::Session(SessionCommand::Dialog {
                                session_id: sesh.id,
                                cmd: DialogCommand::Error(e),
                            }),
                        );
                    }
                });
            }
        });

        Ok(())
    }

    // TODO(XXX): Consider reloading python stuff automatically?
    // TODO(XXX): Consider debouncing created->data_changed events.
    fn config_reloaded(&mut self, event: &NotifyEvent) {
        use notify::EventKind;

        let data_changed = matches!(
            event.kind,
            EventKind::Modify(notify::event::ModifyKind::Data(_) | notify::event::ModifyKind::Any)
        );

        if !event.paths.contains(&config::config_file()) || !data_changed {
            return;
        }

        // Try to reload config, but don't exit app on failure
        if let Err(err) = Python::attach(|py| {
            info!("reloading configuration: data changed");
            self.config
                .borrow_mut(py)
                .replace_with(Config::new().map_err(ErrorKind::from)?);
            Ok::<_, Error>(())
        }) {
            error!("config reload failed: {err}");
            self.dialog_manager
                .show_error(format!("Config reload failed: {err}"));
            return; // Continue with old config
        }

        // Notify sessions of config reload, but don't exit on handler errors
        Python::attach(|py| {
            for sesh in self.sessions.values() {
                if let Err(err) =
                    sesh.event_handlers
                        .session_event(&python::Event::ConfigReloaded {
                            config: self.config.clone_ref(py),
                        })
                {
                    error!("config reload event handler error: {err}");
                    self.dialog_manager
                        .show_error(format!("config reload event handler error: {err}"));
                }
            }
        });
    }
}

#[derive(Debug, Display)]
pub(crate) enum TabAction {
    Shortcut(TabShortcut),
    CreateSessionTab {
        session: python::Session,
    },
    CreateCustomTab {
        title: String,
        layout: Option<Py<Section>>,
        buffers: Vec<Py<Buffer>>,
        tx: oneshot::Sender<python::Tab>,
    },
    Layout {
        tab_id: u32,
        tx: oneshot::Sender<Py<Section>>, // Leaking TUI bits here :-/
    },
    Title {
        tab_id: u32,
        tx: oneshot::Sender<String>,
    },
    SetTitle {
        tab_id: Option<u32>,
        title: String,
    },
    AllShortcuts {
        tab_id: Option<u32>,
        tx: oneshot::Sender<HashMap<KeyEvent, String>>,
    },
    SetShortcut {
        tab_id: Option<u32>,
        key_event: KeyEvent,
        shortcut: Option<Shortcut>,
    },
    Buffer {
        tab_id: u32,
        cmd: BufferCommand,
    },
    TabForSession {
        session_id: Option<u32>,
        tx: oneshot::Sender<python::Tab>,
    },
    AllTabs {
        tx: oneshot::Sender<Vec<python::Tab>>,
    },
}

impl From<TabShortcut> for TabAction {
    fn from(tab_shortcut: TabShortcut) -> Self {
        Self::Shortcut(tab_shortcut)
    }
}

/// helper to await a set of `JoinHandle`'s without caring about results.
pub(crate) async fn join_all(handles: Vec<JoinHandle<()>>, err_prefix: &str) {
    for result in future::join_all(handles).await {
        if let Err(e) = result {
            error!("{err_prefix}: {e}");
        }
    }
}

static SESSION_ID: AtomicU32 = AtomicU32::new(0);
