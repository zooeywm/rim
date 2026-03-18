use std::{path::PathBuf, time::Instant};

use rim_domain::editor::EditorOperationError;
use ropey::Rope;

use super::{BufferId, BufferSwitchDirection, PersistedBufferHistory, RimState, buffer_name_from_path};

impl RimState {
	pub(crate) fn remove_buffer_from_tab_orders(&mut self, buffer_id: BufferId) {
		self.editor.remove_buffer_from_tab_orders(buffer_id);
	}

	pub fn find_buffer_by_path(&self, path: &std::path::Path) -> Option<BufferId> {
		self.editor.find_buffer_by_path(path)
	}

	pub fn create_buffer(&mut self, path: Option<PathBuf>, text: impl Into<String>) -> BufferId {
		self.editor.create_buffer(path, text)
	}

	pub fn close_active_buffer(&mut self) {
		let Some(active_buffer_id) = self.active_buffer_id() else {
			self.workbench.status_bar.message = "buffer close failed: no active buffer".to_string();
			return;
		};
		let active_tab_id = self.active_tab;
		let _ = self.close_buffer_in_tab(active_tab_id, active_buffer_id);
	}

	pub fn close_buffer(&mut self, target_buffer_id: BufferId) {
		if !self.buffers.contains_key(target_buffer_id) {
			self.workbench.status_bar.message = "buffer close failed: target buffer missing".to_string();
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
		self.remove_buffer_from_tab_orders(target_buffer_id);
		self.workbench.in_flight_internal_saves.remove(&target_buffer_id);
		self.workbench.ignore_external_change_until.remove(&target_buffer_id);

		let _ = self.buffers.remove(target_buffer_id);
		if fallback.is_none() {
			fallback = Some(self.create_buffer(None, String::new()));
		}

		let rebound_window_ids = self
			.windows
			.iter()
			.filter_map(|(window_id, window)| (window.buffer_id == Some(target_buffer_id)).then_some(window_id))
			.collect::<Vec<_>>();
		self.window_buffer_views.retain(|(_, buffer_id), _| *buffer_id != target_buffer_id);
		if let Some(fallback_id) = fallback {
			for window_id in rebound_window_ids {
				self.bind_buffer_to_window(window_id, fallback_id, false);
			}
			self.clamp_window_cursors_for_buffer(fallback_id);
		}

		self.align_active_window_scroll_to_cursor();
		self.workbench.status_bar.message = "buffer closed".to_string();
	}

	pub fn close_active_buffer_and_report_global_removal(&mut self) -> Option<(BufferId, bool)> {
		let active_buffer_id = self.active_buffer_id()?;
		let active_tab_id = self.active_tab;
		let removed_globally = self.close_buffer_in_tab(active_tab_id, active_buffer_id);
		Some((active_buffer_id, removed_globally))
	}

	pub fn replaceable_active_untitled_buffer_id(&self) -> Option<BufferId> {
		self.editor.replaceable_active_untitled_buffer_id()
	}

	pub fn prepare_buffer_for_open(&mut self, buffer_id: BufferId, path: PathBuf) {
		self.editor.prepare_buffer_for_open(buffer_id, path);
	}

	pub fn detach_buffer_from_active_tab_and_try_remove(&mut self, buffer_id: BufferId) -> bool {
		let active_tab = self.active_tab;
		if let Some(tab) = self.tabs.get_mut(&active_tab) {
			tab.buffer_order.retain(|id| *id != buffer_id);
		}
		self.try_remove_buffer_globally(buffer_id)
	}

	fn close_buffer_in_tab(&mut self, tab_id: super::TabId, target_buffer_id: BufferId) -> bool {
		if !self.buffers.contains_key(target_buffer_id) {
			self.workbench.status_bar.message = "buffer close failed: target buffer missing".to_string();
			return false;
		}

		let window_ids = self.tabs.get(&tab_id).map(|tab| tab.windows.clone()).unwrap_or_default();
		let tab_buffer_order_before =
			self.tabs.get(&tab_id).map(|tab| tab.buffer_order.clone()).unwrap_or_default();
		let fallback = match tab_buffer_order_before.iter().position(|id| *id == target_buffer_id) {
			Some(idx) if tab_buffer_order_before.len() > 1 => {
				if idx > 0 {
					Some(tab_buffer_order_before[idx - 1])
				} else {
					Some(tab_buffer_order_before[1])
				}
			}
			_ => None,
		};

		if let Some(tab) = self.tabs.get_mut(&tab_id) {
			tab.buffer_order.retain(|id| *id != target_buffer_id);
		}

		let rebound_window_ids = window_ids
			.into_iter()
			.filter(|window_id| {
				self.windows.get(*window_id).is_some_and(|window| window.buffer_id == Some(target_buffer_id))
			})
			.collect::<Vec<_>>();

		self.window_buffer_views.retain(|(window_id, buffer_id), _| {
			!(*buffer_id == target_buffer_id && rebound_window_ids.contains(window_id))
		});

		let fallback_id = fallback.unwrap_or_else(|| self.create_buffer(None, String::new()));
		for window_id in rebound_window_ids {
			self.bind_buffer_to_window(window_id, fallback_id, false);
		}
		self.clamp_window_cursors_for_buffer(fallback_id);

		let removed_globally = self.try_remove_buffer_globally(target_buffer_id);
		self.align_active_window_scroll_to_cursor();
		self.workbench.status_bar.message = "buffer closed".to_string();
		removed_globally
	}

	fn try_remove_buffer_globally(&mut self, target_buffer_id: BufferId) -> bool {
		let still_visible_in_tab = self.tabs.values().any(|tab| tab.buffer_order.contains(&target_buffer_id));
		let still_bound_to_window =
			self.windows.values().any(|window| window.buffer_id == Some(target_buffer_id));
		if still_visible_in_tab || still_bound_to_window {
			return false;
		}

		self.buffer_order.retain(|id| *id != target_buffer_id);
		self.workbench.in_flight_internal_saves.remove(&target_buffer_id);
		self.workbench.ignore_external_change_until.remove(&target_buffer_id);
		self.window_buffer_views.retain(|(_, buffer_id), _| *buffer_id != target_buffer_id);
		let _ = self.buffers.remove(target_buffer_id);
		true
	}

	pub fn replace_buffer_text_preserving_cursor(&mut self, buffer_id: BufferId, text: String) {
		if self.editor.replace_buffer_text_preserving_cursor(buffer_id, text) {
			self.align_active_window_scroll_to_cursor();
		}
	}

	pub fn create_untitled_buffer(&mut self) -> BufferId {
		let buffer_id = self.editor.create_untitled_buffer();
		self.workbench.status_bar.message = "new buffer".to_string();
		buffer_id
	}

	pub fn switch_active_window_buffer(&mut self, direction: BufferSwitchDirection) {
		let Some(target) = self.editor.switch_active_window_buffer(direction) else {
			return;
		};
		self.align_active_window_scroll_to_cursor();
		if let Some(buffer) = self.buffers.get(target) {
			self.workbench.status_bar.message = format!("buffer {}", buffer.name);
		}
	}

	pub fn active_buffer_save_snapshot(
		&self,
		path_override: Option<PathBuf>,
	) -> Result<(BufferId, PathBuf, String), &'static str> {
		self.editor.active_buffer_save_snapshot(path_override)
	}

