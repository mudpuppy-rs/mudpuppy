use std::{fs, panic, process};

use tracing::error;
use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::app::restore_terminal;
use crate::config::{config_dir, data_dir};
use crate::error::{ConfigError, Error};
use crate::{cli, Result, CRATE_NAME};

/// Set up logging to a log file in the data directory.
///
/// By default, no logging is done to STDOUT/STDERR - this would corrupt a TUI
/// application.
///
/// By default, only `INFO` level log lines and above are written to the log file,
/// and ANSI will be enabled. The log filter level can be adjusted using the
/// normal `RUST_LOG` environment variable semantics.
///
/// If the optional `console-subscriber` dependency is enabled the application
/// will be configured for `tokio-console`.
///
/// # Errors
///
/// If the data directory can't be created, or the log file can't be created,
/// or the `RUST_LOG` environment variable is invalid, this function will return
/// an error result.
#[allow(clippy::module_name_repetitions)]
pub fn init_logging(args: &cli::Args) -> Result<()> {
    let data_dir = data_dir();
    fs::create_dir_all(data_dir).map_err(|e| {
        Error::Config(ConfigError::Logging(format!(
            "creating data dir {:?}: {e}",
            data_dir.display()
        )))
    })?;
    let config_dir = config_dir();
    fs::create_dir_all(config_dir).map_err(|e| {
        Error::Config(ConfigError::Logging(format!(
            "creating config dir {:?}: {e}",
            config_dir.display()
        )))
    })?;

    let log_file = data_dir.join(format!("{CRATE_NAME}.log"));
    let log_file = fs::File::create(&log_file).map_err(|e| {
        Error::Config(ConfigError::Logging(format!(
            "creating log file {:?}: {e}",
            log_file.display()
        )))
    })?;
    let env_filter = EnvFilter::builder()
        .with_default_directive(args.log_level.into())
        .from_env()
        .map_err(|e| {
            Error::Config(ConfigError::Logging(format!(
                "invalid RUST_LOG env var filter config: {e}"
            )))
        })?;

    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(true)
        .with_filter(env_filter);

    let registry = tracing_subscriber::registry();
    #[cfg(feature = "console-subscriber")]
    let registry = registry.with(console_subscriber::spawn());

    registry
        .with(file_subscriber)
        .with(ErrorLayer::default())
        .init();

    Ok(())
}

pub fn init_panic_handler() {
    panic::set_hook(Box::new(move |panic_info| {
        if let Err(err) = restore_terminal() {
            error!("error restoring terminal: {}", err);
        }
        #[cfg(not(debug_assertions))]
        {
            use human_panic::{handle_dump, metadata, print_msg};
            let meta = metadata!();
            print_msg(handle_dump(&meta, panic_info), &meta)
                .expect("human-panic: printing error message to console failed");
        }
        #[cfg(debug_assertions)]
        {
            better_panic::Settings::auto()
                .most_recent_first(false)
                .lineno_suffix(true)
                .verbosity(better_panic::Verbosity::Full)
                .create_panic_handler()(panic_info);
        }
        error!("panic: {panic_info}");
        process::exit(1);
    }));
}
