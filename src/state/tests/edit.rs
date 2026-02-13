use super::common::{set_active_buffer_text, test_state};
use crate::state::{BufferEditSnapshot, BufferHistoryEntry, CursorState};

#[test]
fn cursor_move_right_should_stop_at_line_end() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abc");
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_right();
	assert_eq!(state.active_cursor().row, 1);
	assert_eq!(state.active_cursor().col, 3);
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
	assert_eq!(state.active_cursor().col, 1);
}

#[test]
fn vertical_move_should_restore_preferred_col_when_line_has_enough_width() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abcd\nx\nabcd");
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_right();
	assert_eq!(state.active_cursor().col, 4);

	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 2);
	assert_eq!(state.active_cursor().col, 1);

	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 3);
	assert_eq!(state.active_cursor().col, 4);

	state.move_cursor_up();
	assert_eq!(state.active_cursor().row, 2);
	assert_eq!(state.active_cursor().col, 1);

	state.move_cursor_up();
	assert_eq!(state.active_cursor().row, 1);
	assert_eq!(state.active_cursor().col, 4);
}

#[test]
fn horizontal_move_should_reset_preferred_col_memory() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abcd\nx\nabcd");
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_down();
	assert_eq!(state.active_cursor().col, 1);

	state.move_cursor_left();
	assert_eq!(state.active_cursor().col, 1);

	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 3);
	assert_eq!(state.active_cursor().col, 1);
}

#[test]
fn line_start_should_reset_preferred_col_memory() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abcd\nx\nabcd");
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_down();
	assert_eq!(state.active_cursor().col, 1);

	state.move_cursor_line_start();
	assert_eq!(state.active_cursor().col, 1);

	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 3);
	assert_eq!(state.active_cursor().col, 1);
}

#[test]
fn line_end_should_reset_preferred_col_memory() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abcd\nx\nabcd");
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_down();
	assert_eq!(state.active_cursor().col, 1);

	state.move_cursor_line_end();
	assert_eq!(state.active_cursor().col, 1);

	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 3);
	assert_eq!(state.active_cursor().col, 1);
}

#[test]
fn move_cursor_file_start_should_jump_to_first_line() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "a\nb\nc");
	state.move_cursor_down();
	state.move_cursor_down();
	state.move_cursor_right();
	state.move_cursor_file_start();

	assert_eq!(state.active_cursor().row, 1);
	assert_eq!(state.active_cursor().col, 1);
}

#[test]
fn move_cursor_file_end_should_jump_to_last_line() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abcd\nx");
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_file_end();

	assert_eq!(state.active_cursor().row, 2);
	assert_eq!(state.active_cursor().col, 1);
}

#[test]
fn trailing_newline_should_not_create_extra_movable_row() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abc\n");

	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 1);
	assert_eq!(state.active_cursor().col, 1);
}

#[test]
fn move_cursor_file_end_should_ignore_trailing_newline_row() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abc\n");
	state.move_cursor_file_end();

	assert_eq!(state.active_cursor().row, 1);
	assert_eq!(state.active_cursor().col, 1);
}

#[test]
fn cursor_move_down_at_bottom_should_scroll_window() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "1\n2\n3\n4\n5\n6");
	state.update_active_tab_layout(80, 3);

	state.move_cursor_down();
	state.move_cursor_down();
	let active_window_id = state.active_window_id();
	let before = state.windows.get(active_window_id).expect("window exists").scroll_y;
	assert_eq!(before, 0);

	state.move_cursor_down();
	let after = state.windows.get(active_window_id).expect("window exists").scroll_y;
	assert_eq!(state.active_cursor().row, 4);
	assert_eq!(after, 1);
}

#[test]
fn cursor_scroll_threshold_should_trigger_earlier() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "1\n2\n3\n4\n5\n6");
	state.update_active_tab_layout(80, 4);
	state.cursor_scroll_threshold = 1;

	state.move_cursor_down();
	state.move_cursor_down();
	let active_window_id = state.active_window_id();
	let before = state.windows.get(active_window_id).expect("window exists").scroll_y;
	assert_eq!(before, 0);

	state.move_cursor_down();
	let after = state.windows.get(active_window_id).expect("window exists").scroll_y;
	assert_eq!(state.active_cursor().row, 4);
	assert_eq!(after, 1);
}

