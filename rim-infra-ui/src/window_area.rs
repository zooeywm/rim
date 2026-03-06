use ratatui::{buffer::{Buffer, Cell}, layout::Rect, style::{Color, Style}, widgets::{Paragraph, Widget}};
use rim_kernel::state::RimState;
use ropey::Rope;
use unicode_width::UnicodeWidthChar;

const TAB_DISPLAY_WIDTH: usize = 4;

pub(super) struct WindowAreaWidget {
	windows:            Vec<WindowView>,
	selection_segments: Vec<SelectionSegment>,
	vertical_lines:     Vec<VerticalLine>,
	horizontal_lines:   Vec<HorizontalLine>,
}

#[derive(Debug)]
struct WindowView {
	local_rect:        Rect,
	number_col_width:  u16,
	line_numbers_text: String,
	text_text:         String,
}

#[derive(Debug)]
struct VerticalLine {
	x:       u16,
	y_start: u16,
	y_end:   u16,
}

#[derive(Debug)]
struct HorizontalLine {
	x_start:      u16,
	x_end:        u16,
	y:            u16,
	left_join_x:  Option<u16>,
	right_join_x: Option<u16>,
}

#[derive(Debug)]
struct SelectionSegment {
	x_start: u16,
	x_end:   u16,
	y:       u16,
}

#[derive(Clone, Copy)]
struct VisualSelectionSpec {
	text_rect:  Rect,
	scroll_x:   u16,
	scroll_y:   u16,
	anchor:     rim_kernel::state::CursorState,
	cursor:     rim_kernel::state::CursorState,
	line_wise:  bool,
	block_wise: bool,
}

