use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pyo3::{pyclass, pymethods};
use strum::Display;
use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tracing::{Level, debug, instrument, trace, warn};

use crate::error::{Error, ErrorKind};
use crate::net::connection;
use crate::python;

#[derive(Debug)]
pub(crate) struct Prompt {
    id: u32,
    mode: PromptMode,
    flusher: Option<PromptFlusher>,
    content: String,
    py_event_tx: UnboundedSender<(u32, python::Event)>,
    conn_action_tx: Option<UnboundedSender<connection::Action>>,
}

impl Prompt {
    #[instrument(level = Level::TRACE, skip(self), fields(id = self.id, content = self.content, new_content = content))]
    pub(crate) fn set_content(&mut self, content: String) -> Result<String, Error> {
        if self.content == content {
            trace!("skipping no-change content update");
            return Ok(content);
        }
        debug!("prompt changed");

        let old_content = mem::replace(&mut self.content, content.clone());
        self.py_event_tx
            .send((
                self.id,
                python::Event::PromptChanged {
                    from: old_content.clone(),
                    to: content,
                },
            ))
            .map_err(ErrorKind::from)?;

        Ok(old_content)
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id = self.id, ?mode = self.mode, ?new_mode = mode))]
    pub(crate) fn set_mode(&mut self, mode: PromptMode) -> Result<PromptMode, Error> {
        let had_flusher = self.flusher().is_some();
        let old_mode = mem::replace(&mut self.mode, mode.clone());
        if let Some(old_flusher) = self.flusher.take() {
            old_flusher.stop();
        }

        if let Some(conn_action_tx) = &self.conn_action_tx {
            self.init_flusher(conn_action_tx.clone());
        }

        if let Some(conn_action_tx) = &self.conn_action_tx {
            // If we switched from an unsignalled mode to a signalled mode, we need to
            // schedule one flush to ensure we don't miss any data that was in the buffer
            // from before.
            if had_flusher && self.flusher.is_none() {
                let tx = conn_action_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    trace!("one time signalled prompt flush running");
                    if let Err(err) = tx.send(connection::Action::Flush) {
                        warn!(err=?err, "failed to send prompt flush");
                    }
                });
            }
        }

        self.py_event_tx
            .send((
                self.id,
                python::Event::PromptModeChanged {
                    from: old_mode.clone(),
                    to: mode,
                },
            ))
            .map_err(ErrorKind::from)?;

        Ok(old_mode)
    }

    pub(crate) fn content(&self) -> &str {
        &self.content
    }

    pub(crate) fn mode(&self) -> &PromptMode {
        &self.mode
    }

    pub(super) fn new(id: u32, py_event_tx: UnboundedSender<(u32, python::Event)>) -> Self {
        Self {
            id,
            mode: PromptMode::default(),
            flusher: None, // No flusher until connect()
            content: String::new(),
            py_event_tx,
            conn_action_tx: None,
        }
    }

    pub(super) fn init_flusher(&mut self, conn_action_tx: UnboundedSender<connection::Action>) {
        if self.flusher.is_some() {
            trace!("flusher already initialized");
            return;
        }
        self.flusher = self.mode.flusher(conn_action_tx.clone());
        self.conn_action_tx = Some(conn_action_tx);
    }

    pub(super) fn flusher(&self) -> Option<&PromptFlusher> {
        self.flusher.as_ref()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Display)]
#[pyclass]
pub enum PromptMode {
    // When dealing with a MUD that doesn't explicitly terminate prompts in some way, we can end up
    // with data in the buffer after deframing that may or may not be a prompt.
    //
    // If it isn't a prompt, we expect to receive more data that will have a line ending Soon(TM).
    // If it is a prompt, we won't get anything else; the game sent something like "Enter username: "
    // and is expecting the player to act before it will send any more data. There's no way to tell
    // the two apart definitively, so in this mode we use a heuristic: if we don't receive more data
    // and deframe a line before the Duration expires, consider what's in the buffer a prompt and flush
    // it as a received prompt line.
    #[strum(to_string = "unsignalled prompt mode ({timeout:?} flush timeout)")]
    Unsignalled { timeout: Duration },

