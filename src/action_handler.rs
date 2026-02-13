use crate::action::{
    AppAction, BufferAction, EditorAction, FileAction, FileLoadSource, LayoutAction, SystemAction,
    TabAction, WindowAction,
};
use crate::file_io_service::{FileIo, FileIoServiceError, FileIoState};
use crate::file_watcher_service::{FileWatcher, FileWatcherServiceError, FileWatcherState};
use crate::state::{
    AppState, BufferEditSnapshot, BufferHistoryEntry, BufferId, BufferSwitchDirection, CursorState,
    EditorMode, FocusDirection, NormalSequenceKey, PendingInsertUndoGroup, SplitAxis,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::hash::{Hash, Hasher};
use std::ops::ControlFlow;
use std::path::PathBuf;
use thiserror::Error;
use tracing::error;
use tracing::info;

type NormalKey = NormalSequenceKey;

#[derive(Debug)]
enum SequenceMatch {
    Action(AppAction),
    Pending,
    NoMatch,
}

#[derive(Debug)]
enum PreEditCapture {
    Entry {
        buffer_id: BufferId,
        entry: BufferHistoryEntry,
    },
}

#[derive(Debug, Clone, Copy)]
struct VisualSelectionBounds {
    start: CursorState,
    end: CursorState,
    start_row: usize,
    end_row: usize,
}

#[derive(Debug, Error)]
enum ActionHandlerError {
    #[error("enqueue watch for opened file failed")]
    OpenFileWatch {
        #[source]
        source: FileWatcherServiceError,
    },
    #[error("enqueue initial file load failed")]
    OpenFileLoad {
        #[source]
        source: FileIoServiceError,
    },
    #[error("enqueue external reload failed")]
    ExternalReload {
        #[source]
        source: FileIoServiceError,
    },
    #[error("enqueue watch after save failed")]
    SaveWatch {
        #[source]
        source: FileWatcherServiceError,
    },
    #[error("enqueue unwatch for closed buffer failed")]
    CloseBufferUnwatch {
        #[source]
        source: FileWatcherServiceError,
    },
    #[error("enqueue file save failed")]
    Save {
        #[source]
        source: FileIoServiceError,
    },
    #[error("enqueue file reload failed")]
    Reload {
        #[source]
        source: FileIoServiceError,
    },
    #[error("enqueue file save for :wa failed")]
    SaveAll {
        #[source]
        source: FileIoServiceError,
    },
}

#[derive(dep_inj::DepInj)]
#[target(ActionHandlerImpl)]
pub struct ActionHandlerState {}

impl ActionHandlerState {
    pub fn new() -> Self {
        Self {}
    }
}

impl<Deps> ActionHandlerImpl<Deps>
where
    Deps: AsRef<ActionHandlerState>
        + AsMut<ActionHandlerState>
        + AsMut<AppState>
        + AsRef<FileIoState>
        + AsRef<FileWatcherState>
        + FileIo
        + FileWatcher,
{
    pub(crate) fn apply(&mut self, action: AppAction) -> ControlFlow<()> {
        let mut state = std::mem::take(self.prj_ref_mut().as_mut());
        let flow = self.dispatch_internal(&mut state, action);
        *self.prj_ref_mut().as_mut() = state;
        flow
    }
}

impl Default for ActionHandlerState {
    fn default() -> Self {
        Self::new()
    }
}

impl<Deps> ActionHandlerImpl<Deps>
where
    Deps: AsRef<ActionHandlerState>
        + AsMut<ActionHandlerState>
        + AsMut<AppState>
        + AsRef<FileIoState>
        + AsRef<FileWatcherState>
        + FileIo
        + FileWatcher,
{
    fn text_fingerprint(text: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    fn mark_internal_save(state: &mut AppState, buffer_id: BufferId, text: &str) {
        state
            .last_internal_save_fingerprint
            .insert(buffer_id, Self::text_fingerprint(text));
    }

    fn should_ignore_external_reload(
        state: &mut AppState,
        buffer_id: BufferId,
        text: &str,
    ) -> bool {
        let incoming = Self::text_fingerprint(text);
        let Some(expected) = state
            .last_internal_save_fingerprint
            .get(&buffer_id)
            .copied()
        else {
            return false;
        };
        if expected == incoming {
            state.last_internal_save_fingerprint.remove(&buffer_id);
            return true;
        }
        state.last_internal_save_fingerprint.remove(&buffer_id);
        false
    }

    fn begin_insert_undo_group(state: &mut AppState) {
        if state.pending_insert_group.is_some() {
            return;
        }
        let Some(buffer_id) = state.active_buffer_id() else {
            return;
        };
        state.pending_insert_group = Some(PendingInsertUndoGroup {
            buffer_id,
            before_cursor: state.active_cursor(),
            edits: Vec::new(),
        });
    }

    fn append_insert_undo_edit(
        state: &mut AppState,
        buffer_id: BufferId,
        edit: BufferEditSnapshot,
    ) {
        let Some(group) = state.pending_insert_group.as_mut() else {
            return;
        };
        if group.buffer_id != buffer_id {
            return;
        }
        group.edits.push(edit);
    }

    fn commit_insert_undo_group(state: &mut AppState) {
        let Some(group) = state.pending_insert_group.take() else {
            return;
        };
        if group.edits.is_empty() {
            return;
        }
        let after_cursor = state
            .buffers
            .get(group.buffer_id)
            .map(|buffer| buffer.cursor)
            .unwrap_or(group.before_cursor);
        state.push_buffer_history_entry(
            group.buffer_id,
            BufferHistoryEntry {
                edits: group.edits,
                before_cursor: group.before_cursor,
                after_cursor,
            },
        );
    }

    fn dispatch_internal(&mut self, state: &mut AppState, action: AppAction) -> ControlFlow<()> {
        match action {
            AppAction::Editor(EditorAction::KeyPressed(key)) => {
                return self.handle_key(state, key);
            }
            AppAction::Editor(editor_action) => {
                self.apply_editor_action(state, editor_action);
            }
            AppAction::Layout(LayoutAction::SplitHorizontal) => {
                state.split_active_window(SplitAxis::Horizontal);
            }
            AppAction::Layout(LayoutAction::SplitVertical) => {
                state.split_active_window(SplitAxis::Vertical);
            }
            AppAction::Layout(LayoutAction::ViewportResized { .. }) => {}
            AppAction::Window(WindowAction::FocusLeft) => state.focus_window(FocusDirection::Left),
            AppAction::Window(WindowAction::FocusDown) => state.focus_window(FocusDirection::Down),
            AppAction::Window(WindowAction::FocusUp) => state.focus_window(FocusDirection::Up),
            AppAction::Window(WindowAction::FocusRight) => {
                state.focus_window(FocusDirection::Right)
            }
            AppAction::Window(WindowAction::CloseActive) => state.close_active_window(),
            AppAction::Buffer(BufferAction::SwitchPrev) => {
                state.switch_active_window_buffer(BufferSwitchDirection::Prev);
            }
            AppAction::Buffer(BufferAction::SwitchNext) => {
                state.switch_active_window_buffer(BufferSwitchDirection::Next);
            }
            AppAction::Tab(TabAction::New) => {
                state.open_new_tab();
            }
            AppAction::Tab(TabAction::CloseCurrent) => {
                state.close_current_tab();
            }
            AppAction::Tab(TabAction::SwitchPrev) => {
                state.switch_to_prev_tab();
            }
            AppAction::Tab(TabAction::SwitchNext) => {
                state.switch_to_next_tab();
            }
            AppAction::File(FileAction::LoadCompleted {
                buffer_id,
                source,
                result,
            }) => match (source, result) {
                (FileLoadSource::Open, Ok(text)) => {
                    if let Some(buffer) = state.buffers.get_mut(buffer_id) {
                        buffer.text = text;
                    } else {
                        error!(
                            "load completed for unknown buffer: buffer_id={:?}",
                            buffer_id
                        );
                    }
                    state.set_buffer_dirty(buffer_id, false);
                    state.set_buffer_externally_modified(buffer_id, false);
                    state.status_bar.message = "file loaded".to_string();
                }
                (FileLoadSource::Open, Err(err)) => {
                    error!("file load failed: buffer_id={:?}, error={}", buffer_id, err);
                    state.status_bar.message = format!("load failed: {}", err);
                }
                (FileLoadSource::External, Ok(text)) => {
                    let is_active = state.active_buffer_id() == Some(buffer_id);
                    let Some((current_fingerprint, is_dirty, name)) =
                        state.buffers.get(buffer_id).map(|buffer| {
                            (
                                Self::text_fingerprint(buffer.text.as_str()),
                                buffer.dirty,
                                buffer.name.clone(),
                            )
                        })
                    else {
                        error!(
                            "external changed for unknown buffer: buffer_id={:?}",
                            buffer_id
                        );
                        return ControlFlow::Continue(());
                    };
                    let incoming_fingerprint = Self::text_fingerprint(&text);
                    if current_fingerprint == incoming_fingerprint {
                        state.set_buffer_externally_modified(buffer_id, false);
                        if is_active && state.status_bar.message.starts_with("reloading ") {
                            state.status_bar.message = "file saved".to_string();
                        }
                        return ControlFlow::Continue(());
                    }
                    if is_dirty {
                        state.set_buffer_externally_modified(buffer_id, true);
                        if is_active {
                            state.status_bar.message =
                                "file changed externally; use :w! to overwrite or :e! to reload"
                                    .to_string();
                        }
                        return ControlFlow::Continue(());
                    }
                    if Self::should_ignore_external_reload(state, buffer_id, &text) {
                        state.set_buffer_externally_modified(buffer_id, false);
                        if is_active {
                            state.status_bar.message = "file saved".to_string();
                        }
                        return ControlFlow::Continue(());
                    }
                    state.replace_buffer_text_preserving_cursor(buffer_id, text);
                    state.set_buffer_dirty(buffer_id, false);
                    state.set_buffer_externally_modified(buffer_id, false);
                    if is_active {
                        state.status_bar.message = format!("reloaded {}", name);
                    }
                }
                (FileLoadSource::External, Err(err)) => {
                    error!(
                        "external change reload failed: buffer_id={:?}, error={}",
                        buffer_id, err
                    );
                }
            },
            AppAction::File(FileAction::OpenRequested { path }) => {
                info!("open_file: {}", path.display());
                let normalized_path = AppState::normalize_file_path(path.as_path());
                if let Some(buffer_id) = state.find_buffer_by_path(normalized_path.as_path()) {
                    state.bind_buffer_to_active_window(buffer_id);
                    state.status_bar.message = format!("switched {}", path.display());
                    return ControlFlow::Continue(());
                }
                let buffer_id = state.create_buffer(Some(normalized_path.clone()), String::new());
                state.bind_buffer_to_active_window(buffer_id);
                state.status_bar.message = format!("loading {}", path.display());
                if let Err(source) = self
                    .prj_ref()
                    .enqueue_watch(buffer_id, normalized_path.clone())
                {
                    let err = ActionHandlerError::OpenFileWatch { source };
                    error!(
                        "watch worker unavailable while enqueueing file watch: {}",
                        err
                    );
                }
                if let Err(source) = self.prj_ref().enqueue_load(buffer_id, normalized_path) {
                    let err = ActionHandlerError::OpenFileLoad { source };
                    error!("io worker unavailable while enqueueing file load: {}", err);
                    state.status_bar.message = "load failed: io worker unavailable".to_string();
                }
            }
            AppAction::File(FileAction::ExternalChangeDetected { buffer_id, path }) => {
                if state.in_flight_internal_saves.contains(&buffer_id) {
                    return ControlFlow::Continue(());
                }
                let Some(buffer) = state.buffers.get(buffer_id) else {
                    error!(
                        "external change detected for unknown buffer: buffer_id={:?}",
                        buffer_id
                    );
                    return ControlFlow::Continue(());
                };
                if buffer.dirty {
                    state.set_buffer_externally_modified(buffer_id, true);
                    if state.active_buffer_id() == Some(buffer_id) {
                        state.status_bar.message =
                            "file changed externally; use :w! to overwrite or :e! to reload"
                                .to_string();
                    }
                    return ControlFlow::Continue(());
                }
                if let Err(source) = self.prj_ref().enqueue_external_load(buffer_id, path) {
                    let err = ActionHandlerError::ExternalReload { source };
                    error!(
                        "io worker unavailable while enqueueing external reload: {}",
                        err
                    );
                    if state.active_buffer_id() == Some(buffer_id) {
                        state.status_bar.message =
                            "reload failed: io worker unavailable".to_string();
                    }
                    return ControlFlow::Continue(());
                }
            }
            AppAction::File(FileAction::SaveCompleted { buffer_id, result }) => match result {
                Ok(()) => {
                    state.in_flight_internal_saves.remove(&buffer_id);
                    if !state
                        .last_internal_save_fingerprint
                        .contains_key(&buffer_id)
                    {
                        let text = state
                            .buffers
                            .get(buffer_id)
                            .map(|buffer| buffer.text.clone());
                        if let Some(text) = text {
                            Self::mark_internal_save(state, buffer_id, &text);
                        }
                    }
                    state.apply_pending_save_path_if_matches(buffer_id);
                    if let Some(path) = state
                        .buffers
                        .get(buffer_id)
                        .and_then(|buffer| buffer.path.clone())
                        && let Err(source) = self.prj_ref().enqueue_watch(buffer_id, path)
                    {
                        let err = ActionHandlerError::SaveWatch { source };
                        error!(
                            "watch worker unavailable while enqueueing file watch: {}",
                            err
                        );
                    }
                    state.set_buffer_dirty(buffer_id, false);
                    state.set_buffer_externally_modified(buffer_id, false);
                    state.status_bar.message = "file saved".to_string();
                    if state.quit_after_save {
                        state.quit_after_save = false;
                        return ControlFlow::Break(());
                    }
                }
                Err(err) => {
                    state.in_flight_internal_saves.remove(&buffer_id);
                    state.last_internal_save_fingerprint.remove(&buffer_id);
                    state.quit_after_save = false;
                    state.clear_pending_save_path_if_matches(buffer_id);
                    error!("file save failed: buffer_id={:?} error={}", buffer_id, err);
                    state.status_bar.message = format!("save failed: {}", err);
                }
            },
            AppAction::System(SystemAction::Quit) => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    fn handle_key(&mut self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        if !state.is_visual_mode() {
            state.visual_g_pending = false;
        }

        if key.modifiers.contains(KeyModifiers::ALT) {
            state.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return ControlFlow::Continue(());
        }

        let mode_before = state.mode;
        let pre_edit_capture = Self::capture_pre_edit_snapshot(state, mode_before, key);

        let flow = if state.is_command_mode() {
            state.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            self.handle_command_mode_key(state, key)
        } else if state.is_visual_mode() {
            state.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            self.handle_visual_mode_key(state, key)
        } else if state.is_insert_mode() {
            state.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            let insert_edit = Self::capture_insert_mode_pre_edit(state, key);
            let flow = self.handle_insert_mode_key(state, key);
            if let Some((buffer_id, edit)) = insert_edit {
                Self::append_insert_undo_edit(state, buffer_id, edit);
            }
            flow
        } else {
            self.handle_normal_mode_key(state, key)
        };

        if let Some(PreEditCapture::Entry { buffer_id, entry }) = pre_edit_capture {
            state.push_buffer_history_entry(buffer_id, entry);
        }
        if mode_before == EditorMode::Insert && state.mode != EditorMode::Insert {
            Self::commit_insert_undo_group(state);
        }

        flow
    }

    fn capture_insert_mode_pre_edit(
        state: &AppState,
        key: KeyEvent,
    ) -> Option<(BufferId, BufferEditSnapshot)> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return None;
        }

        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let cursor = buffer.cursor;
        let text = buffer.text.as_str();
        let row_idx = cursor.row.saturating_sub(1) as usize;

        match key.code {
            KeyCode::Char(ch) => {
                let start_byte = cursor_slot_to_byte_idx(text, cursor.row, cursor.col)?;
                Some((
                    buffer_id,
                    BufferEditSnapshot {
                        start_byte,
                        deleted_text: String::new(),
                        inserted_text: ch.to_string(),
                    },
                ))
            }
            KeyCode::Tab => {
                let start_byte = cursor_slot_to_byte_idx(text, cursor.row, cursor.col)?;
                Some((
                    buffer_id,
                    BufferEditSnapshot {
                        start_byte,
                        deleted_text: String::new(),
                        inserted_text: "\t".to_string(),
                    },
                ))
            }
            KeyCode::Enter => {
                let start_byte = cursor_slot_to_byte_idx(text, cursor.row, cursor.col)?;
                Some((
                    buffer_id,
                    BufferEditSnapshot {
                        start_byte,
                        deleted_text: String::new(),
                        inserted_text: "\n".to_string(),
                    },
                ))
            }
            KeyCode::Backspace => {
                if cursor.col > 1 {
                    let line_start = line_start_byte_idx(text, row_idx)?;
                    let line = line_text_at(text, row_idx)?;
                    let start =
                        line_start + char_to_byte_idx(line, cursor.col.saturating_sub(2) as usize);
                    let end =
                        line_start + char_to_byte_idx(line, cursor.col.saturating_sub(1) as usize);
                    Some((
                        buffer_id,
                        BufferEditSnapshot {
                            start_byte: start,
                            deleted_text: text[start..end].to_string(),
                            inserted_text: String::new(),
                        },
                    ))
                } else if cursor.row > 1 {
                    let line_start = line_start_byte_idx(text, row_idx)?;
                    let start = line_start.saturating_sub(1);
                    Some((
                        buffer_id,
                        BufferEditSnapshot {
                            start_byte: start,
                            deleted_text: "\n".to_string(),
                            inserted_text: String::new(),
                        },
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn capture_open_line_below_pre_edit(
        state: &AppState,
    ) -> Option<(BufferId, BufferEditSnapshot)> {
        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let cursor = buffer.cursor;
        let line = line_text_at(&buffer.text, cursor.row.saturating_sub(1) as usize)?;
        let line_start = line_start_byte_idx(&buffer.text, cursor.row.saturating_sub(1) as usize)?;
        let start_byte = line_start.saturating_add(line.len());

        Some((
            buffer_id,
            BufferEditSnapshot {
                start_byte,
                deleted_text: String::new(),
                inserted_text: "\n".to_string(),
            },
        ))
    }

    fn capture_open_line_above_pre_edit(
        state: &AppState,
    ) -> Option<(BufferId, BufferEditSnapshot)> {
        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let cursor = buffer.cursor;
        let start_byte = line_start_byte_idx(&buffer.text, cursor.row.saturating_sub(1) as usize)?;

        Some((
            buffer_id,
            BufferEditSnapshot {
                start_byte,
                deleted_text: String::new(),
                inserted_text: "\n".to_string(),
            },
        ))
    }

    fn single_edit_entry(
        before_cursor: CursorState,
        after_cursor: CursorState,
        start_byte: usize,
        deleted_text: String,
        inserted_text: String,
    ) -> BufferHistoryEntry {
        BufferHistoryEntry {
            edits: vec![BufferEditSnapshot {
                start_byte,
                deleted_text,
                inserted_text,
            }],
            before_cursor,
            after_cursor,
        }
    }

    fn capture_visual_selection_bounds(
        state: &AppState,
        text: &str,
    ) -> Option<VisualSelectionBounds> {
        let anchor = state.visual_anchor?;
        let cursor = state.active_cursor();
        let (start, end) = if (anchor.row, anchor.col) <= (cursor.row, cursor.col) {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        let start_row = start.row.saturating_sub(1) as usize;
        let end_row = end.row.saturating_sub(1) as usize;
        let line_count = text.split('\n').count();
        if start_row >= line_count || end_row >= line_count {
            return None;
        }
        Some(VisualSelectionBounds {
            start,
            end,
            start_row,
            end_row,
        })
    }

    fn visual_line_byte_range(
        text: &str,
        start_row: usize,
        end_row: usize,
        line_count: usize,
    ) -> Option<(usize, usize)> {
        let start_byte = if end_row + 1 < line_count {
            line_start_byte_idx(text, start_row)?
        } else if start_row > 0 {
            line_start_byte_idx(text, start_row)?.saturating_sub(1)
        } else {
            0
        };
        let end_byte = if end_row + 1 < line_count {
            line_start_byte_idx(text, end_row.saturating_add(1))?
        } else {
            text.len()
        };
        Some((start_byte, end_byte))
    }

    fn capture_cut_char_to_slot_pre_edit(
        state: &AppState,
    ) -> Option<(BufferId, BufferHistoryEntry)> {
        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let cursor = buffer.cursor;
        let row_idx = cursor.row.saturating_sub(1) as usize;
        let col_idx = cursor.col.saturating_sub(1) as usize;
        let line = line_text_at(&buffer.text, row_idx)?;
        let char_count = line.chars().count();
        if col_idx >= char_count {
            return None;
        }

        let line_start = line_start_byte_idx(&buffer.text, row_idx)?;
        let start = line_start + char_to_byte_idx(line, col_idx);
        let end = line_start + char_to_byte_idx(line, col_idx.saturating_add(1));
        let deleted_text = buffer.text[start..end].to_string();
        let entry = Self::single_edit_entry(cursor, cursor, start, deleted_text, String::new());
        Some((buffer_id, entry))
    }

    fn capture_delete_current_line_pre_edit(
        state: &AppState,
    ) -> Option<(BufferId, BufferHistoryEntry)> {
        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let cursor = buffer.cursor;
        let row_idx = cursor.row.saturating_sub(1) as usize;

        let mut lines = buffer
            .text
            .split('\n')
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }
        if row_idx >= lines.len() {
            return None;
        }

        let start = if row_idx + 1 < lines.len() {
            line_start_byte_idx(&buffer.text, row_idx)?
        } else if row_idx > 0 {
            line_start_byte_idx(&buffer.text, row_idx)?.saturating_sub(1)
        } else {
            0
        };
        let end = if row_idx + 1 < lines.len() {
            line_start_byte_idx(&buffer.text, row_idx.saturating_add(1))?
        } else {
            buffer.text.len()
        };
        let deleted_text = buffer.text[start..end].to_string();

        lines.remove(row_idx);
        if lines.is_empty() {
            lines.push(String::new());
        }
        let after_text = lines.join("\n");
        let visible_rows = if after_text.is_empty() {
            1
        } else {
            after_text.lines().count().max(1)
        };
        let after_row = row_idx
            .min(visible_rows.saturating_sub(1))
            .saturating_add(1) as u16;
        let after_cursor = CursorState {
            row: after_row,
            col: 1,
        };

        let entry =
            Self::single_edit_entry(cursor, after_cursor, start, deleted_text, String::new());
        Some((buffer_id, entry))
    }

    fn capture_join_line_below_pre_edit(
        state: &AppState,
    ) -> Option<(BufferId, BufferHistoryEntry)> {
        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let cursor = buffer.cursor;
        let row_idx = cursor.row.saturating_sub(1) as usize;

        let current_line = line_text_at(&buffer.text, row_idx)?;
        let next_line = line_text_at(&buffer.text, row_idx.saturating_add(1))?;
        let current_start = line_start_byte_idx(&buffer.text, row_idx)?;
        let boundary = current_start.saturating_add(current_line.len());
        let next_trimmed = next_line.trim_start();
        let trimmed_prefix_len = next_line.len().saturating_sub(next_trimmed.len());
        let deleted_text = format!("\n{}", &next_line[..trimmed_prefix_len]);
        let inserted_text =
            if !current_line.is_empty() && !next_trimmed.is_empty() && !current_line.ends_with(' ')
            {
                " ".to_string()
            } else {
                String::new()
            };

        let mut merged_preview = String::from(current_line);
        if !inserted_text.is_empty() {
            merged_preview.push(' ');
        }
        merged_preview.push_str(next_trimmed);
        let max_col = merged_preview.chars().count() as u16 + 1;
        let after_cursor = CursorState {
            row: cursor.row,
            col: cursor.col.min(max_col).max(1),
        };

        let entry =
            Self::single_edit_entry(cursor, after_cursor, boundary, deleted_text, inserted_text);
        Some((buffer_id, entry))
    }

    fn capture_paste_after_cursor_pre_edit(
        state: &AppState,
    ) -> Option<(BufferId, BufferHistoryEntry)> {
        let slot_text = state.line_slot.as_ref()?;
        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let cursor = buffer.cursor;
        let row_idx = cursor.row.saturating_sub(1) as usize;
        let mut lines = buffer
            .text
            .split('\n')
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }

        if state.line_slot_line_wise {
            let insert_at = row_idx.saturating_add(1).min(lines.len());
            let mut inserted_lines = slot_text
                .split('\n')
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if inserted_lines.is_empty() {
                inserted_lines.push(String::new());
            }
            let inserted_count = inserted_lines.len() as u16;
            let inserted_joined = inserted_lines.join("\n");

            let (start_byte, inserted_text) = if insert_at < lines.len() {
                (
                    line_start_byte_idx(&buffer.text, insert_at)?,
                    format!("{}\n", inserted_joined),
                )
            } else {
                (buffer.text.len(), format!("\n{}", inserted_joined))
            };
            let after_cursor = CursorState {
                row: insert_at as u16 + inserted_count,
                col: 1,
            };

            let entry = Self::single_edit_entry(
                cursor,
                after_cursor,
                start_byte,
                String::new(),
                inserted_text,
            );
            return Some((buffer_id, entry));
        }

        let line = line_text_at(&buffer.text, row_idx)?;
        let line_start = line_start_byte_idx(&buffer.text, row_idx)?;
        let col_idx = cursor.col.saturating_sub(1) as usize;
        let char_count = line.chars().count();
        let insert_char_idx = col_idx.saturating_add(1).min(char_count);
        let start_byte = line_start + char_to_byte_idx(line, insert_char_idx);
        let after_cursor = CursorState {
            row: cursor.row,
            col: cursor.col.saturating_add(slot_text.chars().count() as u16),
        };

        let entry = Self::single_edit_entry(
            cursor,
            after_cursor,
            start_byte,
            String::new(),
            slot_text.clone(),
        );
        Some((buffer_id, entry))
    }

    fn capture_pre_edit_snapshot(
        state: &AppState,
        mode: EditorMode,
        key: KeyEvent,
    ) -> Option<PreEditCapture> {
        match mode {
            EditorMode::Normal => Self::capture_normal_mode_pre_edit_snapshot(state, key),
            EditorMode::VisualChar | EditorMode::VisualLine => {
                if key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Char('d') {
                    let (buffer_id, entry) = Self::capture_visual_delete_pre_edit(state)?;
                    return Some(PreEditCapture::Entry { buffer_id, entry });
                }
                if key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Char('p') {
                    if let Some((buffer_id, entry)) = Self::capture_visual_paste_pre_edit(state) {
                        return Some(PreEditCapture::Entry { buffer_id, entry });
                    }
                    return None;
                }
                None
            }
            EditorMode::Insert | EditorMode::Command => None,
        }
    }

    fn capture_normal_mode_pre_edit_snapshot(
        state: &AppState,
        key: KeyEvent,
    ) -> Option<PreEditCapture> {
        let normal_key = Self::to_normal_key(state, key)?;
        let mut keys = state.normal_sequence.clone();
        keys.push(normal_key);

        let SequenceMatch::Action(AppAction::Editor(editor_action)) =
            Self::resolve_normal_sequence(&keys)
        else {
            return None;
        };
        if !Self::is_immediate_text_mutating_editor_action(editor_action) {
            return None;
        }

        match editor_action {
            EditorAction::CutCharToSlot => {
                let (buffer_id, entry) = Self::capture_cut_char_to_slot_pre_edit(state)?;
                Some(PreEditCapture::Entry { buffer_id, entry })
            }
            EditorAction::DeleteCurrentLineToSlot => {
                let (buffer_id, entry) = Self::capture_delete_current_line_pre_edit(state)?;
                Some(PreEditCapture::Entry { buffer_id, entry })
            }
            EditorAction::JoinLineBelow => {
                let (buffer_id, entry) = Self::capture_join_line_below_pre_edit(state)?;
                Some(PreEditCapture::Entry { buffer_id, entry })
            }
            EditorAction::PasteSlotAfterCursor => {
                let (buffer_id, entry) = Self::capture_paste_after_cursor_pre_edit(state)?;
                Some(PreEditCapture::Entry { buffer_id, entry })
            }
            _ => None,
        }
    }

    fn is_immediate_text_mutating_editor_action(action: EditorAction) -> bool {
        matches!(
            action,
            EditorAction::JoinLineBelow
                | EditorAction::CutCharToSlot
                | EditorAction::PasteSlotAfterCursor
                | EditorAction::DeleteCurrentLineToSlot
        )
    }

    fn capture_visual_delete_pre_edit(state: &AppState) -> Option<(BufferId, BufferHistoryEntry)> {
        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let before_cursor = buffer.cursor;
        let bounds = Self::capture_visual_selection_bounds(state, &buffer.text)?;
        let mut lines = buffer
            .text
            .split('\n')
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }

        if state.is_visual_line_mode() {
            let (start_byte, end_byte) = Self::visual_line_byte_range(
                &buffer.text,
                bounds.start_row,
                bounds.end_row,
                lines.len(),
            )?;
            let deleted_text = buffer.text[start_byte..end_byte].to_string();

            lines.drain(bounds.start_row..=bounds.end_row);
            if lines.is_empty() {
                lines.push(String::new());
            }
            let after_text = lines.join("\n");
            let visible_rows = if after_text.is_empty() {
                1
            } else {
                after_text.lines().count().max(1)
            };
            let after_row = bounds
                .start_row
                .min(visible_rows.saturating_sub(1))
                .saturating_add(1) as u16;
            let after_cursor = CursorState {
                row: after_row,
                col: 1,
            };

            let entry = Self::single_edit_entry(
                before_cursor,
                after_cursor,
                start_byte,
                deleted_text,
                String::new(),
            );
            return Some((buffer_id, entry));
        }

        let start_line_len = lines[bounds.start_row].chars().count() as u16;
        let end_line_len = lines[bounds.end_row].chars().count() as u16;
        let start_col = bounds.start.col.max(1).min(start_line_len.max(1));
        let end_col = bounds.end.col.max(1).min(end_line_len.max(1));

        if bounds.start_row == bounds.end_row {
            let line_start = line_start_byte_idx(&buffer.text, bounds.start_row)?;
            let line = line_text_at(&buffer.text, bounds.start_row)?;
            let start_in_line = char_to_byte_idx(line, start_col.saturating_sub(1) as usize);
            let end_in_line = char_to_byte_idx(line, end_col as usize);
            let start_byte = line_start.saturating_add(start_in_line);
            let end_byte = line_start.saturating_add(end_in_line);
            let deleted_text = buffer.text[start_byte..end_byte].to_string();
            let removed_chars = end_col.saturating_sub(start_col).saturating_add(1);
            let new_len = start_line_len.saturating_sub(removed_chars);
            let after_cursor = CursorState {
                row: bounds.start.row,
                col: start_col.min(new_len.saturating_add(1)),
            };
            let entry = Self::single_edit_entry(
                before_cursor,
                after_cursor,
                start_byte,
                deleted_text,
                String::new(),
            );
            return Some((buffer_id, entry));
        }

        let start_line_start = line_start_byte_idx(&buffer.text, bounds.start_row)?;
        let start_line = line_text_at(&buffer.text, bounds.start_row)?;
        let start_keep = char_to_byte_idx(start_line, start_col.saturating_sub(1) as usize);
        let start_byte = start_line_start.saturating_add(start_keep);

        let end_line_start = line_start_byte_idx(&buffer.text, bounds.end_row)?;
        let end_line = line_text_at(&buffer.text, bounds.end_row)?;
        let end_del = char_to_byte_idx(end_line, end_col as usize);
        let end_byte = end_line_start.saturating_add(end_del);
        let deleted_text = buffer.text[start_byte..end_byte].to_string();

        let merged_prefix = &start_line[..start_keep];
        let merged_suffix = &end_line[end_del..];
        let merged_len =
            merged_prefix.chars().count() as u16 + merged_suffix.chars().count() as u16;
        let after_cursor = CursorState {
            row: bounds.start.row,
            col: start_col.min(merged_len.saturating_add(1)),
        };

        let entry = Self::single_edit_entry(
            before_cursor,
            after_cursor,
            start_byte,
            deleted_text,
            String::new(),
        );
        Some((buffer_id, entry))
    }

    fn capture_visual_paste_pre_edit(state: &AppState) -> Option<(BufferId, BufferHistoryEntry)> {
        let slot_text = state.line_slot.as_ref()?;
        let buffer_id = state.active_buffer_id()?;
        let buffer = state.buffers.get(buffer_id)?;
        let before_cursor = buffer.cursor;
        let bounds = Self::capture_visual_selection_bounds(state, &buffer.text)?;

        let mut lines = buffer
            .text
            .split('\n')
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }

        if state.is_visual_line_mode() {
            let (start_byte, end_byte) = Self::visual_line_byte_range(
                &buffer.text,
                bounds.start_row,
                bounds.end_row,
                lines.len(),
            )?;
            let deleted_text = buffer.text[start_byte..end_byte].to_string();
            let mut replacement = slot_text
                .split('\n')
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if replacement.is_empty() {
                replacement.push(String::new());
            }
            let inserted_text = replacement.join("\n");
            let after_cursor = CursorState {
                row: bounds.start_row.saturating_add(1) as u16,
                col: 1,
            };
            let entry = Self::single_edit_entry(
                before_cursor,
                after_cursor,
                start_byte,
                deleted_text,
                inserted_text,
            );
            return Some((buffer_id, entry));
        }

        let start_line = line_text_at(&buffer.text, bounds.start_row)?;
        let end_line = line_text_at(&buffer.text, bounds.end_row)?;
        let start_line_len = start_line.chars().count() as u16;
        let end_line_len = end_line.chars().count() as u16;
        let start_col = bounds.start.col.max(1).min(start_line_len.max(1));
        let end_col = bounds.end.col.max(1).min(end_line_len.max(1));
        let start_line_start = line_start_byte_idx(&buffer.text, bounds.start_row)?;
        let end_line_start = line_start_byte_idx(&buffer.text, bounds.end_row)?;
        let start_in_line = char_to_byte_idx(start_line, start_col.saturating_sub(1) as usize);
        let end_in_line = char_to_byte_idx(end_line, end_col as usize);
        let start_byte = start_line_start.saturating_add(start_in_line);
        let end_byte = end_line_start.saturating_add(end_in_line);
        let deleted_text = buffer.text[start_byte..end_byte].to_string();
        let after_cursor = CursorState {
            row: bounds.start_row.saturating_add(1) as u16,
            col: start_col,
        };
        let entry = Self::single_edit_entry(
            before_cursor,
            after_cursor,
            start_byte,
            deleted_text,
            slot_text.clone(),
        );
        Some((buffer_id, entry))
    }

    fn handle_normal_mode_key(&mut self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        let Some(normal_key) = Self::to_normal_key(state, key) else {
            state.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return ControlFlow::Continue(());
        };

        state.normal_sequence.push(normal_key);

        loop {
            match Self::resolve_normal_sequence(&state.normal_sequence) {
                SequenceMatch::Action(action) => {
                    state.normal_sequence.clear();
                    state.status_bar.key_sequence.clear();
                    return self.dispatch_internal(state, action);
                }
                SequenceMatch::Pending => {
                    state.status_bar.key_sequence =
                        Self::render_normal_sequence(&state.normal_sequence);
                    return ControlFlow::Continue(());
                }
                SequenceMatch::NoMatch => {
                    if state.normal_sequence.len() <= 1 {
                        state.normal_sequence.clear();
                        state.status_bar.key_sequence.clear();
                        return ControlFlow::Continue(());
                    }
                    let last = *state
                        .normal_sequence
                        .last()
                        .expect("normal sequence has at least one key");
                    state.normal_sequence.clear();
                    state.normal_sequence.push(last);
                    state.status_bar.key_sequence =
                        Self::render_normal_sequence(&state.normal_sequence);
                }
            }
        }
    }

    fn to_normal_key(state: &AppState, key: KeyEvent) -> Option<NormalKey> {
        if key.modifiers.contains(KeyModifiers::ALT) {
            return None;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char(ch) = key.code {
                return Some(NormalKey::Ctrl(ch.to_ascii_lowercase()));
            }
            return None;
        }

        if let KeyCode::Char(ch) = key.code {
            if ch == state.leader_key {
                return Some(NormalKey::Leader);
            }
            let normalized =
                if key.modifiers.contains(KeyModifiers::SHIFT) && ch.is_ascii_lowercase() {
                    ch.to_ascii_uppercase()
                } else {
                    ch
                };
            return Some(NormalKey::Char(normalized));
        }

        if key.code == KeyCode::Tab {
            return Some(NormalKey::Tab);
        }

        None
    }

    fn resolve_normal_sequence(keys: &[NormalKey]) -> SequenceMatch {
        use NormalKey as K;

        match keys {
            [K::Leader] => SequenceMatch::Pending,
            [K::Leader, K::Char('w')] => SequenceMatch::Pending,
            [K::Leader, K::Char('w'), K::Char('v')] => {
                SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitVertical))
            }
            [K::Leader, K::Char('w'), K::Char('h')] => {
                SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitHorizontal))
            }
            [K::Leader, K::Tab] => SequenceMatch::Pending,
            [K::Leader, K::Tab, K::Char('n')] => {
                SequenceMatch::Action(AppAction::Tab(TabAction::New))
            }
            [K::Leader, K::Tab, K::Char('d')] => {
                SequenceMatch::Action(AppAction::Tab(TabAction::CloseCurrent))
            }
            [K::Leader, K::Tab, K::Char('[')] => {
                SequenceMatch::Action(AppAction::Tab(TabAction::SwitchPrev))
            }
            [K::Leader, K::Tab, K::Char(']')] => {
                SequenceMatch::Action(AppAction::Tab(TabAction::SwitchNext))
            }
            [K::Leader, K::Char('b')] => SequenceMatch::Pending,
            [K::Leader, K::Char('b'), K::Char('d')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::CloseActiveBuffer))
            }
            [K::Leader, K::Char('b'), K::Char('n')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::NewEmptyBuffer))
            }
            [K::Char('d')] => SequenceMatch::Pending,
            [K::Char('d'), K::Char('d')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::DeleteCurrentLineToSlot))
            }
            [K::Char('i')] => SequenceMatch::Action(AppAction::Editor(EditorAction::EnterInsert)),
            [K::Char('a')] => SequenceMatch::Action(AppAction::Editor(EditorAction::AppendInsert)),
            [K::Char('o')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::OpenLineBelowInsert))
            }
            [K::Char('O')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::OpenLineAboveInsert))
            }
            [K::Char(':')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::EnterCommandMode))
            }
            [K::Char('v')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::EnterVisualMode))
            }
            [K::Char('V')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::EnterVisualLineMode))
            }
            [K::Char('u')] => SequenceMatch::Action(AppAction::Editor(EditorAction::Undo)),
            [K::Char('h')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveLeft)),
            [K::Char('0')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveLineStart)),
            [K::Char('$')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveLineEnd)),
            [K::Char('j')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveDown)),
            [K::Char('k')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveUp)),
            [K::Char('l')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveRight)),
            [K::Char('g')] => SequenceMatch::Pending,
            [K::Char('g'), K::Char('g')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileStart))
            }
            [K::Char('G')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileEnd)),
            [K::Char('J')] => SequenceMatch::Action(AppAction::Editor(EditorAction::JoinLineBelow)),
            [K::Char('x')] => SequenceMatch::Action(AppAction::Editor(EditorAction::CutCharToSlot)),
            [K::Char('p')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::PasteSlotAfterCursor))
            }
            [K::Char('H')] => SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchPrev)),
            [K::Char('L')] => SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchNext)),
            [K::Char('{')] => SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchPrev)),
            [K::Char('}')] => SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchNext)),
            [K::Ctrl('h')] => SequenceMatch::Action(AppAction::Window(WindowAction::FocusLeft)),
            [K::Ctrl('j')] => SequenceMatch::Action(AppAction::Window(WindowAction::FocusDown)),
            [K::Ctrl('k')] => SequenceMatch::Action(AppAction::Window(WindowAction::FocusUp)),
            [K::Ctrl('l')] => SequenceMatch::Action(AppAction::Window(WindowAction::FocusRight)),
            [K::Ctrl('e')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewDown))
            }
            [K::Ctrl('y')] => SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewUp)),
            [K::Ctrl('d')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageDown))
            }
            [K::Ctrl('u')] => {
                SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageUp))
            }
            [K::Ctrl('r')] => SequenceMatch::Action(AppAction::Editor(EditorAction::Redo)),
            _ => SequenceMatch::NoMatch,
        }
    }

    fn render_normal_sequence(keys: &[NormalKey]) -> String {
        keys.iter()
            .map(|key| match key {
                NormalKey::Leader => "<leader>".to_string(),
                NormalKey::Tab => "<tab>".to_string(),
                NormalKey::Char(ch) => ch.to_string(),
                NormalKey::Ctrl(ch) => format!("<C-{}>", ch),
            })
            .collect::<Vec<_>>()
            .join("")
    }

    fn apply_editor_action(&mut self, state: &mut AppState, action: EditorAction) {
        match action {
            EditorAction::KeyPressed(_) => {}
            EditorAction::EnterInsert => {
                Self::begin_insert_undo_group(state);
                state.enter_insert_mode();
            }
            EditorAction::AppendInsert => {
                Self::begin_insert_undo_group(state);
                state.move_cursor_right_for_insert();
                state.enter_insert_mode();
            }
            EditorAction::OpenLineBelowInsert => {
                Self::begin_insert_undo_group(state);
                let pre_edit = Self::capture_open_line_below_pre_edit(state);
                state.open_line_below_at_cursor();
                if let Some((buffer_id, edit)) = pre_edit {
                    Self::append_insert_undo_edit(state, buffer_id, edit);
                }
                state.enter_insert_mode();
            }
            EditorAction::OpenLineAboveInsert => {
                Self::begin_insert_undo_group(state);
                let pre_edit = Self::capture_open_line_above_pre_edit(state);
                state.open_line_above_at_cursor();
                if let Some((buffer_id, edit)) = pre_edit {
                    Self::append_insert_undo_edit(state, buffer_id, edit);
                }
                state.enter_insert_mode();
            }
            EditorAction::EnterCommandMode => state.enter_command_mode(),
            EditorAction::EnterVisualMode => state.enter_visual_mode(),
            EditorAction::EnterVisualLineMode => state.enter_visual_line_mode(),
            EditorAction::MoveLeft => state.move_cursor_left(),
            EditorAction::MoveLineStart => state.move_cursor_line_start(),
            EditorAction::MoveLineEnd => state.move_cursor_line_end(),
            EditorAction::MoveDown => state.move_cursor_down(),
            EditorAction::MoveUp => state.move_cursor_up(),
            EditorAction::MoveRight => state.move_cursor_right(),
            EditorAction::MoveFileStart => state.move_cursor_file_start(),
            EditorAction::MoveFileEnd => state.move_cursor_file_end(),
            EditorAction::ScrollViewDown => state.scroll_view_down_one_line(),
            EditorAction::ScrollViewUp => state.scroll_view_up_one_line(),
            EditorAction::ScrollViewHalfPageDown => state.scroll_view_down_half_page(),
            EditorAction::ScrollViewHalfPageUp => state.scroll_view_up_half_page(),
            EditorAction::Undo => state.undo_active_buffer_edit(),
            EditorAction::Redo => state.redo_active_buffer_edit(),
            EditorAction::JoinLineBelow => state.join_line_below_at_cursor(),
            EditorAction::CutCharToSlot => state.cut_current_char_to_slot(),
            EditorAction::PasteSlotAfterCursor => state.paste_slot_at_cursor(),
            EditorAction::DeleteCurrentLineToSlot => state.delete_current_line_to_slot(),
            EditorAction::CloseActiveBuffer => {
                let closed_buffer_id = state.active_buffer_id();
                state.close_active_buffer();
                if let Some(buffer_id) = closed_buffer_id
                    && let Err(source) = self.prj_ref().enqueue_unwatch(buffer_id)
                {
                    let err = ActionHandlerError::CloseBufferUnwatch { source };
                    error!(
                        "watch worker unavailable while enqueueing file unwatch: {}",
                        err
                    );
                }
            }
            EditorAction::NewEmptyBuffer => {
                state.create_untitled_buffer();
            }
        }
    }

    fn handle_insert_mode_key(&self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return ControlFlow::Continue(());
        }

        match key.code {
            KeyCode::Esc => {
                state.exit_insert_mode();
            }
            KeyCode::Enter => {
                state.insert_newline_at_cursor();
            }
            KeyCode::Backspace => {
                state.backspace_at_cursor();
            }
            KeyCode::Left => state.move_cursor_left(),
            KeyCode::Down => state.move_cursor_down(),
            KeyCode::Up => state.move_cursor_up(),
            KeyCode::Right => state.move_cursor_right_for_insert(),
            KeyCode::Tab => state.insert_char_at_cursor('\t'),
            KeyCode::Char(ch) => {
                state.insert_char_at_cursor(ch);
            }
            _ => {}
        }

        ControlFlow::Continue(())
    }

    fn handle_visual_mode_key(&mut self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            state.visual_g_pending = false;
            match key.code {
                KeyCode::Char('e') => state.scroll_view_down_one_line(),
                KeyCode::Char('y') => state.scroll_view_up_one_line(),
                KeyCode::Char('d') => state.scroll_view_down_half_page(),
                KeyCode::Char('u') => state.scroll_view_up_half_page(),
                _ => {}
            }
            return ControlFlow::Continue(());
        }

        match key.code {
            KeyCode::Esc => {
                state.visual_g_pending = false;
                state.exit_visual_mode();
            }
            KeyCode::Char('v') => state.enter_visual_line_mode(),
            KeyCode::Char('V') => state.enter_visual_line_mode(),
            KeyCode::Char('d') => state.delete_visual_selection_to_slot(),
            KeyCode::Char('y') => state.yank_visual_selection_to_slot(),
            KeyCode::Char('p') => state.replace_visual_selection_with_slot(),
            KeyCode::Char('h') => {
                if state.is_visual_line_mode() {
                    state.move_cursor_left();
                } else {
                    state.move_cursor_left_for_visual_char();
                }
            }
            KeyCode::Char('j') => state.move_cursor_down(),
            KeyCode::Char('k') => state.move_cursor_up(),
            KeyCode::Char('l') => {
                if state.is_visual_line_mode() {
                    state.move_cursor_right();
                } else {
                    state.move_cursor_right_for_visual_char();
                }
            }
            KeyCode::Char('0') => state.move_cursor_line_start(),
            KeyCode::Char('$') => state.move_cursor_line_end(),
            KeyCode::Char('g') => {
                if state.visual_g_pending {
                    state.visual_g_pending = false;
                    state.move_cursor_file_start();
                } else {
                    state.visual_g_pending = true;
                }
                return ControlFlow::Continue(());
            }
            KeyCode::Char('G') => state.move_cursor_file_end(),
            _ => {}
        }
        state.visual_g_pending = false;
        ControlFlow::Continue(())
    }

    fn handle_command_mode_key(&mut self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return ControlFlow::Continue(());
        }

        match key.code {
            KeyCode::Esc => state.exit_command_mode(),
            KeyCode::Enter => {
                let command = state.take_command_line();
                match command.as_str() {
                    "" => {}
                    "qa" => {
                        return self
                            .dispatch_internal(state, AppAction::System(SystemAction::Quit));
                    }
                    "q!" | "quit!" => {
                        if state.active_tab_window_ids().len() > 1 {
                            return self.dispatch_internal(
                                state,
                                AppAction::Window(WindowAction::CloseActive),
                            );
                        } else if state.tabs.len() > 1 {
                            return self
                                .dispatch_internal(state, AppAction::Tab(TabAction::CloseCurrent));
                        } else {
                            return self
                                .dispatch_internal(state, AppAction::System(SystemAction::Quit));
                        }
                    }
                    "q" | "quit" => {
                        if state.has_dirty_buffers() {
                            state.status_bar.message =
                                "quit blocked: unsaved changes (use :q!)".to_string();
                            return ControlFlow::Continue(());
                        }
                        if state.active_tab_window_ids().len() > 1 {
                            return self.dispatch_internal(
                                state,
                                AppAction::Window(WindowAction::CloseActive),
                            );
                        } else if state.tabs.len() > 1 {
                            return self
                                .dispatch_internal(state, AppAction::Tab(TabAction::CloseCurrent));
                        } else {
                            return self
                                .dispatch_internal(state, AppAction::System(SystemAction::Quit));
                        }
                    }
                    "w" => {
                        self.enqueue_save_active_buffer(state, false, false, None);
                    }
                    "w!" => {
                        self.enqueue_save_active_buffer(state, false, true, None);
                    }
                    "wa" => {
                        self.enqueue_save_all_buffers(state);
                    }
                    "wq" => {
                        self.enqueue_save_active_buffer(state, true, false, None);
                    }
                    "wq!" => {
                        self.enqueue_save_active_buffer(state, true, true, None);
                    }
                    "e" => {
                        self.enqueue_reload_active_buffer(state, false);
                    }
                    "e!" => {
                        self.enqueue_reload_active_buffer(state, true);
                    }
                    _ if command.starts_with("e ") => {
                        let path = command[2..].trim();
                        if path.is_empty() {
                            state.status_bar.message = "open failed: empty path".to_string();
                        } else {
                            return self.dispatch_internal(
                                state,
                                AppAction::File(FileAction::OpenRequested {
                                    path: PathBuf::from(path),
                                }),
                            );
                        }
                    }
                    _ if command.starts_with("w ") => {
                        let path = command[2..].trim();
                        if path.is_empty() {
                            state.status_bar.message = "save failed: empty path".to_string();
                        } else {
                            self.enqueue_save_active_buffer(
                                state,
                                false,
                                false,
                                Some(PathBuf::from(path)),
                            );
                        }
                    }
                    _ if command.starts_with("w! ") => {
                        let path = command[3..].trim();
                        if path.is_empty() {
                            state.status_bar.message = "save failed: empty path".to_string();
                        } else {
                            self.enqueue_save_active_buffer(
                                state,
                                false,
                                true,
                                Some(PathBuf::from(path)),
                            );
                        }
                    }
                    _ if command.starts_with("wq ") => {
                        let path = command[3..].trim();
                        if path.is_empty() {
                            state.status_bar.message = "save failed: empty path".to_string();
                        } else {
                            self.enqueue_save_active_buffer(
                                state,
                                true,
                                false,
                                Some(PathBuf::from(path)),
                            );
                        }
                    }
                    _ if command.starts_with("wq! ") => {
                        let path = command[4..].trim();
                        if path.is_empty() {
                            state.status_bar.message = "save failed: empty path".to_string();
                        } else {
                            self.enqueue_save_active_buffer(
                                state,
                                true,
                                true,
                                Some(PathBuf::from(path)),
                            );
                        }
                    }
                    _ => {
                        state.status_bar.message = format!("unknown command: {}", command);
                    }
                }
            }
            KeyCode::Backspace => state.pop_command_char(),
            KeyCode::Char(ch) => state.push_command_char(ch),
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn enqueue_save_active_buffer(
        &mut self,
        state: &mut AppState,
        quit_after_save: bool,
        force_overwrite: bool,
        path_override: Option<PathBuf>,
    ) {
        if !force_overwrite
            && path_override.is_none()
            && matches!(state.active_buffer_is_externally_modified(), Some(true))
        {
            state.status_bar.message =
                "save blocked: file changed externally (use :w! to overwrite)".to_string();
            state.quit_after_save = false;
            return;
        }

        let bind_override_path = matches!(
            (path_override.as_ref(), state.active_buffer_has_path()),
            (Some(_), Some(false))
        );
        let (buffer_id, path, text) = match state.active_buffer_save_snapshot(path_override.clone())
        {
            Ok(snapshot) => snapshot,
            Err(reason) => {
                state.status_bar.message = format!("save failed: {}", reason);
                state.quit_after_save = false;
                return;
            }
        };

        Self::mark_internal_save(state, buffer_id, &text);
        state.in_flight_internal_saves.insert(buffer_id);
        if let Err(source) = self.prj_ref().enqueue_save(buffer_id, path, text) {
            let err = ActionHandlerError::Save { source };
            error!("io worker unavailable while enqueueing file save: {}", err);
            state.status_bar.message = "save failed: io worker unavailable".to_string();
            state.in_flight_internal_saves.remove(&buffer_id);
            state.last_internal_save_fingerprint.remove(&buffer_id);
            state.quit_after_save = false;
            return;
        }

        if bind_override_path {
            state.set_pending_save_path(buffer_id, path_override);
        } else {
            state.set_pending_save_path(buffer_id, None);
        }
        state.quit_after_save = quit_after_save;
        state.status_bar.message = "saving...".to_string();
    }

    fn enqueue_reload_active_buffer(&mut self, state: &mut AppState, force_reload: bool) {
        let active_is_dirty = state
            .active_buffer_id()
            .and_then(|id| state.buffers.get(id))
            .map(|buffer| buffer.dirty)
            .unwrap_or(false);
        if !force_reload && active_is_dirty {
            state.status_bar.message =
                "reload blocked: buffer is dirty (use :e! to force reload)".to_string();
            return;
        }

        let (buffer_id, path) = match state.active_buffer_load_target() {
            Ok(target) => target,
            Err(reason) => {
                state.status_bar.message = format!("reload failed: {}", reason);
                return;
            }
        };

        if let Err(source) = self.prj_ref().enqueue_load(buffer_id, path.clone()) {
            let err = ActionHandlerError::Reload { source };
            error!("io worker unavailable while enqueueing file load: {}", err);
            state.status_bar.message = "reload failed: io worker unavailable".to_string();
            return;
        }
        state.status_bar.message = format!("loading {}", path.display());
    }

    fn enqueue_save_all_buffers(&mut self, state: &mut AppState) {
        let (snapshots, missing_path) = state.all_buffer_save_snapshots();
        if snapshots.is_empty() {
            if missing_path > 0 {
                state.status_bar.message = "save failed: no buffer has file path".to_string();
            } else {
                state.status_bar.message = "nothing to save".to_string();
            }
            return;
        }

        let mut enqueued = 0usize;
        for (buffer_id, path, text) in snapshots {
            Self::mark_internal_save(state, buffer_id, &text);
            state.in_flight_internal_saves.insert(buffer_id);
            if let Err(source) = self.prj_ref().enqueue_save(buffer_id, path, text) {
                let err = ActionHandlerError::SaveAll { source };
                error!("io worker unavailable while enqueueing file save: {}", err);
                state.status_bar.message = "save failed: io worker unavailable".to_string();
                state.in_flight_internal_saves.remove(&buffer_id);
                state.last_internal_save_fingerprint.remove(&buffer_id);
                state.quit_after_save = false;
                return;
            }
            enqueued = enqueued.saturating_add(1);
        }

        state.quit_after_save = false;
        if missing_path > 0 {
            state.status_bar.message = format!(
                "saving {} buffers ({} skipped: no path)",
                enqueued, missing_path
            );
        } else {
            state.status_bar.message = format!("saving {} buffers...", enqueued);
        }
    }
}