impl WindowAreaWidget {
	pub(super) fn from_state(state: &RimState, content_area: Rect) -> (Self, Option<(u16, u16)>) {
		let mut windows = Vec::new();
		let mut selection_segments = Vec::new();
		let mut cursor_position = None;

		for window_id in state.active_tab_window_ids() {
			let Some(window) = state.windows.get(window_id) else {
				continue;
			};

			let mut local_rect = Rect {
				x:      window.x,
				y:      window.y,
				width:  window.width.max(1),
				height: window.height.max(1),
			};
			if window.x > 0 {
				local_rect.x = local_rect.x.saturating_add(1);
				local_rect.width = local_rect.width.saturating_sub(1);
			}
			if window.y > 0 {
				local_rect.y = local_rect.y.saturating_add(1);
				local_rect.height = local_rect.height.saturating_sub(1);
			}
			if local_rect.width == 0 || local_rect.height == 0 {
				continue;
			}

			let buffer_text =
				window.buffer_id.and_then(|buffer_id| state.buffers.get(buffer_id)).map(|buf| &buf.text);
			let total_lines = buffer_text.map(rope_display_line_count).unwrap_or(1);
			let desired_number_col_width = total_lines.to_string().len() as u16 + 1;
			let number_col_width =
				if local_rect.width <= desired_number_col_width { 0 } else { desired_number_col_width };
			let text_width = local_rect.width.saturating_sub(number_col_width);
			if text_width == 0 {
				continue;
			}

			let text_rect = Rect {
				x:      local_rect.x.saturating_add(number_col_width),
				y:      local_rect.y,
				width:  text_width,
				height: local_rect.height,
			};

			let scroll_y = window.scroll_y as usize;
			let scroll_x = window.scroll_x as usize;
			let visible_rows = local_rect.height as usize;
			let line_numbers_text = if number_col_width == 0 {
				String::new()
			} else {
				(scroll_y..scroll_y.saturating_add(visible_rows))
					.map(|row_idx| {
						format!("{:>width$} ", row_idx + 1, width = number_col_width.saturating_sub(1) as usize)
					})
					.collect::<Vec<_>>()
					.join("\n")
			};
			let text_text = (scroll_y..scroll_y.saturating_add(visible_rows))
				.map(|row_idx| {
					let line = buffer_text
						.and_then(|text| rope_logical_line(text, row_idx))
						.unwrap_or_else(empty_owned_logical_line);
					let rendered = render_line_for_display(line.text.as_str(), line.has_newline);
					visible_slice_by_display_width(&rendered, scroll_x, text_width as usize)
				})
				.collect::<Vec<_>>()
				.join("\n");

			if state.active_window_id() == window_id {
				let cursor = state.active_cursor();
				let line_idx = cursor.row.saturating_sub(1) as usize;
				let active_line = buffer_text
					.and_then(|text| rope_logical_line(text, line_idx))
					.map(|line| line.text)
					.unwrap_or_default();
				let cursor_col_chars = cursor.col.saturating_sub(1) as usize;
				let cursor_display_col = display_width_of_char_prefix(active_line.as_str(), cursor_col_chars);
				let cursor_line = cursor.row.saturating_sub(1);
				let row_in_view =
					cursor_line >= window.scroll_y && cursor_line < window.scroll_y.saturating_add(text_rect.height);
				let col_in_view_left = cursor_display_col >= scroll_x;
				if row_in_view && col_in_view_left {
					let cursor_x_offset = cursor_display_col
						.saturating_sub(scroll_x)
						.min(text_rect.width.saturating_sub(1) as usize) as u16;
					let cursor_x_local = text_rect.x.saturating_add(cursor_x_offset);
					let cursor_y_local = text_rect.y.saturating_add(cursor_line.saturating_sub(window.scroll_y));
					cursor_position = Some((
						content_area.x.saturating_add(cursor_x_local),
						content_area.y.saturating_add(cursor_y_local),
					));
				}

				if state.is_visual_mode()
					&& let Some(anchor) = state.visual_anchor
					&& let Some(text) = buffer_text
				{
					selection_segments.extend(collect_visual_selection_segments_rope(text, VisualSelectionSpec {
						text_rect,
						scroll_x: window.scroll_x,
						scroll_y: window.scroll_y,
						anchor,
						cursor,
						line_wise: state.is_visual_line_mode(),
						block_wise: state.is_visual_block_mode(),
					}));
				}
			}

			windows.push(WindowView { local_rect, number_col_width, line_numbers_text, text_text });
		}

		let (vertical_lines, horizontal_lines) = collect_split_lines(state, content_area);
		(Self { windows, selection_segments, vertical_lines, horizontal_lines }, cursor_position)
	}
}

#[derive(Debug, Clone)]
struct OwnedLogicalLine {
	text:        String,
	has_newline: bool,
}

fn empty_owned_logical_line() -> OwnedLogicalLine {
	OwnedLogicalLine { text: String::new(), has_newline: false }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
struct LogicalLine<'a> {
	text:        &'a str,
	has_newline: bool,
}

#[cfg(test)]
fn logical_lines_with_newline_info(content: &str) -> Vec<LogicalLine<'_>> {
	let mut lines =
		content.lines().map(|line| LogicalLine { text: line, has_newline: false }).collect::<Vec<_>>();
	if lines.is_empty() {
		lines.push(LogicalLine { text: "", has_newline: false });
		return lines;
	}
	let len = lines.len();
	for line in lines.iter_mut().take(len.saturating_sub(1)) {
		line.has_newline = true;
	}
	if content.ends_with('\n')
		&& let Some(last) = lines.last_mut()
	{
		last.has_newline = true;
	}
	lines
}

fn rope_ends_with_newline(text: &Rope) -> bool {
	text.len_chars() > 0 && text.char(text.len_chars().saturating_sub(1)) == '\n'
}

fn rope_display_line_count(text: &Rope) -> usize {
	let line_count = text.len_lines();
	if line_count == 0 {
		return 1;
	}
	if rope_ends_with_newline(text) { line_count.saturating_sub(1).max(1) } else { line_count.max(1) }
}

