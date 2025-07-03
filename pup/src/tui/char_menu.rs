use async_trait::async_trait;
use pyo3::{Py, PyRef, Python};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use std::collections::HashMap;
use tracing::{debug, error, info};

use crate::app::{AppData, TabAction};
use crate::config::{Config, config_file};
use crate::error::Error;
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::session::Character;
use crate::shortcut::{MenuShortcut, Shortcut};
use crate::tui::{Constraint, Section, Tab};

#[derive(Debug)]
pub(crate) struct CharacterMenu {
    list: List<'static>,
    state: ListState,
    characters: Vec<Character>,
    layout: Py<Section>,
    shortcuts: HashMap<KeyEvent, Shortcut>,
}

impl CharacterMenu {
    pub(crate) fn new(config: &Config) -> Self {
        let mut ml = Self {
            list: List::default(),
            state: ListState::default(),
            characters: Vec::default(),
            layout: initial_layout(),
            shortcuts: default_shortcuts(),
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

    fn all_shortcuts(&self, _: &AppData) -> Result<HashMap<KeyEvent, Shortcut>, Error> {
        Ok(self.shortcuts.clone())
    }

    fn lookup_shortcut(
        &self,
        _app: &AppData,
        key_event: &KeyEvent,
    ) -> Result<Option<Shortcut>, Error> {
        Ok(self.shortcuts.get(key_event).cloned())
    }

    fn set_shortcut(
        &mut self,
        _app: &mut AppData,
        key_event: &KeyEvent,
        shortcut: Option<Shortcut>,
    ) -> Result<(), Error> {
        match shortcut {
            None => {
                if let Some((key_event, shortcut)) = self.shortcuts.remove_entry(key_event) {
                    info!(key_event=?key_event, shortcut=shortcut.to_string(), "shortcut removed");
                }
            }
            Some(shortcut) => {
                info!(key_event=?key_event, shortcut=shortcut.to_string(), "shortcut added");
                self.shortcuts.insert(*key_event, shortcut);
            }
        }
        Ok(())
    }

    async fn shortcut(
        &mut self,
        app: &mut AppData,
        shortcut: &Shortcut,
    ) -> Result<Option<TabAction>, Error> {
        let Shortcut::Menu(shortcut) = shortcut else {
            return Ok(None);
        };

        match shortcut {
            MenuShortcut::Up => {
                self.state.select_previous();
                Ok(None)
            }
            MenuShortcut::Down => {
                self.state.select_next();
                Ok(None)
            }
            MenuShortcut::Connect => {
                let Some(selected) = self.state.selected() else {
                    return Ok(None);
                };
                let Some(character) = self.characters.get(selected) else {
                    error!(
                        "selected character index {selected} out of bounds (len: {})",
                        self.characters.len()
                    );
                    return Ok(None);
                };

                info!("creating session for {character}");
                let session = app.new_session(character)?;
                Python::with_gil(|py| session.connect(py))?;
                Ok(Some(TabAction::Create { session }))
            }
        }
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

fn default_shortcuts() -> HashMap<KeyEvent, Shortcut> {
    HashMap::from([
        (
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
            },
            MenuShortcut::Up.into(),
        ),
        (
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
            },
            MenuShortcut::Down.into(),
        ),
        (
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
            },
            MenuShortcut::Connect.into(),
        ),
    ])
}

const CHAR_LIST_SECTION_ROOT_NAME: &str = "Character List Tab";

const CHAR_LIST_SECTION_NAME: &str = "Characters";
const CHAR_LIST_HELP_SECTION_NAME: &str = "Characters List Help";
