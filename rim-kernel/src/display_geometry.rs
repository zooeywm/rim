use ropey::Rope;
use unicode_width::UnicodeWidthChar;

use crate::state::{CursorState, rope_ends_with_newline, rope_line_count, rope_line_without_newline};

pub const TAB_DISPLAY_WIDTH: usize = 4;

pub fn char_display_width(ch: char) -> usize {
	if ch == '\t' { TAB_DISPLAY_WIDTH } else { UnicodeWidthChar::width(ch).unwrap_or(0) }
}

pub fn display_width_of_char_prefix(line: &str, char_count: usize) -> usize {
	line.chars().take(char_count).map(char_display_width).sum()
}

pub fn display_width_of_char_prefix_with_virtual(line: &str, char_count: usize) -> usize {
	let line_chars = line.chars().count();
	let base = display_width_of_char_prefix(line, char_count.min(line_chars));
	base.saturating_add(char_count.saturating_sub(line_chars))
}

pub fn display_col_of_cursor_slot(line: &str, col: u16) -> u16 {
	let char_count = col.saturating_sub(1) as usize;
	display_width_of_char_prefix_with_virtual(line, char_count) as u16
}

pub fn cursor_col_for_display_slot(line: &str, target_display_col: u16) -> u16 {
	let mut consumed = 0u16;
	let mut col = 1u16;
	for ch in line.chars() {
		let width = char_display_width(ch).max(1) as u16;
		if consumed.saturating_add(width) > target_display_col {
			return col;
		}
		consumed = consumed.saturating_add(width);
		col = col.saturating_add(1);
	}
	if target_display_col <= consumed {
		col
	} else {
		col.saturating_add(target_display_col.saturating_sub(consumed))
	}
}

pub fn previous_char_display_width_at_cursor(line: &str, col: u16) -> u16 {
	if col <= 1 {
		return 1;
	}
	let prev_idx = col.saturating_sub(2) as usize;
	line.chars().nth(prev_idx).map(|ch| char_display_width(ch).max(1) as u16).unwrap_or(1)
}

pub fn wrapped_line_rows(line: &str, has_newline: bool, width: usize) -> u16 {
	let mut display_width = line.chars().map(char_display_width).sum::<usize>();
	if has_newline {
		display_width = display_width.saturating_add(1);
	}
	display_width.max(1).div_ceil(width.max(1)) as u16
}

pub fn wrapped_total_rows_for_rope(text: &Rope, width: usize) -> u16 {
	let width = width.max(1);
	let mut total = 0u16;
	let lines = rope_line_count(text);
	for row_idx in 0..lines {
		let has_newline = row_idx + 1 < lines || (row_idx + 1 == lines && rope_ends_with_newline(text));
		let line_rows = rope_line_without_newline(text, row_idx)
			.map(|line| wrapped_line_rows(line.as_str(), has_newline, width))
			.unwrap_or_else(|| wrapped_line_rows("", has_newline, width));
		total = total.saturating_add(line_rows);
	}
	total.max(1)
}

pub fn wrapped_rows_before_row_for_rope(text: &Rope, row: u16, width: usize) -> u16 {
	let width = width.max(1);
	let mut total = 0u16;
	let row_idx_limit = row.saturating_sub(1) as usize;
	let lines = rope_line_count(text);
	for row_idx in 0..row_idx_limit.min(lines) {
		let has_newline = row_idx + 1 < lines || (row_idx + 1 == lines && rope_ends_with_newline(text));
		let line_rows = rope_line_without_newline(text, row_idx)
			.map(|line| wrapped_line_rows(line.as_str(), has_newline, width))
			.unwrap_or_else(|| wrapped_line_rows("", has_newline, width));
		total = total.saturating_add(line_rows);
	}
	total
}

pub fn wrapped_row_index_for_cursor(text: &Rope, cursor: CursorState, width: usize) -> u16 {
	let width = width.max(1);
	let before = wrapped_rows_before_row_for_rope(text, cursor.row, width);
	let row_index = cursor.row.saturating_sub(1) as usize;
	let char_index = cursor.col.saturating_sub(1) as usize;
	let display_col = rope_line_without_newline(text, row_index)
		.map(|line| display_width_of_char_prefix(line.as_str(), char_index))
		.unwrap_or(0);
	before.saturating_add((display_col / width) as u16)
}

pub fn wrapped_row_index_for_row_display_col(text: &Rope, row: u16, display_col: usize, width: usize) -> u16 {
	let width = width.max(1);
	let before = wrapped_rows_before_row_for_rope(text, row, width);
	before.saturating_add((display_col / width) as u16)
}

pub fn navigable_col_for_display_target(text: &Rope, row: u16, target_display_col: u16) -> u16 {
	let row_index = row.saturating_sub(1) as usize;
	let Some(line) = rope_line_without_newline(text, row_index) else {
		return 1;
	};
	let mut consumed = 0u16;
	let mut col = 1u16;
	for ch in line.chars() {
		let width = char_display_width(ch).max(1) as u16;
		if consumed >= target_display_col {
			break;
		}
		if consumed.saturating_add(width) > target_display_col {
			break;
		}
		consumed = consumed.saturating_add(width);
		col = col.saturating_add(1);
	}
	col.min(line.chars().count() as u16).max(1)
}

pub fn wrap_line_with_display_span(line: &str, max_cols: usize) -> Vec<(usize, usize, String)> {
	if max_cols == 0 || line.is_empty() {
		return vec![(0, 0, String::new())];
	}

	let mut rows = Vec::new();
	let mut current = String::new();
	let mut row_start = 0usize;
	let mut row_width = 0usize;
	for ch in line.chars() {
		let width = char_display_width(ch).max(1);
		if row_width > 0 && row_width.saturating_add(width) > max_cols {
			rows.push((row_start, row_start.saturating_add(row_width), std::mem::take(&mut current)));
			row_start = row_start.saturating_add(row_width);
			row_width = 0;
		}
		current.push(ch);
		row_width = row_width.saturating_add(width);
	}
	rows.push((row_start, row_start.saturating_add(row_width), current));
	rows
}
