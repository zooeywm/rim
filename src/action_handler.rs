use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ops::ControlFlow;
use std::path::PathBuf;
use tracing::error;
use tracing::info;

use crate::action::{
    AppAction, BufferAction, EditorAction, FileAction, LayoutAction, StatusAction, SystemAction,
    TabAction, WindowAction,
};
use crate::io_gateway::IoGateway;
use crate::state::{AppState, BufferSwitchDirection, FocusDirection, SplitAxis};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NormalKey {
    Leader,
    Tab,
    Char(char),
    Ctrl(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NormalCommand {
    EnterInsert,
    AppendInsert,
    OpenLineBelowInsert,
    OpenLineAboveInsert,
    EnterCommand,
    EnterVisual,
    MoveLeft,
    MoveLineStart,
    MoveLineEnd,
    MoveDown,
    MoveUp,
    MoveRight,
    SplitHorizontal,
    SplitVertical,
    CutCharToSlot,
    PasteSlotAfterCursor,
    DeleteCurrentLineToSlot,
    NewTab,
    CloseCurrentTab,
    SwitchPrevTab,
    SwitchNextTab,
    SwitchPrevBuffer,
    SwitchNextBuffer,
    CloseActiveBuffer,
    NewEmptyBuffer,
    FocusWindowLeft,
    FocusWindowDown,
    FocusWindowUp,
    FocusWindowRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SequenceMatch {
    Command(NormalCommand),
    Pending,
    NoMatch,
}

pub struct ActionHandler {
    normal_sequence: Vec<NormalKey>,
}

impl ActionHandler {
    pub fn new() -> Self {
        Self {
            normal_sequence: Vec::new(),
        }
    }
}

impl ActionHandler {
    pub fn apply(
        &mut self,
        state: &mut AppState,
        io_gateway: &IoGateway,
        action: AppAction,
    ) -> ControlFlow<()> {
        match action {
            AppAction::Editor(EditorAction::KeyPressed(key)) => {
                return self.handle_key(state, io_gateway, key);
            }
            AppAction::Layout(LayoutAction::SplitHorizontal) => {
                state.split_active_window(SplitAxis::Horizontal);
            }
            AppAction::Layout(LayoutAction::SplitVertical) => {
                state.split_active_window(SplitAxis::Vertical);
            }
            AppAction::Layout(LayoutAction::ViewportResized { .. }) => {}
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
            AppAction::Window(WindowAction::FocusLeft) => {
                state.focus_window(FocusDirection::Left);
            }
            AppAction::Window(WindowAction::FocusDown) => {
                state.focus_window(FocusDirection::Down);
            }
            AppAction::Window(WindowAction::FocusUp) => {
                state.focus_window(FocusDirection::Up);
            }
            AppAction::Window(WindowAction::FocusRight) => {
                state.focus_window(FocusDirection::Right);
            }
            AppAction::Window(WindowAction::CloseActive) => {
                state.close_active_window();
            }
            AppAction::Buffer(BufferAction::SwitchPrev) => {
                state.switch_active_window_buffer(BufferSwitchDirection::Prev);
            }
            AppAction::Buffer(BufferAction::SwitchNext) => {
                state.switch_active_window_buffer(BufferSwitchDirection::Next);
            }
            AppAction::Status(StatusAction::SetMode(mode)) => {
                state.status_bar.mode = mode;
            }
            AppAction::Status(StatusAction::SetMessage(message)) => {
                state.status_bar.message = message;
            }
            AppAction::Status(StatusAction::ClearMessage) => {
                state.status_bar.message.clear();
            }
            AppAction::File(FileAction::LoadCompleted { buffer_id, result }) => match result {
                Ok(text) => {
                    if let Some(buffer) = state.buffers.get_mut(buffer_id) {
                        buffer.text = text;
                    } else {
                        error!(
                            "load completed for unknown buffer: buffer_id={:?}",
                            buffer_id
                        );
                    }
                    state.status_bar.message = "file loaded".to_string();
                }
                Err(err) => {
                    error!("file load failed: buffer_id={:?}, error={}", buffer_id, err);
                    state.status_bar.message = format!("load failed: {}", err);
                }
            },
            AppAction::File(FileAction::OpenRequested { path }) => {
                info!("open_file: {}", path.display());
                let buffer_id = state.create_buffer(Some(path.clone()), String::new());
                state.bind_buffer_to_active_window(buffer_id);
                state.status_bar.message = format!("loading {}", path.display());
                if let Err(err) = io_gateway.enqueue_load(buffer_id, path) {
                    error!("io worker unavailable while enqueueing file load: {}", err);
                    state.status_bar.message = "load failed: io worker unavailable".to_string();
                }
            }
            AppAction::File(FileAction::SaveCompleted { buffer_id, result }) => match result {
                Ok(()) => {
                    state.apply_pending_save_path_if_matches(buffer_id);
                    state.status_bar.message = "file saved".to_string();
                    if state.quit_after_save {
                        state.quit_after_save = false;
                        return ControlFlow::Break(());
                    }
                }
                Err(err) => {
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

    fn handle_key(
        &mut self,
        state: &mut AppState,
        io_gateway: &IoGateway,
        key: KeyEvent,
    ) -> ControlFlow<()> {
        if key.modifiers.contains(KeyModifiers::ALT) {
            self.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return ControlFlow::Continue(());
        }

        if state.is_command_mode() {
            self.normal_sequence.clear();
            state.status_bar.key_sequence.clear();
            return self.handle_command_mode_key(state, io_gateway, key);
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
                SequenceMatch::Command(command) => {
                    self.normal_sequence.clear();
                    state.status_bar.key_sequence.clear();
                    self.apply_normal_command(state, command);
                    return ControlFlow::Continue(());
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
            return Some(NormalKey::Char(ch));
        }

        if key.code == KeyCode::Tab {
            return Some(NormalKey::Tab);
        }

        None
    }

    fn resolve_normal_sequence(keys: &[NormalKey]) -> SequenceMatch {
        use NormalCommand as C;
        use NormalKey as K;

        match keys {
            [K::Leader] => SequenceMatch::Pending,
            [K::Leader, K::Char('w')] => SequenceMatch::Pending,
            [K::Leader, K::Char('w'), K::Char('v')] => SequenceMatch::Command(C::SplitVertical),
            [K::Leader, K::Char('w'), K::Char('h')] => SequenceMatch::Command(C::SplitHorizontal),
            [K::Leader, K::Tab] => SequenceMatch::Pending,
            [K::Leader, K::Tab, K::Char('n')] => SequenceMatch::Command(C::NewTab),
            [K::Leader, K::Tab, K::Char('d')] => SequenceMatch::Command(C::CloseCurrentTab),
            [K::Leader, K::Tab, K::Char('[')] => SequenceMatch::Command(C::SwitchPrevTab),
            [K::Leader, K::Tab, K::Char(']')] => SequenceMatch::Command(C::SwitchNextTab),
            [K::Leader, K::Char('b')] => SequenceMatch::Pending,
            [K::Leader, K::Char('b'), K::Char('d')] => SequenceMatch::Command(C::CloseActiveBuffer),
            [K::Leader, K::Char('b'), K::Char('n')] => SequenceMatch::Command(C::NewEmptyBuffer),
            [K::Char('d')] => SequenceMatch::Pending,
            [K::Char('d'), K::Char('d')] => SequenceMatch::Command(C::DeleteCurrentLineToSlot),
            [K::Char('i')] => SequenceMatch::Command(C::EnterInsert),
            [K::Char('a')] => SequenceMatch::Command(C::AppendInsert),
            [K::Char('o')] => SequenceMatch::Command(C::OpenLineBelowInsert),
            [K::Char('O')] => SequenceMatch::Command(C::OpenLineAboveInsert),
            [K::Char(':')] => SequenceMatch::Command(C::EnterCommand),
            [K::Char('v')] => SequenceMatch::Command(C::EnterVisual),
            [K::Char('h')] => SequenceMatch::Command(C::MoveLeft),
            [K::Char('0')] => SequenceMatch::Command(C::MoveLineStart),
            [K::Char('$')] => SequenceMatch::Command(C::MoveLineEnd),
            [K::Char('j')] => SequenceMatch::Command(C::MoveDown),
            [K::Char('k')] => SequenceMatch::Command(C::MoveUp),
            [K::Char('l')] => SequenceMatch::Command(C::MoveRight),
            [K::Char('x')] => SequenceMatch::Command(C::CutCharToSlot),
            [K::Char('p')] => SequenceMatch::Command(C::PasteSlotAfterCursor),
            [K::Char('H')] => SequenceMatch::Command(C::SwitchPrevBuffer),
            [K::Char('L')] => SequenceMatch::Command(C::SwitchNextBuffer),
            [K::Char('{')] => SequenceMatch::Command(C::SwitchPrevBuffer),
            [K::Char('}')] => SequenceMatch::Command(C::SwitchNextBuffer),
            [K::Ctrl('h')] => SequenceMatch::Command(C::FocusWindowLeft),
            [K::Ctrl('j')] => SequenceMatch::Command(C::FocusWindowDown),
            [K::Ctrl('k')] => SequenceMatch::Command(C::FocusWindowUp),
            [K::Ctrl('l')] => SequenceMatch::Command(C::FocusWindowRight),
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

    fn apply_normal_command(&self, state: &mut AppState, command: NormalCommand) {
        match command {
            NormalCommand::EnterInsert => state.enter_insert_mode(),
            NormalCommand::AppendInsert => {
                state.move_cursor_right();
                state.enter_insert_mode();
            }
            NormalCommand::OpenLineBelowInsert => {
                state.open_line_below_at_cursor();
                state.enter_insert_mode();
            }
            NormalCommand::OpenLineAboveInsert => {
                state.open_line_above_at_cursor();
                state.enter_insert_mode();
            }
            NormalCommand::EnterCommand => state.enter_command_mode(),
            NormalCommand::EnterVisual => state.enter_visual_mode(),
            NormalCommand::MoveLeft => state.move_cursor_left(),
            NormalCommand::MoveLineStart => state.move_cursor_line_start(),
            NormalCommand::MoveLineEnd => state.move_cursor_line_end(),
            NormalCommand::MoveDown => state.move_cursor_down(),
            NormalCommand::MoveUp => state.move_cursor_up(),
            NormalCommand::MoveRight => state.move_cursor_right(),
            NormalCommand::SplitHorizontal => state.split_active_window(SplitAxis::Horizontal),
            NormalCommand::SplitVertical => state.split_active_window(SplitAxis::Vertical),
            NormalCommand::CutCharToSlot => state.cut_current_char_to_slot(),
            NormalCommand::PasteSlotAfterCursor => state.paste_slot_at_cursor(),
            NormalCommand::DeleteCurrentLineToSlot => state.delete_current_line_to_slot(),
            NormalCommand::NewTab => {
                state.open_new_tab();
            }
            NormalCommand::CloseCurrentTab => {
                state.close_current_tab();
            }
            NormalCommand::SwitchPrevTab => {
                state.switch_to_prev_tab();
            }
            NormalCommand::SwitchNextTab => {
                state.switch_to_next_tab();
            }
            NormalCommand::SwitchPrevBuffer => {
                state.switch_active_window_buffer(BufferSwitchDirection::Prev);
            }
            NormalCommand::SwitchNextBuffer => {
                state.switch_active_window_buffer(BufferSwitchDirection::Next);
            }
            NormalCommand::CloseActiveBuffer => {
                state.close_active_buffer();
            }
            NormalCommand::NewEmptyBuffer => {
                state.create_and_bind_empty_buffer();
            }
            NormalCommand::FocusWindowLeft => state.focus_window(FocusDirection::Left),
            NormalCommand::FocusWindowDown => state.focus_window(FocusDirection::Down),
            NormalCommand::FocusWindowUp => state.focus_window(FocusDirection::Up),
            NormalCommand::FocusWindowRight => state.focus_window(FocusDirection::Right),
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
            KeyCode::Right => state.move_cursor_right(),
            KeyCode::Tab => state.insert_char_at_cursor('\t'),
            KeyCode::Char(ch) => {
                state.insert_char_at_cursor(ch);
            }
            _ => {}
        }

        ControlFlow::Continue(())
    }

    fn handle_visual_mode_key(&self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return ControlFlow::Continue(());
        }

        match key.code {
            KeyCode::Esc => state.exit_visual_mode(),
            KeyCode::Char('v') => state.enter_visual_line_mode(),
            KeyCode::Char('d') => state.delete_visual_selection_to_slot(),
            KeyCode::Char('y') => state.yank_visual_selection_to_slot(),
            KeyCode::Char('p') => state.replace_visual_selection_with_slot(),
            KeyCode::Char('h') => state.move_cursor_left(),
            KeyCode::Char('j') => state.move_cursor_down(),
            KeyCode::Char('k') => state.move_cursor_up(),
            KeyCode::Char('l') => state.move_cursor_right(),
            KeyCode::Char('0') => state.move_cursor_line_start(),
            KeyCode::Char('$') => state.move_cursor_line_end(),
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn handle_command_mode_key(
        &self,
        state: &mut AppState,
        io_gateway: &IoGateway,
        key: KeyEvent,
    ) -> ControlFlow<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return ControlFlow::Continue(());
        }

        match key.code {
            KeyCode::Esc => state.exit_command_mode(),
            KeyCode::Enter => {
                let command = state.take_command_line();
                match command.as_str() {
                    "" => {}
                    "qa" => return ControlFlow::Break(()),
                    "q" | "quit" => {
                        if state.active_tab_window_ids().len() > 1 {
                            state.close_active_window();
                        } else if state.tabs.len() > 1 {
                            state.close_current_tab();
                        } else {
                            return ControlFlow::Break(());
                        }
                    }
                    "w" => {
                        self.enqueue_save_active_buffer(state, io_gateway, false, None);
                    }
                    "wa" => {
                        self.enqueue_save_all_buffers(state, io_gateway);
                    }
                    "wq" => {
                        self.enqueue_save_active_buffer(state, io_gateway, true, None);
                    }
                    _ if command.starts_with("w ") => {
                        let path = command[2..].trim();
                        if path.is_empty() {
                            state.status_bar.message = "save failed: empty path".to_string();
                        } else {
                            self.enqueue_save_active_buffer(
                                state,
                                io_gateway,
                                false,
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
                                io_gateway,
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
        &self,
        state: &mut AppState,
        io_gateway: &IoGateway,
        quit_after_save: bool,
        path_override: Option<PathBuf>,
    ) {
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

        if let Err(err) = io_gateway.enqueue_save(buffer_id, path, text) {
            error!("io worker unavailable while enqueueing file save: {}", err);
            state.status_bar.message = "save failed: io worker unavailable".to_string();
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

    fn enqueue_save_all_buffers(&self, state: &mut AppState, io_gateway: &IoGateway) {
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
            if let Err(err) = io_gateway.enqueue_save(buffer_id, path, text) {
                error!("io worker unavailable while enqueueing file save: {}", err);
                state.status_bar.message = "save failed: io worker unavailable".to_string();
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
    use super::{ActionHandler, NormalCommand, NormalKey, SequenceMatch};
    use crate::state::AppState;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn to_normal_key_should_map_leader_char_to_leader_token() {
        let mut state = AppState::new();
        state.set_leader_key(' ');
        let key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);

        let mapped = ActionHandler::to_normal_key(&state, key);
        assert_eq!(mapped, Some(NormalKey::Leader));
    }

    #[test]
    fn resolve_normal_sequence_should_keep_leader_w_pending() {
        let seq = vec![NormalKey::Leader, NormalKey::Char('w')];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(resolved, SequenceMatch::Pending);
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_w_v_to_split_vertical() {
        let seq = vec![
            NormalKey::Leader,
            NormalKey::Char('w'),
            NormalKey::Char('v'),
        ];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::SplitVertical)
        );
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_w_h_to_split_horizontal() {
        let seq = vec![
            NormalKey::Leader,
            NormalKey::Char('w'),
            NormalKey::Char('h'),
        ];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::SplitHorizontal)
        );
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_tab_n_to_new_tab() {
        let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('n')];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(resolved, SequenceMatch::Command(NormalCommand::NewTab));
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_tab_d_to_close_tab() {
        let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('d')];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::CloseCurrentTab)
        );
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_tab_left_bracket_to_prev_tab() {
        let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('[')];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::SwitchPrevTab)
        );
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_tab_right_bracket_to_next_tab() {
        let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char(']')];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::SwitchNextTab)
        );
    }

    #[test]
    fn resolve_normal_sequence_should_map_upper_h_to_prev_buffer() {
        let seq = vec![NormalKey::Char('H')];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::SwitchPrevBuffer)
        );
    }

    #[test]
    fn resolve_normal_sequence_should_map_upper_l_to_next_buffer() {
        let seq = vec![NormalKey::Char('L')];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::SwitchNextBuffer)
        );
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_b_d_to_close_active_buffer() {
        let seq = vec![
            NormalKey::Leader,
            NormalKey::Char('b'),
            NormalKey::Char('d'),
        ];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::CloseActiveBuffer)
        );
    }

    #[test]
    fn resolve_normal_sequence_should_map_leader_b_n_to_new_empty_buffer() {
        let seq = vec![
            NormalKey::Leader,
            NormalKey::Char('b'),
            NormalKey::Char('n'),
        ];
        let resolved = ActionHandler::resolve_normal_sequence(&seq);
        assert_eq!(
            resolved,
            SequenceMatch::Command(NormalCommand::NewEmptyBuffer)
        );
    }
}