fn rope_logical_line(text: &Rope, row_idx: usize) -> Option<OwnedLogicalLine> {
	if row_idx >= rope_display_line_count(text) {
		return None;
	}
	let mut line = text.line(row_idx).to_string();
	let mut has_newline = false;
	if line.ends_with('\n') {
		has_newline = true;
		line.pop();
		if line.ends_with('\r') {
			line.pop();
		}
	}
	Some(OwnedLogicalLine { text: line, has_newline })
}

fn render_line_for_display(line: &str, has_newline: bool) -> String {
	let expanded_line = expand_tabs_for_display(line);
	if has_newline {
		let mut rendered = String::with_capacity(expanded_line.len().saturating_add(3));
		rendered.push_str(expanded_line.as_str());
		rendered.push(' ');
		rendered
	} else {
		expanded_line
	}
}

impl Widget for WindowAreaWidget {
	fn render(self, area: Rect, buf: &mut Buffer) {
		if self.windows.is_empty() {
			Paragraph::new("").render(area, buf);
			return;
		}

		for window in self.windows {
			let abs_rect = Rect {
				x:      area.x.saturating_add(window.local_rect.x),
				y:      area.y.saturating_add(window.local_rect.y),
				width:  window.local_rect.width,
				height: window.local_rect.height,
			};
			let number_rect = Rect {
				x:      abs_rect.x,
				y:      abs_rect.y,
				width:  window.number_col_width,
				height: abs_rect.height,
			};
			let text_rect = Rect {
				x:      abs_rect.x.saturating_add(window.number_col_width),
				y:      abs_rect.y,
				width:  abs_rect.width.saturating_sub(window.number_col_width),
				height: abs_rect.height,
			};

			Paragraph::new(window.line_numbers_text.as_str())
				.style(Style::default().fg(Color::DarkGray))
				.render(number_rect, buf);
			Paragraph::new(window.text_text.as_str()).render(text_rect, buf);
		}

		for segment in self.selection_segments {
			let abs_y = area.y.saturating_add(segment.y);
			for x in segment.x_start..segment.x_end {
				let abs_x = area.x.saturating_add(x);
				if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
					cell.set_bg(Color::DarkGray);
				}
			}
		}

		for line in self.horizontal_lines {
			let abs_y = area.y.saturating_add(line.y);
			if let Some(x) = line.left_join_x {
				let abs_x = area.x.saturating_add(x);
				if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
					set_right_tee_cell(cell);
				}
			}

			for x in line.x_start..line.x_end {
				let abs_x = area.x.saturating_add(x);
				if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
					set_separator_cell(cell);
				}
			}

			if let Some(x) = line.right_join_x {
				let abs_x = area.x.saturating_add(x);
				if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
					set_left_tee_cell(cell);
				}
			}
		}

		for line in self.vertical_lines {
			let abs_x = area.x.saturating_add(line.x);
			for y in line.y_start..line.y_end {
				let abs_y = area.y.saturating_add(y);
				if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
					let is_start = y == line.y_start;
					let is_end = y.saturating_add(1) == line.y_end;
					let dirs = if is_start && is_end {
						DIR_UP | DIR_DOWN
					} else if is_start {
						DIR_DOWN
					} else if is_end {
						DIR_UP
					} else {
						DIR_UP | DIR_DOWN
					};
					merge_cell(cell, dirs);
				}
			}
		}
	}
}

