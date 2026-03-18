use super::RimState;
use crate::state::EditorOperationError;

impl RimState {
	pub fn begin_visual_block_insert(&mut self, append: bool) {
		match self.editor.begin_visual_block_insert(append) {
			Ok(pending) => {
				self.enter_block_insert_mode(pending);
				self.align_active_window_scroll_to_cursor();
			}
			Err(EditorOperationError::NoAnchor) => {
				self.workbench.status_bar.message = "block insert failed: no anchor".to_string();
				self.exit_visual_mode();
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "block insert failed: no active buffer".to_string();
				self.exit_visual_mode();
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("block insert failed: {:?}", other);
				self.exit_visual_mode();
			}
		}
	}

	pub fn insert_char_at_block_cursor(&mut self, ch: char) {
		if self.pending_block_insert.is_none() {
			self.insert_char_at_cursor(ch);
			return;
		}
		if self.editor.insert_char_at_block_cursor(ch) {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn backspace_at_block_cursor(&mut self) {
		if self.pending_block_insert.is_none() {
			self.backspace_at_cursor();
			return;
		}
		if self.editor.backspace_at_block_cursor() {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn delete_visual_selection_to_slot(&mut self) -> bool {
		match self.editor.delete_visual_selection_to_slot() {
			Ok(()) => {
				self.align_active_window_scroll_to_cursor();
				self.exit_visual_mode();
				self.workbench.status_bar.message = "selection deleted".to_string();
				true
			}
			Err(EditorOperationError::NoAnchor) => {
				self.workbench.status_bar.message = "visual delete failed: no anchor".to_string();
				self.exit_visual_mode();
				false
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "visual delete failed: no active buffer".to_string();
				self.exit_visual_mode();
				false
			}
			Err(EditorOperationError::OutOfRange) => {
				self.workbench.status_bar.message = "visual delete failed: out of range".to_string();
				self.exit_visual_mode();
				false
			}
			Err(EditorOperationError::EmptySelection) => {
				self.workbench.status_bar.message = "visual delete failed: empty".to_string();
				self.exit_visual_mode();
				false
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("visual delete failed: {:?}", other);
				self.exit_visual_mode();
				false
			}
		}
	}

	pub fn change_visual_selection_to_insert_mode(&mut self) {
		let block_insert = self.editor.pending_block_insert_from_visual_selection();

		self.begin_insert_history_group();
		if !self.delete_visual_selection_to_slot() {
			self.cancel_insert_history_group();
			return;
		}

		if let Some(pending) = block_insert {
			let target_col = self
				.active_buffer_rope()
				.map(|_| self.editor.visual_block_col_for_display_target(pending.start_row, pending.base_display_col))
				.unwrap_or(1);
			if let Some(cursor) = self.active_buffer_cursor_mut() {
				cursor.row = pending.start_row;
				cursor.col = target_col;
			}
			self.enter_block_insert_mode(pending);
			self.preferred_col = None;
			self.align_active_window_scroll_to_cursor();
		} else {
			self.enter_insert_mode();
		}
		self.workbench.status_bar.message = "selection changed".to_string();
	}

	pub fn yank_visual_selection_to_slot(&mut self) {
		match self.editor.yank_visual_selection_to_slot() {
			Ok(()) => {
				self.exit_visual_mode();
				self.workbench.status_bar.message = "selection yanked".to_string();
			}
			Err(EditorOperationError::NoAnchor) => {
				self.workbench.status_bar.message = "visual yank failed: no anchor".to_string();
				self.exit_visual_mode();
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "visual yank failed: no active buffer".to_string();
				self.exit_visual_mode();
			}
			Err(EditorOperationError::OutOfRange) => {
				self.workbench.status_bar.message = "visual yank failed: out of range".to_string();
				self.exit_visual_mode();
			}
			Err(EditorOperationError::EmptySelection) => {
				self.workbench.status_bar.message = "visual yank failed: empty".to_string();
				self.exit_visual_mode();
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("visual yank failed: {:?}", other);
				self.exit_visual_mode();
			}
		}
	}

	pub fn replace_visual_selection_with_slot(&mut self) {
		match self.editor.replace_visual_selection_with_slot() {
			Ok(()) => {
				self.align_active_window_scroll_to_cursor();
				self.exit_visual_mode();
				self.workbench.status_bar.message = "selection replaced".to_string();
			}
			Err(EditorOperationError::SlotEmpty) => {
				self.workbench.status_bar.message = "paste failed: slot is empty".to_string();
				self.exit_visual_mode();
			}
			Err(EditorOperationError::NoAnchor) => {
				self.workbench.status_bar.message = "visual paste failed: no anchor".to_string();
				self.exit_visual_mode();
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "visual paste failed: no active buffer".to_string();
				self.exit_visual_mode();
			}
			Err(EditorOperationError::OutOfRange) => {
				self.workbench.status_bar.message = "visual paste failed: out of range".to_string();
				self.exit_visual_mode();
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("visual paste failed: {:?}", other);
				self.exit_visual_mode();
			}
		}
	}
}
