use ropey::Rope;
use tracing::error;

use super::{ActionHandlerError, RimState};
use crate::{ports::{StorageIo, SwapEditOp}, state::{BufferId, compute_rope_text_diff}};

#[derive(Debug, Clone)]
pub(super) struct BufferTextSnapshot {
	pub(super) buffer_id: BufferId,
	pub(super) text:      Rope,
	pub(super) cursor:    crate::state::CursorState,
}

#[derive(Debug)]
struct TextDiff {
	start_char:    usize,
	deleted_text:  String,
	inserted_text: String,
}

pub(super) fn capture_active_buffer_text_snapshot(state: &RimState) -> Option<BufferTextSnapshot> {
	let buffer_id = state.active_buffer_id()?;
	let buffer = state.buffers.get(buffer_id)?;
	Some(BufferTextSnapshot { buffer_id, text: buffer.text.clone(), cursor: state.active_cursor() })
}

pub(super) fn enqueue_swap_ops<P>(ports: &P, state: &RimState, buffer_id: BufferId, ops: Vec<SwapEditOp>)
where P: StorageIo {
	if ops.is_empty() {
		return;
	}
	let Some(source_path) = state.buffers.get(buffer_id).and_then(|buffer| buffer.path.clone()) else {
		return;
	};
	for op in ops {
		if let Err(source) = ports.enqueue_edit(buffer_id, source_path.clone(), op) {
			let err = ActionHandlerError::PersistenceSwapEdit { source };
			error!("persistence worker unavailable while enqueueing swap edit: {}", err);
			break;
		}
	}
}

pub(super) fn swap_ops_from_text_diff(before: &Rope, after: &Rope) -> Vec<SwapEditOp> {
	if let Some(ops) = swap_ops_from_linewise_text_diff(before, after) {
		return ops;
	}

	let Some(diff) = compute_rope_text_diff(before, after) else {
		return Vec::new();
	};
	let delete_len = diff.deleted_text.chars().count();

	let mut ops = Vec::new();
	if delete_len > 0 {
		ops.push(SwapEditOp::Delete { pos: diff.start_char, len: delete_len });
	}
	if !diff.inserted_text.is_empty() {
		ops.push(SwapEditOp::Insert { pos: diff.start_char, text: diff.inserted_text });
	}
	ops
}

fn swap_ops_from_linewise_text_diff(before: &Rope, after: &Rope) -> Option<Vec<SwapEditOp>> {
	if before == after || before.len_lines() != after.len_lines() {
		return None;
	}

	let mut ops = Vec::new();
	let mut prior_rows_char_delta = 0isize;

	for row_idx in 0..before.len_lines() {
		let before_line = rope_line_text_without_newline(before, row_idx);
		let after_line = rope_line_text_without_newline(after, row_idx);
		if before_line == after_line {
			continue;
		}

		let Some(line_diff) = compute_text_diff(before_line.as_str(), after_line.as_str()) else {
			continue;
		};
		let base_pos = before.line_to_char(row_idx).saturating_add(line_diff.start_char);
		let pos = apply_char_delta(base_pos, prior_rows_char_delta);
		let delete_len = line_diff.deleted_text.chars().count();
		let insert_len = line_diff.inserted_text.chars().count();

		if delete_len > 0 {
			ops.push(SwapEditOp::Delete { pos, len: delete_len });
		}
		if !line_diff.inserted_text.is_empty() {
			ops.push(SwapEditOp::Insert { pos, text: line_diff.inserted_text });
		}

		prior_rows_char_delta += insert_len as isize - delete_len as isize;
	}

	Some(ops)
}

pub(super) fn enqueue_swap_ops_from_text_diff<P>(
	ports: &P,
	state: &RimState,
	before: Option<BufferTextSnapshot>,
) where
	P: StorageIo,
{
	let Some(before) = before else {
		return;
	};
	let Some(after_buffer) = state.buffers.get(before.buffer_id) else {
		return;
	};
	let ops = swap_ops_from_text_diff(&before.text, &after_buffer.text);
	enqueue_swap_ops(ports, state, before.buffer_id, ops);
}

fn compute_text_diff(before: &str, after: &str) -> Option<TextDiff> {
	if before == after {
		return None;
	}

	let before_chars = before.chars().collect::<Vec<_>>();
	let after_chars = after.chars().collect::<Vec<_>>();

	let mut common_prefix = 0usize;
	while common_prefix < before_chars.len()
		&& common_prefix < after_chars.len()
		&& before_chars[common_prefix] == after_chars[common_prefix]
	{
		common_prefix = common_prefix.saturating_add(1);
	}

	let mut before_mid_end = before_chars.len();
	let mut after_mid_end = after_chars.len();
	while before_mid_end > common_prefix
		&& after_mid_end > common_prefix
		&& before_chars[before_mid_end.saturating_sub(1)] == after_chars[after_mid_end.saturating_sub(1)]
	{
		before_mid_end = before_mid_end.saturating_sub(1);
		after_mid_end = after_mid_end.saturating_sub(1);
	}

	Some(TextDiff {
		start_char:    common_prefix,
		deleted_text:  before_chars[common_prefix..before_mid_end].iter().collect(),
		inserted_text: after_chars[common_prefix..after_mid_end].iter().collect(),
	})
}

fn rope_line_text_without_newline(text: &Rope, row_idx: usize) -> String {
	let mut line = text.line(row_idx).to_string();
	if line.ends_with('\n') {
		line.pop();
		if line.ends_with('\r') {
			line.pop();
		}
	}
	line
}

fn apply_char_delta(pos: usize, delta: isize) -> usize {
	if delta >= 0 { pos.saturating_add(delta as usize) } else { pos.saturating_sub(delta.unsigned_abs()) }
}
