mod alias;
mod buffer;
mod gmcp;
mod input;
mod mudline;
mod prompt;
mod timer;
mod trigger;

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::Arc;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use pyo3::types::PyModule;
use pyo3::{Py, Python};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::bytes::Bytes;
use tracing::{Level, debug, error, info, instrument, trace, warn};

use crate::dialog::DialogManager;
use crate::error::{ConfigError, Error, ErrorKind};
use crate::keyboard::KeyEvent;
use crate::net::telnet::codec::{Item as TelnetItem, Negotiation as TelnetNegotiation};
use crate::net::{connection, telnet};
use crate::{python, slash_command};

use crate::app::SlashCommand;
use crate::config::{Character, Config, Mud};
pub(crate) use alias::*;
pub(crate) use buffer::*;
pub(crate) use input::*;
pub(crate) use mudline::*;
pub(crate) use prompt::*;
pub(crate) use timer::*;
pub(crate) use trigger::*;

#[derive(Debug)]
pub(super) struct Session {
    pub(super) id: u32,
    pub(super) character: String,
    pub(super) event_handlers: python::Handlers,
    pub(super) dialog_manager: DialogManager,
    pub(super) prompt: Prompt,
    pub(super) input: Py<Input>,
    pub(super) output: Buffer,
    pub(super) scrollback: Buffer,
    pub(super) extra_buffers: HashMap<String, Py<Buffer>>,
    pub(super) triggers: Vec<Py<Trigger>>,
    pub(super) aliases: Vec<Py<Alias>>,
    pub(super) slash_commands: HashMap<String, Arc<dyn SlashCommand>>,

    state: ConnectionState,
    telnet_state: telnet::negotiation::Table,
    gmcp_packages: HashSet<String>,
    #[allow(dead_code)] // TODO(XXX): use user_module for reload support.
    pub(super) user_module: Option<Py<PyModule>>,
    config: Py<Config>,

    conn_event_tx: UnboundedSender<connection::Event>,
    pub(super) python_event_tx: UnboundedSender<(u32, python::Event)>,
}

