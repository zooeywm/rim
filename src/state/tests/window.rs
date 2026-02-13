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
