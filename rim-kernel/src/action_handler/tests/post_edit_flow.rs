use ropey::Rope;

use super::support::{RecordingPorts, normalize_test_path};
use crate::{
	action::{AppAction, EditorAction, KeyCode, KeyEvent, KeyModifiers},
	ports::SwapEditOp,
	state::RimState,
};

#[test]
fn swap_ops_from_text_diff_should_split_multiline_block_insert_into_multiple_inserts() {
	let before = Rope::from_str("abc\ndef");
	let after = Rope::from_str("aXbc\ndXef");

	let ops = super::super::post_edit_flow::swap_ops_from_text_diff(&before, &after);

	assert_eq!(
		ops,
		vec![
			SwapEditOp::Insert { pos: 1, text: "X".to_string() },
			SwapEditOp::Insert { pos: 6, text: "X".to_string() },
		]
	);
}

#[test]
fn visual_block_insert_should_enqueue_multiple_swap_insert_ops() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let path = normalize_test_path("block_swap_ops.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "abc\ndef");
	state.bind_buffer_to_active_window(buffer_id);

	for key in [
		KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
		KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
		KeyEvent::new(KeyCode::Char('I'), KeyModifiers::SHIFT),
		KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
	] {
		let _ = state.apply_action(&ports, AppAction::Editor(EditorAction::KeyPressed(key)));
	}

	let ops = ports
		.swap_edits
		.borrow()
		.iter()
		.filter(|(id, ..)| *id == buffer_id)
		.map(|(_, _, op)| op.clone())
		.collect::<Vec<_>>();

	assert_eq!(
		ops,
		vec![
			SwapEditOp::Insert { pos: 1, text: "X".to_string() },
			SwapEditOp::Insert { pos: 6, text: "X".to_string() },
		]
	);
}
