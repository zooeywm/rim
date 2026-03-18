use ratatui::{buffer::Buffer, layout::{Constraint, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap}};
use rim_application::state::{NotificationCenterItem, NotificationCenterView, NotificationLevel, RimState};

pub(super) struct NotificationCenterWidget {
	view: NotificationCenterView,
	area: Rect,
}

impl NotificationCenterWidget {
	pub(super) fn from_state(state: &RimState, content_area: Rect) -> Option<Self> {
		let view = state.notification_center_view()?;
		let width = content_area.width.saturating_sub(4).clamp(72, 140).min(content_area.width);
		let height = content_area.height.saturating_sub(2).clamp(18, 40).min(content_area.height);
		let x = content_area.x.saturating_add(content_area.width.saturating_sub(width) / 2);
		let y = content_area.y.saturating_add(content_area.height.saturating_sub(height) / 2);
		Some(Self { view, area: Rect { x, y, width, height } })
	}
}

impl Widget for NotificationCenterWidget {
	fn render(self, _area: Rect, buf: &mut Buffer) {
		Clear.render(self.area, buf);
		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(Color::Cyan))
			.title(Line::from(vec![Span::styled(" Notifications ", Style::default().fg(Color::Cyan))]))
			.title_bottom(Line::from(vec![
				Span::raw("<F1> keymap hints  "),
				Span::styled(
					format!("unread {}", self.view.unread_total),
					Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
				),
			]));
		block.render(self.area, buf);
		let inner = self.area.inner(ratatui::layout::Margin { horizontal: 1, vertical: 1 });
		if inner.width < 8 || inner.height < 4 {
			return;
		}

		let [list_area, divider_area, detail_area] =
			Layout::horizontal([Constraint::Percentage(45), Constraint::Length(1), Constraint::Percentage(55)])
				.areas(inner);
		for y in divider_area.y..divider_area.y.saturating_add(divider_area.height) {
			buf[(divider_area.x, y)].set_symbol("│").set_fg(Color::Cyan);
		}

		let list_lines = self
			.view
			.items
			.iter()
			.enumerate()
			.map(|(index, item)| render_list_item(item, index == self.view.selected, list_area.width as usize))
			.collect::<Vec<_>>();
		Paragraph::new(list_lines).render(list_area, buf);

		let detail_item = self.view.items.get(self.view.selected).expect("selected item exists");
		let detail_lines = vec![
			Line::from(vec![
				Span::styled("Type: ", Style::default().fg(Color::DarkGray)),
				Span::styled(detail_item.level.label(), level_style(detail_item.level)),
			]),
			Line::from(vec![
				Span::styled("Time: ", Style::default().fg(Color::DarkGray)),
				Span::raw(detail_item.created_at_local.clone()),
			]),
			Line::from(vec![
				Span::styled("Read: ", Style::default().fg(Color::DarkGray)),
				Span::raw(if detail_item.read { "yes" } else { "no" }),
			]),
			Line::raw(""),
			Line::from(vec![Span::styled("Message", Style::default().add_modifier(Modifier::BOLD))]),
			Line::raw(detail_item.message.clone()),
		];
		Paragraph::new(detail_lines).wrap(Wrap { trim: false }).render(detail_area, buf);
	}
}

fn render_list_item(item: &NotificationCenterItem, selected: bool, width: usize) -> Line<'static> {
	let mut prefix = if item.read { " " } else { "*" }.to_string();
	prefix.push(' ');
	let line = format!("{}{} [{}] {}", prefix, item.created_at_local, item.level.label(), item.message);
	let truncated = truncate_with_ellipsis(line.as_str(), width);
	if selected {
		Line::styled(truncated, Style::default().bg(Color::Blue).fg(Color::White))
	} else {
		Line::styled(truncated, level_style(item.level))
	}
}

fn level_style(level: NotificationLevel) -> Style {
	match level {
		NotificationLevel::Info => Style::default().fg(Color::Cyan),
		NotificationLevel::Warn => Style::default().fg(Color::Yellow),
		NotificationLevel::Error => Style::default().fg(Color::Red),
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
