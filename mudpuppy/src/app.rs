use std::fmt::{Debug, Formatter};
use std::io::{self, stdout};
use std::num::NonZeroUsize;
use std::sync::Arc;

use async_trait::async_trait;
use crossterm::event::{KeyEvent, MouseEvent};
use futures::channel::mpsc::{channel as futures_channel, Receiver};
use futures::stream::FuturesUnordered;
use futures::{FutureExt, SinkExt, StreamExt};
use notify::{
    Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use pyo3::{Py, PyObject, PyResult, Python};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::crossterm::event::{Event as TermEvent, KeyEventKind};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::ExecutableCommand;
use ratatui::layout::Constraint::{Fill, Length, Max, Min};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use ratatui::{crossterm, Frame, Terminal};
use tokio::select;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::RwLock;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error, info, instrument, trace, warn, Level};

use crate::client::Client;
use crate::config::{config_dir, config_file, GlobalConfig};
use crate::error::Error;
use crate::idmap::IdMap;
use crate::model::{InputMode, Mud, SessionInfo, Shortcut, Timer};
use crate::net::connection;
use crate::python::{self, PyApp};
use crate::tui::{mudlist, session};
use crate::{cli, Result, CRATE_NAME};

pub struct App {
    config: GlobalConfig,
    tabs: Vec<Box<dyn Tab>>,
}

impl App {
    #[must_use]
    pub fn new(config: GlobalConfig) -> Self {
        Self {
            config: config.clone(),
            tabs: vec![Box::new(mudlist::Widget::new(config))],
        }
    }

    /// Run the mudpuppy TUI application.
    ///
    /// This will take over stdout, entering the alternative screen mode, and
    /// beginning the main application loop.
    ///
    /// # Errors
    /// Returns errors in a variety of circumstances such as terminal initialization error,
    /// config live-reloading initialization error, or unexpected TUI drawing failure.
    ///
    /// In general recoverable errors will be shown to the user and then dismissed without
    /// breaking from the application loop. Unrecoverable errors will be displayed and then
    /// yielded from this function to initiate shutdown.
    #[allow(clippy::too_many_lines)] // right at threshold, consider refactor later.
    pub async fn run(&mut self, args: cli::Args) -> Result<()> {
        let mouse_enabled = self.config.lookup(|c| c.mouse_enabled, false);
        let mut terminal = init_terminal(mouse_enabled)?;

        let (event_tx, mut event_rx) = unbounded_channel();
        let (conn_tx, mut conn_rx) = unbounded_channel();
        let state_lock = Arc::new(RwLock::new(State::new(
            self.config.clone(),
            event_tx.clone(),
            conn_tx,
        )));

        let mut crossterm_events = crossterm::event::EventStream::new();
        let (_watcher, mut config_event_rx) =
            config_reload_init().map_err(|e| Error::Internal(e.to_string()))?;

        let (python_callback_tx, mut python_callback_rx) = unbounded_channel();
        let py_app = PyApp {
            config: self.config.clone(),
            state: state_lock.clone(),
            waker: python_callback_tx,
        };

        info!("initializing python environment");
        let (event_handlers, py_user_modules) = match python::init(py_app) {
            Ok((event_handlers, py_user_modules)) => (event_handlers, py_user_modules),
            Err(err) => {
                error!("{}", err);
                state_lock.write().await.ui_state = err.into();
                let event_handlers: Py<python::EventHandlers> =
                    Python::with_gil(|py| Py::new(py, python::EventHandlers::new()))?;
                (event_handlers, Vec::default())
            }
        };

        let mut event_futures: FuturesUnordered<python::PyFuture> = FuturesUnordered::new();
        let mut draw_interval = interval(args.frame_rate_duration()?);
        draw_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        if !args.connect.is_empty() {
            let mut state = state_lock.write().await;
            for mud_name in args.connect {
                let mud = state.config.must_lookup_mud(&mud_name)?;
                let session_info = state.new_session(mud)?;
                info!("created new session {session_info}");
                self.handle_tab_action(
                    &mut state,
                    TabAction::New {
                        session_info,
                        switch: true,
                    },
                )
                .await?;
            }
        }

        loop {
            let mut state = state_lock.write().await;

            match state.ui_state {
                UiState::Exit => {
                    trace!("breaking select loop for exit");
                    break;
                }
                UiState::ReloadPython => {
                    event_futures.clear();

                    state.timers.clear();
                    for client in state.clients.values_mut() {
                        client.triggers.clear();
                        client.aliases.clear();
                    }

                    trace!("reloading python modules");
                    python::reload(&py_user_modules)?;
                    trace!("done");
                    event_tx.send(python::Event::PythonReloaded {})?;

                    for (id, _) in &state.clients {
                        event_tx.send(python::Event::ResumeSession { id: *id })?;
                    }

                    state.ui_state = UiState::Running;
                }
                _ => {}
            }

            let res = select! {
                 _ = draw_interval.tick() => {
                    self.draw(&mut state, &mut terminal);
                    Ok(())
                }
                Some(()) = python_callback_rx.recv() => {
                    Ok(())
                }
                Some(res) = event_futures.next() => {
                    let res: PyResult<PyObject> = res;
                    res.map(|_| ()).map_err(Into::into)
                }
                Some(event) = event_rx.recv() => {
                    dispatch_event(&event_handlers, &event, &mut event_futures)
                }
                Some(event) = conn_rx.recv() => {
                    if let Some(client) = state.clients.get_mut(event.session_id) {
                        client.process_event(event.event, &mut event_futures)
                    } else {
                        Ok(())
                    }
                }
                Some(Ok(event)) = crossterm_events.next().fuse() => {
                    match self.handle_term_event(&mut state, &mut event_futures, &event).await {
                        Ok(Some(action)) => {
                            self.handle_tab_action(&mut state, action).await
                        },
                        Err(err) => Err(err),
                        _ => Ok(()),
                    }
                },
                Some(event) = config_event_rx.next() => {
                    if let Ok(event) = event {
                        config_reload_event(&self.config, &mut self.tabs, &mut state, &event);
                    }
                    Ok(())
               }
            };
            if let Err(err) = res {
                error!("{err}");
                state.ui_state = err.into();
            }
        }

        info!("disconnecting all clients");
        // TODO(XXX): Dumb and serial. Should do this in parallel.
        for client in state_lock.write().await.clients.values_mut() {
            client.disconnect().await?;
        }

        restore_terminal()
    }

