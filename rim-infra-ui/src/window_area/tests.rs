use std::path::PathBuf;

use ratatui::layout::Rect;
use rim_application::state::{CursorState, RimState};

use super::{DIR_DOWN, DIR_LEFT, DIR_RIGHT, DIR_UP, SelectionSegment, VisualSelectionSpec, WindowAreaWidget, collect_visual_selection_segments, dirs_from_symbol, display_width_of_char_prefix, render_line_for_display, symbol_from_dirs, visible_slice_by_display_width};

fn merged_symbol(existing: &str, add_dirs: u8) -> &'static str {
	symbol_from_dirs(dirs_from_symbol(existing) | add_dirs)
}

#[test]
fn symbol_and_dir_mapping_table() {
	let cases = [
		("│", DIR_UP | DIR_DOWN),
		("─", DIR_LEFT | DIR_RIGHT),
		("├", DIR_UP | DIR_DOWN | DIR_RIGHT),
		("┤", DIR_UP | DIR_DOWN | DIR_LEFT),
		("┬", DIR_LEFT | DIR_RIGHT | DIR_DOWN),
		("┴", DIR_LEFT | DIR_RIGHT | DIR_UP),
		("┼", DIR_UP | DIR_DOWN | DIR_LEFT | DIR_RIGHT),
	];

	for (symbol, dirs) in cases {
		assert_eq!(dirs_from_symbol(symbol), dirs);
		assert_eq!(symbol_from_dirs(dirs), symbol);
	}
}

#[test]
fn merge_table_for_common_intersections() {
	let cases = [
		("─", DIR_DOWN, "┬"),
		("─", DIR_UP, "┴"),
		("─", DIR_UP | DIR_DOWN, "┼"),
		("│", DIR_LEFT, "┤"),
		("│", DIR_RIGHT, "├"),
	];

	for (existing, add_dirs, expected) in cases {
		assert_eq!(merged_symbol(existing, add_dirs), expected);
	}
}

#[test]
fn merge_table_for_recent_regressions() {
	let cases = [
		// set_right_tee_cell: DIR_UP | DIR_RIGHT
		("─", DIR_UP | DIR_RIGHT, "┴"),
		// set_left_tee_cell: DIR_UP | DIR_LEFT
		("─", DIR_UP | DIR_LEFT, "┴"),
	];

	for (existing, add_dirs, expected) in cases {
		assert_eq!(merged_symbol(existing, add_dirs), expected);
	}
}

#[test]
fn display_width_prefix_counts_wide_chars() {
	let line = "a中b";
	assert_eq!(display_width_of_char_prefix(line, 1), 1);
	assert_eq!(display_width_of_char_prefix(line, 2), 3);
	assert_eq!(display_width_of_char_prefix(line, 3), 4);
}

#[test]
fn display_width_prefix_counts_tab_as_four_spaces() {
	let line = "a\tb";
	assert_eq!(display_width_of_char_prefix(line, 1), 1);
	assert_eq!(display_width_of_char_prefix(line, 2), 5);
	assert_eq!(display_width_of_char_prefix(line, 3), 6);
}

#[test]
fn visible_slice_uses_display_columns() {
	let line = "a中bc";
	assert_eq!(visible_slice_by_display_width(line, 0, 3), "a中");
	assert_eq!(visible_slice_by_display_width(line, 1, 2), "中");
	assert_eq!(visible_slice_by_display_width(line, 3, 2), "bc");
}

#[test]
fn render_line_for_display_should_expand_tab_to_spaces() {
	assert_eq!(render_line_for_display("\t", false), "    ");
	assert_eq!(render_line_for_display("a\tb", false), "a    b");
}

#[test]
fn visual_segments_should_cover_range_in_single_line() {
	let content = "abcdef";
	let text_rect = Rect { x: 0, y: 0, width: 10, height: 1 };
	let segments = collect_visual_selection_segments(content, VisualSelectionSpec {
		text_rect,
		scroll_x: 0,
		scroll_y: 0,
		anchor: CursorState { row: 1, col: 2 },
		cursor: CursorState { row: 1, col: 4 },
		line_wise: false,
		block_wise: false,
	});
	assert_eq!(segments.len(), 1);
	let SelectionSegment { x_start, x_end, y } = segments[0];
	assert_eq!(y, 0);
	assert_eq!(x_start, 1);
	assert_eq!(x_end, 4);
}

