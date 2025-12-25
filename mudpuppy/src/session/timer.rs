use std::time::Duration;

use pyo3::types::PyAnyMethods;
use pyo3::{Bound, Py, PyAny, Python, pyclass, pymethods};
use pyo3_async_runtimes::tokio as pyo3tokio;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, error, info, trace, warn};

use crate::error::{Error, ErrorKind};
use crate::python::{self, SessionCommand, dispatch_command, require_coroutine};
use crate::session::InputLine;

// TODO(XXX): flagset instead of bools
#[derive(Debug)]
#[pyclass]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Timer {
    #[pyo3(get, set)]
    pub(crate) name: String,

    #[pyo3(get)]
    duration: Duration,

    #[pyo3(get, set)]
    callback: Option<Py<PyAny>>,

    #[pyo3(get, set)]
    reaction: Option<String>,

    #[pyo3(get, set)]
    session: Option<python::Session>,

    #[pyo3(get)]
    hit_count: u64,

    task: Option<JoinHandle<()>>,
}

#[pymethods]
impl Timer {
    #[new]
    #[pyo3(signature = (name, duration_seconds, *, callback=None, reaction=None, session=None))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        py: Python<'_>,
        name: String,
        duration_seconds: u64,
        callback: Option<Py<PyAny>>,
        reaction: Option<String>,
        session: Option<python::Session>,
    ) -> Result<Py<Self>, Error> {
        // TODO(XXX): better duration parsing. Chrono?
        let duration = Duration::from_secs(duration_seconds);

        if callback.is_none() && reaction.is_none() {
            return Err(ErrorKind::InvalidTimer(
                "one of callback or reaction must be provided".to_owned(),
            )
            .into());
        }

        if reaction.is_some() && session.is_none() {
            return Err(ErrorKind::InvalidTimer(
                "reaction requires a session to be provided".to_owned(),
            )
            .into());
        }

        if let Some(callback) = callback.as_ref() {
            require_coroutine(py, "Timer callback", callback)?;
        }

        let t = Self {
            name,
            duration,
            callback,
            reaction,
            session,
            hit_count: 0,
            task: None,
        };

        Ok(Py::new(py, t)?)
    }

    #[getter(duration)]
    fn duration(&self) -> u64 {
        self.duration.as_secs()
    }

    fn running(&self) -> bool {
        self.task.is_some()
    }

    fn start(self_: &Bound<Self>) {
        let config = self_.borrow();
        if config.task.is_some() {
            warn!(name = config.name, "timer is already running");
            return;
        }
        let name = config.name.clone();
        drop(config);

        info!(name, "starting timer");
        let py_config = self_.as_unbound().clone_ref(self_.py());
        let locals =
            pyo3tokio::get_current_locals(self_.py()).expect("failed to get event loop locals");

        let handle =
            pyo3tokio::get_runtime().spawn(pyo3tokio::scope(locals, run_timer_loop(py_config)));
        self_.borrow_mut().task = Some(handle);
    }

    fn stop(self_: &Bound<Self>) {
        let mut config = self_.borrow_mut();
        let Some(task) = config.task.take() else {
            warn!(name = config.name, "timer is already stopped");
            return;
        };
        let name = config.name.clone();
        drop(config);

        info!(name, "stopping timer");
        task.abort();
    }

    fn __str__(&self) -> String {
        // TODO(XXX): better Display for Timer
        format!("{self:?}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

async fn run_timer_loop(py_config: Py<Timer>) {
    loop {
        sleep(Python::attach(|py| py_config.borrow(py).duration)).await;

        let Some((callback, reaction, session)) = Python::attach(|py| {
            let mut config = py_config.borrow_mut(py);
            config.hit_count += 1;
            trace!(
                name = config.name,
                hit_count = config.hit_count,
                "timer fired"
            );
            let name = config.name.clone();
            let callback = config.callback.clone();
            let reaction = config.reaction.clone();
            let session = config.session.clone();
            drop(config);

            let callback = match callback
                .map(|cb| {
                    cb.bind(py)
                        .call1((py_config.clone_ref(py),))
                        .and_then(|py_future| pyo3tokio::into_future(py_future))
                })
                .transpose()
            {
                Ok(callback) => callback,
                Err(err) => {
                    let error_msg = Error::from(err);
                    error!(name, "timer failed: {error_msg}");
                    if let Some(error_tx) = python::ERROR_TX.get(py) {
                        let _ = error_tx.send(format!("Timer '{name}' failed: {error_msg}"));
                    }
                    return None;
                }
            };

            Some((callback, reaction, session))
        }) else {
            return;
        };

        if let Some(callback) = callback {
            let name = Python::attach(|py| py_config.borrow(py).name.clone());
            debug!(name, "invoking timer callback");
            if let Err(err) = callback.await {
                let error_msg = Error::from(err);
                error!(name, "timer failed: {error_msg}");
                Python::attach(|py| {
                    if let Some(error_tx) = python::ERROR_TX.get(py) {
                        let _ =
                            error_tx.send(format!("Timer '{name}' callback failed: {error_msg}"));
                    }
                });
                return;
            }
        }

        if let (Some(reaction), Some(session)) = (reaction, session) {
            debug!(reaction, %session, "sending timer reaction");
            Python::attach(|py| {
                let _ = dispatch_command(
                    py,
                    SessionCommand::SendLine {
                        session_id: session.id,
                        line: InputLine::new(reaction, None, None, true),
                        skip_aliases: false,
                    },
                );
            });
        }
    }
}
