use std::fmt::Debug;

use pyo3::Py;
use ratatui::Frame;
use ratatui::layout::Constraint::{Fill, Length};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::{AppData, TabAction};
use crate::config::{CRATE_NAME, Config};
use crate::error::{Error, ErrorKind};
use crate::tui::{CharacterMenu, Section};

#[derive(Debug)]
pub(crate) struct Chrome {
    active_tab_id: u32,
    next_tab_id: u32,
    tabs: Vec<TabInfo>,
    // TODO(XXX): Py<Layout>!
}

impl Chrome {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            active_tab_id: 0, // ID 0 is the character menu
            next_tab_id: 1,
            tabs: vec![TabInfo {
                id: 0,
                tab: Box::new(CharacterMenu::new(config)),
                position: 0,
            }],
        }
    }

    // TODO(XXX): Styling.
    pub(crate) fn render(&mut self, app: &mut AppData, f: &mut Frame) -> Result<(), Error> {
        // TODO(XXX): Py<Layout>!
        let [tab_bar, tab_content] = Layout::vertical([Length(3), Fill(0)]).areas(f.area());

        // Sort tabs by position for rendering
        let sorted_tabs = self.tabs();

        // Find the index of the active tab in the sorted order for selection
        let active_idx = sorted_tabs
            .iter()
            .position(|tab_info| tab_info.id == self.active_tab_id)
            .unwrap_or(0);

        f.render_widget(
            Tabs::new(sorted_tabs.iter().map(|t| t.tab.rendered_title(app)))
                .select(active_idx)
                .highlight_style(Style::default().fg(Color::Black).bg(Color::LightMagenta))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(CRATE_NAME.to_uppercase()),
                ),
            tab_bar,
        );

        self.active_tab().render(app, f, tab_content)
    }

    pub(crate) fn config_reloaded(&mut self, config: &Config) -> Result<(), Error> {
        for tab_info in &mut self.tabs {
            tab_info.tab.config_reloaded(config)?;
        }
        Ok(())
    }

    pub(crate) fn active_tab(&mut self) -> &mut dyn Tab {
        // This unwrap is safe because we manage active_tab_id internally
        // and always ensure it corresponds to an existing tab
        let index = self.find_tab_by_id(self.active_tab_id).unwrap();
        self.tabs[index].tab.as_mut()
    }

    pub(crate) fn active_tab_id(&self) -> u32 {
        self.active_tab_id
    }

    pub(crate) fn get_tab(&self, id: u32) -> Result<&dyn Tab, Error> {
        let index = self.find_tab_by_id(id).ok_or(ErrorKind::InvalidTabId(id))?;
        Ok(self.tabs[index].tab.as_ref())
    }

    pub(crate) fn get_tab_mut(&mut self, id: u32) -> Result<&mut dyn Tab, Error> {
        let index = self.find_tab_by_id(id).ok_or(ErrorKind::InvalidTabId(id))?;
        Ok(self.tabs[index].tab.as_mut())
    }

    pub(crate) fn tabs(&self) -> Vec<&TabInfo> {
        let mut tabs: Vec<_> = self.tabs.iter().collect();
        tabs.sort_by_key(|tab_info| tab_info.position);
        tabs
    }

    pub(crate) fn tab_for_session(&self, session_id: u32) -> Option<&TabInfo> {
        self.tabs
            .iter()
            .find(|tab_info| tab_info.tab.session_id() == Some(session_id))
    }

    pub(crate) fn new_tab(&mut self, tab: impl Tab + 'static) -> u32 {
        let position = self.tabs.len();
        let id = self.next_tab_id;
        self.next_tab_id += 1;

        self.tabs.push(TabInfo {
            id,
            tab: Box::new(tab),
            position,
        });

        self.active_tab_id = id;
        self.active_tab_id
    }

    pub(crate) fn next_tab(&mut self) -> u32 {
        if self.tabs.len() <= 1 {
            return self.active_tab_id;
        }

        let current_idx = self.find_tab_by_id(self.active_tab_id).unwrap();
        let next_idx = (current_idx + 1) % self.tabs.len();
        self.active_tab_id = self.tabs[next_idx].id;
        self.active_tab_id
    }

    pub(crate) fn previous_tab(&mut self) -> u32 {
        if self.tabs.len() <= 1 {
            return self.active_tab_id;
        }

        let current_idx = self.find_tab_by_id(self.active_tab_id).unwrap();
        let prev_idx = if current_idx == 0 {
            self.tabs.len() - 1
        } else {
            current_idx - 1
        };
        self.active_tab_id = self.tabs[prev_idx].id;
        self.active_tab_id
    }

    pub(crate) fn close_tab(&mut self, tab_id: u32) -> (u32, Option<Box<dyn Tab>>) {
        // Can't close the character list (tab with ID 0).
        if tab_id == 0 {
            return (0, None);
        }

        let Some(tab_index) = self.find_tab_by_id(tab_id) else {
            return (self.active_tab_id, None);
        };

        let removed = self.tabs.remove(tab_index);

        // Adjust positions for all tabs that were after the removed one
        for tab_info in &mut self.tabs {
            if tab_info.position > removed.position {
                tab_info.position -= 1;
            }
        }

        // If we closed the active tab, switch to the leftmost tab (position 0)
        if self.active_tab_id == tab_id {
            // Find the tab with position 0 (the leftmost tab)
            let leftmost_tab = self
                .tabs
                .iter()
                .find(|tab_info| tab_info.position == 0)
                .unwrap_or(&self.tabs[0]); // Fallback to first tab if no position 0 found

            self.active_tab_id = leftmost_tab.id;
        }

        (self.active_tab_id, Some(removed.tab))
    }

    pub(crate) fn switch_to_list(&mut self) {
        // The character list is always the tab with ID 0
        self.active_tab_id = 0;
    }

    pub(crate) fn switch_to(&mut self, id: u32) -> Result<(), Error> {
        if self.find_tab_by_id(id).is_none() {
            return Err(ErrorKind::InvalidTabId(id).into());
        }
        self.active_tab_id = id;
        Ok(())
    }

    pub(crate) fn switch_to_session(&mut self, session_id: u32) -> Result<(), Error> {
        match self
            .tabs
            .iter()
            .find(|t| t.tab.session_id() == Some(session_id))
        {
            Some(tab_info) => {
                self.active_tab_id = tab_info.id;
                Ok(())
            }
            None => Err(ErrorKind::NoSuchSession(session_id).into()),
        }
    }

    pub(crate) fn move_tab_left(&mut self, tab_id: u32) -> Result<(), Error> {
        let tab_index = self
            .find_tab_by_id(tab_id)
            .ok_or(ErrorKind::InvalidTabId(tab_id))?;
        let tab_info = &self.tabs[tab_index];

        if tab_info.position == 0 {
            return Ok(()); // Already at leftmost position
        }

        let tab_info_position = tab_info.position;
        // Find the tab that's to the left of this one
        if let Some(left_tab) = self
            .tabs
            .iter_mut()
            .find(|t| t.position == tab_info_position - 1)
        {
            left_tab.position += 1;
        }

        // Update this tab's position
        self.tabs[tab_index].position -= 1;

        Ok(())
    }

    pub(crate) fn move_tab_right(&mut self, tab_id: u32) -> Result<(), Error> {
        let max_position = self.tabs.len() - 1;

        let tab_index = self
            .find_tab_by_id(tab_id)
            .ok_or(ErrorKind::InvalidTabId(tab_id))?;
        let tab_info = &self.tabs[tab_index];

        if tab_info.position == max_position {
            return Ok(()); // Already at rightmost position
        }

        // Find the tab that's to the right of this one
        let tab_info_position = tab_info.position;
        if let Some(right_tab) = self
            .tabs
            .iter_mut()
            .find(|t| t.position == tab_info_position + 1)
        {
            right_tab.position -= 1;
        }

        // Update this tab's position
        self.tabs[tab_index].position += 1;

        Ok(())
    }

    pub(crate) fn find_tab_by_id(&self, id: u32) -> Option<usize> {
        self.tabs.iter().position(|tab_info| tab_info.id == id)
    }
}

#[derive(Debug)]
pub(crate) struct TabInfo {
    pub(super) id: u32,
    tab: Box<dyn Tab>,
    position: usize,
}

pub(crate) trait Tab: Debug + Send + Sync {
    fn title(&self, app: &AppData) -> String;

    fn rendered_title(&self, app: &AppData) -> Line {
        Line::from(self.title(app))
    }

    fn set_title(&mut self, _: &AppData, _: &str) -> Result<(), Error> {
        Err(ErrorKind::Internal("unsupported".to_string()).into())
    }

    fn render(
        &mut self,
        app: &mut AppData,
        f: &mut Frame<'_>,
        tab_content: Rect,
    ) -> Result<(), Error>;

    fn session_id(&self) -> Option<u32> {
        None
    }

    fn config_reloaded(&mut self, _config: &Config) -> Result<(), Error> {
        Ok(())
    }

    fn layout(&self) -> Py<Section>;

    fn crossterm_event(
        &mut self,
        _app: &mut AppData,
        _event: &crossterm::event::Event,
    ) -> Result<Option<TabAction>, Error> {
        Ok(None)
    }
}
