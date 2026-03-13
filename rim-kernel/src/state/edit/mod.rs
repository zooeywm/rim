mod core_edit;
mod movement;
mod visual;

use ropey::Rope;

use super::{CursorState, PendingBlockInsert, RimState, rope_ends_with_newline, rope_line_count, rope_line_len_chars, rope_line_without_newline};

impl RimState {
	fn active_buffer_cursor_mut(&mut self) -> Option<&mut CursorState> {
		let active_window_id = self.active_window_id();
		self.windows.get_mut(active_window_id).map(|window| &mut window.cursor)
	}

	fn active_buffer_and_window_mut(&mut self) -> Option<(&mut super::BufferState, &mut super::WindowState)> {
		let buffer_id = self.active_buffer_id()?;
		let window_id = self.active_window_id();
		let buffer = self.buffers.get_mut(buffer_id)?;
		let window = self.windows.get_mut(window_id)?;
		Some((buffer, window))
	}
}

fn split_lines_owned(text: &str) -> Vec<String> {
	let mut lines = text.split('\n').map(ToString::to_string).collect::<Vec<_>>();
	if lines.is_empty() {
		lines.push(String::new());
	}
	lines
}

fn rope_editable_line_count(text: &Rope) -> usize { text.len_lines().max(1) }

fn rope_editable_line_len_chars(text: &Rope, row_idx: usize) -> Option<usize> {
	if row_idx >= rope_editable_line_count(text) {
		return None;
	}
	let mut line = text.line(row_idx).to_string();
	if line.ends_with('\n') {
		line.pop();
		if line.ends_with('\r') {
			line.pop();
		}
	}
	Some(line.chars().count())
}

fn rope_line_start_char(text: &Rope, row_idx: usize) -> Option<usize> {
	(row_idx < rope_editable_line_count(text)).then(|| text.line_to_char(row_idx))
}

fn rope_line_char_end_without_newline(text: &Rope, row_idx: usize) -> Option<usize> {
	let start = rope_line_start_char(text, row_idx)?;
	let line_len = rope_editable_line_len_chars(text, row_idx)?;
	Some(start.saturating_add(line_len))
}

fn rope_line_char_range_without_newline(text: &Rope, row_idx: usize) -> Option<std::ops::Range<usize>> {
	let start = rope_line_start_char(text, row_idx)?;
	let end = rope_line_char_end_without_newline(text, row_idx)?;
	Some(start..end)
}

fn rope_cursor_char(text: &Rope, row_idx: usize, col_idx: usize) -> Option<usize> {
	let start = rope_line_start_char(text, row_idx)?;
	let line_len = rope_editable_line_len_chars(text, row_idx)?;
	Some(start.saturating_add(col_idx.min(line_len)))
}

fn rope_block_char_range(
	text: &Rope,
	row_idx: usize,
	start_col: u16,
	end_col: u16,
) -> Option<std::ops::Range<usize>> {
	let line = rope_line_without_newline(text, row_idx)?;
	let (start_idx, end_idx) = block_char_range_for_line(line.as_str(), start_col, end_col)?;
	let line_start = rope_line_start_char(text, row_idx)?;
	Some(line_start.saturating_add(start_idx)..line_start.saturating_add(end_idx))
}

fn rope_linewise_char_range(text: &Rope, start_row: usize, end_row: usize) -> Option<std::ops::Range<usize>> {
	let start = rope_line_start_char(text, start_row)?;
	let end = if end_row.saturating_add(1) < rope_editable_line_count(text) {
		rope_line_start_char(text, end_row.saturating_add(1))?
	} else {
		text.len_chars()
	};
	Some(start..end)
}

fn rope_join_rows_without_newline(text: &Rope, start_row: usize, end_row: usize) -> Option<String> {
	if start_row > end_row || end_row >= rope_editable_line_count(text) {
		return None;
	}
	let mut joined = String::new();
	for row_idx in start_row..=end_row {
		if row_idx > start_row {
			joined.push('\n');
		}
		joined.push_str(rope_line_without_newline(text, row_idx)?.as_str());
	}
	Some(joined)
}

fn rope_linewise_insertion_text(slot_text: &str, has_following_rows: bool) -> String {
	let mut replacement = slot_text.to_string();
	if has_following_rows {
		replacement.push('\n');
	}
	replacement
}

fn ensure_rope_editable_rows(text: &mut Rope, target_row_idx: usize) {
	while rope_editable_line_count(text) <= target_row_idx {
		text.insert(text.len_chars(), "\n");
	}
}

fn pad_rope_line_to_char_len(text: &mut Rope, row_idx: usize, target_len: usize) {
	let Some(line_len) = rope_editable_line_len_chars(text, row_idx) else {
		return;
	};
	if line_len >= target_len {
		return;
	}
	let Some(insert_at) = rope_line_char_end_without_newline(text, row_idx) else {
		return;
	};
	text.insert(insert_at, " ".repeat(target_len.saturating_sub(line_len)).as_str());
}

fn block_char_range_for_line(line: &str, start_col: u16, end_col: u16) -> Option<(usize, usize)> {
	let line_len = line.chars().count();
	let start_idx = start_col.saturating_sub(1) as usize;
	let end_idx = end_col as usize;
	let clamped_start = start_idx.min(line_len);
	let clamped_end = end_idx.min(line_len);
	if clamped_start >= clamped_end {
		return None;
	}
	Some((clamped_start, clamped_end))
}

fn clamp_cursor_col_for_line(line: &str, desired_col: u16) -> u16 {
	desired_col.min(line.chars().count() as u16 + 1).max(1)
}
