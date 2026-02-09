use std::io;
use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::state::BufferId;

#[derive(Debug)]
#[allow(dead_code)]
pub enum AppAction {
    Editor(EditorAction),
    Layout(LayoutAction),
    Window(WindowAction),
    Buffer(BufferAction),
    Tab(TabAction),
    Status(StatusAction),
    File(FileAction),
    System(SystemAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    KeyPressed(KeyEvent),
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum StatusAction {
    SetMode(String),
    SetMessage(String),
    ClearMessage,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum FileAction {
    OpenRequested {
        path: PathBuf,
    },
    LoadCompleted {
        buffer_id: BufferId,
        result: io::Result<String>,
    },
    SaveRequested {
        buffer_id: BufferId,
        path: PathBuf,
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
