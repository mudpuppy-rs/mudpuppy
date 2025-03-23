mod app;
mod cli;
mod config;
mod error;
mod keyboard;
mod logging;
mod net;
mod panic;
mod python;
mod session;
mod slash_command;

use std::error::Error as StdError;
use std::io::{stdout, IsTerminal};

use clap::Parser;
use pyo3::{PyResult, Python};
use pyo3_async_runtimes::tokio as pyo3tokio;
use tokio::runtime;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{error, info, instrument};

use app::App;
use config::{Config, CRATE_NAME};
use python::{pup, APP};

fn main() -> Result<(), Box<dyn StdError>> {
    #[instrument(skip(args, config))]
    async fn main(args: cli::Args, config: Config) -> PyResult<()> {
        info!(args=?args, "starting app");
        pyo3_pylogger::register("pup");

        let (py_tx, py_rx) = unbounded_channel();
        Python::with_gil(|py| {
            APP.set(py, py_tx).unwrap();
        });

        App::new(args, config).run(py_rx).await.map_err(Into::into)
    }

    panic::init();

    let args = cli::Args::parse();
    logging::init(&args)?;

    if !args.headless && !IsTerminal::is_terminal(&stdout()) {
        let msg = format!(
            "{CRATE_NAME} is a TUI application that can only be run when STDOUT is a regular terminal.");
        error!("{msg}");
        return Err(msg.into());
    }

    pyo3::append_to_inittab!(pup);
    pyo3::prepare_freethreaded_python();

    let mut builder = runtime::Builder::new_multi_thread();
    builder.enable_all();
    pyo3tokio::init(builder);

    let config = Config::new()?;
    match Python::with_gil(|py| pyo3tokio::run(py, main(args, config))) {
        Ok(()) => Ok(()),
        Err(e) => {
            // We want to invoke the panic handler explicitly - returning Err(e) will not.
            panic!("{}", e)
        }
    }
}
