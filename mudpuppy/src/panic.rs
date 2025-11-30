use std::io::{self, stdout};
use std::{panic, process};

// TODO(XXX): isolate crossterm bits to TUI mode.

use crossterm::ExecutableCommand;
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode};
use tracing::error;

pub(super) fn install_handler() {
    panic::set_hook(Box::new(panic_handler));
}

fn panic_handler(panic_info: &panic::PanicHookInfo) {
    if let Err(err) = restore_terminal() {
        error!(err=?err, "error restoring terminal");
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
}

pub(crate) fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(crossterm::event::DisableMouseCapture)?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
