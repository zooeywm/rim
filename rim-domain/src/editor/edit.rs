use crate::{edit::{ensure_rope_editable_rows, pad_rope_line_to_char_len, rope_cursor_char, rope_editable_line_count, rope_editable_line_len_chars, rope_line_char_end_without_newline, rope_line_char_range_without_newline, rope_line_start_char, split_lines_owned}, editor::{EditorOperationError, EditorState}, model::{BufferState, WindowState}, text::{apply_text_delta_redo, apply_text_delta_undo, rope_ends_with_newline, rope_line_count, rope_line_len_chars}};

fn active_buffer_and_window_mut(state: &mut EditorState) -> Option<(&mut BufferState, &mut WindowState)> {
	let buffer_id = state.active_buffer_id()?;
	let window_id = state.active_window_id();
	let (buffers, windows) = (&mut state.buffers, &mut state.windows);
	let buffer = buffers.get_mut(buffer_id)?;
	let window = windows.get_mut(window_id)?;
	Some((buffer, window))
}

impl EditorState {
	pub fn insert_char_at_cursor(&mut self, ch: char) -> bool {
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return false;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		let Some(insert_at) = rope_cursor_char(&buffer.text, row_idx, col_idx) else {
			return false;
		};
		let mut encoded = [0; 4];
		buffer.text.insert(insert_at, ch.encode_utf8(&mut encoded));
		window.cursor.col = window.cursor.col.saturating_add(1);
		self.mark_active_buffer_dirty();
		true
	}

	pub fn insert_newline_at_cursor(&mut self) -> bool {
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return false;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		let Some(insert_at) = rope_cursor_char(&buffer.text, row_idx, col_idx) else {
			return false;
		};
		buffer.text.insert(insert_at, "\n");
		window.cursor.row = window.cursor.row.saturating_add(1);
		window.cursor.col = 1;
		self.mark_active_buffer_dirty();
		true
	}

	pub fn open_line_below_at_cursor(&mut self) -> bool {
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return false;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let Some(insert_at) = rope_line_char_end_without_newline(&buffer.text, row_idx) else {
			return false;
		};
		buffer.text.insert(insert_at, "\n");
		window.cursor.row = window.cursor.row.saturating_add(1);
		window.cursor.col = 1;
		self.mark_active_buffer_dirty();
		true
	}

	pub fn open_line_above_at_cursor(&mut self) -> bool {
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return false;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let Some(insert_at) = rope_line_start_char(&buffer.text, row_idx) else {
			return false;
		};
		buffer.text.insert(insert_at, "\n");
		window.cursor.col = 1;
		self.mark_active_buffer_dirty();
		true
	}

	pub fn join_line_below_at_cursor(&mut self) -> bool {
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return false;
		};

		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		if row_idx + 1 >= rope_editable_line_count(&buffer.text) {
			return false;
		}

		let Some(current_range) = rope_line_char_range_without_newline(&buffer.text, row_idx) else {
			return false;
		};
		let Some(next_range) = rope_line_char_range_without_newline(&buffer.text, row_idx.saturating_add(1))
		else {
			return false;
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
		true
	}

	pub fn backspace_at_cursor(&mut self) -> bool {
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return false;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		if row_idx >= rope_editable_line_count(&buffer.text) {
			return false;
		}

		if col_idx > 0 {
			let Some(delete_end) = rope_cursor_char(&buffer.text, row_idx, col_idx) else {
				return false;
			};
			let delete_start = delete_end.saturating_sub(1);
			buffer.text.remove(delete_start..delete_end);
			window.cursor.col = window.cursor.col.saturating_sub(1);
		} else if row_idx > 0 {
			let Some(current_start) = rope_line_start_char(&buffer.text, row_idx) else {
				return false;
			};
			if current_start == 0 {
				return false;
			}
			let prev_char_len = rope_line_len_chars(&buffer.text, row_idx.saturating_sub(1)) as u16;
			buffer.text.remove(current_start.saturating_sub(1)..current_start);
			window.cursor.row = window.cursor.row.saturating_sub(1);
			window.cursor.col = prev_char_len.saturating_add(1);
		} else {
			return false;
		}
		self.mark_active_buffer_dirty();
		true
	}

	pub fn cut_current_char_to_slot(&mut self) -> Result<(), EditorOperationError> {
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return Err(EditorOperationError::NoActiveBuffer);
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		let Some(line_range) = rope_line_char_range_without_newline(&buffer.text, row_idx) else {
			return Err(EditorOperationError::OutOfRange);
		};
		let char_count = line_range.end.saturating_sub(line_range.start);
		if col_idx >= char_count {
			return Err(EditorOperationError::NoChar);
		}

		let start = line_range.start.saturating_add(col_idx);
		let end = start.saturating_add(1);
		let cut = buffer.text.slice(start..end).to_string();
		buffer.text.remove(start..end);
		self.mark_active_buffer_dirty();
		self.line_slot = Some(cut);
		self.line_slot_line_wise = false;
		self.line_slot_block_wise = false;
		Ok(())
	}

