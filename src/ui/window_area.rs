use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthChar;

use crate::state::AppState;

pub(super) struct WindowAreaWidget {
    windows: Vec<WindowView>,
    vertical_lines: Vec<VerticalLine>,
    horizontal_lines: Vec<HorizontalLine>,
}

#[derive(Debug)]
struct WindowView {
    local_rect: Rect,
    number_col_width: u16,
    line_numbers_text: String,
    text_text: String,
}

#[derive(Debug)]
struct VerticalLine {
    x: u16,
    y_start: u16,
    y_end: u16,
}

#[derive(Debug)]
struct HorizontalLine {
    x_start: u16,
    x_end: u16,
    y: u16,
    left_join_x: Option<u16>,
    right_join_x: Option<u16>,
}

impl WindowAreaWidget {
    pub(super) fn from_state(state: &AppState, content_area: Rect) -> (Self, Option<(u16, u16)>) {
        let mut windows = Vec::new();
        let mut cursor_position = None;

        for window_id in state.active_tab_window_ids() {
            let Some(window) = state.windows.get(window_id) else {
                continue;
            };

            let mut local_rect = Rect {
                x: window.x,
                y: window.y,
                width: window.width.max(1),
                height: window.height.max(1),
            };
            if window.x > 0 {
                local_rect.x = local_rect.x.saturating_add(1);
                local_rect.width = local_rect.width.saturating_sub(1);
            }
            if window.y > 0 {
                local_rect.y = local_rect.y.saturating_add(1);
                local_rect.height = local_rect.height.saturating_sub(1);
            }
            if local_rect.width == 0 || local_rect.height == 0 {
                continue;
            }

            let number_col_width = if local_rect.width <= 5 {
                0
            } else {
                local_rect.width.min(5)
            };
            let text_width = local_rect.width.saturating_sub(number_col_width);
            if text_width == 0 {
                continue;
            }

            let text_rect = Rect {
                x: local_rect.x.saturating_add(number_col_width),
                y: local_rect.y,
                width: text_width,
                height: local_rect.height,
            };

            let content = window
                .buffer_id
                .and_then(|buffer_id| state.buffers.get(buffer_id))
                .map(|buf| buf.text.as_str())
                .unwrap_or("");
            let scroll_y = window.scroll_y as usize;
            let scroll_x = window.scroll_x as usize;
            let visible_rows = local_rect.height as usize;
            let line_numbers_text = if number_col_width == 0 {
                String::new()
            } else {
                content
                    .lines()
                    .enumerate()
                    .skip(scroll_y)
                    .take(visible_rows)
                    .map(|(i, _)| format!("{:>4} ", i + 1))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let text_text = content
                .lines()
                .skip(scroll_y)
                .take(visible_rows)
                .map(|line| visible_slice_by_display_width(line, scroll_x, text_width as usize))
                .collect::<Vec<_>>()
                .join("\n");

            if state.active_window_id() == window_id {
                let cursor = state.active_cursor();
                let line_idx = cursor.row.saturating_sub(1) as usize;
                let active_line = content.lines().nth(line_idx).unwrap_or("");
                let cursor_col_chars = cursor.col.saturating_sub(1) as usize;
                let cursor_display_col =
                    display_width_of_char_prefix(active_line, cursor_col_chars);
                let cursor_x_offset = cursor_display_col
                    .saturating_sub(scroll_x)
                    .min(text_rect.width.saturating_sub(1) as usize)
                    as u16;
                let cursor_x_local = text_rect.x.saturating_add(cursor_x_offset).min(
                    text_rect
                        .x
                        .saturating_add(text_rect.width.saturating_sub(1)),
                );
                let cursor_y_local = text_rect
                    .y
                    .saturating_add(cursor.row.saturating_sub(1).saturating_sub(window.scroll_y))
                    .min(
                        text_rect
                            .y
                            .saturating_add(text_rect.height.saturating_sub(1)),
                    );
                cursor_position = Some((
                    content_area.x.saturating_add(cursor_x_local),
                    content_area.y.saturating_add(cursor_y_local),
                ));
            }

            windows.push(WindowView {
                local_rect,
                number_col_width,
                line_numbers_text,
                text_text,
            });
        }

        let (vertical_lines, horizontal_lines) = collect_split_lines(state, content_area);
        (
            Self {
                windows,
                vertical_lines,
                horizontal_lines,
            },
            cursor_position,
        )
    }
}

impl Widget for WindowAreaWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.windows.is_empty() {
            Paragraph::new("").render(area, buf);
            return;
        }

