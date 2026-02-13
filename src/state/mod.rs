use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

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
    pub name: String,
    pub path: Option<PathBuf>,
    pub text: String,
    pub dirty: bool,
    pub externally_modified: bool,
    pub cursor: CursorState,
    pub undo_stack: Vec<BufferHistoryEntry>,
    pub redo_stack: Vec<BufferHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferHistoryEntry {
    pub edits: Vec<BufferEditSnapshot>,
    pub before_cursor: CursorState,
    pub after_cursor: CursorState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferEditSnapshot {
    pub start_byte: usize,
    pub deleted_text: String,
    pub inserted_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowState {
    pub buffer_id: Option<BufferId>,
    pub scroll_x: u16,
    pub scroll_y: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabState {
    pub windows: Vec<WindowId>,
    pub active_window: WindowId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
    pub mode: String,
    pub message: String,
    pub key_sequence: String,
}

impl Default for StatusBarState {
    fn default() -> Self {
        Self {
            mode: "NORMAL".to_string(),
            message: "new file".to_string(),
            key_sequence: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorState {
    pub row: u16,
    pub col: u16,
}

impl Default for CursorState {
    fn default() -> Self {
        Self { row: 1, col: 1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Normal,
    Insert,
    Command,
    VisualChar,
    VisualLine,
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
    pub buffer_id: BufferId,
    pub before_cursor: CursorState,
    pub edits: Vec<BufferEditSnapshot>,
}

#[derive(Debug)]
pub struct AppState {
    pub title: String,
    pub active_tab: TabId,
    pub leader_key: char,
    pub mode: EditorMode,
    pub visual_anchor: Option<CursorState>,
    pub command_line: String,
    pub quit_after_save: bool,
    pub pending_save_path: Option<(BufferId, PathBuf)>,
    pub preferred_col: Option<u16>,
    pub line_slot: Option<String>,
    pub line_slot_line_wise: bool,
    pub cursor_scroll_threshold: u16,
    pub normal_sequence: Vec<NormalSequenceKey>,
    pub visual_g_pending: bool,
    pub pending_insert_group: Option<PendingInsertUndoGroup>,
    pub in_flight_internal_saves: HashSet<BufferId>,
    pub last_internal_save_fingerprint: HashMap<BufferId, u64>,
    pub buffers: SlotMap<BufferId, BufferState>,
    pub buffer_order: Vec<BufferId>,
    pub windows: SlotMap<WindowId, WindowState>,
    pub tabs: BTreeMap<TabId, TabState>,
    pub status_bar: StatusBarState,
}

impl AppState {
    const MAX_HISTORY_ENTRIES: usize = 256;

    pub fn normalize_file_path(path: &Path) -> PathBuf {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        };
        std::fs::canonicalize(&absolute).unwrap_or(absolute)
    }

    pub fn new() -> Self {
        let buffers = SlotMap::with_key();
        let mut windows = SlotMap::with_key();
        let mut tabs = BTreeMap::new();

        let tab_id = TabId(1);
        let window_id = windows.insert(WindowState::default());

        tabs.insert(
            tab_id,
            TabState {
                windows: vec![window_id],
                active_window: window_id,
            },
        );

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
            cursor_scroll_threshold: 0,
            normal_sequence: Vec::new(),
            visual_g_pending: false,
            pending_insert_group: None,
            in_flight_internal_saves: HashSet::new(),
            last_internal_save_fingerprint: HashMap::new(),
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

        let id = self.windows.insert(WindowState {
            buffer_id,
            ..WindowState::default()
        });
        Some(id)
    }

    pub fn status_line(&self) -> String {
        let cursor = self.active_cursor();
        let total_rows = self
            .active_buffer_id()
            .and_then(|buffer_id| self.buffers.get(buffer_id))
            .map(|buffer| {
                if buffer.text.is_empty() {
                    1
                } else {
                    buffer.text.lines().count() as u16
                }
            })
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
            return format!(
                "{} | {} | {}",
                self.status_bar.mode, self.status_bar.message, cursor_pos
            );
        }

        format!(
            "{} | {} | keys {} | {}",
            self.status_bar.mode, self.status_bar.message, self.status_bar.key_sequence, cursor_pos
        )
    }

    pub fn active_buffer_id(&self) -> Option<BufferId> {
        self.windows
            .get(self.active_window_id())
            .and_then(|window| window.buffer_id)
    }

    pub fn bind_buffer_to_active_window(&mut self, buffer_id: BufferId) {
        let active_window_id = self.active_window_id();
        let window = self
            .windows
            .get_mut(active_window_id)
            .expect("invariant: active window id must exist");
        window.buffer_id = Some(buffer_id);
    }

    pub fn is_insert_mode(&self) -> bool {
        self.mode == EditorMode::Insert
    }

    pub fn is_command_mode(&self) -> bool {
        self.mode == EditorMode::Command
    }

    pub fn is_visual_mode(&self) -> bool {
        matches!(self.mode, EditorMode::VisualChar | EditorMode::VisualLine)
    }

    pub fn is_visual_line_mode(&self) -> bool {
        self.mode == EditorMode::VisualLine
    }

    pub fn enter_insert_mode(&mut self) {
        self.mode = EditorMode::Insert;
        self.visual_anchor = None;
        self.status_bar.mode = "INSERT".to_string();
    }

    pub fn exit_insert_mode(&mut self) {
        self.mode = EditorMode::Normal;
        self.visual_anchor = None;
        self.status_bar.mode = "NORMAL".to_string();
        self.clamp_cursor_to_navigable_col();
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = EditorMode::Command;
        self.visual_anchor = None;
        self.command_line.clear();
        self.status_bar.mode = "COMMAND".to_string();
    }

    pub fn exit_command_mode(&mut self) {
        self.mode = EditorMode::Normal;
        self.visual_anchor = None;
        self.command_line.clear();
        self.status_bar.mode = "NORMAL".to_string();
    }

    pub fn enter_visual_mode(&mut self) {
        self.mode = EditorMode::VisualChar;
        self.visual_anchor = Some(self.active_cursor());
        self.status_bar.mode = "VISUAL".to_string();
    }

    pub fn enter_visual_line_mode(&mut self) {
        let anchor_row = self
            .visual_anchor
            .map(|cursor| cursor.row)
            .unwrap_or_else(|| self.active_cursor().row);
        self.mode = EditorMode::VisualLine;
        self.visual_anchor = Some(CursorState {
            row: anchor_row,
            col: 1,
        });
        self.status_bar.mode = "VISUAL LINE".to_string();
    }

    pub fn exit_visual_mode(&mut self) {
        self.mode = EditorMode::Normal;
        self.visual_anchor = None;
        self.status_bar.mode = "NORMAL".to_string();
    }

    pub fn push_command_char(&mut self, ch: char) {
        self.command_line.push(ch);
    }

    pub fn pop_command_char(&mut self) {
        let _ = self.command_line.pop();
    }

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
        Ok((buffer_id, path, buffer.text.clone()))
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
            snapshots.push((buffer_id, path, buffer.text.clone()));
        }

        snapshots.sort_by_key(|(id, _, _)| id.data().as_ffi());
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

    pub fn set_buffer_externally_modified(
        &mut self,
        buffer_id: BufferId,
        externally_modified: bool,
    ) {
        if let Some(buffer) = self.buffers.get_mut(buffer_id) {
            buffer.externally_modified = externally_modified;
        }
    }

    pub fn active_buffer_is_externally_modified(&self) -> Option<bool> {
        let buffer_id = self.active_buffer_id()?;
        let buffer = self.buffers.get(buffer_id)?;
        Some(buffer.externally_modified)
    }

    pub fn mark_active_buffer_dirty(&mut self) {
        if let Some(buffer_id) = self.active_buffer_id() {
            self.set_buffer_dirty(buffer_id, true);
        }
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

    pub fn undo_active_buffer_edit(&mut self) {
        let Some(buffer_id) = self.active_buffer_id() else {
            self.status_bar.message = "undo failed: no active buffer".to_string();
            return;
        };
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
        buffer.cursor = previous_entry.before_cursor;
        buffer.redo_stack.push(previous_entry);
        if buffer.redo_stack.len() > Self::MAX_HISTORY_ENTRIES {
            buffer.redo_stack.remove(0);
        }
        buffer.dirty = true;

        self.align_active_window_scroll_to_cursor();
        self.status_bar.message = "undo".to_string();
    }

    pub fn redo_active_buffer_edit(&mut self) {
        let Some(buffer_id) = self.active_buffer_id() else {
            self.status_bar.message = "redo failed: no active buffer".to_string();
            return;
        };
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
        buffer.cursor = next_entry.after_cursor;
        buffer.undo_stack.push(next_entry);
        if buffer.undo_stack.len() > Self::MAX_HISTORY_ENTRIES {
            buffer.undo_stack.remove(0);
        }
        buffer.dirty = true;

        self.align_active_window_scroll_to_cursor();
        self.status_bar.message = "redo".to_string();
    }

    pub fn has_dirty_buffers(&self) -> bool {
        self.buffers.values().any(|buffer| buffer.dirty)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

fn buffer_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
}

fn apply_text_delta_undo(text: &mut String, delta: &BufferEditSnapshot) {
    let undo_end = delta.start_byte.saturating_add(delta.inserted_text.len());
    text.replace_range(delta.start_byte..undo_end, &delta.deleted_text);
}

fn apply_text_delta_redo(text: &mut String, delta: &BufferEditSnapshot) {
    let redo_end = delta.start_byte.saturating_add(delta.deleted_text.len());
    text.replace_range(delta.start_byte..redo_end, &delta.inserted_text);
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