    fn draw(&mut self, state: &mut State, terminal: &mut Terminal<impl Backend>) {
        terminal
            .draw(|frame| {
                let area = frame.area();

                frame.render_widget(Clear, area);

                if let Err(err) = self.draw_tabs(state, frame, area) {
                    state.ui_state = err.into();
                }

                if let UiState::Error(error) = &state.ui_state {
                    draw_error_popup(frame, area, error);
                }
            })
            .map(|_| ())
            .unwrap();
    }

    fn draw_tabs(&mut self, state: &mut State, frame: &mut Frame<'_>, area: Rect) -> Result<()> {
        assert!(
            state.selected_tab < self.tabs.len(),
            "selected tab out of bounds"
        );
        let [tab_bar, tab_content] = Layout::vertical([Length(3), Fill(0)]).areas(area);

        self.tabs[state.selected_tab].draw(state, frame, tab_content)?;

        let mut titles: Vec<Line> = Vec::with_capacity(self.tabs.len());
        for (tab_id, tab) in self.tabs.iter().enumerate() {
            titles.push(match tab.session_id() {
                Some(sesh_id) => {
                    let sesh = state
                        .clients
                        .get(sesh_id)
                        .ok_or(Error::UnknownSession(sesh_id))?;
                    let sesh_focused = state.selected_tab == tab_id;

                    if sesh.output.new_data > 0 && !sesh_focused {
                        vec![
                            Span::styled(
                                tab.title().to_string(),
                                Style::default()
                                    .fg(Color::Magenta)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(" [{}]", sesh.output.new_data),
                                Style::default().add_modifier(Modifier::DIM),
                            ),
                        ]
                        .into()
                    } else {
                        vec![Span::styled(
                            tab.title().to_string(),
                            Style::default().fg(Color::Magenta),
                        )]
                        .into()
                    }
                }
                None => vec![Span::styled(
                    tab.title().to_string(),
                    Style::default().fg(Color::Magenta),
                )]
                .into(),
            });
        }

        let tabs = Tabs::new(titles)
            .select(state.selected_tab)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::LightMagenta))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(CRATE_NAME.to_uppercase()),
            );
        frame.render_widget(tabs, tab_bar);
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self, state, event_futures, event))]
    async fn handle_term_event(
        &mut self,
        state: &mut State,
        event_futures: &mut FuturesUnordered<python::PyFuture>,
        event: &TermEvent,
    ) -> Result<Option<TabAction>, Error> {
        match event {
            TermEvent::Key(key_event) => {
                self.handle_key_event(state, event_futures, key_event).await
            }
            TermEvent::Mouse(mouse_event) => {
                self.handle_mouse_event(state, event_futures, mouse_event)
                    .await
            }
            _ => {
                let Some(current_tab) = self.tabs.get_mut(state.selected_tab) else {
                    return Ok(None);
                };
                current_tab.term_event(state, event_futures, event)
            }
        }
    }

    async fn handle_key_event(
        &mut self,
        state: &mut State,
        event_futures: &mut FuturesUnordered<python::PyFuture>,
        key_event: &KeyEvent,
    ) -> Result<Option<TabAction>, Error> {
        let Some(current_tab) = self.tabs.get_mut(state.selected_tab) else {
            return Ok(None);
        };

        // Ignore release & repeat events. These only happen on Windows in practice and we're not that specific.
        if key_event.kind != KeyEventKind::Press {
            return Ok(None);
        }

        // If we're in an error state - handle the keys for dismissing the popup.
        if let UiState::Error(err) = &state.ui_state {
            if err.fatal() {
                state.ui_state = UiState::Exit;
                return Ok(None);
            }
            match key_event.code {
                crossterm::event::KeyCode::Char('c') => {
                    trace!("dismissed error ui_state");
                    state.ui_state = UiState::Running;
                }
                crossterm::event::KeyCode::Char('q') => {
                    state.ui_state = UiState::Exit;
                    return Ok(None);
                }
                _ => {} // Eat other key presses.
            }
            return Ok(None);
        }

        // If there's a keybinding for this key event we want to translate it into a shortcut and
        // not dispatch the key event to the current tab - it'll get the shortcut instead.
        //
        // If there's no keybinding, then dispatch the raw key event.
        let Some(shortcut) = self.config.key_binding(current_tab.input_mode(), key_event) else {
            return current_tab.term_event(state, event_futures, &TermEvent::Key(*key_event));
        };

        trace!("mapped {key_event:?} to shortcut: {shortcut:?}");

        // Intercept Quit to exit the application.
        if let Shortcut::Quit = shortcut {
            state.ui_state = UiState::Exit;
            return Ok(None);
        }

        // If the shortcut is a tab action, handle it ourselves.
        if let Ok(tab_action) = TabAction::try_from(shortcut) {
            return self
                .handle_tab_action(state, tab_action)
                .await
                .map(|()| None);
        }

        // Let the current table handle everything else.
        current_tab.shortcut(state, shortcut).await
    }

    async fn handle_tab_action(
        &mut self,
        state: &mut State,
        action: TabAction,
    ) -> Result<(), Error> {
        match action {
            TabAction::New {
                session_info,
                switch,
            } => return self.new_session(state, session_info, switch).await,
            TabAction::Next => {
                state.selected_tab = (state.selected_tab + 1) % self.tabs.len();
            }
            TabAction::Prev => {
                state.selected_tab = (state.selected_tab + self.tabs.len() - 1) % self.tabs.len();
            }
            TabAction::Close => {
                self.tabs.remove(state.selected_tab);
                state.selected_tab = state.selected_tab.saturating_sub(1);
            }
            TabAction::SwapLeft => {
                if state.selected_tab > 1 {
                    self.tabs.swap(state.selected_tab, state.selected_tab - 1);
                    state.selected_tab -= 1;
                }
            }
            TabAction::SwapRight => {
                if state.selected_tab + 1 < self.tabs.len() {
                    self.tabs.swap(state.selected_tab, state.selected_tab + 1);
                    state.selected_tab += 1;
                }
            }
        }

        state.active_session_id = self
            .tabs
            .get(state.selected_tab)
            .and_then(|tab| tab.session_id());

        let new_title = self
            .tabs
            .get(state.selected_tab)
            .map(|t| t.title())
            .unwrap_or_default();
        info!("switched to tab {} ({})", state.selected_tab, new_title);
        // TODO(XXX): tab switch event?
        Ok(())
    }

    async fn handle_mouse_event(
        &mut self,
        state: &mut State,
        event_futures: &mut FuturesUnordered<python::PyFuture>,
        mouse_event: &MouseEvent,
    ) -> Result<Option<TabAction>, Error> {
        let Some(current_tab) = self.tabs.get_mut(state.selected_tab) else {
            return Ok(None);
        };

        // If mouse scroll isn't enabled, forward the mouse event to the current tab.
        if !self.config.lookup(|c| c.mouse_scroll, false) {
            return current_tab.term_event(state, event_futures, &TermEvent::Mouse(*mouse_event));
        }

        // If possible, translate the mouse event into a shortcut and handle it that way.
        if let Ok(shortcut) = Shortcut::try_from(*mouse_event) {
            return current_tab.shortcut(state, shortcut).await.map(|_| None);
        }

        // Any other mouse events go to the current tab.
        current_tab.term_event(state, event_futures, &TermEvent::Mouse(*mouse_event))
    }

    async fn new_session(
        &mut self,
        state: &mut State,
        session_info: Arc<SessionInfo>,
        switch: bool,
    ) -> Result<(), Error> {
        trace!("creating new session tab for {session_info}");
        let tab = Box::new(session::Widget::new(
            self.config.clone(),
            session_info.clone(),
        )?);
        self.tabs.push(tab);
        // TODO(XXX): new tab event?

        if switch {
            state.selected_tab = self.tabs.len() - 1;
            let new_title = self
                .tabs
                .get(state.selected_tab)
                .map(|t| t.title())
                .unwrap_or_default();
            info!("switched to tab {} ({})", state.selected_tab, new_title);
        }

        let clients = &mut state.clients;
        let Some(client) = clients.get_mut(session_info.id) else {
            warn!("missing client for new tab action: {session_info}");
            return Ok(());
        };

        client.connect().await?;
        Ok(())
    }
}

