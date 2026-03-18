use std::ops::Range;

use ropey::Rope;

use crate::{display_geometry::{char_display_width, cursor_col_for_display_slot, display_col_of_cursor_slot, previous_char_display_width_at_cursor}, text::rope_line_without_newline};

pub fn split_lines_owned(text: &str) -> Vec<String> {
	let mut lines = text.split('\n').map(ToString::to_string).collect::<Vec<_>>();
	if lines.is_empty() {
		lines.push(String::new());
	}
	lines
}

pub fn rope_editable_line_count(text: &Rope) -> usize { text.len_lines().max(1) }

pub fn rope_editable_line_len_chars(text: &Rope, row_idx: usize) -> Option<usize> {
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

pub fn rope_line_start_char(text: &Rope, row_idx: usize) -> Option<usize> {
	(row_idx < rope_editable_line_count(text)).then(|| text.line_to_char(row_idx))
}

pub fn rope_line_char_end_without_newline(text: &Rope, row_idx: usize) -> Option<usize> {
	let start = rope_line_start_char(text, row_idx)?;
	let line_len = rope_editable_line_len_chars(text, row_idx)?;
	Some(start.saturating_add(line_len))
}

pub fn rope_line_char_range_without_newline(text: &Rope, row_idx: usize) -> Option<Range<usize>> {
	let start = rope_line_start_char(text, row_idx)?;
	let end = rope_line_char_end_without_newline(text, row_idx)?;
	Some(start..end)
}

pub fn rope_cursor_char(text: &Rope, row_idx: usize, col_idx: usize) -> Option<usize> {
	let start = rope_line_start_char(text, row_idx)?;
	let line_len = rope_editable_line_len_chars(text, row_idx)?;
	Some(start.saturating_add(col_idx.min(line_len)))
}

pub fn rope_block_char_range(
	text: &Rope,
	row_idx: usize,
	start_col: u16,
	end_col: u16,
) -> Option<Range<usize>> {
	let line = rope_line_without_newline(text, row_idx)?;
	let (start_idx, end_idx) = block_char_range_for_line(line.as_str(), start_col, end_col)?;
	let line_start = rope_line_start_char(text, row_idx)?;
	Some(line_start.saturating_add(start_idx)..line_start.saturating_add(end_idx))
}

pub fn rope_linewise_char_range(text: &Rope, start_row: usize, end_row: usize) -> Option<Range<usize>> {
	let start = rope_line_start_char(text, start_row)?;
	let end = if end_row.saturating_add(1) < rope_editable_line_count(text) {
		rope_line_start_char(text, end_row.saturating_add(1))?
	} else {
		text.len_chars()
	};
	Some(start..end)
}

pub fn rope_join_rows_without_newline(text: &Rope, start_row: usize, end_row: usize) -> Option<String> {
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

pub fn rope_linewise_insertion_text(slot_text: &str, has_following_rows: bool) -> String {
	let mut replacement = slot_text.to_string();
	if has_following_rows {
		replacement.push('\n');
	}
	replacement
}

pub fn ensure_rope_editable_rows(text: &mut Rope, target_row_idx: usize) {
	while rope_editable_line_count(text) <= target_row_idx {
		text.insert(text.len_chars(), "\n");
	}
}

pub fn pad_rope_line_to_char_len(text: &mut Rope, row_idx: usize, target_len: usize) {
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

pub fn block_char_range_for_line(line: &str, start_col: u16, end_col: u16) -> Option<(usize, usize)> {
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

pub fn clamp_cursor_col_for_line(line: &str, desired_col: u16) -> u16 {
	desired_col.min(line.chars().count() as u16 + 1).max(1)
}

pub fn expand_tab_padding_at_display_target(text: &mut Rope, row: u16, target_display_col: u16) {
	let Some((row_idx, tab_char_idx, tab_width)) =
		tab_padding_span_at_display_target(text, row, target_display_col)
	else {
		return;
	};
	let tab_start = rope_cursor_char(text, row_idx, tab_char_idx).expect("tab start must exist");
	let tab_end = rope_cursor_char(text, row_idx, tab_char_idx.saturating_add(1)).expect("tab end must exist");
	text.remove(tab_start..tab_end);
	text.insert(tab_start, &" ".repeat(tab_width as usize));
}

pub fn tab_padding_span_at_display_target(
	text: &Rope,
	row: u16,
	target_display_col: u16,
) -> Option<(usize, usize, u16)> {
	let row_idx = row.saturating_sub(1) as usize;
	let line = rope_line_without_newline(text, row_idx)?;
	let mut consumed = 0u16;

	for (char_idx, ch) in line.chars().enumerate() {
		let width = char_display_width(ch).max(1) as u16;
		if ch == '\t' && consumed < target_display_col && target_display_col < consumed.saturating_add(width) {
			return Some((row_idx, char_idx, width));
		}
		consumed = consumed.saturating_add(width);
	}

	None
}

pub fn block_col_for_display_target(text: &Rope, row: u16, target_display_col: u16) -> u16 {
	let row_index = row.saturating_sub(1) as usize;
	let line = rope_line_without_newline(text, row_index).unwrap_or_default();
	cursor_col_for_display_slot(line.as_str(), target_display_col)
}

pub fn cursor_slot_display_col(text: &Rope, row: u16, col: u16) -> u16 {
	let row_index = row.saturating_sub(1) as usize;
	let line = rope_line_without_newline(text, row_index).unwrap_or_default();
	display_col_of_cursor_slot(line.as_str(), col)
}

pub fn previous_char_display_width(text: &Rope, row: u16, col: u16) -> u16 {
	let row_index = row.saturating_sub(1) as usize;
	let line = rope_line_without_newline(text, row_index).unwrap_or_default();
	previous_char_display_width_at_cursor(line.as_str(), col)
}
