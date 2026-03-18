use crate::{display_geometry::{char_display_width, display_width_of_char_prefix_with_virtual, navigable_col_for_display_target as geom_navigable_col_for_display_target}, editor::EditorState, text::{rope_ends_with_newline, rope_is_empty, rope_line_count, rope_line_len_chars, rope_line_without_newline}};

impl EditorState {
	pub fn active_cursor(&self) -> crate::model::CursorState {
		self.windows.get(self.active_window_id()).map(|window| window.cursor).unwrap_or_default()
	}

	pub fn move_cursor_up(&mut self) {
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
	}

	pub fn move_cursor_down(&mut self) {
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
	}

	pub fn move_cursor_left(&mut self) {
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.col > 1
		{
			cursor.col = cursor.col.saturating_sub(1);
		}
		self.preferred_col = None;
	}

	pub fn move_cursor_right(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_navigable_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.col < max_col
		{
			cursor.col = cursor.col.saturating_add(1);
		}
		self.preferred_col = None;
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
			if let Some(cursor) = self.active_buffer_cursor_mut() {
				cursor.col = target_col;
			}
			return;
		}
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.col > 1
		{
			cursor.col = cursor.col.saturating_sub(1);
		}
		self.preferred_col = None;
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
			if let Some(cursor) = self.active_buffer_cursor_mut() {
				cursor.col = target_col;
			}
			return;
		}
		let row = self.active_cursor().row;
		let max_col = self.max_visual_char_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut()
			&& cursor.col < max_col
		{
			cursor.col = cursor.col.saturating_add(1);
		}
		self.preferred_col = None;
	}

	pub fn move_cursor_line_start(&mut self) {
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = 1;
		}
		self.preferred_col = None;
	}

	pub fn move_cursor_line_end(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_navigable_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = max_col;
		}
		self.preferred_col = None;
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
	}

	pub fn move_cursor_to_insert_line_end_slot(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = max_col.max(1);
		}
		self.preferred_col = None;
	}

	pub fn clamp_cursor_to_navigable_col(&mut self) {
		let row = self.active_cursor().row;
		let max_col = self.max_navigable_col_for_row(row);
		if let Some(cursor) = self.active_buffer_cursor_mut() {
			cursor.col = cursor.col.min(max_col).max(1);
		}
	}

	pub fn target_display_col_for_vertical_move(&mut self) -> u16 {
		if self.is_visual_block_mode() {
			let col = self.visual_block_cursor_display_col.unwrap_or_else(|| self.active_cursor_display_col());
			self.preferred_col = Some(col);
			return col;
		}
		self.capture_preferred_col_for_vertical()
	}

	pub fn capture_preferred_col_for_vertical(&mut self) -> u16 {
		if let Some(col) = self.preferred_col {
			return col;
		}
		let col = self.active_cursor_display_col();
		self.preferred_col = Some(col);
		col
	}

	pub fn max_row(&self) -> u16 {
		self.active_buffer_rope().map(|text| rope_line_count(text) as u16).unwrap_or(1)
	}

	pub fn max_col_for_row(&self, row: u16) -> u16 {
		let row_index = row.saturating_sub(1) as usize;
		let line_len =
			self.active_buffer_rope().map(|text| rope_line_len_chars(text, row_index) as u16).unwrap_or(0);
		line_len.saturating_add(1)
	}

	pub fn active_cursor_display_col(&self) -> u16 {
		if self.is_visual_block_mode()
			&& let Some(col) = self.visual_block_cursor_display_col
		{
			return col;
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
			.map(|line| display_width_of_char_prefix_with_virtual(line.as_str(), char_index) as u16)
			.unwrap_or(0)
	}

	pub fn active_line_display_width(&self) -> u16 {
		let cursor = self.active_cursor();
		let row_index = cursor.row.saturating_sub(1) as usize;
		let base_width = self
			.active_buffer_rope()
			.and_then(|text| rope_line_without_newline(text, row_index))
			.map(|line| line.chars().map(|ch| char_display_width(ch) as u16).sum())
			.unwrap_or(0);

		if self.is_visual_block_mode() || self.is_block_insert_mode() {
			base_width.max(self.active_cursor_display_col())
		} else {
			base_width
		}
	}

	pub fn navigable_col_for_display_target(&self, row: u16, target_display_col: u16) -> u16 {
		let Some(text) = self.active_buffer_rope() else {
			return 1;
		};
		let col = geom_navigable_col_for_display_target(text, row, target_display_col);
		col.min(self.max_navigable_col_for_row(row)).max(1)
	}

	pub fn visual_block_col_for_display_target(&self, row: u16, target_display_col: u16) -> u16 {
		crate::edit::block_col_for_display_target(
			self.active_buffer_rope().unwrap_or(&ropey::Rope::new()),
			row,
			target_display_col,
		)
	}

	fn active_buffer_cursor_mut(&mut self) -> Option<&mut crate::model::CursorState> {
		let active_window_id = self.active_window_id();
		self.windows.get_mut(active_window_id).map(|window| &mut window.cursor)
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

	fn is_visual_block_mode(&self) -> bool { self.mode == crate::model::EditorMode::VisualBlock }

	fn is_block_insert_mode(&self) -> bool {
		self.mode == crate::model::EditorMode::Insert && self.pending_block_insert.is_some()
	}
}
