mod core_edit;
mod movement;
mod visual;

pub(crate) use rim_domain::edit::{block_col_for_display_target, clamp_cursor_col_for_line, cursor_slot_display_col, expand_tab_padding_at_display_target, pad_rope_line_to_char_len, previous_char_display_width, rope_block_char_range, rope_cursor_char, rope_editable_line_count, rope_editable_line_len_chars, rope_join_rows_without_newline, rope_linewise_char_range, rope_linewise_insertion_text, split_lines_owned};

use super::{CursorState, RimState};

impl RimState {
	fn active_buffer_cursor_mut(&mut self) -> Option<&mut CursorState> {
		let active_window_id = self.active_window_id();
		self.windows.get_mut(active_window_id).map(|window| &mut window.cursor)
	}

	fn active_buffer_and_window_mut(&mut self) -> Option<(&mut super::BufferState, &mut super::WindowState)> {
		let buffer_id = self.active_buffer_id()?;
		let window_id = self.active_window_id();
		let (buffers, windows) = (&mut self.editor.buffers, &mut self.editor.windows);
		let buffer = buffers.get_mut(buffer_id)?;
		let window = windows.get_mut(window_id)?;
		Some((buffer, window))
	}
}