fn line_start_byte_idx(text: &str, row_idx: usize) -> Option<usize> {
    if row_idx == 0 {
        return Some(0);
    }

    let mut row = 0usize;
    for (idx, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            row = row.saturating_add(1);
            if row == row_idx {
                return Some(idx.saturating_add(1));
            }
        }
    }
    None
}

fn line_text_at(text: &str, row_idx: usize) -> Option<&str> {
    if text.is_empty() && row_idx == 0 {
        return Some("");
    }
    if let Some(line) = text.lines().nth(row_idx) {
        return Some(line);
    }
    if text.ends_with('\n') && row_idx == text.lines().count() {
        return Some("");
    }
    None
}

fn cursor_slot_to_byte_idx(text: &str, row: u16, col: u16) -> Option<usize> {
    let row_idx = row.saturating_sub(1) as usize;
    let line_start = line_start_byte_idx(text, row_idx)?;
    let line = line_text_at(text, row_idx)?;
    let in_line = char_to_byte_idx(line, col.saturating_sub(1) as usize);
    Some(line_start.saturating_add(in_line))
}

fn char_to_byte_idx(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::{ActionHandlerImpl, ActionHandlerState, NormalKey, SequenceMatch};
    use crate::action::{AppAction, BufferAction, EditorAction, LayoutAction, TabAction};
    use crate::file_io_service::{FileIo, FileIoImpl, FileIoServiceError, FileIoState};
    use crate::file_watcher_service::{
        FileWatcher, FileWatcherImpl, FileWatcherServiceError, FileWatcherState,
    };
    use crate::state::{AppState, BufferId};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::ops::ControlFlow;
    use std::path::PathBuf;

    struct TestHarness<'a> {
        app_state: AppState,
        action_handler_state: ActionHandlerState,
        io: &'a FileIoState,
        watcher: &'a FileWatcherState,
    }

    impl AsRef<ActionHandlerState> for TestHarness<'_> {
        fn as_ref(&self) -> &ActionHandlerState {
            &self.action_handler_state
        }
    }

    impl AsMut<ActionHandlerState> for TestHarness<'_> {
        fn as_mut(&mut self) -> &mut ActionHandlerState {
            &mut self.action_handler_state
        }
    }

    impl AsMut<AppState> for TestHarness<'_> {
        fn as_mut(&mut self) -> &mut AppState {
            &mut self.app_state
        }
    }

    impl AsRef<FileIoState> for TestHarness<'_> {
        fn as_ref(&self) -> &FileIoState {
            self.io
        }
    }

    impl AsRef<FileWatcherState> for TestHarness<'_> {
        fn as_ref(&self) -> &FileWatcherState {
            self.watcher
        }
    }

    impl FileIo for TestHarness<'_> {
        fn enqueue_load(
            &self,
            buffer_id: BufferId,
            path: PathBuf,
        ) -> Result<(), FileIoServiceError> {
            FileIoImpl::inj_ref(self).enqueue_load(buffer_id, path)
        }

        fn enqueue_save(
            &self,
            buffer_id: BufferId,
            path: PathBuf,
            text: String,
        ) -> Result<(), FileIoServiceError> {
            FileIoImpl::inj_ref(self).enqueue_save(buffer_id, path, text)
        }

        fn enqueue_external_load(
            &self,
            buffer_id: BufferId,
            path: PathBuf,
        ) -> Result<(), FileIoServiceError> {
            FileIoImpl::inj_ref(self).enqueue_external_load(buffer_id, path)
        }
    }

    impl FileWatcher for TestHarness<'_> {
        fn enqueue_watch(
            &self,
            buffer_id: BufferId,
            path: PathBuf,
        ) -> Result<(), FileWatcherServiceError> {
            FileWatcherImpl::inj_ref(self).enqueue_watch(buffer_id, path)
        }

        fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherServiceError> {
            FileWatcherImpl::inj_ref(self).enqueue_unwatch(buffer_id)
        }
    }

    fn dispatch_test_action(
        handler: &mut ActionHandlerState,
        state: &mut AppState,
        file_io_service: &FileIoState,
        file_watcher_service: &FileWatcherState,
        action: AppAction,
    ) -> ControlFlow<()> {
        let mut harness = TestHarness {
            app_state: std::mem::take(state),
            action_handler_state: std::mem::take(handler),
            io: file_io_service,
            watcher: file_watcher_service,
        };
        let flow = ActionHandlerImpl::inj_ref_mut(&mut harness).apply(action);
        *state = harness.app_state;
        *handler = harness.action_handler_state;
        flow
    }

    fn map_normal_key(state: &AppState, key: KeyEvent) -> Option<NormalKey> {
        ActionHandlerImpl::<TestHarness<'_>>::to_normal_key(state, key)
    }

    fn resolve_keys(keys: &[NormalKey]) -> SequenceMatch {
        ActionHandlerImpl::<TestHarness<'_>>::resolve_normal_sequence(keys)
    }

    #[test]
    fn to_normal_key_should_map_leader_char_to_leader_token() {
        let mut state = AppState::new();
        state.leader_key = ' ';
        let key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);

        let mapped = map_normal_key(&state, key);
        assert_eq!(mapped, Some(NormalKey::Leader));
    }

    #[test]
    fn resolve_normal_sequence_should_keep_leader_w_pending() {
        let seq = vec![NormalKey::Leader, NormalKey::Char('w')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(resolved, SequenceMatch::Pending));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_w_v_to_split_vertical() {
        let seq = vec![
            NormalKey::Leader,
            NormalKey::Char('w'),
            NormalKey::Char('v'),
        ];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitVertical))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_w_h_to_split_horizontal() {
        let seq = vec![
            NormalKey::Leader,
            NormalKey::Char('w'),
            NormalKey::Char('h'),
        ];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitHorizontal))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_tab_n_to_new_tab() {
        let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('n')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Tab(TabAction::New))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_tab_d_to_close_tab() {
        let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('d')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Tab(TabAction::CloseCurrent))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_tab_left_bracket_to_prev_tab() {
        let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('[')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Tab(TabAction::SwitchPrev))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_tab_right_bracket_to_next_tab() {
        let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char(']')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Tab(TabAction::SwitchNext))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_upper_h_to_prev_buffer() {
        let seq = vec![NormalKey::Char('H')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchPrev))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_upper_l_to_next_buffer() {
        let seq = vec![NormalKey::Char('L')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchNext))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_gg_to_move_file_start() {
        let seq = vec![NormalKey::Char('g'), NormalKey::Char('g')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileStart))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_upper_g_to_move_file_end() {
        let seq = vec![NormalKey::Char('G')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileEnd))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_upper_j_to_join_line_below() {
        let seq = vec![NormalKey::Char('J')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::JoinLineBelow))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_upper_v_to_enter_visual_line_mode() {
        let seq = vec![NormalKey::Char('V')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::EnterVisualLineMode))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_u_to_undo() {
        let seq = vec![NormalKey::Char('u')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::Undo))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_ctrl_e_to_scroll_view_down() {
        let seq = vec![NormalKey::Ctrl('e')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewDown))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_ctrl_y_to_scroll_view_up() {
        let seq = vec![NormalKey::Ctrl('y')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewUp))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_ctrl_d_to_scroll_view_half_page_down() {
        let seq = vec![NormalKey::Ctrl('d')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageDown))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_ctrl_u_to_scroll_view_half_page_up() {
        let seq = vec![NormalKey::Ctrl('u')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageUp))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_ctrl_r_to_redo() {
        let seq = vec![NormalKey::Ctrl('r')];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::Redo))
        ));
    }

    #[test]
    fn to_normal_key_should_map_shift_g_to_upper_g() {
        let state = AppState::new();
        let key = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SHIFT);
        let mapped = map_normal_key(&state, key);
        assert_eq!(mapped, Some(NormalKey::Char('G')));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_b_d_to_close_active_buffer() {
        let seq = vec![
            NormalKey::Leader,
            NormalKey::Char('b'),
            NormalKey::Char('d'),
        ];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::CloseActiveBuffer))
        ));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_b_n_to_new_empty_buffer() {
        let seq = vec![
            NormalKey::Leader,
            NormalKey::Char('b'),
            NormalKey::Char('n'),
        ];
        let resolved = resolve_keys(&seq);
        assert!(matches!(
            resolved,
            SequenceMatch::Action(AppAction::Editor(EditorAction::NewEmptyBuffer))
        ));
    }

    #[test]
    fn file_load_completed_should_mark_buffer_clean() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, true);
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::LoadCompleted {
                buffer_id,
                source: crate::action::FileLoadSource::Open,
                result: Ok("loaded".to_string()),
            }),
        );

        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert!(!buffer.dirty);
    }

    #[test]
    fn file_save_completed_should_mark_buffer_clean() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, true);
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::SaveCompleted {
                buffer_id,
                result: Ok(()),
            }),
        );

        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert!(!buffer.dirty);
    }

    #[test]
    fn external_changed_should_be_ignored_when_it_matches_internal_save_fingerprint() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "old");
        state.bind_buffer_to_active_window(buffer_id);

        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::SaveCompleted {
                buffer_id,
                result: Ok(()),
            }),
        );

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::LoadCompleted {
                buffer_id,
                source: crate::action::FileLoadSource::External,
                result: Ok("old".to_string()),
            }),
        );

        assert_eq!(state.status_bar.message, "file saved");
        assert_eq!(
            state.buffers.get(buffer_id).expect("buffer exists").text,
            "old"
        );
    }

    #[test]
    fn external_changed_should_reload_when_content_differs_from_internal_save_fingerprint() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "old");
        state.bind_buffer_to_active_window(buffer_id);

        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::SaveCompleted {
                buffer_id,
                result: Ok(()),
            }),
        );

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::LoadCompleted {
                buffer_id,
                source: crate::action::FileLoadSource::External,
                result: Ok("new".to_string()),
            }),
        );

        assert_eq!(
            state.buffers.get(buffer_id).expect("buffer exists").text,
            "new"
        );
    }

    #[test]
    fn internal_save_echo_should_not_leave_reloading_message() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let path = PathBuf::from("a.txt");
        let buffer_id = state.create_buffer(Some(path.clone()), "old");
        state.bind_buffer_to_active_window(buffer_id);

        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::SaveCompleted {
                buffer_id,
                result: Ok(()),
            }),
        );
        assert_eq!(state.status_bar.message, "file saved");

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path }),
        );
        assert_eq!(state.status_bar.message, "file saved");

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::LoadCompleted {
                buffer_id,
                source: crate::action::FileLoadSource::External,
                result: Ok("old".to_string()),
            }),
        );
        assert_eq!(state.status_bar.message, "file saved");
    }

    #[test]
    fn external_change_detected_should_be_ignored_while_internal_save_in_flight() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let path = PathBuf::from("a.txt");
        let buffer_id = state.create_buffer(Some(path.clone()), "old");
        state.bind_buffer_to_active_window(buffer_id);
        state.in_flight_internal_saves.insert(buffer_id);

        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path }),
        );

        assert_ne!(state.status_bar.message, "reloading a.txt");
    }

    #[test]
    fn command_q_should_be_blocked_when_any_buffer_is_dirty() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "abc");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, true);
        state.enter_command_mode();
        state.push_command_char('q');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        assert_eq!(
            state.status_bar.message,
            "quit blocked: unsaved changes (use :q!)"
        );
    }

    #[test]
    fn command_q_bang_should_force_quit_when_buffer_is_dirty() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "abc");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, true);
        state.enter_command_mode();
        state.push_command_char('q');
        state.push_command_char('!');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Break(())));
    }

    #[test]
    fn command_q_should_quit_when_all_buffers_are_clean() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "abc");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, false);
        state.enter_command_mode();
        state.push_command_char('q');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Break(())));
    }

    #[test]
    fn external_changed_should_reload_when_buffer_is_clean() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "old");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, false);
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::LoadCompleted {
                buffer_id,
                source: crate::action::FileLoadSource::External,
                result: Ok("new".to_string()),
            }),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "new");
        assert!(!buffer.dirty);
    }

    #[test]
    fn external_changed_should_not_reload_when_buffer_is_dirty() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "old");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, true);
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::File(crate::action::FileAction::LoadCompleted {
                buffer_id,
                source: crate::action::FileLoadSource::External,
                result: Ok("new".to_string()),
            }),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "old");
        assert!(buffer.dirty);
        assert!(buffer.externally_modified);
        assert_eq!(
            state.status_bar.message,
            "file changed externally; use :w! to overwrite or :e! to reload"
        );
    }

    #[test]
    fn command_w_should_be_blocked_when_file_was_changed_externally() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "old");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, true);
        state.set_buffer_externally_modified(buffer_id, true);
        state.enter_command_mode();
        state.push_command_char('w');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        assert_eq!(
            state.status_bar.message,
            "save blocked: file changed externally (use :w! to overwrite)"
        );
    }

    #[test]
    fn command_e_should_be_blocked_when_buffer_is_dirty() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "old");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, true);
        state.enter_command_mode();
        state.push_command_char('e');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        assert_eq!(
            state.status_bar.message,
            "reload blocked: buffer is dirty (use :e! to force reload)"
        );
    }

    #[test]
    fn command_e_bang_should_reload_even_when_buffer_is_dirty() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let path = PathBuf::from("a.txt");
        let buffer_id = state.create_buffer(Some(path.clone()), "old");
        state.bind_buffer_to_active_window(buffer_id);
        state.set_buffer_dirty(buffer_id, true);
        state.enter_command_mode();
        state.push_command_char('e');
        state.push_command_char('!');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        assert_eq!(
            state.status_bar.message,
            format!("loading {}", path.display())
        );
    }

    #[test]
    fn command_e_with_path_should_open_new_file_buffer() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let initial = state.create_buffer(None, "old");
        state.bind_buffer_to_active_window(initial);
        state.enter_command_mode();
        state.push_command_char('e');
        state.push_command_char(' ');
        state.push_command_char('b');
        state.push_command_char('.');
        state.push_command_char('t');
        state.push_command_char('x');
        state.push_command_char('t');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        let active_id = state.active_buffer_id().expect("active buffer exists");
        assert_ne!(active_id, initial);
        let buffer = state.buffers.get(active_id).expect("buffer exists");
        let expected = AppState::normalize_file_path(std::path::Path::new("b.txt"));
        assert_eq!(buffer.path.as_deref(), Some(expected.as_path()));
        assert_eq!(state.status_bar.message, "loading b.txt");
    }

    #[test]
    fn command_e_with_same_path_should_reuse_existing_buffer_in_same_tab() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let existing_path = AppState::normalize_file_path(std::path::Path::new("b.txt"));
        let existing = state.create_buffer(Some(existing_path), "old");
        state.bind_buffer_to_active_window(existing);
        state.create_untitled_buffer();
        state.enter_command_mode();
        state.push_command_char('e');
        state.push_command_char(' ');
        state.push_command_char('b');
        state.push_command_char('.');
        state.push_command_char('t');
        state.push_command_char('x');
        state.push_command_char('t');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        let active_id = state.active_buffer_id().expect("active buffer exists");
        assert_eq!(active_id, existing);
        assert_eq!(state.status_bar.message, "switched b.txt");
        let expected = AppState::normalize_file_path(std::path::Path::new("b.txt"));
        let count = state
            .buffers
            .iter()
            .filter(|(_, buffer)| buffer.path.as_deref() == Some(expected.as_path()))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn command_e_with_same_path_should_reuse_existing_buffer_across_tabs() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let existing_path = AppState::normalize_file_path(std::path::Path::new("b.txt"));
        let existing = state.create_buffer(Some(existing_path), "old");
        state.bind_buffer_to_active_window(existing);
        state.open_new_tab();
        state.create_untitled_buffer();
        state.enter_command_mode();
        state.push_command_char('e');
        state.push_command_char(' ');
        state.push_command_char('b');
        state.push_command_char('.');
        state.push_command_char('t');
        state.push_command_char('x');
        state.push_command_char('t');
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let flow = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
        );

        assert!(matches!(flow, ControlFlow::Continue(())));
        let active_id = state.active_buffer_id().expect("active buffer exists");
        assert_eq!(active_id, existing);
        assert_eq!(state.status_bar.message, "switched b.txt");
        let expected = AppState::normalize_file_path(std::path::Path::new("b.txt"));
        let count = state
            .buffers
            .iter()
            .filter(|(_, buffer)| buffer.path.as_deref() == Some(expected.as_path()))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn visual_mode_should_support_ctrl_scroll_keys() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "a\nb\nc\nd");
        state.bind_buffer_to_active_window(buffer_id);
        state.enter_visual_mode();
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('e'),
                KeyModifiers::CONTROL,
            ))),
        );
        assert_eq!(state.active_cursor().row, 2);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('y'),
                KeyModifiers::CONTROL,
            ))),
        );
        assert_eq!(state.active_cursor().row, 1);
    }

    #[test]
    fn visual_mode_should_support_gg_and_g() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "a\nb\nc\nd");
        state.bind_buffer_to_active_window(buffer_id);
        state.move_cursor_down();
        state.move_cursor_down();
        state.enter_visual_mode();
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('g'),
                KeyModifiers::NONE,
            ))),
        );
        assert_eq!(state.active_cursor().row, 3);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('g'),
                KeyModifiers::NONE,
            ))),
        );
        assert_eq!(state.active_cursor().row, 1);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('G'),
                KeyModifiers::SHIFT,
            ))),
        );
        assert_eq!(state.active_cursor().row, 4);
    }

    #[test]
    fn visual_delete_should_be_undoable_with_single_u() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "abcd");
        state.bind_buffer_to_active_window(buffer_id);
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        for key in [
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        ] {
            let _ = dispatch_test_action(
                &mut handler,
                &mut state,
                &file_io_service,
                &file_watcher_service,
                AppAction::Editor(EditorAction::KeyPressed(key)),
            );
        }
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "cd");

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::NONE,
            ))),
        );
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "abcd");
    }

    #[test]
    fn visual_paste_should_be_undoable_with_single_u() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "abcd");
        state.bind_buffer_to_active_window(buffer_id);
        state.line_slot = Some("XY".to_string());
        state.line_slot_line_wise = false;
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        for key in [
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
        ] {
            let _ = dispatch_test_action(
                &mut handler,
                &mut state,
                &file_io_service,
                &file_watcher_service,
                AppAction::Editor(EditorAction::KeyPressed(key)),
            );
        }
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "XYcd");

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::NONE,
            ))),
        );
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "abcd");
    }

    #[test]
    fn normal_line_wise_paste_should_be_undoable_with_single_u() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "a\nc");
        state.bind_buffer_to_active_window(buffer_id);
        state.line_slot = Some("b".to_string());
        state.line_slot_line_wise = true;
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('p'),
                KeyModifiers::NONE,
            ))),
        );
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "a\nb\nc");

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::NONE,
            ))),
        );
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "a\nc");
    }

    #[test]
    fn insert_typing_should_be_grouped_into_single_undo_step() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "");
        state.bind_buffer_to_active_window(buffer_id);
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        for key in [
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ] {
            let _ = dispatch_test_action(
                &mut handler,
                &mut state,
                &file_io_service,
                &file_watcher_service,
                AppAction::Editor(EditorAction::KeyPressed(key)),
            );
        }

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::NONE,
            ))),
        );
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "");

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('r'),
                KeyModifiers::CONTROL,
            ))),
        );
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "use");
    }

    #[test]
    fn open_line_below_insert_should_be_grouped_into_single_undo_step() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "a");
        state.bind_buffer_to_active_window(buffer_id);
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        for key in [
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ] {
            let _ = dispatch_test_action(
                &mut handler,
                &mut state,
                &file_io_service,
                &file_watcher_service,
                AppAction::Editor(EditorAction::KeyPressed(key)),
            );
        }

        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "a\nuse");

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::NONE,
            ))),
        );
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "a");
    }

    #[test]
    fn open_line_above_insert_should_be_grouped_into_single_undo_step() {
        let mut handler = ActionHandlerState::new();
        let mut state = AppState::new();
        let buffer_id = state.create_buffer(None, "a");
        state.bind_buffer_to_active_window(buffer_id);
        let (tx, _rx) = flume::bounded(8);
        let file_io_service = FileIoState::start(tx.clone());
        let file_watcher_service = FileWatcherState::start(tx);

        for key in [
            KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ] {
            let _ = dispatch_test_action(
                &mut handler,
                &mut state,
                &file_io_service,
                &file_watcher_service,
                AppAction::Editor(EditorAction::KeyPressed(key)),
            );
        }

        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "use\na");

        let _ = dispatch_test_action(
            &mut handler,
            &mut state,
            &file_io_service,
            &file_watcher_service,
            AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::NONE,
            ))),
        );
        let buffer = state.buffers.get(buffer_id).expect("buffer exists");
        assert_eq!(buffer.text, "a");
    }
}
