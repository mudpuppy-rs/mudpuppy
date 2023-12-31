use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{instrument, trace, Level};

use crate::net::connection;

#[derive(Debug)]
pub(super) struct PromptFlusher {
    pub(super) send_flush: Arc<AtomicBool>,
    pub(super) extend_timeout: Arc<Notify>,
    handle: JoinHandle<()>,
}

impl PromptFlusher {
    #[instrument(level = Level::TRACE, skip(action_tx))]
    pub(super) fn new(
        action_tx: UnboundedSender<connection::Action>,
        flush_after: Duration,
    ) -> Self {
        let send_flush = Arc::new(AtomicBool::new(true));
        let extend_timeout = Arc::new(Notify::new());

        trace!("spawning flusher task");
        let handle = tokio::spawn(flusher_task(
            send_flush.clone(),
            extend_timeout.clone(),
            action_tx,
            flush_after,
        ));

        Self {
            send_flush,
            extend_timeout,
            handle,
        }
    }

    #[instrument(level = Level::TRACE, skip(self))]
    pub(super) fn stop(self) {
        trace!("aborting flusher task");
        self.handle.abort();
    }

    #[instrument(level = Level::TRACE, skip(self))]
    pub(super) fn extend_timeout(&self) {
        self.send_flush.store(false, Ordering::SeqCst);
        self.extend_timeout.notify_one();
    }
}

#[instrument(level = Level::TRACE, skip(extend_timeout, action_tx))]
async fn flusher_task(
    send_flush: Arc<AtomicBool>,
    extend_timeout: Arc<Notify>,
    action_tx: UnboundedSender<connection::Action>,
    flush_after: Duration,
) {
    loop {
        // Wait for either the timeout to expire or a notification to reset the timeout
        tokio::select! {
            () = tokio::time::sleep(flush_after) => {
                // Check the flag to see if the timeout should still be considered expired
                if send_flush.load(Ordering::SeqCst) {
                    trace!("timeout expired, sending flush message");
                    if action_tx.send(connection::Action::Flush).is_err() {
                        break;
                    }
                    send_flush.store(false, Ordering::SeqCst);
                }
            },
            () = extend_timeout.notified() => {
                if !send_flush.swap(true, Ordering::SeqCst) {
                    trace!("timeout reset");
                }
            }
        }
    }
}
