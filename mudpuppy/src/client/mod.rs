mod gmcp;
pub mod input;
pub mod output;
mod prompt_flusher;

use std::fmt::{Debug, Display, Formatter};
use std::mem;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::MouseEvent;
use futures::stream::FuturesUnordered;
use pyo3::{pyclass, pymethods, Py, PyRefMut, Python};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, instrument, trace, warn, Level};

use crate::client::gmcp::Gmcp;
use crate::client::input::{EchoState, Input};
use crate::client::output::Output;
use crate::client::prompt_flusher::PromptFlusher;
use crate::config::GlobalConfig;
use crate::error::Error;
use crate::idmap::{IdMap, Identifiable};
use crate::model::{
    Alias, AliasConfig, InputLine, KeyEvent as PyKeyEvent, MouseEvent as PyMouseEvent, MudLine,
    PromptMode, PromptSignal, SessionInfo, Trigger, TriggerConfig,
};
use crate::net::telnet::codec::{Item as TelnetItem, Negotiation};
use crate::net::{connection, stream, telnet};
use crate::python;
use crate::tui::annotation::Annotation;
use crate::tui::button::Button;
use crate::tui::extrabuffer::ExtraBuffer;
use crate::tui::gauge::Gauge;
use crate::tui::layout::LayoutNode;
use crate::tui::session;

/// A telnet MUD client.
#[derive(Debug)]
pub struct Client {
    /// The MUD session the client is configured for.
    pub info: Arc<SessionInfo>,
    pub input: Py<Input>,
    pub output: Output,
    pub prompt: Option<MudLine>,
    pub triggers: IdMap<Trigger>,
    pub aliases: IdMap<Alias>,
    pub buffer_dimensions: (u16, u16),
    pub layout: Py<LayoutNode>,
    pub extra_buffers: IdMap<ExtraBuffer>,
    pub gauges: IdMap<Py<Gauge>>,
    pub buttons: IdMap<Py<Button>>,
    pub annotations: IdMap<Py<Annotation>>,
    pub gmcp: Gmcp,
    config: GlobalConfig,
    event_tx: UnboundedSender<python::Event>,
    conn_tx: UnboundedSender<connection::Event>,
    conn_state: State,
    telnet_state: telnet::negotiation::Table,
    prompt_mode: PromptMode,
    prompt_flusher: Option<PromptFlusher>,
}

impl Client {
    /// Construct a new `Client` for the given [`SessionInfo`].
    ///
    /// The client will be created in a disconnected state. To connect to the MUD server
    /// you must call [`Client::connect`].
    #[must_use]
    pub fn new(
        info: Arc<SessionInfo>,
        config: GlobalConfig,
        event_tx: UnboundedSender<python::Event>,
        conn_tx: UnboundedSender<connection::Event>,
    ) -> Self {
        let id = info.id;

        let input = Python::with_gil(|py| Py::new(py, Input::default()).unwrap());
        Self {
            info,
            input,
            output: Output::default(),
            prompt: None,
            triggers: IdMap::default(),
            aliases: IdMap::default(),
            buffer_dimensions: (0, 0),
            layout: session::initial_layout(),
            extra_buffers: IdMap::default(),
            gauges: IdMap::default(),
            buttons: IdMap::default(),
            annotations: IdMap::default(),
            gmcp: Gmcp::new(id),
            config,
            event_tx,
            conn_tx,
            conn_state: State::default(),
            telnet_state: initial_telnet_state(),
            prompt_mode: PromptMode::default(),
            prompt_flusher: None,
        }
    }

