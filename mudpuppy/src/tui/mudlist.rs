use async_trait::async_trait;
use ratatui::layout::Constraint::{Max, Min};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use tracing::{info, instrument, Level};

use crate::app::{State, Tab, TabAction, TabKind};
use crate::config::{config_file, GlobalConfig};
use crate::error::Error;
use crate::model::{InputMode, Mud, Shortcut};
use crate::Result;

#[derive(Debug)]
pub struct Widget {
    config: GlobalConfig,
    muds: Vec<Mud>,
    list: List<'static>,
    state: ListState,
}

impl Widget {
    pub fn new(config: GlobalConfig) -> Self {
        let mut widget = Self {
            config,
            muds: Vec::default(),
            list: List::default(),
            state: ListState::default(),
        };
        widget.load();
        widget
    }

    fn load(&mut self) {
        let muds = self
            .config
            .lookup(|config| config.muds.clone(), Vec::default());
        let items = muds
            .iter()
            .map(|mud| ListItem::new(mud.name.clone()))
            .collect::<Vec<_>>();

        self.state = ListState::default();
        self.state.select(items.first().map(|_| 0));
        self.muds = muds;
        self.list = List::new(items)
            .block(
                Block::default()
                    .title("Choose a MUD")
                    .borders(Borders::ALL)
                    .border_style(Color::Magenta),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::LightMagenta)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("➠ ");
    }

    fn draw_help(frame: &mut Frame<'_>, area: Rect) {
        let help_text: Vec<Line> = vec![
            format!(
                "* Edit {} to add/edit/remove MUDs. This list will reload automatically.",
                config_file().display()
            )
            .into(),
            "* Use the arrow keys to select a MUD in the list.".into(),
            "* Press enter to connect to a MUD.".into(),
        ];
        let help_paragraph =
            Paragraph::new(help_text).block(Block::default().title("Help:").borders(Borders::ALL));
        frame.render_widget(help_paragraph, area);
    }
}

#[async_trait]
impl Tab for Widget {
    fn kind(&self) -> TabKind {
        TabKind::MudList {}
    }

    fn input_mode(&self) -> InputMode {
        InputMode::MudList
    }

    // TODO(XXX): Text styling.
    fn title(&self) -> Line {
        "MUDs".into()
    }

    fn reload_config(&mut self) -> Result<(), Error> {
        self.load();
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self, state))]
    async fn shortcut(
        &mut self,
        state: &mut State,
        shortcut: Shortcut,
    ) -> Result<Option<TabAction>, Error> {
        match shortcut {
            Shortcut::MudListNext => {
                self.state.select_next();
            }
            Shortcut::MudListPrev => {
                self.state.select_previous();
            }
            Shortcut::MudListConnect => {
                if let Some(selected) = self.state.selected() {
                    if selected >= self.muds.len() {
                        return Ok(None);
                    }
                    let session_info = state.new_session(self.muds[selected].clone())?;
                    info!("created new session {session_info}");
                    return Ok(Some(TabAction::New {
                        session_info,
                        switch: true,
                    }));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    // TODO(XXX): Text styling.
    fn draw(&mut self, _state: &mut State, frame: &mut Frame<'_>, area: Rect) -> Result<()> {
        let [mud_list, help] = Layout::vertical([Min(10), Max(5)]).areas(area);

        Self::draw_help(frame, help);

        if self.list.is_empty() {
            frame.render_widget::<Text>("No MUDs configured...".into(), mud_list);
        } else {
            frame.render_stateful_widget(&self.list, mud_list, &mut self.state);
        }

        Ok(())
    }
}
