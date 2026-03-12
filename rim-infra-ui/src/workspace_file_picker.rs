use std::collections::HashSet;

use ratatui::{buffer::Buffer, layout::{Constraint, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap}};
use rim_kernel::{
	preview::preview_rows,
	state::{RimState, WorkspaceFileMatch, WorkspaceFilePickerState, compute_workspace_file_picker_body_layout},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(super) struct WorkspaceFilePickerWidget {
	picker:   WorkspaceFilePickerState,
	area:     Rect,
	cursor_x: u16,
	cursor_y: u16,
	word_wrap: bool,
}

impl WorkspaceFilePickerWidget {
	pub(super) fn from_state(state: &RimState, content_area: Rect) -> Option<Self> {
		if content_area.width == 0 || content_area.height == 0 {
			return None;
		}
		let picker = state.workspace_file_picker()?.clone();
		let width = content_area.width.saturating_sub(4).clamp(56, 140).min(content_area.width);
		let height = content_area.height.saturating_sub(1).max(14).min(content_area.height);
		let x = content_area.x.saturating_add(content_area.width.saturating_sub(width) / 2);
		let y = content_area.y.saturating_add(content_area.height.saturating_sub(height) / 2);
		let area = Rect { x, y, width, height };
		let input_prefix = "> ";
		let input_width = area.width.saturating_sub(6) as usize;
		let query_line = tail_fit(input_prefix, picker.query.as_str(), input_width);
		let cursor_x = area
			.x
			.saturating_add(2)
			.saturating_add(UnicodeWidthStr::width(query_line.as_str()) as u16)
			.min(area.x.saturating_add(area.width.saturating_sub(1)));
		let cursor_y = area.y.saturating_add(1).min(area.y.saturating_add(area.height.saturating_sub(1)));
		Some(Self {
			picker,
			area,
			cursor_x,
			cursor_y,
			word_wrap: state.picker_preview_word_wrap_enabled(),
		})
	}

	pub(super) fn cursor_position(&self) -> (u16, u16) { (self.cursor_x, self.cursor_y) }
}

impl Widget for WorkspaceFilePickerWidget {
	fn render(self, _area: Rect, buf: &mut Buffer) {
		Clear.render(self.area, buf);
		let block =
			Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)).title(" Files ");
		let inner = block.inner(self.area);
		block.render(self.area, buf);

		let [input_area, body_area] = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
		render_query_row(&self.picker, input_area, buf);
		render_body(&self.picker, body_area, buf, self.word_wrap);
	}
}

fn render_query_row(picker: &WorkspaceFilePickerState, area: Rect, buf: &mut Buffer) {
	let query = tail_fit("> ", picker.query.as_str(), area.width.saturating_sub(2) as usize);
	Paragraph::new(Line::from(vec![
		Span::styled("> ", Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)),
		Span::styled(
			query.strip_prefix("> ").unwrap_or(query.as_str()).to_string(),
			Style::default().fg(Color::White),
		),
	]))
	.render(area, buf);

	let counter = format!("{}/{}", picker.total_matches, picker.total_files);
	let counter_width = UnicodeWidthStr::width(counter.as_str()) as u16;
	if counter_width < area.width {
		let counter_area = Rect {
			x:      area.x.saturating_add(area.width.saturating_sub(counter_width)),
			y:      area.y,
			width:  counter_width,
			height: 1,
		};
		Paragraph::new(counter).render(counter_area, buf);
	}
}

fn render_body(picker: &WorkspaceFilePickerState, area: Rect, buf: &mut Buffer, word_wrap: bool) {
	if area.width == 0 || area.height == 0 {
		return;
	}
	draw_horizontal_separator(area, buf);
	let content_area = Rect {
		x:      area.x,
		y:      area.y.saturating_add(1),
		width:  area.width,
		height: area.height.saturating_sub(1),
	};
	if content_area.width == 0 || content_area.height == 0 {
		return;
	}

	let layout = compute_workspace_file_picker_body_layout(content_area.width);
	if layout.horizontal_split {
		let list_area = Rect {
			x:      content_area.x,
			y:      content_area.y,
			width:  layout.list_width,
			height: content_area.height,
		};
		let divider_area = Rect {
			x:      list_area.x.saturating_add(list_area.width),
			y:      content_area.y,
			width:  layout.divider_width,
			height: content_area.height,
		};
		let preview_area = Rect {
			x:      divider_area.x.saturating_add(divider_area.width),
			y:      content_area.y,
			width:  layout.preview_width,
			height: content_area.height,
		};
		draw_vertical_separator(divider_area, buf);
		render_result_list(picker, list_area, buf);
		render_preview(picker, preview_area, buf, word_wrap);
		return;
	}
	let [list_area, divider_area, preview_area] =
		Layout::vertical([Constraint::Percentage(52), Constraint::Length(1), Constraint::Min(1)])
			.areas(content_area);
	draw_horizontal_separator_between(divider_area, buf);
	render_result_list(picker, list_area, buf);
	render_preview(picker, preview_area, buf, word_wrap);
}

