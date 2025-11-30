mod app;
mod cli;
mod config;
mod error;
mod headless;
mod keyboard;
mod logging;
mod net;
mod panic;
mod python;
mod session;
mod shortcut;
mod slash_command;
mod tui;

use std::process::exit;

use clap::Parser;
use pyo3::Python;
use pyo3_async_runtimes::tokio as pyo3tokio;
use tokio::runtime;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{error, info, instrument};

use crate::error::{Error, ErrorKind};
use app::App;
use config::{CRATE_NAME, Config};
use python::{APP, pup};

fn main() -> Result<(), Error> {
    #[instrument(skip(args))]
    async fn main(args: cli::Args) -> Result<(), Error> {
        info!(args=?args, "starting app");
        pyo3_pylogger::register(CRATE_NAME);

        let (py_tx, py_rx) = unbounded_channel();
        Python::attach(|py| {
            APP.set(py, py_tx).unwrap();
        });

        let config = Config::new().map_err(ErrorKind::from)?;

        App::new(args, &config)?.run(py_rx).await
    }

    panic::install_handler();

    let args = cli::Args::parse();
    logging::init(&args)?;

    pyo3::append_to_inittab!(pup);
    Python::initialize();

    let mut builder = runtime::Builder::new_multi_thread();
    builder.enable_all();
    pyo3tokio::init(builder);

    let clean_exit = Python::attach(|py| {
        // pyo3tokio::run constrains the closure to return a PyResult<T>.
        // Converting our nice error type w/ tracing context into a Python
        // error is too lossy, and renders poorly when returned from main.
        //
        // Instead, we handle logging/outputting the top-level err ourselves
        // and only return an indicator of whether to exit cleanly or not
        // from the runtime closure future completing.
        pyo3tokio::run(py, async move {
            if let Err(e) = main(args).await {
                let _ = panic::restore_terminal();
                error!("{e}");
                // printing to stderr is fine, we've restored the term out of alt mode.
                eprintln!("{e}");
                return Ok(false);
            }
            Ok(true)
        })
    })?;
    match clean_exit {
        true => Ok(()),
        false => exit(1),
    }
}