fn collect_split_lines(state: &RimState, content_area: Rect) -> (Vec<VerticalLine>, Vec<HorizontalLine>) {
	let mut vertical_lines = Vec::new();
	let mut horizontal_lines = Vec::new();

	for window_id in state.active_tab_window_ids() {
		let Some(window) = state.windows.get(window_id) else {
			continue;
		};

		let right = window.x.saturating_add(window.width);
		let bottom = window.y.saturating_add(window.height);

		if right < content_area.width {
			let y_start = window.y;
			if y_start < bottom {
				vertical_lines.push(VerticalLine { x: right, y_start, y_end: bottom });
			}
		}
		if bottom < content_area.height {
			let origin_x = window.x;
			let mut start_x = origin_x;
			let mut left_join_x = None;
			if window.x > 0 {
				left_join_x = Some(origin_x);
				start_x = start_x.saturating_add(1);
			}
			let right_join_x = if right < content_area.width { Some(right) } else { None };
			horizontal_lines.push(HorizontalLine {
				x_start: start_x,
				x_end: right,
				y: bottom,
				left_join_x,
				right_join_x,
			});
		}
	}

	(vertical_lines, horizontal_lines)
}

#[cfg(test)]
fn collect_visual_selection_segments(content: &str, spec: VisualSelectionSpec) -> Vec<SelectionSegment> {
	let (start, end) = if spec.block_wise {
		(
			rim_kernel::state::CursorState {
				row: spec.anchor.row.min(spec.cursor.row),
				col: spec.anchor.col.min(spec.cursor.col),
			},
			rim_kernel::state::CursorState {
				row: spec.anchor.row.max(spec.cursor.row),
				col: spec.anchor.col.max(spec.cursor.col),
			},
		)
	} else if (spec.anchor.row, spec.anchor.col) <= (spec.cursor.row, spec.cursor.col) {
		(spec.anchor, spec.cursor)
	} else {
		(spec.cursor, spec.anchor)
	};
	let mut segments = Vec::new();
	if spec.text_rect.width == 0 || spec.text_rect.height == 0 {
		return segments;
	}

	let first_visible_row = spec.scroll_y.saturating_add(1);
	let last_visible_row = spec.scroll_y.saturating_add(spec.text_rect.height);
	let visible_right_exclusive = spec.scroll_x.saturating_add(spec.text_rect.width);

	let logical_lines = logical_lines_with_newline_info(content);
	for row in start.row..=end.row {
		if row < first_visible_row || row > last_visible_row {
			continue;
		}
		let Some(logical_line) = logical_lines.get(row.saturating_sub(1) as usize) else {
			continue;
		};
		let line = logical_line.text;
		let line_len = line.chars().count() as u16;
		let selectable_len = if spec.block_wise {
			line_len
		} else if logical_line.has_newline {
			line_len.saturating_add(1)
		} else {
			line_len
		};
		if selectable_len == 0 {
			continue;
		}

		let (mut col_start, mut col_end) = if spec.line_wise {
			(1, selectable_len)
		} else if spec.block_wise {
			if start.col > selectable_len {
				continue;
			}
			(start.col, end.col.min(selectable_len))
		} else if start.row == end.row {
			(start.col, end.col)
		} else if row == start.row {
			(start.col, selectable_len)
		} else if row == end.row {
			(1, end.col)
		} else {
			(1, selectable_len)
		};

		col_start = col_start.max(1).min(selectable_len.max(1));
		col_end = col_end.max(1).min(selectable_len.max(1));
		if col_start > col_end {
			continue;
		}

		let start_display =
			display_width_of_logical_col(line, logical_line.has_newline, col_start.saturating_sub(1) as usize)
				as u16;
		let end_display = display_width_of_logical_col(line, logical_line.has_newline, col_end as usize) as u16;
		let seg_start = start_display.max(spec.scroll_x);
		let seg_end = end_display.min(visible_right_exclusive);
		if seg_start >= seg_end {
			continue;
		}

		let y = spec.text_rect.y.saturating_add(row.saturating_sub(first_visible_row));
		let x_start = spec.text_rect.x.saturating_add(seg_start.saturating_sub(spec.scroll_x));
		let x_end = spec.text_rect.x.saturating_add(seg_end.saturating_sub(spec.scroll_x));
		segments.push(SelectionSegment { x_start, x_end, y });
	}

	segments
}

