use std::path::PathBuf;

use super::{super::mode_flow::SequenceMatch, support::{RecordingPorts, dispatch_test_action, map_normal_key, resolve_keys}};
use crate::{action::{AppAction, BufferAction, EditorAction, KeyCode, KeyEvent, KeyModifiers, LayoutAction, TabAction}, command::{CommandConfigFile, CommandKeymapSection, KeyBindingOn, KeymapBindingConfig}, state::{NormalSequenceKey, RimState}};

#[test]
fn to_normal_key_should_map_leader_char_to_leader_token() {
	let mut state = RimState::new();
	state.leader_key = ' ';
	let key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);

	let mapped = map_normal_key(&state, key);
	assert_eq!(mapped, Some(NormalSequenceKey::Leader));
}

#[test]
fn resolve_normal_sequence_should_keep_leader_w_pending() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Char('w')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Pending));
}

#[test]
fn resolve_normal_sequence_should_map_leader_w_v_to_split_vertical() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Char('w'), NormalSequenceKey::Char('v')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitVertical))));
}

#[test]
fn resolve_normal_sequence_should_map_leader_w_h_to_split_horizontal() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Char('w'), NormalSequenceKey::Char('h')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitHorizontal))));
}

#[test]
fn resolve_normal_sequence_should_map_leader_tab_n_to_new_tab() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Tab, NormalSequenceKey::Char('n')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Tab(TabAction::New))));
}

#[test]
fn close_active_buffer_should_not_teardown_runtime_bindings_when_other_tab_still_uses_it() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let shared_path = PathBuf::from("shared.rs");
	let shared = state.create_buffer(Some(shared_path.clone()), "shared");
	state.bind_buffer_to_active_window(shared);
	let second_tab = state.open_new_tab();
	state.bind_buffer_to_active_window(shared);

	let _ = state.apply_action(&ports, AppAction::Editor(EditorAction::CloseActiveBuffer));

	assert!(state.buffers.contains_key(shared));
	assert!(ports.unwatches.borrow().is_empty());
	assert!(ports.closes.borrow().is_empty());
	state.switch_tab(crate::state::TabId(1));
	assert_eq!(state.active_buffer_id(), Some(shared));
	assert_eq!(state.buffers.get(shared).and_then(|buffer| buffer.path.clone()), Some(shared_path));
	assert_eq!(state.active_tab, crate::state::TabId(1));
	assert_eq!(second_tab.0, 2);
}

#[test]
fn resolve_normal_sequence_should_map_leader_tab_d_to_close_tab() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Tab, NormalSequenceKey::Char('d')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Tab(TabAction::CloseCurrent))));
}

#[test]
fn resolve_normal_sequence_should_map_leader_tab_left_bracket_to_prev_tab() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Tab, NormalSequenceKey::Char('[')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Tab(TabAction::SwitchPrev))));
}

#[test]
fn resolve_normal_sequence_should_map_leader_tab_right_bracket_to_next_tab() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Tab, NormalSequenceKey::Char(']')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Tab(TabAction::SwitchNext))));
}

#[test]
fn resolve_normal_sequence_should_map_upper_h_to_prev_buffer() {
	let seq = vec![NormalSequenceKey::Char('H')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchPrev))));
}

#[test]
fn resolve_normal_sequence_should_map_upper_l_to_next_buffer() {
	let seq = vec![NormalSequenceKey::Char('L')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchNext))));
}

#[test]
fn resolve_normal_sequence_should_map_gg_to_move_file_start() {
	let seq = vec![NormalSequenceKey::Char('g'), NormalSequenceKey::Char('g')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileStart))));
}

#[test]
fn resolve_normal_sequence_should_map_upper_g_to_move_file_end() {
	let seq = vec![NormalSequenceKey::Char('G')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileEnd))));
}

#[test]
fn resolve_normal_sequence_should_map_upper_j_to_join_line_below() {
	let seq = vec![NormalSequenceKey::Char('J')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::JoinLineBelow))));
}

#[test]
fn resolve_normal_sequence_should_map_upper_v_to_enter_visual_line_mode() {
	let seq = vec![NormalSequenceKey::Char('V')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::EnterVisualLineMode))));
}

#[test]
fn resolve_normal_sequence_should_map_ctrl_v_to_enter_visual_block_mode() {
	let seq = vec![NormalSequenceKey::Ctrl('v')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::EnterVisualBlockMode))));
}

#[test]
fn resolve_normal_sequence_should_map_u_to_undo() {
	let seq = vec![NormalSequenceKey::Char('u')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::Undo))));
}

#[test]
fn resolve_normal_sequence_should_map_ctrl_e_to_scroll_view_down() {
	let seq = vec![NormalSequenceKey::Ctrl('e')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewDown))));
}

#[test]
fn resolve_normal_sequence_should_map_ctrl_y_to_scroll_view_up() {
	let seq = vec![NormalSequenceKey::Ctrl('y')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewUp))));
}

#[test]
fn resolve_normal_sequence_should_map_ctrl_d_to_scroll_view_half_page_down() {
	let seq = vec![NormalSequenceKey::Ctrl('d')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageDown))));
}

#[test]
fn resolve_normal_sequence_should_map_ctrl_u_to_scroll_view_half_page_up() {
	let seq = vec![NormalSequenceKey::Ctrl('u')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageUp))));
}

#[test]
fn resolve_normal_sequence_should_map_ctrl_r_to_redo() {
	let seq = vec![NormalSequenceKey::Ctrl('r')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::Redo))));
}

