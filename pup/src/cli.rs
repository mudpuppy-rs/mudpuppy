use clap::Parser;
use tracing::level_filters::LevelFilter;

// TODO(XXX): Version from build.rs
#[derive(Parser, Debug)]
#[command(author, about)]
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
    // If you add new CLI args, don't forget to update `user-guide/src/cli.md`.
}