#[test]
fn visual_line_segments_should_cover_entire_line_width() {
	let content = "abcdef";
	let text_rect = Rect { x: 0, y: 0, width: 10, height: 1 };
	let segments = collect_visual_selection_segments(content, VisualSelectionSpec {
		text_rect,
		scroll_x: 0,
		scroll_y: 0,
		anchor: CursorState { row: 1, col: 3 },
		cursor: CursorState { row: 1, col: 3 },
		line_wise: true,
		block_wise: false,
	});
	assert_eq!(segments.len(), 1);
	let SelectionSegment { x_start, x_end, y } = segments[0];
	assert_eq!(y, 0);
	assert_eq!(x_start, 0);
	assert_eq!(x_end, 6);
}

#[test]
fn visual_segments_should_include_newline_marker_slot() {
	let content = "ab\ncd";
	let text_rect = Rect { x: 0, y: 0, width: 10, height: 2 };
	let segments = collect_visual_selection_segments(content, VisualSelectionSpec {
		text_rect,
		scroll_x: 0,
		scroll_y: 0,
		anchor: CursorState { row: 1, col: 2 },
		cursor: CursorState { row: 1, col: 3 },
		line_wise: false,
		block_wise: false,
	});
	assert_eq!(segments.len(), 1);
	let SelectionSegment { x_start, x_end, y } = segments[0];
	assert_eq!(y, 0);
	assert_eq!(x_start, 1);
	assert_eq!(x_end, 3);
}

#[test]
fn visual_block_segments_should_cover_same_columns_across_rows() {
	let content = "abcd\nefgh\nijkl";
	let text_rect = Rect { x: 0, y: 0, width: 10, height: 3 };
	let segments = collect_visual_selection_segments(content, VisualSelectionSpec {
		text_rect,
		scroll_x: 0,
		scroll_y: 0,
		anchor: CursorState { row: 1, col: 2 },
		cursor: CursorState { row: 3, col: 3 },
		line_wise: false,
		block_wise: true,
	});
	assert_eq!(segments.len(), 3);
	for (index, segment) in segments.iter().enumerate() {
		assert_eq!(segment.y, index as u16);
		assert_eq!(segment.x_start, 1);
		assert_eq!(segment.x_end, 3);
	}
}

#[test]
fn visual_block_segments_should_extend_with_virtual_cursor_column() {
	let content = "abcdef
x
zzz";
	let text_rect = Rect { x: 0, y: 0, width: 20, height: 3 };
	let segments = collect_visual_selection_segments(content, VisualSelectionSpec {
		text_rect,
		scroll_x: 0,
		scroll_y: 0,
		anchor: CursorState { row: 1, col: 5 },
		cursor: CursorState { row: 3, col: 9 },
		line_wise: false,
		block_wise: true,
	});
	assert_eq!(segments.len(), 3);
	for (index, segment) in segments.iter().enumerate() {
		assert_eq!(segment.y, index as u16);
		assert_eq!(segment.x_start, 4);
		assert_eq!(segment.x_end, 9);
	}
}

#[test]
fn visual_block_segments_should_cover_blank_rows_inside_block() {
	let content = "abcdef


xyz";
	let text_rect = Rect { x: 0, y: 0, width: 20, height: 4 };
	let segments = collect_visual_selection_segments(content, VisualSelectionSpec {
		text_rect,
		scroll_x: 0,
		scroll_y: 0,
		anchor: CursorState { row: 1, col: 5 },
		cursor: CursorState { row: 4, col: 9 },
		line_wise: false,
		block_wise: true,
	});
	assert_eq!(segments.len(), 4);
	for (index, segment) in segments.iter().enumerate() {
		assert_eq!(segment.y, index as u16);
		assert_eq!(segment.x_start, 4);
		assert_eq!(segment.x_end, 9);
	}
}

#[test]
fn visual_block_segments_should_stay_rectangular_with_tab_mixed_lines() {
	let content = "ab\tcdef\nabcdefg\nab\txyzz";
	let text_rect = Rect { x: 0, y: 0, width: 40, height: 3 };
	let segments = collect_visual_selection_segments(content, VisualSelectionSpec {
		text_rect,
		scroll_x: 0,
		scroll_y: 0,
		anchor: CursorState { row: 1, col: 5 },
		cursor: CursorState { row: 3, col: 7 },
		line_wise: false,
		block_wise: true,
	});
	assert_eq!(segments.len(), 3);
	let first = &segments[0];
	for segment in &segments[1..] {
		assert_eq!(segment.x_start, first.x_start);
		assert_eq!(segment.x_end, first.x_end);
	}
}