fn draw_error_popup(frame: &mut Frame, area: Rect, error: &Error) {
    let popup_area = centered_rect(area, 80, 90);
    frame.render_widget(Clear, popup_area);

    let [error_text, help] = Layout::vertical([Min(5), Max(3)]).areas(popup_area);

    frame.render_widget(
        Paragraph::new::<Text>(format!("{error}").into()).block(
            Block::default()
                .borders(Borders::all())
                .border_style(Color::Red)
                .title("Error"),
        ),
        error_text,
    );

    // TODO(XXX): styling plz.
    let help_text: Text = match error.fatal() {
        false => "Press 'q' to exit or 'c' to continue".to_string(),
        true => "Press any key to exit.".to_string(),
    }
    .into();

    let help_paragraph = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(Color::Red),
    );
    frame.render_widget(help_paragraph, help);
}

fn dispatch_event(
    event_handlers: &Py<python::EventHandlers>,
    event: &python::Event,
    futures: &mut FuturesUnordered<python::PyFuture>,
) -> Result<(), Error> {
    // Dispatch the event to each registered event handler.
    Python::with_gil(|py| {
        event_handlers
            .bind(py)
            .borrow()
            .dispatch(py, event, futures)
    })
}

fn config_reload_init() -> NotifyResult<(RecommendedWatcher, Receiver<NotifyResult<NotifyEvent>>)> {
    let (mut config_event_tx, config_event_rx) = futures_channel(1);
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            futures::executor::block_on(async {
                config_event_tx.send(res).await.unwrap();
            });
        },
        notify::Config::default(),
    )?;

    let config_dir_path = config_dir();
    info!("registering watch for {}", config_dir_path.display());
    watcher.watch(config_dir_path, RecursiveMode::NonRecursive)?;

    Ok((watcher, config_event_rx))
}