fn render_result_list(picker: &WorkspaceFilePickerState, area: Rect, buf: &mut Buffer) {
	let list_lines = if picker.items.is_empty() {
		vec![Line::styled("No matching files", Style::default().fg(Color::DarkGray))]
	} else {
		let visible_rows = area.height.max(1) as usize;
		let start = picker
			.selected
			.saturating_add(1)
			.saturating_sub(visible_rows)
			.min(picker.items.len().saturating_sub(visible_rows));
		picker
			.items
			.iter()
			.skip(start)
			.take(visible_rows)
			.enumerate()
			.map(|(offset, item)| {
				render_workspace_file_item(item, start + offset == picker.selected, area.width as usize)
			})
			.collect::<Vec<_>>()
	};
	Paragraph::new(list_lines).render(area, buf);
}

fn render_preview(picker: &WorkspaceFilePickerState, area: Rect, buf: &mut Buffer, word_wrap: bool) {
	let lines = if picker.preview_lines.is_empty() {
		vec![Line::styled("Select a file to preview", Style::default().fg(Color::DarkGray))]
	} else {
		let wrapped = preview_rows(picker.preview_lines.as_slice(), area.width as usize, word_wrap);
		let scroll = picker.preview_scroll.min(wrapped.len().saturating_sub(1));
		wrapped
			.into_iter()
			.skip(scroll)
			.take(area.height.max(1) as usize)
			.map(|line| Line::styled(line, Style::default().fg(Color::Gray)))
			.collect::<Vec<_>>()
	};
	let paragraph = if word_wrap {
		Paragraph::new(lines).wrap(Wrap { trim: false })
	} else {
		Paragraph::new(lines)
	};
	paragraph.render(area, buf);
}

fn render_workspace_file_item(item: &WorkspaceFileMatch, selected: bool, width: usize) -> Line<'static> {
	let row_style = if selected { Style::default().bg(Color::Rgb(34, 44, 64)) } else { Style::default() };
	let base_style = row_style.fg(Color::Gray);
	let highlight_style = row_style.fg(Color::White).add_modifier(Modifier::BOLD);
	let highlight_set = item.match_indices.iter().copied().collect::<HashSet<_>>();
	let mut spans = Vec::new();
	let mut used_width = 0usize;

	for (index, ch) in item.relative_path.chars().enumerate() {
		let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
		if used_width.saturating_add(ch_width) > width {
			break;
		}
		let style = if highlight_set.contains(&index) { highlight_style } else { base_style };
		spans.push(Span::styled(ch.to_string(), style));
		used_width = used_width.saturating_add(ch_width);
	}

	if used_width < width {
		spans.push(Span::styled(" ".repeat(width - used_width), base_style));
	}

	Line::from(spans)
}

fn tail_fit(prefix: &str, query: &str, width: usize) -> String {
	let mut rendered = prefix.to_string();
	let prefix_width = UnicodeWidthStr::width(prefix);
	if prefix_width >= width {
		return rendered;
	}
	let available = width - prefix_width;
	let tail = query
		.chars()
		.rev()
		.scan(0usize, |used_width, ch| {
			let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
			if used_width.saturating_add(ch_width) > available {
				return None;
			}
			*used_width = used_width.saturating_add(ch_width);
			Some(ch)
		})
		.collect::<Vec<_>>()
		.into_iter()
		.rev()
		.collect::<String>();
	rendered.push_str(tail.as_str());
	rendered
}

fn draw_horizontal_separator(area: Rect, buf: &mut Buffer) {
	if area.width == 0 || area.height == 0 {
		return;
	}
	for offset in 0..area.width {
		let symbol = if offset == 0 {
			"├"
		} else if offset == area.width.saturating_sub(1) {
			"┤"
		} else {
			"─"
		};
		buf[(area.x + offset, area.y)].set_symbol(symbol).set_fg(Color::Cyan);
	}
}

fn draw_horizontal_separator_between(area: Rect, buf: &mut Buffer) {
	if area.width == 0 || area.height == 0 {
		return;
	}
	for offset in 0..area.width {
		buf[(area.x + offset, area.y)].set_symbol("─").set_fg(Color::Cyan);
	}
}

fn draw_vertical_separator(area: Rect, buf: &mut Buffer) {
	if area.width == 0 || area.height == 0 {
		return;
	}
	for offset in 0..area.height {
		buf[(area.x, area.y + offset)].set_symbol("│").set_fg(Color::Cyan);
	}
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use ratatui::layout::Rect;
	use rim_kernel::state::{RimState, WorkspaceFileEntry};

	use super::WorkspaceFilePickerWidget;

	#[test]
	fn workspace_file_picker_widget_should_stay_within_content_area_on_tiny_layout() {
		let mut state = RimState::new();
		state.open_workspace_file_picker(vec![WorkspaceFileEntry {
			absolute_path: PathBuf::from("/tmp/a.txt"),
			relative_path: "a.txt".to_string(),
		}]);
		let content_area = Rect { x: 0, y: 0, width: 20, height: 4 };
		let widget = WorkspaceFilePickerWidget::from_state(&state, content_area).expect("picker widget exists");
		assert!(widget.area.width <= content_area.width);
		assert!(widget.area.height <= content_area.height);
		let (cursor_x, cursor_y) = widget.cursor_position();
		assert!(cursor_x < content_area.width);
		assert!(cursor_y < content_area.height);
	}
}
