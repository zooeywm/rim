use std::{collections::{BTreeMap, HashMap, HashSet}, fmt, path::{Path, PathBuf}, time::{Duration, Instant}};

use ropey::Rope;
use slotmap::{Key, SlotMap, new_key_type};
use tracing::error;

mod buffer;
mod edit;
mod tab;
mod window;

new_key_type! { pub struct BufferId; }
new_key_type! { pub struct WindowId; }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferState {
	pub name:                String,
	pub path:                Option<PathBuf>,
	pub text:                Rope,
	// This is the last clean snapshot loaded from or saved to disk.
	pub clean_text:          Rope,
	pub dirty:               bool,
	pub externally_modified: bool,
	pub undo_stack:          Vec<BufferHistoryEntry>,
	pub redo_stack:          Vec<BufferHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferHistoryEntry {
	pub edits:         Vec<BufferEditSnapshot>,
	pub before_cursor: CursorState,
	pub after_cursor:  CursorState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedBufferHistory {
	pub current_text: String,
	pub cursor:       CursorState,
	pub undo_stack:   Vec<BufferHistoryEntry>,
	pub redo_stack:   Vec<BufferHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferEditSnapshot {
	pub start_byte:    usize,
	pub deleted_text:  String,
	pub inserted_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RopeTextDiff {
	pub start_char:    usize,
	pub start_byte:    usize,
	pub deleted_text:  String,
	pub inserted_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowState {
	pub buffer_id: Option<BufferId>,
	pub cursor:    CursorState,
	pub scroll_x:  u16,
	pub scroll_y:  u16,
	pub x:         u16,
	pub y:         u16,
	pub width:     u16,
	pub height:    u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabState {
	pub windows:       Vec<WindowId>,
	pub active_window: WindowId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
	pub mode:         StatusBarMode,
	pub message:      String,
	pub key_sequence: String,
}

impl Default for StatusBarState {
	fn default() -> Self {
		Self {
			mode:         StatusBarMode::Normal,
			message:      "new file".to_string(),
			key_sequence: String::new(),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBarMode {
	Normal,
	Insert,
	InsertBlock,
	Command,
	Visual,
	VisualLine,
	VisualBlock,
}

impl fmt::Display for StatusBarMode {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let label = match self {
			Self::Normal => "NORMAL",
			Self::Insert => "INSERT",
			Self::InsertBlock => "INSERT BLOCK",
			Self::Command => "COMMAND",
			Self::Visual => "VISUAL",
			Self::VisualLine => "VISUAL LINE",
			Self::VisualBlock => "VISUAL BLOCK",
		};
		f.write_str(label)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorState {
	pub row: u16,
	pub col: u16,
}

impl Default for CursorState {
	fn default() -> Self { Self { row: 1, col: 1 } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
	Normal,
	Insert,
	Command,
	VisualChar,
	VisualLine,
	VisualBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitAxis {
	Horizontal,
	Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalSequenceKey {
	Leader,
	Tab,
	Char(char),
	Ctrl(char),
}

#[derive(Debug)]
pub struct PendingInsertUndoGroup {
	pub buffer_id:     BufferId,
	pub before_cursor: CursorState,
	pub edits:         Vec<BufferEditSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingBlockInsert {
	pub start_row: u16,
	pub end_row:   u16,
	pub base_col:  u16,
}

#[derive(Debug, Clone)]
pub struct PendingSwapDecision {
	pub buffer_id:      BufferId,
	pub source_path:    PathBuf,
	pub base_text:      String,
	pub owner_pid:      u32,
	pub owner_username: String,
}

#[derive(Debug)]
pub struct RimState {
	pub title:                        String,
	pub active_tab:                   TabId,
	pub leader_key:                   char,
	pub mode:                         EditorMode,
	pub visual_anchor:                Option<CursorState>,
	pub command_line:                 String,
	pub quit_after_save:              bool,
	pub pending_save_path:            Option<(BufferId, PathBuf)>,
	pub preferred_col:                Option<u16>,
	pub line_slot:                    Option<String>,
	pub line_slot_line_wise:          bool,
	pub line_slot_block_wise:         bool,
	pub cursor_scroll_threshold:      u16,
	pub normal_sequence:              Vec<NormalSequenceKey>,
	pub visual_g_pending:             bool,
	pub pending_insert_group:         Option<PendingInsertUndoGroup>,
	pub pending_block_insert:         Option<PendingBlockInsert>,
	pub pending_swap_decision:        Option<PendingSwapDecision>,
	pub in_flight_internal_saves:     HashSet<BufferId>,
	pub ignore_external_change_until: HashMap<BufferId, Instant>,
	pub buffers:                      SlotMap<BufferId, BufferState>,
	pub buffer_order:                 Vec<BufferId>,
	pub windows:                      SlotMap<WindowId, WindowState>,
	pub tabs:                         BTreeMap<TabId, TabState>,
	pub status_bar:                   StatusBarState,
}

impl RimState {
	const INTERNAL_SAVE_WATCHER_IGNORE_WINDOW: Duration = Duration::from_millis(750);
	const MAX_HISTORY_ENTRIES: usize = 256;

	pub fn new() -> Self {
		let buffers = SlotMap::with_key();
		let mut windows = SlotMap::with_key();
		let mut tabs = BTreeMap::new();

		let tab_id = TabId(1);
		let window_id = windows.insert(WindowState::default());

		tabs.insert(tab_id, TabState { windows: vec![window_id], active_window: window_id });

		Self {
			title: "Rim".to_string(),
			active_tab: tab_id,
			leader_key: ' ',
			mode: EditorMode::Normal,
			visual_anchor: None,
			command_line: String::new(),
			quit_after_save: false,
			pending_save_path: None,
			preferred_col: None,
			line_slot: None,
			line_slot_line_wise: false,
			line_slot_block_wise: false,
			cursor_scroll_threshold: 0,
			normal_sequence: Vec::new(),
			visual_g_pending: false,
			pending_insert_group: None,
			pending_block_insert: None,
			pending_swap_decision: None,
			in_flight_internal_saves: HashSet::new(),
			ignore_external_change_until: HashMap::new(),
			buffers,
			buffer_order: Vec::new(),
			windows,
			tabs,
			status_bar: StatusBarState::default(),
		}
	}

	pub fn create_window(&mut self, buffer_id: Option<BufferId>) -> Option<WindowId> {
		if let Some(buffer_id) = buffer_id
			&& !self.buffers.contains_key(buffer_id)
		{
			error!("create_window failed: buffer {:?} not found", buffer_id);
			return None;
		}

		let id = self.windows.insert(WindowState { buffer_id, ..WindowState::default() });
		Some(id)
	}

	pub fn status_line(&self) -> String {
		let cursor = self.active_cursor();
		let total_rows = self
			.active_buffer_id()
			.and_then(|buffer_id| self.buffers.get(buffer_id))
			.map(|buffer| rope_line_count(&buffer.text) as u16)
			.unwrap_or(1);
		let progress = if cursor.row <= 1 {
			"Top".to_string()
		} else if cursor.row >= total_rows {
			"Bot".to_string()
		} else {
			let percent = (u32::from(cursor.row) * 100 / u32::from(total_rows)) as u16;
			format!("{}%", percent)
		};
		let cursor_pos = format!("{}:{} {}", cursor.row, cursor.col, progress);

		if self.mode == EditorMode::Command {
			return format!(":{} | {}", self.command_line, cursor_pos);
		}
		if self.status_bar.key_sequence.is_empty() {
			return format!("{} | {}", self.status_bar.message, cursor_pos);
		}

		format!("{} | keys {} | {}", self.status_bar.message, self.status_bar.key_sequence, cursor_pos)
	}

	pub fn active_buffer_id(&self) -> Option<BufferId> {
		self.windows.get(self.active_window_id()).and_then(|window| window.buffer_id)
	}

	pub fn bind_buffer_to_active_window(&mut self, buffer_id: BufferId) {
		let active_window_id = self.active_window_id();
		let window = self.windows.get_mut(active_window_id).expect("invariant: active window id must exist");
		window.buffer_id = Some(buffer_id);
		if let Some(buffer) = self.buffers.get(buffer_id) {
			window.cursor = clamp_cursor_for_rope(&buffer.text, window.cursor);
		}
	}

	pub fn is_insert_mode(&self) -> bool { self.mode == EditorMode::Insert }

	pub fn is_command_mode(&self) -> bool { self.mode == EditorMode::Command }

	pub fn is_visual_mode(&self) -> bool {
		matches!(self.mode, EditorMode::VisualChar | EditorMode::VisualLine | EditorMode::VisualBlock)
	}

	pub fn is_visual_line_mode(&self) -> bool { self.mode == EditorMode::VisualLine }

	pub fn is_visual_block_mode(&self) -> bool { self.mode == EditorMode::VisualBlock }

	pub fn is_block_insert_mode(&self) -> bool {
		self.mode == EditorMode::Insert && self.pending_block_insert.is_some()
	}

	pub fn enter_insert_mode(&mut self) {
		self.mode = EditorMode::Insert;
		self.visual_anchor = None;
		self.pending_block_insert = None;
		self.status_bar.mode = StatusBarMode::Insert;
	}

	pub fn enter_block_insert_mode(&mut self, pending: PendingBlockInsert) {
		self.mode = EditorMode::Insert;
		self.visual_anchor = None;
		self.pending_block_insert = Some(pending);
		self.status_bar.mode = StatusBarMode::InsertBlock;
	}

	pub fn exit_insert_mode(&mut self) {
		self.mode = EditorMode::Normal;
		self.visual_anchor = None;
		self.pending_block_insert = None;
		self.status_bar.mode = StatusBarMode::Normal;
		self.clamp_cursor_to_navigable_col();
	}

	pub fn enter_command_mode(&mut self) {
		self.mode = EditorMode::Command;
		self.visual_anchor = None;
		self.command_line.clear();
		self.status_bar.mode = StatusBarMode::Command;
	}

	pub fn exit_command_mode(&mut self) {
		self.mode = EditorMode::Normal;
		self.visual_anchor = None;
		self.command_line.clear();
		self.status_bar.mode = StatusBarMode::Normal;
	}

	pub fn enter_visual_mode(&mut self) {
		self.mode = EditorMode::VisualChar;
		if self.visual_anchor.is_none() {
			self.visual_anchor = Some(self.active_cursor());
		}
		self.status_bar.mode = StatusBarMode::Visual;
	}

	pub fn enter_visual_line_mode(&mut self) {
		let anchor_row = self.visual_anchor.map(|cursor| cursor.row).unwrap_or_else(|| self.active_cursor().row);
		self.mode = EditorMode::VisualLine;
		self.visual_anchor = Some(CursorState { row: anchor_row, col: 1 });
		self.status_bar.mode = StatusBarMode::VisualLine;
	}

	pub fn enter_visual_block_mode(&mut self) {
		self.mode = EditorMode::VisualBlock;
		if self.visual_anchor.is_none() {
			self.visual_anchor = Some(self.active_cursor());
		}
		self.status_bar.mode = StatusBarMode::VisualBlock;
	}

	pub fn exit_visual_mode(&mut self) {
		self.mode = EditorMode::Normal;
		self.visual_anchor = None;
		self.status_bar.mode = StatusBarMode::Normal;
	}

	pub fn push_command_char(&mut self, ch: char) { self.command_line.push(ch); }

	pub fn pop_command_char(&mut self) { let _ = self.command_line.pop(); }

	pub fn take_command_line(&mut self) -> String {
		let command = self.command_line.trim().to_string();
		self.exit_command_mode();
		command
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

	pub fn set_pending_swap_decision(&mut self, pending: PendingSwapDecision) {
		self.pending_swap_decision = Some(pending);
	}

	pub fn take_pending_swap_decision(&mut self) -> Option<PendingSwapDecision> {
		self.pending_swap_decision.take()
	}

	pub fn begin_insert_history_group(&mut self) {
		if self.pending_insert_group.is_some() {
			return;
		}
		let Some(buffer_id) = self.active_buffer_id() else {
			return;
		};
		self.pending_insert_group =
			Some(PendingInsertUndoGroup { buffer_id, before_cursor: self.active_cursor(), edits: Vec::new() });
	}

	pub fn cancel_insert_history_group(&mut self) { self.pending_insert_group = None; }

	pub fn commit_insert_history_group(&mut self) {
		let Some(group) = self.pending_insert_group.take() else {
			return;
		};
		if group.edits.is_empty() {
			return;
		}
		let after_cursor = self.cursor_for_buffer(group.buffer_id).unwrap_or(group.before_cursor);
		self.push_buffer_history_entry(group.buffer_id, BufferHistoryEntry {
			edits: group.edits,
			before_cursor: group.before_cursor,
			after_cursor,
		});
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
}

impl Default for RimState {
	fn default() -> Self { Self::new() }
}

fn buffer_name_from_path(path: &Path) -> Option<String> {
	path.file_name().map(|name| name.to_string_lossy().to_string())
}

pub(crate) fn rope_line_count(text: &Rope) -> usize {
	let line_count = text.len_lines();
	if line_count == 0 {
		return 1;
	}
	if rope_ends_with_newline(text) { line_count.saturating_sub(1).max(1) } else { line_count.max(1) }
}

pub(crate) fn rope_is_empty(text: &Rope) -> bool { text.len_chars() == 0 }

pub(crate) fn rope_line_without_newline(text: &Rope, row_index: usize) -> Option<String> {
	if row_index >= rope_line_count(text) {
		return None;
	}
	let mut line = text.line(row_index).to_string();
	if line.ends_with('\n') {
		line.pop();
		if line.ends_with('\r') {
			line.pop();
		}
	}
	Some(line)
}

pub(crate) fn rope_line_len_chars(text: &Rope, row_index: usize) -> usize {
	rope_line_without_newline(text, row_index).map(|line| line.chars().count()).unwrap_or(0)
}

pub(crate) fn rope_ends_with_newline(text: &Rope) -> bool {
	text.len_chars() > 0 && text.char(text.len_chars().saturating_sub(1)) == '\n'
}

pub(crate) fn clamp_cursor_for_rope(text: &Rope, cursor: CursorState) -> CursorState {
	let max_row = rope_line_count(text) as u16;
	let row = cursor.row.min(max_row).max(1);
	let row_index = row.saturating_sub(1) as usize;
	let max_col = rope_line_len_chars(text, row_index).max(1) as u16;
	let col = cursor.col.min(max_col).max(1);
	CursorState { row, col }
}

fn apply_text_delta_undo(text: &mut Rope, delta: &BufferEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let inserted_end_byte = delta.start_byte.saturating_add(delta.inserted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(inserted_end_byte);
	text.remove(start_char..end_char);
	text.insert(start_char, delta.deleted_text.as_str());
}

fn apply_text_delta_redo(text: &mut Rope, delta: &BufferEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let deleted_end_byte = delta.start_byte.saturating_add(delta.deleted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(deleted_end_byte);
	text.remove(start_char..end_char);
	text.insert(start_char, delta.inserted_text.as_str());
}

fn merge_adjacent_insert_history_edits(
	last_edit: &mut BufferEditSnapshot,
	next_edit: &BufferEditSnapshot,
) -> bool {
	if !last_edit.deleted_text.is_empty() || !next_edit.deleted_text.is_empty() {
		return false;
	}
	let expected_start = last_edit.start_byte.saturating_add(last_edit.inserted_text.len());
	if next_edit.start_byte != expected_start {
		return false;
	}
	last_edit.inserted_text.push_str(next_edit.inserted_text.as_str());
	true
}

pub(crate) fn compute_rope_text_diff(before: &Rope, after: &Rope) -> Option<RopeTextDiff> {
	if before == after {
		return None;
	}

	let mut common_prefix_chars = 0usize;
	let mut common_prefix_bytes = 0usize;
	for (before_ch, after_ch) in before.chars().zip(after.chars()) {
		if before_ch != after_ch {
			break;
		}
		common_prefix_chars = common_prefix_chars.saturating_add(1);
		common_prefix_bytes = common_prefix_bytes.saturating_add(before_ch.len_utf8());
	}

	let before_len_chars = before.len_chars();
	let after_len_chars = after.len_chars();
	let mut common_suffix_chars = 0usize;
	let mut before_mid_end = before_len_chars;
	let mut after_mid_end = after_len_chars;
	while before_mid_end > common_prefix_chars && after_mid_end > common_prefix_chars {
		if before.char(before_mid_end.saturating_sub(1)) != after.char(after_mid_end.saturating_sub(1)) {
			break;
		}
		common_suffix_chars = common_suffix_chars.saturating_add(1);
		before_mid_end = before_mid_end.saturating_sub(1);
		after_mid_end = after_mid_end.saturating_sub(1);
	}

	Some(RopeTextDiff {
		start_char:    common_prefix_chars,
		start_byte:    common_prefix_bytes,
		deleted_text:  before.slice(common_prefix_chars..before_mid_end).to_string(),
		inserted_text: after.slice(common_prefix_chars..after_mid_end).to_string(),
	})
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDirection {
	Left,
	Down,
	Up,
	Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSwitchDirection {
	Prev,
	Next,
}

#[cfg(test)]
mod tests;
