use super::{BufferHistoryEntry, CursorState, EditorMode, PendingBlockInsert, PendingInsertUndoGroup, PendingSwapDecision, RimState, StatusBarMode, rope_line_count};

impl RimState {
	pub fn status_line(&self) -> String {
		let cursor = self.active_cursor();
		let total_rows = self
			.active_buffer_id()
			.and_then(|buffer_id| self.buffers.get(buffer_id))
			.map(|buffer| rope_line_count(&buffer.text) as u16)
			.unwrap_or(1);
		let progress = if cursor.row <= 1 {
			"Top".to_string()
		} else if cursor.row >= total_rows {
			"Bot".to_string()
		} else {
			let percent = (u32::from(cursor.row) * 100 / u32::from(total_rows)) as u16;
			format!("{}%", percent)
		};
		let cursor_pos = format!("{}:{} {}", cursor.row, cursor.col, progress);

		if self.mode == EditorMode::Command {
			return format!(":{} | {}", self.command_line, cursor_pos);
		}
		if self.status_bar.key_sequence.is_empty() {
			return format!("{} | {}", self.status_bar.message, cursor_pos);
		}

		format!("{} | keys {} | {}", self.status_bar.message, self.status_bar.key_sequence, cursor_pos)
	}

	pub fn is_insert_mode(&self) -> bool { self.mode == EditorMode::Insert }

	pub fn is_command_mode(&self) -> bool { self.mode == EditorMode::Command }

	pub fn is_visual_mode(&self) -> bool {
		matches!(self.mode, EditorMode::VisualChar | EditorMode::VisualLine | EditorMode::VisualBlock)
	}

	pub fn is_visual_line_mode(&self) -> bool { self.mode == EditorMode::VisualLine }

	pub fn is_visual_block_mode(&self) -> bool { self.mode == EditorMode::VisualBlock }

	pub fn is_block_insert_mode(&self) -> bool {
		self.mode == EditorMode::Insert && self.pending_block_insert.is_some()
	}

	pub fn enter_insert_mode(&mut self) {
		self.mode = EditorMode::Insert;
		self.visual_anchor = None;
		self.pending_block_insert = None;
		self.status_bar.mode = StatusBarMode::Insert;
		self.close_key_hints();
	}

	pub fn enter_block_insert_mode(&mut self, pending: PendingBlockInsert) {
		self.mode = EditorMode::Insert;
		self.visual_anchor = None;
		self.pending_block_insert = Some(pending);
		self.status_bar.mode = StatusBarMode::InsertBlock;
		self.close_key_hints();
	}

	pub fn exit_insert_mode(&mut self) {
		self.mode = EditorMode::Normal;
		self.visual_anchor = None;
		self.pending_block_insert = None;
		self.status_bar.mode = StatusBarMode::Normal;
		self.close_key_hints();
		self.clamp_cursor_to_navigable_col();
	}

	pub fn enter_command_mode(&mut self) {
		self.mode = EditorMode::Command;
		self.visual_anchor = None;
		self.command_line.clear();
		self.status_bar.mode = StatusBarMode::Command;
		self.close_key_hints();
	}

	pub fn exit_command_mode(&mut self) {
		self.mode = EditorMode::Normal;
		self.visual_anchor = None;
		self.command_line.clear();
		self.status_bar.mode = StatusBarMode::Normal;
		self.close_key_hints();
	}

	pub fn enter_visual_mode(&mut self) {
		self.mode = EditorMode::VisualChar;
		if self.visual_anchor.is_none() {
			self.visual_anchor = Some(self.active_cursor());
		}
		self.status_bar.mode = StatusBarMode::Visual;
		self.close_key_hints();
	}

	pub fn enter_visual_line_mode(&mut self) {
		let anchor_row = self.visual_anchor.map(|cursor| cursor.row).unwrap_or_else(|| self.active_cursor().row);
		self.mode = EditorMode::VisualLine;
		self.visual_anchor = Some(CursorState { row: anchor_row, col: 1 });
		self.status_bar.mode = StatusBarMode::VisualLine;
		self.close_key_hints();
	}

	pub fn enter_visual_block_mode(&mut self) {
		self.mode = EditorMode::VisualBlock;
		if self.visual_anchor.is_none() {
			self.visual_anchor = Some(self.active_cursor());
		}
		self.status_bar.mode = StatusBarMode::VisualBlock;
		self.close_key_hints();
	}

	pub fn exit_visual_mode(&mut self) {
		self.mode = EditorMode::Normal;
		self.visual_anchor = None;
		self.status_bar.mode = StatusBarMode::Normal;
		self.close_key_hints();
	}

	pub fn push_command_char(&mut self, ch: char) { self.command_line.push(ch); }

	pub fn pop_command_char(&mut self) { let _ = self.command_line.pop(); }

	pub fn take_command_line(&mut self) -> String {
		let command = self.command_line.trim().to_string();
		self.exit_command_mode();
		command
	}

	pub fn set_pending_swap_decision(&mut self, pending: PendingSwapDecision) {
		self.pending_swap_decision = Some(pending);
	}

	pub fn take_pending_swap_decision(&mut self) -> Option<PendingSwapDecision> {
		self.pending_swap_decision.take()
	}

	pub fn begin_insert_history_group(&mut self) {
		if self.pending_insert_group.is_some() {
			return;
		}
		let Some(buffer_id) = self.active_buffer_id() else {
			return;
		};
		self.pending_insert_group =
			Some(PendingInsertUndoGroup { buffer_id, before_cursor: self.active_cursor(), edits: Vec::new() });
	}

	pub fn cancel_insert_history_group(&mut self) { self.pending_insert_group = None; }

	pub fn commit_insert_history_group(&mut self) {
		let Some(group) = self.pending_insert_group.take() else {
			return;
		};
		if group.edits.is_empty() {
			return;
		}
		let after_cursor = self.cursor_for_buffer(group.buffer_id).unwrap_or(group.before_cursor);
		self.push_buffer_history_entry(group.buffer_id, BufferHistoryEntry {
			edits: group.edits,
			before_cursor: group.before_cursor,
			after_cursor,
		});
	}
}
