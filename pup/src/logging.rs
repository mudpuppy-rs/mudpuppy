use std::error::Error as StdError;
use std::fs;

use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::{cli, config};

pub(super) fn init(args: &cli::Args) -> Result<(), Box<dyn StdError>> {
    let data_dir = config::data_dir();
    fs::create_dir_all(data_dir)?;
    let log_file = data_dir.join(format!("{}.log", config::CRATE_NAME));
    let log_file = fs::File::create(&log_file)?;

    let env_filter = EnvFilter::builder()
        .with_default_directive(args.log_level.into())
        .from_env()?;

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
