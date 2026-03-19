use rim_domain::display_geometry::{char_display_width as geom_char_display_width, display_width_of_char_prefix_with_virtual as geom_display_width_of_char_prefix_with_virtual, wrapped_row_index_for_cursor as geom_wrapped_row_index_for_cursor, wrapped_total_rows_for_rope as geom_wrapped_total_rows_for_rope};
use ropey::Rope;
use tracing::{error, trace};

use super::{CursorState, FocusDirection, RimState, SplitAxis, WindowId, WindowState};

impl RimState {
	pub fn focus_window(&mut self, direction: FocusDirection) { self.editor.focus_window(direction); }

	pub fn close_active_window(&mut self) {
		if !self.editor.close_active_window() {
			return;
		}
		self.workbench.status_bar.message = "window closed".to_string();
	}

	pub fn split_active_window(&mut self, axis: SplitAxis) {
		let active_window_id = self.active_window_id();
		let Some(new_window_id) = self.editor.split_active_window(axis) else {
			error!(
				"split_active_window failed: unable to create new window for buffer {:?}",
				self.windows.get(active_window_id).and_then(|window| window.buffer_id)
			);
			return;
		};
		self.center_window_on_cursor_if_hidden(active_window_id);
		self.center_window_on_cursor_if_hidden(new_window_id);
		self.workbench.status_bar.message = match axis {
			SplitAxis::Horizontal => "split horizontal".to_string(),
			SplitAxis::Vertical => "split vertical".to_string(),
		};
	}

	pub fn update_active_tab_layout(&mut self, width: u16, height: u16) {
		trace!("update_active_tab_layout");
		let window_ids = self.active_tab_window_ids();
		let previous_windows = window_ids
			.iter()
			.filter_map(|id| self.windows.get(*id).copied().map(|window| (*id, window)))
			.collect::<std::collections::HashMap<_, _>>();
		self.editor.update_active_tab_layout(width, height);

		for window_id in window_ids {
			let previous_window = previous_windows.get(&window_id).copied();
			self.reconcile_window_view_to_layout(window_id, previous_window);
			self.sync_window_view_binding(window_id);
		}
	}
}

impl RimState {
	fn clamp_cursor_for_layout_mode(&self, text: &Rope, cursor: CursorState) -> CursorState {
		self.editor.clamp_cursor_for_layout_mode(text, cursor)
	}

