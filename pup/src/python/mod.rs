mod api;
mod command;
mod events;

use std::future::Future as StdFuture;
use std::pin::Pin;

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use pyo3::ffi::c_str;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyAnyMethods, PyList, PyListMethods, PyModule};
use pyo3::{Bound, PyAny, PyErr, PyObject, PyResult, Python};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, instrument, trace};

use crate::config::config_dir;
use crate::error::Error;

pub(crate) use api::*;
pub(crate) use command::*;
pub(crate) use events::*;

pub(super) type PyFuture = Pin<Box<dyn StdFuture<Output = PyResult<PyObject>> + Send + 'static>>;

#[instrument]
pub(super) async fn init_python_env() -> Result {
    Python::with_gil(|py| {
        trace!("adding config dir to sys.path");
        py.import("sys")?
            .getattr("path")?
            .downcast_into::<PyList>()
            .unwrap()
            .insert(0, config_dir())?;

        trace!("loading built-in setup.py");
        let _ = PyModule::from_code(
            py,
            c_str!(include_str!("setup.py")),
            c_str!("setup.py"),
            c_str!("setup"),
        )?;

        Ok::<(), PyErr>(())
    })?;

    Ok(())
}

// TODO(XXX): load more than just pup_test...
#[instrument]
pub(super) async fn run_user_setup() -> Result {
    let mut py_futures = FuturesUnordered::new();

    Python::with_gil(|py| {
        trace!("loading user pup_test.py");
        let module = PyModule::import(py, "pup_test")?;
        if let Ok(setup_fn) = module.getattr("setup") {
            let setup_future = pyo3_async_runtimes::tokio::into_future(setup_fn.call0()?)?;
            py_futures.push(Box::pin(setup_future));
        }

        Ok::<(), PyErr>(())
    })?;

    while let Some(result) = py_futures.next().await {
        if let Err(err) = result {
            error!("python setup error: {err}");
        }
    }

    Ok(())
}

pub(crate) static APP: GILOnceCell<UnboundedSender<Command>> = GILOnceCell::new();

type Result<T = ()> = std::result::Result<T, Error>;
type FutureResult<'a> = PyResult<Bound<'a, PyAny>>;
