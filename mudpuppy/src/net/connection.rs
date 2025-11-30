use std::fmt::{Debug, Formatter};
use std::ops::ControlFlow;

use futures::{SinkExt, StreamExt};
use tokio::select;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tokio_util::bytes::Bytes;
use tokio_util::codec::Framed;
use tracing::{Level, error, instrument, trace};

use crate::error::{Error, ErrorKind};
pub use crate::net::stream::Info;
use crate::net::stream::Stream;
use crate::net::telnet;
use crate::session::Mud;

/// A handle to an active MUD server connection.
pub struct Handle {
    /// The session ID of the connection.
    pub session: u32,

    pub action_tx: UnboundedSender<Action>,

    _task: JoinHandle<Result<(), Error>>,
}

impl Handle {
    pub fn new(session: u32, mud: Mud, event_tx: UnboundedSender<Event>) -> Self {
        let (action_tx, action_rx) = unbounded_channel();

        Self {
            session,
            _task: Connection::spawn(session, mud, action_rx, event_tx),
            action_tx,
        }
    }

    pub fn send(&self, action: impl Into<Action>) -> Result<(), Error> {
        Ok(self
            .action_tx
            .send(action.into())
            .map_err(ErrorKind::from)?)
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
    pub session_id: u32,
    pub event: SessionEvent,
}

#[derive(Debug)]
pub enum SessionEvent {
    /// The session is connected.
    Connected(Info),

    /// The connection has disconnected.
    Disconnected,

    Error(Error),

    /// A low-level telnet protocol event.
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
/// channel and dispatching connection events over an ` event_tx ` channel.
struct Connection {
    session_id: u32,
    stream: Framed<Stream, telnet::codec::Codec>,
    event_tx: UnboundedSender<Event>,
}

impl Connection {
    #[instrument(level = Level::TRACE, skip(action_rx, event_tx))]
    fn spawn(
        session_id: u32,
        mud: Mud,
        action_rx: UnboundedReceiver<Action>,
        event_tx: UnboundedSender<Event>,
    ) -> JoinHandle<Result<(), Error>> {
        tokio::spawn(async move {
            let stream = match Stream::connect(&mud).await {
                Ok(stream) => stream,
                Err(err) => {
                    error!("connection error: {err}");
                    event_tx
                        .send(Event {
                            session_id,
                            event: SessionEvent::Error(err),
                        })
                        .map_err(ErrorKind::from)?;
                    return Ok(());
                }
            };

            // 32 KiB capacity
            let stream = Framed::with_capacity(stream, telnet::codec::Codec::new(), 32_768);

            event_tx
                .send(Event {
                    session_id,
                    event: SessionEvent::Connected(stream.get_ref().info()),
                })
                .map_err(ErrorKind::from)?;

            Connection {
                session_id,
                stream,
                event_tx,
            }
            .io_loop(action_rx)
            .await
        })
    }

    #[instrument(level = Level::TRACE, skip(self, action_rx), fields(self.session = %self.session_id))]
    async fn io_loop(mut self, mut action_rx: UnboundedReceiver<Action>) -> Result<(), Error> {
        trace!("connection i/o loop starting");
        loop {
            let select_res = select! {
                cf = self.stream_read() => cf,
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
                    error!("breaking from select! due to err: {err:?}");
                    let msg = format!("connection i/o loop: {err}");
                    let _ = self.event_tx.send(Event {
                        session_id: self.session_id,
                        event: SessionEvent::Error(err),
                    });
                    return Err(ErrorKind::Internal(msg).into());
                }
                ControlFlow::Break(None) => {
                    trace!("breaking from select! for normal close");
                    break;
                }
            }
        }
        self.emit_event(SessionEvent::Disconnected)?;
        trace!("connection i/o loop finished");
        Ok(())
    }

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

    async fn handle_action(&mut self, action: Action) -> ControlFlow<Option<Error>> {
        match action {
            Action::Disconnect => ControlFlow::Break(None),
            Action::Send(item) => self.telnet_write(item).await,
            Action::Flush => {
                let Some(partial_line) = self.stream.codec_mut().partial_line() else {
                    return ControlFlow::Continue(());
                };
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
        Ok(self
            .event_tx
            .send(Event {
                session_id: self.session_id,
                event,
            })
            .map_err(ErrorKind::from)?)
    }
}