    /// Process connection events.
    #[instrument(level = Level::TRACE, skip(self, event, futures), fields(session_id = %self.info.id))]
    pub fn process_event(
        &mut self,
        event: connection::SessionEvent,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<(), Error> {
        match event {
            connection::SessionEvent::Error(err) => {
                self.conn_state = State::Disconnected;
                self.event_tx.send(self.connection_event())?;
                self.output.push(output::Item::ConnectionEvent {
                    status: self.status(),
                });
                return Err(err);
            }
            connection::SessionEvent::Disconnected => {
                self.conn_state = State::Disconnected;
                self.event_tx.send(self.connection_event())?;
                self.output.push(output::Item::ConnectionEvent {
                    status: self.status(),
                });
                return Ok(());
            }
            connection::SessionEvent::PartialLine(data) => {
                let mut prompt = MudLine::from(data);
                prompt.prompt = true;

                self.process_prompt(&mut prompt, futures)?;

                let item = output::Item::Prompt {
                    prompt: prompt.clone(),
                };
                trace!("{item}");
                self.output.push(item);
                self.prompt = Some(prompt.clone());
                self.event_tx.send(python::Event::Prompt {
                    id: self.info.id,
                    prompt,
                })?;
            }
            connection::SessionEvent::Telnet(item) => {
                self.process_telnet(item, futures)?;
            }
        }

        Ok(())
    }

    /// Process a key event, potentially sending a line of input.
    ///
    /// If the key press is the enter key, the input buffer's contents are popped,
    /// run through the alias machinery, and then used to send a line of input to
    /// the game.
    ///
    /// Otherwise, the key press is used to update the input buffer.
    ///
    /// # Errors
    /// If the client is not connected and the key press sends the input buffer.
    pub fn key_event(
        &mut self,
        futures: &mut FuturesUnordered<python::PyFuture>,
        event: &KeyEvent,
    ) -> Result<(), Error> {
        // If the key event was Enter being pressed, send the queued input.
        if let &KeyEvent {
            code: KeyCode::Enter,
            ..
        } = event
        {
            return self.transmit_queued_input(futures);
        }

        // Otherwise, handle the input key event.
        Python::with_gil(|py| {
            self.input.borrow_mut(py).handle_key_event(event);
        });

        if let Ok(model_event) = PyKeyEvent::try_from(*event) {
            self.event_tx.send(python::Event::KeyPress {
                id: self.info.id,
                key: model_event,
            })?;
        }
        Ok(())
    }

    pub fn mouse_event(
        &mut self,
        _futures: &mut FuturesUnordered<python::PyFuture>,
        event: &MouseEvent,
    ) -> Result<(), Error> {
        if let Ok(mouse_event) = PyMouseEvent::try_from(*event) {
            //debug!("mouse event: {mouse_event}");
            self.event_tx.send(python::Event::Mouse {
                id: self.info.id,
                event: mouse_event,
            })?;
        }

        Ok(())
    }

    fn transmit_queued_input(
        &mut self,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<(), Error> {
        // Pull the to-be-sent input. If it's None, transmit an empty line.
        let Some(queued_input) = Python::with_gil(|py| self.input.borrow_mut(py).pop()) else {
            return self.transmit_input(InputLine::default(), futures);
        };

        let cmd_separator = self
            .config
            .must_lookup_mud(&self.info.mud_name)?
            .command_separator;

        // If the queued input is itself empty after trim, or there's no command
        // separator configured for the MUD, we can blast it out as-is.
        if queued_input.empty() || cmd_separator.is_none() {
            return self.transmit_input(queued_input, futures);
        }

        // Otherwise, dice up the input into multiple transmits. Don't bother with
        // empty fragments.
        for fragment in queued_input.split(&cmd_separator.unwrap_or_default()) {
            self.transmit_input(fragment, futures)?;
        }
        Ok(())
    }

    fn transmit_input(
        &mut self,
        mut input: InputLine,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<(), Error> {
        let session_id = self.info.id;

        let empty_transmit = input.sent.is_empty();
        let mut skip_transmit = false;

        // Empty lines can't match aliases.
        if !empty_transmit {
            // Run the input line through each enabled alias to see if any match. A mutable ref to
            // input is passed to allow changing it when an alias matches.
            for alias in self.aliases.values_mut().filter(|alias| alias.enabled) {
                Self::evaluate_alias(session_id, alias, &mut input, futures)?;

                // If an alias replaced the to-be-sent text that we know wasn't empty originally
                // with empty text, then we take that as an indicator that the alias "ate" the
                // input (e.g. to call a callback) and we skip transmitting anything. We also
                // don't bother evaluating any other aliases.
                if input.sent.is_empty() {
                    skip_transmit = true;
                    break;
                }
            }
        }

        // If we're not transmitting anything, send an event and add the input to the output
        // buffer as if it were sent, but never send any content to the MUD.
        if skip_transmit {
            trace!("pushing non-transmitted line: {input:?}");
            self.output.push(output::Item::Input {
                line: input.clone(),
            });
            self.event_tx.send(python::Event::InputLine {
                id: self.info.id,
                input,
            })?;
            return Ok(());
        }

        // If there's a line to send, send it. The internal send line machinery will add it
        // to our output and emit the event.
        trace!("transmitting line: {input:?}");
        self.send_line(input)
    }

    /// Connect the client to the MUD server.
    ///
    /// # Errors
    /// If the connection can't be established.
    // TODO(XXX): Timeouts....
    #[instrument(level = Level::TRACE, skip(self), fields(self.info = %self.info))]
    pub async fn connect(&mut self) -> Result<(), Error> {
        if !matches!(self.conn_state, State::Disconnected) {
            warn!("already connected");
            return Ok(());
        }

        let mud = self.config.must_lookup_mud(&self.info.mud_name)?;

        self.conn_state = State::Connecting;
        self.telnet_state = initial_telnet_state();
        self.event_tx.send(self.connection_event())?;
        match connection::connect(self.info.id, &mud, self.conn_tx.clone()).await {
            Ok((handle, info)) => {
                let tx = handle.action_tx.clone();
                self.conn_state = State::Connected { handle, info };
                self.event_tx.send(self.connection_event())?;
                self.output.push(output::Item::ConnectionEvent {
                    status: self.status(),
                });

                if matches!(self.prompt_mode, PromptMode::Unsignalled { .. }) {
                    trace!("spawning new prompt flusher");
                    if let Some(flusher) = self
                        .prompt_flusher
                        .replace(PromptFlusher::new(tx, Duration::from_millis(200)))
                    {
                        trace!("stopping old prompt flusher");
                        flusher.stop();
                    }
                }

                self.request_enable_option(telnet::option::GMCP)?;

                Ok(())
            }
            Err(err) => {
                self.conn_state = State::Disconnected;
                self.event_tx.send(self.connection_event())?;
                Err(err)
            }
        }
    }

    /// Disconnect the client from the MUD server.
    ///
    /// Returns immediately and without error if the connection to a MUD server
    /// is already disconnected.
    ///
    /// # Errors
    /// If joining on the client connection task fails.
    #[instrument(level = Level::TRACE, skip(self))]
    pub async fn disconnect(&mut self) -> Result<(), Error> {
        let State::Connected { handle, .. } = mem::take(&mut self.conn_state) else {
            return Ok(());
        };
        if let Some(flusher) = self.prompt_flusher.take() {
            flusher.stop();
        }
        handle.send(connection::Action::Disconnect)?;
        handle
            .task
            .await
            .map_err(|_| Error::Internal("joining on client conn".into()))??;
        Ok(())
    }

    /// Send a line to the connection.
    ///
    /// # Errors
    /// If the client is not connected.
    #[instrument(level = Level::TRACE, skip(self, line), fields(sent = ?line.sent, original = ?line.original, scripted = ?line.scripted))]
    pub fn send_line(&mut self, line: InputLine) -> Result<(), Error> {
        let mud = self.config.must_lookup_mud(&self.info.mud_name)?;

        match &mud.command_separator {
            Some(sep) => {
                for fragment in line.sent.split(sep) {
                    let mut line = line.clone();
                    if line.sent != fragment {
                        line.original = Some(line.sent);
                        line.sent = fragment.to_string();
                    }
                    self.send_line_internal(line)?;
                }
                Ok(())
            }
            None => self.send_line_internal(line),
        }
    }

    #[instrument(level = Level::TRACE, skip(self, line), fields(sent = ?line.sent))]
    fn send_line_internal(&mut self, line: InputLine) -> Result<(), Error> {
        debug!("send");
        self.connected_handle()?
            .send(connection::Action::Send(TelnetItem::Line(
                line.sent.clone().into(),
            )))?;
        self.event_tx.send(python::Event::InputLine {
            id: self.info.id,
            input: line.clone(),
        })?;
        self.output.push(output::Item::Input { line });
        Ok(())
    }

    /// Enable a telnet protocol option.
    ///
    /// # Errors
    /// If the client isn't connected.
    #[instrument(level = Level::TRACE, skip(self))]
    pub fn request_enable_option(&mut self, option: u8) -> Result<(), Error> {
        if let Some(negotiation) = self.telnet_state.request_enable_option(option) {
            info!("negotiating enabling option {option}");
            trace!("sending negotiation {negotiation:?}");
            self.connected_handle()?
                .send(connection::Action::Send(negotiation.into()))?;
        }
        Ok(())
    }

    /// Disable a telnet protocol option.
    ///
    /// # Errors
    /// If the client isn't connected.
    #[instrument(level = Level::TRACE, skip(self))]
    pub fn request_disable_option(&mut self, option: u8) -> Result<(), Error> {
        if let Some(negotiation) = self.telnet_state.request_disable_option(option) {
            info!("negotiating disabling option {option}");
            trace!("sending negotiation {negotiation:?}");
            self.connected_handle()?
                .send(connection::Action::Send(negotiation.into()))?;
        }
        Ok(())
    }

    /// Send a telnet subnegotiation message for a given option.
    ///
    /// # Errors
    /// If the client isn't connected.
    #[instrument(level = Level::TRACE, skip(self, data))]
    pub fn send_subnegotiation(&self, option: u8, data: Vec<u8>) -> Result<(), Error> {
        trace!(
            "sending {} byte option {option} subnegotiation ",
            data.len()
        );
        self.connected_handle()?
            .send(connection::Action::Send(TelnetItem::Subnegotiation(
                option,
                data.into(),
            )))
    }

    /// Returns true if GMCP has been negotiated.
    pub fn gmcp_enabled(&self) -> bool {
        self.gmcp.ready
    }

    /// # Errors
    /// If not connected, or if GMCP is not negotiated, or the data fails to serialize
    /// to JSON.
    pub fn gmcp_send(&self, module: &str, data: impl Serialize) -> Result<(), Error> {
        self.connected_handle()?
            .send(self.gmcp.encode(module, data)?.into())
    }

    /// # Errors
    /// If not connected, or if GMCP is not negotiated.
    pub fn gmcp_send_json(&self, module: &str, json: &str) -> Result<(), Error> {
        self.connected_handle()?
            .send(self.gmcp.encode_json(module, json)?.into())
    }

    /// # Errors
    /// If not connected, or if GMCP is not negotiated.
    pub fn gmcp_register(&self, module: &str) -> Result<(), Error> {
        self.connected_handle()?
            .send(self.gmcp.register(module)?.into())
    }

    /// # Errors
    /// If not connected, or if GMCP is not negotiated.
    pub fn gmcp_unregister(&self, module: &str) -> Result<(), Error> {
        self.connected_handle()?
            .send(self.gmcp.unregister(module)?.into())
    }

    /// Returns whether the client is presently connected.
    ///
    /// For more granular information, prefer [`Client::status()`].
    #[must_use]
    pub fn connected(&self) -> bool {
        matches!(self.conn_state, State::Connected { .. })
    }

    /// Retrieve the [`Status`] of the client's connection to the MUD server.
    #[must_use]
    pub fn status(&self) -> Status {
        match &self.conn_state {
            State::Disconnected => Status::Disconnected {},
            State::Connecting => Status::Connecting {},
            State::Connected { info, .. } => Status::Connected { info: info.clone() },
        }
    }

    fn process_telnet(
        &mut self,
        item: TelnetItem,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<(), Error> {
        if matches!(item, TelnetItem::Line(_)) {
            trace!("{item:?}");
        } else {
            debug!("{item:?}");
        }
        match item {
            TelnetItem::Line(data) => self.process_output_line(MudLine::from(data), futures),
            TelnetItem::Negotiation(negotiation) => self.process_negotiation(negotiation),
            TelnetItem::IacCommand(iac) => self.process_iac(iac),
            TelnetItem::Subnegotiation(opt, data) => self.process_subnegotiation(opt, &data),
        }
    }

    fn process_output_line(
        &mut self,
        mut line: MudLine,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<(), Error> {
        self.process_mudline(&mut line, futures)?;

        let item = output::Item::Mud { line };
        self.output.push(item);

        if let Some(flusher) = &self.prompt_flusher {
            flusher.extend_timeout();
        }

        Ok(())
    }

    fn process_prompt(
        &mut self,
        prompt: &mut MudLine,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<(), Error> {
        self.process_mudline(prompt, futures)
    }

    fn process_mudline(
        &mut self,
        line: &mut MudLine,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<(), Error> {
        // TODO(XXX): awkward. avoid alloc. Doing this presently to avoid two mutable
        //  borrows of self - one for triggers, and one for send_line.
        let mut trigger_send = Vec::new();

        for trigger in self.triggers.values_mut().filter(|trigger| trigger.enabled) {
            if let Some(expansion) = Self::evaluate_trigger(self.info.id, trigger, line, futures)? {
                trigger_send.push(expansion);
            }
        }

        for line in trigger_send {
            self.send_line(InputLine::new(line, true, true))?;
        }

        Ok(())
    }

    fn process_iac(&self, command: u8) -> Result<(), Error> {
        if let Some(prompt_signal) = self.prompt_mode.signal() {
            if u8::from(prompt_signal) == command {
                if let Ok(handle) = self.connected_handle() {
                    trace!("prompt signal received: {prompt_signal}");
                    handle.send(connection::Action::Flush)?;
                }
            } else {
                warn!("unexpected IAC command {command} - our prompt signal is {prompt_signal}");
            }
        }
        self.event_tx
            .send(python::Event::Iac {
                id: self.info.id,
                command,
            })
            .map_err(Into::into)
    }

    fn process_negotiation(&mut self, negotiation: Negotiation) -> Result<(), Error> {
        if let (item, Some(event)) = self.gmcp.handle_negotiation(negotiation) {
            if let Some(item) = item {
                self.connected_handle()?.send(item.into())?;
            }
            self.event_tx.send(event)?;
        }

        match negotiation {
            Negotiation::Will(opt) | Negotiation::Do(opt) => {
                if let Some(reply) = self
                    .telnet_state
                    .reply_enable_if_supported(opt, matches!(negotiation, Negotiation::Will(_)))
                {
                    info!("option {opt} enabled");
                    trace!("sending reply: {reply:?}");
                    self.connected_handle()?
                        .send(connection::Action::Send(reply.into()))?;

                    match opt {
                        telnet::option::ECHO => Python::with_gil(|py| {
                            self.input
                                .borrow_mut(py)
                                .set_telnet_echo(EchoState::Password);
                        }),
                        telnet::option::EOR => self.set_prompt_mode(PromptMode::Signalled {
                            signal: PromptSignal::EndOfRecord,
                        }),
                        _ => {}
                    }

                    self.event_tx.send(python::Event::OptionEnabled {
                        id: self.info.id,
                        option: opt,
                    })?;
                }
            }
            Negotiation::Wont(opt) | Negotiation::Dont(opt) => {
                if let Some(reply) = self
                    .telnet_state
                    .reply_disable_if_enabled(opt, matches!(negotiation, Negotiation::Wont(_)))
                {
                    info!("option {opt} disabled");
                    trace!("sending reply: {reply:?}");
                    self.connected_handle()?
                        .send(connection::Action::Send(reply.into()))?;

                    match opt {
                        telnet::option::ECHO => Python::with_gil(|py| {
                            self.input
                                .borrow_mut(py)
                                .set_telnet_echo(EchoState::Enabled);
                        }),
                        // TODO(XXX): config for timeout?
                        telnet::option::EOR => self.set_prompt_mode(PromptMode::Unsignalled {
                            timeout: Duration::from_millis(200),
                        }),
                        _ => {}
                    }

                    self.event_tx.send(python::Event::OptionDisabled {
                        id: self.info.id,
                        option: opt,
                    })?;
                }
            }
        }

        Ok(())
    }

    fn process_subnegotiation(&mut self, opt: u8, data: &[u8]) -> Result<(), Error> {
        if opt == telnet::option::GMCP {
            if let Some(event) = self.gmcp.decode(data)? {
                if self.config.must_lookup_mud(&self.info.mud_name)?.debug_gmcp {
                    self.output.push(event.clone().into());
                }
                self.event_tx.send(event.into())?;
            }
        }

        self.event_tx
            .send(python::Event::Subnegotiation {
                id: self.info.id,
                option: opt,
                data: data.to_vec(),
            })
            .map_err(Into::into)
    }

    #[instrument(
        level = Level::TRACE,
        skip(trigger, line, futures),
        fields(trigger_id = %trigger.id()))
    ]
    fn evaluate_trigger(
        session_id: u32,
        trigger: &mut Trigger,
        line: &mut MudLine,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<Option<String>, Error> {
        let expansion = Python::with_gil(|py| {
            let mut trigger_config: PyRefMut<'_, TriggerConfig> = trigger.config.extract(py)?;

            let (matched, groups) = trigger_config.matches(line);
            if !matched {
                return Ok::<_, Error>(None);
            }
            trigger_config.hit_count += 1;

            debug!("trigger {} matched line", trigger.id());

            if let Some(callback) = &trigger_config.callback {
                trace!("preparing callback future for matches: {groups:?}");
                futures.push(Box::pin(pyo3_async_runtimes::tokio::into_future(
                    callback
                        .call1(py, (session_id, trigger.id(), line.clone(), groups.clone()))?
                        .into_bound(py),
                )?));
            }

            if let Some(highlight) = &trigger_config.highlight {
                trace!("invoking trigger highlight with match groups: {groups:?}");
                let new_line = highlight.call1(py, (line.clone(), groups))?;
                let new_line: MudLine = new_line.extract(py)?;
                trace!("line was replaced by trigger: {new_line:?}");
                *line = new_line;
            }

            if trigger_config.gag {
                trace!("line was gagged by trigger default");
                line.gag = true;
            }

            Ok(trigger_config.expansion.clone())
        })?;

        Ok(expansion)
    }

    #[instrument(
        level = Level::TRACE,
        skip(alias, futures),
        fields(alias_id = %alias.id()))
    ]
    fn evaluate_alias(
        session_id: u32,
        alias: &mut Alias,
        input: &mut InputLine,
        futures: &mut FuturesUnordered<python::PyFuture>,
    ) -> Result<(), Error> {
        Python::with_gil(|py| {
            let mut alias_config: PyRefMut<'_, AliasConfig> = alias.config.extract(py)?;
            let (matched, groups) = alias_config.matches(&input.sent);
            if !matched {
                return Ok(());
            }

            alias_config.hit_count += 1;
            debug!("alias {} matched line", alias.id());

            if let Some(callback) = &alias_config.callback {
                trace!("preparing callback future for matches: {groups:?}");
                futures.push(Box::pin(pyo3_async_runtimes::tokio::into_future(
                    callback
                        .call1(py, (session_id, alias.id(), input.clone(), groups.clone()))?
                        .into_bound(py),
                )?));
            }

            // Preserve the original input, and replace what will be sent with the alias expansion
            // or "" if there is no expansion.
            input.original = Some(input.sent.clone());
            input.sent = alias_config.expansion.clone().unwrap_or_default();
            Ok(())
        })
    }

    fn set_prompt_mode(&mut self, new_mode: PromptMode) {
        info!("prompt mode set to {new_mode}");
        self.prompt_mode = new_mode;

        if let Some(flusher) = mem::take(&mut self.prompt_flusher) {
            trace!("stopping previous prompt mode flusher");
            flusher.stop();
        }

        let Ok(handle) = self.connected_handle() else {
            return;
        };

        match self.prompt_mode {
            // If we've switched to an unsignalled prompt mode we need to spawn a new prompt flusher.
            PromptMode::Unsignalled { timeout } => {
                trace!("spawning new prompt flusher");
                self.prompt_flusher = Some(PromptFlusher::new(handle.action_tx.clone(), timeout));
            }

            // If we're switching to a signalled prompt mode, schedule a single flush event in 200ms.
            // Often we'll have enabled a new prompt mode at the beginning of a telnet connection after
            // the server has already sent a single unterminated prompt because it wasn't yet sure
            // whether we supported a signalled mode.
            PromptMode::Signalled { .. } => {
                let tx = handle.action_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    trace!("one time signalled prompt flush running");
                    if let Err(err) = tx.send(connection::Action::Flush) {
                        warn!("failed to send prompt flush: {err}");
                    }
                });
            }
        }
    }

    fn connected_handle(&self) -> Result<&connection::Handle, Error> {
        match &self.conn_state {
            State::Connected { handle, .. } => Ok(handle),
            _ => Err(Error::NotConnected),
        }
    }

    fn connection_event(&self) -> python::Event {
        python::Event::Connection {
            id: self.info.id,
            status: self.status(),
        }
    }
}

impl Display for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Client {} {} {} {} triggers {} aliases {} telnet options enabled",
            self.info,
            self.status(),
            self.prompt_mode,
            self.triggers.len(),
            self.aliases.len(),
            self.telnet_state.enabled_locally().len()
        )
    }
}

