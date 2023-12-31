use std::time::Duration;

use clap::Parser;
use tracing::level_filters::LevelFilter;

use crate::config::version;
use crate::error::Error;
use crate::Result;

#[derive(Parser, Debug)]
#[command(author, version = version(), about)]
pub struct Args {
    #[arg(
        short,
        long,
        value_name = "FLOAT",
        help = "Frame rate, i.e. number of frames per second",
        default_value_t = 60.0
    )]
    pub frame_rate: f64,

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
        help = "Log level filter. Default is INFO"
    )]
    pub log_level: LevelFilter,
}

impl Args {
    /// # Errors
    /// If the frame rate is not between 0 and 120
    pub fn frame_rate_duration(&self) -> Result<Duration> {
        if self.frame_rate <= 0.0 || self.frame_rate > 120.0 {
            return Err(Error::Cli(format!(
                "frame_rate must be between 0 and 120. Provided: {}",
                self.frame_rate
            )));
        }

        // Safe based on bounds above.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Ok(Duration::from_micros(
            (1_000_000.0 / self.frame_rate) as u64,
        ))
    }
}
