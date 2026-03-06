use std::path::PathBuf;

use ropey::Rope;

use super::{BufferId, BufferState, BufferSwitchDirection, RimState, rope_line_count};

impl RimState {
	pub fn find_buffer_by_path(&self, path: &std::path::Path) -> Option<BufferId> {
		self
			.buffers
			.iter()
			.find_map(|(buffer_id, buffer)| (buffer.path.as_deref() == Some(path)).then_some(buffer_id))
	}

	pub fn create_buffer(&mut self, path: Option<PathBuf>, text: impl Into<String>) -> BufferId {
		let text = text.into();
		let rope = Rope::from_str(text.as_str());
		let name =
			path.as_deref().and_then(super::buffer_name_from_path).unwrap_or_else(|| "untitled".to_string());

		let id = self.buffers.insert(BufferState {
			name,
			path,
			text: rope.clone(),
			clean_text: rope,
			dirty: false,
			externally_modified: false,
			undo_stack: Vec::new(),
			redo_stack: Vec::new(),
		});
		self.buffer_order.push(id);
		id
	}

	pub fn close_active_buffer(&mut self) {
		let Some(active_buffer_id) = self.active_buffer_id() else {
			self.status_bar.message = "buffer close failed: no active buffer".to_string();
			return;
		};
		self.close_buffer(active_buffer_id);
	}

	pub fn close_buffer(&mut self, target_buffer_id: BufferId) {
		if !self.buffers.contains_key(target_buffer_id) {
			self.status_bar.message = "buffer close failed: target buffer missing".to_string();
			return;
		}

		let mut fallback = match self.buffer_order.iter().position(|id| *id == target_buffer_id) {
			Some(idx) if self.buffer_order.len() > 1 => {
				if idx > 0 {
					Some(self.buffer_order[idx - 1])
				} else {
					Some(self.buffer_order[1])
				}
			}
			_ => None,
		};
		self.buffer_order.retain(|id| *id != target_buffer_id);
		self.in_flight_internal_saves.remove(&target_buffer_id);
		self.ignore_external_change_until.remove(&target_buffer_id);

		let _ = self.buffers.remove(target_buffer_id);
		if fallback.is_none() {
			fallback = Some(self.create_buffer(None, String::new()));
		}

		for (_, window) in &mut self.windows {
			if window.buffer_id == Some(target_buffer_id) {
				window.buffer_id = fallback;
			}
		}
		if let Some(fallback_id) = fallback {
			self.clamp_window_cursors_for_buffer(fallback_id);
		}

		self.align_active_window_scroll_to_cursor();
		self.status_bar.message = "buffer closed".to_string();
	}

	pub fn replace_buffer_text_preserving_cursor(&mut self, buffer_id: BufferId, text: String) {
		let is_active = self.active_buffer_id() == Some(buffer_id);
		let Some(buffer) = self.buffers.get_mut(buffer_id) else {
			return;
		};
		let previous_max_row = rope_line_count(&buffer.text) as u16;

		buffer.text = Rope::from_str(text.as_str());
		let new_max_row = rope_line_count(&buffer.text) as u16;
		for (_, window) in &mut self.windows {
			if window.buffer_id != Some(buffer_id) {
				continue;
			}
			if window.cursor.row >= previous_max_row {
				window.cursor.row = new_max_row;
			} else {
				window.cursor = super::clamp_cursor_for_rope(&buffer.text, window.cursor);
			}
			window.cursor = super::clamp_cursor_for_rope(&buffer.text, window.cursor);
		}

		if is_active {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn create_untitled_buffer(&mut self) -> BufferId {
		let previous_active = self.active_buffer_id();
		let buffer_id = self.create_buffer(None, String::new());
		if let Some(active_buffer_id) = previous_active {
			self.move_buffer_after(buffer_id, active_buffer_id);
		}
		self.bind_buffer_to_active_window(buffer_id);
		self.status_bar.message = "new buffer".to_string();
		buffer_id
	}

	pub fn switch_active_window_buffer(&mut self, direction: BufferSwitchDirection) {
		let active_window_id = self.active_window_id();
		if self.buffer_order.is_empty() {
			return;
		}

		let current = self
			.windows
			.get(active_window_id)
			.expect("invariant: active window id must exist in windows")
			.buffer_id;
		let target = match current.and_then(|id| self.buffer_order.iter().position(|x| *x == id)) {
			Some(idx) => match direction {
				BufferSwitchDirection::Prev => {
					if idx == 0 {
						*self.buffer_order.last().expect("non-empty by construction")
					} else {
						self.buffer_order[idx.saturating_sub(1)]
					}
				}
				BufferSwitchDirection::Next => {
					if idx + 1 >= self.buffer_order.len() {
						self.buffer_order[0]
					} else {
						self.buffer_order[idx + 1]
					}
				}
			},
			None => match direction {
				BufferSwitchDirection::Prev => *self.buffer_order.last().expect("non-empty by construction"),
				BufferSwitchDirection::Next => self.buffer_order[0],
			},
		};

		if let Some(window) = self.windows.get_mut(active_window_id) {
			window.buffer_id = Some(target);
		}
		self.clamp_window_cursors_for_buffer(target);
		self.align_active_window_scroll_to_cursor();
		if let Some(buffer) = self.buffers.get(target) {
			self.status_bar.message = format!("buffer {}", buffer.name);
		}
	}

	fn move_buffer_after(&mut self, buffer_id: BufferId, anchor_id: BufferId) {
		if buffer_id == anchor_id {
			return;
		}
		let Some(from_idx) = self.buffer_order.iter().position(|id| *id == buffer_id) else {
			return;
		};
		self.buffer_order.remove(from_idx);
		if let Some(anchor_idx) = self.buffer_order.iter().position(|id| *id == anchor_id) {
			self.buffer_order.insert(anchor_idx + 1, buffer_id);
		} else {
			self.buffer_order.push(buffer_id);
		}
	}
}