fn collect_visual_selection_segments_rope(
	content: &Rope,
	spec: VisualSelectionSpec,
) -> Vec<SelectionSegment> {
	let (start, end) = if spec.block_wise {
		(
			rim_kernel::state::CursorState {
				row: spec.anchor.row.min(spec.cursor.row),
				col: spec.anchor.col.min(spec.cursor.col),
			},
			rim_kernel::state::CursorState {
				row: spec.anchor.row.max(spec.cursor.row),
				col: spec.anchor.col.max(spec.cursor.col),
			},
		)
	} else if (spec.anchor.row, spec.anchor.col) <= (spec.cursor.row, spec.cursor.col) {
		(spec.anchor, spec.cursor)
	} else {
		(spec.cursor, spec.anchor)
	};
	let mut segments = Vec::new();
	if spec.text_rect.width == 0 || spec.text_rect.height == 0 {
		return segments;
	}

	let first_visible_row = spec.scroll_y.saturating_add(1);
	let last_visible_row = spec.scroll_y.saturating_add(spec.text_rect.height);
	let visible_right_exclusive = spec.scroll_x.saturating_add(spec.text_rect.width);

	for row in start.row..=end.row {
		if row < first_visible_row || row > last_visible_row {
			continue;
		}
		let Some(logical_line) = rope_logical_line(content, row.saturating_sub(1) as usize) else {
			continue;
		};
		let line = logical_line.text.as_str();
		let line_len = line.chars().count() as u16;
		let selectable_len = if spec.block_wise {
			line_len
		} else if logical_line.has_newline {
			line_len.saturating_add(1)
		} else {
			line_len
		};
		if selectable_len == 0 {
			continue;
		}

		let (mut col_start, mut col_end) = if spec.line_wise {
			(1, selectable_len)
		} else if spec.block_wise {
			if start.col > selectable_len {
				continue;
			}
			(start.col, end.col.min(selectable_len))
		} else if start.row == end.row {
			(start.col, end.col)
		} else if row == start.row {
			(start.col, selectable_len)
		} else if row == end.row {
			(1, end.col)
		} else {
			(1, selectable_len)
		};

		col_start = col_start.max(1).min(selectable_len.max(1));
		col_end = col_end.max(1).min(selectable_len.max(1));
		if col_start > col_end {
			continue;
		}

		let start_display =
			display_width_of_logical_col(line, logical_line.has_newline, col_start.saturating_sub(1) as usize)
				as u16;
		let end_display = display_width_of_logical_col(line, logical_line.has_newline, col_end as usize) as u16;
		let seg_start = start_display.max(spec.scroll_x);
		let seg_end = end_display.min(visible_right_exclusive);
		if seg_start >= seg_end {
			continue;
		}

		let y = spec.text_rect.y.saturating_add(row.saturating_sub(first_visible_row));
		let x_start = spec.text_rect.x.saturating_add(seg_start.saturating_sub(spec.scroll_x));
		let x_end = spec.text_rect.x.saturating_add(seg_end.saturating_sub(spec.scroll_x));
		segments.push(SelectionSegment { x_start, x_end, y });
	}

	segments
}

fn display_width_of_char_prefix(line: &str, char_count: usize) -> usize {
	line.chars().take(char_count).map(char_display_width).sum()
}

fn display_width_of_logical_col(line: &str, has_newline: bool, logical_char_count: usize) -> usize {
	let line_char_count = line.chars().count();
	let mut width = display_width_of_char_prefix(line, logical_char_count.min(line_char_count));
	if has_newline && logical_char_count > line_char_count {
		width = width.saturating_add(1);
	}
	width
}

