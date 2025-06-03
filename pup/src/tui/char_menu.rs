use async_trait::async_trait;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use pyo3::{Py, PyRef, Python};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use tracing::{debug, info};

use crate::app::{AppData, TabAction};
use crate::config::{Config, config_file};
use crate::error::Error;
use crate::session::Character;
use crate::tui::{Constraint, Section, Tab};

#[derive(Debug)]
pub(crate) struct CharacterMenu {
    list: List<'static>,
    state: ListState,
    characters: Vec<Character>,
    layout: Py<Section>,
}

impl CharacterMenu {
    pub(crate) fn new(config: &Config) -> Self {
        let mut ml = Self {
            list: List::default(),
            state: ListState::default(),
            characters: Vec::default(),
            layout: initial_layout(),
        };
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
                    .title("Choose a character")
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

#[async_trait]
impl Tab for CharacterMenu {
    fn title(&self, _: &AppData) -> String {
        "Menu".to_string()
    }

    fn render(
        &mut self,
        _: &mut AppData,
        f: &mut Frame<'_>,
        tab_content: Rect,
    ) -> Result<(), Error> {
        let sections = Python::with_gil(|py| {
            let layout: PyRef<'_, Section> = self.layout.extract(py)?;
            layout.partition_by_name(py, tab_content)
        })?;

        // Safety: we unconditionally create these sections in layout init.
        let char_list = sections.get(CHAR_LIST_SECTION_NAME).unwrap();
        let help = sections.get(CHAR_LIST_HELP_SECTION_NAME).unwrap();

        draw_help(f, *help);

        if self.list.is_empty() {
            // TODO(XXX): styling.
            f.render_widget::<Text>("No characters configured...".into(), *char_list);
        } else {
            f.render_stateful_widget(&self.list, *char_list, &mut self.state);
        }

        Ok(())
    }

    // TODO(XXX): this misses direct mutation from Python of the Py<Config>.
    fn config_reloaded(&mut self, config: &Config) -> Result<(), Error> {
        self.load(config);
        Ok(())
    }

    fn layout(&self) -> Py<Section> {
        Python::with_gil(|py| self.layout.clone_ref(py))
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
                info!("creating session for {selected}");
                let session = app.new_session(selected)?;

                // TODO(XXX): update to not require GIL.
                Python::with_gil(|py| session.connect(py))?;

                return Ok(Some(TabAction::Create { session }));
            }
            _ => {}
        }

        Ok(None)
    }
}

fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let help_text: Vec<Line> = vec![
        format!(
            "* Edit {} to add/edit/remove characters. This list will reload automatically.",
            config_file().display()
        )
        .into(),
        "* Use the arrow keys to select a character in the list.".into(),
        "* Press enter to create a new session.".into(),
    ];
    frame.render_widget(
        Paragraph::new(help_text).block(Block::default().title("Help:").borders(Borders::ALL)),
        area,
    );
}

fn initial_layout() -> Py<Section> {
    Python::with_gil(|py| {
        debug!("configuring initial layout");
        let char_list = Section::new(py, CHAR_LIST_SECTION_NAME.to_string());
        let help = Section::new(py, CHAR_LIST_HELP_SECTION_NAME.to_string());
        let mut root = Section::new(py, CHAR_LIST_SECTION_ROOT_NAME.to_string());
        root.append_child(py, Constraint::Min(10), char_list)?;
        root.append_child(py, Constraint::Max(5), help)?;
        Py::new(py, root)
    })
    .unwrap() // Safety: no chance for duplicate sections.
}

const CHAR_LIST_SECTION_ROOT_NAME: &str = "Character List Tab";

const CHAR_LIST_SECTION_NAME: &str = "Characters";
const CHAR_LIST_HELP_SECTION_NAME: &str = "Characters List Help";
