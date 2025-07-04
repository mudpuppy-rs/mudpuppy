use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use futures::StreamExt;
use notify::event::Event as NotifyEvent;
use pyo3::{Py, Python};
use strum::Display;
use tokio::select;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;
use tracing::{Level, error, info, instrument, trace, warn};

use crate::config::{self, Config};
use crate::error::{Error, ErrorKind};
use crate::headless::Headless;
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::net::connection::{self};
use crate::python::{Event, NewSessionHandler};
use crate::session::{Character, Session};
use crate::shortcut::{Shortcut, TabShortcut};
pub(crate) use crate::slash_command::SlashCommand;
use crate::tui::{Section, Tui};
use crate::{cli, python, slash_command};

#[derive(Debug)]
pub(super) struct App {
    args: cli::Args,
    config: Config,
    data: AppData,
    frontend: Frontend,
}

impl App {
    pub(super) fn new(args: cli::Args, config: &Config) -> Result<Self, Error> {
        Ok(Self {
            data: AppData::new(args.clone(), config.clone()),
            frontend: match args.headless {
                true => Headless::new().into(),
                false => Tui::new(&args, config)?.into(),
            },
            config: config.clone(),
            args,
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

        python::init_python_env(&self.args).await?;

        // Spawn a task to run the Python user setup code.
        let config = self.config.clone();
        let task_locals = Python::with_gil(pyo3_async_runtimes::tokio::get_current_locals)?;
        let mut setup_task =
            tokio::spawn(pyo3_async_runtimes::tokio::scope(task_locals, async move {
                if let Err(e) = python::run_user_setup(config).await {
                    error!("python user setup failed: {e}");
                }
            }));

        let result = loop {
            if self.data.should_quit {
                info!("quit request processed");
                break Ok(());
            }
            select! {
                // User python module setup has completed
                setup_result = &mut setup_task, if !setup_task.is_finished() => {
                    match setup_result {
                        Ok(()) => {
                            info!("Python setup completed successfully");
                            self.data.auto_connect(&mut self.frontend).await?;
                        }
                        Err(e) => error!("Python setup task panicked: {e}"),
                    }
                }
                // Configuration reload
                Some(event) = config_event_rx.next() => {
                    if let Ok(event) = event {
                        self.data.config_reloaded(&event)?;

                        Python::with_gil(|py| {
                            self.frontend.config_reloaded(& self.data.config.borrow(py))?;
                            Ok::<(), Error>(())
                        })?;
                    }
                }
                // Connection event dispatch
                Some(connection::Event{session_id, event}) = conn_event_rx.recv() => {
                    let Ok(session) = self.data.session_mut(session_id) else {
                        warn!("dropping event for missing session {session_id}: {event:?}");
                        continue;
                    };
                    session.handle_event(&event)?;
                }
                // Python event dispatch
                Some((id, event)) = python_event_rx.recv() => {
                    self.data.session(id)?.event_handlers.session_event(id, &event)?;
                }
                // Python API command dispatch
                Some(py_cmd) = py_rx.recv() => {
                    // TODO(XXX): feels awkward to have this layer handle UI related commands ad-hoc
                    if let python::Command::Tab(tab_action) = py_cmd {
                        if let Err(err) =  self.frontend.tab_action(&mut self.data, tab_action).await {
                            error!("python error: {err}");
                        }
                    } else {
                        match py_cmd.exec(&mut self.data) {
                            Ok(true) => {
                                info!("quitting");
                                break Ok(());
                            }
                            Err(err) => {
                                error!("python error: {err}");
                            }
                            _ => {}
                        }
                    }
                }
                // Frontend processing
                result = self.frontend.run(&mut self.data) => {
                    if let Err(err) = result {
                        error!("app error: {err}");
                        break Err(err);
                    }
                }
            }
        };

        self.frontend.exit();
        result
    }
}

#[derive(Debug)]
pub(super) struct AppData {
    pub(super) should_quit: bool,
    pub(super) args: cli::Args,
    pub(super) config: Py<Config>,
    pub(super) active_session: Option<u32>,
    pub(super) sessions: HashMap<u32, Session>,
    pub(super) new_session_handlers: Vec<NewSessionHandler>,
    pub(super) slash_commands: HashMap<String, Arc<dyn SlashCommand>>,
    pub(super) shortcuts: HashMap<KeyEvent, Shortcut>,

    conn_event_tx: Option<UnboundedSender<connection::Event>>,
    python_event_tx: Option<UnboundedSender<(u32, python::Event)>>,
}

impl AppData {
    fn new(args: cli::Args, config: Config) -> Self {
        Self {
            should_quit: false,
            args,
            config: Python::with_gil(|py| Py::new(py, config).unwrap()),
            active_session: None,
            sessions: HashMap::new(),
            new_session_handlers: Vec::new(),
            slash_commands: slash_command::builtin(),
            shortcuts: Self::default_shortcuts(),
            conn_event_tx: None,
            python_event_tx: None,
        }
    }

    fn default_shortcuts() -> HashMap<KeyEvent, Shortcut> {
        let mut shortcuts = HashMap::new();
        shortcuts.insert(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Shortcut::Quit {},
        );
        shortcuts.insert(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            Shortcut::Quit {},
        );
        shortcuts.insert(
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
            TabShortcut::SwitchToNext {}.into(),
        );
        shortcuts.insert(
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
            TabShortcut::SwitchToPrevious {}.into(),
        );
        shortcuts.insert(
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT),
            TabShortcut::MoveRight { tab_id: None }.into(),
        );
        shortcuts.insert(
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
            TabShortcut::MoveLeft { tab_id: None }.into(),
        );
        shortcuts.insert(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
            TabShortcut::Close { tab_id: None }.into(),
        );
        shortcuts
    }

    pub(crate) fn config(&self) -> Py<Config> {
        Python::with_gil(|py| self.config.clone_ref(py))
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

    pub(crate) fn new_session(&mut self, character: &Character) -> Result<python::Session, Error> {
        let (Some(conn_event_tx), Some(py_event_tx)) = (&self.conn_event_tx, &self.python_event_tx)
        else {
            return Err(ErrorKind::Internal("App not running".to_owned()).into());
        };

        let new_id = SESSION_ID.fetch_add(1, Ordering::SeqCst);
        self.sessions.insert(
            new_id,
            Session::new(
                new_id,
                character.clone(),
                conn_event_tx.clone(),
                py_event_tx.clone(),
            )?,
        );

        let new_sesh = python::Session {
            id: new_id,
            character: character.clone(),
        };

        for handler in &self.new_session_handlers {
            handler.execute(new_sesh.clone())?;
        }

        if self.active_session_py().is_none() {
            self.set_active_session(Some(new_id))?;
        }

        Ok(new_sesh)
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

        for (id, sesh) in &self.sessions {
            sesh.event_handlers.session_event(
                *id,
                &Event::ActiveSessionChanged {
                    changed_from: mk_session(from),
                    changed_to: mk_session(session_id),
                },
            )?;
        }

        Ok(())
    }

    pub(crate) fn close_session(&mut self, session_id: u32) -> Result<(), Error> {
        if self.active_session == Some(session_id) {
            self.set_active_session(None)?;
        }

        self.session(session_id)?
            .event_handlers
            .session_event(session_id, &python::Event::SessionClosed {})?;

        self.sessions.remove(&session_id);
        Ok(())
    }

    async fn auto_connect(&mut self, fe: &mut Frontend) -> Result<(), Error> {
        let auto_connect = self.args.connect.clone();

        let mut sessions = Vec::with_capacity(auto_connect.len());
        Python::with_gil(|py| {
            trace!(?auto_connect, "auto-connecting");
            for char_name in &auto_connect {
                let Some(character) = ({
                    self.config
                        .borrow(py)
                        .characters
                        .iter()
                        .find(|m| m.name == *char_name)
                        .cloned()
                }) else {
                    error!("character not found in config: {char_name}");
                    continue;
                };
                info!(character=?character, "auto-connecting");
                let sesh = self.new_session(&character)?;
                sesh.connect(py)?;
                sessions.push(sesh);
            }
            Ok::<(), Error>(())
        })?;

        for sesh in sessions {
            fe.tab_action(self, TabAction::Create { session: sesh })
                .await?;
        }
        Ok(())
    }

    // TODO(XXX): Consider reloading python stuff automatically?
    // TODO(XXX): Consider debouncing created->data_changed events.
    fn config_reloaded(&mut self, event: &NotifyEvent) -> Result<(), Error> {
        use notify::EventKind;

        let data_changed = matches!(
            event.kind,
            EventKind::Modify(notify::event::ModifyKind::Data(_) | notify::event::ModifyKind::Any)
        );

        if !event.paths.contains(&config::config_file()) || !data_changed {
            return Ok(());
        }

        Python::with_gil(|py| {
            info!("reloading configuration: data changed");
            self.config.borrow_mut(py).load()
        })
        .map_err(ErrorKind::from)?;

        for (id, sesh) in &self.sessions {
            sesh.event_handlers.session_event(
                *id,
                &Event::ConfigReloaded {
                    config: self.config(),
                },
            )?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(super) enum Frontend {
    Headless(Headless),
    Tui(Tui),
}

impl Frontend {
    async fn run(&mut self, app: &mut AppData) -> Result<(), Error> {
        match self {
            Frontend::Headless(headless) => headless.run(app).await,
            Frontend::Tui(tui) => tui.run(app).await,
        }
    }

    #[instrument(skip(self, app, action) fields(action=action.to_string()))]
    async fn tab_action(&mut self, app: &mut AppData, action: TabAction) -> Result<(), Error> {
        let Frontend::Tui(tui) = self else {
            warn!(action=?action, "ignoring tab action in headless mode");
            return Ok(());
        };
        tui.handle_tab_action(app, action)
    }

    fn config_reloaded(&mut self, config: &Config) -> Result<(), Error> {
        match self {
            // Headless mode doesn't require any config reloading.
            Frontend::Headless(_) => Ok(()),
            Frontend::Tui(tui) => tui.config_reloaded(config),
        }
    }

    fn exit(&mut self) {
        // Headless mode doesn't require any exit cleanup.
        let Frontend::Tui(tui) = self else {
            return;
        };

        tui.exit();
    }
}

impl From<Headless> for Frontend {
    fn from(h: Headless) -> Self {
        Frontend::Headless(h)
    }
}

impl From<Tui> for Frontend {
    fn from(t: Tui) -> Self {
        Frontend::Tui(t)
    }
}

#[derive(Debug, Display)]
pub(crate) enum TabAction {
    Shortcut(TabShortcut),
    Create {
        session: python::Session,
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

static SESSION_ID: AtomicU32 = AtomicU32::new(0);
