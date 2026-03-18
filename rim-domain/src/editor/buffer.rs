use std::path::{Path, PathBuf};

use ropey::Rope;
use slotmap::Key;

use crate::{editor::EditorState, model::{BufferEditSnapshot, BufferHistoryEntry, BufferId, BufferState, BufferSwitchDirection, CursorState, EditorMode, PersistedBufferHistory, TabId}, text::{buffer_name_from_path, clamp_cursor_for_rope, compute_rope_text_diff, merge_adjacent_insert_history_edits, rope_line_count}};

impl EditorState {
	pub const MAX_HISTORY_ENTRIES: usize = 256;

	pub fn tab_id_for_window(&self, window_id: crate::model::WindowId) -> Option<TabId> {
		self.tabs.iter().find_map(|(tab_id, tab)| tab.windows.contains(&window_id).then_some(*tab_id))
	}

	pub fn register_buffer_in_tab_order(
		&mut self,
		tab_id: TabId,
		buffer_id: BufferId,
		anchor_buffer_id: Option<BufferId>,
	) {
		let Some(tab) = self.tabs.get_mut(&tab_id) else {
			return;
		};
		if tab.buffer_order.contains(&buffer_id) {
			return;
		}
		if let Some(anchor_buffer_id) = anchor_buffer_id
			&& let Some(anchor_idx) = tab.buffer_order.iter().position(|id| *id == anchor_buffer_id)
		{
			tab.buffer_order.insert(anchor_idx + 1, buffer_id);
			return;
		}
		tab.buffer_order.push(buffer_id);
	}

	pub fn remove_buffer_from_tab_orders(&mut self, buffer_id: BufferId) {
		for tab in self.tabs.values_mut() {
			tab.buffer_order.retain(|id| *id != buffer_id);
		}
	}

	pub fn move_buffer_after_in_tab(&mut self, tab_id: TabId, buffer_id: BufferId, anchor_id: BufferId) {
		let Some(tab) = self.tabs.get_mut(&tab_id) else {
			return;
		};
		if buffer_id == anchor_id {
			return;
		}
		let Some(from_idx) = tab.buffer_order.iter().position(|id| *id == buffer_id) else {
			return;
		};
		tab.buffer_order.remove(from_idx);
		if let Some(anchor_idx) = tab.buffer_order.iter().position(|id| *id == anchor_id) {
			tab.buffer_order.insert(anchor_idx + 1, buffer_id);
		} else {
			tab.buffer_order.push(buffer_id);
		}
	}

	pub fn find_buffer_by_path(&self, path: &Path) -> Option<BufferId> {
		self
			.buffers
			.iter()
			.find_map(|(buffer_id, buffer)| (buffer.path.as_deref() == Some(path)).then_some(buffer_id))
	}

	pub fn create_buffer(&mut self, path: Option<PathBuf>, text: impl Into<String>) -> BufferId {
		let text = text.into();
		let rope = Rope::from_str(text.as_str());
		let name = path.as_deref().and_then(buffer_name_from_path).unwrap_or_else(|| "untitled".to_string());

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
		self.register_buffer_in_tab_order(self.active_tab, id, None);
		id
	}

	pub fn create_untitled_buffer(&mut self) -> BufferId {
		let previous_active = self.active_buffer_id();
		let buffer_id = self.create_buffer(None, String::new());
		if let Some(active_buffer_id) = previous_active {
			self.move_buffer_after(buffer_id, active_buffer_id);
			self.move_buffer_after_in_tab(self.active_tab, buffer_id, active_buffer_id);
		}
		self.bind_buffer_to_active_window(buffer_id);
		buffer_id
	}

	pub fn switch_active_window_buffer(&mut self, direction: BufferSwitchDirection) -> Option<BufferId> {
		let active_window_id = self.active_window_id();
		let active_tab_buffers = self.active_tab_buffer_ids();
		if active_tab_buffers.is_empty() {
			return None;
		}

		let current = self
			.windows
			.get(active_window_id)
			.expect("invariant: active window id must exist in windows")
			.buffer_id;
		let target = match current.and_then(|id| active_tab_buffers.iter().position(|x| *x == id)) {
			Some(idx) => match direction {
				BufferSwitchDirection::Prev => {
					if idx == 0 {
						*active_tab_buffers.last().expect("non-empty by construction")
					} else {
						active_tab_buffers[idx.saturating_sub(1)]
					}
				}
				BufferSwitchDirection::Next => {
					if idx + 1 >= active_tab_buffers.len() {
						active_tab_buffers[0]
					} else {
						active_tab_buffers[idx + 1]
					}
				}
			},
			None => match direction {
				BufferSwitchDirection::Prev => *active_tab_buffers.last().expect("non-empty by construction"),
				BufferSwitchDirection::Next => active_tab_buffers[0],
			},
		};

		self.bind_buffer_to_window(active_window_id, target, true);
		self.clamp_window_cursors_for_buffer(target);
		Some(target)
	}

	pub fn replaceable_active_untitled_buffer_id(&self) -> Option<BufferId> {
		let active_buffer_id = self.active_buffer_id()?;
		let active_tab = self.tabs.get(&self.active_tab)?;
		if active_tab.buffer_order.as_slice() != [active_buffer_id] {
			return None;
		}
		let buffer = self.buffers.get(active_buffer_id)?;
		(buffer.path.is_none() && !buffer.dirty && !buffer.externally_modified).then_some(active_buffer_id)
	}

