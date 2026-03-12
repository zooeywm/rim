use std::collections::HashSet;

use ratatui::{buffer::Buffer, layout::Rect, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Clear, Paragraph, Widget}};
use rim_kernel::{command::CommandPaletteMatch, state::{CommandPaletteState, RimState}};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const COMMAND_INPUT_MAX_ROWS: usize = 4;
const MAX_RESULTS: usize = 12;

pub(super) struct CommandPaletteWidgets {
	input:    CommandPaletteInputWidget,
	results:  CommandPaletteResultsWidget,
	cursor_x: u16,
	cursor_y: u16,
}

impl CommandPaletteWidgets {
	pub(super) fn from_state(state: &RimState, content_area: Rect) -> Option<Self> {
		let palette = state.command_palette()?.clone();
		let width = content_area.width.saturating_sub(6).clamp(48, 104);
		let x = content_area.x.saturating_add(content_area.width.saturating_sub(width) / 2);
		let input_inner_width = width.saturating_sub(2).max(1) as usize;
		let wrapped_input = wrap_command_input(palette.query.as_str(), input_inner_width);
		let visible_input_rows = wrapped_input.len().clamp(1, COMMAND_INPUT_MAX_ROWS);
		let input_area =
			Rect { x, y: content_area.y.saturating_add(1), width, height: visible_input_rows as u16 + 2 };
		let result_rows = palette.items.len().clamp(1, MAX_RESULTS) as u16;
		let available_result_rows =
			content_area.height.saturating_sub(input_area.height).saturating_sub(4).max(3);
		let results_area = Rect {
			x,
			y: input_area.y.saturating_add(input_area.height),
			width,
			height: result_rows.min(available_result_rows.saturating_sub(2)).saturating_add(2),
		};
		let hidden_rows = wrapped_input.len().saturating_sub(visible_input_rows);
		let visible_input = &wrapped_input[hidden_rows..];
		let cursor_line = visible_input.last().cloned().unwrap_or_else(|| "> ".to_string());
		let cursor_x =
			input_area.x.saturating_add(1).saturating_add(UnicodeWidthStr::width(cursor_line.as_str()) as u16);
		let cursor_y = input_area.y.saturating_add(1).saturating_add(visible_input_rows.saturating_sub(1) as u16);

		Some(Self {
			input: CommandPaletteInputWidget { query: palette.query.clone(), area: input_area },
			results: CommandPaletteResultsWidget { palette, area: results_area },
			cursor_x,
			cursor_y,
		})
	}

	pub(super) fn cursor_position(&self) -> (u16, u16) { (self.cursor_x, self.cursor_y) }
}

impl Widget for CommandPaletteWidgets {
	fn render(self, _area: Rect, buf: &mut Buffer) {
		let Self { input, results, .. } = self;
		let input_area = input.area;
		let results_area = results.area;
		input.render(input_area, buf);
		results.render(results_area, buf);
	}
}

struct CommandPaletteInputWidget {
	query: String,
	area:  Rect,
}

impl Widget for CommandPaletteInputWidget {
	fn render(self, _area: Rect, buf: &mut Buffer) {
		Clear.render(self.area, buf);
		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(Color::Cyan))
			.title(" Cmdline ");
		let inner = block.inner(self.area);
		block.render(self.area, buf);
		let wrapped = wrap_command_input(self.query.as_str(), inner.width as usize);
		let visible_rows = wrapped.len().clamp(1, inner.height as usize);
		let hidden_rows = wrapped.len().saturating_sub(visible_rows);
		let lines = wrapped[hidden_rows..]
			.iter()
			.enumerate()
			.map(|(index, row)| {
				if index == 0 {
					Line::from(vec![
						Span::styled("> ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
						Span::styled(
							row.strip_prefix("> ").unwrap_or(row.as_str()).to_string(),
							Style::default().fg(Color::White),
						),
					])
				} else {
					Line::styled(row.clone(), Style::default().fg(Color::White))
				}
			})
			.collect::<Vec<_>>();
		Paragraph::new(lines).render(inner, buf);
	}
}

struct CommandPaletteResultsWidget {
	palette: CommandPaletteState,
	area:    Rect,
}

impl Widget for CommandPaletteResultsWidget {
	fn render(self, _area: Rect, buf: &mut Buffer) {
		Clear.render(self.area, buf);
		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(Color::Cyan))
			.title(" Commands ");
		let inner = block.inner(self.area);
		block.render(self.area, buf);

