use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ops::ControlFlow;
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
                return self.handle_key(state, key);
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
            AppAction::File(FileAction::SaveRequested { buffer_id, path }) => {
                error!(
                    "unhandled file action: SaveRequested buffer_id={:?} path={}",
                    buffer_id,
                    path.display()
                );
            }
            AppAction::File(FileAction::SaveCompleted { buffer_id, result }) => match result {
                Ok(()) => {
                    error!(
                        "unhandled file action: SaveCompleted success buffer_id={:?}",
                        buffer_id
                    );
                }
                Err(err) => {
                    error!(
                        "unhandled file action: SaveCompleted failed buffer_id={:?} error={}",
                        buffer_id, err
                    );
                }
            },
            AppAction::System(SystemAction::Quit) => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    fn handle_key(&self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        if key.modifiers.contains(KeyModifiers::ALT) {
            return ControlFlow::Continue(());
        }

        if state.is_insert_mode() {
            return self.handle_insert_mode_key(state, key);
        }

        self.handle_normal_mode_key(state, key)
    }

    fn handle_normal_mode_key(&self, state: &mut AppState, key: KeyEvent) -> ControlFlow<()> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                ControlFlow::Break(())
            }
            (KeyCode::Char('i'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.enter_insert_mode();
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
            (KeyCode::Char('t'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                state.open_new_tab();
                ControlFlow::Continue(())
            }
            (KeyCode::Char('x'), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
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
            (KeyCode::Char('w'), m) if m.contains(KeyModifiers::CONTROL) => {
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
}
