use crate::action::{
    AppAction, BufferAction, EditorAction, FileAction, FileLoadSource, LayoutAction, SystemAction,
    TabAction, WindowAction,
};
use crate::file_io_service::{FileIo, FileIoServiceError, FileIoState};
use crate::file_watcher_service::{FileWatcher, FileWatcherServiceError, FileWatcherState};
use crate::state::{AppState, BufferId, BufferSwitchDirection, FocusDirection, SplitAxis};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::ControlFlow;
use std::path::PathBuf;
use thiserror::Error;
use tracing::error;
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NormalKey {
    Leader,
    Tab,
    Char(char),
    Ctrl(char),
}

#[derive(Debug)]
enum SequenceMatch {
    Action(AppAction),
    Pending,
    NoMatch,
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
pub struct ActionHandlerState {
    normal_sequence: Vec<NormalKey>,
    visual_g_pending: bool,
    in_flight_internal_saves: HashSet<BufferId>,
    last_internal_save_fingerprint: HashMap<BufferId, u64>,
}

pub(crate) trait ActionHandler {
    fn apply(&mut self, action: AppAction) -> ControlFlow<()>;
}

impl ActionHandlerState {
    pub fn new() -> Self {
        Self {
            normal_sequence: Vec::new(),
            visual_g_pending: false,
            in_flight_internal_saves: HashSet::new(),
            last_internal_save_fingerprint: HashMap::new(),
        }
    }