impl Session {
    pub(super) fn new(
        id: u32,
        character: String,
        config: &Py<Config>,
        conn_event_tx: UnboundedSender<connection::Event>,
        python_event_tx: UnboundedSender<(u32, python::Event)>,
    ) -> Result<Self, Error> {
        let output = Buffer::new(OUTPUT_BUFFER_NAME.to_string())?;
        let scrollback = Buffer::new(SCROLL_BUFFER_NAME.to_string())?;

        let Some(py_character) = Python::attach(|py| config.borrow(py).character(py, &character))
        else {
            return Err(ErrorKind::from(ConfigError::InvalidCharacter(format!(
                "character {character} doesn't exist in config"
            )))
            .into());
        };

        let character_module = Python::attach(|py| py_character.borrow(py).module.clone());

        if let Some(module) = character_module {
            // TODO(XXX): hold onto Py module for reloads.
            python::run_character_setup(id, &character, module);
        }

        Ok(Self {
            id,
            event_handlers: python::Handlers::new(id),
            dialog_manager: DialogManager::new(),
            prompt: Prompt::new(id, python_event_tx.clone()),
            input: Python::attach(|py| Py::new(py, Input::new(py, id, python_event_tx.clone())?))?,
            output,
            scrollback,
            extra_buffers: HashMap::default(),
            triggers: Vec::default(),
            aliases: Vec::default(),
            slash_commands: slash_command::builtin(),

            state: ConnectionState::default(),
            telnet_state: telnet::negotiation::Table::default(),
            gmcp_packages: HashSet::default(),
            user_module: None,
            config: Python::attach(|py| config.clone_ref(py)),
            character,

            conn_event_tx,
            python_event_tx,
        })
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
    pub(super) fn connect(&mut self) -> Result<(), Error> {
        if !matches!(self.state, ConnectionState::Disconnected) {
            return Ok(());
        }

        let mud = Python::attach(|py| Ok::<_, Error>(self.mud(py)?.borrow(py).clone()))?;

        self.state = ConnectionState::Connecting(connection::Handle::new(
            self.id,
            mud,
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

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
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

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
    pub(super) fn request_enable_option(&mut self, option: u8) -> Result<(), Error> {
        let Some(negotiation) = self.telnet_state.request_enable_option(option) else {
            return Ok(());
        };

        debug!("negotiating enabling option");
        trace!("sending negotiation {negotiation:?}");
        self.connected_handle()?
            .send(connection::Action::Send(negotiation.into()))
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
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

    #[instrument(level = Level::TRACE, skip(self, data), fields(id=self.id, character_name=self.character, data_len = data.len()))]
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

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
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

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
    pub(crate) fn unregister_gmcp_package(&mut self, package: &str) -> Result<(), Error> {
        if !self.gmcp_packages.contains(package) {
            return Ok(());
        }

        self.gmcp_packages.remove(package);

        if self.gmcp_enabled() {
            debug!("unregistering");
            self.send_telnet_item(gmcp::unregister(package).map_err(ErrorKind::from)?)?;
        }
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
    pub(crate) fn send_gmcp_message(
        &self,
        package: &str,
        data: impl serde::Serialize + Debug,
    ) -> Result<(), Error> {
        if self.gmcp_enabled() {
            warn!("GMCP is not enabled, ignoring send");
            return Ok(());
        }

        if !self.gmcp_packages.contains(package) {
            warn!(package, "GMCP package not enabled, ignoring send");
            return Ok(());
        }

        trace!("sending message");
        self.send_telnet_item(gmcp::encode(package, data).map_err(ErrorKind::from)?)?;
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
    pub(super) fn flush_prompt(&self) -> Result<(), Error> {
        let Ok(connected_handle) = self.connected_handle() else {
            return Ok(());
        };
        let _ = connected_handle.send(connection::Action::Flush);
        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self), fields(id=self.id, character_name=self.character))]
    pub(super) fn send_line(&mut self, line: InputLine, skip_aliases: bool) -> Result<(), Error> {
        let send_separator = Python::attach(|py| {
            Ok::<_, Error>(
                self.config
                    .borrow(py)
                    .resolve_settings(py, Some(&self.character))?
                    .send_separator
                    .clone(),
            )
        })?;

        let lines = match line.sent.contains(&send_separator) {
            true => line.split(&send_separator),
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
                    Python::attach(|py| {
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

            let session_name = self.character.clone();
            tokio::spawn(async move {
                while let Some(result) = futures.next().await {
                    if let Err(err) = result {
                        // Note: Error::from() to collect backtrace from PyErr.
                        let error_msg = Error::from(err);
                        error!("{session_name} alias callback error: {error_msg}");
                        Python::attach(|py| {
                            if let Some(error_tx) = python::ERROR_TX.get(py) {
                                let _ = error_tx.send(format!(
                                    "Alias callback error for '{session_name}': {error_msg}"
                                ));
                            }
                        });
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

            self.connected_handle()?
                .send(telnet::codec::Item::Line(Bytes::copy_from_slice(
                    line.sent.as_bytes(),
                )))?;
            self.python_event_tx
                .send((self.id, python::Event::InputLine { line: line.clone() }))
                .map_err(ErrorKind::from)?;
            self.output.add(line.into());
        }

        Ok(())
    }

    #[instrument(level = Level::TRACE, skip(self, event), fields(id=self.id, character_name=self.character))]
    pub(super) fn key_event(&mut self, event: &KeyEvent) {
        trace!("updating input");
        Python::attach(|py| {
            self.input.borrow_mut(py).key_event(event);
        });
    }

    #[instrument(level = Level::TRACE, skip(self, event), fields(id=self.id, character_name=self.character))]
    #[expect(clippy::too_many_lines)] // Just on the cusp of needing a refactor.
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
                let gmcp_echo = Python::attach(|py| {
                    Ok::<_, Error>(
                        self.config
                            .borrow(py)
                            .resolve_settings(py, Some(&self.character))?
                            .gmcp_echo,
                    )
                })?;
                if gmcp_echo {
                    self.output.add(OutputItem::Debug {
                        line: gmcp_event.to_string(),
                    });
                }

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

    #[instrument(level = Level::TRACE, skip(self, line), fields(id=self.id, character_name=self.character, line_len=line.len()))]
    fn process_line(&mut self, line: &Bytes) -> Result<(), Error> {
        if let Some(flusher) = self.prompt.flusher() {
            flusher.extend_timeout();
        }

        let mut line = line.into();
        let mut futures = FuturesUnordered::new();
        let py_sesh = python::Session::from(&*self);
        Python::attach(|py| {
            for t in &self.triggers {
                Trigger::evaluate(py, t.clone(), &futures, &py_sesh, &mut line)?;
            }
            Ok::<(), Error>(())
        })?;

        let character_name = self.character.clone();
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

    fn character(&self, py: Python<'_>) -> Result<Py<Character>, Error> {
        self.config
            .borrow(py)
            .character(py, &self.character)
            .ok_or_else(|| {
                ErrorKind::from(ConfigError::InvalidCharacter(format!(
                    "character {name} isn't defined in configuration",
                    name = self.character
                )))
                .into()
            })
    }

    fn mud(&self, py: Python<'_>) -> Result<Py<Mud>, Error> {
        let character = self.character(py)?;
        let name = &character.borrow(py).mud;
        self.config
            .borrow(py)
            .mud(py, name)
            .ok_or_else(|| ErrorKind::NoSuchMud(name.to_owned()).into())
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
