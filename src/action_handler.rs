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

pub struct ActionHandler;

impl ActionHandler {
    pub fn apply(
        &self,
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
        &self,
        state: &mut AppState,
        io_gateway: &IoGateway,
        key: KeyEvent,
    ) -> ControlFlow<()> {
        if key.modifiers.contains(KeyModifiers::ALT) {
            return ControlFlow::Continue(());
        }

        if state.is_command_mode() {
            return self.handle_command_mode_key(state, io_gateway, key);
        }

        if state.is_visual_mode() {
            return self.handle_visual_mode_key(state, key);
        }

        if state.is_insert_mode() {
            return self.handle_insert_mode_key(state, key);
        }

        self.handle_normal_mode_key(state, key)
    }

    fn handle_normal_mode_key(&self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('i'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.enter_insert_mode();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('a'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.move_cursor_right();
                state.enter_insert_mode();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('o'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.open_line_below_at_cursor();
                state.enter_insert_mode();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('O'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.open_line_above_at_cursor();
                state.enter_insert_mode();
                ControlFlow::Continue(())
            }
            (KeyCode::Char(':'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.enter_command_mode();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('v'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.enter_visual_mode();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('h'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.move_cursor_left();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('0'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.move_cursor_line_start();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('$'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.move_cursor_line_end();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('j'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.move_cursor_down();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('k'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.move_cursor_up();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('l'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.move_cursor_right();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('H'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.split_active_window(SplitAxis::Horizontal);
                ControlFlow::Continue(())
            }
            (KeyCode::Char('V'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.split_active_window(SplitAxis::Vertical);
                ControlFlow::Continue(())
            }
            (KeyCode::Char('x'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.cut_current_char_to_slot();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('p'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.paste_slot_at_cursor();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('t'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.open_new_tab();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('X'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.close_current_tab();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('['), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.switch_to_prev_tab();
                ControlFlow::Continue(())
            }
            (KeyCode::Char(']'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.switch_to_next_tab();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('{'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.switch_active_window_buffer(BufferSwitchDirection::Prev);
                ControlFlow::Continue(())
            }
            (KeyCode::Char('}'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.switch_active_window_buffer(BufferSwitchDirection::Next);
                ControlFlow::Continue(())
            }
            (KeyCode::Char('h'), m) if m.contains(KeyModifiers::CONTROL) => {
                state.focus_window(FocusDirection::Left);
                ControlFlow::Continue(())
            }
            (KeyCode::Char('j'), m) if m.contains(KeyModifiers::CONTROL) => {
                state.focus_window(FocusDirection::Down);
                ControlFlow::Continue(())
            }
            (KeyCode::Char('k'), m) if m.contains(KeyModifiers::CONTROL) => {
                state.focus_window(FocusDirection::Up);
                ControlFlow::Continue(())
            }
            (KeyCode::Char('l'), m) if m.contains(KeyModifiers::CONTROL) => {
                state.focus_window(FocusDirection::Right);
                ControlFlow::Continue(())
            }
            (KeyCode::Char('W'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.close_active_window();
                ControlFlow::Continue(())
            }
            _ => ControlFlow::Continue(()),
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
                    "q" | "quit" => return ControlFlow::Break(()),
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