        for window in self.windows {
            let abs_rect = Rect {
                x: area.x.saturating_add(window.local_rect.x),
                y: area.y.saturating_add(window.local_rect.y),
                width: window.local_rect.width,
                height: window.local_rect.height,
            };
            let number_rect = Rect {
                x: abs_rect.x,
                y: abs_rect.y,
                width: window.number_col_width,
                height: abs_rect.height,
            };
            let text_rect = Rect {
                x: abs_rect.x.saturating_add(window.number_col_width),
                y: abs_rect.y,
                width: abs_rect.width.saturating_sub(window.number_col_width),
                height: abs_rect.height,
            };

            Paragraph::new(window.line_numbers_text.as_str())
                .style(Style::default().fg(Color::DarkGray))
                .render(number_rect, buf);
            Paragraph::new(window.text_text.as_str()).render(text_rect, buf);
        }

        for line in self.horizontal_lines {
            let abs_y = area.y.saturating_add(line.y);
            if let Some(x) = line.left_join_x {
                let abs_x = area.x.saturating_add(x);
                if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
                    set_right_tee_cell(cell);
                }
            }

            for x in line.x_start..line.x_end {
                let abs_x = area.x.saturating_add(x);
                if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
                    set_separator_cell(cell);
                }
            }

            if let Some(x) = line.right_join_x {
                let abs_x = area.x.saturating_add(x);
                if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
                    set_left_tee_cell(cell);
                }
            }
        }

        for line in self.vertical_lines {
            let abs_x = area.x.saturating_add(line.x);
            for y in line.y_start..line.y_end {
                let abs_y = area.y.saturating_add(y);
                if let Some(cell) = buf.cell_mut((abs_x, abs_y)) {
                    let is_start = y == line.y_start;
                    let is_end = y.saturating_add(1) == line.y_end;
                    let dirs = if is_start && is_end {
                        DIR_UP | DIR_DOWN
                    } else if is_start {
                        DIR_DOWN
                    } else if is_end {
                        DIR_UP
                    } else {
                        DIR_UP | DIR_DOWN
                    };
                    merge_cell(cell, dirs);
                }
            }
        }
    }
}

fn collect_split_lines(
    state: &AppState,
    content_area: Rect,
) -> (Vec<VerticalLine>, Vec<HorizontalLine>) {
    let mut vertical_lines = Vec::new();
    let mut horizontal_lines = Vec::new();

    for window_id in state.active_tab_window_ids() {
        let Some(window) = state.windows.get(window_id) else {
            continue;
        };

        let right = window.x.saturating_add(window.width);
        let bottom = window.y.saturating_add(window.height);

        if right < content_area.width {
            let y_start = window.y;
            if y_start < bottom {
                vertical_lines.push(VerticalLine {
                    x: right,
                    y_start,
                    y_end: bottom,
                });
            }
        }
        if bottom < content_area.height {
            let origin_x = window.x;
            let mut start_x = origin_x;
            let mut left_join_x = None;
            if window.x > 0 {
                left_join_x = Some(origin_x);
                start_x = start_x.saturating_add(1);
            }
            let right_join_x = if right < content_area.width {
                Some(right)
            } else {
                None
            };
            horizontal_lines.push(HorizontalLine {
                x_start: start_x,
                x_end: right,
                y: bottom,
                left_join_x,
                right_join_x,
            });
        }
    }

    (vertical_lines, horizontal_lines)
}