	pub fn prepare_buffer_for_open(&mut self, buffer_id: BufferId, path: PathBuf) {
		let Some(buffer) = self.buffers.get_mut(buffer_id) else {
			return;
		};
		buffer.path = Some(path.clone());
		buffer.name = buffer_name_from_path(&path).unwrap_or_else(|| "untitled".to_string());
		buffer.text = Rope::new();
		buffer.clean_text = Rope::new();
		buffer.dirty = false;
		buffer.externally_modified = false;
		buffer.undo_stack.clear();
		buffer.redo_stack.clear();
		self.pending_insert_group = self.pending_insert_group.take().filter(|group| group.buffer_id != buffer_id);
	}

	pub fn replace_buffer_text_preserving_cursor(&mut self, buffer_id: BufferId, text: String) -> bool {
		let is_active = self.active_buffer_id() == Some(buffer_id);
		let (previous_max_row, new_max_row, next_text) = {
			let Some(buffer) = self.buffers.get_mut(buffer_id) else {
				return false;
			};
			let previous_max_row = rope_line_count(&buffer.text) as u16;
			buffer.text = Rope::from_str(text.as_str());
			let next_text = buffer.text.clone();
			let new_max_row = rope_line_count(&next_text) as u16;
			(previous_max_row, new_max_row, next_text)
		};
		for ((_, saved_buffer_id), view) in &mut self.window_buffer_views {
			if *saved_buffer_id != buffer_id {
				continue;
			}
			if view.cursor.row >= previous_max_row {
				view.cursor.row = new_max_row;
			}
			view.cursor = clamp_cursor_for_rope(&next_text, view.cursor);
		}
		for (_, window) in &mut self.windows {
			if window.buffer_id != Some(buffer_id) {
				continue;
			}
			if window.cursor.row >= previous_max_row {
				window.cursor.row = new_max_row;
			} else {
				window.cursor = clamp_cursor_for_rope(&next_text, window.cursor);
			}
			window.cursor = clamp_cursor_for_rope(&next_text, window.cursor);
		}
		is_active
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
		if self
			.pending_insert_group
			.as_ref()
			.is_some_and(|group| group.buffer_id == buffer_id && group.edits.is_empty())
			&& let Some(buffer) = self.buffers.get_mut(buffer_id)
		{
			buffer.redo_stack.clear();
		}
		let Some(group) = self.pending_insert_group.as_mut() else {
			return false;
		};
		if group.buffer_id != buffer_id {
			return false;
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

	pub fn restore_buffer_persisted_history(
		&mut self,
		buffer_id: BufferId,
		persisted_history: PersistedBufferHistory,
		restore_view: bool,
	) -> bool {
		let Some(buffer) = self.buffers.get_mut(buffer_id) else {
			return false;
		};

		if buffer.text != persisted_history.current_text.as_str() {
			return false;
		}

		buffer.undo_stack = persisted_history.undo_stack;
		buffer.redo_stack = persisted_history.redo_stack;
		if restore_view {
			let persisted_cursor = clamp_cursor_for_rope(&buffer.text, persisted_history.cursor);
			for ((_, saved_buffer_id), view) in &mut self.window_buffer_views {
				if *saved_buffer_id == buffer_id {
					view.cursor = persisted_cursor;
				}
			}
			for (_, window) in &mut self.windows {
				if window.buffer_id == Some(buffer_id) {
					window.cursor = persisted_cursor;
				}
			}
		}
		if self.pending_insert_group.as_ref().is_some_and(|group| group.buffer_id == buffer_id) {
			self.pending_insert_group = None;
		}

		true
	}

	pub fn has_dirty_buffers(&self) -> bool { self.buffers.values().any(|buffer| buffer.dirty) }

	pub fn active_buffer_rope(&self) -> Option<&Rope> {
		let buffer_id = self.active_buffer_id()?;
		self.buffers.get(buffer_id).map(|buffer| &buffer.text)
	}

	pub fn clamp_window_cursors_for_buffer(&mut self, buffer_id: BufferId) {
		let Some(text) = self.buffers.get(buffer_id).map(|buffer| buffer.text.clone()) else {
			return;
		};
		for ((_, saved_buffer_id), view) in &mut self.window_buffer_views {
			if *saved_buffer_id == buffer_id {
				view.cursor = clamp_cursor_for_rope(&text, view.cursor);
			}
		}
		for (_, window) in &mut self.windows {
			if window.buffer_id == Some(buffer_id) {
				window.cursor = clamp_cursor_for_rope(&text, window.cursor);
			}
		}
	}

	pub fn cursor_for_buffer(&self, buffer_id: BufferId) -> Option<CursorState> {
		self
			.windows
			.values()
			.find(|window| window.buffer_id == Some(buffer_id))
			.map(|window| window.cursor)
			.or_else(|| {
				self
					.window_buffer_views
					.iter()
					.find_map(|((_, saved_buffer_id), view)| (*saved_buffer_id == buffer_id).then_some(view.cursor))
			})
	}

	pub fn active_buffer_text_string(&self) -> Option<String> {
		self.active_buffer_rope().map(ToString::to_string)
	}

	pub fn buffer_text_string(&self, buffer_id: BufferId) -> Option<String> {
		self.buffers.get(buffer_id).map(|buffer| buffer.text.to_string())
	}

	pub fn move_buffer_after(&mut self, buffer_id: BufferId, anchor_id: BufferId) {
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
