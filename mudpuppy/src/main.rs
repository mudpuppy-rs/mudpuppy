use std::error::Error as StdError;
use std::io::{self, IsTerminal};

use clap::Parser;
use pyo3::exceptions::PyRuntimeError;
use pyo3::{PyResult, Python};
use pyo3_async_runtimes::tokio as pyo3tokio;
use tokio::runtime;
use tracing::{error, info, instrument};

use mudpuppy::app::App;
use mudpuppy::cli;
use mudpuppy::config::{self, GlobalConfig};
use mudpuppy::python;

fn main() -> Result<(), Box<dyn StdError>> {
    // Note: we can't use the pyo3_asyncio main fn macro because it won't
    //  allow us to append to the init tab before the free-threaded python
    //  environment is initialized. We can't do it after that point, so
    //  do all the macro ceremony ourselves by hand.
    #[instrument]
    async fn main() -> PyResult<()> {
        if !IsTerminal::is_terminal(&io::stdout()) {
            return Err(PyRuntimeError::new_err(format!(
                "{} is a TUI application that can only be run when STDOUT is a regular terminal.",
                mudpuppy::CRATE_NAME
            )));
        }

        let args = cli::Args::parse();

        config::init_logging(&args)?;
        config::init_panic_handler();

        info!("{} {}", mudpuppy::CRATE_NAME, mudpuppy::GIT_COMMIT_HASH);

        info!("loading configuration");
        let config = GlobalConfig::new()?;

        info!("starting app");
        let mut app = App::new(config);
        let res = app.run(args).await;

        if let Err(err) = &res {
            error!("exiting with error: {}", err);
        }

        info!("goodbye!");
        res.map_err(Into::into)
    }

    // Put the mudpuppy module into the inittab so that it can be imported
    // by our internal modules.
    use python::mudpuppy_core;
    pyo3::append_to_inittab!(mudpuppy_core);
    pyo3::prepare_freethreaded_python();

    let mut builder = runtime::Builder::new_multi_thread();
    builder.enable_all();
    pyo3tokio::init(builder);
    Python::with_gil(|py| pyo3tokio::run(py, main()))?;
    Ok(())
}
