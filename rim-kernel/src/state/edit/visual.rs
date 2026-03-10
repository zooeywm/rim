use super::{
	CursorState, PendingBlockInsert, RimState, clamp_cursor_col_for_line, pad_rope_line_to_char_len,
	rope_block_char_range, rope_cursor_char, rope_editable_line_count, rope_editable_line_len_chars,
	rope_join_rows_without_newline, rope_line_len_chars, rope_line_without_newline, rope_linewise_char_range,
	rope_linewise_insertion_text, split_lines_owned,
};

impl RimState {
	pub fn begin_visual_block_insert(&mut self, append: bool) {
		let Some((start, end)) = self.normalized_visual_bounds() else {
			self.status_bar.message = "block insert failed: no anchor".to_string();
			self.exit_visual_mode();
			return;
		};
		let insert_col = if append { end.col.saturating_add(1) } else { start.col };

		let Some((_buffer, window)) = self.active_buffer_and_window_mut() else {
			self.status_bar.message = "block insert failed: no active buffer".to_string();
			self.exit_visual_mode();
			return;
		};
		window.cursor.row = start.row;
		window.cursor.col = insert_col;

		// Keep a stable rectangle so every insert-mode edit can be mirrored across it.
		self.enter_block_insert_mode(PendingBlockInsert {
			start_row: start.row,
			end_row: end.row,
			base_col: insert_col,
		});
		self.preferred_col = None;
		self.align_active_window_scroll_to_cursor();
	}

	pub fn insert_char_at_block_cursor(&mut self, ch: char) {
		let Some(block_insert) = self.pending_block_insert else {
			self.insert_char_at_cursor(ch);
			return;
		};
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			return;
		};
		let insert_col = window.cursor.col.saturating_sub(1) as usize;
		let inserted = ch.to_string();

		for row_idx in
			block_insert.start_row.saturating_sub(1) as usize..=block_insert.end_row.saturating_sub(1) as usize
		{
			pad_rope_line_to_char_len(&mut buffer.text, row_idx, insert_col);
			let insert_at = rope_cursor_char(&buffer.text, row_idx, insert_col)
				.expect("block insert cursor must remain addressable");
			buffer.text.insert(insert_at, inserted.as_str());
		}

