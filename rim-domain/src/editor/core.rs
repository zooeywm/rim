use crate::{editor::EditorState, model::{BufferId, WindowBufferViewState, WindowId, WindowState}, text::clamp_cursor_for_rope};

impl EditorState {
	pub fn create_window(&mut self, buffer_id: Option<BufferId>) -> Option<WindowId> {
		if let Some(buffer_id) = buffer_id
			&& !self.buffers.contains_key(buffer_id)
		{
			return None;
		}

		let id = self.windows.insert(WindowState { buffer_id, ..WindowState::default() });
		if let Some(buffer_id) = buffer_id {
			self.window_buffer_views.insert((id, buffer_id), WindowBufferViewState::default());
		}
		Some(id)
	}

	pub fn active_buffer_id(&self) -> Option<BufferId> {
		self.windows.get(self.active_window_id()).and_then(|window| window.buffer_id)
	}

	pub fn bind_buffer_to_active_window(&mut self, buffer_id: BufferId) {
		let active_window_id = self.active_window_id();
		self.bind_buffer_to_window(active_window_id, buffer_id, true);
	}

	pub fn bind_buffer_to_window(
		&mut self,
		window_id: WindowId,
		buffer_id: BufferId,
		persist_previous_cursor: bool,
	) {
		let Some(window_snapshot) = self.windows.get(window_id).copied() else {
			return;
		};
		let previous_buffer_id = window_snapshot.buffer_id;
		if persist_previous_cursor && let Some(previous_buffer_id) = window_snapshot.buffer_id {
			self.window_buffer_views.insert((window_id, previous_buffer_id), WindowBufferViewState {
				cursor:   window_snapshot.cursor,
				scroll_x: window_snapshot.scroll_x,
				scroll_y: window_snapshot.scroll_y,
			});
		}

		let restored_view = self.window_buffer_views.get(&(window_id, buffer_id)).copied().unwrap_or_default();
		let next_cursor = self
			.buffers
			.get(buffer_id)
			.map(|buffer| clamp_cursor_for_rope(&buffer.text, restored_view.cursor))
			.unwrap_or(restored_view.cursor);
		if let Some(window) = self.windows.get_mut(window_id) {
			window.buffer_id = Some(buffer_id);
			window.cursor = next_cursor;
			window.scroll_x = restored_view.scroll_x;
			window.scroll_y = restored_view.scroll_y;
		}
		self.window_buffer_views.insert((window_id, buffer_id), WindowBufferViewState {
			cursor:   next_cursor,
			scroll_x: restored_view.scroll_x,
			scroll_y: restored_view.scroll_y,
		});
		if let Some(tab_id) = self.tab_id_for_window(window_id) {
			self.register_buffer_in_tab_order(tab_id, buffer_id, previous_buffer_id);
		}
	}

	pub fn sync_window_view_binding(&mut self, window_id: WindowId) {
		let Some((buffer_id, cursor, scroll_x, scroll_y)) = self.windows.get(window_id).and_then(|window| {
			window.buffer_id.map(|buffer_id| (buffer_id, window.cursor, window.scroll_x, window.scroll_y))
		}) else {
			return;
		};
		self.window_buffer_views.insert((window_id, buffer_id), WindowBufferViewState {
			cursor,
			scroll_x,
			scroll_y,
		});
	}

	pub fn remove_window_view_bindings(&mut self, window_id: WindowId) {
		self.window_buffer_views.retain(|(candidate_window_id, _), _| *candidate_window_id != window_id);
	}
}
