mod app;
mod config;
mod error;
mod keyboard;
mod logging;
mod net;
mod panic;
mod python;
mod session;

use std::error::Error as StdError;
use std::io::{stdout, IsTerminal};

use pyo3::{PyResult, Python};
use pyo3_async_runtimes::tokio as pyo3tokio;
use tokio::runtime;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{error, info, instrument};

use app::App;
use config::Config;
use python::{pup, APP};

fn main() -> Result<(), Box<dyn StdError>> {
    #[instrument(skip(config))]
    async fn main(config: Config) -> PyResult<()> {
        info!("starting app");
        pyo3_pylogger::register("pup");

        let (py_tx, py_rx) = unbounded_channel();
        Python::with_gil(|py| {
            APP.set(py, py_tx).unwrap();
        });

        App::new(config).run(py_rx).await.map_err(Into::into)
    }

    let config = Config::new()?;

    logging::init()?;
    panic::init();

    if !IsTerminal::is_terminal(&stdout()) {
        let msg =
            "pup is a TUI application that can only be run when STDOUT is a regular terminal.";
        error!("{msg}");
        return Err(msg.into());
    }

    pyo3::append_to_inittab!(pup);
    pyo3::prepare_freethreaded_python();

    let mut builder = runtime::Builder::new_multi_thread();
    builder.enable_all();
    pyo3tokio::init(builder);

    match Python::with_gil(|py| pyo3tokio::run(py, main(config))) {
        Ok(()) => Ok(()),
        Err(e) => {
            // We want to invoke the panic handler explicitly - returning Err(e) will not.
            panic!("{}", e)
        }
    }
}
