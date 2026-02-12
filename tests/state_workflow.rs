use rim::state::AppState;
use std::path::PathBuf;

#[test]
fn new_buffer_should_insert_after_active_and_close_should_fallback_left() {
    let mut state = AppState::new();

    let left = state.create_buffer(Some(PathBuf::from("left.rs")), "left");
    let middle = state.create_buffer(Some(PathBuf::from("middle.rs")), "middle");
    let _right = state.create_buffer(Some(PathBuf::from("right.rs")), "right");
    state.bind_buffer_to_active_window(middle);

    let created = state.create_untitled_buffer();
    assert_eq!(state.active_buffer_id(), Some(created));

    state.close_active_buffer();
    assert_eq!(state.active_buffer_id(), Some(middle));

    state.close_active_buffer();
    assert_eq!(state.active_buffer_id(), Some(left));
}

#[test]
fn vertical_move_should_restore_preferred_column_without_horizontal_input() {
    let mut state = AppState::new();
    let buffer_id = state.create_buffer(Some(PathBuf::from("test.rs")), "abcd\nx\nabcd");
    state.bind_buffer_to_active_window(buffer_id);

    state.move_cursor_right();
    state.move_cursor_right();
    state.move_cursor_right();
    assert_eq!(state.active_cursor().col, 4);

    state.move_cursor_down();
    assert_eq!(state.active_cursor().col, 1);

    state.move_cursor_down();
    assert_eq!(state.active_cursor().col, 4);
}
