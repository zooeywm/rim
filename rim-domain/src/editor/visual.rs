use crate::{display_geometry::char_display_width, edit::{block_col_for_display_target, clamp_cursor_col_for_line, cursor_slot_display_col, expand_tab_padding_at_display_target, pad_rope_line_to_char_len, previous_char_display_width, rope_block_char_range, rope_cursor_char, rope_editable_line_count, rope_editable_line_len_chars, rope_join_rows_without_newline, rope_linewise_char_range, rope_linewise_insertion_text, split_lines_owned}, editor::{EditorOperationError, EditorState}, model::{BufferState, CursorState, EditorMode, PendingBlockInsert, WindowState}, text::{rope_line_count, rope_line_len_chars, rope_line_without_newline}};

fn active_buffer_and_window_mut(state: &mut EditorState) -> Option<(&mut BufferState, &mut WindowState)> {
	let buffer_id = state.active_buffer_id()?;
	let window_id = state.active_window_id();
	let (buffers, windows) = (&mut state.buffers, &mut state.windows);
	let buffer = buffers.get_mut(buffer_id)?;
	let window = windows.get_mut(window_id)?;
	Some((buffer, window))
}

impl EditorState {
	pub fn begin_visual_block_insert(
		&mut self,
		append: bool,
	) -> Result<PendingBlockInsert, EditorOperationError> {
		let Some((start, end)) = self.normalized_visual_bounds() else {
			return Err(EditorOperationError::NoAnchor);
		};
		let Some(text) = self.active_buffer_rope() else {
			return Err(EditorOperationError::NoActiveBuffer);
		};
		let (left_display, right_display) = self.current_visual_block_display_bounds(text, start, end);
		let target_display = if append { right_display } else { left_display };
		let insert_col = block_col_for_display_target(text, start.row, target_display);

		let Some((_buffer, window)) = active_buffer_and_window_mut(self) else {
			return Err(EditorOperationError::NoActiveBuffer);
		};
		window.cursor.row = start.row;
		window.cursor.col = insert_col;

		if append {
			self.pad_visual_block_append_rows(start.row, end.row, target_display);
		}

		self.preferred_col = None;
		Ok(PendingBlockInsert {
			start_row:          start.row,
			end_row:            end.row,
			base_display_col:   target_display,
			cursor_display_col: target_display,
		})
	}

	pub fn insert_char_at_block_cursor(&mut self, ch: char) -> bool {
		let Some(mut block_insert) = self.pending_block_insert else {
			return false;
		};
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return false;
		};
		let insert_display_col = block_insert.cursor_display_col;
		let inserted = ch.to_string();

		for row_idx in
			block_insert.start_row.saturating_sub(1) as usize..=block_insert.end_row.saturating_sub(1) as usize
		{
			let row = row_idx.saturating_add(1) as u16;
			expand_tab_padding_at_display_target(&mut buffer.text, row, insert_display_col);
			let row_insert_col = block_col_for_display_target(&buffer.text, row, insert_display_col);
			let insert_col_idx = row_insert_col.saturating_sub(1) as usize;
			pad_rope_line_to_char_len(&mut buffer.text, row_idx, insert_col_idx);
			let insert_at = rope_cursor_char(&buffer.text, row_idx, insert_col_idx)
				.expect("block insert cursor must remain addressable");
			buffer.text.insert(insert_at, inserted.as_str());
		}

