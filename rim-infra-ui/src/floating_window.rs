use ratatui::{buffer::Buffer, layout::Rect, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Clear, Paragraph, Widget}};
use rim_application::state::{FloatingWindowLine, FloatingWindowPlacement, FloatingWindowState, RimState};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const KEY_COLUMN_WIDTH: usize = 10;
const KEY_GAP_WIDTH: usize = 1;

pub(super) struct FloatingWindowWidget {
	window: FloatingWindowState,
	area:   Rect,
}

impl FloatingWindowWidget {
	pub(super) fn from_state(state: &RimState, content_area: Rect) -> Option<Self> {
		let window = state.floating_window()?.clone();
		let area = resolve_window_area(content_area, window.placement);
		Some(Self { window, area })
	}
}

impl Widget for FloatingWindowWidget {
	fn render(self, _area: Rect, buf: &mut Buffer) {
		Clear.render(self.area, buf);

		let title = match self.window.subtitle.as_deref() {
			Some(subtitle) => format!(" {} | {} ", self.window.title, subtitle),
			None => format!(" {} ", self.window.title),
		};
		let block =
			Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)).title(title);
		let inner = block.inner(self.area);
		block.render(self.area, buf);

		if inner.width == 0 || inner.height == 0 {
			return;
		}

		let footer_rows = if self.window.footer.is_some() { 2u16 } else { 0u16 };
		let body_height = inner.height.saturating_sub(footer_rows).max(1);
		let body_area = Rect { x: inner.x, y: inner.y, width: inner.width, height: body_height };

		let wrapped_before_scroll = self
			.window
			.lines
			.iter()
			.take(self.window.scroll.min(self.window.lines.len()))
			.map(|line| wrapped_line_height(line, inner.width))
			.sum::<usize>();
		let wrapped_total_rows =
			self.window.lines.iter().map(|line| wrapped_line_height(line, inner.width)).sum::<usize>().max(1);

		let body_lines = self
			.window
			.lines
			.iter()
			.skip(self.window.scroll.min(self.window.lines.len()))
			.flat_map(|line| wrap_floating_line(line, inner.width))
			.take(body_height as usize)
			.collect::<Vec<_>>();
		let visible_wrapped_rows = body_lines.len().max(1);
		Paragraph::new(body_lines).render(body_area, buf);

		if let Some(footer) = self.window.footer.as_deref() {
			let footer_area = Rect {
				x:      inner.x,
				y:      inner.y.saturating_add(body_height),
				width:  inner.width,
				height: footer_rows,
			};
			let current_page =
				wrapped_before_scroll.saturating_add(visible_wrapped_rows).div_ceil(body_height as usize).max(1);
			let total_pages = wrapped_total_rows.div_ceil(body_height as usize).max(1);
			Paragraph::new(vec![
				Line::raw(""),
				Line::styled(
					format!("{}  Pg {}/{}", footer, current_page.min(total_pages), total_pages),
					Style::default().fg(Color::DarkGray),
				),
			])
			.render(footer_area, buf);
		}
	}
}

fn wrap_floating_line(line: &FloatingWindowLine, width: u16) -> Vec<Line<'static>> {
	let key_style = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
	let summary_style =
		if line.is_prefix { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Magenta) };
	let width = width as usize;
	let summary_width = width.saturating_sub(KEY_COLUMN_WIDTH + KEY_GAP_WIDTH).max(1);
	let summary_chunks = wrap_plain_text(line.summary.as_str(), summary_width);
	let mut wrapped = Vec::with_capacity(summary_chunks.len().max(1));

	for (index, chunk) in summary_chunks.into_iter().enumerate() {
		let key_text = if index == 0 {
			pad_display_width(line.key.as_str(), KEY_COLUMN_WIDTH)
		} else {
			" ".repeat(KEY_COLUMN_WIDTH)
		};
		wrapped.push(Line::from(vec![
			Span::styled(key_text, key_style),
			Span::raw(" "),
			Span::styled(chunk, summary_style),
		]));
	}

	wrapped
}

fn wrapped_line_height(line: &FloatingWindowLine, width: u16) -> usize {
	let width = width as usize;
	let summary_width = width.saturating_sub(KEY_COLUMN_WIDTH + KEY_GAP_WIDTH).max(1);
	wrap_plain_text(line.summary.as_str(), summary_width).len().max(1)
}

fn wrap_plain_text(text: &str, max_width: usize) -> Vec<String> {
	let mut chunks = Vec::new();
	let mut current = String::new();
	let mut current_width = 0usize;

	for ch in text.chars() {
		let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
		if current_width > 0 && current_width.saturating_add(ch_width) > max_width {
			chunks.push(std::mem::take(&mut current));
			current_width = 0;
		}
		current.push(ch);
		current_width = current_width.saturating_add(ch_width);
	}

	if current.is_empty() {
		chunks.push(String::new());
	} else {
		chunks.push(current);
	}

	chunks
}

fn pad_display_width(text: &str, target_width: usize) -> String {
	let width = UnicodeWidthStr::width(text);
	if width >= target_width {
		text.to_string()
	} else {
		format!("{}{}", text, " ".repeat(target_width - width))
	}
}

fn resolve_window_area(content_area: Rect, placement: FloatingWindowPlacement) -> Rect {
	match placement {
		FloatingWindowPlacement::Centered { width, height } => Rect {
			x:      content_area
				.x
				.saturating_add(content_area.width.saturating_sub(width.min(content_area.width)) / 2),
			y:      content_area
				.y
				.saturating_add(content_area.height.saturating_sub(height.min(content_area.height)) / 2),
			width:  width.min(content_area.width).max(3),
			height: height.min(content_area.height).max(3),
		},
		FloatingWindowPlacement::BottomRight { width, height, margin_right, margin_bottom } => {
			let width = width.min(content_area.width).max(3);
			let height = height.min(content_area.height).max(3);
			Rect {
				x: content_area
					.x
					.saturating_add(content_area.width.saturating_sub(width.saturating_add(margin_right))),
				y: content_area
					.y
					.saturating_add(content_area.height.saturating_sub(height.saturating_add(margin_bottom))),
				width,
				height,
			}
		}
		FloatingWindowPlacement::Absolute { x, y, width, height } => Rect {
			x:      content_area.x.saturating_add(x.min(content_area.width.saturating_sub(1))),
			y:      content_area.y.saturating_add(y.min(content_area.height.saturating_sub(1))),
			width:  width.min(content_area.width).max(3),
			height: height.min(content_area.height).max(3),
		},
	}
}
