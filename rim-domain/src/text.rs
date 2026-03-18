use std::path::Path;

use ropey::Rope;

use crate::model::{BufferEditSnapshot, CursorState, RopeTextDiff};

pub fn buffer_name_from_path(path: &Path) -> Option<String> {
	path.file_name().map(|name| name.to_string_lossy().to_string())
}

pub fn rope_line_count(text: &Rope) -> usize {
	let line_count = text.len_lines();
	if line_count == 0 {
		return 1;
	}
	if rope_ends_with_newline(text) { line_count.saturating_sub(1).max(1) } else { line_count.max(1) }
}

pub fn rope_is_empty(text: &Rope) -> bool { text.len_chars() == 0 }

pub fn rope_line_without_newline(text: &Rope, row_index: usize) -> Option<String> {
	if row_index >= rope_line_count(text) {
		return None;
	}
	let mut line = text.line(row_index).to_string();
	if line.ends_with('\n') {
		line.pop();
		if line.ends_with('\r') {
			line.pop();
		}
	}
	Some(line)
}

pub fn rope_line_len_chars(text: &Rope, row_index: usize) -> usize {
	rope_line_without_newline(text, row_index).map(|line| line.chars().count()).unwrap_or(0)
}

pub fn rope_ends_with_newline(text: &Rope) -> bool {
	text.len_chars() > 0 && text.char(text.len_chars().saturating_sub(1)) == '\n'
}

pub fn clamp_cursor_for_rope(text: &Rope, cursor: CursorState) -> CursorState {
	let max_row = rope_line_count(text) as u16;
	let row = cursor.row.min(max_row).max(1);
	let row_index = row.saturating_sub(1) as usize;
	let max_col = rope_line_len_chars(text, row_index).max(1) as u16;
	let col = cursor.col.min(max_col).max(1);
	CursorState { row, col }
}

pub fn apply_text_delta_undo(text: &mut Rope, delta: &BufferEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let inserted_end_byte = delta.start_byte.saturating_add(delta.inserted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(inserted_end_byte);
	text.remove(start_char..end_char);
	text.insert(start_char, delta.deleted_text.as_str());
}

pub fn apply_text_delta_redo(text: &mut Rope, delta: &BufferEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let deleted_end_byte = delta.start_byte.saturating_add(delta.deleted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(deleted_end_byte);
	text.remove(start_char..end_char);
	text.insert(start_char, delta.inserted_text.as_str());
}

pub fn merge_adjacent_insert_history_edits(
	last_edit: &mut BufferEditSnapshot,
	next_edit: &BufferEditSnapshot,
) -> bool {
	if !last_edit.deleted_text.is_empty() || !next_edit.deleted_text.is_empty() {
		return false;
	}
	let expected_start = last_edit.start_byte.saturating_add(last_edit.inserted_text.len());
	if next_edit.start_byte != expected_start {
		return false;
	}
	last_edit.inserted_text.push_str(next_edit.inserted_text.as_str());
	true
}

pub fn compute_rope_text_diff(before: &Rope, after: &Rope) -> Option<RopeTextDiff> {
	if before == after {
		return None;
	}

	let mut common_prefix_chars = 0usize;
	let mut common_prefix_bytes = 0usize;
	for (before_ch, after_ch) in before.chars().zip(after.chars()) {
		if before_ch != after_ch {
			break;
		}
		common_prefix_chars = common_prefix_chars.saturating_add(1);
		common_prefix_bytes = common_prefix_bytes.saturating_add(before_ch.len_utf8());
	}

	let before_len_chars = before.len_chars();
	let after_len_chars = after.len_chars();
	let mut before_mid_end = before_len_chars;
	let mut after_mid_end = after_len_chars;
	while before_mid_end > common_prefix_chars && after_mid_end > common_prefix_chars {
		if before.char(before_mid_end.saturating_sub(1)) != after.char(after_mid_end.saturating_sub(1)) {
			break;
		}
		before_mid_end = before_mid_end.saturating_sub(1);
		after_mid_end = after_mid_end.saturating_sub(1);
	}

	Some(RopeTextDiff {
		start_char:    common_prefix_chars,
		start_byte:    common_prefix_bytes,
		deleted_text:  before.slice(common_prefix_chars..before_mid_end).to_string(),
		inserted_text: after.slice(common_prefix_chars..after_mid_end).to_string(),
	})
}