#[test]
fn to_normal_key_should_map_shift_g_to_upper_g() {
	let state = RimState::new();
	let key = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SHIFT);
	let mapped = map_normal_key(&state, key);
	assert_eq!(mapped, Some(NormalSequenceKey::Char('G')));
}

#[test]
fn resolve_normal_sequence_should_map_leader_b_d_to_close_active_buffer() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Char('b'), NormalSequenceKey::Char('d')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::CloseActiveBuffer))));
}

#[test]
fn resolve_normal_sequence_should_map_leader_b_n_to_new_empty_buffer() {
	let seq = vec![NormalSequenceKey::Leader, NormalSequenceKey::Char('b'), NormalSequenceKey::Char('n')];
	let resolved = resolve_keys(&seq);
	assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::NewEmptyBuffer))));
}

#[test]
fn configured_normal_key_binding_should_override_builtin_mapping() {
	let mut state = RimState::new();
	let first = state.create_buffer(None, "first");
	let second = state.create_buffer(None, "second");
	state.bind_buffer_to_active_window(first);
	state.bind_buffer_to_active_window(second);
	state.switch_active_window_buffer(crate::state::BufferSwitchDirection::Prev);
	let errors = state.apply_command_config(&CommandConfigFile {
		normal: CommandKeymapSection {
			keymap: vec![KeymapBindingConfig {
				on:   KeyBindingOn::Single("H".to_string()),
				run:  "core.buffer.next".to_string(),
				desc: Some("custom".to_string()),
			}],
		},
		..CommandConfigFile::default()
	});

	assert!(errors.is_empty());
	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT))),
	);

	assert_eq!(state.active_buffer_id(), Some(second));
}

#[test]
fn visual_mode_should_support_ctrl_scroll_keys() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "a\nb\nc\nd");
	state.bind_buffer_to_active_window(buffer_id);
	state.enter_visual_mode();

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL))),
	);
	assert_eq!(state.active_cursor().row, 2);

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL))),
	);
	assert_eq!(state.active_cursor().row, 1);
}

#[test]
fn visual_mode_should_support_gg_and_g() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "a\nb\nc\nd");
	state.bind_buffer_to_active_window(buffer_id);
	state.move_cursor_down();
	state.move_cursor_down();
	state.enter_visual_mode();

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))),
	);
	assert_eq!(state.active_cursor().row, 3);

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))),
	);
	assert_eq!(state.active_cursor().row, 1);

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT))),
	);
	assert_eq!(state.active_cursor().row, 4);
}

#[test]
fn visual_delete_should_be_undoable_with_single_u() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abcd");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "cd");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "abcd");
}

#[test]
fn visual_x_should_delete_selection_to_slot() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abcd");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}

	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "cd");
	assert_eq!(state.line_slot, Some("ab".to_string()));
}

#[test]
fn visual_c_should_delete_selection_and_enter_insert_mode() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abcd");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}

	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "cd");
	assert_eq!(state.line_slot, Some("ab".to_string()));
	assert_eq!(state.mode, crate::state::EditorMode::Insert);
}

#[test]
fn visual_paste_should_be_undoable_with_single_u() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abcd");
	state.bind_buffer_to_active_window(buffer_id);
	state.line_slot = Some("XY".to_string());
	state.line_slot_line_wise = false;
	state.line_slot_block_wise = false;

	for key in [
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "XYcd");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "abcd");
}

#[test]
fn visual_c_typing_should_be_undoable_with_single_u() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abcd");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "XYcd");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "abcd");
}

#[test]
fn visual_block_insert_before_should_be_undoable_with_single_u() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abc\ndef\nghi");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('I'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "aXYbc\ndXYef\ngXYhi");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "abc\ndef\nghi");
}

#[test]
fn visual_block_append_should_insert_after_block_on_each_selected_row() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abc\ndef\nghi");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}

	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "abXYc\ndeXYf\nghXYi");
}

#[test]
fn visual_block_c_should_change_block_and_be_undoable_with_single_u() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abcd\nefgh\nijkl");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}

	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "Xcd\nXgh\nXkl");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "abcd\nefgh\nijkl");
}

#[test]
fn normal_line_wise_paste_should_be_undoable_with_single_u() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "a\nc");
	state.bind_buffer_to_active_window(buffer_id);
	state.line_slot = Some("b".to_string());
	state.line_slot_line_wise = true;
	state.line_slot_block_wise = false;

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "a\nb\nc");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "a\nc");
}

#[test]
fn visual_block_delete_should_be_undoable_with_single_u() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abcd\nefgh\nijkl");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "cd\ngh\nkl");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "abcd\nefgh\nijkl");
}

#[test]
fn insert_typing_should_be_grouped_into_single_undo_step() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}

	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.undo_stack.len(), 1);
	assert_eq!(buffer.undo_stack[0].edits.len(), 1);
	assert_eq!(buffer.undo_stack[0].edits[0].inserted_text, "use");
	assert!(buffer.undo_stack[0].edits[0].deleted_text.is_empty());

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "use");
}

#[test]
fn open_line_below_insert_should_be_grouped_into_single_undo_step() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "a");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}

	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "a\nuse");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "a");
}

#[test]
fn open_line_above_insert_should_be_grouped_into_single_undo_step() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "a");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
	] {
		let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
	}

	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "use\na");

	let _ = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
	);
	let buffer = state.buffers.get(buffer_id).expect("buffer exists");
	assert_eq!(buffer.text.to_string(), "a");
}
