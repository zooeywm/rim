use unicode_width::UnicodeWidthChar;

const PREVIEW_TAB_WIDTH: usize = 4;

pub fn wrap_preview_line(line: &str, width: usize) -> Vec<String> {
	if width == 0 {
		return vec![String::new()];
	}
	let mut rows = Vec::new();
	let mut current = String::new();
	let mut current_width = 0usize;

	for ch in line.chars() {
		if ch == '\t' {
			let spaces = PREVIEW_TAB_WIDTH - (current_width % PREVIEW_TAB_WIDTH);
			for _ in 0..spaces {
				if current_width > 0 && current_width.saturating_add(1) > width {
					rows.push(std::mem::take(&mut current));
					current_width = 0;
				}
				current.push(' ');
				current_width = current_width.saturating_add(1);
			}
			continue;
		}

		let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
		if current_width > 0 && current_width.saturating_add(ch_width) > width {
			rows.push(std::mem::take(&mut current));
			current_width = 0;
		}
		current.push(ch);
		current_width = current_width.saturating_add(ch_width);
	}

	if current.is_empty() {
		rows.push(String::new());
	} else {
		rows.push(current);
	}

	rows
}

pub fn preview_max_scroll(lines: &[String], width: usize) -> usize {
	preview_max_scroll_with_mode(lines, width, true)
}

pub fn preview_max_scroll_with_mode(lines: &[String], width: usize, word_wrap: bool) -> usize {
	preview_rows(lines, width, word_wrap).len().saturating_sub(1)
}

pub fn preview_rows(lines: &[String], width: usize, word_wrap: bool) -> Vec<String> {
	if word_wrap {
		return lines
			.iter()
			.flat_map(|line| wrap_preview_line(line.as_str(), width))
			.collect::<Vec<_>>();
	}
	lines
		.iter()
		.map(|line| wrap_preview_line(line.as_str(), width).into_iter().next().unwrap_or_default())
		.collect()
}

#[cfg(test)]
mod tests {
	use super::{preview_max_scroll, preview_max_scroll_with_mode, preview_rows, wrap_preview_line};

	#[test]
	fn wrap_preview_line_should_expand_tab_to_spaces() {
		let rows = wrap_preview_line("\tX", 8);
		assert_eq!(rows, vec!["    X".to_string()]);
	}

	#[test]
	fn preview_max_scroll_should_match_wrapped_rows() {
		let lines = vec!["a".repeat(9)];
		assert_eq!(preview_max_scroll(lines.as_slice(), 4), 2);
	}

	#[test]
	fn preview_rows_should_keep_single_row_per_line_when_word_wrap_disabled() {
		let lines = vec!["abcdefgh".to_string()];
		let rows = preview_rows(lines.as_slice(), 4, false);
		assert_eq!(rows, vec!["abcd".to_string()]);
		assert_eq!(preview_max_scroll_with_mode(lines.as_slice(), 4, false), 0);
	}
}
