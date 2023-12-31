use std::fmt::{Debug, Formatter};
use std::ops::ControlFlow;

use futures::{SinkExt, StreamExt};
use tokio::select;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio_util::bytes::Bytes;
use tokio_util::codec::Framed;
use tracing::{instrument, trace, Level};

use crate::error::Error;
use crate::model::{Mud, SessionId};
use crate::net::stream::{self, Stream};
use crate::net::telnet;

/// Connect to a MUD server, spawning a connection task that sends events on the given channel.
///
/// A [`Handle`] for managing the connection and [`stream::Info`] describing the connection
/// are returned on success.
///
/// # Errors
///
/// Returns an error if it isn't possible to connect to the specified MUD server.
pub async fn connect(
    session: SessionId,
    mud: &Mud,
    event_tx: UnboundedSender<Event>,
) -> Result<(Handle, stream::Info), Error> {
    let stream = Stream::connect(mud).await?;
    let info: stream::Info = (&stream).into();
    let (action_tx, action_rx) = unbounded_channel();

    let codec = telnet::codec::Codec::default();
    let task = tokio::spawn(
        Connection {
            session,
            stream: Framed::with_capacity(stream, codec, 32_768), // 32 KiB
            event_tx,
        }
        .io_loop(action_rx),
    );

    Ok((
        Handle {
            session,
            task,
            action_tx,
        },
        info,
    ))
}

/// A handle to an active MUD server connection.
pub struct Handle {
    /// The session ID of the connection.
    pub session: SessionId,

    /// A task that can be joined to await the completion of the connection.
    pub task: JoinHandle<Result<(), Error>>,

    pub action_tx: UnboundedSender<Action>,
}

impl Handle {
    /// Sends an action to the connection.
    ///
    /// # Errors
    /// Returns an error if the connection's action transmission channel is already closed.
    /// This should not happen in normal circumstances.
    pub fn send(&self, action: Action) -> Result<(), Error> {
        self.action_tx
            .send(action)
            .map_err(|_| Error::Internal(format!("{} handle tx channel closed", self.session)))
    }
}

impl Debug for Handle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionHandle")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

/// A connection action.
#[derive(Debug)]
pub enum Action {
    /// Terminate the connection.
    Disconnect,

    /// Send the provided item over the connection.
    Send(telnet::codec::Item),

    /// Flush any partially buffered line content.
    Flush,
}

impl From<telnet::codec::Item> for Action {
    fn from(item: telnet::codec::Item) -> Self {
        Action::Send(item)
    }
}

/// A connection event.
#[derive(Debug)]
pub struct Event {
    pub session_id: SessionId,
    pub event: SessionEvent,
}

#[derive(Debug)]
pub enum SessionEvent {
    /// The connection has disconnected.
    Disconnected,

    Error(Error),

    /// A telnet protocol event.
    Telnet(telnet::codec::Item),

    /// Partial line content that was flushed from the line buffer.
    PartialLine(Bytes),
}

/// An active connection to a MUD server.
///
/// Represents a TCP stream (that may or may not be TLS encrypted), that is then
/// framed according to the telnet protocol.
///
/// Operates as an "Actor", receiving action messages from a handle over a `action_rx`
/// channel and dispatching connection events over a `event_tx` channel.
struct Connection {
    session: SessionId,
    stream: Framed<Stream, telnet::codec::Codec>,
    event_tx: UnboundedSender<Event>,
}

impl Connection {
    #[instrument(level = Level::TRACE, skip(self, action_rx), fields(self.session = %self.session))]
    async fn io_loop(mut self, mut action_rx: UnboundedReceiver<Action>) -> Result<(), Error> {
        trace!("connection i/o loop starting");
        loop {
            let select_res = select! {
                cf = self.stream_read() => {
                    cf
                },
                action = action_rx.recv() => {
                    if let Some(action) = action { self.handle_action(action).await } else {
                        trace!("action rx closed");
                        ControlFlow::Break(None)
                    }
                }
            };
            match select_res {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(Some(err)) => {
                    trace!("breaking from select! due to err: {err:?}");
                    let msg = format!("connection i/o loop: {err}");
                    self.event_tx.send(Event {
                        session_id: self.session,
                        event: SessionEvent::Error(err),
                    })?;
                    return Err(Error::Internal(msg));
                }
                ControlFlow::Break(None) => {
                    trace!("breaking from select! for normal close");
                    break;
                }
            }
        }
        self.event_tx.send(Event {
            session_id: self.session,
            event: SessionEvent::Disconnected,
        })?;
        trace!("connection i/o loop finished");
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(self.session = %self.session))]
    async fn stream_read(&mut self) -> ControlFlow<Option<Error>> {
        while let Some(item) = self.stream.next().await {
            let item = match item {
                Ok(item) => item,
                Err(err) => return ControlFlow::Break(Some(err)),
            };

            if let Err(err) = self.emit_event(SessionEvent::Telnet(item)) {
                return ControlFlow::Break(Some(err));
            }
        }

        trace!("stream ended - breaking control flow");
        ControlFlow::Break(None)
    }

    #[instrument(skip(self))]
    async fn handle_action(&mut self, action: Action) -> ControlFlow<Option<Error>> {
        match action {
            Action::Disconnect => ControlFlow::Break(None),
            Action::Send(item) => self.telnet_write(item).await,
            Action::Flush => {
                trace!("flushing line buffer....");
                let Some(partial_line) = self.stream.codec_mut().partial_line() else {
                    return ControlFlow::Continue(());
                };
                let stripped = strip_ansi_escapes::strip(partial_line.clone());
                if stripped.is_empty() {
                    return ControlFlow::Continue(());
                }
                match self.emit_event(SessionEvent::PartialLine(partial_line)) {
                    Err(err) => ControlFlow::Break(Some(err)),
                    Ok(()) => ControlFlow::Continue(()),
                }
            }
        }
    }

    async fn telnet_write(&mut self, item: telnet::codec::Item) -> ControlFlow<Option<Error>> {
        match self.stream.send(item).await {
            Ok(()) => ControlFlow::Continue(()),
            Err(err) => ControlFlow::Break(Some(err)),
        }
    }

    fn emit_event(&self, event: SessionEvent) -> Result<(), Error> {
        self.event_tx
            .send(Event {
                session_id: self.session,
                event,
            })
            .map_err(Into::into)
    }
}
