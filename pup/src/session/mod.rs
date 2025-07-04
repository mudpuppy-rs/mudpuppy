mod alias;
mod buffer;
mod character;
mod gmcp;
mod input;
mod prompt;
mod trigger;

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use pyo3::types::PyModule;
use pyo3::{Py, Python};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::bytes::Bytes;
use tracing::{Level, debug, error, info, instrument, trace, warn};

use crate::error::{Error, ErrorKind};
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::net::telnet::codec::{Item as TelnetItem, Negotiation as TelnetNegotiation};
use crate::net::{connection, telnet};
use crate::python;

use crate::shortcut::{InputShortcut, ScrollShortcut, Shortcut};
pub(crate) use alias::*;
pub(crate) use buffer::*;
pub(crate) use character::*;
pub(crate) use input::*;
pub(crate) use prompt::*;
pub(crate) use trigger::*;

#[derive(Debug)]
pub(super) struct Session {
    pub(super) id: u32,
    pub(super) character: Character,
    pub(super) event_handlers: python::SessionHandlers,
    pub(super) prompt: Prompt,
    pub(super) input: Py<Input>,
    pub(super) output: Buffer,
    pub(super) scrollback: Buffer,
    pub(super) extra_buffers: HashMap<String, Py<Buffer>>,
    pub(super) triggers: Vec<Py<Trigger>>,
    pub(super) aliases: Vec<Py<Alias>>,
    pub(super) shortcuts: HashMap<KeyEvent, Shortcut>,

    state: ConnectionState,
    telnet_state: telnet::negotiation::Table,
    gmcp_packages: HashSet<String>,
    #[allow(dead_code)] // TODO(XXX): use user_module for reload support.
    user_module: Option<Py<PyModule>>,

    conn_event_tx: UnboundedSender<connection::Event>,
    pub(super) python_event_tx: UnboundedSender<(u32, python::Event)>,
}

