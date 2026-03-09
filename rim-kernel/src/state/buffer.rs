use std::{path::PathBuf, time::Instant};

use ropey::Rope;
use slotmap::Key;

use super::{BufferEditSnapshot, BufferHistoryEntry, BufferId, BufferState, BufferSwitchDirection, CursorState, EditorMode, PersistedBufferHistory, RimState, apply_text_delta_redo, apply_text_delta_undo, buffer_name_from_path, clamp_cursor_for_rope, compute_rope_text_diff, merge_adjacent_insert_history_edits, rope_line_count};

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

	pub fn active_buffer_save_snapshot(
		&self,
		path_override: Option<PathBuf>,
	) -> Result<(BufferId, PathBuf, String), &'static str> {
		let buffer_id = self.active_buffer_id().ok_or("no active buffer")?;
		let buffer = self.buffers.get(buffer_id).ok_or("active buffer missing")?;
		let path = match path_override {
			Some(path) => path,
			None => buffer.path.clone().ok_or("buffer has no file path")?,
		};
		Ok((buffer_id, path, buffer.text.to_string()))
	}

	pub fn active_buffer_load_target(&self) -> Result<(BufferId, PathBuf), &'static str> {
		let buffer_id = self.active_buffer_id().ok_or("no active buffer")?;
		let buffer = self.buffers.get(buffer_id).ok_or("active buffer missing")?;
		let path = buffer.path.clone().ok_or("buffer has no file path")?;
		Ok((buffer_id, path))
	}

	pub fn active_buffer_has_path(&self) -> Option<bool> {
		let buffer_id = self.active_buffer_id()?;
		let buffer = self.buffers.get(buffer_id)?;
		Some(buffer.path.is_some())
	}

	pub fn all_buffer_save_snapshots(&self) -> (Vec<(BufferId, PathBuf, String)>, usize) {
		let mut snapshots = Vec::new();
		let mut missing_path = 0usize;

		for (buffer_id, buffer) in &self.buffers {
			let Some(path) = buffer.path.clone() else {
				missing_path = missing_path.saturating_add(1);
				continue;
			};
			snapshots.push((buffer_id, path, buffer.text.to_string()));
		}

		snapshots.sort_by_key(|(id, ..)| id.data().as_ffi());
		(snapshots, missing_path)
	}

	pub fn set_pending_save_path(&mut self, buffer_id: BufferId, path: Option<PathBuf>) {
		self.pending_save_path = path.map(|p| (buffer_id, p));
	}

	pub fn apply_pending_save_path_if_matches(&mut self, buffer_id: BufferId) {
		let Some((pending_buffer_id, path)) = self.pending_save_path.clone() else {
			return;
		};
		if pending_buffer_id != buffer_id {
			return;
		}

		if let Some(buffer) = self.buffers.get_mut(buffer_id) {
			buffer.path = Some(path.clone());
			if let Some(name) = buffer_name_from_path(&path) {
				buffer.name = name;
			}
		}
		self.pending_save_path = None;
	}

	pub fn clear_pending_save_path_if_matches(&mut self, buffer_id: BufferId) {
		if let Some((pending_buffer_id, _)) = self.pending_save_path
			&& pending_buffer_id == buffer_id
		{
			self.pending_save_path = None;
		}
	}

	pub fn set_buffer_dirty(&mut self, buffer_id: BufferId, dirty: bool) {
		if let Some(buffer) = self.buffers.get_mut(buffer_id) {
			buffer.dirty = dirty;
		}
	}

	pub fn mark_buffer_clean(&mut self, buffer_id: BufferId) {
		if let Some(buffer) = self.buffers.get_mut(buffer_id) {
			buffer.clean_text = buffer.text.clone();
			buffer.dirty = false;
		}
	}

	pub fn set_buffer_externally_modified(&mut self, buffer_id: BufferId, externally_modified: bool) {
		if let Some(buffer) = self.buffers.get_mut(buffer_id) {
			buffer.externally_modified = externally_modified;
		}
	}

	pub fn clear_buffer_history(&mut self, buffer_id: BufferId) {
		let Some(buffer) = self.buffers.get_mut(buffer_id) else {
			return;
		};
		buffer.undo_stack.clear();
		buffer.redo_stack.clear();
		if self.pending_insert_group.as_ref().is_some_and(|group| group.buffer_id == buffer_id) {
			self.pending_insert_group = None;
		}
	}

	pub fn mark_recent_internal_save(&mut self, buffer_id: BufferId) {
		self
			.ignore_external_change_until
			.insert(buffer_id, Instant::now() + Self::INTERNAL_SAVE_WATCHER_IGNORE_WINDOW);
	}

	pub fn clear_recent_internal_save(&mut self, buffer_id: BufferId) {
		self.ignore_external_change_until.remove(&buffer_id);
	}

	pub fn should_ignore_recent_external_change(&mut self, buffer_id: BufferId) -> bool {
		let Some(deadline) = self.ignore_external_change_until.get(&buffer_id).copied() else {
			return false;
		};
		if Instant::now() <= deadline {
			return true;
		}
		self.ignore_external_change_until.remove(&buffer_id);
		false
	}

	pub fn active_buffer_is_externally_modified(&self) -> Option<bool> {
		let buffer_id = self.active_buffer_id()?;
		let buffer = self.buffers.get(buffer_id)?;
		Some(buffer.externally_modified)
	}

	pub fn mark_active_buffer_dirty(&mut self) {
		if let Some(buffer_id) = self.active_buffer_id() {
			self.refresh_buffer_dirty(buffer_id);
		}
	}

	pub fn refresh_buffer_dirty(&mut self, buffer_id: BufferId) {
		if let Some(buffer) = self.buffers.get_mut(buffer_id) {
			buffer.dirty = buffer.text != buffer.clean_text;
		}
	}

	fn append_insert_history_edit(&mut self, buffer_id: BufferId, edit: BufferEditSnapshot) -> bool {
		let Some(group) = self.pending_insert_group.as_mut() else {
			return false;
		};
		if group.buffer_id != buffer_id {
			return false;
		}
		if group.edits.is_empty()
			&& let Some(buffer) = self.buffers.get_mut(buffer_id)
		{
			buffer.redo_stack.clear();
		}
		if let Some(last_edit) = group.edits.last_mut()
			&& merge_adjacent_insert_history_edits(last_edit, &edit)
		{
			return true;
		}
		group.edits.push(edit);
		true
	}

	pub fn apply_edit_entry(
		&mut self,
		buffer_id: BufferId,
		entry: BufferHistoryEntry,
		mode_before: EditorMode,
	) {
		if entry.edits.is_empty() {
			return;
		}
		if self.mode == EditorMode::Insert || mode_before == EditorMode::Insert {
			let mut appended_any = false;
			for edit in &entry.edits {
				if self.append_insert_history_edit(buffer_id, edit.clone()) {
					appended_any = true;
				}
			}
			if appended_any {
				return;
			}
		}
		self.push_buffer_history_entry(buffer_id, entry);
	}

	pub fn record_history_from_text_diff(
		&mut self,
		buffer_id: BufferId,
		before_text: &Rope,
		before_cursor: CursorState,
		mode_before: EditorMode,
		skip_history: bool,
	) {
		if skip_history {
			return;
		}
		let Some(after_buffer) = self.buffers.get(buffer_id) else {
			return;
		};
		let Some(diff) = compute_rope_text_diff(before_text, &after_buffer.text) else {
			return;
		};
		let edit = BufferEditSnapshot {
			start_byte:    diff.start_byte,
			deleted_text:  diff.deleted_text,
			inserted_text: diff.inserted_text,
		};
		let after_cursor =
			self.windows.values().find(|window| window.buffer_id == Some(buffer_id)).map(|window| window.cursor);
		let entry = BufferHistoryEntry {
			edits: vec![edit],
			before_cursor,
			after_cursor: after_cursor.unwrap_or(before_cursor),
		};
		self.apply_edit_entry(buffer_id, entry, mode_before);
	}

	pub fn push_buffer_history_entry(&mut self, buffer_id: BufferId, entry: BufferHistoryEntry) {
		let Some(buffer) = self.buffers.get_mut(buffer_id) else {
			return;
		};
		if entry.edits.is_empty() {
			return;
		}

		buffer.undo_stack.push(entry);
		if buffer.undo_stack.len() > Self::MAX_HISTORY_ENTRIES {
			buffer.undo_stack.remove(0);
		}
		buffer.redo_stack.clear();
	}

	pub fn buffer_persisted_history_snapshot(&self, buffer_id: BufferId) -> Option<PersistedBufferHistory> {
		let buffer = self.buffers.get(buffer_id)?;
		let mut undo_stack = buffer.undo_stack.clone();
		if let Some(group) = self.pending_insert_group.as_ref()
			&& group.buffer_id == buffer_id
			&& !group.edits.is_empty()
		{
			undo_stack.push(BufferHistoryEntry {
				edits:         group.edits.clone(),
				before_cursor: group.before_cursor,
				after_cursor:  self.cursor_for_buffer(buffer_id).unwrap_or(group.before_cursor),
			});
			let overflow = undo_stack.len().saturating_sub(Self::MAX_HISTORY_ENTRIES);
			if overflow > 0 {
				undo_stack.drain(0..overflow);
			}
		}

		Some(PersistedBufferHistory {
			current_text: buffer.text.to_string(),
			cursor: self.cursor_for_buffer(buffer_id).unwrap_or_default(),
			undo_stack,
			redo_stack: buffer.redo_stack.clone(),
		})
	}

	pub fn restore_buffer_persisted_history(
		&mut self,
		buffer_id: BufferId,
		persisted_history: PersistedBufferHistory,
	) -> bool {
		let is_active = self.active_buffer_id() == Some(buffer_id);
		let Some(buffer) = self.buffers.get_mut(buffer_id) else {
			return false;
		};

		if buffer.text != persisted_history.current_text.as_str() {
			return false;
		}

		buffer.undo_stack = persisted_history.undo_stack;
		buffer.redo_stack = persisted_history.redo_stack;
		for (_, window) in &mut self.windows {
			if window.buffer_id == Some(buffer_id) {
				window.cursor = clamp_cursor_for_rope(&buffer.text, persisted_history.cursor);
			}
		}
		if self.pending_insert_group.as_ref().is_some_and(|group| group.buffer_id == buffer_id) {
			self.pending_insert_group = None;
		}

		if is_active {
			self.align_active_window_scroll_to_cursor();
		}
		true
	}

	pub fn all_file_backed_persisted_history_snapshots(
		&self,
	) -> Vec<(BufferId, PathBuf, PersistedBufferHistory)> {
		let mut snapshots = self
			.buffers
			.iter()
			.filter_map(|(buffer_id, buffer)| {
				let path = buffer.path.clone()?;
				let snapshot = self.buffer_persisted_history_snapshot(buffer_id)?;
				Some((buffer_id, path, snapshot))
			})
			.collect::<Vec<_>>();
		snapshots.sort_by_key(|(buffer_id, ..)| buffer_id.data().as_ffi());
		snapshots
	}

	pub fn undo_active_buffer_edit(&mut self) {
		let Some(buffer_id) = self.active_buffer_id() else {
			self.status_bar.message = "undo failed: no active buffer".to_string();
			return;
		};
		let active_window_id = self.active_window_id();
		let Some(buffer) = self.buffers.get_mut(buffer_id) else {
			self.status_bar.message = "undo failed: active buffer missing".to_string();
			return;
		};
		let Some(previous_entry) = buffer.undo_stack.pop() else {
			self.status_bar.message = "undo: nothing to undo".to_string();
			return;
		};

		for edit in previous_entry.edits.iter().rev() {
			apply_text_delta_undo(&mut buffer.text, edit);
		}
		if let Some(window) = self.windows.get_mut(active_window_id) {
			window.cursor = previous_entry.before_cursor;
		}
		buffer.redo_stack.push(previous_entry);
		if buffer.redo_stack.len() > Self::MAX_HISTORY_ENTRIES {
			buffer.redo_stack.remove(0);
		}
		buffer.dirty = buffer.text != buffer.clean_text;

		self.align_active_window_scroll_to_cursor();
		self.status_bar.message = "undo".to_string();
	}

	pub fn redo_active_buffer_edit(&mut self) {
		let Some(buffer_id) = self.active_buffer_id() else {
			self.status_bar.message = "redo failed: no active buffer".to_string();
			return;
		};
		let active_window_id = self.active_window_id();
		let Some(buffer) = self.buffers.get_mut(buffer_id) else {
			self.status_bar.message = "redo failed: active buffer missing".to_string();
			return;
		};
		let Some(next_entry) = buffer.redo_stack.pop() else {
			self.status_bar.message = "redo: nothing to redo".to_string();
			return;
		};

		for edit in &next_entry.edits {
			apply_text_delta_redo(&mut buffer.text, edit);
		}
		if let Some(window) = self.windows.get_mut(active_window_id) {
			window.cursor = next_entry.after_cursor;
		}
		buffer.undo_stack.push(next_entry);
		if buffer.undo_stack.len() > Self::MAX_HISTORY_ENTRIES {
			buffer.undo_stack.remove(0);
		}
		buffer.dirty = buffer.text != buffer.clean_text;

		self.align_active_window_scroll_to_cursor();
		self.status_bar.message = "redo".to_string();
	}

	pub fn has_dirty_buffers(&self) -> bool { self.buffers.values().any(|buffer| buffer.dirty) }

	pub fn active_buffer_rope(&self) -> Option<&Rope> {
		let buffer_id = self.active_buffer_id()?;
		self.buffers.get(buffer_id).map(|buffer| &buffer.text)
	}

	pub(crate) fn clamp_window_cursors_for_buffer(&mut self, buffer_id: BufferId) {
		let Some(buffer) = self.buffers.get(buffer_id) else {
			return;
		};
		let text = &buffer.text;
		for (_, window) in &mut self.windows {
			if window.buffer_id == Some(buffer_id) {
				window.cursor = clamp_cursor_for_rope(text, window.cursor);
			}
		}
	}

	pub(crate) fn cursor_for_buffer(&self, buffer_id: BufferId) -> Option<CursorState> {
		self.windows.values().find(|window| window.buffer_id == Some(buffer_id)).map(|window| window.cursor)
	}

	pub fn active_buffer_text_string(&self) -> Option<String> {
		self.active_buffer_rope().map(ToString::to_string)
	}

	pub fn buffer_text_string(&self, buffer_id: BufferId) -> Option<String> {
		self.buffers.get(buffer_id).map(|buffer| buffer.text.to_string())
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
