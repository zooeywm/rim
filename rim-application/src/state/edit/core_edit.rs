use super::{RimState, rope_cursor_char, rope_editable_line_count, rope_editable_line_len_chars, rope_line_char_end_without_newline, rope_line_char_range_without_newline, rope_line_start_char, split_lines_owned};
use crate::state::{rope_ends_with_newline, rope_line_count, rope_line_len_chars};

impl RimState {
	pub fn insert_char_at_cursor(&mut self, ch: char) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			return;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		let Some(insert_at) = rope_cursor_char(&buffer.text, row_idx, col_idx) else {
			return;
		};
		let mut encoded = [0; 4];
		buffer.text.insert(insert_at, ch.encode_utf8(&mut encoded));
		window.cursor.col = window.cursor.col.saturating_add(1);
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
	}

	pub fn insert_newline_at_cursor(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			return;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		let Some(insert_at) = rope_cursor_char(&buffer.text, row_idx, col_idx) else {
			return;
		};
		buffer.text.insert(insert_at, "\n");
		window.cursor.row = window.cursor.row.saturating_add(1);
		window.cursor.col = 1;
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
	}

	pub fn open_line_below_at_cursor(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			return;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let Some(insert_at) = rope_line_char_end_without_newline(&buffer.text, row_idx) else {
			return;
		};
		buffer.text.insert(insert_at, "\n");
		window.cursor.row = window.cursor.row.saturating_add(1);
		window.cursor.col = 1;
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
	}

	pub fn open_line_above_at_cursor(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			return;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let Some(insert_at) = rope_line_start_char(&buffer.text, row_idx) else {
			return;
		};
		buffer.text.insert(insert_at, "\n");
		window.cursor.col = 1;
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
	}

	pub fn join_line_below_at_cursor(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			return;
		};

		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		if row_idx + 1 >= rope_editable_line_count(&buffer.text) {
			return;
		}

		let Some(current_range) = rope_line_char_range_without_newline(&buffer.text, row_idx) else {
			return;
		};
		let Some(next_range) = rope_line_char_range_without_newline(&buffer.text, row_idx.saturating_add(1))
		else {
			return;
		};
		let current = buffer.text.slice(current_range.clone()).to_string();
		let next = buffer.text.slice(next_range.clone()).to_string();
		let next_trimmed = next.trim_start();
		let mut merged = current;
		if !merged.is_empty() && !next_trimmed.is_empty() && !merged.ends_with(' ') {
			merged.push(' ');
		}
		merged.push_str(next_trimmed);

		buffer.text.remove(current_range.start..next_range.end);
		buffer.text.insert(current_range.start, merged.as_str());
		let max_col = merged.chars().count() as u16 + 1;
		window.cursor.col = window.cursor.col.min(max_col).max(1);
		self.mark_active_buffer_dirty();
		self.preferred_col = None;
		self.align_active_window_scroll_to_cursor();
	}

	pub fn backspace_at_cursor(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			return;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		if row_idx >= rope_editable_line_count(&buffer.text) {
			return;
		}

		if col_idx > 0 {
			let Some(delete_end) = rope_cursor_char(&buffer.text, row_idx, col_idx) else {
				return;
			};
			let delete_start = delete_end.saturating_sub(1);
			buffer.text.remove(delete_start..delete_end);
			window.cursor.col = window.cursor.col.saturating_sub(1);
		} else if row_idx > 0 {
			let Some(current_start) = rope_line_start_char(&buffer.text, row_idx) else {
				return;
			};
			if current_start == 0 {
				return;
			}
			let prev_char_len = rope_line_len_chars(&buffer.text, row_idx.saturating_sub(1)) as u16;
			buffer.text.remove(current_start.saturating_sub(1)..current_start);
			window.cursor.row = window.cursor.row.saturating_sub(1);
			window.cursor.col = prev_char_len.saturating_add(1);
		} else {
			return;
		}
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
	}

	pub fn cut_current_char_to_slot(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			self.workbench.status_bar.message = "cut failed: no active buffer".to_string();
			return;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		let Some(line_range) = rope_line_char_range_without_newline(&buffer.text, row_idx) else {
			self.workbench.status_bar.message = "cut failed: out of range".to_string();
			return;
		};
		let char_count = line_range.end.saturating_sub(line_range.start);
		if col_idx >= char_count {
			self.workbench.status_bar.message = "cut failed: no char".to_string();
			return;
		}

		let start = line_range.start.saturating_add(col_idx);
		let end = start.saturating_add(1);
		let cut = buffer.text.slice(start..end).to_string();
		buffer.text.remove(start..end);
		self.mark_active_buffer_dirty();
		self.line_slot = Some(cut);
		self.line_slot_line_wise = false;
		self.line_slot_block_wise = false;
		self.align_active_window_scroll_to_cursor();
		self.workbench.status_bar.message = "char cut".to_string();
	}

	pub fn paste_slot_at_cursor(&mut self) {
		let Some(slot_text) = self.line_slot.clone() else {
			self.workbench.status_bar.message = "paste failed: slot is empty".to_string();
			return;
		};
		let line_wise_slot = self.line_slot_line_wise;
		let block_wise_slot = self.line_slot_block_wise;
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			self.workbench.status_bar.message = "paste failed: no active buffer".to_string();
			return;
		};

		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		if row_idx >= rope_editable_line_count(&buffer.text) {
			self.workbench.status_bar.message = "paste failed: out of range".to_string();
			return;
		}
		if line_wise_slot {
			let insert_row = row_idx.saturating_add(1).min(rope_editable_line_count(&buffer.text));
			let inserted_count = split_lines_owned(&slot_text).len() as u16;
			if insert_row < rope_editable_line_count(&buffer.text) {
				let insert_at = rope_line_start_char(&buffer.text, insert_row).expect("target line start must exist");
				let insertion = format!("{}\n", slot_text);
				buffer.text.insert(insert_at, insertion.as_str());
			} else if rope_ends_with_newline(&buffer.text) {
				buffer.text.insert(buffer.text.len_chars(), slot_text.as_str());
			} else {
				let insert_at = rope_line_char_end_without_newline(&buffer.text, row_idx)
					.expect("active line end must exist while linewise pasting");
				let insertion = format!("\n{}", slot_text);
				buffer.text.insert(insert_at, insertion.as_str());
			}

			window.cursor.row = insert_row as u16 + inserted_count;
			window.cursor.col = 1;
			self.mark_active_buffer_dirty();
			self.align_active_window_scroll_to_cursor();
			self.workbench.status_bar.message = "pasted".to_string();
			return;
		}

		if block_wise_slot {
			let insert_char_idx = window.cursor.col as usize;
			let slot_lines = split_lines_owned(&slot_text);
			let target_last_row = row_idx.saturating_add(slot_lines.len().saturating_sub(1));
			super::ensure_rope_editable_rows(&mut buffer.text, target_last_row);

			for (offset, slot_line) in slot_lines.iter().enumerate() {
				let target_row = row_idx.saturating_add(offset);
				super::pad_rope_line_to_char_len(&mut buffer.text, target_row, insert_char_idx);
				let insert_at = rope_cursor_char(&buffer.text, target_row, insert_char_idx)
					.expect("target cursor must exist while blockwise pasting");
				buffer.text.insert(insert_at, slot_line.as_str());
			}

			window.cursor.row = row_idx.saturating_add(1) as u16;
			window.cursor.col = window
				.cursor
				.col
				.saturating_add(slot_lines.first().map(|line| line.chars().count()).unwrap_or(0) as u16);
			self.mark_active_buffer_dirty();
			self.align_active_window_scroll_to_cursor();
			self.workbench.status_bar.message = "pasted".to_string();
			return;
		}

		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		let char_count = rope_editable_line_len_chars(&buffer.text, row_idx)
			.expect("active line length must exist while pasting");
		let insert_char_idx = col_idx.saturating_add(1).min(char_count);
		let insert_at = rope_cursor_char(&buffer.text, row_idx, insert_char_idx)
			.expect("active cursor must exist while pasting");
		buffer.text.insert(insert_at, slot_text.as_str());
		window.cursor.col = window.cursor.col.saturating_add(slot_text.chars().count() as u16);
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
		self.workbench.status_bar.message = "pasted".to_string();
	}

	pub fn delete_current_line_to_slot(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			self.workbench.status_bar.message = "line delete failed: no active buffer".to_string();
			return;
		};

		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		if row_idx >= rope_editable_line_count(&buffer.text) {
			self.workbench.status_bar.message = "line delete failed: out of range".to_string();
			return;
		}

		let Some(line_range) = rope_line_char_range_without_newline(&buffer.text, row_idx) else {
			self.workbench.status_bar.message = "line delete failed: out of range".to_string();
			return;
		};
		let deleted = buffer.text.slice(line_range.clone()).to_string();
		let line_count = rope_editable_line_count(&buffer.text);
		let is_trailing_blank_line =
			rope_ends_with_newline(&buffer.text) && row_idx + 1 == line_count && line_range.is_empty();
		let delete_range = if line_count == 1 {
			0..buffer.text.len_chars()
		} else if row_idx + 1 < line_count {
			let next_start = rope_line_start_char(&buffer.text, row_idx.saturating_add(1))
				.expect("next line start must exist while deleting middle line");
			line_range.start..next_start
		} else if is_trailing_blank_line {
			line_range.start.saturating_sub(1)..buffer.text.len_chars()
		} else if rope_ends_with_newline(&buffer.text) {
			line_range.start..buffer.text.len_chars()
		} else {
			line_range.start.saturating_sub(1)..buffer.text.len_chars()
		};
		buffer.text.remove(delete_range);
		let visible_rows = rope_line_count(&buffer.text);
		let new_row = row_idx.min(visible_rows.saturating_sub(1)).saturating_add(1) as u16;
		window.cursor.row = new_row;
		window.cursor.col = 1;
		self.mark_active_buffer_dirty();
		self.line_slot = Some(deleted);
		self.line_slot_line_wise = true;
		self.line_slot_block_wise = false;
		self.align_active_window_scroll_to_cursor();
		self.workbench.status_bar.message = "line deleted".to_string();
	}
}
