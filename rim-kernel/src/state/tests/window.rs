use super::common::test_state;
use crate::state::{FocusDirection, SplitAxis};

#[test]
fn horizontal_split_should_half_width_for_two_windows() {
	let mut state = test_state();
	state.split_active_window(SplitAxis::Horizontal);
	state.update_active_tab_layout(100, 20);

	let tab = state.tabs.get(&state.active_tab).expect("active tab exists");
	assert_eq!(tab.windows.len(), 2);

	for id in &tab.windows {
		let w = state.windows.get(*id).expect("window exists");
		assert_eq!(w.width, 50);
		assert_eq!(w.height, 20);
	}
}

#[test]
fn vertical_split_should_half_height_for_two_windows() {
	let mut state = test_state();
	state.split_active_window(SplitAxis::Vertical);
	state.update_active_tab_layout(100, 20);

	let tab = state.tabs.get(&state.active_tab).expect("active tab exists");
	assert_eq!(tab.windows.len(), 2);

	for id in &tab.windows {
		let w = state.windows.get(*id).expect("window exists");
		assert_eq!(w.width, 100);
		assert_eq!(w.height, 10);
	}
}

#[test]
fn split_should_center_existing_window_when_cursor_becomes_invisible() {
	let mut state = test_state();
	let tall_text = (1..=30).map(|n| format!("line-{n}")).collect::<Vec<_>>().join("\n");
	super::common::set_active_buffer_text(&mut state, tall_text.as_str());
	state.update_active_tab_layout(100, 20);
	{
		let active_window_id = state.active_window_id();
		let window = state.windows.get_mut(active_window_id).expect("window should exist");
		window.cursor.row = 15;
		window.cursor.col = 1;
		window.scroll_y = 0;
	}

	state.split_active_window(SplitAxis::Vertical);

	let window_ids = state.active_tab_window_ids();
	let top_window = window_ids
		.iter()
		.filter_map(|id| state.windows.get(*id))
		.find(|window| window.y == 0)
		.expect("top window should exist");
	assert_eq!(top_window.cursor.row, 15);
	assert_eq!(top_window.scroll_y, 9);
}

#[test]
fn nested_split_should_only_affect_active_cell() {
	let mut state = test_state();
	state.update_active_tab_layout(100, 20);
	state.split_active_window(SplitAxis::Vertical);
	state.split_active_window(SplitAxis::Horizontal);
	state.update_active_tab_layout(100, 20);

	let tab = state.tabs.get(&state.active_tab).expect("active tab exists");
	assert_eq!(tab.windows.len(), 3);

	let mut windows = tab.windows.iter().filter_map(|id| state.windows.get(*id)).collect::<Vec<_>>();
	windows.sort_by_key(|w| (w.y, w.x));

	assert_eq!(windows[0].x, 0);
	assert_eq!(windows[0].y, 0);
	assert_eq!(windows[0].width, 100);
	assert_eq!(windows[0].height, 10);

	assert_eq!(windows[1].x, 0);
	assert_eq!(windows[1].y, 10);
	assert_eq!(windows[1].width, 50);
	assert_eq!(windows[1].height, 10);

	assert_eq!(windows[2].x, 50);
	assert_eq!(windows[2].y, 10);
	assert_eq!(windows[2].width, 50);
	assert_eq!(windows[2].height, 10);
}

#[test]
fn focus_window_direction_should_switch_to_adjacent_window() {
	let mut state = test_state();
	state.update_active_tab_layout(100, 20);
	state.split_active_window(SplitAxis::Vertical);
	state.focus_window(FocusDirection::Up);
	let active_after_up = state.active_window_id();
	let up_window = state.windows.get(active_after_up).expect("window should exist");
	assert_eq!(up_window.y, 0);

	state.focus_window(FocusDirection::Down);
	let active_after_down = state.active_window_id();
	let down_window = state.windows.get(active_after_down).expect("window should exist");
	assert_eq!(down_window.y, 10);
}

#[test]
fn close_active_window_should_remove_when_multiple_windows() {
	let mut state = test_state();
	state.update_active_tab_layout(100, 20);
	state.split_active_window(SplitAxis::Vertical);
	let before_count = state.active_tab_window_ids().len();
	state.close_active_window();
	let after_count = state.active_tab_window_ids().len();
	assert_eq!(before_count, 2);
	assert_eq!(after_count, 1);
}

#[test]
fn close_active_window_should_absorb_neighbor_after_v_then_h() {
	let mut state = test_state();
	state.update_active_tab_layout(100, 20);
	state.split_active_window(SplitAxis::Vertical);
	state.split_active_window(SplitAxis::Horizontal);

	state.close_active_window();

	let windows =
		state.active_tab_window_ids().iter().filter_map(|id| state.windows.get(*id)).collect::<Vec<_>>();
	assert_eq!(windows.len(), 2);

	let mut sorted = windows;
	sorted.sort_by_key(|w| (w.y, w.x));
	assert_eq!(sorted[0].x, 0);
	assert_eq!(sorted[0].y, 0);
	assert_eq!(sorted[0].width, 100);
	assert_eq!(sorted[0].height, 10);

	assert_eq!(sorted[1].x, 0);
	assert_eq!(sorted[1].y, 10);
	assert_eq!(sorted[1].width, 100);
	assert_eq!(sorted[1].height, 10);
}

