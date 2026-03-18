use ratatui::{buffer::Buffer, layout::{Constraint, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Clear, Paragraph, Widget}};
use rim_application::state::{NotificationLevel, NotificationPreviewState, RimState};

#[derive(Clone)]
pub(super) struct NotificationPreviewWidget {
	state: NotificationPreviewState,
	area:  Rect,
}

impl NotificationPreviewWidget {
	pub(super) fn from_state(state: &RimState, content_area: Rect) -> Option<Self> {
		let preview = state.notification_preview()?;
		let width = content_area.width.clamp(44, 72);
		let height = 2 + 1 + 10;
		let x = content_area.x.saturating_add(content_area.width.saturating_sub(width));
		let y = content_area.y;
		Some(Self { state: preview, area: Rect { x, y, width, height } })
	}
}

impl Widget for NotificationPreviewWidget {
	fn render(self, _area: Rect, buf: &mut Buffer) {
		Clear.render(self.area, buf);
		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(Color::Cyan))
			.title(Line::from(vec![Span::styled(" Notifications ", Style::default().fg(Color::Cyan))]));
		block.render(self.area, buf);
		let inner = self.area.inner(ratatui::layout::Margin { horizontal: 1, vertical: 1 });
		if inner.width == 0 || inner.height == 0 {
			return;
		}

		let [header_area, body_area] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
		let unread_text = format!("unread {}", self.state.unread_total);
		let unread_width = unread_text.chars().count() as u16;
		let unread_area = Rect {
			x:      header_area.x.saturating_add(header_area.width.saturating_sub(unread_width)),
			y:      header_area.y,
			width:  unread_width.min(header_area.width),
			height: 1,
		};
		Paragraph::new(Line::from(Span::styled(
			unread_text,
			Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
		)))
		.render(unread_area, buf);
		let header_left_width = header_area.width.saturating_sub(unread_area.width.saturating_add(1));
		let header_hint = format!("queue preview | open with {}", self.state.open_center_hint);
		let header_area_left = Rect {
			x:      header_area.x,
			y:      header_area.y,
			width:  header_left_width,
			height: header_area.height,
		};
		Paragraph::new(Line::from(Span::styled(
			truncate_with_ellipsis(header_hint.as_str(), header_left_width as usize),
			Style::default().fg(Color::DarkGray),
		)))
		.render(header_area_left, buf);

		let mut rows = Vec::new();
		for index in 0..5usize {
			if let Some(item) = self.state.items.get(index) {
				let level_style = match item.level {
					NotificationLevel::Info => Style::default().fg(Color::Cyan),
					NotificationLevel::Warn => Style::default().fg(Color::Yellow),
					NotificationLevel::Error => Style::default().fg(Color::Red),
				};
				rows.push(Line::from(vec![
					Span::styled(format!("[{}]", item.level.label()), level_style),
					Span::raw(" "),
					Span::styled(item.created_at_local.clone(), Style::default().fg(Color::DarkGray)),
				]));
				rows.push(Line::from(Span::raw(truncate_with_ellipsis(
					item.message.as_str(),
					body_area.width as usize,
				))));
			} else {
				rows.push(Line::raw(""));
				rows.push(Line::raw(""));
			}
		}
		Paragraph::new(rows).render(body_area, buf);
	}
}

fn truncate_with_ellipsis(text: &str, width: usize) -> String {
	if width == 0 {
		return String::new();
	}
	let mut result = String::new();
	for ch in text.chars() {
		if result.chars().count() + 1 >= width {
			result.push('…');
			return result;
		}
		result.push(ch);
	}
	result
}
