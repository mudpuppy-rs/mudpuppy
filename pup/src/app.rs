use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicU32, Ordering};

use futures::StreamExt;
use notify::event::Event as NotifyEvent;
use pyo3::{Py, Python};
use tokio::select;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::{error, info, instrument, warn, Level};

use crate::config::{self, Config};
use crate::error::Error;
use crate::net::connection::{self};
use crate::python;
use crate::session::{Mud, Session};

#[derive(Debug)]
pub(super) struct App {
    pub(super) config: Py<Config>,
    pub(super) global_event_handlers: python::GlobalHandlers,

    active_session: Option<u32>,
    sessions: HashMap<u32, Session>,
    conn_event_tx: Option<UnboundedSender<connection::Event>>,
    python_event_tx: Option<UnboundedSender<(u32, python::Event)>>,
}

impl App {
    pub(super) fn new(config: Config) -> Self {
        Self {
            config: Python::with_gil(|py| Py::new(py, config).unwrap()),
            global_event_handlers: python::GlobalHandlers::default(),

            active_session: None,
            sessions: HashMap::new(),
            conn_event_tx: None,
            python_event_tx: None,
        }
    }

    #[instrument(level = Level::TRACE, skip(self, py_rx), fields(active_session = ?self.active_session))]
    pub(super) async fn run(
        &mut self,
        mut py_rx: UnboundedReceiver<python::Command>,
    ) -> Result<(), Error> {
        let (conn_event_tx, mut conn_event_rx) = unbounded_channel();
        let (python_event_tx, mut python_event_rx) = unbounded_channel();

        self.conn_event_tx = Some(conn_event_tx);
        self.python_event_tx = Some(python_event_tx);

        python::init_python_env().await?;

        let task_locals = Python::with_gil(pyo3_async_runtimes::tokio::get_current_locals)?;
        let mut setup_task = tokio::spawn(pyo3_async_runtimes::tokio::scope(
            task_locals,
            python::run_user_setup(),
        ));

        let (_watcher, mut config_event_rx) =
            config::reload_watcher().map_err(|e| Error::Internal(e.to_string()))?;

        loop {
            select! {
                Some(py_cmd) = py_rx.recv() => {
                    match py_cmd.exec(self) {
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
                setup_result = &mut setup_task, if !setup_task.is_finished() => {
                    match setup_result {
                        Ok(Ok(())) => info!("Python setup completed successfully"),
                        Ok(Err(e)) => error!("Python setup failed: {e}"),
                        Err(e) => error!("Python setup task panicked: {e}"),
                    }
                }
                Some(connection::Event{session_id, event}) = conn_event_rx.recv() => {
                    let Ok(session) = self.session_mut(session_id) else {
                        warn!("dropping event for missing session {session_id}: {event:?}");
                        return Ok(());
                    };
                    session.event(&event)?;
                }
                Some((id, event)) = python_event_rx.recv() => {
                    self.session(id)?.event_handlers.session_event(&event)?;
                }
                Some(event) = config_event_rx.next() => {
                    if let Ok(event) = event {
                        self.config_reload(&event)?;
                    }
                }
            }
        }
    }

    pub(crate) fn config(&self) -> Py<Config> {
        Python::with_gil(|py| self.config.clone_ref(py))
    }

    pub(crate) fn session(&self, session_id: u32) -> Result<&Session, Error> {
        self.sessions
            .get(&session_id)
            .ok_or(Error::NoSuchSession(session_id))
    }

    pub(crate) fn session_mut(&mut self, session_id: u32) -> Result<&mut Session, Error> {
        self.sessions
            .get_mut(&session_id)
            .ok_or(Error::NoSuchSession(session_id))
    }

    pub(crate) fn sessions(&self) -> Vec<python::Session> {
        self.sessions
            .iter()
            .map(|(id, session)| python::Session {
                id: *id,
                mud: session.mud.clone(),
            })
            .collect()
    }

    pub(crate) fn new_session(&mut self, mud: &Mud) -> Result<python::Session, Error> {
        let (Some(conn_event_tx), Some(py_event_tx)) = (&self.conn_event_tx, &self.python_event_tx)
        else {
            return Err(Error::Internal("App not running".to_owned()));
        };

        let new_id = SESSION_ID.fetch_add(1, Ordering::SeqCst);
        self.sessions.insert(
            new_id,
            Session::new(
                new_id,
                mud.clone(),
                conn_event_tx.clone(),
                py_event_tx.clone(),
            ),
        );
        Ok(python::Session {
            id: new_id,
            mud: mud.clone(),
        })
    }

    pub(crate) fn active_session(&self) -> Option<python::Session> {
        let session = self.session(self.active_session?).ok()?;
        Some(python::Session {
            id: session.id,
            mud: session.mud.clone(),
        })
    }

    pub(crate) fn set_active_session(&mut self, session_id: Option<u32>) -> Result<(), Error> {
        if self.active_session == session_id {
            return Ok(());
        }

        if let Some(new_id) = session_id {
            if !self.sessions.contains_key(&new_id) {
                return Err(Error::NoSuchSession(new_id));
            }
        }

        let from = self.active_session.take();
        self.active_session = session_id;

        let mk_session = |maybe_id: Option<u32>| {
            maybe_id.and_then(|id| {
                self.session(id).ok().map(|sesh| python::Session {
                    id: sesh.id,
                    mud: sesh.mud.clone(),
                })
            })
        };

        self.global_event_handlers
            .global_event(&python::GlobalEvent::ActiveSessionChanged {
                changed_from: mk_session(from),
                changed_to: mk_session(session_id),
            })
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

    // TODO(XXX): Consider reloading python stuff automatically?
    // TODO(XXX): Consider debouncing created->data_changed events.
    fn config_reload(&mut self, event: &NotifyEvent) -> Result<(), Error> {
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
        })?;

        self.global_event_handlers
            .global_event(&python::GlobalEvent::ConfigReloaded {
                config: self.config(),
            })
    }
}

static SESSION_ID: AtomicU32 = AtomicU32::new(0);