	fn reconcile_window_view_to_layout(&mut self, window_id: WindowId, previous_window: Option<WindowState>) {
		let Some(window_snapshot) = self.windows.get(window_id).copied() else {
			return;
		};
		let Some(buffer_id) = window_snapshot.buffer_id else {
			return;
		};
		let Some(buffer) = self.buffers.get(buffer_id) else {
			return;
		};

		let cursor = self.clamp_cursor_for_layout_mode(&buffer.text, window_snapshot.cursor);
		let visible_rows = window_visible_rows(&window_snapshot);
		let visible_cols = window_visible_text_cols(&window_snapshot, &buffer.text);
		let cursor_line = cursor.row.saturating_sub(1);
		let cursor_display_col = if self.is_block_insert_mode() && window_id == self.active_window_id() {
			self
				.pending_block_insert
				.map(|pending| pending.cursor_display_col)
				.unwrap_or_else(|| cursor_display_col_for_window(&buffer.text, cursor))
		} else {
			cursor_display_col_for_window(&buffer.text, cursor)
		};
		if self.word_wrap_enabled() {
			let wrap_width = visible_cols.max(1) as usize;
			let cursor_wrapped_row = geom_wrapped_row_index_for_cursor(&buffer.text, cursor, wrap_width);
			let max_scroll_y =
				geom_wrapped_total_rows_for_rope(&buffer.text, wrap_width).saturating_sub(visible_rows);
			let visible_row_tail = visible_rows.saturating_sub(1);
			let mut next_scroll_y = window_snapshot.scroll_y.min(max_scroll_y);
			let bottom = next_scroll_y.saturating_add(visible_row_tail);
			if cursor_wrapped_row < next_scroll_y {
				next_scroll_y = cursor_wrapped_row;
			} else if cursor_wrapped_row > bottom {
				next_scroll_y = cursor_wrapped_row.saturating_sub(visible_row_tail).min(max_scroll_y);
			}
			if let Some(window) = self.windows.get_mut(window_id) {
				window.cursor = cursor;
				window.scroll_y = next_scroll_y;
				window.scroll_x = 0;
			}
			return;
		}
		let max_scroll_y = (crate::state::rope_line_count(&buffer.text) as u16).saturating_sub(visible_rows);
		let visible_row_tail = visible_rows.saturating_sub(1);
		let max_visible_col_tail = visible_cols.saturating_sub(1);
		let line_display_width = line_display_width_for_window(&buffer.text, cursor).max(cursor_display_col);
		let max_scroll_x = line_display_width.saturating_sub(max_visible_col_tail);

		let mut next_scroll_y = window_snapshot.scroll_y.min(max_scroll_y);
		if let Some(previous_window) = previous_window {
			let previous_visible_rows = window_visible_rows(&previous_window);
			let previous_bottom = previous_window.scroll_y.saturating_add(previous_visible_rows.saturating_sub(1));
			if cursor_line == previous_bottom {
				next_scroll_y = cursor_line.saturating_sub(visible_row_tail).min(max_scroll_y);
			} else if cursor_line == previous_window.scroll_y {
				next_scroll_y = cursor_line.min(max_scroll_y);
			}
		}
		let bottom = next_scroll_y.saturating_add(visible_row_tail);
		if cursor_line < next_scroll_y {
			next_scroll_y = cursor_line;
		} else if cursor_line > bottom {
			next_scroll_y = cursor_line.saturating_sub(visible_row_tail).min(max_scroll_y);
		}

		let mut next_scroll_x = window_snapshot.scroll_x.min(max_scroll_x);
		if let Some(previous_window) = previous_window {
			let previous_visible_cols = window_visible_text_cols(&previous_window, &buffer.text);
			let previous_right = previous_window.scroll_x.saturating_add(previous_visible_cols.saturating_sub(1));
			if cursor_display_col == previous_right {
				next_scroll_x = cursor_display_col.saturating_sub(max_visible_col_tail).min(max_scroll_x);
			} else if cursor_display_col == previous_window.scroll_x {
				next_scroll_x = cursor_display_col.min(max_scroll_x);
			}
		}
		let right = next_scroll_x.saturating_add(max_visible_col_tail);
		if cursor_display_col < next_scroll_x {
			next_scroll_x = cursor_display_col;
		} else if cursor_display_col > right {
			next_scroll_x = cursor_display_col.saturating_sub(max_visible_col_tail).min(max_scroll_x);
		}

		if let Some(window) = self.windows.get_mut(window_id) {
			window.cursor = cursor;
			window.scroll_y = next_scroll_y;
			window.scroll_x = next_scroll_x;
		}
	}
}

fn window_visible_rows(window: &WindowState) -> u16 {
	let reserved_for_split_line = u16::from(window.y > 0);
	window.height.saturating_sub(reserved_for_split_line).max(1)
}

fn window_visible_text_cols(window: &WindowState, text: &Rope) -> u16 {
	let reserved_for_split_line = u16::from(window.x > 0);
	let local_width = window.width.saturating_sub(reserved_for_split_line).max(1);
	let total_lines = crate::state::rope_line_count(text);
	let desired_number_col_width = total_lines.to_string().len() as u16 + 1;
	let number_col_width = if local_width <= desired_number_col_width { 0 } else { desired_number_col_width };
	local_width.saturating_sub(number_col_width).max(1)
}

fn cursor_display_col_for_window(text: &Rope, cursor: crate::state::CursorState) -> u16 {
	let row_index = cursor.row.saturating_sub(1) as usize;
	let char_index = cursor.col.saturating_sub(1) as usize;
	crate::state::rope_line_without_newline(text, row_index)
		.map(|line| geom_display_width_of_char_prefix_with_virtual(line.as_str(), char_index) as u16)
		.unwrap_or(0)
}

fn line_display_width_for_window(text: &Rope, cursor: crate::state::CursorState) -> u16 {
	let row_index = cursor.row.saturating_sub(1) as usize;
	let base_width = crate::state::rope_line_without_newline(text, row_index)
		.map(|line| line.chars().map(|ch| geom_char_display_width(ch) as u16).sum())
		.unwrap_or(0);
	base_width.max(cursor_display_col_for_window(text, cursor))
}
