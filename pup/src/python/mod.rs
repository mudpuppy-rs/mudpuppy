mod api;
mod command;
mod events;

use std::fmt::Display;
use std::future::Future as StdFuture;
use std::pin::Pin;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use pyo3::exceptions::PyTypeError;
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

        // TODO(XXX): Load other built-in modules.
        trace!("loading built-in setup.py");
        PyModule::from_code(
            py,
            c_str!(include_str!("setup.py")),
            c_str!("setup.py"),
            c_str!("setup"),
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

            if let Ok(setup_fn) = module.getattr("setup") {
                let setup_future = pyo3_async_runtimes::tokio::into_future(setup_fn.call0()?)?;
                py_futures.push(Box::pin(setup_future));
            }
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

#[instrument(level = Level::DEBUG)]
pub(super) fn run_character_setup(id: u32, character: &Character) -> Result<Option<Py<PyModule>>> {
    let Some(module) = &character.module else {
        return Ok(None);
    };

    let (future, module) = Python::with_gil(|py| {
        let module = PyModule::import(py, module)?;

        let future = pyo3_async_runtimes::tokio::into_future(module.call_method1(
            "setup",
            (Session {
                id,
                character: character.clone(),
            },),
        )?)?;

        Ok::<_, Error>((future, module.unbind()))
    })?;

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
        .map_err(|_| Error::Internal("getting inspect iscoroutinefunction".to_string()))?;

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

pub(super) type PyFuture = Pin<Box<dyn StdFuture<Output = PyResult<PyObject>> + Send + 'static>>;

pub(crate) static APP: GILOnceCell<UnboundedSender<Command>> = GILOnceCell::new();

type Result<T = ()> = std::result::Result<T, Error>;
type FutureResult<'a> = PyResult<Bound<'a, PyAny>>;