#[test]
fn scroll_view_down_one_line_should_increase_scroll_and_keep_cursor_visible() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "1\n2\n3\n4\n5\n6");
	state.update_active_tab_layout(80, 3);

	state.scroll_view_down_one_line();

	let active_window_id = state.active_window_id();
	let window = state.windows.get(active_window_id).expect("window exists");
	assert_eq!(window.scroll_y, 1);
	assert_eq!(state.active_cursor().row, 2);
}

#[test]
fn scroll_view_up_one_line_should_decrease_scroll_and_keep_cursor_visible() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "1\n2\n3\n4\n5\n6");
	state.update_active_tab_layout(80, 3);
	let active_window_id = state.active_window_id();
	state.windows.get_mut(active_window_id).expect("window exists").scroll_y = 2;
	state.move_cursor_down();
	state.move_cursor_down();
	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 4);

	state.scroll_view_up_one_line();

	let window = state.windows.get(active_window_id).expect("window exists");
	assert_eq!(window.scroll_y, 1);
	assert_eq!(state.active_cursor().row, 4);
}

#[test]
fn scroll_view_should_restore_preferred_col_when_row_changes_back() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abcd\nx\nabcd");
	state.update_active_tab_layout(80, 1);
	state.move_cursor_right();
	state.move_cursor_right();
	state.move_cursor_right();
	assert_eq!(state.active_cursor().col, 4);

	state.scroll_view_down_one_line();
	assert_eq!(state.active_cursor().row, 2);
	assert_eq!(state.active_cursor().col, 1);

	state.scroll_view_down_one_line();
	assert_eq!(state.active_cursor().row, 3);
	assert_eq!(state.active_cursor().col, 4);

	state.scroll_view_up_one_line();
	assert_eq!(state.active_cursor().row, 2);
	assert_eq!(state.active_cursor().col, 1);

	state.scroll_view_up_one_line();
	assert_eq!(state.active_cursor().row, 1);
	assert_eq!(state.active_cursor().col, 4);
}

#[test]
fn scroll_view_down_half_page_should_scroll_by_half_window_height() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "1\n2\n3\n4\n5\n6\n7\n8");
	state.update_active_tab_layout(80, 4);

	state.scroll_view_down_half_page();

	let active_window_id = state.active_window_id();
	let window = state.windows.get(active_window_id).expect("window exists");
	assert_eq!(window.scroll_y, 2);
}

#[test]
fn scroll_view_up_half_page_should_restore_scroll_by_half_window_height() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "1\n2\n3\n4\n5\n6\n7\n8");
	state.update_active_tab_layout(80, 4);
	let active_window_id = state.active_window_id();
	state.windows.get_mut(active_window_id).expect("window exists").scroll_y = 4;

	state.scroll_view_up_half_page();

	let window = state.windows.get(active_window_id).expect("window exists");
	assert_eq!(window.scroll_y, 2);
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
fn join_line_below_at_cursor_should_merge_current_and_next_line() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abc\n  def\nghi");
	state.move_cursor_right();
	state.join_line_below_at_cursor();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "abc def\nghi");
	assert_eq!(buffer.cursor.row, 1);
	assert_eq!(buffer.cursor.col, 2);
}

#[test]
fn join_line_below_at_cursor_on_last_line_should_do_nothing() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abc\ndef");
	state.move_cursor_down();

	state.join_line_below_at_cursor();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "abc\ndef");
	assert_eq!(buffer.cursor.row, 2);
	assert_eq!(buffer.cursor.col, 1);
}

