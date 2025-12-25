use std::collections::HashMap;
use std::fmt::Debug;

use pyo3::types::PyAnyMethods;
use pyo3::{Py, PyRef, Python};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};
use tracing::error;

use crate::app::{AppData, TabAction};
use crate::config::{CRATE_NAME, Config};
use crate::dialog::{ConfirmAction, DialogKind, Severity};
use crate::error::{Error, ErrorKind};
use crate::keyboard::KeyEvent;
use crate::shortcut::Shortcut;
use crate::tui::Section;
use crate::{python, tui};

#[derive(Debug)]
pub(crate) struct Chrome {
    active_tab_id: u32,
    next_tab_id: u32,
    tabs: Vec<TabInfo>,
    // TODO(XXX): Py<Layout>!
}

impl Chrome {
    pub(crate) fn new(config: &Py<Config>) -> Self {
        Self {
            active_tab_id: 0, // ID 0 is the character menu
            next_tab_id: 1,
            tabs: vec![TabInfo {
                id: 0,
                tab: tui::CharacterMenu::new_tab(config),
                position: 0,
            }],
        }
    }

    // TODO(XXX): Styling.
    pub(crate) fn render(&mut self, app: &mut AppData, f: &mut Frame) -> Result<(), Error> {
        // TODO(XXX): Py<Layout>!
        let [tab_bar, tab_content] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(0)]).areas(f.area());

        // Sort tabs by position for rendering
        let sorted_tabs = self.tabs();

        // Find the index of the active tab in the sorted order for selection
        let active_idx = sorted_tabs
            .iter()
            .position(|tab_info| tab_info.id == self.active_tab_id)
            .unwrap_or(0);

        f.render_widget(
            Tabs::new(sorted_tabs.iter().map(|t| t.tab.render_title(app)))
                .select(active_idx)
                .highlight_style(Style::default().fg(Color::Black).bg(Color::LightMagenta))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(CRATE_NAME.to_uppercase()),
                ),
            tab_bar,
        );

        self.active_tab_mut().render(app, f, tab_content)?;

        // Render dialog manager dialogs (if any)
        Python::attach(|py| {
            if let Some(dialog) = app.dialog_manager.borrow(py).get_active() {
                Self::render_dialog(f, dialog, tab_content);
            }
        });

        Ok(())
    }

    fn render_dialog(f: &mut Frame, dialog: &crate::dialog::Dialog, area: Rect) {
        match &dialog.kind {
            DialogKind::Confirmation {
                message,
                confirm_key,
                ..
            } => {
                let popup_area = centered_rect(area, 50, 25);
                let [msg_area, help_area] =
                    Layout::vertical([Constraint::Max(2), Constraint::Max(2)]).areas(popup_area);

                f.render_widget(Clear, msg_area);
                f.render_widget(Clear, help_area);

                f.render_widget(
                    Paragraph::new::<Text>(message.clone().into()).block(
                        Block::default()
                            .borders(Borders::LEFT | Borders::RIGHT | Borders::TOP)
                            .border_style(Color::Yellow)
                            .title("Confirm"),
                    ),
                    msg_area,
                );

                let help = format!("Press '{confirm_key}' to confirm or any other key to cancel");
                let help_paragraph = Paragraph::new::<Text>(help.into()).block(
                    Block::default()
                        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                        .border_style(Color::Yellow),
                );
                f.render_widget(help_paragraph, help_area);
            }
            DialogKind::Notification {
                message, severity, ..
            } => {
                // Use much more space for dialogs (80% width, 60% height)
                let (width, height) = match severity {
                    Severity::Error => (80, 60),
                    Severity::Warning => (70, 50),
                    Severity::Info => (60, 40),
                };
                let popup_area = centered_rect(area, width, height);

                f.render_widget(Clear, popup_area);

                let (title, border_color) = match severity {
                    Severity::Error => ("Error", Color::Red),
                    Severity::Warning => ("Warning", Color::Yellow),
                    Severity::Info => ("Info", Color::Blue),
                };

                f.render_widget(
                    Paragraph::new::<Text>(message.clone().into())
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(border_color)
                                .title(title),
                        )
                        .wrap(Wrap { trim: false }),
                    popup_area,
                );
            }
        }
    }

    pub(crate) fn active_tab(&self) -> &Tab {
        // This unwrap is safe because we manage active_tab_id internally
        // and always ensure it corresponds to an existing tab
        let index = self.find_tab_by_id(self.active_tab_id).unwrap();
        &self.tabs[index].tab
    }

    pub(crate) fn active_tab_mut(&mut self) -> &mut Tab {
        // This unwrap is safe because we manage active_tab_id internally
        // and always ensure it corresponds to an existing tab
        let index = self.find_tab_by_id(self.active_tab_id).unwrap();
        &mut self.tabs[index].tab
    }

    pub(crate) fn active_tab_id(&self) -> u32 {
        self.active_tab_id
    }

    pub(crate) fn get_tab(&self, id: u32) -> Result<&Tab, Error> {
        let index = self.find_tab_by_id(id).ok_or(ErrorKind::InvalidTabId(id))?;
        Ok(&self.tabs[index].tab)
    }

    pub(crate) fn get_tab_mut(&mut self, id: u32) -> Result<&mut Tab, Error> {
        let index = self.find_tab_by_id(id).ok_or(ErrorKind::InvalidTabId(id))?;
        Ok(&mut self.tabs[index].tab)
    }

    pub(crate) fn tabs(&self) -> Vec<&TabInfo> {
        let mut tabs: Vec<_> = self.tabs.iter().collect();
        tabs.sort_by_key(|tab_info| tab_info.position);
        tabs
    }

    pub(crate) fn tab_for_session(&self, session_id: u32) -> Option<&TabInfo> {
        self.tabs.iter().find(|tab_info| match &tab_info.tab.kind {
            TabKind::Session(session) => session.sesh.id == session_id,
            _ => false,
        })
    }

    pub(crate) fn new_tab(&mut self, tab: Tab) -> u32 {
        let position = self.tabs.len();
        let id = self.next_tab_id;
        self.next_tab_id += 1;

        self.tabs.push(TabInfo { id, tab, position });

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

    pub(crate) fn close_tab(&mut self, tab_id: u32) -> (u32, Option<Tab>) {
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
        match self.tabs.iter().find(|t| match &t.tab.kind {
            TabKind::Session(char) => char.sesh.id == session_id,
            _ => false,
        }) {
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
    tab: Tab,
    position: usize,
}

#[derive(Debug)]
pub(crate) struct TabData {
    pub(crate) title: String,
    layout: Py<Section>,
    shortcuts: HashMap<KeyEvent, Shortcut>,
}

impl TabData {
    pub(crate) fn new(
        title: String,
        layout: Py<Section>,
        shortcuts: Option<HashMap<KeyEvent, Shortcut>>,
    ) -> Self {
        Self {
            title,
            layout,
            shortcuts: shortcuts.unwrap_or_default(),
        }
    }

    pub(crate) fn layout(&self) -> Py<Section> {
        Python::attach(|py| self.layout.clone_ref(py))
    }
}

#[derive(Debug)]
pub(crate) enum TabKind {
    Menu(Box<tui::CharacterMenu>),
    Session(Box<tui::Character>),
    Custom(Box<tui::CustomTab>),
}

#[derive(Debug)]
pub(crate) struct Tab {
    pub(crate) data: TabData,
    pub(crate) kind: TabKind,
}

impl Tab {
    pub(crate) fn render_title(&self, app: &AppData) -> Line<'_> {
        if let TabKind::Session(character) = &self.kind {
            character.render_title(app, &self.data)
        } else {
            Line::from(self.data.title.clone())
        }
    }

    pub(crate) fn lookup_shortcut(&self, key_event: &KeyEvent) -> Option<Shortcut> {
        // Note: using GIL here because a Python implemented shortcut can't be cloned
        // on the heap without holding the GIL.
        Python::attach(|_| self.data.shortcuts.get(key_event).cloned())
    }

    pub(crate) fn render(
        &mut self,
        app: &mut AppData,
        f: &mut Frame<'_>,
        tab_content: Rect,
    ) -> Result<(), Error> {
        let sections = Python::attach(|py| {
            let layout: PyRef<'_, Section> =
                self.data.layout.extract(py).map_err(ErrorKind::from)?;
            layout.partition_by_name(py, tab_content)
        })?;
        match &mut self.kind {
            TabKind::Menu(char_menu) => {
                char_menu.render(app, f, &sections);
                Ok(())
            }
            TabKind::Session(session) => session.render(app, f, &sections),
            TabKind::Custom(custom) => custom.render(app, f, &sections),
        }
    }

    pub(crate) fn key_event(
        &mut self,
        app: &mut AppData,
        key_event: &KeyEvent,
    ) -> Result<Option<TabAction>, Error> {
        // Check if a dialog should handle this key event
        if Self::dialog_key_action(app, key_event) {
            return Ok(None);
        }

        match &mut self.kind {
            TabKind::Menu(_) | TabKind::Custom(_) => Ok(None),
            TabKind::Session(session) => session.key_event(app, key_event),
        }
    }

    pub(crate) async fn shortcut(
        &mut self,
        app: &mut AppData,
        shortcut: &Shortcut,
    ) -> Result<Option<TabAction>, Error> {
        match &mut self.kind {
            TabKind::Menu(char_menu) => char_menu.shortcut(app, shortcut),
            TabKind::Session(session) => session.shortcut(app, shortcut).await,
            TabKind::Custom(_) => Ok(None),
        }
    }

    pub(crate) fn session(&self) -> Option<python::Session> {
        if let TabKind::Session(char) = &self.kind {
            Some(char.sesh.clone())
        } else {
            None
        }
    }

    pub(crate) fn set_shortcut(&mut self, key_event: KeyEvent, shortcut: Option<Shortcut>) {
        match shortcut {
            None => self.data.shortcuts.remove(&key_event),
            Some(shortcut) => self.data.shortcuts.insert(key_event, shortcut),
        };
    }

    pub(crate) fn all_shortcuts(&self) -> HashMap<KeyEvent, String> {
        self.data
            .shortcuts
            .iter()
            .map(|(key_event, shortcut)| (*key_event, shortcut.to_string()))
            .collect()
    }

    fn dialog_key_action(app: &mut AppData, key_event: &KeyEvent) -> bool {
        let (consumed, action) =
            Python::attach(|py| app.dialog_manager.borrow_mut(py).handle_key(key_event));
        let Some(action) = action else {
            return consumed;
        };
        match action {
            ConfirmAction::Quit {} => {
                app.should_quit = true;
            }
            ConfirmAction::PyCallback(callback) => {
                // Spawn the Python callback
                let dm = Python::attach(|py| app.dialog_manager.clone_ref(py));
                tokio::spawn(async move {
                    let future_result = Python::attach(|py| {
                        pyo3_async_runtimes::tokio::into_future(callback.bind(py).call0()?)
                    });

                    let future = match future_result {
                        Ok(f) => f,
                        Err(err) => {
                            // Note: Error::from on PyErr to collect backtrace.
                            let err = Error::from(err);
                            error!("dialog callback error: {err}");
                            Python::attach(|py| {
                                dm.borrow_mut(py)
                                    .show_error(format!("Dialog callback failed: {err}"));
                            });
                            return;
                        }
                    };

                    if let Err(err) = future.await {
                        // Note: Error::from on PyErr to collect backtrace.
                        let err = Error::from(err);
                        error!("dialog callback error: {err}");
                        Python::attach(|py| {
                            dm.borrow_mut(py)
                                .show_error(format!("Dialog callback failed: {err}"));
                        });
                    }
                });
            }
        }
        consumed
    }
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
