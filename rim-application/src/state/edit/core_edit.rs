use super::RimState;
use crate::state::EditorOperationError;

impl RimState {
	pub fn insert_char_at_cursor(&mut self, ch: char) {
		if self.editor.insert_char_at_cursor(ch) {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn insert_newline_at_cursor(&mut self) {
		if self.editor.insert_newline_at_cursor() {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn open_line_below_at_cursor(&mut self) {
		if self.editor.open_line_below_at_cursor() {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn open_line_above_at_cursor(&mut self) {
		if self.editor.open_line_above_at_cursor() {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn join_line_below_at_cursor(&mut self) {
		if self.editor.join_line_below_at_cursor() {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn backspace_at_cursor(&mut self) {
		if self.editor.backspace_at_cursor() {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn cut_current_char_to_slot(&mut self) {
		match self.editor.cut_current_char_to_slot() {
			Ok(()) => {
				self.align_active_window_scroll_to_cursor();
				self.workbench.status_bar.message = "char cut".to_string();
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "cut failed: no active buffer".to_string();
			}
			Err(EditorOperationError::OutOfRange) => {
				self.workbench.status_bar.message = "cut failed: out of range".to_string();
			}
			Err(EditorOperationError::NoChar) => {
				self.workbench.status_bar.message = "cut failed: no char".to_string();
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("cut failed: {:?}", other);
			}
		}
	}

	pub fn paste_slot_at_cursor(&mut self) {
		match self.editor.paste_slot_at_cursor() {
			Ok(()) => {
				self.align_active_window_scroll_to_cursor();
				self.workbench.status_bar.message = "pasted".to_string();
			}
			Err(EditorOperationError::SlotEmpty) => {
				self.workbench.status_bar.message = "paste failed: slot is empty".to_string();
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "paste failed: no active buffer".to_string();
			}
			Err(EditorOperationError::OutOfRange) => {
				self.workbench.status_bar.message = "paste failed: out of range".to_string();
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("paste failed: {:?}", other);
			}
		}
	}

	pub fn delete_current_line_to_slot(&mut self) {
		match self.editor.delete_current_line_to_slot() {
			Ok(()) => {
				self.align_active_window_scroll_to_cursor();
				self.workbench.status_bar.message = "line deleted".to_string();
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "line delete failed: no active buffer".to_string();
			}
			Err(EditorOperationError::OutOfRange) => {
				self.workbench.status_bar.message = "line delete failed: out of range".to_string();
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("line delete failed: {:?}", other);
			}
		}
	}
}