impl Session {
    pub(super) fn new(
        id: u32,
        character: Character,
        conn_event_tx: UnboundedSender<connection::Event>,
        python_event_tx: UnboundedSender<(u32, python::Event)>,
    ) -> Result<Self, Error> {
        // TODO(XXX): output wrap settings.
        let mut output = Buffer::new(OUTPUT_BUFFER_NAME.to_string())?;
        output.line_wrap = true;

        let mut scrollback = Buffer::new(SCROLL_BUFFER_NAME.to_string())?;
        // TODO(XXX): scrollbar border settings.
        scrollback.border_left = true;
        scrollback.border_right = true;
        scrollback.border_bottom = true;
        scrollback.scrollbar = Scrollbar::Always;
        scrollback.line_wrap = output.line_wrap;

        Ok(Self {
            id,
            event_handlers: python::SessionHandlers::default(),
            prompt: Prompt::new(id, python_event_tx.clone()),
            input: Python::with_gil(|py| {
                Py::new(py, Input::new(py, id, python_event_tx.clone())?)
            })?,
            output,
            scrollback,
            extra_buffers: HashMap::default(),
            triggers: Vec::default(),
            aliases: Vec::default(),
            shortcuts: default_shortcuts(),

            state: ConnectionState::default(),
            telnet_state: telnet::negotiation::Table::default(),
            gmcp_packages: HashSet::default(),
            user_module: python::run_character_setup(id, &character)?,
            character,

            conn_event_tx,
            python_event_tx,
        })
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(super) fn connect(&mut self) -> Result<(), Error> {
        if !matches!(self.state, ConnectionState::Disconnected) {
            return Ok(());
        }

        self.state = ConnectionState::Connecting(connection::Handle::new(
            self.id,
            self.character.mud.clone(),
            self.conn_event_tx.clone(),
        ));
        self.python_event_tx
            .send((self.id, python::Event::SessionConnecting {}))
            .map_err(ErrorKind::from)?;
        self.output.add(OutputItem::ConnectionEvent {
            message: "Connecting...".to_string(),
            info: None,
        });

        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(super) fn disconnect(&mut self) -> Result<(), Error> {
        let handle = self.connected_handle()?;
        handle
            .action_tx
            .send(connection::Action::Disconnect)
            .map_err(ErrorKind::from)?;
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    pub(super) fn connected(&self) -> Option<connection::Info> {
        match &self.state {
            ConnectionState::Connected { info, .. } => Some(info.clone()),
            _ => None,
        }
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(super) fn request_enable_option(&mut self, option: u8) -> Result<(), Error> {
        let Some(negotiation) = self.telnet_state.request_enable_option(option) else {
            return Ok(());
        };

        debug!("negotiating enabling option");
        trace!("sending negotiation {negotiation:?}");
        self.connected_handle()?
            .send(connection::Action::Send(negotiation.into()))
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(super) fn request_disable_option(&mut self, option: u8) -> Result<(), Error> {
        let Some(negotiation) = self.telnet_state.request_disable_option(option) else {
            return Ok(());
        };

        debug!("negotiating disabling option");
        trace!("sending negotiation {negotiation:?}");
        self.connected_handle()?
            .send(connection::Action::Send(negotiation.into()))
    }

    pub(super) fn protocol_enabled(&self, option: u8) -> bool {
        self.telnet_state.option(option).local_enabled()
    }

    #[instrument(level = Level::TRACE, skip(self, data), fields(id=self.id, character_name=self.character.name, data_len = data.len()))]
    pub(super) fn send_subnegotiation(&self, option: u8, data: Vec<u8>) -> Result<(), Error> {
        debug!("sending subnegotiation");
        self.connected_handle()?
            .send(connection::Action::Send(TelnetItem::Subnegotiation(
                option,
                data.into(),
            )))
    }

    pub(crate) fn gmcp_enabled(&self) -> bool {
        self.protocol_enabled(telnet::option::GMCP)
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(crate) fn register_gmcp_package(&mut self, package: String) -> Result<(), Error> {
        if self.gmcp_enabled() {
            debug!("registering");
            self.send_telnet_item(gmcp::register(&package).map_err(ErrorKind::from)?)?;
        } else {
            debug!("queueing");
        }

        self.gmcp_packages.insert(package);
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(crate) fn unregister_gmcp_package(&mut self, package: String) -> Result<(), Error> {
        if !self.gmcp_packages.contains(&package) {
            return Ok(());
        }

        self.gmcp_packages.remove(&package);

        if self.gmcp_enabled() {
            debug!("unregistering");
            self.send_telnet_item(gmcp::unregister(&package).map_err(ErrorKind::from)?)?;
        }
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(crate) fn send_gmcp_message(
        &self,
        package: &str,
        data: impl serde::Serialize + Debug,
    ) -> Result<(), Error> {
        if self.gmcp_enabled() {
            warn!("GMCP is not enabled, ignoring send");
            return Ok(());
        }

        trace!("sending message");
        self.send_telnet_item(gmcp::encode(package, data).map_err(ErrorKind::from)?)?;
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(super) fn flush_prompt(&self) -> Result<(), Error> {
        let Ok(connected_handle) = self.connected_handle() else {
            return Ok(());
        };
        let _ = connected_handle.send(connection::Action::Flush);
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character.name))]
    pub(super) fn send_line(&mut self, line: InputLine, skip_aliases: bool) -> Result<(), Error> {
        let lines = match &self.character.command_separator {
            Some(separator) if line.sent.contains(separator) => line.split(separator),
            _ => vec![line],
        };

        let py_sesh = python::Session::from(&*self);
        for mut line in lines {
            let mut futures = FuturesUnordered::new();
            let mut skip_transmit = false;

            // Empty lines can't match aliases.
            if !skip_aliases && !line.sent.is_empty() {
                // Run the input line through each enabled alias to see if any match. A mutable ref to
                // input is passed to allow changing it when an alias matches.
                for a in &self.aliases {
                    Python::with_gil(|py| {
                        Alias::evaluate(py, a.clone(), &futures, &py_sesh, &mut line)
                    })?;

                    // If an alias cleared out the to-be-sent text that we know wasn't empty
                    // originally, then we take that as an indicator that the alias "ate" the
                    // input (e.g., to call a callback) and we skip transmitting anything. We also
                    // don't bother evaluating any other aliases.
                    if line.sent.is_empty() {
                        skip_transmit = true;
                        break;
                    }
                }
            }

            let session_name = self.character.to_string();
            tokio::spawn(async move {
                while let Some(result) = futures.next().await {
                    if let Err(err) = result {
                        // Note: Error::from() to collect backtrace from PyErr.
                        error!("{session_name} alias callback error: {}", Error::from(err));
                    }
                }
            });

            if skip_transmit {
                trace!("non-transmitted input processed: {line:?}");
                self.python_event_tx
                    .send((self.id, python::Event::InputLine { line }))
                    .map_err(ErrorKind::from)?;
                return Ok(());
            }

            self.connected_handle()?.send_line(&line.sent)?;
            self.python_event_tx
                .send((self.id, python::Event::InputLine { line: line.clone() }))
                .map_err(ErrorKind::from)?;
            self.output.add(line.into());
        }

        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self, event), fields(id=self.id, character_name=self.character.name))]
    pub(super) fn key_event(&mut self, event: &KeyEvent) {
        trace!("updating input");
        Python::with_gil(|py| {
            self.input.borrow_mut(py).key_event(event);
        });
    }

    #[instrument(level = Level::TRACE, skip(self, event), fields(id=self.id, character_name=self.character.name))]
    pub(super) fn handle_event(&mut self, event: &connection::SessionEvent) -> Result<(), Error> {
        match event {
            connection::SessionEvent::Connected(info) => {
                let ConnectionState::Connecting(handle) = std::mem::take(&mut self.state) else {
                    unreachable!("unexpected connected event");
                };

                self.telnet_state = initial_telnet_state();
                self.prompt.init_flusher(handle.action_tx.clone());
                self.state = ConnectionState::Connected {
                    handle,
                    info: info.clone(),
                };
                self.output.add(OutputItem::ConnectionEvent {
                    message: "Connected".to_string(),
                    info: Some(info.clone()),
                });
                self.python_event_tx
                    .send((
                        self.id,
                        python::Event::SessionConnected { info: info.clone() },
                    ))
                    .map_err(ErrorKind::from)?;
            }
            connection::SessionEvent::Disconnected => {
                info!("session disconnected");
                self.state = ConnectionState::Disconnected;
                self.output.add(OutputItem::ConnectionEvent {
                    message: "Disconnected...".to_string(),
                    info: None,
                });
                self.python_event_tx
                    .send((self.id, python::Event::SessionDisconnected {}))
                    .map_err(ErrorKind::from)?;
            }
            connection::SessionEvent::Error(err) => {
                error!("session error: {err}");
                self.state = ConnectionState::Disconnected;
                for line in err.to_string().lines() {
                    self.output.add(OutputItem::Error {
                        message: line.to_string(),
                    });
                }
                self.output.add(OutputItem::ConnectionEvent {
                    message: "Disconnected...".to_string(),
                    info: None,
                });
                self.python_event_tx
                    .send((self.id, python::Event::SessionDisconnected {}))
                    .map_err(ErrorKind::from)?;
            }
            connection::SessionEvent::Telnet(TelnetItem::Negotiation(negotiation)) => {
                self.telnet_negotiation(*negotiation)?;
            }
            connection::SessionEvent::Telnet(TelnetItem::Subnegotiation(opt, data))
                if *opt == telnet::option::GMCP =>
            {
                let gmcp_event = gmcp::decode(data).map_err(ErrorKind::from)?;
                // TODO(XXX): Debug on/off for GMCP.
                self.output.add(OutputItem::Debug {
                    line: gmcp_event.to_string(),
                });
                if self.protocol_enabled(telnet::option::GMCP) {
                    self.python_event_tx
                        .send((self.id, gmcp_event))
                        .map_err(ErrorKind::from)?;
                } else {
                    warn!("ignoring GMCP subnegotiation for disabled GMCP");
                }
            }
            connection::SessionEvent::Telnet(TelnetItem::Subnegotiation(opt, data)) => {
                self.python_event_tx
                    .send((
                        self.id,
                        python::Event::TelnetSubnegotiation {
                            option: *opt,
                            data: data.to_vec(),
                        },
                    ))
                    .map_err(ErrorKind::from)?;
            }
            connection::SessionEvent::Telnet(TelnetItem::IacCommand(command)) => {
                if let Some(prompt_signal) = self.prompt.mode().signal() {
                    if *command == u8::from(prompt_signal) {
                        debug!("received {prompt_signal} - flushing prompt");
                        self.flush_prompt()?;
                    }
                }
                self.python_event_tx
                    .send((
                        self.id,
                        python::Event::TelnetIacCommand { command: *command },
                    ))
                    .map_err(ErrorKind::from)?;
            }
            connection::SessionEvent::Telnet(TelnetItem::Line(line)) => {
                self.process_line(line)?;
            }
            connection::SessionEvent::PartialLine(content) => {
                self.prompt
                    .set_content(String::from_utf8_lossy(content).to_string())?;
                self.output.add(OutputItem::Prompt {
                    prompt: MudLine {
                        raw: content.clone(),
                        prompt: true,
                        gag: false,
                    },
                });
            }
        }
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self, line), fields(id=self.id, character_name=self.character.name, line_len=line.len()))]
    fn process_line(&mut self, line: &Bytes) -> Result<(), Error> {
        if let Some(flusher) = self.prompt.flusher() {
            flusher.extend_timeout();
        }

        let mut line = line.into();
        let mut futures = FuturesUnordered::new();
        let py_sesh = python::Session::from(&*self);
        Python::with_gil(|py| {
            for t in &self.triggers {
                Trigger::evaluate(py, t.clone(), &futures, &py_sesh, &mut line)?;
            }
            Ok::<(), Error>(())
        })?;

        let character_name = self.character.to_string();
        tokio::spawn(async move {
            while let Some(result) = futures.next().await {
                if let Err(err) = result {
                    // Note: Error::from() to collect backtrace from PyErr.
                    error!(
                        "{character_name} trigger callback error: {}",
                        Error::from(err)
                    );
                }
            }
        });

        self.python_event_tx
            .send((self.id, python::Event::Line { line: line.clone() }))
            .map_err(ErrorKind::from)?;
        self.output.add(OutputItem::Mud { line });

        Ok(())
    }

    fn connected_handle(&self) -> Result<&connection::Handle, Error> {
        match &self.state {
            ConnectionState::Connected { handle, .. } => Ok(handle),
            ConnectionState::Disconnected | ConnectionState::Connecting { .. } => {
                Err(ErrorKind::NotConnected.into())
            }
        }
    }

    fn send_telnet_item(&self, item: TelnetItem) -> Result<(), Error> {
        self.connected_handle()?
            .send(connection::Action::Send(item))
    }

    fn telnet_negotiation(&mut self, negotiation: TelnetNegotiation) -> Result<(), Error> {
        match negotiation {
            TelnetNegotiation::Will(option) | TelnetNegotiation::Do(option) => {
                let Some(reply) = self.telnet_state.reply_enable_if_supported(
                    option,
                    matches!(negotiation, TelnetNegotiation::Will(_)),
                ) else {
                    return Ok(());
                };

                debug!("option enabled");
                trace!("sending reply: {reply:?}");
                self.connected_handle()?
                    .send(connection::Action::Send(reply.into()))?;

                match option {
                    telnet::option::EOR => {
                        let new_mode = PromptMode::Signalled {
                            signal: PromptSignal::EndOfRecord,
                        };
                        info!("new prompt mode: {new_mode}");
                        self.prompt.set_mode(new_mode)?;
                    }
                    telnet::option::GMCP => {
                        info!("GMCP enabled");
                        self.send_telnet_item(gmcp::encode_hello())?;
                        for package in &self.gmcp_packages {
                            self.send_telnet_item(
                                gmcp::register(package.as_str()).map_err(ErrorKind::from)?,
                            )?;
                        }
                        self.python_event_tx
                            .send((self.id, python::Event::GmcpEnabled {}))
                            .map_err(ErrorKind::from)?;
                    }
                    _ => {}
                }
                Ok(self
                    .python_event_tx
                    .send((self.id, python::Event::TelnetOptionEnabled { option }))
                    .map_err(ErrorKind::from)?)
            }
            TelnetNegotiation::Wont(option) | TelnetNegotiation::Dont(option) => {
                let Some(reply) = self.telnet_state.reply_disable_if_enabled(
                    option,
                    matches!(negotiation, TelnetNegotiation::Wont(_)),
                ) else {
                    return Ok(());
                };

                debug!("option disabled");
                trace!("sending reply: {reply:?}");
                self.connected_handle()?
                    .send(connection::Action::Send(reply.into()))?;

                match option {
                    telnet::option::EOR => {
                        let new_mode = PromptMode::default();
                        info!("new default prompt mode: {new_mode}");
                        self.prompt.set_mode(new_mode)?;
                    }
                    telnet::option::GMCP => {
                        info!("GMCP disabled");
                        self.python_event_tx
                            .send((self.id, python::Event::GmcpDisabled {}))
                            .map_err(ErrorKind::from)?;
                    }
                    _ => {}
                }
                Ok(self
                    .python_event_tx
                    .send((self.id, python::Event::TelnetOptionDisabled { option }))
                    .map_err(ErrorKind::from)?)
            }
        }
    }
}

impl From<&Session> for python::Session {
    fn from(sesh: &Session) -> Self {
        Self {
            id: sesh.id,
            character: sesh.character.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub(super) enum ConnectionState {
    #[default]
    Disconnected,
    Connecting(connection::Handle),
    Connected {
        handle: connection::Handle,
        info: connection::Info,
    },
}

pub(super) const OUTPUT_BUFFER_NAME: &str = "MUD Output";

pub(super) const SCROLL_BUFFER_NAME: &str = "Scrollback";

// TODO(XXX): Use config/MUD to determine this?
fn initial_telnet_state() -> telnet::negotiation::Table {
    use telnet::option::{ECHO, EOR, GMCP};
    // TODO(XXX): MCCP...
    // TODO(XXX): GA?

    telnet::negotiation::Table::from([ECHO, EOR, GMCP])
}

#[allow(clippy::too_many_lines)]
fn default_shortcuts() -> HashMap<KeyEvent, Shortcut> {
    HashMap::from([
        // ENTER -> Send input
        (
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::Send.into(),
        ),
        // BACKSPACE or Ctrl-h -> Delete prev char
        (
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::DeletePrev.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('h'),
            },
            InputShortcut::DeletePrev.into(),
        ),
        // DELETE -> Delete next char
        (
            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::DeleteNext.into(),
        ),
        // LEFT or Ctrl-b -> Cursor left
        (
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::CursorLeft.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('b'),
            },
            InputShortcut::CursorLeft.into(),
        ),
        // Ctrl-LEFT or Alt-b -> Cursor word left
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Left,
            },
            InputShortcut::CursorWordLeft.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::ALT,
                code: KeyCode::Char('b'),
            },
            InputShortcut::CursorWordLeft.into(),
        ),
        // RIGHT or Ctrl-f -> Cursor right
        (
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::CursorRight.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('f'),
            },
            InputShortcut::CursorRight.into(),
        ),
        // Ctrl-RIGHT or Alt-f -> Cursor word right
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Right,
            },
            InputShortcut::CursorWordRight.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::ALT,
                code: KeyCode::Char('f'),
            },
            InputShortcut::CursorWordRight.into(),
        ),
        // CTRL-u -> Reset
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('u'),
            },
            InputShortcut::Reset.into(),
        ),
        // Alt-BACKSPACE or CTRL-w -> Delete word left
        (
            KeyEvent {
                modifiers: KeyModifiers::ALT,
                code: KeyCode::Backspace,
            },
            InputShortcut::CursorDeleteWordLeft.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('w'),
            },
            InputShortcut::CursorDeleteWordLeft.into(),
        ),
        // Ctrl-DELETE -> Delete word right
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Delete,
            },
            InputShortcut::CursorDeleteWordRight.into(),
        ),
        // Ctrl-k -> Delete to end
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('k'),
            },
            InputShortcut::CursorDeleteToEnd.into(),
        ),
        // HOME or Ctrl-a -> Cursor start
        (
            KeyEvent {
                code: KeyCode::Home,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::CursorToStart.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('a'),
            },
            InputShortcut::CursorToStart.into(),
        ),
        // END or Ctrl-e -> Cursor end
        (
            KeyEvent {
                code: KeyCode::End,
                modifiers: KeyModifiers::NONE,
            },
            InputShortcut::CursorToEnd.into(),
        ),
        (
            KeyEvent {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('e'),
            },
            InputShortcut::CursorToEnd.into(),
        ),
        // PAGE-UP -> Scroll up
        (
            KeyEvent {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
            },
            ScrollShortcut::Up.into(),
        ),
        // PAGE-DOWN -> Scroll down
        (
            KeyEvent {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
            },
            ScrollShortcut::Down.into(),
        ),
        // SHIFT-HOME -> Scroll top
        (
            KeyEvent {
                modifiers: KeyModifiers::SHIFT,
                code: KeyCode::Home,
            },
            ScrollShortcut::Top.into(),
        ),
        // SHIFT-END -> Scroll bottom
        (
            KeyEvent {
                modifiers: KeyModifiers::SHIFT,
                code: KeyCode::End,
            },
            ScrollShortcut::Bottom.into(),
        ),
    ])
}
