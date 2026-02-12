mod status_bar;
mod terminal_session;
mod top_bar;
mod window_area;

use ratatui::layout::{Constraint, Layout, Rect};

use crate::state::AppState;
use status_bar::StatusBarWidget;
pub(crate) use terminal_session::TerminalSession;
use top_bar::TopBarWidget;
use window_area::WindowAreaWidget;

pub struct Renderer {
    last_content_area: Option<Rect>,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            last_content_area: None,
        }
    }

    pub fn render(&mut self, frame: &mut ratatui::Frame<'_>, state: &mut AppState) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        if self
            .last_content_area
            .map(|last| last.width != chunks[1].width || last.height != chunks[1].height)
            .unwrap_or(true)
        {
            state.update_active_tab_layout(chunks[1].width, chunks[1].height);
            self.last_content_area = Some(chunks[1]);
        }

        let top_bar = TopBarWidget::from_state(state);
        let (window_area, cursor_position) = WindowAreaWidget::from_state(state, chunks[1]);
        let status_bar = StatusBarWidget::from_state(state);

        frame.render_widget(top_bar, chunks[0]);
        frame.render_widget(window_area, chunks[1]);
        frame.render_widget(status_bar, chunks[2]);
        if let Some(cursor_to_draw) = cursor_position {
            frame.set_cursor_position(cursor_to_draw);
        }
    }

    pub fn mark_layout_dirty(&mut self) {
        self.last_content_area = None;
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
