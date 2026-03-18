use std::path::PathBuf;

use ropey::Rope;
use serde::{Deserialize, Serialize};
use slotmap::new_key_type;

new_key_type! { pub struct BufferId; }
new_key_type! { pub struct WindowId; }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferState {
	pub name:                String,
	pub path:                Option<PathBuf>,
	pub text:                Rope,
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
pub struct RopeTextDiff {
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
	pub layout_x:  u32,
	pub layout_y:  u32,
	pub layout_w:  u32,
	pub layout_h:  u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowBufferViewState {
	pub cursor:   CursorState,
	pub scroll_x: u16,
	pub scroll_y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabState {
	pub windows:       Vec<WindowId>,
	pub active_window: WindowId,
	pub buffer_order:  Vec<BufferId>,
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

#[derive(Debug)]
pub struct PendingInsertUndoGroup {
	pub buffer_id:     BufferId,
	pub before_cursor: CursorState,
	pub edits:         Vec<BufferEditSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingBlockInsert {
	pub start_row:          u16,
	pub end_row:            u16,
	pub base_display_col:   u16,
	pub cursor_display_col: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSessionSnapshot {
	pub version:          u32,
	pub buffers:          Vec<WorkspaceBufferSnapshot>,
	pub buffer_order:     Vec<usize>,
	pub tabs:             Vec<WorkspaceTabSnapshot>,
	pub active_tab_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceBufferSnapshot {
	pub path:       Option<PathBuf>,
	pub text:       String,
	pub clean_text: String,
	#[serde(default)]
	pub history:    Option<WorkspaceBufferHistorySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBufferHistorySnapshot {
	pub undo_stack: Vec<BufferHistoryEntry>,
	pub redo_stack: Vec<BufferHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTabSnapshot {
	pub windows:             Vec<WorkspaceWindowSnapshot>,
	pub active_window_index: usize,
	#[serde(default)]
	pub buffer_order:        Vec<usize>,
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

#[derive(Deserialize)]
struct WorkspaceBufferSnapshotCompat {
	path:       Option<PathBuf>,
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
