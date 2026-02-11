use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slotmap::{Key, SlotMap, new_key_type};
use tracing::{error, trace};
use unicode_width::UnicodeWidthChar;

new_key_type! { pub struct BufferId; }
new_key_type! { pub struct WindowId; }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferState {
    pub name: String,
    pub path: Option<PathBuf>,
    pub text: String,
    pub cursor: CursorState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
}

impl Default for StatusBarState {
    fn default() -> Self {
        Self {
            mode: "NORMAL".to_string(),
            message: "new file".to_string(),
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

#[derive(Debug)]
pub struct AppState {
    pub title: String,
    pub active_tab: TabId,
    pub mode: EditorMode,
    pub visual_anchor: Option<CursorState>,
    pub command_line: String,
    pub quit_after_save: bool,
    pub pending_save_path: Option<(BufferId, PathBuf)>,
    pub line_slot: Option<String>,
    pub cursor_scroll_threshold: u16,
    pub buffers: SlotMap<BufferId, BufferState>,
    pub windows: SlotMap<WindowId, WindowState>,
    pub tabs: BTreeMap<TabId, TabState>,
    pub status_bar: StatusBarState,
}

impl AppState {
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
            mode: EditorMode::Normal,
            visual_anchor: None,
            command_line: String::new(),
            quit_after_save: false,
            pending_save_path: None,
            line_slot: None,
            cursor_scroll_threshold: 0,
            buffers,
            windows,
            tabs,
            status_bar: StatusBarState::default(),
        }
    }

    pub fn create_buffer(&mut self, path: Option<PathBuf>, text: impl Into<String>) -> BufferId {
        let text = text.into();
        let name = path
            .as_deref()
            .and_then(buffer_name_from_path)
            .unwrap_or_else(|| "untitled".to_string());

        self.buffers.insert(BufferState {
            name,
            path,
            text,
            cursor: CursorState::default(),
        })
    }

    pub fn open_new_tab(&mut self) -> TabId {
        let tab_id = self.insert_tab_after_active();
        self.switch_tab(tab_id);
        self.status_bar.message = "new tab".to_string();
        tab_id
    }

    pub fn remove_tab(&mut self, tab_id: TabId) {
        if self.tabs.len() <= 1 {
            return;
        }
        if !self.tabs.contains_key(&tab_id) {
            return;
        }

        if self.active_tab == tab_id {
            let next_active = self
                .tabs
                .keys()
                .copied()
                .filter(|id| *id != tab_id && id.0 < tab_id.0)
                .max_by_key(|id| id.0)
                .or_else(|| {
                    self.tabs
                        .keys()
                        .copied()
                        .filter(|id| *id != tab_id && id.0 > tab_id.0)
                        .min_by_key(|id| id.0)
                })
                .expect("invariant: there must be another tab when removing active tab");
            self.active_tab = next_active;
        }

        self.tabs.remove(&tab_id);
        self.compact_tab_ids_after(tab_id);
    }

    pub fn switch_tab(&mut self, tab_id: TabId) {
        if self.tabs.contains_key(&tab_id) {
            self.active_tab = tab_id;
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
        if self.mode == EditorMode::Command {
            return format!(":{}", self.command_line);
        }
        format!("{} | {}", self.status_bar.mode, self.status_bar.message)
    }

    pub fn active_tab_window_ids(&self) -> Vec<WindowId> {
        self.tabs
            .get(&self.active_tab)
            .map(|tab| tab.windows.clone())
            .unwrap_or_default()
    }

    pub fn active_window_id(&self) -> WindowId {
        self.tabs
            .get(&self.active_tab)
            .map(|tab| tab.active_window)
            .expect("active tab must exist")
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

    pub fn close_current_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let current_tab = self.active_tab;
        self.remove_tab(current_tab);
        self.status_bar.message = "tab closed".to_string();
    }

    pub fn switch_to_prev_tab(&mut self) {
        let current_tab = self.active_tab;
        if let Some(prev_tab) = self
            .tabs
            .keys()
            .copied()
            .filter(|id| id.0 < current_tab.0)
            .max_by_key(|id| id.0)
        {
            self.switch_tab(prev_tab);
        }
    }

    pub fn switch_to_next_tab(&mut self) {
        let current_tab = self.active_tab;
        if let Some(next_tab) = self
            .tabs
            .keys()
            .copied()
            .filter(|id| id.0 > current_tab.0)
            .min_by_key(|id| id.0)
        {
            self.switch_tab(next_tab);
        }
    }

    pub fn focus_window(&mut self, direction: FocusDirection) {
        let tab = self
            .tabs
            .get_mut(&self.active_tab)
            .expect("invariant: active tab must exist");
        let active_id = tab.active_window;
        let active = self
            .windows
            .get(active_id)
            .expect("invariant: active window id must exist in windows");
        let active_left = i32::from(active.x);
        let active_right = i32::from(active.x.saturating_add(active.width));
        let active_top = i32::from(active.y);
        let active_bottom = i32::from(active.y.saturating_add(active.height));
        let active_cx = active_left + (active_right - active_left) / 2;
        let active_cy = active_top + (active_bottom - active_top) / 2;

        let best = tab
            .windows
            .iter()
            .copied()
            .filter(|id| *id != active_id)
            .filter_map(|id| self.windows.get(id).map(|w| (id, w)))
            .filter_map(|(id, w)| {
                let left = i32::from(w.x);
                let right = i32::from(w.x.saturating_add(w.width));
                let top = i32::from(w.y);
                let bottom = i32::from(w.y.saturating_add(w.height));
                let cx = left + (right - left) / 2;
                let cy = top + (bottom - top) / 2;

                let score = match direction {
                    FocusDirection::Left if right <= active_left => {
                        Some((active_left - right, (cy - active_cy).abs()))
                    }
                    FocusDirection::Right if left >= active_right => {
                        Some((left - active_right, (cy - active_cy).abs()))
                    }
                    FocusDirection::Up if bottom <= active_top => {
                        Some((active_top - bottom, (cx - active_cx).abs()))
                    }
                    FocusDirection::Down if top >= active_bottom => {
                        Some((top - active_bottom, (cx - active_cx).abs()))
                    }
                    _ => None,
                }?;

                Some((id, score))
            })
            .min_by_key(|(_, score)| *score)
            .map(|(id, _)| id);

        if let Some(target) = best {
            tab.active_window = target;
        }
    }

    pub fn close_active_window(&mut self) {
        let tab_snapshot = self
            .tabs
            .get(&self.active_tab)
            .expect("invariant: active tab must exist");
        let active_window = tab_snapshot.active_window;
        if tab_snapshot.windows.len() <= 1 {
            return;
        }
        let current_idx = tab_snapshot
            .windows
            .iter()
            .position(|id| *id == active_window)
            .unwrap_or(0);
        let remaining_windows = tab_snapshot
            .windows
            .iter()
            .copied()
            .filter(|id| *id != active_window)
            .collect::<Vec<_>>();
        let closed_layout = self
            .windows
            .get(active_window)
            .cloned()
            .expect("invariant: active window id must exist in windows");
        let absorbed_target = self.absorb_closed_window(&remaining_windows, &closed_layout);
        let absorbed_group = absorbed_target.is_none()
            && self.absorb_closed_window_by_group(&remaining_windows, &closed_layout);

        let _ = self.windows.remove(active_window);
        let tab = self
            .tabs
            .get_mut(&self.active_tab)
            .expect("invariant: active tab must exist");
        tab.windows.retain(|id| *id != active_window);
        tab.active_window = if let Some(id) = absorbed_target {
            id
        } else if absorbed_group {
            *tab.windows
                .first()
                .expect("tab must keep at least one window")
        } else {
            let next_idx = current_idx.min(tab.windows.len().saturating_sub(1));
            *tab.windows
                .get(next_idx)
                .expect("tab must keep at least one window")
        };
        self.status_bar.message = "window closed".to_string();
    }

    pub fn move_cursor_left(&mut self) {
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.col > 1
        {
            cursor.col = cursor.col.saturating_sub(1);
        }
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
    }

    pub fn move_cursor_right(&mut self) {
        let row = self.active_cursor().row;
        let max_col = self.max_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.col < max_col
        {
            cursor.col = cursor.col.saturating_add(1);
        }
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
    }

    pub fn move_cursor_line_start(&mut self) {
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = 1;
        }
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
    }

    pub fn move_cursor_line_end(&mut self) {
        let row = self.active_cursor().row;
        let max_col = self.max_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = max_col;
        }
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
    }

