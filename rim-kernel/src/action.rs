use std::{ops::{BitOr, BitOrAssign}, path::PathBuf};

use crate::state::{BufferId, PersistedBufferHistory, WorkspaceSessionSnapshot};

/// Facility-agnostic key code used by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
	Backspace,
	Enter,
	Left,
	Right,
	Up,
	Down,
	Tab,
	Esc,
	Char(char),
}

/// Lightweight bitflag wrapper for key modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
	pub const ALT: Self = Self(1 << 2);
	pub const CONTROL: Self = Self(1 << 1);
	pub const NONE: Self = Self(0);
	pub const SHIFT: Self = Self(1 << 0);

	pub const fn contains(self, rhs: Self) -> bool { (self.0 & rhs.0) == rhs.0 }
}

impl BitOr for KeyModifiers {
	type Output = Self;

	fn bitor(self, rhs: Self) -> Self::Output { Self(self.0 | rhs.0) }
}

impl BitOrAssign for KeyModifiers {
	fn bitor_assign(&mut self, rhs: Self) { self.0 |= rhs.0; }
}

/// Canonical keyboard event flowing into the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
	pub code:      KeyCode,
	pub modifiers: KeyModifiers,
}

impl KeyEvent {
	pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self { Self { code, modifiers } }
}

/// Top-level action envelope consumed by the action handler.
#[derive(Debug)]
pub enum AppAction {
	Editor(EditorAction),
	Layout(LayoutAction),
	Window(WindowAction),
	Buffer(BufferAction),
	Tab(TabAction),
	File(FileAction),
	System(SystemAction),
}

/// Editor behavior actions, including raw key events and high-level commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
	KeyPressed(KeyEvent),
	EnterInsert,
	AppendInsert,
	OpenLineBelowInsert,
	OpenLineAboveInsert,
	EnterCommandMode,
	EnterVisualMode,
	EnterVisualLineMode,
	EnterVisualBlockMode,
	MoveLeft,
	MoveLineStart,
	MoveLineEnd,
	MoveDown,
	MoveUp,
	MoveRight,
	MoveFileStart,
	MoveFileEnd,
	ScrollViewDown,
	ScrollViewUp,
	ScrollViewHalfPageDown,
	ScrollViewHalfPageUp,
	Undo,
	Redo,
	JoinLineBelow,
	CutCharToSlot,
	PasteSlotAfterCursor,
	DeleteCurrentLineToSlot,
	CloseActiveBuffer,
	NewEmptyBuffer,
}

/// Layout-affecting actions emitted by input/runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutAction {
	SplitHorizontal,
	SplitVertical,
	ViewportResized { width: u16, height: u16 },
}

/// Window focus and lifecycle actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
	FocusLeft,
	FocusDown,
	FocusUp,
	FocusRight,
	CloseActive,
}

/// Buffer navigation actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferAction {
	SwitchPrev,
	SwitchNext,
}

/// Tab management actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabAction {
	New,
	CloseCurrent,
	SwitchPrev,
	SwitchNext,
}

/// File-side actions include requests and async completion callbacks.
#[derive(Debug)]
pub enum FileAction {
	OpenRequested {
		path: PathBuf,
	},
	ExternalChangeDetected {
		buffer_id: BufferId,
		path:      PathBuf,
	},
	SwapConflictDetected {
		buffer_id: BufferId,
		result:    anyhow::Result<Option<SwapConflictInfo>>,
	},
	SwapRecoverCompleted {
		buffer_id: BufferId,
		result:    anyhow::Result<Option<String>>,
	},
	UndoHistoryLoaded {
		buffer_id:     BufferId,
		source_path:   PathBuf,
		expected_text: String,
		restore_view:  bool,
		result:        anyhow::Result<Option<PersistedBufferHistory>>,
	},
	WorkspaceSessionLoaded {
		result: anyhow::Result<Option<WorkspaceSessionSnapshot>>,
	},
	LoadCompleted {
		buffer_id: BufferId,
		source:    FileLoadSource,
		result:    anyhow::Result<String>,
	},
	SaveCompleted {
		buffer_id: BufferId,
		result:    anyhow::Result<()>,
	},
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapConflictInfo {
	pub pid:      u32,
	pub username: String,
}

/// Marks whether a load comes from explicit open or external reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileLoadSource {
	Open,
	External,
}

/// Process-level actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
	Quit,
}
