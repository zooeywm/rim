use ropey::Rope;
use unicode_width::UnicodeWidthChar;

use super::{CursorState, PendingBlockInsert, RimState, WindowId, rope_ends_with_newline, rope_is_empty, rope_line_count, rope_line_len_chars, rope_line_without_newline};

const TAB_DISPLAY_WIDTH: usize = 4;

impl RimState {
	pub fn move_cursor_up(&mut self) {
		tracing::trace!("move up");
		let target_col = self.capture_preferred_col_for_vertical();
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.row > 1
		{
			cursor.row = cursor.row.saturating_sub(1);
		}
		let row = self.active_cursor().row;
		let max_col = self.max_navigable_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = target_col.min(max_col).max(1);
		}
		self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Up);
	}

	pub fn move_cursor_down(&mut self) {
		tracing::trace!("move down");
		let target_col = self.capture_preferred_col_for_vertical();
		let max_row = self.max_row();
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.row < max_row
		{
			cursor.row = cursor.row.saturating_add(1);
		}
		let row = self.active_cursor().row;
		let max_col = self.max_navigable_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = target_col.min(max_col).max(1);
		}
		self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Down);
	}

	pub fn move_cursor_left(&mut self) {
		tracing::trace!("move left");
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.col > 1
		{
			cursor.col = cursor.col.saturating_sub(1);
		}
		self.preferred_col = None;
		self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
	}

	pub fn move_cursor_right(&mut self) {
		tracing::trace!("move right");
		let row = self.active_cursor().row;
		let max_col = self.max_navigable_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.col < max_col
		{
			cursor.col = cursor.col.saturating_add(1);
		}
		self.preferred_col = None;
		self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
	}

	pub fn move_cursor_left_for_visual_char(&mut self) {
		if let Some(cursor_mut) = self.active_buffer_cursor_mut()
			&& cursor_mut.col > 1
		{
			cursor_mut.col = cursor_mut.col.saturating_sub(1);
		}
		self.preferred_col = None;
		self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
	}

	pub fn move_cursor_right_for_visual_char(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_visual_char_col_for_row(row);
		if let Some(cursor_mut) = self.active_buffer_cursor_mut()
			&& cursor_mut.col < max_col
		{
			cursor_mut.col = cursor_mut.col.saturating_add(1);
		}
		self.preferred_col = None;
		self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
	}

	pub fn move_cursor_line_start(&mut self) {
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = 1;
		}
		self.preferred_col = None;
		self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
	}

	pub fn move_cursor_line_end(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_navigable_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = max_col;
		}
		self.preferred_col = None;
		self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
	}

	pub fn move_cursor_file_start(&mut self) {
		let target_col = self.capture_preferred_col_for_vertical();
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.row = 1;
		}
		let max_col = self.max_navigable_col_for_row(1);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = target_col.min(max_col).max(1);
		}
		self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Up);
	}

	pub fn move_cursor_file_end(&mut self) {
		let target_col = self.capture_preferred_col_for_vertical();
		let max_row = self.max_row();
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.row = max_row;
		}
		let max_col = self.max_navigable_col_for_row(max_row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = target_col.min(max_col).max(1);
		}
		self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Down);
	}

	pub fn move_cursor_right_for_insert(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.col < max_col
		{
			cursor.col = cursor.col.saturating_add(1);
		}
		self.preferred_col = None;
		self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
	}

	pub(crate) fn clamp_cursor_to_navigable_col(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_navigable_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = cursor.col.min(max_col).max(1);
		}
		self.align_active_window_scroll_to_cursor();
	}

	pub fn scroll_view_down_one_line(&mut self) {
		let target_col = self.capture_preferred_col_for_vertical();
		self.scroll_view_with_col_memory(1, target_col);
	}

	pub fn scroll_view_up_one_line(&mut self) {
		let target_col = self.capture_preferred_col_for_vertical();
		self.scroll_view_with_col_memory(-1, target_col);
	}

	pub fn scroll_view_down_half_page(&mut self) {
		let target_col = self.capture_preferred_col_for_vertical();
		let step = self.active_window_visible_rows().saturating_div(2).max(1) as i16;
		self.scroll_view_with_col_memory(step, target_col);
	}

	pub fn scroll_view_up_half_page(&mut self) {
		let target_col = self.capture_preferred_col_for_vertical();
		let step = self.active_window_visible_rows().saturating_div(2).max(1) as i16;
		self.scroll_view_with_col_memory(-step, target_col);
	}

	fn scroll_view_with_col_memory(&mut self, delta: i16, target_col: u16) {
		let active_window_id = self.active_window_id();
		let visible_rows = self.active_window_visible_rows();
		let max_scroll = self.max_row().saturating_sub(visible_rows);
		if let Some(window) = self.windows.get_mut(active_window_id) {
			if delta >= 0 {
				window.scroll_y = window.scroll_y.saturating_add(delta as u16).min(max_scroll);
			} else {
				window.scroll_y = window.scroll_y.saturating_sub((-delta) as u16);
			}
		}
		self.keep_cursor_in_view_after_scroll(target_col);
	}

	pub fn active_cursor(&self) -> CursorState {
		self.windows.get(self.active_window_id()).map(|window| window.cursor).unwrap_or_default()
	}

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
			end_row:   end.row,
			base_col:  insert_col,
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

	pub fn cut_current_char_to_slot(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			self.status_bar.message = "cut failed: no active buffer".to_string();
			return;
		};
		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		let col_idx = window.cursor.col.saturating_sub(1) as usize;
		let Some(line_range) = rope_line_char_range_without_newline(&buffer.text, row_idx) else {
			self.status_bar.message = "cut failed: out of range".to_string();
			return;
		};
		let char_count = line_range.end.saturating_sub(line_range.start);
		if col_idx >= char_count {
			self.status_bar.message = "cut failed: no char".to_string();
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
		self.status_bar.message = "char cut".to_string();
	}

	pub fn paste_slot_at_cursor(&mut self) {
		let Some(slot_text) = self.line_slot.clone() else {
			self.status_bar.message = "paste failed: slot is empty".to_string();
			return;
		};
		let line_wise_slot = self.line_slot_line_wise;
		let block_wise_slot = self.line_slot_block_wise;
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			self.status_bar.message = "paste failed: no active buffer".to_string();
			return;
		};

		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		if row_idx >= rope_editable_line_count(&buffer.text) {
			self.status_bar.message = "paste failed: out of range".to_string();
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
			self.status_bar.message = "pasted".to_string();
			return;
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
			self.align_active_window_scroll_to_cursor();
			self.status_bar.message = "pasted".to_string();
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
		self.status_bar.message = "pasted".to_string();
	}

	pub fn delete_current_line_to_slot(&mut self) {
		let Some((buffer, window)) = self.active_buffer_and_window_mut() else {
			self.status_bar.message = "line delete failed: no active buffer".to_string();
			return;
		};

		let row_idx = window.cursor.row.saturating_sub(1) as usize;
		if row_idx >= rope_editable_line_count(&buffer.text) {
			self.status_bar.message = "line delete failed: out of range".to_string();
			return;
		}

		let Some(line_range) = rope_line_char_range_without_newline(&buffer.text, row_idx) else {
			self.status_bar.message = "line delete failed: out of range".to_string();
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
		self.status_bar.message = "line deleted".to_string();
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
			let visible_rows = rope_line_count(&buffer.text);
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

	fn capture_preferred_col_for_vertical(&mut self) -> u16 {
		if let Some(col) = self.preferred_col {
			return col;
		}
		let col = self.active_cursor().col;
		self.preferred_col = Some(col);
		col
	}

	fn max_row(&self) -> u16 { self.active_buffer_rope().map(|text| rope_line_count(text) as u16).unwrap_or(1) }

	fn max_col_for_row(&self, row: u16) -> u16 {
		let row_index = row.saturating_sub(1) as usize;
		let line_len =
			self.active_buffer_rope().map(|text| rope_line_len_chars(text, row_index) as u16).unwrap_or(0);
		line_len.saturating_add(1)
	}

	fn max_navigable_col_for_row(&self, row: u16) -> u16 { self.max_col_for_row(row).saturating_sub(1).max(1) }

	fn max_visual_char_col_for_row(&self, row: u16) -> u16 {
		let line_len = self.max_col_for_row(row).saturating_sub(1);
		if self.row_has_newline_char(row) { line_len.saturating_add(1).max(1) } else { line_len.max(1) }
	}

	fn row_has_newline_char(&self, row: u16) -> bool {
		let Some(text) = self.active_buffer_rope() else {
			return false;
		};
		if rope_is_empty(text) {
			return false;
		}
		let total_rows = rope_line_count(text) as u16;
		if row < total_rows {
			return true;
		}
		row == total_rows && rope_ends_with_newline(text)
	}

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

	fn normalized_visual_bounds(&self) -> Option<(CursorState, CursorState)> {
		let anchor = self.visual_anchor?;
		let cursor = self.active_cursor();
		let (mut start, mut end) = if self.is_visual_block_mode() {
			(CursorState { row: anchor.row.min(cursor.row), col: anchor.col.min(cursor.col) }, CursorState {
				row: anchor.row.max(cursor.row),
				col: anchor.col.max(cursor.col),
			})
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

	fn active_window_visible_rows(&self) -> u16 {
		let window_id = self.active_window_id();
		self
			.windows
			.get(window_id)
			.map(|window| {
				let reserved_for_split_line = u16::from(window.y > 0);
				window.height.saturating_sub(reserved_for_split_line).max(1)
			})
			.unwrap_or(1)
	}

	fn keep_cursor_in_view_after_scroll(&mut self, target_col: u16) {
		let active_window_id = self.active_window_id();
		let Some(window) = self.windows.get(active_window_id).cloned() else {
			return;
		};
		let visible_rows = self.active_window_visible_rows();
		let top_row = window.scroll_y.saturating_add(1);
		let bottom_row = top_row.saturating_add(visible_rows.saturating_sub(1));
		let row = self.active_cursor().row;

		let target_row = if row < top_row {
			top_row
		} else if row > bottom_row {
			bottom_row
		} else {
			return;
		};

		let max_col = self.max_navigable_col_for_row(target_row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.row = target_row;
			cursor.col = target_col.min(max_col).max(1);
		}
	}

	fn active_window_visible_text_cols(&self) -> u16 {
		let window_id = self.active_window_id();
		self
			.windows
			.get(window_id)
			.map(|window| {
				let reserved_for_split_line = u16::from(window.x > 0);
				let local_width = window.width.saturating_sub(reserved_for_split_line).max(1);
				let total_lines = self.active_buffer_rope().map(rope_line_count).unwrap_or(1);
				let desired_number_col_width = total_lines.to_string().len() as u16 + 1;
				let number_col_width =
					if local_width <= desired_number_col_width { 0 } else { desired_number_col_width };
				local_width.saturating_sub(number_col_width).max(1)
			})
			.unwrap_or(1)
	}

	fn adjust_scroll_after_vertical_move(&mut self, direction: VerticalMoveDirection) {
		let active_window_id = self.active_window_id();
		let cursor_row = self.active_cursor().row;
		let cursor_line = cursor_row.saturating_sub(1);
		let visible_rows = self.active_window_visible_rows();
		let max_row = self.max_row();
		let max_scroll = max_row.saturating_sub(visible_rows);
		let threshold = self.cursor_scroll_threshold;
		let visible_tail = visible_rows.saturating_sub(1);

		if let Some(window) = self.windows.get_mut(active_window_id) {
			match direction {
				VerticalMoveDirection::Up => {
					let top_trigger = window.scroll_y.saturating_add(threshold);
					if cursor_line < top_trigger {
						window.scroll_y = cursor_line.saturating_sub(threshold).min(max_scroll);
					}
				}
				VerticalMoveDirection::Down => {
					let bottom = window.scroll_y.saturating_add(visible_tail);
					let bottom_trigger = bottom.saturating_sub(threshold);
					if cursor_line > bottom_trigger {
						let needed_top = cursor_line.saturating_add(threshold).saturating_sub(visible_tail);
						window.scroll_y = needed_top.min(max_scroll);
					}
				}
			}
		}
	}

	fn adjust_scroll_after_horizontal_move(&mut self, direction: HorizontalMoveDirection) {
		let active_window_id = self.active_window_id();
		let visible_cols = self.active_window_visible_text_cols();
		let visible_tail = visible_cols.saturating_sub(1);
		let threshold = self.cursor_scroll_threshold.min(visible_tail);
		let cursor_display_col = self.active_cursor_display_col();
		let line_display_width = self.active_line_display_width();
		let max_scroll = line_display_width.saturating_sub(visible_tail);

		if let Some(window) = self.windows.get_mut(active_window_id) {
			match direction {
				HorizontalMoveDirection::Left => {
					let left_trigger = window.scroll_x.saturating_add(threshold);
					if cursor_display_col < left_trigger {
						window.scroll_x = cursor_display_col.saturating_sub(threshold).min(max_scroll);
					}
				}
				HorizontalMoveDirection::Right => {
					let right = window.scroll_x.saturating_add(visible_tail);
					let right_trigger = right.saturating_sub(threshold);
					if cursor_display_col > right_trigger {
						let needed_left = cursor_display_col.saturating_add(threshold).saturating_sub(visible_tail);
						window.scroll_x = needed_left.min(max_scroll);
					}
				}
			}
		}
	}

	pub(super) fn align_active_window_scroll_to_cursor(&mut self) {
		let active_window_id = self.active_window_id();
		self.center_window_on_cursor_if_hidden(active_window_id);
		let Some(window) = self.windows.get(active_window_id).cloned() else {
			return;
		};
		let cursor_line = self.active_cursor().row.saturating_sub(1);
		let visible_rows = self.active_window_visible_rows();
		let max_row = self.max_row();
		let max_scroll = max_row.saturating_sub(visible_rows);
		let threshold = self.cursor_scroll_threshold.min(visible_rows.saturating_sub(1));
		let visible_tail = visible_rows.saturating_sub(1);
		let top_trigger = window.scroll_y.saturating_add(threshold);
		let bottom = window.scroll_y.saturating_add(visible_tail);
		let bottom_trigger = bottom.saturating_sub(threshold);
		let visible_cols = self.active_window_visible_text_cols();
		let col_tail = visible_cols.saturating_sub(1);
		let col_threshold = self.cursor_scroll_threshold.min(col_tail);
		let cursor_display_col = self.active_cursor_display_col();
		let line_display_width = self.active_line_display_width();
		let max_scroll_x = line_display_width.saturating_sub(col_tail);
		let left_trigger = window.scroll_x.saturating_add(col_threshold);
		let right = window.scroll_x.saturating_add(col_tail);
		let right_trigger = right.saturating_sub(col_threshold);

		let mut next_scroll = window.scroll_y;
		if cursor_line < top_trigger {
			next_scroll = cursor_line.saturating_sub(threshold);
		} else if cursor_line > bottom_trigger {
			next_scroll = cursor_line.saturating_add(threshold).saturating_sub(visible_tail);
		}
		next_scroll = next_scroll.min(max_scroll);
		let mut next_scroll_x = window.scroll_x;
		if cursor_display_col < left_trigger {
			next_scroll_x = cursor_display_col.saturating_sub(col_threshold);
		} else if cursor_display_col > right_trigger {
			next_scroll_x = cursor_display_col.saturating_add(col_threshold).saturating_sub(col_tail);
		}
		next_scroll_x = next_scroll_x.min(max_scroll_x);

		if let Some(active_window) = self.windows.get_mut(active_window_id) {
			active_window.scroll_y = next_scroll;
			active_window.scroll_x = next_scroll_x;
		}
	}

	pub(crate) fn center_window_on_cursor_if_hidden(&mut self, window_id: WindowId) {
		let Some(window) = self.windows.get(window_id).cloned() else {
			return;
		};
		let Some(buffer_id) = window.buffer_id else {
			return;
		};
		let Some(buffer) = self.buffers.get(buffer_id) else {
			return;
		};
		let visible_rows = window_visible_rows(&window);
		let visible_cols = window_visible_text_cols(&window, &buffer.text);
		let cursor_line = window.cursor.row.saturating_sub(1);
		let top = window.scroll_y;
		let bottom = window.scroll_y.saturating_add(visible_rows.saturating_sub(1));
		let cursor_display_col = cursor_display_col_for_window(&buffer.text, window.cursor);
		let left = window.scroll_x;
		let right = window.scroll_x.saturating_add(visible_cols.saturating_sub(1));
		let row_hidden = cursor_line < top || cursor_line > bottom;
		let col_hidden = cursor_display_col < left || cursor_display_col > right;
		if !row_hidden && !col_hidden {
			return;
		}

		let max_scroll_y = (rope_line_count(&buffer.text) as u16).saturating_sub(visible_rows);
		let next_scroll_y = cursor_line.saturating_sub(visible_rows / 2).min(max_scroll_y);
		let line_display_width = line_display_width_for_window(&buffer.text, window.cursor);
		let max_scroll_x = line_display_width.saturating_sub(visible_cols.saturating_sub(1));
		let next_scroll_x = cursor_display_col.saturating_sub(visible_cols / 2).min(max_scroll_x);

		if let Some(target_window) = self.windows.get_mut(window_id) {
			target_window.scroll_y = next_scroll_y;
			target_window.scroll_x = next_scroll_x;
		}
	}

	fn active_cursor_display_col(&self) -> u16 {
		let cursor = self.active_cursor();
		let row_index = cursor.row.saturating_sub(1) as usize;
		let char_index = cursor.col.saturating_sub(1) as usize;
		self
			.active_buffer_rope()
			.and_then(|text| rope_line_without_newline(text, row_index))
			.map(|line| display_width_of_char_prefix(line.as_str(), char_index) as u16)
			.unwrap_or(0)
	}

	fn active_line_display_width(&self) -> u16 {
		let row_index = self.active_cursor().row.saturating_sub(1) as usize;
		self
			.active_buffer_rope()
			.and_then(|text| rope_line_without_newline(text, row_index))
			.map(|line| line.chars().map(|ch| char_display_width(ch) as u16).sum())
			.unwrap_or(0)
	}
}

fn window_visible_rows(window: &super::WindowState) -> u16 {
	let reserved_for_split_line = u16::from(window.y > 0);
	window.height.saturating_sub(reserved_for_split_line).max(1)
}

fn window_visible_text_cols(window: &super::WindowState, text: &Rope) -> u16 {
	let reserved_for_split_line = u16::from(window.x > 0);
	let local_width = window.width.saturating_sub(reserved_for_split_line).max(1);
	let total_lines = rope_line_count(text);
	let desired_number_col_width = total_lines.to_string().len() as u16 + 1;
	let number_col_width = if local_width <= desired_number_col_width { 0 } else { desired_number_col_width };
	local_width.saturating_sub(number_col_width).max(1)
}

fn cursor_display_col_for_window(text: &Rope, cursor: CursorState) -> u16 {
	let row_index = cursor.row.saturating_sub(1) as usize;
	let char_index = cursor.col.saturating_sub(1) as usize;
	rope_line_without_newline(text, row_index)
		.map(|line| display_width_of_char_prefix(line.as_str(), char_index) as u16)
		.unwrap_or(0)
}

fn line_display_width_for_window(text: &Rope, cursor: CursorState) -> u16 {
	let row_index = cursor.row.saturating_sub(1) as usize;
	rope_line_without_newline(text, row_index)
		.map(|line| line.chars().map(|ch| char_display_width(ch) as u16).sum())
		.unwrap_or(0)
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

fn display_width_of_char_prefix(line: &str, char_count: usize) -> usize {
	line.chars().take(char_count).map(char_display_width).sum()
}

fn char_display_width(ch: char) -> usize {
	if ch == '\t' { TAB_DISPLAY_WIDTH } else { UnicodeWidthChar::width(ch).unwrap_or(0) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerticalMoveDirection {
	Up,
	Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HorizontalMoveDirection {
	Left,
	Right,
}
