use std::fmt::Debug;

use ratatui::Frame;
use ratatui::layout::Constraint::{Fill, Length};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::AppData;
use crate::config::{CRATE_NAME, Config};
use crate::error::Error;
use crate::tui::{Mudlist, TabAction};

#[derive(Debug)]
pub(crate) struct Chrome {
    active_tab: usize,
    tabs: Vec<Box<dyn Tab>>,
}

impl Chrome {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            active_tab: 0,
            tabs: vec![Box::new(Mudlist::new(config))],
        }
    }

    // TODO(XXX): Styling.
    pub(crate) fn render(&mut self, app: &mut AppData, f: &mut Frame) -> Result<(), Error> {
        let [tab_bar, tab_content] = Layout::vertical([Length(3), Fill(0)]).areas(f.area());

        f.render_widget(
            Tabs::new(self.tabs.iter().map(|t| t.title()))
                .select(self.active_tab)
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
        for tab in &mut self.tabs {
            tab.config_reloaded(config)?;
        }
        Ok(())
    }

    pub(crate) fn active_tab(&mut self) -> &mut dyn Tab {
        self.tabs[self.active_tab].as_mut()
    }

    pub(crate) fn new_tab(&mut self, tab: impl Tab + 'static) -> usize {
        self.tabs.push(Box::new(tab));
        self.active_tab = self.tabs.len() - 1;
        self.active_tab
    }

    pub(crate) fn next_tab(&mut self) -> usize {
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.active_tab
    }

    pub(crate) fn previous_tab(&mut self) -> usize {
        self.active_tab = self.active_tab.saturating_sub(1) % self.tabs.len();
        self.active_tab
    }

    pub(crate) fn close_active_tab(&mut self) -> (usize, Option<Box<dyn Tab>>) {
        if self.active_tab == 0 {
            return (0, None);
        }

        let removed = self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }

        (self.active_tab, Some(removed))
    }
}

pub(crate) trait Tab: Debug + Send + Sync {
    fn title(&self) -> Line;

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

    fn crossterm_event(
        &mut self,
        _app: &mut AppData,
        _event: &crossterm::event::Event,
    ) -> Result<Option<TabAction>, Error> {
        Ok(None)
    }
}
