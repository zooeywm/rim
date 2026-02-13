use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::state::AppState;

pub(super) struct TopBarWidget {
    buffer_spans: Vec<Span<'static>>,
    tab_spans: Vec<Span<'static>>,
    tabs_width: u16,
    show_tabs: bool,
}

impl TopBarWidget {
    pub(super) fn from_state(state: &AppState) -> Self {
        let active_buffer_id = state.active_buffer_id();
        let active_tab_id = state.active_tab;

        let buffer_ids = state
            .buffer_order
            .iter()
            .copied()
            .filter(|id| state.buffers.get(*id).is_some())
            .collect::<Vec<_>>();

        let mut buffer_spans = Vec::new();
        for (idx, id) in buffer_ids.iter().enumerate() {
            let is_active = active_buffer_id == Some(*id);
            let style = if is_active {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let Some(buffer) = state.buffers.get(*id) else {
                continue;
            };
            buffer_spans.push(Span::styled(" ", style));
            buffer_spans.push(Span::styled(buffer.name.clone(), style));
            buffer_spans.push(Span::styled(" ", style));
            if idx + 1 != buffer_ids.len() {
                buffer_spans.push(Span::raw(" "));
            }
        }

        let mut tab_items = state.tabs.keys().copied().collect::<Vec<_>>();
        tab_items.sort_by_key(|id| id.0);
        let show_tabs = tab_items.len() > 1;
        let mut tabs_width: u16 = 0;
        let mut tab_spans = Vec::new();
        if show_tabs {
            for (idx, tab_id) in tab_items.iter().enumerate() {
                let is_active = *tab_id == active_tab_id;
                let style = if is_active {
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let label = format!(" {} ", tab_id.0);
                tabs_width = tabs_width.saturating_add(label.len() as u16);
                tab_spans.push(Span::styled(label, style));
                if idx + 1 != tab_items.len() {
                    tabs_width = tabs_width.saturating_add(1);
                    tab_spans.push(Span::raw(" "));
                }
            }
        }

        Self {
            buffer_spans,
            tab_spans,
            tabs_width,
            show_tabs,
        }
    }
}

impl Widget for TopBarWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.show_tabs {
            let top_chunks = Layout::horizontal([
                Constraint::Min(1),
                Constraint::Length(self.tabs_width.min(area.width)),
            ])
            .split(area);
            Paragraph::new(Line::from(self.buffer_spans)).render(top_chunks[0], buf);
            Paragraph::new(Line::from(self.tab_spans)).render(top_chunks[1], buf);
        } else {
            Paragraph::new(Line::from(self.buffer_spans)).render(area, buf);
        }
    }
}
