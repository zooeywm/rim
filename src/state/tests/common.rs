use crate::state::AppState;
use std::path::PathBuf;

pub(super) fn test_state() -> AppState {
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

pub(super) fn set_active_buffer_text(state: &mut AppState, text: &str) {
    let buffer_id = state.active_buffer_id().expect("buffer id exists");
    let buffer = state
        .buffers
        .get_mut(buffer_id)
        .expect("active buffer exists");
    buffer.text = text.to_string();
}