#[test]
fn close_left_after_h_then_right_v_should_expand_right_group() {
	let mut state = test_state();
	state.update_active_tab_layout(100, 20);

	state.split_active_window(SplitAxis::Horizontal);
	state.split_active_window(SplitAxis::Vertical);

	state.focus_window(FocusDirection::Left);
	state.close_active_window();

	let windows =
		state.active_tab_window_ids().iter().filter_map(|id| state.windows.get(*id)).collect::<Vec<_>>();
	assert_eq!(windows.len(), 2);

	let mut sorted = windows;
	sorted.sort_by_key(|w| (w.y, w.x));

	assert_eq!(sorted[0].x, 0);
	assert_eq!(sorted[0].y, 0);
	assert_eq!(sorted[0].width, 100);
	assert_eq!(sorted[0].height, 10);

	assert_eq!(sorted[1].x, 0);
	assert_eq!(sorted[1].y, 10);
	assert_eq!(sorted[1].width, 100);
	assert_eq!(sorted[1].height, 10);
}

#[test]
fn close_left_after_h_v_h_v_should_not_leave_left_gap() {
	let mut state = test_state();
	state.update_active_tab_layout(100, 20);

	state.split_active_window(SplitAxis::Horizontal);
	state.split_active_window(SplitAxis::Vertical);
	state.split_active_window(SplitAxis::Horizontal);
	state.split_active_window(SplitAxis::Vertical);

	state.focus_window(FocusDirection::Left);
	state.focus_window(FocusDirection::Left);
	state.close_active_window();

	let windows =
		state.active_tab_window_ids().iter().filter_map(|id| state.windows.get(*id)).collect::<Vec<_>>();
	assert_eq!(windows.len(), 4);

	let min_x = windows.iter().map(|w| w.x).min().expect("windows exist");
	let max_right = windows.iter().map(|w| w.x.saturating_add(w.width)).max().expect("windows exist");
	assert_eq!(min_x, 0);
	assert_eq!(max_right, 100);
}

#[test]
fn resize_round_trip_should_preserve_nested_split_layout() {
	let mut state = test_state();
	state.update_active_tab_layout(120, 30);
	state.split_active_window(SplitAxis::Horizontal);
	state.split_active_window(SplitAxis::Vertical);

	let original = sorted_window_rects(&state);
	assert_eq!(original, vec![(0, 0, 60, 30), (60, 0, 60, 15), (60, 15, 60, 15)]);

	state.update_active_tab_layout(17, 7);
	state.update_active_tab_layout(120, 30);

	assert_eq!(sorted_window_rects(&state), original);
}

#[test]
fn resize_should_keep_cursor_visible_at_window_edge_instead_of_recentering() {
	let mut state = test_state();
	let tall_text = (1..=100).map(|index| format!("line-{index}")).collect::<Vec<_>>().join("\n");
	super::common::set_active_buffer_text(&mut state, tall_text.as_str());
	state.update_active_tab_layout(80, 40);

	let active_window_id = state.active_window_id();
	let window = state.windows.get_mut(active_window_id).expect("window should exist");
	window.cursor.row = 34;
	window.cursor.col = 1;
	window.scroll_y = 0;

	state.update_active_tab_layout(80, 10);

	let window = state.windows.get(active_window_id).expect("window should exist");
	assert_eq!(window.cursor.row, 34);
	assert_eq!(window.scroll_y, 24);
}

#[test]
fn resize_taller_should_keep_cursor_bottom_anchored_when_it_was_on_bottom_edge() {
	let mut state = test_state();
	let tall_text = (1..=100).map(|index| format!("line-{index}")).collect::<Vec<_>>().join("\n");
	super::common::set_active_buffer_text(&mut state, tall_text.as_str());
	state.update_active_tab_layout(80, 10);

	let active_window_id = state.active_window_id();
	let window = state.windows.get_mut(active_window_id).expect("window should exist");
	window.cursor.row = 73;
	window.cursor.col = 1;
	window.scroll_y = 63;

	state.update_active_tab_layout(80, 20);

	let window = state.windows.get(active_window_id).expect("window should exist");
	assert_eq!(window.cursor.row, 73);
	assert_eq!(window.scroll_y, 53);
}

fn sorted_window_rects(state: &crate::state::RimState) -> Vec<(u16, u16, u16, u16)> {
	let mut rects = state
		.active_tab_window_ids()
		.iter()
		.filter_map(|window_id| state.windows.get(*window_id))
		.map(|window| (window.x, window.y, window.width, window.height))
		.collect::<Vec<_>>();
	rects.sort_unstable();
	rects
}