fn config_reload_event(
    config: &GlobalConfig,
    tabs: &mut [Box<dyn Tab>],
    state: &mut State,
    event: &NotifyEvent,
) {
    use notify::event::ModifyKind;
    use notify::EventKind;

    let data_changed = matches!(
        event.kind,
        EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Any)
    );
    let created = matches!(event.kind, EventKind::Create(_));
    // TODO(XXX): Consider reloading python stuff automatically?
    // TODO(XXX): Consider debouncing created->data_changed events.
    if !event.paths.contains(&config_file()) || !(created || data_changed) {
        //trace!("skipping unrelated config dir event: {event:?}");
        return;
    }
    info!(
        "reloading configuration: {}",
        if data_changed {
            "data changed"
        } else {
            "created"
        }
    );
    // Reload the config from disk.
    if let Err(err) = config.reload() {
        error!("{err}");
        state.ui_state = err.into();
    }
    // Notify each tab to reprocess the updated config.
    if let Err(err) = tabs.iter_mut().try_for_each(|tab| tab.reload_config()) {
        error!("{err}");
        state.ui_state = err.into();
    }
    let _ = state.event_tx.send(python::Event::ConfigReloaded {});
}

#[async_trait]
pub trait Tab: Debug + Send + Sync {
    fn kind(&self) -> TabKind;

