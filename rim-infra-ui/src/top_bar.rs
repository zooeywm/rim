use ratatui::{buffer::Buffer, layout::{Constraint, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Paragraph, Widget}};
use rim_kernel::state::RimState;

pub(super) struct TopBarWidget {
	buffer_spans: Vec<Span<'static>>,
	tab_spans:    Vec<Span<'static>>,
	tabs_width:   u16,
	show_tabs:    bool,
}

impl TopBarWidget {
	pub(super) fn from_state(state: &RimState) -> Self {
		let active_buffer_id = state.active_buffer_id();
		let active_tab_id = state.active_tab;

		let buffer_ids = state.active_tab_buffer_ids();

		let mut buffer_spans = Vec::new();
		for (idx, id) in buffer_ids.iter().enumerate() {
			let is_active = active_buffer_id == Some(*id);
			let Some(buffer) = state.buffers.get(*id) else {
				continue;
			};
			let deleted_on_disk = buffer.path.as_ref().is_some_and(|path| !path.exists());
			let mut style = if is_active {
				Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::Gray)
			};
			if deleted_on_disk {
				style = style.add_modifier(Modifier::CROSSED_OUT);
			}
			let mut label = buffer.name.clone();
			if buffer.dirty {
				label.push('*');
			}
			buffer_spans.push(Span::styled(" ", style));
			buffer_spans.push(Span::styled(label, style));
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
					Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD)
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

		Self { buffer_spans, tab_spans, tabs_width, show_tabs }
	}
}

impl Widget for TopBarWidget {
	fn render(self, area: Rect, buf: &mut Buffer) {
		if self.show_tabs {
			let top_chunks =
				Layout::horizontal([Constraint::Min(1), Constraint::Length(self.tabs_width.min(area.width))])
					.split(area);
			Paragraph::new(Line::from(self.buffer_spans)).render(top_chunks[0], buf);
			Paragraph::new(Line::from(self.tab_spans)).render(top_chunks[1], buf);
		} else {
			Paragraph::new(Line::from(self.buffer_spans)).render(area, buf);
		}
	}
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use ratatui::style::Modifier;
	use rim_kernel::state::RimState;

	use super::TopBarWidget;

	#[test]
	fn dirty_buffer_should_show_star_in_top_bar_label() {
		let mut state = RimState::new();
		let clean = state.create_buffer(Some(PathBuf::from("clean.rs")), "");
		let dirty = state.create_buffer(Some(PathBuf::from("dirty.rs")), "");
		state.bind_buffer_to_active_window(clean);
		state.bind_buffer_to_active_window(dirty);
		state.bind_buffer_to_active_window(clean);
		state.set_buffer_dirty(dirty, true);

		let widget = TopBarWidget::from_state(&state);
		let labels = widget
			.buffer_spans
			.iter()
			.filter_map(|span| match span.content.as_ref() {
				" " => None,
				content => Some(content.to_string()),
			})
			.collect::<Vec<_>>();

		assert!(labels.iter().any(|label| label == "clean.rs"));
		assert!(labels.iter().any(|label| label == "dirty.rs*"));
	}

	#[test]
	fn top_bar_should_only_show_buffers_from_active_tab() {
		let mut state = RimState::new();
		let first = state.create_buffer(Some(PathBuf::from("first.rs")), "");
		state.bind_buffer_to_active_window(first);
		let second = state.create_buffer(Some(PathBuf::from("second.rs")), "");
		state.bind_buffer_to_active_window(second);
		let second_tab = state.open_new_tab();
		state.switch_tab(second_tab);

		let widget = TopBarWidget::from_state(&state);
		let labels = widget
			.buffer_spans
			.iter()
			.filter_map(|span| match span.content.as_ref() {
				" " => None,
				content => Some(content.to_string()),
			})
			.collect::<Vec<_>>();

		assert_eq!(labels, vec!["untitled".to_string()]);
	}

	#[test]
	fn deleted_file_should_show_crossed_out_label_in_top_bar() {
		let mut state = RimState::new();
		let nanos = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|duration| duration.as_nanos())
			.unwrap_or(0);
		let file_path = std::env::temp_dir().join(format!("rim-topbar-deleted-{}.rs", nanos));
		std::fs::write(file_path.as_path(), "fn main() {}").expect("temp file should be created");

		let buffer_id = state.create_buffer(Some(file_path.clone()), "fn main() {}");
		state.bind_buffer_to_active_window(buffer_id);
		std::fs::remove_file(file_path.as_path()).expect("temp file should be removed");

		let widget = TopBarWidget::from_state(&state);
		let deleted_span = widget
			.buffer_spans
			.iter()
			.find(|span| span.content.as_ref().starts_with("rim-topbar-deleted-"))
			.expect("deleted buffer label should be present");
		assert!(deleted_span.style.add_modifier.contains(Modifier::CROSSED_OUT));
	}
}
