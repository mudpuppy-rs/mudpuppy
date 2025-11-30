use std::fs::{self, File};

use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::cli;
use crate::config::{self, CRATE_NAME};
use crate::error::{Error, ErrorKind};

pub(super) fn init(args: &cli::Args) -> Result<(), Error> {
    let data_dir = config::data_dir();
    fs::create_dir_all(data_dir).map_err(ErrorKind::from)?;

    let log_file =
        File::create(data_dir.join(format!("{CRATE_NAME}.log"))).map_err(ErrorKind::from)?;

    let env_filter = EnvFilter::builder()
        .with_default_directive(args.log_level.into())
        .from_env()
        .map_err(|e| ErrorKind::Cli(format!("invalid environment log level: {e}")))?;

    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_writer(log_file)
        .with_file(true)
        .with_line_number(true)
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
