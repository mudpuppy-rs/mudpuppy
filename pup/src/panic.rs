use std::error::Error as StdError;
use std::io::stdout;
use std::{panic, process};

use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use tracing::error;

pub(super) fn init() {
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

fn restore_terminal() -> Result<(), Box<dyn StdError>> {
    disable_raw_mode()?;
    stdout().execute(crossterm::event::DisableMouseCapture)?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