    pub fn move_cursor_up(&mut self) {
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.row > 1
        {
            cursor.row = cursor.row.saturating_sub(1);
        }
        self.clamp_cursor_col_to_row();
        self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Up);
    }

    pub fn move_cursor_down(&mut self) {
        let max_row = self.max_row();
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.row < max_row
        {
            cursor.row = cursor.row.saturating_add(1);
        }
        self.clamp_cursor_col_to_row();
        self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Down);
    }

    pub fn split_active_window(&mut self, axis: SplitAxis) {
        let tab_id = self.active_tab;
        let active_window_id = self
            .tabs
            .get(&tab_id)
            .map(|t| t.active_window)
            .expect("invariant: active tab must exist");
        let active_window = self
            .windows
            .get(active_window_id)
            .expect("invariant: active window id must exist in windows")
            .clone();
        let (updated_active, new_window_layout) = split_window_layout(&active_window, axis);

        let Some(new_window_id) = self.create_window(active_window.buffer_id) else {
            error!(
                "split_active_window failed: unable to create new window for buffer {:?}",
                active_window.buffer_id
            );
            return;
        };
        if let Some(window) = self.windows.get_mut(active_window_id) {
            *window = updated_active;
        }
        if let Some(window) = self.windows.get_mut(new_window_id) {
            *window = new_window_layout;
        }

        let tab = self
            .tabs
            .get_mut(&tab_id)
            .expect("invariant: active tab must exist");
        tab.windows.push(new_window_id);
        tab.active_window = new_window_id;
        self.status_bar.message = match axis {
            SplitAxis::Horizontal => "split horizontal".to_string(),
            SplitAxis::Vertical => "split vertical".to_string(),
        };
    }

    pub fn update_active_tab_layout(&mut self, width: u16, height: u16) {
        trace!("update_active_tab_layout");
        let tab = self
            .tabs
            .get(&self.active_tab)
            .expect("invariant: active tab must exist");
        let window_ids = tab.windows.clone();

        if window_ids.is_empty() {
            return;
        }

        let max_right = window_ids
            .iter()
            .filter_map(|id| self.windows.get(*id))
            .map(|w| w.x.saturating_add(w.width))
            .max()
            .unwrap_or(0);
        let max_bottom = window_ids
            .iter()
            .filter_map(|id| self.windows.get(*id))
            .map(|w| w.y.saturating_add(w.height))
            .max()
            .unwrap_or(0);

        if max_right == 0 || max_bottom == 0 {
            if let Some(first_id) = window_ids.first().copied() {
                if let Some(window) = self.windows.get_mut(first_id) {
                    window.x = 0;
                    window.y = 0;
                    window.width = width.max(1);
                    window.height = height.max(1);
                }
                for id in window_ids.iter().skip(1) {
                    if let Some(window) = self.windows.get_mut(*id) {
                        window.x = 0;
                        window.y = 0;
                        window.width = width.max(1);
                        window.height = height.max(1);
                    }
                }
            }
            return;
        }

        for id in &window_ids {
            if let Some(window) = self.windows.get_mut(*id) {
                let old_right = window.x.saturating_add(window.width);
                let old_bottom = window.y.saturating_add(window.height);
                let new_x = (u32::from(window.x) * u32::from(width) / u32::from(max_right)) as u16;
                let new_y =
                    (u32::from(window.y) * u32::from(height) / u32::from(max_bottom)) as u16;
                let new_right =
                    (u32::from(old_right) * u32::from(width) / u32::from(max_right)) as u16;
                let new_bottom =
                    (u32::from(old_bottom) * u32::from(height) / u32::from(max_bottom)) as u16;
                window.x = new_x.min(width.saturating_sub(1));
                window.y = new_y.min(height.saturating_sub(1));
                window.width = new_right
                    .saturating_sub(new_x)
                    .max(1)
                    .min(width.saturating_sub(window.x).max(1));
                window.height = new_bottom
                    .saturating_sub(new_y)
                    .max(1)
                    .min(height.saturating_sub(window.y).max(1));
            }
        }
    }

    pub fn active_cursor(&self) -> CursorState {
        self.active_buffer_id()
            .and_then(|buffer_id| self.buffers.get(buffer_id))
            .map(|buffer| buffer.cursor)
            .unwrap_or_default()
    }

    fn clamp_cursor_col_to_row(&mut self) {
        let row = self.active_cursor().row;
        let max_col = self.max_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            if cursor.col > max_col {
                cursor.col = max_col;
            }
            if cursor.col == 0 {
                cursor.col = 1;
            }
        }
    }

    fn max_row(&self) -> u16 {
        self.active_buffer_text()
            .map(|text| text.split('\n').count() as u16)
            .filter(|count| *count > 0)
            .unwrap_or(1)
    }

    fn max_col_for_row(&self, row: u16) -> u16 {
        let row_index = row.saturating_sub(1) as usize;
        let line_len = self
            .active_buffer_text()
            .and_then(|text| text.split('\n').nth(row_index))
            .map(|line| line.chars().count() as u16)
            .unwrap_or(0);
        line_len.saturating_add(1)
    }

    fn active_buffer_text(&self) -> Option<&str> {
        let active_window_id = self.tabs.get(&self.active_tab)?.active_window;
        let buffer_id = self.windows.get(active_window_id)?.buffer_id?;
        self.buffers
            .get(buffer_id)
            .map(|buffer| buffer.text.as_str())
    }

    fn active_buffer_cursor_mut(&mut self) -> Option<&mut CursorState> {
        let buffer_id = self.active_buffer_id()?;
        self.buffers
            .get_mut(buffer_id)
            .map(|buffer| &mut buffer.cursor)
    }

    fn active_buffer_mut(&mut self) -> Option<&mut BufferState> {
        let buffer_id = self.active_buffer_id()?;
        self.buffers.get_mut(buffer_id)
    }

    fn insert_tab_after_active(&mut self) -> TabId {
        let current = self.active_tab.0;
        let new_id = TabId(current.saturating_add(1));
        let window_id = self
            .create_window(None)
            .expect("create default tab window should never fail");
        let old_tabs = std::mem::take(&mut self.tabs);
        let mut rebuilt_tabs = BTreeMap::new();

        for (id, tab) in old_tabs {
            let target_id = if id.0 > current {
                TabId(id.0.saturating_add(1))
            } else {
                id
            };
            rebuilt_tabs.insert(target_id, tab);
        }

        rebuilt_tabs.insert(
            new_id,
            TabState {
                windows: vec![window_id],
                active_window: window_id,
            },
        );
        self.tabs = rebuilt_tabs;
        new_id
    }

    fn compact_tab_ids_after(&mut self, removed: TabId) {
        let old_tabs = std::mem::take(&mut self.tabs);
        let mut rebuilt_tabs = BTreeMap::new();
        for (id, tab) in old_tabs {
            let target_id = if id.0 > removed.0 {
                TabId(id.0.saturating_sub(1))
            } else {
                id
            };
            rebuilt_tabs.insert(target_id, tab);
        }
        if self.active_tab.0 > removed.0 {
            self.active_tab = TabId(self.active_tab.0.saturating_sub(1));
        }
        self.tabs = rebuilt_tabs;
    }

    pub fn switch_active_window_buffer(&mut self, direction: BufferSwitchDirection) {
        let active_window_id = self.active_window_id();
        let mut buffer_ids = self.buffers.keys().collect::<Vec<_>>();
        if buffer_ids.is_empty() {
            return;
        }
        buffer_ids.sort_by_key(|id| id.data().as_ffi());

        let current = self
            .windows
            .get(active_window_id)
            .expect("invariant: active window id must exist in windows")
            .buffer_id;
        let target = match current.and_then(|id| buffer_ids.iter().position(|x| *x == id)) {
            Some(idx) => match direction {
                BufferSwitchDirection::Prev => {
                    if idx == 0 {
                        *buffer_ids.last().expect("non-empty by construction")
                    } else {
                        buffer_ids[idx.saturating_sub(1)]
                    }
                }
                BufferSwitchDirection::Next => {
                    if idx + 1 >= buffer_ids.len() {
                        buffer_ids[0]
                    } else {
                        buffer_ids[idx + 1]
                    }
                }
            },
            None => match direction {
                BufferSwitchDirection::Prev => {
                    *buffer_ids.last().expect("non-empty by construction")
                }
                BufferSwitchDirection::Next => buffer_ids[0],
            },
        };

        if let Some(window) = self.windows.get_mut(active_window_id) {
            window.buffer_id = Some(target);
        }
        self.align_active_window_scroll_to_cursor();
        if let Some(buffer) = self.buffers.get(target) {
            self.status_bar.message = format!("buffer {}", buffer.name);
        }
    }

    fn absorb_closed_window(
        &mut self,
        candidates: &[WindowId],
        closed: &WindowState,
    ) -> Option<WindowId> {
        for id in candidates {
            let Some(w) = self.windows.get(*id).cloned() else {
                continue;
            };
            if w.y == closed.y
                && w.height == closed.height
                && w.x.saturating_add(w.width) == closed.x
            {
                if let Some(target) = self.windows.get_mut(*id) {
                    target.width = target.width.saturating_add(closed.width);
                }
                return Some(*id);
            }
            if w.y == closed.y
                && w.height == closed.height
                && closed.x.saturating_add(closed.width) == w.x
            {
                if let Some(target) = self.windows.get_mut(*id) {
                    target.x = closed.x;
                    target.width = target.width.saturating_add(closed.width);
                }
                return Some(*id);
            }
            if w.x == closed.x
                && w.width == closed.width
                && w.y.saturating_add(w.height) == closed.y
            {
                if let Some(target) = self.windows.get_mut(*id) {
                    target.height = target.height.saturating_add(closed.height);
                }
                return Some(*id);
            }
            if w.x == closed.x
                && w.width == closed.width
                && closed.y.saturating_add(closed.height) == w.y
            {
                if let Some(target) = self.windows.get_mut(*id) {
                    target.y = closed.y;
                    target.height = target.height.saturating_add(closed.height);
                }
                return Some(*id);
            }
        }
        None
    }

    fn absorb_closed_window_by_group(
        &mut self,
        candidates: &[WindowId],
        closed: &WindowState,
    ) -> bool {
        self.absorb_group_from_right(candidates, closed)
            || self.absorb_group_from_left(candidates, closed)
            || self.absorb_group_from_bottom(candidates, closed)
            || self.absorb_group_from_top(candidates, closed)
    }

    fn absorb_group_from_right(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
        let mut group = candidates
            .iter()
            .copied()
            .filter_map(|id| self.windows.get(id).map(|w| (id, w.clone())))
            .filter(|(_, w)| {
                closed.x.saturating_add(closed.width) == w.x
                    && w.width > 0
                    && overlap_len(w.y, w.height, closed.y, closed.height) > 0
            })
            .collect::<Vec<_>>();
        if group.is_empty() {
            return false;
        }
        group.sort_by_key(|(_, w)| w.y);
        if !is_full_vertical_cover(&group, closed) {
            return false;
        }
        for (id, _) in group {
            if let Some(target) = self.windows.get_mut(id) {
                target.x = closed.x;
                target.width = target.width.saturating_add(closed.width);
            }
        }
        true
    }

    fn absorb_group_from_left(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
        let mut group = candidates
            .iter()
            .copied()
            .filter_map(|id| self.windows.get(id).map(|w| (id, w.clone())))
            .filter(|(_, w)| {
                w.x.saturating_add(w.width) == closed.x
                    && w.width > 0
                    && overlap_len(w.y, w.height, closed.y, closed.height) > 0
            })
            .collect::<Vec<_>>();
        if group.is_empty() {
            return false;
        }
        group.sort_by_key(|(_, w)| w.y);
        if !is_full_vertical_cover(&group, closed) {
            return false;
        }
        for (id, _) in group {
            if let Some(target) = self.windows.get_mut(id) {
                target.width = target.width.saturating_add(closed.width);
            }
        }
        true
    }

    fn absorb_group_from_bottom(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
        let mut group = candidates
            .iter()
            .copied()
            .filter_map(|id| self.windows.get(id).map(|w| (id, w.clone())))
            .filter(|(_, w)| {
                closed.y.saturating_add(closed.height) == w.y
                    && w.height > 0
                    && overlap_len(w.x, w.width, closed.x, closed.width) > 0
            })
            .collect::<Vec<_>>();
        if group.is_empty() {
            return false;
        }
        group.sort_by_key(|(_, w)| w.x);
        if !is_full_horizontal_cover(&group, closed) {
            return false;
        }
        for (id, _) in group {
            if let Some(target) = self.windows.get_mut(id) {
                target.y = closed.y;
                target.height = target.height.saturating_add(closed.height);
            }
        }
        true
    }

    fn absorb_group_from_top(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
        let mut group = candidates
            .iter()
            .copied()
            .filter_map(|id| self.windows.get(id).map(|w| (id, w.clone())))
            .filter(|(_, w)| {
                w.y.saturating_add(w.height) == closed.y
                    && w.height > 0
                    && overlap_len(w.x, w.width, closed.x, closed.width) > 0
            })
            .collect::<Vec<_>>();
        if group.is_empty() {
            return false;
        }
        group.sort_by_key(|(_, w)| w.x);
        if !is_full_horizontal_cover(&group, closed) {
            return false;
        }
        for (id, _) in group {
            if let Some(target) = self.windows.get_mut(id) {
                target.height = target.height.saturating_add(closed.height);
            }
        }
        true
    }

    pub fn set_cursor_scroll_threshold(&mut self, threshold: u16) {
        self.cursor_scroll_threshold = threshold;
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

    pub fn insert_char_at_cursor(&mut self, ch: char) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }

        let line = lines
            .get_mut(row_idx)
            .expect("row index must exist after resize");
        let byte_idx = char_to_byte_idx(line, col_idx);
        line.insert(byte_idx, ch);
        buffer.cursor.col = buffer.cursor.col.saturating_add(1);
        buffer.text = lines.join("\n");
        self.align_active_window_scroll_to_cursor();
    }

    pub fn insert_newline_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }

        let line = lines
            .get_mut(row_idx)
            .expect("row index must exist after resize");
        let byte_idx = char_to_byte_idx(line, col_idx);
        let tail = line.split_off(byte_idx);
        lines.insert(row_idx.saturating_add(1), tail);

        buffer.cursor.row = buffer.cursor.row.saturating_add(1);
        buffer.cursor.col = 1;
        buffer.text = lines.join("\n");
        self.align_active_window_scroll_to_cursor();
    }

    pub fn open_line_below_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }

        lines.insert(row_idx.saturating_add(1), String::new());
        buffer.cursor.row = buffer.cursor.row.saturating_add(1);
        buffer.cursor.col = 1;
        buffer.text = lines.join("\n");
        self.align_active_window_scroll_to_cursor();
    }

    pub fn open_line_above_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }

        lines.insert(row_idx, String::new());
        buffer.cursor.col = 1;
        buffer.text = lines.join("\n");
        self.align_active_window_scroll_to_cursor();
    }

    pub fn backspace_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if lines.is_empty() || row_idx >= lines.len() {
            return;
        }

        if col_idx > 0 {
            let line = lines.get_mut(row_idx).expect("row index must exist");
            let end = char_to_byte_idx(line, col_idx);
            col_idx = col_idx.saturating_sub(1);
            let start = char_to_byte_idx(line, col_idx);
            line.drain(start..end);
            buffer.cursor.col = buffer.cursor.col.saturating_sub(1);
        } else if row_idx > 0 {
            let current_line = lines.remove(row_idx);
            let prev_line = lines
                .get_mut(row_idx.saturating_sub(1))
                .expect("previous row must exist");
            let prev_char_len = prev_line.chars().count() as u16;
            prev_line.push_str(&current_line);
            buffer.cursor.row = buffer.cursor.row.saturating_sub(1);
            buffer.cursor.col = prev_char_len.saturating_add(1);
        } else {
            return;
        }

        buffer.text = lines.join("\n");
        self.align_active_window_scroll_to_cursor();
    }

    pub fn cut_current_char_to_slot(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "cut failed: no active buffer".to_string();
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);
        let Some(line) = lines.get_mut(row_idx) else {
            self.status_bar.message = "cut failed: out of range".to_string();
            return;
        };
        let char_count = line.chars().count();
        if col_idx >= char_count {
            self.status_bar.message = "cut failed: no char".to_string();
            return;
        }

        let start = char_to_byte_idx(line, col_idx);
        let end = char_to_byte_idx(line, col_idx.saturating_add(1));
        let cut = line[start..end].to_string();
        line.drain(start..end);
        buffer.text = lines.join("\n");
        self.line_slot = Some(cut);
        self.align_active_window_scroll_to_cursor();
        self.status_bar.message = "char cut".to_string();
    }

    pub fn paste_slot_at_cursor(&mut self) {
        let Some(slot_text) = self.line_slot.clone() else {
            self.status_bar.message = "paste failed: slot is empty".to_string();
            return;
        };
        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "paste failed: no active buffer".to_string();
            return;
        };

        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);
        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }
        let line = lines
            .get_mut(row_idx)
            .expect("row index must exist after resize");
        let char_count = line.chars().count();
        let insert_char_idx = col_idx.saturating_add(1).min(char_count);
        let byte_idx = char_to_byte_idx(line, insert_char_idx);
        line.insert_str(byte_idx, &slot_text);
        buffer.cursor.col = buffer
            .cursor
            .col
            .saturating_add(slot_text.chars().count() as u16);
        buffer.text = lines.join("\n");
        self.align_active_window_scroll_to_cursor();
        self.status_bar.message = "pasted".to_string();
    }

    pub fn delete_visual_selection_to_slot(&mut self) {
        let line_wise = self.is_visual_line_mode();
        let Some((start, end)) = self.normalized_visual_bounds() else {
            self.status_bar.message = "visual delete failed: no anchor".to_string();
            self.exit_visual_mode();
            return;
        };

        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "visual delete failed: no active buffer".to_string();
            self.exit_visual_mode();
            return;
        };

        let mut lines = split_lines_owned(&buffer.text);
        let start_row = start.row.saturating_sub(1) as usize;
        let end_row = end.row.saturating_sub(1) as usize;
        if start_row >= lines.len() || end_row >= lines.len() {
            self.status_bar.message = "visual delete failed: out of range".to_string();
            self.exit_visual_mode();
            return;
        }

        if line_wise {
            let deleted = lines[start_row..=end_row].join("\n");
            lines.drain(start_row..=end_row);
            if lines.is_empty() {
                lines.push(String::new());
            }
            buffer.text = lines.join("\n");
            let new_row = start_row.min(lines.len().saturating_sub(1)).saturating_add(1) as u16;
            buffer.cursor.row = new_row;
            buffer.cursor.col = 1;
            self.line_slot = Some(deleted);
            self.align_active_window_scroll_to_cursor();
            self.exit_visual_mode();
            self.status_bar.message = "selection deleted".to_string();
            return;
        }

        let start_line_len = lines[start_row].chars().count() as u16;
        let end_line_len = lines[end_row].chars().count() as u16;
        if start_line_len == 0 && end_line_len == 0 {
            self.status_bar.message = "visual delete failed: empty".to_string();
            self.exit_visual_mode();
            return;
        }

        let start_col = start.col.max(1).min(start_line_len.max(1));
        let end_col = end.col.max(1).min(end_line_len.max(1));

        let deleted_text = if start_row == end_row {
            let line = &mut lines[start_row];
            let start_byte = char_to_byte_idx(line, start_col.saturating_sub(1) as usize);
            let end_byte = char_to_byte_idx(line, end_col as usize);
            let deleted = line[start_byte..end_byte].to_string();
            line.drain(start_byte..end_byte);
            deleted
        } else {
            let start_line = lines[start_row].clone();
            let end_line = lines[end_row].clone();
            let start_keep_byte =
                char_to_byte_idx(&start_line, start_col.saturating_sub(1) as usize);
            let end_del_byte = char_to_byte_idx(&end_line, end_col as usize);

            let mut deleted_parts = Vec::new();
            deleted_parts.push(start_line[start_keep_byte..].to_string());
            for line in lines.iter().take(end_row).skip(start_row.saturating_add(1)) {
                deleted_parts.push(line.clone());
            }
            deleted_parts.push(end_line[..end_del_byte].to_string());

            let merged_line = format!("{}{}", &start_line[..start_keep_byte], &end_line[end_del_byte..]);
            lines[start_row] = merged_line;
            for _ in start_row.saturating_add(1)..=end_row {
                lines.remove(start_row.saturating_add(1));
            }

            deleted_parts.join("\n")
        };

        buffer.text = lines.join("\n");
        buffer.cursor.row = start_row.saturating_add(1) as u16;
        let line_len = lines[start_row].chars().count() as u16;
        buffer.cursor.col = start_col.min(line_len.saturating_add(1));
        self.line_slot = Some(deleted_text);
        self.align_active_window_scroll_to_cursor();
        self.exit_visual_mode();
        self.status_bar.message = "selection deleted".to_string();
    }

    pub fn yank_visual_selection_to_slot(&mut self) {
        let Some((start, end)) = self.normalized_visual_bounds() else {
            self.status_bar.message = "visual yank failed: no anchor".to_string();
            self.exit_visual_mode();
            return;
        };

        let Some(text) = self.active_buffer_text() else {
            self.status_bar.message = "visual yank failed: no active buffer".to_string();
            self.exit_visual_mode();
            return;
        };
        let lines = split_lines_owned(text);
        let start_row = start.row.saturating_sub(1) as usize;
        let end_row = end.row.saturating_sub(1) as usize;
        if start_row >= lines.len() || end_row >= lines.len() {
            self.status_bar.message = "visual yank failed: out of range".to_string();
            self.exit_visual_mode();
            return;
        }

        let start_line = &lines[start_row];
        let end_line = &lines[end_row];
        let start_line_len = start_line.chars().count() as u16;
        let end_line_len = end_line.chars().count() as u16;
        let start_col = start.col.max(1).min(start_line_len.max(1));
        let end_col = end.col.max(1).min(end_line_len.max(1));

        let yanked = if start_row == end_row {
            let start_byte = char_to_byte_idx(start_line, start_col.saturating_sub(1) as usize);
            let end_byte = char_to_byte_idx(start_line, end_col as usize);
            start_line[start_byte..end_byte].to_string()
        } else {
            let mut parts = Vec::new();
            let start_keep_byte =
                char_to_byte_idx(start_line, start_col.saturating_sub(1) as usize);
            parts.push(start_line[start_keep_byte..].to_string());
            for line in lines.iter().take(end_row).skip(start_row.saturating_add(1)) {
                parts.push(line.clone());
            }
            let end_del_byte = char_to_byte_idx(end_line, end_col as usize);
            parts.push(end_line[..end_del_byte].to_string());
            parts.join("\n")
        };

        self.line_slot = Some(yanked);
        self.exit_visual_mode();
        self.status_bar.message = "selection yanked".to_string();
    }

    pub fn replace_visual_selection_with_slot(&mut self) {
        let line_wise = self.is_visual_line_mode();
        let Some(slot_text) = self.line_slot.clone() else {
            self.status_bar.message = "paste failed: slot is empty".to_string();
            self.exit_visual_mode();
            return;
        };
        let Some((start, end)) = self.normalized_visual_bounds() else {
            self.status_bar.message = "visual paste failed: no anchor".to_string();
            self.exit_visual_mode();
            return;
        };

        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "visual paste failed: no active buffer".to_string();
            self.exit_visual_mode();
            return;
        };

        let mut lines = split_lines_owned(&buffer.text);
        let start_row = start.row.saturating_sub(1) as usize;
        let end_row = end.row.saturating_sub(1) as usize;
        if start_row >= lines.len() || end_row >= lines.len() {
            self.status_bar.message = "visual paste failed: out of range".to_string();
            self.exit_visual_mode();
            return;
        }

        if line_wise {
            let replacement = split_lines_owned(&slot_text);
            lines.splice(start_row..=end_row, replacement);
            if lines.is_empty() {
                lines.push(String::new());
            }
            buffer.text = lines.join("\n");
            buffer.cursor.row = start_row.saturating_add(1) as u16;
            buffer.cursor.col = 1;
            self.align_active_window_scroll_to_cursor();
            self.exit_visual_mode();
            self.status_bar.message = "selection replaced".to_string();
            return;
        }

        let start_line = lines[start_row].clone();
        let end_line = lines[end_row].clone();
        let start_line_len = start_line.chars().count() as u16;
        let end_line_len = end_line.chars().count() as u16;
        let start_col = start.col.max(1).min(start_line_len.max(1));
        let end_col = end.col.max(1).min(end_line_len.max(1));

        let prefix_end = char_to_byte_idx(&start_line, start_col.saturating_sub(1) as usize);
        let suffix_start = char_to_byte_idx(&end_line, end_col as usize);
        let prefix = start_line[..prefix_end].to_string();
        let suffix = end_line[suffix_start..].to_string();

        let slot_lines = split_lines_owned(&slot_text);
        let mut replacement = Vec::new();
        if slot_lines.len() == 1 {
            replacement.push(format!("{}{}{}", prefix, slot_lines[0], suffix));
        } else {
            replacement.push(format!("{}{}", prefix, slot_lines[0]));
            for line in slot_lines.iter().skip(1).take(slot_lines.len().saturating_sub(2)) {
                replacement.push(line.clone());
            }
            let last = slot_lines.last().cloned().unwrap_or_default();
            replacement.push(format!("{}{}", last, suffix));
        }

        lines.splice(start_row..=end_row, replacement);
        buffer.text = lines.join("\n");
        buffer.cursor.row = start_row.saturating_add(1) as u16;
        buffer.cursor.col = start_col;
        self.align_active_window_scroll_to_cursor();
        self.exit_visual_mode();
        self.status_bar.message = "selection replaced".to_string();
    }

    fn normalized_visual_bounds(&self) -> Option<(CursorState, CursorState)> {
        let anchor = self.visual_anchor?;
        let cursor = self.active_cursor();
        let (mut start, mut end) = if (anchor.row, anchor.col) <= (cursor.row, cursor.col) {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };

        if self.is_visual_line_mode() {
            start.col = 1;
            end.col = self.max_col_for_row(end.row).saturating_sub(1).max(1);
        }
        Some((start, end))
    }

    fn active_window_visible_rows(&self) -> u16 {
        let window_id = self.active_window_id();
        self.windows
            .get(window_id)
            .map(|window| {
                let reserved_for_split_line = u16::from(window.y > 0);
                window.height.saturating_sub(reserved_for_split_line).max(1)
            })
            .unwrap_or(1)
    }

    fn active_window_visible_text_cols(&self) -> u16 {
        let window_id = self.active_window_id();
        self.windows
            .get(window_id)
            .map(|window| {
                let reserved_for_split_line = u16::from(window.x > 0);
                let local_width = window.width.saturating_sub(reserved_for_split_line).max(1);
                let number_col_width = if local_width <= 5 {
                    0
                } else {
                    local_width.min(5)
                };
                local_width.saturating_sub(number_col_width).max(1)
            })
            .unwrap_or(1)
    }

    fn adjust_scroll_after_vertical_move(&mut self, direction: VerticalMoveDirection) {
        let active_window_id = self.active_window_id();
        let cursor_row = self.active_cursor().row;
        let cursor_line = cursor_row.saturating_sub(1);
        let visible_rows = self.active_window_visible_rows();
        let max_row = self.max_row();
        let max_scroll = max_row.saturating_sub(visible_rows);
        let threshold = self.cursor_scroll_threshold;
        let visible_tail = visible_rows.saturating_sub(1);

        if let Some(window) = self.windows.get_mut(active_window_id) {
            match direction {
                VerticalMoveDirection::Up => {
                    let top_trigger = window.scroll_y.saturating_add(threshold);
                    if cursor_line < top_trigger {
                        window.scroll_y = cursor_line.saturating_sub(threshold).min(max_scroll);
                    }
                }
                VerticalMoveDirection::Down => {
                    let bottom = window.scroll_y.saturating_add(visible_tail);
                    let bottom_trigger = bottom.saturating_sub(threshold);
                    if cursor_line > bottom_trigger {
                        let needed_top = cursor_line
                            .saturating_add(threshold)
                            .saturating_sub(visible_tail);
                        window.scroll_y = needed_top.min(max_scroll);
                    }
                }
            }
        }
    }

    fn adjust_scroll_after_horizontal_move(&mut self, direction: HorizontalMoveDirection) {
        let active_window_id = self.active_window_id();
        let visible_cols = self.active_window_visible_text_cols();
        let visible_tail = visible_cols.saturating_sub(1);
        let threshold = self.cursor_scroll_threshold.min(visible_tail);
        let cursor_display_col = self.active_cursor_display_col();
        let line_display_width = self.active_line_display_width();
        let max_scroll = line_display_width.saturating_sub(visible_tail);

        if let Some(window) = self.windows.get_mut(active_window_id) {
            match direction {
                HorizontalMoveDirection::Left => {
                    let left_trigger = window.scroll_x.saturating_add(threshold);
                    if cursor_display_col < left_trigger {
                        window.scroll_x =
                            cursor_display_col.saturating_sub(threshold).min(max_scroll);
                    }
                }
                HorizontalMoveDirection::Right => {
                    let right = window.scroll_x.saturating_add(visible_tail);
                    let right_trigger = right.saturating_sub(threshold);
                    if cursor_display_col > right_trigger {
                        let needed_left = cursor_display_col
                            .saturating_add(threshold)
                            .saturating_sub(visible_tail);
                        window.scroll_x = needed_left.min(max_scroll);
                    }
                }
            }
        }
    }

    fn align_active_window_scroll_to_cursor(&mut self) {
        let active_window_id = self.active_window_id();
        let Some(window) = self.windows.get(active_window_id).cloned() else {
            return;
        };
        let cursor_line = self.active_cursor().row.saturating_sub(1);
        let visible_rows = self.active_window_visible_rows();
        let max_row = self.max_row();
        let max_scroll = max_row.saturating_sub(visible_rows);
        let threshold = self
            .cursor_scroll_threshold
            .min(visible_rows.saturating_sub(1));
        let visible_tail = visible_rows.saturating_sub(1);
        let top_trigger = window.scroll_y.saturating_add(threshold);
        let bottom = window.scroll_y.saturating_add(visible_tail);
        let bottom_trigger = bottom.saturating_sub(threshold);
        let visible_cols = self.active_window_visible_text_cols();
        let col_tail = visible_cols.saturating_sub(1);
        let col_threshold = self.cursor_scroll_threshold.min(col_tail);
        let cursor_display_col = self.active_cursor_display_col();
        let line_display_width = self.active_line_display_width();
        let max_scroll_x = line_display_width.saturating_sub(col_tail);
        let left_trigger = window.scroll_x.saturating_add(col_threshold);
        let right = window.scroll_x.saturating_add(col_tail);
        let right_trigger = right.saturating_sub(col_threshold);

        let mut next_scroll = window.scroll_y;
        if cursor_line < top_trigger {
            next_scroll = cursor_line.saturating_sub(threshold);
        } else if cursor_line > bottom_trigger {
            next_scroll = cursor_line
                .saturating_add(threshold)
                .saturating_sub(visible_tail);
        }
        next_scroll = next_scroll.min(max_scroll);
        let mut next_scroll_x = window.scroll_x;
        if cursor_display_col < left_trigger {
            next_scroll_x = cursor_display_col.saturating_sub(col_threshold);
        } else if cursor_display_col > right_trigger {
            next_scroll_x = cursor_display_col
                .saturating_add(col_threshold)
                .saturating_sub(col_tail);
        }
        next_scroll_x = next_scroll_x.min(max_scroll_x);

        if let Some(active_window) = self.windows.get_mut(active_window_id) {
            active_window.scroll_y = next_scroll;
            active_window.scroll_x = next_scroll_x;
        }
    }

    fn active_cursor_display_col(&self) -> u16 {
        let cursor = self.active_cursor();
        let row_index = cursor.row.saturating_sub(1) as usize;
        let char_index = cursor.col.saturating_sub(1) as usize;
        self.active_buffer_text()
            .and_then(|text| text.split('\n').nth(row_index))
            .map(|line| display_width_of_char_prefix(line, char_index) as u16)
            .unwrap_or(0)
    }

    fn active_line_display_width(&self) -> u16 {
        let row_index = self.active_cursor().row.saturating_sub(1) as usize;
        self.active_buffer_text()
            .and_then(|text| text.split('\n').nth(row_index))
            .map(|line| {
                line.chars()
                    .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0) as u16)
                    .sum()
            })
            .unwrap_or(0)
    }
}

