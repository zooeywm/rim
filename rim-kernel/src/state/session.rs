use std::collections::HashMap;

use ropey::Rope;
use serde::Deserialize;
use slotmap::SlotMap;

use super::{BufferHistoryEntry, BufferState, EditorMode, RimState, StatusBarState, TabId, TabState, WindowBufferViewState, WindowState, WorkspaceBufferHistorySnapshot, WorkspaceBufferSnapshot, WorkspaceSessionSnapshot, WorkspaceTabSnapshot, WorkspaceWindowBufferViewSnapshot, WorkspaceWindowSnapshot, buffer_name_from_path, clamp_cursor_for_rope};

const WORKSPACE_SESSION_VERSION: u32 = 1;

impl RimState {
	pub fn workspace_session_snapshot(&self) -> WorkspaceSessionSnapshot {
		let mut buffer_ids = self
			.buffer_order
			.iter()
			.copied()
			.filter(|buffer_id| self.buffers.contains_key(*buffer_id))
			.collect::<Vec<_>>();
		for buffer_id in self.buffers.keys() {
			if !buffer_ids.contains(&buffer_id) {
				buffer_ids.push(buffer_id);
			}
		}

		let buffer_index_by_id =
			buffer_ids.iter().enumerate().map(|(index, buffer_id)| (*buffer_id, index)).collect::<HashMap<_, _>>();
		let buffers = buffer_ids
			.iter()
			.filter_map(|buffer_id| {
				let buffer = self.buffers.get(*buffer_id)?;
				let history = if buffer.path.is_none() {
					self.buffer_persisted_history_snapshot(*buffer_id).map(|history| WorkspaceBufferHistorySnapshot {
						undo_stack: history.undo_stack,
						redo_stack: history.redo_stack,
					})
				} else {
					None
				};
				Some(WorkspaceBufferSnapshot {
					path: buffer.path.clone(),
					text: buffer.text.to_string(),
					clean_text: buffer.clean_text.to_string(),
					history,
				})
			})
			.collect::<Vec<_>>();
		let buffer_order = self
			.buffer_order
			.iter()
			.filter_map(|buffer_id| buffer_index_by_id.get(buffer_id).copied())
			.collect::<Vec<_>>();
		let tab_items = self.tabs.iter().collect::<Vec<_>>();
		let active_tab_index = tab_items.iter().position(|(tab_id, _)| **tab_id == self.active_tab).unwrap_or(0);
		let tabs = tab_items
			.into_iter()
			.map(|(_, tab)| WorkspaceTabSnapshot {
				windows:             tab
					.windows
					.iter()
					.filter_map(|window_id| {
						let window = self.windows.get(*window_id)?;
						let mut views = self
							.window_buffer_views
							.iter()
							.filter_map(|((candidate_window_id, buffer_id), view)| {
								(*candidate_window_id == *window_id).then(|| {
									let buffer_index = buffer_index_by_id.get(buffer_id).copied()?;
									Some(WorkspaceWindowBufferViewSnapshot {
										buffer_index,
										cursor: view.cursor,
										scroll_x: view.scroll_x,
										scroll_y: view.scroll_y,
									})
								})?
							})
							.collect::<Vec<_>>();
						if let Some(buffer_id) = window.buffer_id
							&& let Some(buffer_index) = buffer_index_by_id.get(&buffer_id).copied()
						{
							views.retain(|view| view.buffer_index != buffer_index);
							views.push(WorkspaceWindowBufferViewSnapshot {
								buffer_index,
								cursor: window.cursor,
								scroll_x: window.scroll_x,
								scroll_y: window.scroll_y,
							});
						}
						views.sort_by_key(|view| view.buffer_index);
						Some(WorkspaceWindowSnapshot {
							buffer_index: window
								.buffer_id
								.and_then(|buffer_id| buffer_index_by_id.get(&buffer_id).copied()),
							x: window.x,
							y: window.y,
							width: window.width,
							height: window.height,
							views,
						})
					})
					.collect(),
				active_window_index: tab
					.windows
					.iter()
					.position(|window_id| *window_id == tab.active_window)
					.unwrap_or(0),
				buffer_order:        tab
					.buffer_order
					.iter()
					.filter_map(|buffer_id| buffer_index_by_id.get(buffer_id).copied())
					.collect(),
			})
			.collect::<Vec<_>>();

		WorkspaceSessionSnapshot {
			version: WORKSPACE_SESSION_VERSION,
			buffers,
			buffer_order,
			tabs,
			active_tab_index,
		}
	}