		window.cursor.row = block_insert.start_row;
		window.cursor.col = window.cursor.col.saturating_add(1);
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
	}

	pub fn backspace_at_block_cursor(&mut self) {
		let Some(block_insert) = self.pending_block_insert else {
			self.backspace_at_cursor();
			return;
		};
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			return;
		};
		if window.cursor.col <= block_insert.base_col {
			return;
		}

		let delete_col = window.cursor.col.saturating_sub(2) as usize;
		for row_idx in
			block_insert.start_row.saturating_sub(1) as usize..=block_insert.end_row.saturating_sub(1) as usize
		{
			let Some(line_len) = rope_editable_line_len_chars(&buffer.text, row_idx) else {
				continue;
			};
			if delete_col >= line_len {
				continue;
			}
			let delete_start =
				rope_cursor_char(&buffer.text, row_idx, delete_col).expect("block backspace start must exist");
			let delete_end = rope_cursor_char(&buffer.text, row_idx, delete_col.saturating_add(1))
				.expect("block backspace end must exist");
			buffer.text.remove(delete_start..delete_end);
		}

		window.cursor.row = block_insert.start_row;
		window.cursor.col = window.cursor.col.saturating_sub(1);
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
	}

	pub fn delete_visual_selection_to_slot(&mut self) -> bool {
		let line_wise = self.is_visual_line_mode();
		let block_wise = self.is_visual_block_mode();
		let Some((start, end)) = self.normalized_visual_bounds() else {
			self.status_bar.message = "visual delete failed: no anchor".to_string();
			self.exit_visual_mode();
			return false;
		};

		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			self.status_bar.message = "visual delete failed: no active buffer".to_string();
			self.exit_visual_mode();
			return false;
		};

		let start_row = start.row.saturating_sub(1) as usize;
		let end_row = end.row.saturating_sub(1) as usize;
		let editable_line_count = rope_editable_line_count(&buffer.text);
		if start_row >= editable_line_count || end_row >= editable_line_count {
			self.status_bar.message = "visual delete failed: out of range".to_string();
			self.exit_visual_mode();
			return false;
		}

		if block_wise {
			let mut deleted_parts = Vec::new();
			let mut delete_ranges = Vec::new();
			let mut deleted_any = false;

			for row_idx in start_row..=end_row {
				let Some(range) = rope_block_char_range(&buffer.text, row_idx, start.col, end.col) else {
					deleted_parts.push(String::new());
					continue;
				};
				deleted_parts.push(buffer.text.slice(range.clone()).to_string());
				delete_ranges.push(range);
				deleted_any = true;
			}

			if !deleted_any {
				self.status_bar.message = "visual delete failed: empty".to_string();
				self.exit_visual_mode();
				return false;
			}

			for range in delete_ranges.into_iter().rev() {
				buffer.text.remove(range);
			}

			window.cursor.row = start.row;
			let line = rope_line_without_newline(&buffer.text, start_row).unwrap_or_default();
			window.cursor.col = clamp_cursor_col_for_line(line.as_str(), start.col);
			self.mark_active_buffer_dirty();
			self.line_slot = Some(deleted_parts.join("\n"));
			self.line_slot_line_wise = false;
			self.line_slot_block_wise = true;
			self.align_active_window_scroll_to_cursor();
			self.exit_visual_mode();
			self.status_bar.message = "selection deleted".to_string();
			return true;
		}

		if line_wise {
			let Some(deleted) = rope_join_rows_without_newline(&buffer.text, start_row, end_row) else {
				self.status_bar.message = "visual delete failed: out of range".to_string();
				self.exit_visual_mode();
				return false;
			};
			let Some(delete_range) = rope_linewise_char_range(&buffer.text, start_row, end_row) else {
				self.status_bar.message = "visual delete failed: out of range".to_string();
				self.exit_visual_mode();
				return false;
			};
			buffer.text.remove(delete_range);
			let visible_rows = super::rope_line_count(&buffer.text);
			let new_row = start_row.min(visible_rows.saturating_sub(1)).saturating_add(1) as u16;
			window.cursor.row = new_row;
			window.cursor.col = 1;
			self.line_slot = Some(deleted);
			self.line_slot_line_wise = true;
			self.line_slot_block_wise = false;
			self.mark_active_buffer_dirty();
			self.align_active_window_scroll_to_cursor();
			self.exit_visual_mode();
			self.status_bar.message = "selection deleted".to_string();
			return true;
		}

		let start_line_len = rope_editable_line_len_chars(&buffer.text, start_row).unwrap_or(0) as u16;
		let end_line_len = rope_editable_line_len_chars(&buffer.text, end_row).unwrap_or(0) as u16;
		if start_line_len == 0 && end_line_len == 0 {
			self.status_bar.message = "visual delete failed: empty".to_string();
			self.exit_visual_mode();
			return false;
		}

		let start_col = start.col.max(1).min(start_line_len.max(1));
		let end_col = end.col.max(1).min(end_line_len.max(1));

		let Some(delete_start) = rope_cursor_char(&buffer.text, start_row, start_col.saturating_sub(1) as usize)
		else {
			self.status_bar.message = "visual delete failed: out of range".to_string();
			self.exit_visual_mode();
			return false;
		};
		let Some(delete_end) = rope_cursor_char(&buffer.text, end_row, end_col as usize) else {
			self.status_bar.message = "visual delete failed: out of range".to_string();
			self.exit_visual_mode();
			return false;
		};
		let deleted_text = buffer.text.slice(delete_start..delete_end).to_string();
		buffer.text.remove(delete_start..delete_end);
		window.cursor.row = start_row.saturating_add(1) as u16;
		let line_len = rope_line_len_chars(&buffer.text, start_row) as u16;
		window.cursor.col = start_col.min(line_len.saturating_add(1));
		self.mark_active_buffer_dirty();
		self.line_slot = Some(deleted_text);
		self.line_slot_line_wise = false;
		self.line_slot_block_wise = false;
		self.align_active_window_scroll_to_cursor();
		self.exit_visual_mode();
		self.status_bar.message = "selection deleted".to_string();
		true
	}

	pub fn change_visual_selection_to_insert_mode(&mut self) {
		let block_insert = self
			.is_visual_block_mode()
			.then(|| {
				let (start, end) = self.normalized_visual_bounds()?;
				Some(PendingBlockInsert { start_row: start.row, end_row: end.row, base_col: start.col })
			})
			.flatten();

		self.begin_insert_history_group();
		if !self.delete_visual_selection_to_slot() {
			self.cancel_insert_history_group();
			return;
		}

		if let Some(pending) = block_insert {
			if let Some(cursor) = self.active_buffer_cursor_mut() {
				cursor.row = pending.start_row;
				cursor.col = pending.base_col;
			}
			self.enter_block_insert_mode(pending);
			self.preferred_col = None;
			self.align_active_window_scroll_to_cursor();
		} else {
			self.enter_insert_mode();
		}
		self.status_bar.message = "selection changed".to_string();
	}

	pub fn yank_visual_selection_to_slot(&mut self) {
		let line_wise = self.is_visual_line_mode();
		let block_wise = self.is_visual_block_mode();
		let Some((start, end)) = self.normalized_visual_bounds() else {
			self.status_bar.message = "visual yank failed: no anchor".to_string();
			self.exit_visual_mode();
			return;
		};

		let Some(text) = self.active_buffer_rope() else {
			self.status_bar.message = "visual yank failed: no active buffer".to_string();
			self.exit_visual_mode();
			return;
		};
		let start_row = start.row.saturating_sub(1) as usize;
		let end_row = end.row.saturating_sub(1) as usize;
		if start_row >= rope_editable_line_count(text) || end_row >= rope_editable_line_count(text) {
			self.status_bar.message = "visual yank failed: out of range".to_string();
			self.exit_visual_mode();
			return;
		}
		if block_wise {
			let mut yanked_parts = Vec::new();
			let mut yanked_any = false;
			for row_idx in start_row..=end_row {
				let Some(range) = rope_block_char_range(text, row_idx, start.col, end.col) else {
					yanked_parts.push(String::new());
					continue;
				};
				yanked_parts.push(text.slice(range).to_string());
				yanked_any = true;
			}
			if !yanked_any {
				self.status_bar.message = "visual yank failed: empty".to_string();
				self.exit_visual_mode();
				return;
			}
			self.line_slot = Some(yanked_parts.join("\n"));
			self.line_slot_line_wise = false;
			self.line_slot_block_wise = true;
			self.exit_visual_mode();
			self.status_bar.message = "selection yanked".to_string();
			return;
		}
		if line_wise {
			self.line_slot = rope_join_rows_without_newline(text, start_row, end_row);
			self.line_slot_line_wise = true;
			self.line_slot_block_wise = false;
			self.exit_visual_mode();
			self.status_bar.message = "selection yanked".to_string();
			return;
		}

		let start_line_len = rope_editable_line_len_chars(text, start_row).unwrap_or(0) as u16;
		let end_line_len = rope_editable_line_len_chars(text, end_row).unwrap_or(0) as u16;
		let start_col = start.col.max(1).min(start_line_len.max(1));
		let end_col = end.col.max(1).min(end_line_len.max(1));

		let Some(yank_start) = rope_cursor_char(text, start_row, start_col.saturating_sub(1) as usize) else {
			self.status_bar.message = "visual yank failed: out of range".to_string();
			self.exit_visual_mode();
			return;
		};
		let Some(yank_end) = rope_cursor_char(text, end_row, end_col as usize) else {
			self.status_bar.message = "visual yank failed: out of range".to_string();
			self.exit_visual_mode();
			return;
		};
		let yanked = text.slice(yank_start..yank_end).to_string();

		self.line_slot = Some(yanked);
		self.line_slot_line_wise = false;
		self.line_slot_block_wise = false;
		self.exit_visual_mode();
		self.status_bar.message = "selection yanked".to_string();
	}

	pub fn replace_visual_selection_with_slot(&mut self) {
		let line_wise = self.is_visual_line_mode();
		let block_wise = self.is_visual_block_mode();
		let Some(slot_text) = self.line_slot.clone() else {
			self.status_bar.message = "paste failed: slot is empty".to_string();
			self.exit_visual_mode();
			return;
		};
		let slot_block_wise = self.line_slot_block_wise;
		let Some((start, end)) = self.normalized_visual_bounds() else {
			self.status_bar.message = "visual paste failed: no anchor".to_string();
			self.exit_visual_mode();
			return;
		};

		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			self.status_bar.message = "visual paste failed: no active buffer".to_string();
			self.exit_visual_mode();
			return;
		};

		let start_row = start.row.saturating_sub(1) as usize;
		let end_row = end.row.saturating_sub(1) as usize;
		let editable_line_count = rope_editable_line_count(&buffer.text);
		if start_row >= editable_line_count || end_row >= editable_line_count {
			self.status_bar.message = "visual paste failed: out of range".to_string();
			self.exit_visual_mode();
			return;
		}

		if block_wise {
			let block_lines = split_lines_owned(&slot_text);
			let mut replacements = Vec::new();
			for row_idx in start_row..=end_row {
				let slot_offset = row_idx.saturating_sub(start_row);
				let replacement = if slot_block_wise {
					block_lines.get(slot_offset).cloned().unwrap_or_default()
				} else {
					block_lines.first().cloned().unwrap_or_default()
				};
				let Some(range) = rope_block_char_range(&buffer.text, row_idx, start.col, end.col) else {
					continue;
				};
				replacements.push((range, replacement));
			}

			for (range, replacement) in replacements.into_iter().rev() {
				buffer.text.remove(range.clone());
				if !replacement.is_empty() {
					buffer.text.insert(range.start, replacement.as_str());
				}
			}

			window.cursor.row = start.row;
			let line = rope_line_without_newline(&buffer.text, start_row).unwrap_or_default();
			window.cursor.col = clamp_cursor_col_for_line(line.as_str(), start.col);
			self.mark_active_buffer_dirty();
			self.align_active_window_scroll_to_cursor();
			self.exit_visual_mode();
			self.status_bar.message = "selection replaced".to_string();
			return;
		}

		if line_wise {
			let Some(replace_range) = rope_linewise_char_range(&buffer.text, start_row, end_row) else {
				self.status_bar.message = "visual paste failed: out of range".to_string();
				self.exit_visual_mode();
				return;
			};
			let has_following_rows = end_row.saturating_add(1) < rope_editable_line_count(&buffer.text);
			let replacement = rope_linewise_insertion_text(slot_text.as_str(), has_following_rows);
			buffer.text.remove(replace_range.clone());
			if !replacement.is_empty() {
				buffer.text.insert(replace_range.start, replacement.as_str());
			}
			window.cursor.row = start_row.saturating_add(1) as u16;
			window.cursor.col = 1;
			self.mark_active_buffer_dirty();
			self.align_active_window_scroll_to_cursor();
			self.exit_visual_mode();
			self.status_bar.message = "selection replaced".to_string();
			return;
		}

		let start_line_len = rope_editable_line_len_chars(&buffer.text, start_row).unwrap_or(0) as u16;
		let end_line_len = rope_editable_line_len_chars(&buffer.text, end_row).unwrap_or(0) as u16;
		let start_col = start.col.max(1).min(start_line_len.max(1));
		let end_col = end.col.max(1).min(end_line_len.max(1));

		let Some(replace_start) = rope_cursor_char(&buffer.text, start_row, start_col.saturating_sub(1) as usize)
		else {
			self.status_bar.message = "visual paste failed: out of range".to_string();
			self.exit_visual_mode();
			return;
		};
		let Some(replace_end) = rope_cursor_char(&buffer.text, end_row, end_col as usize) else {
			self.status_bar.message = "visual paste failed: out of range".to_string();
			self.exit_visual_mode();
			return;
		};
		buffer.text.remove(replace_start..replace_end);
		if !slot_text.is_empty() {
			buffer.text.insert(replace_start, slot_text.as_str());
		}
		window.cursor.row = start_row.saturating_add(1) as u16;
		window.cursor.col = start_col;
		self.mark_active_buffer_dirty();
		self.align_active_window_scroll_to_cursor();
		self.exit_visual_mode();
		self.status_bar.message = "selection replaced".to_string();
	}

	fn normalized_visual_bounds(&self) -> Option<(CursorState, CursorState)> {
		let anchor = self.visual_anchor?;
		let cursor = self.active_cursor();
		let (mut start, mut end) = if self.is_visual_block_mode() {
			(
				CursorState { row: anchor.row.min(cursor.row), col: anchor.col.min(cursor.col) },
				CursorState { row: anchor.row.max(cursor.row), col: anchor.col.max(cursor.col) },
			)
		} else if (anchor.row, anchor.col) <= (cursor.row, cursor.col) {
			(anchor, cursor)
		} else {
			(cursor, anchor)
		};

		if self.is_visual_line_mode() {
			start.col = 1;
			end.col = self.max_col_for_row(end.row).saturating_sub(1).max(1);
		}
		Some((start, end))
	}
}
