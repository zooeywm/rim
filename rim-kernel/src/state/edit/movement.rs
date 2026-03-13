use ropey::Rope;

use super::{CursorState, RimState, rope_ends_with_newline, rope_line_count, rope_line_len_chars, rope_line_without_newline};
use crate::{display_geometry::{char_display_width as geom_char_display_width, cursor_col_for_display_slot as geom_cursor_col_for_display_slot, display_col_of_cursor_slot as geom_display_col_of_cursor_slot, display_width_of_char_prefix_with_virtual as geom_display_width_of_char_prefix_with_virtual, navigable_col_for_display_target as geom_navigable_col_for_display_target, wrapped_row_index_for_cursor as geom_wrapped_row_index_for_cursor, wrapped_total_rows_for_rope as geom_wrapped_total_rows_for_rope}, state::{WindowId, rope_is_empty}};

impl RimState {
	pub fn move_cursor_up(&mut self) {
		tracing::trace!("move up");
		let target_display_col = self.target_display_col_for_vertical_move();
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.row > 1
		{
			cursor.row = cursor.row.saturating_sub(1);
		}
		let row = self.active_cursor().row;
		let target_col = if self.is_visual_block_mode() || self.is_block_insert_mode() {
			self.visual_block_col_for_display_target(row, target_display_col)
		} else {
			self.navigable_col_for_display_target(row, target_display_col)
		};
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = target_col;
		}
		self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Up);
	}

	pub fn move_cursor_down(&mut self) {
		tracing::trace!("move down");
		let target_display_col = self.target_display_col_for_vertical_move();
		let max_row = self.max_row();
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.row < max_row
		{
			cursor.row = cursor.row.saturating_add(1);
		}
		let row = self.active_cursor().row;
		let target_col = if self.is_visual_block_mode() || self.is_block_insert_mode() {
			self.visual_block_col_for_display_target(row, target_display_col)
		} else {
			self.navigable_col_for_display_target(row, target_display_col)
		};
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = target_col;
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
		if self.is_visual_block_mode() {
			let next_display_col = self
				.visual_block_cursor_display_col
				.unwrap_or_else(|| self.active_cursor_display_col())
				.saturating_sub(1);
			self.visual_block_cursor_display_col = Some(next_display_col);
			self.preferred_col = Some(next_display_col);
			let row = self.active_cursor().row;
			let target_col = self.visual_block_col_for_display_target(row, next_display_col);
			if let Some(cursor_mut) = self.active_buffer_cursor_mut() {
				cursor_mut.col = target_col;
			}
			self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
			return;
		}
		if let Some(cursor_mut) = self.active_buffer_cursor_mut()
			&& cursor_mut.col > 1
		{
			cursor_mut.col = cursor_mut.col.saturating_sub(1);
		}
		self.preferred_col = None;
		self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
	}

	pub fn move_cursor_right_for_visual_char(&mut self) {
		if self.is_visual_block_mode() {
			let next_display_col = self
				.visual_block_cursor_display_col
				.unwrap_or_else(|| self.active_cursor_display_col())
				.saturating_add(1);
			self.visual_block_cursor_display_col = Some(next_display_col);
			self.preferred_col = Some(next_display_col);
			let row = self.active_cursor().row;
			let target_col = self.visual_block_col_for_display_target(row, next_display_col);
			if let Some(cursor_mut) = self.active_buffer_cursor_mut() {
				cursor_mut.col = target_col;
			}
			self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
			return;
		}
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
		let target_display_col = self.capture_preferred_col_for_vertical();
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.row = 1;
		}
		let target_col = self.navigable_col_for_display_target(1, target_display_col);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = target_col;
		}
		self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Up);
	}

	pub fn move_cursor_file_end(&mut self) {
		let target_display_col = self.capture_preferred_col_for_vertical();
		let max_row = self.max_row();
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.row = max_row;
		}
		let target_col = self.navigable_col_for_display_target(max_row, target_display_col);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = target_col;
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

	pub fn move_cursor_to_insert_line_end_slot(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = max_col.max(1);
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
		let target_display_col = self.capture_preferred_col_for_vertical();
		self.scroll_view_with_col_memory(1, target_display_col);
	}

	pub fn scroll_view_up_one_line(&mut self) {
		let target_display_col = self.capture_preferred_col_for_vertical();
		self.scroll_view_with_col_memory(-1, target_display_col);
	}

	pub fn scroll_view_down_half_page(&mut self) {
		let target_display_col = self.capture_preferred_col_for_vertical();
		let step = self.active_window_visible_rows().saturating_div(2).max(1) as i16;
		self.move_cursor_and_scroll_half_page(step, target_display_col);
	}

	pub fn scroll_view_up_half_page(&mut self) {
		let target_display_col = self.capture_preferred_col_for_vertical();
		let step = self.active_window_visible_rows().saturating_div(2).max(1) as i16;
		self.move_cursor_and_scroll_half_page(-step, target_display_col);
	}

	fn scroll_view_with_col_memory(&mut self, delta: i16, target_display_col: u16) -> bool {
		let active_window_id = self.active_window_id();
		let visible_rows = self.active_window_visible_rows();
		let max_scroll = self.max_scroll_y_for_active_window(visible_rows);
		let mut changed = false;
		if let Some(window) = self.windows.get_mut(active_window_id) {
			let previous_scroll = window.scroll_y;
			if delta >= 0 {
				window.scroll_y = window.scroll_y.saturating_add(delta as u16).min(max_scroll);
			} else {
				window.scroll_y = window.scroll_y.saturating_sub((-delta) as u16);
			}
			changed = window.scroll_y != previous_scroll;
		}
		self.keep_cursor_in_view_after_scroll(target_display_col);
		changed
	}

	pub fn active_cursor(&self) -> CursorState {
		self.windows.get(self.active_window_id()).map(|window| window.cursor).unwrap_or_default()
	}

	fn target_display_col_for_vertical_move(&mut self) -> u16 {
		if self.is_visual_block_mode() {
			let col = self.visual_block_cursor_display_col.unwrap_or_else(|| self.active_cursor_display_col());
			self.preferred_col = Some(col);
			return col;
		}
		self.capture_preferred_col_for_vertical()
	}

	fn capture_preferred_col_for_vertical(&mut self) -> u16 {
		if let Some(col) = self.preferred_col {
			return col;
		}
		let col = self.active_cursor_display_col();
		self.preferred_col = Some(col);
		col
	}

	fn move_cursor_and_scroll_half_page(&mut self, delta: i16, target_display_col: u16) {
		let active_window_id = self.active_window_id();
		let Some(window_snapshot) = self.windows.get(active_window_id).copied() else {
			return;
		};
		let visible_rows = self.active_window_visible_rows();
		let max_row = self.max_row();
		let max_scroll = max_row.saturating_sub(visible_rows);
		let current_row = window_snapshot.cursor.row;
		let next_row = if delta >= 0 {
			current_row.saturating_add(delta as u16).min(max_row)
		} else {
			current_row.saturating_sub((-delta) as u16).max(1)
		};
		let relative_row = current_row.saturating_sub(window_snapshot.scroll_y.saturating_add(1));
		let next_cursor_line = next_row.saturating_sub(1);
		let next_scroll = next_cursor_line.saturating_sub(relative_row).min(max_scroll);
		let target_col = self.navigable_col_for_display_target(next_row, target_display_col);
		if let Some(window) = self.windows.get_mut(active_window_id) {
			window.cursor.row = next_row;
			window.cursor.col = target_col;
			window.scroll_y = next_scroll;
		}
	}

	fn max_row(&self) -> u16 { self.active_buffer_rope().map(|text| rope_line_count(text) as u16).unwrap_or(1) }

	pub(super) fn max_col_for_row(&self, row: u16) -> u16 {
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

	fn keep_cursor_in_view_after_scroll(&mut self, target_display_col: u16) {
		if self.word_wrap_enabled() {
			// Keep current behavior predictable in wrap mode: scroll follows viewport only.
			return;
		}
		let active_window_id = self.active_window_id();
		let Some(window) = self.windows.get(active_window_id).cloned() else {
			return;
		};
		let visible_rows = self.active_window_visible_rows();
		let threshold = self.cursor_scroll_threshold.min(visible_rows.saturating_sub(1));
		let top_row = window.scroll_y.saturating_add(1);
		let bottom_row = top_row.saturating_add(visible_rows.saturating_sub(1));
		let top_safe_row = top_row.saturating_add(threshold);
		let bottom_safe_row = bottom_row.saturating_sub(threshold);
		let row = self.active_cursor().row;

		let target_row = if row < top_safe_row {
			top_safe_row
		} else if row > bottom_safe_row {
			bottom_safe_row.max(top_safe_row)
		} else {
			return;
		};

		let target_col = self.navigable_col_for_display_target(target_row, target_display_col);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.row = target_row;
			cursor.col = target_col;
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
		if self.word_wrap_enabled() {
			self.adjust_scroll_after_move_wrapped(direction);
			return;
		}
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
		if self.word_wrap_enabled() {
			let _ = direction;
			self.align_active_window_scroll_to_cursor();
			if let Some(window) = self.windows.get_mut(self.active_window_id()) {
				window.scroll_x = 0;
			}
			return;
		}
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

	pub(in crate::state) fn align_active_window_scroll_to_cursor(&mut self) {
		if self.word_wrap_enabled() {
			self.align_active_window_scroll_to_cursor_wrapped();
			return;
		}
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
		if self.word_wrap_enabled() {
			self.center_window_on_cursor_if_hidden_wrapped(window_id);
			return;
		}
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

	pub(crate) fn active_cursor_display_col(&self) -> u16 {
		if self.is_visual_block_mode() {
			if let Some(col) = self.visual_block_cursor_display_col {
				return col;
			}
		}
		if let Some(block_insert) = self.pending_block_insert {
			return block_insert.cursor_display_col;
		}
		let cursor = self.active_cursor();
		let row_index = cursor.row.saturating_sub(1) as usize;
		let char_index = cursor.col.saturating_sub(1) as usize;
		self
			.active_buffer_rope()
			.and_then(|text| rope_line_without_newline(text, row_index))
			.map(|line| geom_display_width_of_char_prefix_with_virtual(line.as_str(), char_index) as u16)
			.unwrap_or(0)
	}

	fn active_line_display_width(&self) -> u16 {
		let cursor = self.active_cursor();
		let row_index = cursor.row.saturating_sub(1) as usize;
		let base_width = self
			.active_buffer_rope()
			.and_then(|text| rope_line_without_newline(text, row_index))
			.map(|line| line.chars().map(|ch| geom_char_display_width(ch) as u16).sum())
			.unwrap_or(0);

		if self.is_visual_block_mode() || self.is_block_insert_mode() {
			base_width.max(self.active_cursor_display_col())
		} else {
			base_width
		}
	}

	fn navigable_col_for_display_target(&self, row: u16, target_display_col: u16) -> u16 {
		let Some(text) = self.active_buffer_rope() else {
			return 1;
		};
		let col = geom_navigable_col_for_display_target(text, row, target_display_col);
		col.min(self.max_navigable_col_for_row(row)).max(1)
	}

	fn visual_block_col_for_display_target(&self, row: u16, target_display_col: u16) -> u16 {
		let row_index = row.saturating_sub(1) as usize;
		let Some(line) = self.active_buffer_rope().and_then(|text| rope_line_without_newline(text, row_index))
		else {
			return target_display_col.saturating_add(1).max(1);
		};
		geom_cursor_col_for_display_slot(line.as_str(), target_display_col)
	}

	fn max_scroll_y_for_active_window(&self, visible_rows: u16) -> u16 {
		if self.word_wrap_enabled() {
			self.wrapped_total_rows_for_active_buffer().saturating_sub(visible_rows)
		} else {
			self.max_row().saturating_sub(visible_rows)
		}
	}

	fn wrapped_total_rows_for_active_buffer(&self) -> u16 {
		let Some(text) = self.active_buffer_rope() else {
			return 1;
		};
		let width = self.active_window_visible_text_cols().max(1) as usize;
		geom_wrapped_total_rows_for_rope(text, width)
	}

	fn active_cursor_wrapped_row_index(&self) -> u16 {
		let cursor = self.active_cursor();
		let width = self.active_window_visible_text_cols().max(1) as usize;
		let Some(text) = self.active_buffer_rope() else {
			return 0;
		};
		geom_wrapped_row_index_for_cursor(text, cursor, width)
	}

	fn adjust_scroll_after_move_wrapped(&mut self, direction: VerticalMoveDirection) {
		let active_window_id = self.active_window_id();
		let visible_rows = self.active_window_visible_rows();
		let max_scroll = self.max_scroll_y_for_active_window(visible_rows);
		let threshold = self.cursor_scroll_threshold.min(visible_rows.saturating_sub(1));
		let visible_tail = visible_rows.saturating_sub(1);
		let cursor_wrapped_row = self.active_cursor_wrapped_row_index();

		if let Some(window) = self.windows.get_mut(active_window_id) {
			match direction {
				VerticalMoveDirection::Up => {
					let top_trigger = window.scroll_y.saturating_add(threshold);
					if cursor_wrapped_row < top_trigger {
						window.scroll_y = cursor_wrapped_row.saturating_sub(threshold).min(max_scroll);
					}
				}
				VerticalMoveDirection::Down => {
					let bottom = window.scroll_y.saturating_add(visible_tail);
					let bottom_trigger = bottom.saturating_sub(threshold);
					if cursor_wrapped_row > bottom_trigger {
						let needed_top = cursor_wrapped_row.saturating_add(threshold).saturating_sub(visible_tail);
						window.scroll_y = needed_top.min(max_scroll);
					}
				}
			}
		}
	}

	fn align_active_window_scroll_to_cursor_wrapped(&mut self) {
		let active_window_id = self.active_window_id();
		self.center_window_on_cursor_if_hidden_wrapped(active_window_id);
		let Some(window) = self.windows.get(active_window_id).cloned() else {
			return;
		};
		let cursor_wrapped_row = self.active_cursor_wrapped_row_index();
		let visible_rows = self.active_window_visible_rows();
		let max_scroll = self.max_scroll_y_for_active_window(visible_rows);
		let threshold = self.cursor_scroll_threshold.min(visible_rows.saturating_sub(1));
		let visible_tail = visible_rows.saturating_sub(1);
		let top_trigger = window.scroll_y.saturating_add(threshold);
		let bottom = window.scroll_y.saturating_add(visible_tail);
		let bottom_trigger = bottom.saturating_sub(threshold);

		let mut next_scroll = window.scroll_y;
		if cursor_wrapped_row < top_trigger {
			next_scroll = cursor_wrapped_row.saturating_sub(threshold);
		} else if cursor_wrapped_row > bottom_trigger {
			next_scroll = cursor_wrapped_row.saturating_add(threshold).saturating_sub(visible_tail);
		}
		next_scroll = next_scroll.min(max_scroll);

		if let Some(active_window) = self.windows.get_mut(active_window_id) {
			active_window.scroll_y = next_scroll;
			active_window.scroll_x = 0;
		}
	}

	fn center_window_on_cursor_if_hidden_wrapped(&mut self, window_id: WindowId) {
		let Some(window) = self.windows.get(window_id).cloned() else {
			return;
		};
		let visible_rows = window_visible_rows(&window);
		let top = window.scroll_y;
		let bottom = window.scroll_y.saturating_add(visible_rows.saturating_sub(1));
		let cursor_wrapped_row = self.active_cursor_wrapped_row_index();
		if cursor_wrapped_row >= top && cursor_wrapped_row <= bottom {
			return;
		}
		let max_scroll = self.max_scroll_y_for_active_window(visible_rows);
		let next_scroll = cursor_wrapped_row.saturating_sub(visible_rows / 2).min(max_scroll);
		if let Some(target_window) = self.windows.get_mut(window_id) {
			target_window.scroll_y = next_scroll;
			target_window.scroll_x = 0;
		}
	}
}

fn window_visible_rows(window: &super::super::WindowState) -> u16 {
	let reserved_for_split_line = u16::from(window.y > 0);
	window.height.saturating_sub(reserved_for_split_line).max(1)
}

fn window_visible_text_cols(window: &super::super::WindowState, text: &Rope) -> u16 {
	let reserved_for_split_line = u16::from(window.x > 0);
	let local_width = window.width.saturating_sub(reserved_for_split_line).max(1);
	let total_lines = rope_line_count(text);
	let desired_number_col_width = total_lines.to_string().len() as u16 + 1;
	let number_col_width = if local_width <= desired_number_col_width { 0 } else { desired_number_col_width };
	local_width.saturating_sub(number_col_width).max(1)
}

fn cursor_display_col_for_window(text: &Rope, cursor: CursorState) -> u16 {
	let row_index = cursor.row.saturating_sub(1) as usize;
	rope_line_without_newline(text, row_index)
		.map(|line| geom_display_col_of_cursor_slot(line.as_str(), cursor.col))
		.unwrap_or(0)
}

fn line_display_width_for_window(text: &Rope, cursor: CursorState) -> u16 {
	let row_index = cursor.row.saturating_sub(1) as usize;
	rope_line_without_newline(text, row_index)
		.map(|line| line.chars().map(|ch| geom_char_display_width(ch) as u16).sum())
		.unwrap_or(0)
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