    // Used for a MUD that signals prompts using EOR or GA.
    #[strum(to_string = "signalled prompt mode ({signal})")]
    Signalled { signal: PromptSignal },
}

impl PromptMode {
    pub(super) fn flusher(
        &self,
        conn_action_tx: UnboundedSender<connection::Action>,
    ) -> Option<PromptFlusher> {
        match self {
            PromptMode::Signalled { .. } => None,
            PromptMode::Unsignalled { timeout } => {
                Some(PromptFlusher::new(*timeout, conn_action_tx))
            }
        }
    }
}

#[pymethods]
impl PromptMode {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }

    #[must_use]
    pub fn signal(&self) -> Option<PromptSignal> {
        match self {
            PromptMode::Unsignalled { .. } => None,
            PromptMode::Signalled { signal } => Some(*signal),
        }
    }
}

impl Default for PromptMode {
    fn default() -> Self {
        Self::Unsignalled {
            timeout: Duration::from_millis(200),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Display)]
#[pyclass(eq, eq_int)]
pub enum PromptSignal {
    #[strum(message = "end of record (EoR)")]
    EndOfRecord,
    #[strum(message = "go ahead (GA)")]
    GoAhead,
}

impl From<PromptSignal> for u8 {
    fn from(value: PromptSignal) -> Self {
        use crate::net::telnet::command;
        match value {
            PromptSignal::EndOfRecord => command::EOR,
            PromptSignal::GoAhead => command::GA,
        }
    }
}

#[pymethods]
#[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
impl PromptSignal {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Debug)]
pub(super) struct PromptFlusher {
    send_flush: Arc<AtomicBool>,
    extend_timeout: Arc<Notify>,
    handle: JoinHandle<()>,
}

impl PromptFlusher {
    #[instrument(level = Level::TRACE, skip(conn_action_tx))]
    pub(super) fn new(
        flush_after: Duration,
        conn_action_tx: UnboundedSender<connection::Action>,
    ) -> Self {
        let send_flush = Arc::new(AtomicBool::new(true));
        let extend_timeout = Arc::new(Notify::new());

        trace!("spawning flusher task");
        let handle = tokio::spawn(flusher_task(
            send_flush.clone(),
            extend_timeout.clone(),
            conn_action_tx,
            flush_after,
        ));

        Self {
            send_flush,
            extend_timeout,
            handle,
        }
    }

    #[instrument(level = Level::TRACE, skip(self))]
    pub(crate) fn stop(self) {
        trace!("aborting flusher task");
        self.handle.abort();
    }

    #[instrument(level = Level::TRACE, skip(self))]
    pub(crate) fn extend_timeout(&self) {
        self.send_flush.store(false, Ordering::SeqCst);
        self.extend_timeout.notify_one();
    }
}

#[instrument(level = Level::TRACE, skip(extend_timeout, conn_action_tx))]
async fn flusher_task(
    send_flush: Arc<AtomicBool>,
    extend_timeout: Arc<Notify>,
    conn_action_tx: UnboundedSender<connection::Action>,
    flush_after: Duration,
) {
    loop {
        // Wait for either the timeout to expire or a notification to reset the timeout
        tokio::select! {
            () = tokio::time::sleep(flush_after) => {
                // Check the flag to see if the timeout should still be considered expired
                if !send_flush.load(Ordering::SeqCst) {
                    continue;
                }
                trace!("timeout expired, sending flush message");
                let _ = conn_action_tx.send(connection::Action::Flush);
                send_flush.store(false, Ordering::SeqCst);
            },
            () = extend_timeout.notified() => {
                if !send_flush.swap(true, Ordering::SeqCst) {
                    trace!("timeout reset");
                }
            }
        }
    }
}