fn visible_slice_by_display_width(line: &str, skip_cols: usize, max_cols: usize) -> String {
	if max_cols == 0 || line.is_empty() {
		return String::new();
	}

	let mut consumed = 0usize;
	let mut start = line.len();
	for (idx, ch) in line.char_indices() {
		let width = char_display_width(ch);
		if consumed + width <= skip_cols {
			consumed += width;
			continue;
		}
		start = if consumed < skip_cols { idx + ch.len_utf8() } else { idx };
		break;
	}

	let mut out = String::new();
	let mut used = 0usize;
	for ch in line[start..].chars() {
		let width = char_display_width(ch);
		if width == 0 {
			if !out.is_empty() {
				out.push(ch);
			}
			continue;
		}
		if used + width > max_cols {
			break;
		}
		out.push(ch);
		used += width;
	}
	out
}

fn char_display_width(ch: char) -> usize {
	if ch == '\t' { TAB_DISPLAY_WIDTH } else { UnicodeWidthChar::width(ch).unwrap_or(0) }
}

fn expand_tabs_for_display(line: &str) -> String {
	let mut rendered = String::with_capacity(line.len());
	for ch in line.chars() {
		if ch == '\t' {
			rendered.push_str("    ");
		} else {
			rendered.push(ch);
		}
	}
	rendered
}

fn set_separator_cell(cell: &mut Cell) { merge_cell(cell, DIR_LEFT | DIR_RIGHT); }

fn set_right_tee_cell(cell: &mut Cell) { merge_cell(cell, DIR_UP | DIR_RIGHT); }

fn set_left_tee_cell(cell: &mut Cell) { merge_cell(cell, DIR_UP | DIR_LEFT); }

const DIR_UP: u8 = 0b0001;
const DIR_DOWN: u8 = 0b0010;
const DIR_LEFT: u8 = 0b0100;
const DIR_RIGHT: u8 = 0b1000;

fn merge_cell(cell: &mut Cell, add_dirs: u8) {
	let merged = symbol_from_dirs(dirs_from_symbol(cell.symbol()) | add_dirs);
	cell.set_symbol(merged);
	cell.set_fg(Color::DarkGray);
}

fn dirs_from_symbol(symbol: &str) -> u8 {
	match symbol {
		"│" => DIR_UP | DIR_DOWN,
		"─" => DIR_LEFT | DIR_RIGHT,
		"├" => DIR_UP | DIR_DOWN | DIR_RIGHT,
		"┤" => DIR_UP | DIR_DOWN | DIR_LEFT,
		"┬" => DIR_LEFT | DIR_RIGHT | DIR_DOWN,
		"┴" => DIR_LEFT | DIR_RIGHT | DIR_UP,
		"┼" => DIR_UP | DIR_DOWN | DIR_LEFT | DIR_RIGHT,
		"┌" => DIR_DOWN | DIR_RIGHT,
		"┐" => DIR_DOWN | DIR_LEFT,
		"└" => DIR_UP | DIR_RIGHT,
		"┘" => DIR_UP | DIR_LEFT,
		_ => 0,
	}
}

fn symbol_from_dirs(dirs: u8) -> &'static str {
	match dirs {
		d if d == (DIR_UP | DIR_DOWN | DIR_LEFT | DIR_RIGHT) => "┼",
		d if d == (DIR_UP | DIR_DOWN | DIR_RIGHT) => "├",
		d if d == (DIR_UP | DIR_DOWN | DIR_LEFT) => "┤",
		d if d == (DIR_LEFT | DIR_RIGHT | DIR_DOWN) => "┬",
		d if d == (DIR_LEFT | DIR_RIGHT | DIR_UP) => "┴",
		d if d == (DIR_UP | DIR_DOWN) => "│",
		d if d == (DIR_LEFT | DIR_RIGHT) => "─",
		d if d == (DIR_DOWN | DIR_RIGHT) => "┌",
		d if d == (DIR_DOWN | DIR_LEFT) => "┐",
		d if d == (DIR_UP | DIR_RIGHT) => "└",
		d if d == (DIR_UP | DIR_LEFT) => "┘",
		d if d == DIR_UP || d == DIR_DOWN => "│",
		d if d == DIR_LEFT || d == DIR_RIGHT => "─",
		_ => " ",
	}
}

#[cfg(test)]
mod tests;
