use std::path::PathBuf;

use super::common::{set_active_buffer_text, test_state};
use crate::state::{AppState, BufferSwitchDirection, FocusDirection, SplitAxis};

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
	assert_eq!(state.active_cursor().col, 1);

	state.focus_window(FocusDirection::Down);
	assert_eq!(state.active_cursor().row, 3);
	assert_eq!(state.active_cursor().col, 1);
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
		let window = state.windows.get_mut(active_window_id).expect("active window should exist");
		window.scroll_y = 800;
	}

	state.switch_active_window_buffer(BufferSwitchDirection::Next);
	assert_eq!(state.active_buffer_id(), Some(b2));

	let scroll_y = state.windows.get(active_window_id).expect("active window should exist").scroll_y;
	assert_eq!(scroll_y, 0);
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
fn active_buffer_save_snapshot_should_fail_without_path() {
	let mut state = test_state();
	let untitled = state.create_buffer(None, "x");
	state.bind_buffer_to_active_window(untitled);
	let err = state.active_buffer_save_snapshot(None).expect_err("snapshot should fail");
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
fn close_active_buffer_should_rebind_active_window_to_another_buffer() {
	let mut state = test_state();
	let current = state.active_buffer_id().expect("buffer id exists");
	let other = state.create_buffer(Some(PathBuf::from("other.rs")), "x");

	state.close_active_buffer();

	assert!(!state.buffers.contains_key(current));
	assert!(state.buffers.contains_key(other));
	assert_eq!(state.active_buffer_id(), Some(other));
}

#[test]
fn close_active_buffer_should_create_untitled_when_last_buffer_closed() {
	let mut state = AppState::new();
	let only = state.create_buffer(None, "hello");
	state.bind_buffer_to_active_window(only);

	state.close_active_buffer();

	assert!(!state.buffers.contains_key(only));
	let rebound = state.active_buffer_id().expect("active buffer should exist");
	let buffer = state.buffers.get(rebound).expect("buffer exists");
	assert_eq!(buffer.name, "untitled");
	assert_eq!(buffer.path, None);
	assert_eq!(buffer.text, "");
}

#[test]
fn close_active_buffer_should_fallback_to_left_buffer_when_available() {
	let mut state = AppState::new();
	let left = state.create_buffer(Some(PathBuf::from("left.rs")), "left");
	let middle = state.create_buffer(Some(PathBuf::from("middle.rs")), "middle");
	let _right = state.create_buffer(Some(PathBuf::from("right.rs")), "right");
	state.bind_buffer_to_active_window(middle);

	state.close_active_buffer();

	assert!(!state.buffers.contains_key(middle));
	assert_eq!(state.active_buffer_id(), Some(left));
}

#[test]
fn create_untitled_buffer_should_bind_new_untitled_to_active_window() {
	let mut state = test_state();
	let old = state.active_buffer_id().expect("buffer id exists");

	let new_buffer_id = state.create_untitled_buffer();

	assert_ne!(new_buffer_id, old);
	assert_eq!(state.active_buffer_id(), Some(new_buffer_id));
	let buffer = state.buffers.get(new_buffer_id).expect("buffer exists");
	assert_eq!(buffer.name, "untitled");
	assert_eq!(buffer.path, None);
	assert_eq!(buffer.text, "");
}

#[test]
fn create_untitled_buffer_should_insert_after_previous_active_in_switch_order() {
	let mut state = test_state();
	let first = state.active_buffer_id().expect("buffer id exists");
	let right = state.create_buffer(Some(PathBuf::from("right.rs")), "right");
	state.bind_buffer_to_active_window(first);

	let created = state.create_untitled_buffer();
	assert_eq!(state.active_buffer_id(), Some(created));

	state.switch_active_window_buffer(BufferSwitchDirection::Next);
	assert_eq!(state.active_buffer_id(), Some(right));
}

#[test]
fn create_buffer_should_start_clean() {
	let mut state = AppState::new();
	let buffer_id = state.create_buffer(Some(PathBuf::from("a.rs")), "hello");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert!(!buffer.dirty);
}

#[test]
fn editing_active_buffer_should_mark_dirty() {
	let mut state = test_state();
	let buffer_id = state.active_buffer_id().expect("active buffer exists");
	assert!(!state.buffers.get(buffer_id).expect("buffer exists").dirty);

	state.insert_char_at_cursor('x');

	assert!(state.buffers.get(buffer_id).expect("buffer exists").dirty);
}

#[test]
fn replace_buffer_text_preserving_cursor_should_keep_bottom_when_file_grows() {
	let mut state = test_state();
	let buffer_id = state.active_buffer_id().expect("active buffer exists");
	set_active_buffer_text(&mut state, "a\nb");
	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 2);

	state.replace_buffer_text_preserving_cursor(buffer_id, "a\nb\nc\nd".to_string());

	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.cursor.row, 4);
	assert_eq!(buffer.cursor.col, 1);
}
