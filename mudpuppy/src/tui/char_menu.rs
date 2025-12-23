use std::collections::HashMap;

use pyo3::{Py, Python};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use tracing::{debug, error, info};

use crate::app::{self, AppData, TabAction};
use crate::config::{Config, config_file};
use crate::error::Error;
use crate::keyboard::{KeyCode, KeyEvent, KeyModifiers};
use crate::shortcut::{MenuShortcut, Shortcut};
use crate::tui::chrome::{TabData, TabKind};
use crate::tui::{Constraint, Section, Tab};

#[derive(Debug)]
pub(crate) struct CharacterMenu {
    state: ListState,
    config: Py<Config>,
}

impl CharacterMenu {
    pub(crate) fn new_tab(config: &Py<Config>) -> Tab {
        let mut state = ListState::default();
        state.select(Some(0));
        Tab {
            data: TabData::new(
                "Menu".to_string(),
                initial_layout(),
                Some(default_shortcuts()),
            ),
            kind: TabKind::Menu(Box::new(Self {
                state,
                config: Python::attach(|py| config.clone_ref(py)),
            })),
        }
    }

    fn sorted_characters(&self) -> Vec<(String, String)> {
        let mut characters = Python::attach(|py| {
            self.config
                .borrow(py)
                .characters
                .iter(py)
                .map(|(name, char)| {
                    let mud = char.borrow(py).mud.clone();
                    (name, mud)
                })
                .collect::<Vec<_>>()
        });
        characters
            .sort_by(|(name_a, mud_a), (name_b, mud_b)| (name_a, mud_a).cmp(&(name_b, mud_b)));
        characters
    }

    pub(crate) fn render(
        &mut self,
        _: &mut AppData,
        f: &mut Frame<'_>,
        sections: &HashMap<String, Rect>,
    ) {
        // Safety: we unconditionally create these sections in layout init.
        let char_list = sections.get(CHAR_LIST_SECTION_NAME).unwrap();
        let help = sections.get(CHAR_LIST_HELP_SECTION_NAME).unwrap();

        draw_help(f, *help);

        let items = self
            .sorted_characters()
            .iter()
            .map(|(name, mud)| ListItem::new(format!("{name}@{mud}")))
            .collect::<Vec<_>>();

        // Ensure we have a valid selection if the list isn't empty
        if !items.is_empty() && self.state.selected().is_none() {
            self.state.select(Some(0));
        } else if items.is_empty() {
            self.state.select(None);
        }

        let list = List::new(items)
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

        if list.is_empty() {
            // TODO(XXX): styling.
            f.render_widget::<Text>("No characters configured...".into(), *char_list);
        } else {
            f.render_stateful_widget(&list, *char_list, &mut self.state);
        }
    }

    pub(crate) fn shortcut(
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
                let characters = self.sorted_characters();
                let Some((name, _)) = characters.get(selected) else {
                    error!(
                        "selected character index {selected} out of bounds (len: {})",
                        characters.len()
                    );
                    return Ok(None);
                };

                info!(item = ?name, "list item selected, creating session");
                let (session, handles) = app.new_session(name)?;

                let session_clone = session.clone();
                tokio::spawn(async move {
                    app::join_all(handles, "new session handler panicked").await;
                    if let Err(e) = Python::attach(|py| session_clone.connect(py)) {
                        error!("failed to connect session: {e}");
                    }
                });

                Ok(Some(TabAction::CreateSessionTab { session }))
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
    Python::attach(|py| {
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