	pub fn paste_slot_at_cursor(&mut self) -> Result<(), EditorOperationError> {
		let Some(slot_text) = self.line_slot.clone() else {
			return Err(EditorOperationError::SlotEmpty);
		};
		let line_wise_slot = self.line_slot_line_wise;
		let block_wise_slot = self.line_slot_block_wise;
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return Err(EditorOperationError::NoActiveBuffer);
		};

		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		if row_idx >= rope_editable_line_count(&buffer.text) {
			return Err(EditorOperationError::OutOfRange);
		}
		if line_wise_slot {
			let insert_row = row_idx.saturating_add(1).min(rope_editable_line_count(&buffer.text));
			let inserted_count = split_lines_owned(&slot_text).len() as u16;
			if insert_row < rope_editable_line_count(&buffer.text) {
				let insert_at = rope_line_start_char(&buffer.text, insert_row)
					.expect("target line start must exist while linewise pasting");
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
			return Ok(());
		}

		if block_wise_slot {
			let insert_char_idx = window.cursor.col as usize;
			let slot_lines = split_lines_owned(&slot_text);
			let target_last_row = row_idx.saturating_add(slot_lines.len().saturating_sub(1));
			ensure_rope_editable_rows(&mut buffer.text, target_last_row);

			for (offset, slot_line) in slot_lines.iter().enumerate() {
				let target_row = row_idx.saturating_add(offset);
				pad_rope_line_to_char_len(&mut buffer.text, target_row, insert_char_idx);
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
			return Ok(());
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
		Ok(())
	}

	pub fn delete_current_line_to_slot(&mut self) -> Result<(), EditorOperationError> {
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return Err(EditorOperationError::NoActiveBuffer);
		};

		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		if row_idx >= rope_editable_line_count(&buffer.text) {
			return Err(EditorOperationError::OutOfRange);
		}

		let Some(line_range) = rope_line_char_range_without_newline(&buffer.text, row_idx) else {
			return Err(EditorOperationError::OutOfRange);
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
		Ok(())
	}

	pub fn undo_active_buffer_edit(&mut self) -> Result<(), EditorOperationError> {
		let Some(buffer_id) = self.active_buffer_id() else {
			return Err(EditorOperationError::NoActiveBuffer);
		};
		let active_window_id = self.active_window_id();
		let before_cursor = {
			let Some(buffer) = self.buffers.get_mut(buffer_id) else {
				return Err(EditorOperationError::ActiveBufferMissing);
			};
			let Some(previous_entry) = buffer.undo_stack.pop() else {
				return Err(EditorOperationError::NothingToUndo);
			};
			let before_cursor = previous_entry.before_cursor;
			for edit in previous_entry.edits.iter().rev() {
				apply_text_delta_undo(&mut buffer.text, edit);
			}
			buffer.redo_stack.push(previous_entry);
			if buffer.redo_stack.len() > Self::MAX_HISTORY_ENTRIES {
				buffer.redo_stack.remove(0);
			}
			buffer.dirty = buffer.text != buffer.clean_text;
			before_cursor
		};
		if let Some(window) = self.windows.get_mut(active_window_id) {
			window.cursor = before_cursor;
		}
		self.sync_window_view_binding(active_window_id);
		Ok(())
	}

	pub fn redo_active_buffer_edit(&mut self) -> Result<(), EditorOperationError> {
		let Some(buffer_id) = self.active_buffer_id() else {
			return Err(EditorOperationError::NoActiveBuffer);
		};
		let active_window_id = self.active_window_id();
		let after_cursor = {
			let Some(buffer) = self.buffers.get_mut(buffer_id) else {
				return Err(EditorOperationError::ActiveBufferMissing);
			};
			let Some(next_entry) = buffer.redo_stack.pop() else {
				return Err(EditorOperationError::NothingToRedo);
			};
			let after_cursor = next_entry.after_cursor;
			for edit in &next_entry.edits {
				apply_text_delta_redo(&mut buffer.text, edit);
			}
			buffer.undo_stack.push(next_entry);
			if buffer.undo_stack.len() > Self::MAX_HISTORY_ENTRIES {
				buffer.undo_stack.remove(0);
			}
			buffer.dirty = buffer.text != buffer.clean_text;
			after_cursor
		};
		if let Some(window) = self.windows.get_mut(active_window_id) {
			window.cursor = after_cursor;
		}
		self.sync_window_view_binding(active_window_id);
		Ok(())
	}
}