	pub fn restore_workspace_session(&mut self, snapshot: WorkspaceSessionSnapshot) -> bool {
		if snapshot.version != WORKSPACE_SESSION_VERSION || snapshot.tabs.is_empty() {
			return false;
		}

		self.mode = EditorMode::Normal;
		self.visual_anchor = None;
		self.command_line.clear();
		self.quit_after_save = false;
		self.pending_save_path = None;
		self.preferred_col = None;
		self.line_slot = None;
		self.line_slot_line_wise = false;
		self.line_slot_block_wise = false;
		self.normal_sequence.clear();
		self.visual_g_pending = false;
		self.pending_insert_group = None;
		self.pending_block_insert = None;
		self.pending_swap_decision = None;
		self.in_flight_internal_saves.clear();
		self.ignore_external_change_until.clear();
		self.window_buffer_views.clear();
		self.status_bar = StatusBarState::default();

		self.buffers = SlotMap::with_key();
		self.buffer_order.clear();
		self.windows = SlotMap::with_key();
		self.tabs.clear();

		let mut restored_buffer_ids = Vec::with_capacity(snapshot.buffers.len());
		for buffer_snapshot in snapshot.buffers {
			let history = buffer_snapshot
				.path
				.is_none()
				.then_some(buffer_snapshot.history)
				.flatten()
				.unwrap_or(WorkspaceBufferHistorySnapshot { undo_stack: Vec::new(), redo_stack: Vec::new() });
			let rope = Rope::from_str(buffer_snapshot.text.as_str());
			let clean_rope = Rope::from_str(buffer_snapshot.clean_text.as_str());
			let name = buffer_snapshot
				.path
				.as_deref()
				.and_then(buffer_name_from_path)
				.unwrap_or_else(|| "untitled".to_string());
			let buffer_id = self.buffers.insert(BufferState {
				name,
				path: buffer_snapshot.path,
				text: rope.clone(),
				clean_text: clean_rope,
				dirty: rope != buffer_snapshot.clean_text.as_str(),
				externally_modified: false,
				undo_stack: history.undo_stack,
				redo_stack: history.redo_stack,
			});
			restored_buffer_ids.push(buffer_id);
		}

		self.buffer_order = snapshot
			.buffer_order
			.into_iter()
			.filter_map(|index| restored_buffer_ids.get(index).copied())
			.collect::<Vec<_>>();
		for buffer_id in &restored_buffer_ids {
			if !self.buffer_order.contains(buffer_id) {
				self.buffer_order.push(*buffer_id);
			}
		}

		let mut tab_ids = Vec::new();
		for (tab_index, tab_snapshot) in snapshot.tabs.into_iter().enumerate() {
			let mut window_ids = Vec::new();
			for window_snapshot in tab_snapshot.windows {
				let buffer_id =
					window_snapshot.buffer_index.and_then(|index| restored_buffer_ids.get(index).copied());
				let current_view = window_snapshot.buffer_index.and_then(|buffer_index| {
					window_snapshot.views.iter().find(|view| view.buffer_index == buffer_index).copied()
				});
				let mut window = WindowState {
					buffer_id,
					x: window_snapshot.x,
					y: window_snapshot.y,
					width: window_snapshot.width.max(1),
					height: window_snapshot.height.max(1),
					..WindowState::default()
				};
				if let Some(view) = current_view {
					window.cursor = view.cursor;
					window.scroll_x = view.scroll_x;
					window.scroll_y = view.scroll_y;
				}
				let window_id = self.windows.insert(window);
				for view_snapshot in window_snapshot.views {
					let Some(buffer_id) = restored_buffer_ids.get(view_snapshot.buffer_index).copied() else {
						continue;
					};
					let clamped_cursor = self
						.buffers
						.get(buffer_id)
						.map(|buffer| clamp_cursor_for_rope(&buffer.text, view_snapshot.cursor))
						.unwrap_or(view_snapshot.cursor);
					self.window_buffer_views.insert((window_id, buffer_id), WindowBufferViewState {
						cursor:   clamped_cursor,
						scroll_x: view_snapshot.scroll_x,
						scroll_y: view_snapshot.scroll_y,
					});
				}
				if let Some(buffer_id) = buffer_id {
					let current_view =
						self.window_buffer_views.get(&(window_id, buffer_id)).copied().unwrap_or_default();
					if let Some(window) = self.windows.get_mut(window_id) {
						window.cursor = current_view.cursor;
						window.scroll_x = current_view.scroll_x;
						window.scroll_y = current_view.scroll_y;
					}
				}
				window_ids.push(window_id);
			}
			if window_ids.is_empty() {
				continue;
			}
			let tab_id = TabId(tab_index as u64 + 1);
			let active_window = window_ids
				.get(tab_snapshot.active_window_index.min(window_ids.len().saturating_sub(1)))
				.copied()
				.unwrap_or(window_ids[0]);
			let mut buffer_order = tab_snapshot
				.buffer_order
				.into_iter()
				.filter_map(|index| restored_buffer_ids.get(index).copied())
				.collect::<Vec<_>>();
			for window_id in &window_ids {
				let Some(buffer_id) = self.windows.get(*window_id).and_then(|window| window.buffer_id) else {
					continue;
				};
				if !buffer_order.contains(&buffer_id) {
					buffer_order.push(buffer_id);
				}
			}
			self.tabs.insert(tab_id, TabState { windows: window_ids, active_window, buffer_order });
			tab_ids.push(tab_id);
		}

		if self.tabs.is_empty() {
			return false;
		}
		self.active_tab = tab_ids
			.get(snapshot.active_tab_index.min(tab_ids.len().saturating_sub(1)))
			.copied()
			.unwrap_or(tab_ids[0]);
		self.status_bar.message = "session restored".to_string();
		true
	}

	pub fn has_restorable_workspace_session(&self) -> bool { !self.tabs.is_empty() }
}

#[derive(Deserialize)]
struct WorkspaceBufferSnapshotCompat {
	path:       Option<std::path::PathBuf>,
	text:       String,
	clean_text: String,
	#[serde(default)]
	history:    Option<WorkspaceBufferHistorySnapshot>,
	#[serde(default)]
	undo_stack: Vec<BufferHistoryEntry>,
	#[serde(default)]
	redo_stack: Vec<BufferHistoryEntry>,
}

impl<'de> Deserialize<'de> for WorkspaceBufferSnapshot {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where D: serde::Deserializer<'de> {
		let compat = WorkspaceBufferSnapshotCompat::deserialize(deserializer)?;
		let history = compat.history.or_else(|| {
			((compat.path.is_none()) && (!compat.undo_stack.is_empty() || !compat.redo_stack.is_empty())).then_some(
				WorkspaceBufferHistorySnapshot { undo_stack: compat.undo_stack, redo_stack: compat.redo_stack },
			)
		});
		Ok(Self { path: compat.path, text: compat.text, clean_text: compat.clean_text, history })
	}
}
