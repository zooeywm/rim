use std::io;
use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::state::BufferId;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    KeyPressed(KeyEvent),
    EnterInsert,
    AppendInsert,
    OpenLineBelowInsert,
    OpenLineAboveInsert,
    EnterCommandMode,
    EnterVisualMode,
    MoveLeft,
    MoveLineStart,
    MoveLineEnd,
    MoveDown,
    MoveUp,
    MoveRight,
    CutCharToSlot,
    PasteSlotAfterCursor,
    DeleteCurrentLineToSlot,
    CloseActiveBuffer,
    NewEmptyBuffer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutAction {
    SplitHorizontal,
    SplitVertical,
    ViewportResized { width: u16, height: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    FocusLeft,
    FocusDown,
    FocusUp,
    FocusRight,
    CloseActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferAction {
    SwitchPrev,
    SwitchNext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabAction {
    New,
    CloseCurrent,
    SwitchPrev,
    SwitchNext,
}

#[derive(Debug)]
pub enum FileAction {
    OpenRequested {
        path: PathBuf,
    },
    LoadCompleted {
        buffer_id: BufferId,
        result: io::Result<String>,
    },
    SaveCompleted {
        buffer_id: BufferId,
        result: io::Result<()>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    Quit,
}
