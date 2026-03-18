use ratatui::{buffer::Buffer, layout::Rect, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Paragraph, Widget}};
use rim_application::state::{RimState, StatusBarMode};

pub(super) struct StatusBarWidget {
	mode:        StatusBarMode,
	status_line: String,
}

impl StatusBarWidget {
	pub(super) fn from_state(state: &RimState) -> Self {
		Self { mode: state.status_bar.mode, status_line: state.status_line() }
	}
}

impl Widget for StatusBarWidget {
	fn render(self, area: Rect, buf: &mut Buffer) {
		Paragraph::new(Line::from(vec![
			Span::styled(
				format!(" {} ", self.mode),
				Style::default().fg(Color::White).bg(Color::Blue).add_modifier(Modifier::BOLD),
			),
			Span::raw(format!(" {}", self.status_line)),
		]))
		.render(area, buf);
	}
}
