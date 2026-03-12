use std::path::PathBuf;

use super::common::set_active_buffer_text;
use crate::state::{
	BufferEditSnapshot, BufferHistoryEntry, BufferSwitchDirection, CursorState, RimState, SplitAxis,
};

#[test]
fn workspace_session_snapshot_should_roundtrip_tabs_windows_and_views() {
	let mut state = RimState::new();
	let first = state.create_buffer(Some(PathBuf::from("first.rs")), "first\nline\ntext");
	let _second = state.create_buffer(Some(PathBuf::from("second.rs")), "second\nline\ntext");
	let scratch = state.create_buffer(None, "scratch");
	state.bind_buffer_to_active_window(first);
	state.update_active_tab_layout(100, 20);
	{
		let active_window_id = state.active_window_id();
		let window = state.windows.get_mut(active_window_id).expect("active window should exist");
		window.cursor = CursorState { row: 2, col: 3 };
		window.scroll_y = 1;
		window.scroll_x = 2;
	}
	state.switch_active_window_buffer(BufferSwitchDirection::Next);
	{
		let active_window_id = state.active_window_id();
		let window = state.windows.get_mut(active_window_id).expect("active window should exist");
		window.cursor = CursorState { row: 3, col: 2 };
		window.scroll_y = 2;
	}
	state.switch_active_window_buffer(BufferSwitchDirection::Prev);
	state.split_active_window(SplitAxis::Vertical);
	let right_window_id = state.active_window_id();
	{
		let window = state.windows.get_mut(right_window_id).expect("split window should exist");
		window.cursor = CursorState { row: 2, col: 4 };
		window.scroll_y = 1;
	}
	state.open_new_tab();
	let third = state.create_buffer(Some(PathBuf::from("third.rs")), "third");
	state.bind_buffer_to_active_window(third);
	set_active_buffer_text(&mut state, "third\nbuffer");
	{
		let active_window_id = state.active_window_id();
		let window = state.windows.get_mut(active_window_id).expect("active window should exist");
		window.cursor = CursorState { row: 2, col: 2 };
	}
	state.push_buffer_history_entry(
		scratch,
		BufferHistoryEntry {
			edits: vec![BufferEditSnapshot {
				start_byte: 0,
				deleted_text: String::new(),
				inserted_text: "scratch".to_string(),
			}],
			before_cursor: CursorState { row: 1, col: 1 },
			after_cursor: CursorState { row: 1, col: 8 },
		},
	);

	let snapshot = state.workspace_session_snapshot();
	assert!(snapshot.buffers.iter().any(|buffer| buffer.path.is_none() && buffer.history.is_some()));
	assert!(snapshot.buffers.iter().any(|buffer| buffer.path.is_some() && buffer.history.is_none()));
	let mut restored = RimState::new();

	assert!(restored.restore_workspace_session(snapshot));
	assert_eq!(restored.tabs.len(), 2);
	assert_eq!(restored.buffer_order.len(), 5);
	assert_eq!(restored.active_tab.0, 2);
	let active_buffer = restored.active_buffer_id().expect("active buffer should exist");
	assert_eq!(
		restored.buffers.get(active_buffer).and_then(|buffer| buffer.path.clone()),
		Some(PathBuf::from("third.rs"))
	);
	assert_eq!(restored.active_cursor(), CursorState { row: 2, col: 2 });

	restored.switch_to_prev_tab();
	let left_window_id = restored.active_tab_window_ids()[0];
	let second_restored =
		restored.find_buffer_by_path(PathBuf::from("second.rs").as_path()).expect("buffer should exist");
	let restored_view = restored
		.window_buffer_views
		.get(&(left_window_id, second_restored))
		.copied()
		.expect("window+buffer view should exist");
	assert_eq!(restored_view.cursor, CursorState { row: 3, col: 2 });
	assert_eq!(restored_view.scroll_y, 2);

	restored.tabs.get_mut(&restored.active_tab).expect("tab should exist").active_window = left_window_id;
	restored.switch_active_window_buffer(BufferSwitchDirection::Next);
	assert_eq!(restored.active_cursor(), CursorState { row: 3, col: 2 });

	let right_window_id = restored.active_tab_window_ids()[1];
	let right_window = restored.windows.get(right_window_id).expect("right window should exist");
	assert_eq!(right_window.cursor, CursorState { row: 2, col: 4 });
	assert_eq!(right_window.scroll_y, 1);
	let scratch_restored = restored
		.buffers
		.iter()
		.find_map(|(buffer_id, buffer)| buffer.path.is_none().then_some(buffer_id))
		.expect("scratch buffer should exist");
	assert_eq!(
		restored.buffers.get(scratch_restored).expect("scratch buffer should exist").undo_stack.len(),
		1
	);
}
