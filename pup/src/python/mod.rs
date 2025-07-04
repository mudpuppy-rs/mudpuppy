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
use pyo3::sync::GILOnceCell;
use pyo3::types::{
    PyAnyMethods, PyBool, PyBoolMethods, PyFunction, PyList, PyListMethods, PyModule,
};
use pyo3::{Bound, Py, PyAny, PyErr, PyObject, PyResult, Python};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{Level, debug, error, instrument, trace};

use crate::config::{Config, config_dir};
use crate::error::Error;

use crate::cli;
use crate::session::Character;
pub(crate) use api::*;
pub(crate) use command::*;
pub(crate) use events::*;

// TODO(XXX): restore macro for loading built-in modules in a more DRY way.
#[instrument(skip(args))]
pub(super) async fn init_python_env(args: &cli::Args) -> Result {
    let mut py_futures = FuturesUnordered::new();

    Python::with_gil(|py| {
        trace!(dir=?config_dir(), "adding config dir to sys.path");
        py.import("sys")?
            .getattr("path")?
            .downcast_into::<PyList>()
            .unwrap()
            .insert(0, config_dir().to_string_lossy())?;

        trace!("loading built-in logging.py");
        PyModule::from_code(
            py,
            c_str!(include_str!("logging.py")),
            c_str!("logging.py"),
            c_str!("logging"),
        )
        .map(|_| ())?;

        trace!("loading built-in telnet_charset.py");
        PyModule::from_code(
            py,
            c_str!(include_str!("telnet_charset.py")),
            c_str!("telnet_charset.py"),
            c_str!("telnet_charset"),
        )
        .map(|_| ())?;

        trace!("loading built-in telnet_naws.py");
        PyModule::from_code(
            py,
            c_str!(include_str!("telnet_naws.py")),
            c_str!("telnet_naws.py"),
            c_str!("telnet_naws"),
        )
        .map(|_| ())?;

        trace!("loading built-in history.py");
        PyModule::from_code(
            py,
            c_str!(include_str!("history.py")),
            c_str!("history.py"),
            c_str!("history"),
        )
        .map(|_| ())?;

        trace!("loading built-in cmd_py.py");
        PyModule::from_code(
            py,
            c_str!(include_str!("cmd_py.py")),
            c_str!("cmd_py.py"),
            c_str!("cmd_py"),
        )
        .map(|_| ())?;

        if args.headless {
            trace!("loading built-in headless.py");
            let module = PyModule::from_code(
                py,
                c_str!(include_str!("headless.py")),
                c_str!("headless.py"),
                c_str!("headless"),
            )?;

            let setup_fn = module.getattr("setup").unwrap();
            require_coroutine(py, format!("{module} setup"), setup_fn.as_ref())?;
            py_futures.push(Box::pin(pyo3_async_runtimes::tokio::into_future(
                setup_fn.call0()?,
            )?));
        } else {
            trace!("loading built-in tui.py");
            PyModule::from_code(
                py,
                c_str!(include_str!("tui.py")),
                c_str!("tui.py"),
                c_str!("tui"),
            )
            .map(|_| ())?;
        }
        Ok::<(), Error>(())
    })?;

    while let Some(result) = py_futures.next().await {
        if let Err(err) = result {
            error!("python setup error: {err}");
        }
    }

    Ok(())
}

#[instrument(level = Level::DEBUG, skip(config))]
pub(super) async fn run_user_setup(config: Config) -> Result {
    let mut py_futures = FuturesUnordered::new();

    Python::with_gil(|py| {
        for module in &config.modules {
            debug!(module, "loading");
            let module = PyModule::import(py, module)?;

            if let Ok(setup_fn) = module.getattr("setup") {
                require_coroutine(py, format!("{module} setup"), setup_fn.as_ref())?;
                let setup_future = pyo3_async_runtimes::tokio::into_future(setup_fn.call0()?)?;
                py_futures.push(Box::pin(setup_future));
            }
        }
        Ok::<(), PyErr>(())
    })?;

    while let Some(result) = py_futures.next().await {
        if let Err(err) = result {
            // Note: Error::from() to collect backtrace from PyErr.
            error!("python config module setup error: {}", Error::from(err));
        }
    }

    Ok(())
}

#[instrument(level = Level::DEBUG, fields(character = character.name))]
pub(super) fn run_character_setup(id: u32, character: &Character) -> Result<Option<Py<PyModule>>> {
    let Some(module) = &character.module else {
        return Ok(None);
    };

    let (future, module) = Python::with_gil(|py| {
        let module = PyModule::import(py, module)?;

        let future = if let Ok(setup_fn) = module.getattr("setup") {
            require_coroutine(py, format!("{module} setup"), setup_fn.as_ref())?;
            Some(pyo3_async_runtimes::tokio::into_future(setup_fn.call1(
                (Session {
                    id,
                    character: character.clone(),
                },),
            )?)?)
        } else {
            None
        };

        Ok::<_, Error>((future, module.unbind()))
    })?;

    if let Some(future) = future {
        let character_name = character.to_string();
        tokio::spawn(async move {
            if let Err(err) = future.await {
                // Note: Error::from() to collect backtrace from PyErr.
                error!(
                    "character {character_name} setup error: {}",
                    Error::from(err)
                );
            }
        });
    }

    Ok(Some(module))
}

pub(super) fn require_coroutine(
    py: Python<'_>,
    typ: impl Display,
    callback: &PyObject,
) -> PyResult<()> {
    // TODO(XXX): possible optimization - cache ref to this fn?
    let iscoroutinefunction = py
        .import("inspect")?
        .getattr("iscoroutinefunction")?
        .downcast_into::<PyFunction>()
        .map_err(|e| {
            PyRuntimeError::new_err(format!("getting inspect iscoroutinefunction: {e}"))
        })?;

    let is_coroutine = iscoroutinefunction
        .call1((callback,))?
        .downcast::<PyBool>()?
        .is_true();

    match is_coroutine {
        true => Ok(()),
        false => Err(PyTypeError::new_err(format!(
            "{typ} handler must be a coroutine function"
        ))),
    }
}

pub(super) fn label_for_coroutine(py: Python<'_>, callback: &PyObject) -> Option<String> {
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

pub(super) type PyFuture = Pin<Box<dyn StdFuture<Output = PyResult<PyObject>> + Send + 'static>>;

pub(crate) static APP: GILOnceCell<UnboundedSender<Command>> = GILOnceCell::new();

type Result<T = ()> = std::result::Result<T, Error>;
type FutureResult<'a> = PyResult<Bound<'a, PyAny>>;
