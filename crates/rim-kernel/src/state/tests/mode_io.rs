use super::common::{set_active_buffer_text, test_state};

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
fn exit_insert_mode_should_clamp_cursor_from_trailing_newline_slot() {
	let mut state = test_state();
	super::common::set_active_buffer_text(&mut state, "abc");
	state.move_cursor_line_end();
	assert_eq!(state.active_cursor().col, 3);

	state.move_cursor_right_for_insert();
	assert_eq!(state.active_cursor().col, 4);

	state.enter_insert_mode();
	state.exit_insert_mode();
	assert_eq!(state.active_cursor().col, 3);
}

#[test]
fn command_mode_should_toggle_and_show_prompt_in_status_line() {
	let mut state = test_state();
	state.enter_command_mode();
	state.push_command_char('q');
	assert!(state.is_command_mode());
	assert_eq!(state.status_bar.mode, "COMMAND");
	assert!(state.status_line().contains(":q"));
	assert!(state.status_line().contains("1:1"));

	state.exit_command_mode();
	assert!(!state.is_command_mode());
	assert_eq!(state.status_bar.mode, "NORMAL");
}

#[test]
fn normal_mode_status_line_should_include_cursor_position() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "ab\ncd");
	assert!(state.status_line().contains("1:1 Top"));

	state.move_cursor_right();
	assert!(state.status_line().contains("1:2 Top"));

	state.move_cursor_down();
	assert!(state.status_line().contains("2:2 Bot"));
}

#[test]
fn normal_mode_status_line_should_show_percentage_between_top_and_bot() {
	let mut state = test_state();
	set_active_buffer_text(&mut state, "a\nb\nc\nd");

	state.move_cursor_down();
	assert!(state.status_line().contains("2:1 50%"));

	state.move_cursor_down();
	assert!(state.status_line().contains("3:1 75%"));
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
