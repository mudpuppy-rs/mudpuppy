use clap::Parser;
use std::time::Duration;
use tracing::level_filters::LevelFilter;

use crate::config::version;
use crate::error::{Error, ErrorKind};

#[derive(Debug, Clone, Parser)]
#[command(author, about, version = version())]
pub struct Args {
    #[arg(
        short,
        long,
        value_name = "MUD_NAME",
        help = "MUD name to auto-connect to at startup. Can be specified multiple times"
    )]
    pub connect: Vec<String>,

    #[arg(
        short,
        long,
        value_name = "LEVEL",
        default_value = "INFO",
        help = "Log level filter."
    )]
    pub log_level: LevelFilter,

    #[arg(long, default_value = "false", help = "Run in headless mode (no TUI).")]
    pub headless: bool,

    #[arg(
        short,
        long,
        value_name = "FLOAT",
        help = "Frame rate, i.e. number of frames per second. Ignored with --headless.",
        default_value_t = 60.0
    )]
    pub frame_rate: f64,
    // If you add new CLI args, don't forget to update `user-guide/src/cli.md`.
}

impl Args {
    /// # Errors
    /// If the frame rate is not between 0 and 120
    pub fn frame_rate_duration(&self) -> Result<Duration, Error> {
        if self.frame_rate <= 0.0 || self.frame_rate > 120.0 {
            return Err(ErrorKind::Cli(format!(
                "frame_rate must be between 0 and 120. Provided: {}",
                self.frame_rate
            ))
            .into());
        }

        // Safety: range is checked before this point.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Ok(Duration::from_micros(
            (1_000_000.0 / self.frame_rate) as u64,
        ))
    }
}