impl Identifiable for Client {
    fn id(&self) -> u32 {
        self.info.id
    }
}

/// Status of the client's connection to the MUD server.
#[derive(Clone, Debug, Eq, PartialEq)]
#[pyclass]
pub enum Status {
    /// The client is not connected to the MUD server.
    Disconnected {},

    /// The client is in the process of connecting to the MUD server.
    Connecting {},

    /// The client is connected to the MUD server.
    ///
    /// Details of the connection are available in the [`stream::Info`].
    Connected { info: stream::Info },
}

#[pymethods]
impl Status {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connecting {} => write!(f, "connecting"),
            Self::Connected { info } => write!(f, "{info}"),
            Self::Disconnected {} => write!(f, "disconnected"),
        }
    }
}

/// Internal state of the client's connection.
///
/// Similar to [`Status`] but offers access to a [`connection::Handle`] when
/// connected to the MUD server.
#[derive(Debug, Default)]
enum State {
    #[default]
    Disconnected,
    Connecting,
    Connected {
        handle: connection::Handle,
        info: stream::Info,
    },
}

// TODO(XXX): Use config/MUD to determine this?
fn initial_telnet_state() -> telnet::negotiation::Table {
    use telnet::command::GA;
    use telnet::option::{ECHO, EOR};
    // TODO(XXX): MCCP...

    telnet::negotiation::Table::from([ECHO, EOR, GA])
}