#[test]
fn cursor_move_right_and_left_should_adjust_horizontal_scroll() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "abcdefghijklmnopqrstuvwxyz");
	state.update_active_tab_layout(12, 8);

	for _ in 0..15 {
		state.move_cursor_right();
	}
	let window_id = state.active_window_id();
	let scrolled_right = state.windows.get(window_id).expect("window exists").scroll_x;
	assert!(scrolled_right > 0);

	for _ in 0..15 {
		state.move_cursor_left();
	}
	let scrolled_left = state.windows.get(window_id).expect("window exists").scroll_x;
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
	let scroll_x = state.windows.get(window_id).expect("window exists").scroll_x;
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
	assert_eq!(state.active_cursor().col, 4);
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
	state.line_slot_line_wise = false;
	state.move_cursor_right();
	state.paste_slot_at_cursor();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "adbc");
	assert_eq!(buffer.cursor.row, 1);
	assert_eq!(buffer.cursor.col, 4);
}

#[test]
fn paste_slot_at_cursor_should_insert_line_wise_slot_as_new_line_below() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "a\nc");
	state.line_slot = Some("b".to_string());
	state.line_slot_line_wise = true;

	state.paste_slot_at_cursor();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "a\nb\nc");
	assert_eq!(buffer.cursor.row, 2);
	assert_eq!(buffer.cursor.col, 1);
}

#[test]
fn delete_current_line_to_slot_should_remove_line_and_store_it() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "a\nb\nc");
	state.move_cursor_down();

	state.delete_current_line_to_slot();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "a\nc");
	assert_eq!(buffer.cursor.row, 2);
	assert_eq!(buffer.cursor.col, 1);
	assert_eq!(state.line_slot, Some("b".to_string()));
}

#[test]
fn delete_current_line_to_slot_should_keep_one_empty_line_when_last_line_deleted() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "only");

	state.delete_current_line_to_slot();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "");
	assert_eq!(buffer.cursor.row, 1);
	assert_eq!(buffer.cursor.col, 1);
	assert_eq!(state.line_slot, Some("only".to_string()));
}

#[test]
fn delete_current_line_to_slot_should_clamp_cursor_when_text_has_trailing_newline() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "a\nb\n");
	state.move_cursor_down();
	assert_eq!(state.active_cursor().row, 2);

	state.delete_current_line_to_slot();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "a\n");
	assert_eq!(buffer.cursor.row, 1);
	assert_eq!(buffer.cursor.col, 1);
}

#[test]
fn visual_char_move_right_should_stop_at_newline_slot_without_crossing_row() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "ab\ncd");
	state.move_cursor_right();
	state.enter_visual_mode();

	state.move_cursor_right_for_visual_char();
	state.move_cursor_right_for_visual_char();

	assert_eq!(state.active_cursor().row, 1);
	assert_eq!(state.active_cursor().col, 3);
}

#[test]
fn undo_active_buffer_edit_should_restore_previous_text_and_cursor() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "ab");
	state.move_cursor_right();
	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	state.insert_char_at_cursor('x');
	state.push_buffer_history_entry(buffer_id, BufferHistoryEntry {
		edits:         vec![BufferEditSnapshot {
			start_byte:    1,
			deleted_text:  String::new(),
			inserted_text: "x".to_string(),
		}],
		before_cursor: CursorState { row: 1, col: 2 },
		after_cursor:  CursorState { row: 1, col: 3 },
	});

	state.undo_active_buffer_edit();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "ab");
	assert_eq!(buffer.cursor, CursorState { row: 1, col: 2 });
}

#[test]
fn redo_active_buffer_edit_should_reapply_last_undone_change() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "ab");
	state.move_cursor_right();
	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	state.insert_char_at_cursor('x');
	state.push_buffer_history_entry(buffer_id, BufferHistoryEntry {
		edits:         vec![BufferEditSnapshot {
			start_byte:    1,
			deleted_text:  String::new(),
			inserted_text: "x".to_string(),
		}],
		before_cursor: CursorState { row: 1, col: 2 },
		after_cursor:  CursorState { row: 1, col: 3 },
	});
	state.undo_active_buffer_edit();

	state.redo_active_buffer_edit();

	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text, "axb");
	assert_eq!(buffer.cursor, CursorState { row: 1, col: 3 });
}