fn overlap_len(a_start: u16, a_len: u16, b_start: u16, b_len: u16) -> u16 {
    let a_end = a_start.saturating_add(a_len);
    let b_end = b_start.saturating_add(b_len);
    let start = a_start.max(b_start);
    let end = a_end.min(b_end);
    end.saturating_sub(start)
}

fn is_full_vertical_cover(group: &[(WindowId, WindowState)], closed: &WindowState) -> bool {
    let mut cursor = closed.y;
    let end = closed.y.saturating_add(closed.height);
    for (_, w) in group {
        if w.y > cursor {
            return false;
        }
        let w_end = w.y.saturating_add(w.height);
        if w_end > cursor {
            cursor = w_end;
        }
        if cursor >= end {
            return true;
        }
    }
    cursor >= end
}

fn is_full_horizontal_cover(group: &[(WindowId, WindowState)], closed: &WindowState) -> bool {
    let mut cursor = closed.x;
    let end = closed.x.saturating_add(closed.width);
    for (_, w) in group {
        if w.x > cursor {
            return false;
        }
        let w_end = w.x.saturating_add(w.width);
        if w_end > cursor {
            cursor = w_end;
        }
        if cursor >= end {
            return true;
        }
    }
    cursor >= end
}

fn buffer_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
}

fn split_lines_owned(text: &str) -> Vec<String> {
    let mut lines = text
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    s.char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

fn display_width_of_char_prefix(line: &str, char_count: usize) -> usize {
    line.chars()
        .take(char_count)
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

fn split_window_layout(window: &WindowState, axis: SplitAxis) -> (WindowState, WindowState) {
    let mut first = window.clone();
    let mut second = window.clone();
    let base_width = window.width.max(1);
    let base_height = window.height.max(1);
    match axis {
        SplitAxis::Horizontal => {
            let left_w = (base_width / 2).max(1);
            let right_w = base_width.saturating_sub(left_w).max(1);
            first.width = left_w;
            first.height = base_height;
            second.x = window.x.saturating_add(left_w);
            second.width = right_w;
            second.height = base_height;
        }
        SplitAxis::Vertical => {
            let top_h = (base_height / 2).max(1);
            let bottom_h = base_height.saturating_sub(top_h).max(1);
            first.width = base_width;
            first.height = top_h;
            second.width = base_width;
            second.y = window.y.saturating_add(top_h);
            second.height = bottom_h;
        }
    }
    (first, second)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerticalMoveDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HorizontalMoveDirection {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::{AppState, BufferSwitchDirection, CursorState, FocusDirection, SplitAxis, TabId};
    use std::path::PathBuf;

    fn test_state() -> AppState {
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(Some(PathBuf::from("test.rs")), "fn main() {}");
        let active_window_id = state.active_window_id();
        let window = state
            .windows
            .get_mut(active_window_id)
            .expect("active window should exist");
        window.buffer_id = Some(buffer_id);
        state
    }

    fn set_active_buffer_text(state: &mut AppState, text: &str) {
        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state
            .buffers
            .get_mut(buffer_id)
            .expect("active buffer exists");
        buffer.text = text.to_string();
    }

    #[test]
    fn switch_and_remove_tab_flow() {
        let mut state = test_state();
        let tab2 = state.open_new_tab();

        state.switch_tab(tab2);
        assert_eq!(state.active_tab, tab2);

        state.remove_tab(super::TabId(1));
    }

    #[test]
    fn remove_active_tab_should_switch_to_another_when_not_last() {
        let mut state = test_state();
        let tab2 = state.open_new_tab();
        state.switch_tab(tab2);
        state.remove_tab(state.active_tab);
        assert_eq!(state.active_tab, TabId(1));
        assert_eq!(state.tabs.len(), 1);
    }

    #[test]
    fn horizontal_split_should_half_width_for_two_windows() {
        let mut state = test_state();
        state.split_active_window(SplitAxis::Horizontal);
        state.update_active_tab_layout(100, 20);

        let tab = state
            .tabs
            .get(&state.active_tab)
            .expect("active tab exists");
        assert_eq!(tab.windows.len(), 2);

        for id in &tab.windows {
            let w = state.windows.get(*id).expect("window exists");
            assert_eq!(w.width, 50);
            assert_eq!(w.height, 20);
        }
    }

    #[test]
    fn vertical_split_should_half_height_for_two_windows() {
        let mut state = test_state();
        state.split_active_window(SplitAxis::Vertical);
        state.update_active_tab_layout(100, 20);

        let tab = state
            .tabs
            .get(&state.active_tab)
            .expect("active tab exists");
        assert_eq!(tab.windows.len(), 2);

        for id in &tab.windows {
            let w = state.windows.get(*id).expect("window exists");
            assert_eq!(w.width, 100);
            assert_eq!(w.height, 10);
        }
    }

    #[test]
    fn nested_split_should_only_affect_active_cell() {
        let mut state = test_state();
        state.update_active_tab_layout(100, 20);
        state.split_active_window(SplitAxis::Vertical);
        state.split_active_window(SplitAxis::Horizontal);
        state.update_active_tab_layout(100, 20);

        let tab = state
            .tabs
            .get(&state.active_tab)
            .expect("active tab exists");
        assert_eq!(tab.windows.len(), 3);

        let mut windows = tab
            .windows
            .iter()
            .filter_map(|id| state.windows.get(*id))
            .collect::<Vec<_>>();
        windows.sort_by_key(|w| (w.y, w.x));

        assert_eq!(windows[0].x, 0);
        assert_eq!(windows[0].y, 0);
        assert_eq!(windows[0].width, 100);
        assert_eq!(windows[0].height, 10);

        assert_eq!(windows[1].x, 0);
        assert_eq!(windows[1].y, 10);
        assert_eq!(windows[1].width, 50);
        assert_eq!(windows[1].height, 10);

        assert_eq!(windows[2].x, 50);
        assert_eq!(windows[2].y, 10);
        assert_eq!(windows[2].width, 50);
        assert_eq!(windows[2].height, 10);
    }

    #[test]
    fn open_new_tab_should_insert_after_active_tab() {
        let mut state = test_state();
        let old_tab2 = state.open_new_tab();
        let old_tab3 = state.open_new_tab();
        let old_tab4 = state.open_new_tab();
        assert_eq!(old_tab2.0, 2);
        assert_eq!(old_tab3.0, 3);
        assert_eq!(old_tab4.0, 4);
        state.switch_tab(super::TabId(1));

        let created = state.open_new_tab();
        assert_eq!(created.0, 2);
        assert_eq!(state.active_tab, super::TabId(2));
        assert!(state.tabs.contains_key(&super::TabId(3)));
        assert!(state.tabs.contains_key(&super::TabId(4)));
        assert!(state.tabs.contains_key(&super::TabId(5)));
    }

    #[test]
    fn open_new_tab_should_create_default_window_without_buffer() {
        let mut state = test_state();
        let tab_id = state.open_new_tab();
        let tab = state.tabs.get(&tab_id).expect("new tab should exist");
        assert_eq!(tab.windows.len(), 1);
        let window_id = tab.windows[0];
        let window = state.windows.get(window_id).expect("window should exist");
        assert_eq!(window.buffer_id, None);
        assert_eq!(tab.active_window, window_id);
    }

    #[test]
    fn switch_prev_next_tab_should_change_active_tab_by_one_step() {
        let mut state = test_state();
        let tab2 = state.open_new_tab();
        let tab3 = state.open_new_tab();
        state.switch_tab(tab2);

        state.switch_to_next_tab();
        assert_eq!(state.active_tab, tab3);

        state.switch_to_prev_tab();
        assert_eq!(state.active_tab, tab2);
    }

    #[test]
    fn switch_prev_next_tab_should_noop_at_edges() {
        let mut state = test_state();
        let tab2 = state.open_new_tab();
        state.switch_tab(super::TabId(1));

        state.switch_to_prev_tab();
        assert_eq!(state.active_tab, super::TabId(1));

        state.switch_tab(tab2);
        state.switch_to_next_tab();
        assert!(state.tabs.contains_key(&tab2));
        assert_eq!(state.active_tab, tab2);
    }

    #[test]
    fn close_current_tab_should_switch_to_another_tab() {
        let mut state = test_state();
        let tab1 = state.active_tab;
        let tab2 = state.open_new_tab();
        state.switch_tab(tab2);
        state.close_current_tab();

        assert_eq!(state.active_tab, tab1);
        assert!(!state.tabs.contains_key(&tab2));
    }

    #[test]
    fn close_current_tab_should_prefer_lower_tab_id() {
        let mut state = test_state();
        let tab2 = state.open_new_tab();
        let tab3 = state.open_new_tab();
        state.switch_tab(tab3);
        state.close_current_tab();

        assert_eq!(state.active_tab, tab2);
        assert!(!state.tabs.contains_key(&tab3));
    }

    #[test]
    fn close_middle_tab_should_compact_following_tab_ids() {
        let mut state = test_state();
        let tab2 = state.open_new_tab();
        let tab3 = state.open_new_tab();

        state.switch_tab(tab2);
        state.close_current_tab();

        assert!(!state.tabs.contains_key(&tab3));
        assert!(state.tabs.contains_key(&TabId(2)));
        assert_eq!(state.tabs.len(), 2);
    }

    #[test]
    fn close_current_tab_should_noop_when_only_one_tab() {
        let mut state = test_state();
        let active_before = state.active_tab;
        let tab_count_before = state.tabs.len();
        state.close_current_tab();

        assert_eq!(state.active_tab, active_before);
        assert_eq!(state.tabs.len(), tab_count_before);
    }

    #[test]
    fn open_new_tab_should_reuse_deleted_tab_id() {
        let mut state = test_state();
        let tab2 = state.open_new_tab();
        state.switch_tab(tab2);
        state.close_current_tab();

        let recreated = state.open_new_tab();
        assert_eq!(recreated, tab2);
    }

    #[test]
    fn cursor_move_right_should_stop_at_line_end() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abc");
        state.move_cursor_right();
        state.move_cursor_right();
        state.move_cursor_right();
        state.move_cursor_right();
        assert_eq!(state.active_cursor().row, 1);
        assert_eq!(state.active_cursor().col, 4);
    }

    #[test]
    fn cursor_move_down_should_clamp_column_to_target_line() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abcd\nx");
        state.move_cursor_right();
        state.move_cursor_right();
        state.move_cursor_right();
        state.move_cursor_down();
        assert_eq!(state.active_cursor().row, 2);
        assert_eq!(state.active_cursor().col, 2);
    }

    #[test]
    fn cursor_move_down_at_bottom_should_scroll_window() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "1\n2\n3\n4\n5\n6");
        state.update_active_tab_layout(80, 3);

        state.move_cursor_down();
        state.move_cursor_down();
        let active_window_id = state.active_window_id();
        let before = state
            .windows
            .get(active_window_id)
            .expect("window exists")
            .scroll_y;
        assert_eq!(before, 0);

        state.move_cursor_down();
        let after = state
            .windows
            .get(active_window_id)
            .expect("window exists")
            .scroll_y;
        assert_eq!(state.active_cursor().row, 4);
        assert_eq!(after, 1);
    }

    #[test]
    fn cursor_scroll_threshold_should_trigger_earlier() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "1\n2\n3\n4\n5\n6");
        state.update_active_tab_layout(80, 4);
        state.set_cursor_scroll_threshold(1);

        state.move_cursor_down();
        state.move_cursor_down();
        let active_window_id = state.active_window_id();
        let before = state
            .windows
            .get(active_window_id)
            .expect("window exists")
            .scroll_y;
        assert_eq!(before, 0);

        state.move_cursor_down();
        let after = state
            .windows
            .get(active_window_id)
            .expect("window exists")
            .scroll_y;
        assert_eq!(state.active_cursor().row, 4);
        assert_eq!(after, 1);
    }

    #[test]
    fn same_buffer_in_different_windows_should_share_cursor_position() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "a\nb\nc");
        state.update_active_tab_layout(100, 20);
        state.split_active_window(SplitAxis::Vertical);

        state.move_cursor_down();
        state.move_cursor_down();
        assert_eq!(state.active_cursor().row, 3);
        assert_eq!(state.active_cursor().col, 1);

        state.focus_window(FocusDirection::Up);
        assert_eq!(state.active_cursor().row, 3);
        assert_eq!(state.active_cursor().col, 1);
        state.move_cursor_right();
        assert_eq!(state.active_cursor().row, 3);
        assert_eq!(state.active_cursor().col, 2);

        state.focus_window(FocusDirection::Down);
        assert_eq!(state.active_cursor().row, 3);
        assert_eq!(state.active_cursor().col, 2);
    }

    #[test]
    fn different_buffers_should_keep_separate_cursor_positions() {
        let mut state = test_state();
        let b1 = state.active_buffer_id().expect("active buffer exists");
        let b2 = state.create_buffer(Some(PathBuf::from("b2.rs")), "x\ny\nz");
        set_active_buffer_text(&mut state, "a\nb\nc");

        state.move_cursor_down();
        state.move_cursor_down();
        assert_eq!(state.active_cursor().row, 3);
        assert_eq!(state.active_buffer_id(), Some(b1));

        state.switch_active_window_buffer(BufferSwitchDirection::Next);
        assert_eq!(state.active_buffer_id(), Some(b2));
        assert_eq!(state.active_cursor().row, 1);
        assert_eq!(state.active_cursor().col, 1);

        state.move_cursor_down();
        assert_eq!(state.active_cursor().row, 2);

        state.switch_active_window_buffer(BufferSwitchDirection::Prev);
        assert_eq!(state.active_buffer_id(), Some(b1));
        assert_eq!(state.active_cursor().row, 3);
        assert_eq!(state.active_cursor().col, 1);
    }

    #[test]
    fn focus_window_direction_should_switch_to_adjacent_window() {
        let mut state = test_state();
        state.update_active_tab_layout(100, 20);
        state.split_active_window(SplitAxis::Vertical);
        state.focus_window(FocusDirection::Up);
        let active_after_up = state.active_window_id();
        let up_window = state
            .windows
            .get(active_after_up)
            .expect("window should exist");
        assert_eq!(up_window.y, 0);

        state.focus_window(FocusDirection::Down);
        let active_after_down = state.active_window_id();
        let down_window = state
            .windows
            .get(active_after_down)
            .expect("window should exist");
        assert_eq!(down_window.y, 10);
    }

    #[test]
    fn close_active_window_should_remove_when_multiple_windows() {
        let mut state = test_state();
        state.update_active_tab_layout(100, 20);
        state.split_active_window(SplitAxis::Vertical);
        let before_count = state.active_tab_window_ids().len();
        state.close_active_window();
        let after_count = state.active_tab_window_ids().len();
        assert_eq!(before_count, 2);
        assert_eq!(after_count, 1);
    }

    #[test]
    fn close_active_window_should_absorb_neighbor_after_v_then_h() {
        let mut state = test_state();
        state.update_active_tab_layout(100, 20);
        state.split_active_window(SplitAxis::Vertical);
        state.split_active_window(SplitAxis::Horizontal);

        state.close_active_window();

        let windows = state
            .active_tab_window_ids()
            .iter()
            .filter_map(|id| state.windows.get(*id))
            .collect::<Vec<_>>();
        assert_eq!(windows.len(), 2);

        let mut sorted = windows;
        sorted.sort_by_key(|w| (w.y, w.x));
        assert_eq!(sorted[0].x, 0);
        assert_eq!(sorted[0].y, 0);
        assert_eq!(sorted[0].width, 100);
        assert_eq!(sorted[0].height, 10);

        assert_eq!(sorted[1].x, 0);
        assert_eq!(sorted[1].y, 10);
        assert_eq!(sorted[1].width, 100);
        assert_eq!(sorted[1].height, 10);
    }

    #[test]
    fn close_left_after_h_then_right_v_should_expand_right_group() {
        let mut state = test_state();
        state.update_active_tab_layout(100, 20);

        state.split_active_window(SplitAxis::Horizontal);
        state.split_active_window(SplitAxis::Vertical);

        state.focus_window(FocusDirection::Left);
        state.close_active_window();

        let windows = state
            .active_tab_window_ids()
            .iter()
            .filter_map(|id| state.windows.get(*id))
            .collect::<Vec<_>>();
        assert_eq!(windows.len(), 2);

        let mut sorted = windows;
        sorted.sort_by_key(|w| (w.y, w.x));

        assert_eq!(sorted[0].x, 0);
        assert_eq!(sorted[0].y, 0);
        assert_eq!(sorted[0].width, 100);
        assert_eq!(sorted[0].height, 10);

        assert_eq!(sorted[1].x, 0);
        assert_eq!(sorted[1].y, 10);
        assert_eq!(sorted[1].width, 100);
        assert_eq!(sorted[1].height, 10);
    }

    #[test]
    fn close_left_after_h_v_h_v_should_not_leave_left_gap() {
        let mut state = test_state();
        state.update_active_tab_layout(100, 20);

        state.split_active_window(SplitAxis::Horizontal);
        state.split_active_window(SplitAxis::Vertical);
        state.split_active_window(SplitAxis::Horizontal);
        state.split_active_window(SplitAxis::Vertical);

        state.focus_window(FocusDirection::Left);
        state.focus_window(FocusDirection::Left);
        state.close_active_window();

        let windows = state
            .active_tab_window_ids()
            .iter()
            .filter_map(|id| state.windows.get(*id))
            .collect::<Vec<_>>();
        assert_eq!(windows.len(), 4);

        let min_x = windows.iter().map(|w| w.x).min().expect("windows exist");
        let max_right = windows
            .iter()
            .map(|w| w.x.saturating_add(w.width))
            .max()
            .expect("windows exist");
        assert_eq!(min_x, 0);
        assert_eq!(max_right, 100);
    }

    #[test]
    fn switch_active_window_buffer_should_cycle_next_and_prev() {
        let mut state = test_state();
        let b1 = state.active_buffer_id().expect("active buffer exists");
        let b2 = state.create_buffer(Some(PathBuf::from("b2.rs")), "b2");
        let b3 = state.create_buffer(Some(PathBuf::from("b3.rs")), "b3");

        state.switch_active_window_buffer(BufferSwitchDirection::Next);
        assert_eq!(state.active_buffer_id(), Some(b2));

        state.switch_active_window_buffer(BufferSwitchDirection::Next);
        assert_eq!(state.active_buffer_id(), Some(b3));

        state.switch_active_window_buffer(BufferSwitchDirection::Next);
        assert_eq!(state.active_buffer_id(), Some(b1));

        state.switch_active_window_buffer(BufferSwitchDirection::Prev);
        assert_eq!(state.active_buffer_id(), Some(b3));
    }

    #[test]
    fn switch_active_window_buffer_should_bind_when_window_has_no_buffer() {
        let mut state = AppState::new();
        let b1 = state.create_buffer(Some(PathBuf::from("a.rs")), "a");
        let b2 = state.create_buffer(Some(PathBuf::from("b.rs")), "b");

        state.switch_active_window_buffer(BufferSwitchDirection::Next);
        assert_eq!(state.active_buffer_id(), Some(b1));

        state.switch_active_window_buffer(BufferSwitchDirection::Prev);
        assert_eq!(state.active_buffer_id(), Some(b2));
    }

    #[test]
    fn switch_active_window_buffer_should_realign_scroll_to_target_cursor() {
        let mut state = test_state();
        let b2 = state.create_buffer(Some(PathBuf::from("b2.rs")), "line\nline\nline");
        state.update_active_tab_layout(80, 10);

        let active_window_id = state.active_window_id();
        {
            let window = state
                .windows
                .get_mut(active_window_id)
                .expect("active window should exist");
            window.scroll_y = 800;
        }

        state.switch_active_window_buffer(BufferSwitchDirection::Next);
        assert_eq!(state.active_buffer_id(), Some(b2));

        let scroll_y = state
            .windows
            .get(active_window_id)
            .expect("active window should exist")
            .scroll_y;
        assert_eq!(scroll_y, 0);
    }

    #[test]
    fn insert_mode_should_toggle_status_mode_text() {
        let mut state = test_state();
        assert_eq!(state.status_bar.mode, "NORMAL");
        assert!(!state.is_insert_mode());

        state.enter_insert_mode();
        assert_eq!(state.status_bar.mode, "INSERT");
        assert!(state.is_insert_mode());

        state.exit_insert_mode();
        assert_eq!(state.status_bar.mode, "NORMAL");
        assert!(!state.is_insert_mode());
    }

    #[test]
    fn visual_mode_should_set_anchor_and_status_mode() {
        let mut state = test_state();
        state.move_cursor_right();
        state.move_cursor_down();
        let cursor = state.active_cursor();
        state.enter_visual_mode();

        assert!(state.is_visual_mode());
        assert_eq!(state.visual_anchor, Some(cursor));
        assert_eq!(state.status_bar.mode, "VISUAL");
    }

    #[test]
    fn visual_mode_exit_should_clear_anchor_and_restore_normal_mode() {
        let mut state = test_state();
        state.enter_visual_mode();
        state.exit_visual_mode();

        assert!(!state.is_visual_mode());
        assert_eq!(state.visual_anchor, None);
        assert_eq!(state.status_bar.mode, "NORMAL");
    }

    #[test]
    fn visual_line_mode_should_set_line_anchor_and_status_mode() {
        let mut state = test_state();
        state.move_cursor_right();
        state.enter_visual_mode();
        state.enter_visual_line_mode();

        assert!(state.is_visual_line_mode());
        assert_eq!(state.visual_anchor, Some(CursorState { row: 1, col: 1 }));
        assert_eq!(state.status_bar.mode, "VISUAL LINE");
    }

    #[test]
    fn visual_delete_should_remove_selected_chars_in_single_line() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abcdef");
        state.move_cursor_right();
        state.enter_visual_mode();
        state.move_cursor_right();
        state.move_cursor_right();
        state.delete_visual_selection_to_slot();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "aef");
        assert_eq!(state.line_slot, Some("bcd".to_string()));
        assert!(!state.is_visual_mode());
        assert_eq!(state.status_bar.mode, "NORMAL");
    }

    #[test]
    fn visual_delete_should_remove_selected_chars_across_lines() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abc\ndef\nghi");
        state.move_cursor_right();
        state.enter_visual_mode();
        state.move_cursor_down();
        state.move_cursor_down();
        state.move_cursor_right();
        state.delete_visual_selection_to_slot();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "a");
        assert_eq!(state.line_slot, Some("bc\ndef\nghi".to_string()));
        assert_eq!(buffer.cursor.row, 1);
        assert_eq!(buffer.cursor.col, 2);
    }

    #[test]
    fn visual_yank_should_copy_selection_without_modifying_buffer() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abcdef");
        state.move_cursor_right();
        state.enter_visual_mode();
        state.move_cursor_right();
        state.move_cursor_right();
        state.yank_visual_selection_to_slot();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "abcdef");
        assert_eq!(state.line_slot, Some("bcd".to_string()));
        assert!(!state.is_visual_mode());
    }

    #[test]
    fn visual_paste_should_replace_selection_in_single_line() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abcdef");
        state.line_slot = Some("XY".to_string());
        state.move_cursor_right();
        state.enter_visual_mode();
        state.move_cursor_right();
        state.move_cursor_right();
        state.replace_visual_selection_with_slot();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "aXYef");
        assert!(!state.is_visual_mode());
    }

    #[test]
    fn visual_paste_should_replace_selection_across_lines() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abc\ndef\nghi");
        state.line_slot = Some("Z".to_string());
        state.move_cursor_right();
        state.enter_visual_mode();
        state.move_cursor_down();
        state.move_cursor_down();
        state.move_cursor_right();
        state.replace_visual_selection_with_slot();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "aZ");
        assert!(!state.is_visual_mode());
    }

    #[test]
    fn visual_line_delete_should_remove_whole_lines() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "a\nb\nc");
        state.enter_visual_mode();
        state.enter_visual_line_mode();
        state.move_cursor_down();
        state.delete_visual_selection_to_slot();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "c");
        assert_eq!(state.line_slot, Some("a\nb".to_string()));
        assert!(!state.is_visual_mode());
    }

    #[test]
    fn insert_char_and_newline_should_edit_buffer_at_cursor() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "ab");
        state.enter_insert_mode();

        state.insert_char_at_cursor('X');
        state.insert_newline_at_cursor();
        state.insert_char_at_cursor('Y');

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let text = &state.buffers.get(buffer_id).expect("buffer exists").text;
        assert_eq!(text, "X\nYab");
    }

    #[test]
    fn open_line_below_at_cursor_should_insert_empty_line_and_move_cursor_to_line_start() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abc\ndef");
        state.move_cursor_right();
        state.open_line_below_at_cursor();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "abc\n\ndef");
        assert_eq!(buffer.cursor.row, 2);
        assert_eq!(buffer.cursor.col, 1);
    }

    #[test]
    fn open_line_above_at_cursor_should_insert_empty_line_and_keep_cursor_on_current_row_index() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abc\ndef");
        state.move_cursor_down();
        state.move_cursor_right();
        state.open_line_above_at_cursor();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "abc\n\ndef");
        assert_eq!(buffer.cursor.row, 2);
        assert_eq!(buffer.cursor.col, 1);
    }

    #[test]
    fn backspace_at_line_start_should_join_with_previous_line() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abc\ndef");
        state.move_cursor_down();
        state.backspace_at_cursor();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "abcdef");
        assert_eq!(buffer.cursor.row, 1);
        assert_eq!(buffer.cursor.col, 4);
    }

    #[test]
    fn cursor_move_right_and_left_should_adjust_horizontal_scroll() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abcdefghijklmnopqrstuvwxyz");
        state.update_active_tab_layout(20, 8);

        for _ in 0..15 {
            state.move_cursor_right();
        }
        let window_id = state.active_window_id();
        let scrolled_right = state
            .windows
            .get(window_id)
            .expect("window exists")
            .scroll_x;
        assert!(scrolled_right > 0);

        for _ in 0..15 {
            state.move_cursor_left();
        }
        let scrolled_left = state
            .windows
            .get(window_id)
            .expect("window exists")
            .scroll_x;
        assert_eq!(scrolled_left, 0);
    }

    #[test]
    fn insert_char_should_adjust_horizontal_scroll() {
        let mut state = test_state();
        state.update_active_tab_layout(20, 8);
        state.enter_insert_mode();

        for _ in 0..20 {
            state.insert_char_at_cursor('x');
        }

        let window_id = state.active_window_id();
        let scroll_x = state
            .windows
            .get(window_id)
            .expect("window exists")
            .scroll_x;
        assert!(scroll_x > 0);
    }

    #[test]
    fn cursor_move_line_start_and_end_should_jump_in_current_line() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abcd");
        state.move_cursor_right();
        state.move_cursor_right();
        state.move_cursor_right();
        assert_eq!(state.active_cursor().col, 4);

        state.move_cursor_line_start();
        assert_eq!(state.active_cursor().col, 1);

        state.move_cursor_line_end();
        assert_eq!(state.active_cursor().col, 5);
    }

    #[test]
    fn command_mode_should_toggle_and_show_prompt_in_status_line() {
        let mut state = test_state();
        state.enter_command_mode();
        state.push_command_char('q');
        assert!(state.is_command_mode());
        assert_eq!(state.status_bar.mode, "COMMAND");
        assert!(state.status_line().contains(":q"));

        state.exit_command_mode();
        assert!(!state.is_command_mode());
        assert_eq!(state.status_bar.mode, "NORMAL");
    }

    #[test]
    fn take_command_line_should_return_trimmed_text_and_leave_command_mode() {
        let mut state = test_state();
        state.enter_command_mode();
        state.push_command_char(' ');
        state.push_command_char('q');
        state.push_command_char(' ');
        let cmd = state.take_command_line();
        assert_eq!(cmd, "q");
        assert!(!state.is_command_mode());
        assert_eq!(state.status_bar.mode, "NORMAL");
    }

    #[test]
    fn active_buffer_save_snapshot_should_fail_without_path() {
        let mut state = test_state();
        let untitled = state.create_buffer(None, "x");
        state.bind_buffer_to_active_window(untitled);
        let err = state
            .active_buffer_save_snapshot(None)
            .expect_err("snapshot should fail");
        assert_eq!(err, "buffer has no file path");
    }

    #[test]
    fn apply_pending_save_path_should_update_buffer_metadata() {
        let mut state = test_state();
        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let target = PathBuf::from("/tmp/new_name.rs");
        state.set_pending_save_path(buffer_id, Some(target.clone()));
        state.apply_pending_save_path_if_matches(buffer_id);

        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.path, Some(target));
        assert_eq!(buffer.name, "new_name.rs");
    }

    #[test]
    fn all_buffer_save_snapshots_should_skip_untitled_buffers() {
        let mut state = test_state();
        let _untitled = state.create_buffer(None, "u");
        let _named = state.create_buffer(Some(PathBuf::from("/tmp/b.rs")), "b");

        let (snapshots, missing_path) = state.all_buffer_save_snapshots();
        assert_eq!(missing_path, 1);
        assert!(snapshots.len() >= 2);
    }

    #[test]
    fn active_buffer_has_path_should_reflect_current_buffer_binding() {
        let mut state = test_state();
        assert_eq!(state.active_buffer_has_path(), Some(true));

        let untitled = state.create_buffer(None, "u");
        state.bind_buffer_to_active_window(untitled);
        assert_eq!(state.active_buffer_has_path(), Some(false));
    }

    #[test]
    fn cut_current_char_to_slot_should_remove_char_and_store_it() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "abcd");
        state.move_cursor_right();
        state.cut_current_char_to_slot();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "acd");
        assert_eq!(state.line_slot, Some("b".to_string()));
    }

    #[test]
    fn paste_slot_at_cursor_should_insert_slot_text_after_cursor() {
        let mut state = test_state();
        set_active_buffer_text(&mut state, "ad");
        state.line_slot = Some("bc".to_string());
        state.move_cursor_right();
        state.paste_slot_at_cursor();

        let buffer_id = state.active_buffer_id().expect("buffer id exists");
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "adbc");
        assert_eq!(buffer.cursor.row, 1);
        assert_eq!(buffer.cursor.col, 4);
    }
}
