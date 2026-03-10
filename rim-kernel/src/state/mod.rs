use std::{collections::{BTreeMap, HashMap, HashSet}, fmt, path::{Path, PathBuf}, time::{Duration, Instant}};

use ropey::Rope;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};
use tracing::error;

use crate::command::{CommandConfigFile, CommandRegistry, PluginCommandRegistration};

mod buffer;
mod edit;
mod mode;
mod session;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct WindowBufferViewState {
	pub cursor:   CursorState,
	pub scroll_x: u16,
	pub scroll_y: u16,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSessionSnapshot {
	pub version:          u32,
	pub buffers:          Vec<WorkspaceBufferSnapshot>,
	pub buffer_order:     Vec<usize>,
	pub tabs:             Vec<WorkspaceTabSnapshot>,
	pub active_tab_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBufferSnapshot {
	pub path:       Option<PathBuf>,
	pub text:       String,
	pub clean_text: String,
	pub undo_stack: Vec<BufferHistoryEntry>,
	pub redo_stack: Vec<BufferHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTabSnapshot {
	pub windows:             Vec<WorkspaceWindowSnapshot>,
	pub active_window_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceWindowSnapshot {
	pub buffer_index: Option<usize>,
	pub x:            u16,
	pub y:            u16,
	pub width:        u16,
	pub height:       u16,
	pub views:        Vec<WorkspaceWindowBufferViewSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceWindowBufferViewSnapshot {
	pub buffer_index: usize,
	pub cursor:       CursorState,
	pub scroll_x:     u16,
	pub scroll_y:     u16,
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
	pub command_registry:             CommandRegistry,
	pub(crate) window_buffer_views:   HashMap<(WindowId, BufferId), WindowBufferViewState>,
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
			command_registry: CommandRegistry::with_defaults(),
			window_buffer_views: HashMap::new(),
			buffers,
			buffer_order: Vec::new(),
			windows,
			tabs,
			status_bar: StatusBarState::default(),
		}
	}

	pub fn apply_command_config(&mut self, config: &CommandConfigFile) -> Vec<String> {
		self.command_registry.apply_config(config)
	}

	pub fn register_plugin_command(&mut self, registration: PluginCommandRegistration) -> Result<(), String> {
		self.command_registry.register_plugin_command(registration)
	}

	pub fn create_window(&mut self, buffer_id: Option<BufferId>) -> Option<WindowId> {
		if let Some(buffer_id) = buffer_id
			&& !self.buffers.contains_key(buffer_id)
		{
			error!("create_window failed: buffer {:?} not found", buffer_id);
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

	pub(crate) fn bind_buffer_to_window(
		&mut self,
		window_id: WindowId,
		buffer_id: BufferId,
		persist_previous_cursor: bool,
	) {
		let Some(window_snapshot) = self.windows.get(window_id).copied() else {
			return;
		};
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
	}

	pub(crate) fn sync_window_view_binding(&mut self, window_id: WindowId) {
		let Some(window) = self.windows.get(window_id) else {
			return;
		};
		let Some(buffer_id) = window.buffer_id else {
			return;
		};
		self.window_buffer_views.insert((window_id, buffer_id), WindowBufferViewState {
			cursor:   window.cursor,
			scroll_x: window.scroll_x,
			scroll_y: window.scroll_y,
		});
	}

	pub(crate) fn remove_window_view_bindings(&mut self, window_id: WindowId) {
		self.window_buffer_views.retain(|(candidate_window_id, _), _| *candidate_window_id != window_id);
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
