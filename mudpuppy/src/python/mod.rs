mod api;
mod command;
mod events;

use std::fmt::Display;
use std::future::Future as StdFuture;
use std::pin::Pin;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::ffi::c_str;
use pyo3::sync::PyOnceLock;
use pyo3::types::{
    PyAnyMethods, PyBool, PyBoolMethods, PyFunction, PyList, PyListMethods, PyModule,
};
use pyo3::{Bound, Py, PyAny, PyErr, PyResult, Python};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{Level, debug, error, instrument, trace};

use crate::config::{Config, config_dir};
use crate::error::Error;

use crate::dialog::DialogManager;
pub(crate) use api::*;
pub(crate) use command::*;
pub(crate) use events::*;

macro_rules! import_builtin_modules {
     ($($module:expr),* $(,)?) => {
        Python::attach(|py| {
            $(
                debug!(concat!("loading built-in module ", $module, ".py"));
                PyModule::from_code(
                    py,
                    c_str!(include_str!(concat!($module, ".py"))),
                    c_str!(concat!($module, ".py")),
                    c_str!($module),
                )
                .unwrap(); // Safety: builtin modules must always compile!
            )*
        });
    }
}

// TODO(XXX): restore macro for loading built-in modules in a more DRY way.
#[instrument]
pub(super) async fn init_python_env() -> Result {
    Python::attach(|py| {
        trace!(dir=?config_dir(), "adding config dir to sys.path");
        py.import("sys")?
            .getattr("path")?
            .cast_into::<PyList>()
            .unwrap()
            .insert(0, config_dir().to_string_lossy())?;
        Ok::<_, Error>(())
    })?;

    import_builtin_modules!(
        "logging",
        "pup_events",
        "history",
        "telnet_charset",
        "telnet_naws",
        "cmd_py",
        "tui"
    );

    Ok(())
}

#[instrument(level = Level::DEBUG, skip(config, dm))]
pub(super) async fn run_user_setup(config: &Py<Config>, dm: &mut DialogManager) {
    let mut py_futures = FuturesUnordered::new();

    let import_result = Python::attach(|py| {
        // TODO(XXX): retain module handles for reload?
        for module in &config.borrow(py).modules {
            debug!(module, "loading");
            let module = PyModule::import(py, module)?;

            if let Ok(setup_fn) = module.getattr("setup") {
                require_coroutine(py, format!("{module} setup"), setup_fn.as_ref())?;
                let setup_future = pyo3_async_runtimes::tokio::into_future(setup_fn.call0()?)?;
                py_futures.push(Box::pin(setup_future));
            }
        }
        Ok::<(), PyErr>(())
    });

    if let Err(err) = import_result {
        // Note: Error::from() to collect backtrace from PyErr.
        let err = Error::from(err);
        error!("python user module setup error: {err}");
        dm.show_error(err.to_string());
        return;
    }

    while let Some(result) = py_futures.next().await {
        if let Err(err) = result {
            // Note: Error::from() to collect backtrace from PyErr.
            let err = Error::from(err);
            error!("python user module setup error: {err}");
            dm.show_error(err.to_string());
        }
    }
}

#[instrument(level = Level::DEBUG)]
pub(super) fn run_character_setup(
    id: u32,
    character: &str,
    module: String,
) -> Option<Py<PyModule>> {
    let (future, module) = match Python::attach(|py| {
        let module = PyModule::import(py, module)?;

        let future = if let Ok(setup_fn) = module.getattr("setup") {
            require_coroutine(py, format!("{module} setup"), setup_fn.as_ref())?;
            Some(pyo3_async_runtimes::tokio::into_future(setup_fn.call1(
                (Session {
                    id,
                    character: character.to_owned(),
                },),
            )?)?)
        } else {
            None
        };

        Ok::<_, Error>((future, module.unbind()))
    }) {
        Ok((future, module)) => (future, module),
        Err(err) => {
            error!("character {character} module import error: {err}");
            let _ = Python::attach(|py| {
                dispatch_command(
                    py,
                    SessionCommand::Dialog {
                        session_id: id,
                        cmd: DialogCommand::Error(err),
                    },
                )
            });
            return None;
        }
    };

    // Spawn the setup future if it exists
    if let Some(future) = future {
        let character_name = character.to_string();
        tokio::spawn(async move {
            if let Err(error) = future.await {
                // NOTE: Using Error::from to collect backtrace from PyErr.
                let error = Error::from(error);
                error!("character {character_name} module setup error: {error}");
                let _ = Python::attach(|py| {
                    dispatch_command(
                        py,
                        SessionCommand::Dialog {
                            session_id: id,
                            cmd: DialogCommand::Error(error),
                        },
                    )
                });
            }
        });
    }

    Some(module)
}

pub(super) fn require_coroutine(
    py: Python<'_>,
    typ: impl Display,
    callback: &Py<PyAny>,
) -> PyResult<()> {
    // TODO(XXX): possible optimization - cache ref to this fn?
    let iscoroutinefunction = py
        .import("inspect")?
        .getattr("iscoroutinefunction")?
        .cast_into::<PyFunction>()
        .map_err(|e| {
            PyRuntimeError::new_err(format!("getting inspect iscoroutinefunction: {e}"))
        })?;

    let is_coroutine = iscoroutinefunction
        .call1((callback,))?
        .cast_into::<PyBool>()?
        .is_true();

    match is_coroutine {
        true => Ok(()),
        false => Err(PyTypeError::new_err(format!(
            "{typ} handler must be a coroutine function"
        ))),
    }
}

pub(super) fn label_for_coroutine(py: Python<'_>, callback: &Py<PyAny>) -> Option<String> {
    // TODO(XXX): possible optimization - cache ref to this fn?
    let getmodule = py.import("inspect").ok()?.getattr("getmodule").ok()?;

    let module = getmodule.call1((callback,)).ok()?;
    let module_name = module
        .getattr("__name__")
        .ok()?
        .str()
        .map(|pystr| pystr.to_string())
        .unwrap_or("unknown".to_string());
    let callback = callback.bind(py);
    let callback_qualname = callback
        .getattr("__qualname__")
        .ok()?
        .str()
        .map(|pystr| pystr.to_string())
        .unwrap_or("unknown".to_string());

    Some(format!("{module_name}.{callback_qualname}"))
}

pub(super) type PyFuture = Pin<Box<dyn StdFuture<Output = PyResult<Py<PyAny>>> + Send + 'static>>;

pub(crate) static APP: PyOnceLock<UnboundedSender<Command>> = PyOnceLock::new();

type Result<T = ()> = std::result::Result<T, Error>;
type FutureResult<'a> = PyResult<Bound<'a, PyAny>>;