    fn text_fingerprint(text: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    fn mark_internal_save(&mut self, buffer_id: BufferId, text: &str) {
        self.last_internal_save_fingerprint
            .insert(buffer_id, ActionHandlerState::text_fingerprint(text));
    }

    fn should_ignore_external_reload(&mut self, buffer_id: BufferId, text: &str) -> bool {
        let incoming = ActionHandlerState::text_fingerprint(text);
        let Some(expected) = self.last_internal_save_fingerprint.get(&buffer_id).copied() else {
            return false;
        };
        if expected == incoming {
            self.last_internal_save_fingerprint.remove(&buffer_id);
            return true;
        }
        self.last_internal_save_fingerprint.remove(&buffer_id);
        false
    }
}

impl<Deps> ActionHandler for ActionHandlerImpl<Deps>
where
    Deps: AsRef<ActionHandlerState>
        + AsMut<ActionHandlerState>
        + AsMut<AppState>
        + AsRef<FileIoState>
        + AsRef<FileWatcherState>
        + FileIo
        + FileWatcher,
{
    fn apply(&mut self, action: AppAction) -> ControlFlow<()> {
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
                    let Some(buffer) = state.buffers.get(buffer_id) else {
                        error!(
                            "external changed for unknown buffer: buffer_id={:?}",
                            buffer_id
                        );
                        return ControlFlow::Continue(());
                    };
                    let current_fingerprint =
                        ActionHandlerState::text_fingerprint(buffer.text.as_str());
                    let incoming_fingerprint = ActionHandlerState::text_fingerprint(&text);
                    if current_fingerprint == incoming_fingerprint {
                        state.set_buffer_externally_modified(buffer_id, false);
                        if is_active && state.status_bar.message.starts_with("reloading ") {
                            state.status_bar.message = "file saved".to_string();
                        }
                        return ControlFlow::Continue(());
                    }
                    if buffer.dirty {
                        state.set_buffer_externally_modified(buffer_id, true);
                        if is_active {
                            state.status_bar.message =
                                "file changed externally; use :w! to overwrite or :e! to reload"
                                    .to_string();
                        }
                        return ControlFlow::Continue(());
                    }
                    if self.should_ignore_external_reload(buffer_id, &text) {
                        state.set_buffer_externally_modified(buffer_id, false);
                        if is_active {
                            state.status_bar.message = "file saved".to_string();
                        }
                        return ControlFlow::Continue(());
                    }
                    let name = buffer.name.clone();
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
                let buffer_id = state.create_buffer(Some(path.clone()), String::new());
                state.bind_buffer_to_active_window(buffer_id);
                state.status_bar.message = format!("loading {}", path.display());
                if let Err(source) = self.prj_ref().enqueue_watch(buffer_id, path.clone()) {
                    let err = ActionHandlerError::OpenFileWatch { source };
                    error!(
                        "watch worker unavailable while enqueueing file watch: {}",
                        err
                    );
                }
                if let Err(source) = self.prj_ref().enqueue_load(buffer_id, path) {
                    let err = ActionHandlerError::OpenFileLoad { source };
                    error!("io worker unavailable while enqueueing file load: {}", err);
                    state.status_bar.message = "load failed: io worker unavailable".to_string();
                }
            }
            AppAction::File(FileAction::ExternalChangeDetected { buffer_id, path }) => {
                if self.in_flight_internal_saves.contains(&buffer_id) {
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
                if let Err(source) = self
                    .prj_ref()
                    .enqueue_external_load(buffer_id, path.clone())
                {
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
                    self.in_flight_internal_saves.remove(&buffer_id);
                    if !self.last_internal_save_fingerprint.contains_key(&buffer_id)
                        && let Some(text) = state
                            .buffers
                            .get(buffer_id)
                            .map(|buffer| buffer.text.as_str())
                    {
                        self.mark_internal_save(buffer_id, text);
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
                    self.in_flight_internal_saves.remove(&buffer_id);
                    self.last_internal_save_fingerprint.remove(&buffer_id);
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
            self.visual_g_pending = false;
        }

        if key.modifiers.contains(KeyModifiers::ALT) {
            self.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return ControlFlow::Continue(());
        }

        if state.is_command_mode() {
            self.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return self.handle_command_mode_key(state, key);
        }

        if state.is_visual_mode() {
            self.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return self.handle_visual_mode_key(state, key);
        }

        if state.is_insert_mode() {
            self.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return self.handle_insert_mode_key(state, key);
        }

        self.handle_normal_mode_key(state, key)
    }

    fn handle_normal_mode_key(&mut self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        let Some(normal_key) = Self::to_normal_key(state, key) else {
            self.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return ControlFlow::Continue(());
        };

        self.normal_sequence.push(normal_key);

        loop {
            match Self::resolve_normal_sequence(&self.normal_sequence) {
                SequenceMatch::Action(action) => {
                    self.normal_sequence.clear();
                    state.status_bar.key_sequence.clear();
                    return self.dispatch_internal(state, action);
                }
                SequenceMatch::Pending => {
                    state.status_bar.key_sequence =
                        Self::render_normal_sequence(&self.normal_sequence);
                    return ControlFlow::Continue(());
                }
                SequenceMatch::NoMatch => {
                    if self.normal_sequence.len() <= 1 {
                        self.normal_sequence.clear();
                        state.status_bar.key_sequence.clear();
                        return ControlFlow::Continue(());
                    }
                    let last = *self
                        .normal_sequence
                        .last()
                        .expect("normal sequence has at least one key");
                    self.normal_sequence.clear();
                    self.normal_sequence.push(last);
                    state.status_bar.key_sequence =
                        Self::render_normal_sequence(&self.normal_sequence);
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
            EditorAction::EnterInsert => state.enter_insert_mode(),
            EditorAction::AppendInsert => {
                state.move_cursor_right_for_insert();
                state.enter_insert_mode();
            }
            EditorAction::OpenLineBelowInsert => {
                state.open_line_below_at_cursor();
                state.enter_insert_mode();
            }
            EditorAction::OpenLineAboveInsert => {
                state.open_line_above_at_cursor();
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
            self.visual_g_pending = false;
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
                self.visual_g_pending = false;
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
                if self.visual_g_pending {
                    self.visual_g_pending = false;
                    state.move_cursor_file_start();
                } else {
                    self.visual_g_pending = true;
                }
                return ControlFlow::Continue(());
            }
            KeyCode::Char('G') => state.move_cursor_file_end(),
            _ => {}
        }
        self.visual_g_pending = false;
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

        self.mark_internal_save(buffer_id, &text);
        self.in_flight_internal_saves.insert(buffer_id);
        if let Err(source) = self.prj_ref().enqueue_save(buffer_id, path, text) {
            let err = ActionHandlerError::Save { source };
            error!("io worker unavailable while enqueueing file save: {}", err);
            state.status_bar.message = "save failed: io worker unavailable".to_string();
            self.in_flight_internal_saves.remove(&buffer_id);
            self.last_internal_save_fingerprint.remove(&buffer_id);
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
            self.mark_internal_save(buffer_id, &text);
            self.in_flight_internal_saves.insert(buffer_id);
            if let Err(source) = self.prj_ref().enqueue_save(buffer_id, path, text) {
                let err = ActionHandlerError::SaveAll { source };
                error!("io worker unavailable while enqueueing file save: {}", err);
                state.status_bar.message = "save failed: io worker unavailable".to_string();
                self.in_flight_internal_saves.remove(&buffer_id);
                self.last_internal_save_fingerprint.remove(&buffer_id);
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

#[cfg(test)]
mod tests {
    use super::{ActionHandler, ActionHandlerImpl, ActionHandlerState, NormalKey, SequenceMatch};
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
        handler.in_flight_internal_saves.insert(buffer_id);

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
        assert_eq!(buffer.path.as_deref(), Some(std::path::Path::new("b.txt")));
        assert_eq!(state.status_bar.message, "loading b.txt");
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
}