    fn input_mode(&self) -> InputMode;

    fn title(&self) -> Line;

    #[must_use]
    fn session_id(&self) -> Option<u32> {
        match self.kind() {
            TabKind::MudList {} => None,
            TabKind::Session { session } => Some(session.id),
        }
    }

    /// The component should reload its configuration if appropriate.
    ///
    /// # Errors
    /// If the updated configuration is invalid, an error should be returned to communicate
    /// to the user.
    fn reload_config(&mut self) -> Result<(), Error> {
        Ok(())
    }

    async fn shortcut(
        &mut self,
        _state: &mut State,
        _shortcut: Shortcut,
    ) -> Result<Option<TabAction>, Error> {
        Ok(None)
    }

    /// Handle a terminal event when the Tab is active.
    ///
    /// An optional tab action can be returned to spawn, change, or close tabs.
    ///
    /// # Errors
    /// Return an error if the event provokes an error state.
    // TODO(XXX): is this needed? Session only uses it to forward key/mouse term events. Never
    //   returns a tab action.
    fn term_event(
        &mut self,
        _state: &mut State,
        _futures: &mut FuturesUnordered<python::PyFuture>,
        _event: &TermEvent,
    ) -> Result<Option<TabAction>, Error> {
        Ok(None)
    }

    /// Draw the component.
    ///
    /// # Errors
    /// Return an error if the component fails to draw.
    fn draw(&mut self, state: &mut State, frame: &mut Frame<'_>, area: Rect) -> Result<()>;
}

// TODO(XXX): split into a simple enum for type, and a variant enum for state.
#[derive(Debug)]
pub enum TabKind {
    MudList {},
    Session { session: Arc<SessionInfo> },
}

impl TabKind {
    #[must_use]
    pub fn config_key(&self) -> &'static str {
        match self {
            Self::MudList {} => "mudlist",
            Self::Session { .. } => "mud",
        }
    }
}

pub struct State {
    pub ui_state: UiState,
    pub event_tx: UnboundedSender<python::Event>,
    pub active_session_id: Option<u32>,
    pub timers: IdMap<Timer>,

    config: GlobalConfig,
    selected_tab: usize,
    clients: IdMap<Client>,
    conn_tx: UnboundedSender<connection::Event>,
}

