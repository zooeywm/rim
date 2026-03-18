mod core_edit;
mod movement;
mod visual;

use super::{CursorState, RimState};

impl RimState {
	fn active_buffer_cursor_mut(&mut self) -> Option<&mut CursorState> {
		let active_window_id = self.active_window_id();
		self.windows.get_mut(active_window_id).map(|window| &mut window.cursor)
	}
}