#[test]
fn visual_block_cursor_should_render_inside_tab_padding() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(
		Some(PathBuf::from("vb_tab_padding.rs")),
		"	foo
bar",
	);
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(20, 6);
	let content_area = Rect { x: 0, y: 0, width: 20, height: 6 };
	state.enter_visual_block_mode();
	let (_, initial_cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	state.move_cursor_right_for_visual_char();
	state.move_cursor_right_for_visual_char();

	let (_, cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	let (initial_x, initial_y) =
		initial_cursor_position.expect("initial visual block cursor should be drawable");
	let (x, y) = cursor_position.expect("visual block cursor should be drawable inside tab padding");
	assert_eq!(y, initial_y);
	assert_eq!(x, initial_x + 2);
}

#[test]
fn visual_block_cursor_should_render_at_virtual_column() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(
		Some(PathBuf::from("virtual_cursor.rs")),
		"abcdef
x
zzz",
	);
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(8, 6);
	for _ in 0..4 {
		state.move_cursor_right();
	}
	state.enter_visual_block_mode();
	state.move_cursor_down();
	state.move_cursor_down();
	for _ in 0..4 {
		state.move_cursor_right_for_visual_char();
	}

	let content_area = Rect { x: 0, y: 0, width: 8, height: 6 };
	let (_, cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	let (x, y) = cursor_position.expect("visual block cursor should be drawable");
	assert_eq!((x, y), (7, 2));
}

#[test]
fn visual_block_append_cursor_should_land_on_previous_block_right_edge() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(
		Some(PathBuf::from("block_append_cursor.rs")),
		"    enqueue_save(&self,
        StorageIoImpl::inj_ref",
	);
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(40, 6);
	for _ in 0..8 {
		state.move_cursor_right();
	}
	state.enter_visual_block_mode();
	state.move_cursor_down();

	let content_area = Rect { x: 0, y: 0, width: 40, height: 6 };
	let (widget_before, _) = WindowAreaWidget::from_state(&state, content_area);
	let expected_x = widget_before
		.selection_segments
		.iter()
		.filter(|segment| segment.y == 0)
		.map(|segment| segment.x_end)
		.max()
		.expect("top-row block segment should exist");

	state.begin_visual_block_insert(true);

	let (_, cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	let (x, y) = cursor_position.expect("block append cursor should be drawable");
	assert_eq!((x, y), (expected_x, 0));
}

#[test]
fn visual_block_insert_before_cursor_should_land_on_previous_block_left_edge() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(
		Some(PathBuf::from("block_insert_before_cursor.rs")),
		"    enqueue_save(&self,
        StorageIoImpl::inj_ref",
	);
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(40, 6);
	for _ in 0..8 {
		state.move_cursor_right();
	}
	state.enter_visual_block_mode();
	state.move_cursor_down();

	let content_area = Rect { x: 0, y: 0, width: 40, height: 6 };
	let (widget_before, _) = WindowAreaWidget::from_state(&state, content_area);
	let expected_x = widget_before
		.selection_segments
		.iter()
		.filter(|segment| segment.y == 0)
		.map(|segment| segment.x_start)
		.min()
		.expect("top-row block segment should exist");

	state.begin_visual_block_insert(false);

	let (_, cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	let (x, y) = cursor_position.expect("block insert-before cursor should be drawable");
	assert_eq!((x, y), (expected_x, 0));
}

#[test]
fn visual_block_append_cursor_should_align_with_tabbed_block_edge() {
	let line1 = "	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), \
	             StorageIoError> {";
	let line2 = "		StorageIoImpl::inj_ref(self.storage_io).enqueue_save(buffer_id, path, text)";
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(
		Some(PathBuf::from("tabbed_block_append.rs")),
		format!(
			"{line1}
{line2}"
		),
	);
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(120, 6);
	let top_col = line1.find("ue").expect("top row selection should exist") as u16 + 1;
	let bottom_col = line2.find('g').expect("bottom row selection should exist") as u16 + 1;
	let active_window = state.active_window_id();
	state.windows.get_mut(active_window).expect("window exists").cursor = CursorState { row: 1, col: top_col };
	state.enter_visual_block_mode();
	state.windows.get_mut(active_window).expect("window exists").cursor =
		CursorState { row: 2, col: bottom_col };

	let content_area = Rect { x: 0, y: 0, width: 120, height: 6 };
	let (widget_before, _) = WindowAreaWidget::from_state(&state, content_area);
	let expected_x = widget_before
		.selection_segments
		.iter()
		.filter(|segment| segment.y == 0)
		.map(|segment| segment.x_end)
		.max()
		.expect("top-row block segment should exist");

	state.begin_visual_block_insert(true);

	let (_, cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	let (x, y) = cursor_position.expect("tabbed block append cursor should be drawable");
	assert_eq!((x, y), (expected_x, 0));
}

#[test]
fn visual_block_insert_before_cursor_should_align_with_tabbed_block_edge() {
	let line1 = "	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), \
	             StorageIoError> {";
	let line2 = "		StorageIoImpl::inj_ref(self.storage_io).enqueue_save(buffer_id, path, text)";
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(
		Some(PathBuf::from("tabbed_block_insert_before.rs")),
		format!(
			"{line1}
{line2}"
		),
	);
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(120, 6);
	let top_col = line1.find("ue").expect("top row selection should exist") as u16 + 1;
	let bottom_col = line2.find('g').expect("bottom row selection should exist") as u16 + 1;
	let active_window = state.active_window_id();
	state.windows.get_mut(active_window).expect("window exists").cursor = CursorState { row: 1, col: top_col };
	state.enter_visual_block_mode();
	state.windows.get_mut(active_window).expect("window exists").cursor =
		CursorState { row: 2, col: bottom_col };

	let content_area = Rect { x: 0, y: 0, width: 120, height: 6 };
	let (widget_before, _) = WindowAreaWidget::from_state(&state, content_area);
	let expected_x = widget_before
		.selection_segments
		.iter()
		.filter(|segment| segment.y == 0)
		.map(|segment| segment.x_start)
		.min()
		.expect("top-row block segment should exist");

	state.begin_visual_block_insert(false);

	let (_, cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	let (x, y) = cursor_position.expect("tabbed block insert-before cursor should be drawable");
	assert_eq!((x, y), (expected_x, 0));
}

#[test]
fn cursor_should_not_render_when_row_is_above_visible_area() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(Some(PathBuf::from("test.rs")), "a\nb\nc");
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(40, 4);
	let active_window = state.active_window_id();
	state.windows.get_mut(active_window).expect("window exists").scroll_y = 2;

	let content_area = Rect { x: 0, y: 0, width: 40, height: 4 };
	let (_, cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	assert_eq!(cursor_position, None);
}

#[test]
fn word_wrap_should_keep_line_numbers_visible() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(Some(PathBuf::from("wrap.rs")), "abcdefghij\nxyz");
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(8, 6);
	state.toggle_word_wrap();

	let content_area = Rect { x: 0, y: 0, width: 8, height: 6 };
	let (widget, _) = WindowAreaWidget::from_state(&state, content_area);
	assert_eq!(widget.windows.len(), 1);
	let window = &widget.windows[0];
	assert!(window.number_col_width > 0);
	assert!(window.line_numbers_text.contains("1 "));
}

#[test]
fn visual_selection_should_highlight_in_word_wrap_mode() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(Some(PathBuf::from("visual_wrap.rs")), "abcdefghijkl");
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(8, 6);
	state.enter_visual_mode();
	for _ in 0..6 {
		state.move_cursor_right_for_visual_char();
	}
	state.toggle_word_wrap();

	let content_area = Rect { x: 0, y: 0, width: 8, height: 6 };
	let (widget, _) = WindowAreaWidget::from_state(&state, content_area);
	assert!(!widget.selection_segments.is_empty());
}

#[test]
fn word_wrap_cursor_should_stay_inside_window_area_after_scroll() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(Some(PathBuf::from("cursor_wrap.rs")), "abcdefghijklmnop");
	state.bind_buffer_to_active_window(buffer_id);
	state.update_active_tab_layout(8, 6);
	state.toggle_word_wrap();
	for _ in 0..8 {
		state.scroll_view_down_one_line();
	}
	let content_area = Rect { x: 0, y: 0, width: 8, height: 6 };
	let (_, cursor_position) = WindowAreaWidget::from_state(&state, content_area);
	let (x, y) = cursor_position.expect("cursor should be drawable in wrap mode");
	assert!(x >= content_area.x && x < content_area.x + content_area.width);
	assert!(y >= content_area.y && y < content_area.y + content_area.height);
}
