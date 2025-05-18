use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use pyo3::Python;
use ratatui::Frame;
use ratatui::layout::Constraint::{Max, Min};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use tracing::info;

use crate::app::AppData;
use crate::config::{Config, config_file};
use crate::error::Error;
use crate::session::Character;
use crate::tui::{Tab, TabAction};

#[derive(Debug, Default)]
pub(crate) struct Mudlist {
    list: List<'static>,
    state: ListState,
    characters: Vec<Character>,
}

impl Mudlist {
    pub(crate) fn new(config: &Config) -> Self {
        let mut ml = Self::default();
        ml.load(config);
        ml
    }

    fn load(&mut self, config: &Config) {
        let items = config
            .characters
            .iter()
            .map(|mud| ListItem::new(mud.name.clone()))
            .collect::<Vec<_>>();

        self.state = ListState::default();
        self.state.select(items.first().map(|_| 0));
        self.characters.clone_from(&config.characters);
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
}

impl Tab for Mudlist {
    fn title(&self) -> Line<'_> {
        Line::from("MUDs")
    }

    fn render(
        &mut self,
        _: &mut AppData,
        f: &mut Frame<'_>,
        tab_content: Rect,
    ) -> Result<(), Error> {
        let [mud_list, help] = Layout::vertical([Min(10), Max(5)]).areas(tab_content);

        draw_help(f, help);

        if self.list.is_empty() {
            f.render_widget::<Text>("No MUDs configured...".into(), mud_list);
        } else {
            f.render_stateful_widget(&self.list, mud_list, &mut self.state);
        }

        Ok(())
    }

    // TODO(XXX): this misses direct mutation from Python of the Py<Config>.
    fn config_reloaded(&mut self, config: &Config) -> Result<(), Error> {
        self.load(config);
        Ok(())
    }

    // TODO(XXX): key bindings lookup
    fn crossterm_event(
        &mut self,
        app: &mut AppData,
        event: &Event,
    ) -> Result<Option<TabAction>, Error> {
        match event {
            Event::Key(crossterm::event::KeyEvent {
                code: KeyCode::Up,
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::NONE,
                ..
            }) => {
                self.state.select_previous();
            }
            Event::Key(crossterm::event::KeyEvent {
                code: KeyCode::Down,
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::NONE,
                ..
            }) => {
                self.state.select_next();
            }
            Event::Key(crossterm::event::KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::NONE,
                ..
            }) => {
                let Some(selected) = self.state.selected() else {
                    return Ok(None);
                };
                if selected >= self.characters.len() {
                    return Ok(None);
                }

                let selected = self.characters.get(selected).unwrap();
                info!("spawning session for {selected}");
                let sesh = app.new_session(selected)?;

                // TODO(XXX): update to not require GIL.
                Python::with_gil(|py| sesh.connect(py))?;

                return Ok(Some(TabAction::Create(sesh)));
            }
            _ => {}
        }

        Ok(None)
    }
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
    frame.render_widget(
        Paragraph::new(help_text).block(Block::default().title("Help:").borders(Borders::ALL)),
        area,
    );
}