		block_insert.cursor_display_col =
			block_insert.cursor_display_col.saturating_add(char_display_width(ch).max(1) as u16);
		window.cursor.row = block_insert.start_row;
		window.cursor.col =
			block_col_for_display_target(&buffer.text, block_insert.start_row, block_insert.cursor_display_col);
		self.pending_block_insert = Some(block_insert);
		self.mark_active_buffer_dirty();
		true
	}

	pub fn backspace_at_block_cursor(&mut self) -> bool {
		let Some(mut block_insert) = self.pending_block_insert else {
			return false;
		};
		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return false;
		};
		let current_display_col = block_insert.cursor_display_col;
		let base_display_col = block_insert.base_display_col;
		if current_display_col <= base_display_col {
			return false;
		}
		let delete_width = previous_char_display_width(&buffer.text, block_insert.start_row, window.cursor.col);
		let delete_start_display_col = current_display_col.saturating_sub(delete_width);

		for row_idx in
			block_insert.start_row.saturating_sub(1) as usize..=block_insert.end_row.saturating_sub(1) as usize
		{
			let row = row_idx.saturating_add(1) as u16;
			let delete_start_col = block_col_for_display_target(&buffer.text, row, delete_start_display_col);
			let delete_end_col = block_col_for_display_target(&buffer.text, row, current_display_col);
			let delete_col = delete_start_col.saturating_sub(1) as usize;
			let delete_end_col_idx = delete_end_col.saturating_sub(1) as usize;
			let Some(line_len) = rope_editable_line_len_chars(&buffer.text, row_idx) else {
				continue;
			};
			if delete_col >= line_len || delete_end_col_idx <= delete_col {
				continue;
			}
			let delete_start =
				rope_cursor_char(&buffer.text, row_idx, delete_col).expect("block backspace start must exist");
			let delete_end =
				rope_cursor_char(&buffer.text, row_idx, delete_end_col_idx).expect("block backspace end must exist");
			buffer.text.remove(delete_start..delete_end);
		}

		block_insert.cursor_display_col = delete_start_display_col;
		window.cursor.row = block_insert.start_row;
		window.cursor.col =
			block_col_for_display_target(&buffer.text, block_insert.start_row, delete_start_display_col);
		self.pending_block_insert = Some(block_insert);
		self.mark_active_buffer_dirty();
		true
	}

	pub fn delete_visual_selection_to_slot(&mut self) -> Result<(), EditorOperationError> {
		let line_wise = self.mode == EditorMode::VisualLine;
		let block_wise = self.mode == EditorMode::VisualBlock;
		let Some((start, end)) = self.normalized_visual_bounds() else {
			return Err(EditorOperationError::NoAnchor);
		};

		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return Err(EditorOperationError::NoActiveBuffer);
		};

		let start_row = start.row.saturating_sub(1) as usize;
		let end_row = end.row.saturating_sub(1) as usize;
		let editable_line_count = rope_editable_line_count(&buffer.text);
		if start_row >= editable_line_count || end_row >= editable_line_count {
			return Err(EditorOperationError::OutOfRange);
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
				return Err(EditorOperationError::EmptySelection);
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
			return Ok(());
		}

		if line_wise {
			let Some(deleted) = rope_join_rows_without_newline(&buffer.text, start_row, end_row) else {
				return Err(EditorOperationError::OutOfRange);
			};
			let Some(delete_range) = rope_linewise_char_range(&buffer.text, start_row, end_row) else {
				return Err(EditorOperationError::OutOfRange);
			};
			buffer.text.remove(delete_range);
			let visible_rows = rope_line_count(&buffer.text);
			let new_row = start_row.min(visible_rows.saturating_sub(1)).saturating_add(1) as u16;
			window.cursor.row = new_row;
			window.cursor.col = 1;
			self.line_slot = Some(deleted);
			self.line_slot_line_wise = true;
			self.line_slot_block_wise = false;
			self.mark_active_buffer_dirty();
			return Ok(());
		}

		let start_line_len = rope_editable_line_len_chars(&buffer.text, start_row).unwrap_or(0) as u16;
		let end_line_len = rope_editable_line_len_chars(&buffer.text, end_row).unwrap_or(0) as u16;
		if start_line_len == 0 && end_line_len == 0 {
			return Err(EditorOperationError::EmptySelection);
		}

		let start_col = start.col.max(1).min(start_line_len.max(1));
		let end_col = end.col.max(1).min(end_line_len.max(1));

		let Some(delete_start) = rope_cursor_char(&buffer.text, start_row, start_col.saturating_sub(1) as usize)
		else {
			return Err(EditorOperationError::OutOfRange);
		};
		let Some(delete_end) = rope_cursor_char(&buffer.text, end_row, end_col as usize) else {
			return Err(EditorOperationError::OutOfRange);
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
		Ok(())
	}

	pub fn pending_block_insert_from_visual_selection(&self) -> Option<PendingBlockInsert> {
		(self.mode == EditorMode::VisualBlock)
			.then(|| {
				let (start, end) = self.normalized_visual_bounds()?;
				let text = self.active_buffer_rope()?;
				let base_display_col = self
					.visual_block_anchor_display_col
					.unwrap_or_else(|| cursor_slot_display_col(text, start.row, start.col));
				Some(PendingBlockInsert {
					start_row: start.row,
					end_row: end.row,
					base_display_col,
					cursor_display_col: base_display_col,
				})
			})
			.flatten()
	}

	pub fn yank_visual_selection_to_slot(&mut self) -> Result<(), EditorOperationError> {
		let line_wise = self.mode == EditorMode::VisualLine;
		let block_wise = self.mode == EditorMode::VisualBlock;
		let Some((start, end)) = self.normalized_visual_bounds() else {
			return Err(EditorOperationError::NoAnchor);
		};

		let Some(text) = self.active_buffer_rope() else {
			return Err(EditorOperationError::NoActiveBuffer);
		};
		let start_row = start.row.saturating_sub(1) as usize;
		let end_row = end.row.saturating_sub(1) as usize;
		if start_row >= rope_editable_line_count(text) || end_row >= rope_editable_line_count(text) {
			return Err(EditorOperationError::OutOfRange);
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
				return Err(EditorOperationError::EmptySelection);
			}
			self.line_slot = Some(yanked_parts.join("\n"));
			self.line_slot_line_wise = false;
			self.line_slot_block_wise = true;
			return Ok(());
		}
		if line_wise {
			self.line_slot = rope_join_rows_without_newline(text, start_row, end_row);
			self.line_slot_line_wise = true;
			self.line_slot_block_wise = false;
			return Ok(());
		}

		let start_line_len = rope_editable_line_len_chars(text, start_row).unwrap_or(0) as u16;
		let end_line_len = rope_editable_line_len_chars(text, end_row).unwrap_or(0) as u16;
		let start_col = start.col.max(1).min(start_line_len.max(1));
		let end_col = end.col.max(1).min(end_line_len.max(1));

		let Some(yank_start) = rope_cursor_char(text, start_row, start_col.saturating_sub(1) as usize) else {
			return Err(EditorOperationError::OutOfRange);
		};
		let Some(yank_end) = rope_cursor_char(text, end_row, end_col as usize) else {
			return Err(EditorOperationError::OutOfRange);
		};
		let yanked = text.slice(yank_start..yank_end).to_string();

		self.line_slot = Some(yanked);
		self.line_slot_line_wise = false;
		self.line_slot_block_wise = false;
		Ok(())
	}

	pub fn replace_visual_selection_with_slot(&mut self) -> Result<(), EditorOperationError> {
		let line_wise = self.mode == EditorMode::VisualLine;
		let block_wise = self.mode == EditorMode::VisualBlock;
		let Some(slot_text) = self.line_slot.clone() else {
			return Err(EditorOperationError::SlotEmpty);
		};
		let slot_block_wise = self.line_slot_block_wise;
		let Some((start, end)) = self.normalized_visual_bounds() else {
			return Err(EditorOperationError::NoAnchor);
		};

		let Some((buffer, window)) = active_buffer_and_window_mut(self) else {
			return Err(EditorOperationError::NoActiveBuffer);
		};

		let start_row = start.row.saturating_sub(1) as usize;
		let end_row = end.row.saturating_sub(1) as usize;
		let editable_line_count = rope_editable_line_count(&buffer.text);
		if start_row >= editable_line_count || end_row >= editable_line_count {
			return Err(EditorOperationError::OutOfRange);
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
			return Ok(());
		}

		if line_wise {
			let Some(replace_range) = rope_linewise_char_range(&buffer.text, start_row, end_row) else {
				return Err(EditorOperationError::OutOfRange);
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
			return Ok(());
		}

		let start_line_len = rope_editable_line_len_chars(&buffer.text, start_row).unwrap_or(0) as u16;
		let end_line_len = rope_editable_line_len_chars(&buffer.text, end_row).unwrap_or(0) as u16;
		let start_col = start.col.max(1).min(start_line_len.max(1));
		let end_col = end.col.max(1).min(end_line_len.max(1));

		let Some(replace_start) = rope_cursor_char(&buffer.text, start_row, start_col.saturating_sub(1) as usize)
		else {
			return Err(EditorOperationError::OutOfRange);
		};
		let Some(replace_end) = rope_cursor_char(&buffer.text, end_row, end_col as usize) else {
			return Err(EditorOperationError::OutOfRange);
		};
		buffer.text.remove(replace_start..replace_end);
		if !slot_text.is_empty() {
			buffer.text.insert(replace_start, slot_text.as_str());
		}
		window.cursor.row = start_row.saturating_add(1) as u16;
		window.cursor.col = start_col;
		self.mark_active_buffer_dirty();
		Ok(())
	}

	pub fn current_visual_block_display_bounds(
		&self,
		text: &ropey::Rope,
		start: CursorState,
		end: CursorState,
	) -> (u16, u16) {
		let anchor_display = self
			.visual_block_anchor_display_col
			.unwrap_or_else(|| cursor_slot_display_col(text, start.row, start.col));
		let cursor_display =
			self.visual_block_cursor_display_col.unwrap_or_else(|| cursor_slot_display_col(text, end.row, end.col));
		let left = anchor_display.min(cursor_display);
		let right =
			anchor_display.saturating_add(1).max(cursor_display.saturating_add(1)).max(left.saturating_add(1));
		(left, right)
	}

	pub fn normalized_visual_bounds(&self) -> Option<(CursorState, CursorState)> {
		let anchor = self.visual_anchor?;
		let cursor = self.active_cursor();
		let (mut start, mut end) = if self.mode == EditorMode::VisualBlock {
			(CursorState { row: anchor.row.min(cursor.row), col: anchor.col.min(cursor.col) }, CursorState {
				row: anchor.row.max(cursor.row),
				col: anchor.col.max(cursor.col),
			})
		} else if (anchor.row, anchor.col) <= (cursor.row, cursor.col) {
			(anchor, cursor)
		} else {
			(cursor, anchor)
		};

		if self.mode == EditorMode::VisualLine {
			start.col = 1;
			end.col = self.max_col_for_row(end.row).saturating_sub(1).max(1);
		}
		Some((start, end))
	}

	fn pad_visual_block_append_rows(&mut self, start_row: u16, end_row: u16, target_display: u16) {
		let Some((buffer, _window)) = active_buffer_and_window_mut(self) else {
			return;
		};
		let mut padded_any = false;

		for row_idx in start_row.saturating_sub(1) as usize..=end_row.saturating_sub(1) as usize {
			let row = row_idx.saturating_add(1) as u16;
			let insert_col = block_col_for_display_target(&buffer.text, row, target_display);
			let target_len = insert_col.saturating_sub(1) as usize;
			let line_len = rope_editable_line_len_chars(&buffer.text, row_idx).unwrap_or(0);
			if line_len >= target_len {
				continue;
			}

			pad_rope_line_to_char_len(&mut buffer.text, row_idx, target_len);
			padded_any = true;
		}

		if padded_any {
			self.mark_active_buffer_dirty();
		}
	}
}
