pub mod app;
pub mod cli;
pub mod client;
pub mod config;
pub mod error;
pub mod idmap;
pub mod model;
pub mod net;
pub mod python;
pub mod tui;

pub static CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

pub static GIT_COMMIT_HASH: &str = env!("MUDPUPPY_GIT_INFO");

pub type Result<T, E = error::Error> = core::result::Result<T, E>;