	pub fn active_buffer_load_target(&self) -> Result<(BufferId, PathBuf), &'static str> {
		self.editor.active_buffer_load_target()
	}

	pub fn active_buffer_has_path(&self) -> Option<bool> { self.editor.active_buffer_has_path() }

	pub fn all_buffer_save_snapshots(&self) -> (Vec<(BufferId, PathBuf, String)>, usize) {
		self.editor.all_buffer_save_snapshots()
	}

	pub fn set_pending_save_path(&mut self, buffer_id: BufferId, path: Option<PathBuf>) {
		self.workbench.pending_save_path = path.map(|p| (buffer_id, p));
	}

	pub fn apply_pending_save_path_if_matches(&mut self, buffer_id: BufferId) {
		let Some((pending_buffer_id, path)) = self.workbench.pending_save_path.clone() else {
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
		self.workbench.pending_save_path = None;
	}

	pub fn clear_pending_save_path_if_matches(&mut self, buffer_id: BufferId) {
		if let Some((pending_buffer_id, _)) = self.workbench.pending_save_path
			&& pending_buffer_id == buffer_id
		{
			self.workbench.pending_save_path = None;
		}
	}

	pub fn set_buffer_dirty(&mut self, buffer_id: BufferId, dirty: bool) {
		self.editor.set_buffer_dirty(buffer_id, dirty);
	}

	pub fn mark_buffer_clean(&mut self, buffer_id: BufferId) { self.editor.mark_buffer_clean(buffer_id); }

	pub fn set_buffer_externally_modified(&mut self, buffer_id: BufferId, externally_modified: bool) {
		self.editor.set_buffer_externally_modified(buffer_id, externally_modified);
	}

	pub fn clear_buffer_history(&mut self, buffer_id: BufferId) { self.editor.clear_buffer_history(buffer_id); }

	pub fn mark_recent_internal_save(&mut self, buffer_id: BufferId) {
		self
			.workbench
			.ignore_external_change_until
			.insert(buffer_id, Instant::now() + Self::INTERNAL_SAVE_WATCHER_IGNORE_WINDOW);
	}

	pub fn clear_recent_internal_save(&mut self, buffer_id: BufferId) {
		self.workbench.ignore_external_change_until.remove(&buffer_id);
	}

	pub fn should_ignore_recent_external_change(&mut self, buffer_id: BufferId) -> bool {
		let Some(deadline) = self.workbench.ignore_external_change_until.get(&buffer_id).copied() else {
			return false;
		};
		if Instant::now() <= deadline {
			return true;
		}
		self.workbench.ignore_external_change_until.remove(&buffer_id);
		false
	}

	pub fn active_buffer_is_externally_modified(&self) -> Option<bool> {
		self.editor.active_buffer_is_externally_modified()
	}

	pub fn mark_active_buffer_dirty(&mut self) { self.editor.mark_active_buffer_dirty(); }

	pub fn refresh_buffer_dirty(&mut self, buffer_id: BufferId) { self.editor.refresh_buffer_dirty(buffer_id); }

	pub fn apply_edit_entry(
		&mut self,
		buffer_id: BufferId,
		entry: super::BufferHistoryEntry,
		mode_before: super::EditorMode,
	) {
		self.editor.apply_edit_entry(buffer_id, entry, mode_before);
	}

	pub fn record_history_from_text_diff(
		&mut self,
		buffer_id: BufferId,
		before_text: &Rope,
		before_cursor: super::CursorState,
		mode_before: super::EditorMode,
		skip_history: bool,
	) {
		self.editor.record_history_from_text_diff(
			buffer_id,
			before_text,
			before_cursor,
			mode_before,
			skip_history,
		);
	}

	pub fn push_buffer_history_entry(&mut self, buffer_id: BufferId, entry: super::BufferHistoryEntry) {
		self.editor.push_buffer_history_entry(buffer_id, entry);
	}

	pub fn buffer_persisted_history_snapshot(&self, buffer_id: BufferId) -> Option<PersistedBufferHistory> {
		self.editor.buffer_persisted_history_snapshot(buffer_id)
	}

	pub fn restore_buffer_persisted_history(
		&mut self,
		buffer_id: BufferId,
		persisted_history: PersistedBufferHistory,
		restore_view: bool,
	) -> bool {
		let is_active = self.active_buffer_id() == Some(buffer_id);
		if self.editor.restore_buffer_persisted_history(buffer_id, persisted_history, restore_view) {
			if restore_view && is_active {
				self.align_active_window_scroll_to_cursor();
			}
			true
		} else {
			false
		}
	}

	pub fn all_file_backed_persisted_history_snapshots(
		&self,
	) -> Vec<(BufferId, PathBuf, PersistedBufferHistory)> {
		self.editor.all_file_backed_persisted_history_snapshots()
	}

	pub fn undo_active_buffer_edit(&mut self) {
		match self.editor.undo_active_buffer_edit() {
			Ok(()) => {
				self.align_active_window_scroll_to_cursor();
				self.workbench.status_bar.message = "undo".to_string();
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "undo failed: no active buffer".to_string();
			}
			Err(EditorOperationError::ActiveBufferMissing) => {
				self.workbench.status_bar.message = "undo failed: active buffer missing".to_string();
			}
			Err(EditorOperationError::NothingToUndo) => {
				self.workbench.status_bar.message = "undo: nothing to undo".to_string();
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("undo failed: {:?}", other);
			}
		}
	}

	pub fn redo_active_buffer_edit(&mut self) {
		match self.editor.redo_active_buffer_edit() {
			Ok(()) => {
				self.align_active_window_scroll_to_cursor();
				self.workbench.status_bar.message = "redo".to_string();
			}
			Err(EditorOperationError::NoActiveBuffer) => {
				self.workbench.status_bar.message = "redo failed: no active buffer".to_string();
			}
			Err(EditorOperationError::ActiveBufferMissing) => {
				self.workbench.status_bar.message = "redo failed: active buffer missing".to_string();
			}
			Err(EditorOperationError::NothingToRedo) => {
				self.workbench.status_bar.message = "redo: nothing to redo".to_string();
			}
			Err(other) => {
				self.workbench.status_bar.message = format!("redo failed: {:?}", other);
			}
		}
	}

	pub fn has_dirty_buffers(&self) -> bool { self.editor.has_dirty_buffers() }

	pub fn active_buffer_rope(&self) -> Option<&Rope> { self.editor.active_buffer_rope() }

	pub(crate) fn clamp_window_cursors_for_buffer(&mut self, buffer_id: BufferId) {
		self.editor.clamp_window_cursors_for_buffer(buffer_id);
	}

	pub(crate) fn cursor_for_buffer(&self, buffer_id: BufferId) -> Option<super::CursorState> {
		self.editor.cursor_for_buffer(buffer_id)
	}

	pub fn active_buffer_text_string(&self) -> Option<String> { self.editor.active_buffer_text_string() }

	pub fn buffer_text_string(&self, buffer_id: BufferId) -> Option<String> {
		self.editor.buffer_text_string(buffer_id)
	}
}
