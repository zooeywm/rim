mod buffer;
mod core;
mod edit;
mod movement;
mod session;
mod tab;
mod visual;
mod window;

use std::collections::{BTreeMap, HashMap};

use slotmap::SlotMap;

use crate::model::{BufferId, BufferState, CursorState, EditorMode, PendingBlockInsert, PendingInsertUndoGroup, TabId, TabState, WindowBufferViewState, WindowId, WindowState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorOperationError {
	NoActiveBuffer,
	ActiveBufferMissing,
	NoAnchor,
	OutOfRange,
	EmptySelection,
	NoChar,
	SlotEmpty,
	NothingToUndo,
	NothingToRedo,
}

#[derive(Debug)]
pub struct EditorState {
	pub active_tab:                      TabId,
	pub mode:                            EditorMode,
	pub visual_anchor:                   Option<CursorState>,
	pub visual_block_anchor_display_col: Option<u16>,
	pub visual_block_cursor_display_col: Option<u16>,
	pub preferred_col:                   Option<u16>,
	pub line_slot:                       Option<String>,
	pub line_slot_line_wise:             bool,
	pub line_slot_block_wise:            bool,
	pub pending_insert_group:            Option<PendingInsertUndoGroup>,
	pub pending_block_insert:            Option<PendingBlockInsert>,
	pub window_buffer_views:             HashMap<(WindowId, BufferId), WindowBufferViewState>,
	pub buffers:                         SlotMap<BufferId, BufferState>,
	pub buffer_order:                    Vec<BufferId>,
	pub windows:                         SlotMap<WindowId, WindowState>,
	pub tabs:                            BTreeMap<TabId, TabState>,
}

impl EditorState {
	pub fn new() -> Self {
		let mut state = Self::empty();
		let window_id = state.windows.insert(WindowState::default());
		let tab_id = TabId(1);
		state.tabs.insert(tab_id, TabState {
			windows:       vec![window_id],
			active_window: window_id,
			buffer_order:  Vec::new(),
		});
		state.active_tab = tab_id;
		state
	}

	pub fn empty() -> Self {
		Self {
			active_tab:                      TabId(1),
			mode:                            EditorMode::Normal,
			visual_anchor:                   None,
			visual_block_anchor_display_col: None,
			visual_block_cursor_display_col: None,
			preferred_col:                   None,
			line_slot:                       None,
			line_slot_line_wise:             false,
			line_slot_block_wise:            false,
			pending_insert_group:            None,
			pending_block_insert:            None,
			window_buffer_views:             HashMap::new(),
			buffers:                         SlotMap::with_key(),
			buffer_order:                    Vec::new(),
			windows:                         SlotMap::with_key(),
			tabs:                            BTreeMap::new(),
		}
	}

	pub fn reset_runtime_state(&mut self) {
		self.mode = EditorMode::Normal;
		self.visual_anchor = None;
		self.visual_block_anchor_display_col = None;
		self.visual_block_cursor_display_col = None;
		self.preferred_col = None;
		self.line_slot = None;
		self.line_slot_line_wise = false;
		self.line_slot_block_wise = false;
		self.pending_insert_group = None;
		self.pending_block_insert = None;
	}
}

impl Default for EditorState {
	fn default() -> Self { Self::new() }
}