impl State {
    #[must_use]
    pub fn new(
        config: GlobalConfig,
        event_tx: UnboundedSender<python::Event>,
        conn_tx: UnboundedSender<connection::Event>,
    ) -> Self {
        Self {
            ui_state: UiState::default(),
            event_tx,
            active_session_id: None,
            timers: IdMap::default(),
            config,
            selected_tab: 0,
            clients: IdMap::default(),
            conn_tx,
        }
    }

    /// # Errors
    /// If the event channel is full, an error is returned.
    pub fn new_session(&mut self, mud: Mud) -> Result<Arc<SessionInfo>> {
        let new_id = self.clients.construct(|id| {
            Client::new(
                Arc::new(SessionInfo {
                    id,
                    mud_name: mud.name.clone(),
                }),
                self.config.clone(),
                self.event_tx.clone(),
                self.conn_tx.clone(),
            )
        });

        self.active_session_id = Some(new_id);
        // Safety: we just constructed this ID above.
        let info = self
            .clients
            .get(new_id)
            .ok_or(Error::UnknownSession(new_id))?
            .info
            .clone();

        self.event_tx.send(python::Event::NewSession {
            id: new_id,
            info: info.as_ref().clone(),
            mud,
        })?;

        Ok(info)
    }

    pub fn client_for_id_mut(&mut self, session_id: u32) -> Option<&mut Client> {
        self.clients.get_mut(session_id)
    }

    pub fn client_for_id(&self, session_id: u32) -> Option<&Client> {
        self.clients.get(session_id)
    }

    pub fn client_ids(&self) -> Vec<u32> {
        self.clients.ids()
    }

    pub fn all_client_info(&self) -> Vec<SessionInfo> {
        self.clients
            .iter()
            .map(|(_, client)| client.info.as_ref().clone())
            .collect()
    }
}

impl Debug for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("ui_state", &self.ui_state)
            .field("selected_tab", &self.selected_tab)
            .field("clients", &self.clients)
            .field("timers", &self.timers)
            .field("active_session_id", &self.active_session_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub enum UiState {
    #[default]
    Running,
    ReloadPython,
    Error(Error),
    Exit,
}

impl From<Error> for UiState {
    fn from(err: Error) -> Self {
        Self::Error(err)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum TabAction {
    New {
        session_info: Arc<SessionInfo>,
        switch: bool,
    },
    Close,
    Next,
    Prev,
    SwapLeft,
    SwapRight,
}

impl TryFrom<Shortcut> for TabAction {
    type Error = ();

    fn try_from(shortcut: Shortcut) -> std::result::Result<Self, Self::Error> {
        Ok(match shortcut {
            Shortcut::TabNext => Self::Next,
            Shortcut::TabPrev => Self::Prev,
            Shortcut::TabClose => Self::Close,
            Shortcut::TabSwapLeft => Self::SwapLeft,
            Shortcut::TabSwapRight => Self::SwapRight,
            _ => return Err(()),
        })
    }
}

fn init_terminal(mouse_enabled: bool) -> io::Result<Terminal<impl Backend>> {
    enable_raw_mode()?;

    let mut out = stdout();
    out.execute(EnterAlternateScreen)?;

    if mouse_enabled {
        debug!("enabling mouse capture");
        out.execute(crossterm::event::EnableMouseCapture)?;
    }

    // TODO(XXX): should support bracketed paste here w/ EnableBracketedPaste.

    // increase the cache size to avoid flickering for indeterminate layouts
    Layout::init_cache(NonZeroUsize::new(100).unwrap());
    Terminal::new(CrosstermBackend::new(out))
}

pub(crate) fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    stdout().execute(crossterm::event::DisableMouseCapture)?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    fn layout_split(area: Rect, dir: Direction, percent: u16) -> Rect {
        Layout::default()
            .direction(dir)
            .constraints([
                Constraint::Percentage((100 - percent) / 2),
                Constraint::Percentage(percent),
                Constraint::Percentage((100 - percent) / 2),
            ])
            .split(area)[1]
    }

    layout_split(
        layout_split(area, Direction::Vertical, percent_y),
        Direction::Horizontal,
        percent_x,
    )
}
