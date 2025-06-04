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
mod slash_command;
mod tui;

use std::error::Error as StdError;

use clap::Parser;
use pyo3::Python;
use pyo3_async_runtimes::tokio as pyo3tokio;
use tokio::runtime;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{error, info, instrument};

use crate::error::Error;
use app::App;
use config::{CRATE_NAME, Config};
use python::{APP, pup};

fn main() -> Result<(), Box<dyn StdError>> {
    #[instrument(skip(args))]
    async fn main(args: cli::Args) -> Result<(), Error> {
        info!(args=?args, "starting app");
        pyo3_pylogger::register(CRATE_NAME);

        let (py_tx, py_rx) = unbounded_channel();
        Python::with_gil(|py| {
            APP.set(py, py_tx).unwrap();
        });

        let config = Config::new()?;

        App::new(args, &config)?.run(py_rx).await
    }

    panic::install_handler();

    let args = cli::Args::parse();
    logging::init(&args)?;

    pyo3::append_to_inittab!(pup);
    pyo3::prepare_freethreaded_python();

    let mut builder = runtime::Builder::new_multi_thread();
    builder.enable_all();
    pyo3tokio::init(builder);

    Python::with_gil(|py| {
        pyo3tokio::run(py, async move {
            if let Err(e) = main(args).await {
                error!("{e}");
                let _ = panic::restore_terminal();
                return Err(e.into());
            }
            Ok(())
        })
    })?;
    Ok(())
}