fn display_width_of_char_prefix(line: &str, char_count: usize) -> usize {
    line.chars()
        .take(char_count)
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

fn visible_slice_by_display_width(line: &str, skip_cols: usize, max_cols: usize) -> String {
    if max_cols == 0 || line.is_empty() {
        return String::new();
    }

    let mut consumed = 0usize;
    let mut start = line.len();
    for (idx, ch) in line.char_indices() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if consumed + width <= skip_cols {
            consumed += width;
            continue;
        }
        start = if consumed < skip_cols {
            idx + ch.len_utf8()
        } else {
            idx
        };
        break;
    }

    let mut out = String::new();
    let mut used = 0usize;
    for ch in line[start..].chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width == 0 {
            if !out.is_empty() {
                out.push(ch);
            }
            continue;
        }
        if used + width > max_cols {
            break;
        }
        out.push(ch);
        used += width;
    }
    out
}

fn set_separator_cell(cell: &mut Cell) {
    merge_cell(cell, DIR_LEFT | DIR_RIGHT);
}

fn set_right_tee_cell(cell: &mut Cell) {
    merge_cell(cell, DIR_UP | DIR_RIGHT);
}

fn set_left_tee_cell(cell: &mut Cell) {
    merge_cell(cell, DIR_UP | DIR_LEFT);
}

const DIR_UP: u8 = 0b0001;
const DIR_DOWN: u8 = 0b0010;
const DIR_LEFT: u8 = 0b0100;
const DIR_RIGHT: u8 = 0b1000;

fn merge_cell(cell: &mut Cell, add_dirs: u8) {
    let merged = symbol_from_dirs(dirs_from_symbol(cell.symbol()) | add_dirs);
    cell.set_symbol(merged);
    cell.set_fg(Color::DarkGray);
}

fn dirs_from_symbol(symbol: &str) -> u8 {
    match symbol {
        "│" => DIR_UP | DIR_DOWN,
        "─" => DIR_LEFT | DIR_RIGHT,
        "├" => DIR_UP | DIR_DOWN | DIR_RIGHT,
        "┤" => DIR_UP | DIR_DOWN | DIR_LEFT,
        "┬" => DIR_LEFT | DIR_RIGHT | DIR_DOWN,
        "┴" => DIR_LEFT | DIR_RIGHT | DIR_UP,
        "┼" => DIR_UP | DIR_DOWN | DIR_LEFT | DIR_RIGHT,
        "┌" => DIR_DOWN | DIR_RIGHT,
        "┐" => DIR_DOWN | DIR_LEFT,
        "└" => DIR_UP | DIR_RIGHT,
        "┘" => DIR_UP | DIR_LEFT,
        _ => 0,
    }
}

fn symbol_from_dirs(dirs: u8) -> &'static str {
    match dirs {
        d if d == (DIR_UP | DIR_DOWN | DIR_LEFT | DIR_RIGHT) => "┼",
        d if d == (DIR_UP | DIR_DOWN | DIR_RIGHT) => "├",
        d if d == (DIR_UP | DIR_DOWN | DIR_LEFT) => "┤",
        d if d == (DIR_LEFT | DIR_RIGHT | DIR_DOWN) => "┬",
        d if d == (DIR_LEFT | DIR_RIGHT | DIR_UP) => "┴",
        d if d == (DIR_UP | DIR_DOWN) => "│",
        d if d == (DIR_LEFT | DIR_RIGHT) => "─",
        d if d == (DIR_DOWN | DIR_RIGHT) => "┌",
        d if d == (DIR_DOWN | DIR_LEFT) => "┐",
        d if d == (DIR_UP | DIR_RIGHT) => "└",
        d if d == (DIR_UP | DIR_LEFT) => "┘",
        d if d == DIR_UP || d == DIR_DOWN => "│",
        d if d == DIR_LEFT || d == DIR_RIGHT => "─",
        _ => " ",
    }
}

#[cfg(test)]
mod tests {
    use super::{DIR_DOWN, DIR_LEFT, DIR_RIGHT, DIR_UP, dirs_from_symbol, symbol_from_dirs};
    use super::{display_width_of_char_prefix, visible_slice_by_display_width};

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
    fn visible_slice_uses_display_columns() {
        let line = "a中bc";
        assert_eq!(visible_slice_by_display_width(line, 0, 3), "a中");
        assert_eq!(visible_slice_by_display_width(line, 1, 2), "中");
        assert_eq!(visible_slice_by_display_width(line, 3, 2), "bc");
    }
}