		let lines = if self.palette.items.is_empty() {
			vec![Line::styled("No matching commands", Style::default().fg(Color::DarkGray))]
		} else {
			let name_width = compute_name_column_width(inner.width as usize);
			let command_width = compute_command_column_width(inner.width as usize, name_width);
			let desc_width = inner.width as usize - name_width - command_width - 2;
			self
				.palette
				.items
				.iter()
				.take(MAX_RESULTS)
				.enumerate()
				.map(|(index, item)| {
					render_command_palette_item(
						item,
						index == self.palette.selected,
						name_width,
						command_width,
						desc_width,
					)
				})
				.collect::<Vec<_>>()
		};
		Paragraph::new(lines).render(inner, buf);
	}
}

fn compute_name_column_width(body_width: usize) -> usize {
	let preferred = body_width / 6;
	preferred.clamp(8, 18)
}

fn compute_command_column_width(body_width: usize, name_width: usize) -> usize {
	let remaining = body_width.saturating_sub(name_width + 2);
	let preferred = remaining.saturating_mul(2) / 5;
	preferred.clamp(24, remaining.saturating_sub(8).max(24))
}

fn render_command_palette_item(
	item: &CommandPaletteMatch,
	selected: bool,
	name_width: usize,
	command_width: usize,
	desc_width: usize,
) -> Line<'static> {
	let row_style = if selected { Style::default().bg(Color::Rgb(18, 36, 52)) } else { Style::default() };
	let name_style = if item.is_error {
		row_style.fg(Color::LightRed).add_modifier(Modifier::BOLD)
	} else {
		row_style.fg(Color::Rgb(109, 208, 255)).add_modifier(Modifier::BOLD)
	};
	let command_style = if item.is_error {
		row_style.fg(Color::LightRed).add_modifier(Modifier::BOLD)
	} else {
		row_style.fg(Color::Rgb(150, 220, 255)).add_modifier(Modifier::BOLD)
	};
	let command_base_style = if item.is_error {
		row_style.fg(Color::Rgb(255, 150, 150)).add_modifier(Modifier::BOLD)
	} else {
		row_style.fg(Color::Rgb(176, 190, 214)).add_modifier(Modifier::BOLD)
	};
	let desc_highlight_style =
		if item.is_error { row_style.fg(Color::LightRed) } else { row_style.fg(Color::Rgb(255, 198, 109)) };
	let desc_base_style =
		if item.is_error { row_style.fg(Color::Rgb(255, 170, 170)) } else { row_style.fg(Color::Gray) };
	let mut spans = highlighted_text(
		item.name.as_str(),
		name_width,
		&item.name_match_indices,
		name_style,
		if item.is_error { row_style.fg(Color::Rgb(255, 170, 170)) } else { row_style.fg(Color::White) },
		true,
	);
	spans.push(Span::styled(" ", row_style));
	spans.extend(highlighted_text(
		item.command_id_label.as_str(),
		command_width,
		&item.command_id_match_indices,
		command_style,
		command_base_style,
		true,
	));
	spans.push(Span::styled(" ", row_style));
	spans.extend(highlighted_text(
		item.description.as_str(),
		desc_width,
		&item.description_match_indices,
		desc_highlight_style,
		desc_base_style,
		false,
	));
	Line::from(spans)
}

fn highlighted_text(
	text: &str,
	width: usize,
	indices: &[usize],
	highlight_style: Style,
	base_style: Style,
	pad_to_width: bool,
) -> Vec<Span<'static>> {
	let highlight_set = indices.iter().copied().collect::<HashSet<_>>();
	let mut spans = Vec::new();
	let mut display_width = 0usize;

	for (index, ch) in text.chars().enumerate() {
		let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
		if display_width.saturating_add(ch_width) > width {
			break;
		}
		let style = if highlight_set.contains(&index) { highlight_style } else { base_style };
		spans.push(Span::styled(ch.to_string(), style));
		display_width = display_width.saturating_add(ch_width);
	}

	if pad_to_width && display_width < width {
		spans.push(Span::styled(" ".repeat(width - display_width), base_style));
	}

	spans
}

fn wrap_command_input(query: &str, width: usize) -> Vec<String> {
	let prompt = "> ";
	let mut rows = Vec::new();
	let mut current = prompt.to_string();
	let mut current_width = UnicodeWidthStr::width(prompt);

	for ch in query.chars() {
		let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
		if current_width > 0 && current_width.saturating_add(ch_width) > width {
			rows.push(std::mem::take(&mut current));
			current_width = 0;
		}
		current.push(ch);
		current_width = current_width.saturating_add(ch_width);
	}

	if current.is_empty() {
		rows.push(prompt.to_string());
	} else {
		rows.push(current);
	}

	rows
}
