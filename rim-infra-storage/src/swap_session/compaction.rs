use anyhow::Result;
use rim_application::ports::SwapEditOp;
use ropey::Rope;

use super::{BufferedSwapOp, SwapSession, protocol::{append_buffered_swap_ops, append_buffered_swap_ops_iter, apply_swap_op, truncate_swap_file}};

impl SwapSession {
	pub(crate) async fn flush_pending(&mut self) -> Result<()> {
		if self.pending_ops.is_empty() {
			return Ok(());
		}
		let compaction_plan = compact_logged_suffix_against_pending_prefix(
			self.snapshot_rope.as_ref(),
			self.logged_ops.as_slice(),
			self.pending_ops.as_slice(),
			&self.rope,
		);
		let mut rewritten_logged_ops =
			compaction_plan.compacted.then(|| self.logged_ops[..compaction_plan.retained_logged_len].to_vec());
		let mut ops_to_append = Vec::new();
		let mut tail_rewrite_start =
			if compaction_plan.compacted { compaction_plan.retained_logged_len } else { self.logged_ops.len() };
		let mut should_rewrite_logged_tail = compaction_plan.compacted;

		for op in self.pending_ops[compaction_plan.pending_start..].iter().cloned() {
			let compaction = if let Some(rewritten_logged_ops) = rewritten_logged_ops.as_mut() {
				compact_buffered_op_against_logged_ops(rewritten_logged_ops, &op)
			} else {
				let preview = preview_compact_buffered_op_against_logged_ops(self.logged_ops.as_slice(), &op);
				if preview == TailCompaction::None {
					TailCompaction::None
				} else {
					rewritten_logged_ops = Some(self.logged_ops.clone());
					compact_buffered_op_against_logged_ops(
						rewritten_logged_ops.as_mut().expect("rewritten logged ops should exist"),
						&op,
					)
				}
			};
			match compaction {
				TailCompaction::None => ops_to_append.push(op),
				TailCompaction::RemovedLast => {
					let rewritten_logged_len =
						rewritten_logged_ops.as_ref().expect("rewritten logged ops should exist").len();
					tail_rewrite_start = tail_rewrite_start.min(rewritten_logged_len);
					should_rewrite_logged_tail = true;
				}
				TailCompaction::MutatedLast => {
					let rewritten_logged_len =
						rewritten_logged_ops.as_ref().expect("rewritten logged ops should exist").len();
					tail_rewrite_start = tail_rewrite_start.min(rewritten_logged_len.saturating_sub(1));
					should_rewrite_logged_tail = true;
				}
			}
		}

		if should_rewrite_logged_tail {
			self.logged_ops = rewritten_logged_ops.expect("rewrite path should materialize logged ops");
			self.rewrite_logged_tail(tail_rewrite_start, ops_to_append.as_slice()).await?;
		} else {
			let appended_offsets = append_buffered_swap_ops(self.swap_path.as_path(), &ops_to_append).await?;
			self.logged_ops.extend(ops_to_append);
			self.logged_end_offsets.extend(appended_offsets);
		}
		self.pending_ops.clear();
		self.last_pending_at = None;
		Ok(())
	}

	async fn rewrite_logged_tail(&mut self, tail_start: usize, ops_to_append: &[BufferedSwapOp]) -> Result<()> {
		let truncate_len = self.logged_truncate_offset(tail_start);
		truncate_swap_file(self.swap_path.as_path(), truncate_len).await?;
		self.logged_end_offsets.truncate(tail_start);

		let retained_tail = &self.logged_ops[tail_start..];
		if !retained_tail.is_empty() || !ops_to_append.is_empty() {
			let appended_offsets = append_buffered_swap_ops_iter(
				self.swap_path.as_path(),
				retained_tail.iter().chain(ops_to_append.iter()),
			)
			.await?;
			self.logged_end_offsets.extend(appended_offsets);
		}
		Ok(())
	}

	fn logged_truncate_offset(&self, retained_logged_len: usize) -> u64 {
		if retained_logged_len == 0 {
			return self.snapshot_len;
		}
		self.logged_end_offsets.get(retained_logged_len.saturating_sub(1)).copied().unwrap_or(self.snapshot_len)
	}
}

fn remove_string_char_range(text: &mut String, start_char: usize, end_char: usize) {
	let start_byte = char_index_to_byte_index(text.as_str(), start_char);
	let end_byte = char_index_to_byte_index(text.as_str(), end_char);
	text.replace_range(start_byte..end_byte, "");
}

fn char_index_to_byte_index(text: &str, char_index: usize) -> usize {
	if char_index == 0 {
		return 0;
	}
	text.char_indices().nth(char_index).map_or(text.len(), |(byte_index, _)| byte_index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TailCompaction {
	None,
	RemovedLast,
	MutatedLast,
}

fn compact_buffered_op_against_logged_ops(
	logged_ops: &mut Vec<BufferedSwapOp>,
	op: &BufferedSwapOp,
) -> TailCompaction {
	match &op.op {
		SwapEditOp::Delete { pos, len } => compact_delete_against_insert_tail(logged_ops, *pos, *len),
		SwapEditOp::Insert { pos, text } => compact_insert_against_delete_tail(logged_ops, *pos, text.as_str()),
	}
}

fn preview_compact_buffered_op_against_logged_ops(
	logged_ops: &[BufferedSwapOp],
	op: &BufferedSwapOp,
) -> TailCompaction {
	match &op.op {
		SwapEditOp::Delete { pos, len } => preview_compact_delete_against_insert_tail(logged_ops, *pos, *len),
		SwapEditOp::Insert { pos, text } => {
			preview_compact_insert_against_delete_tail(logged_ops, *pos, text.as_str())
		}
	}
}

fn compact_logged_suffix_against_pending_prefix(
	snapshot_rope: Option<&Rope>,
	logged_ops: &[BufferedSwapOp],
	pending_ops: &[BufferedSwapOp],
	current_rope: &Rope,
) -> LoggedPendingCompactionPlan {
	let Some(snapshot_rope) = snapshot_rope else {
		return LoggedPendingCompactionPlan {
			retained_logged_len: logged_ops.len(),
			pending_start:       0,
			compacted:           false,
		};
	};

	let mut best: Option<(usize, usize)> = None;
	for logged_suffix_len in 1..=logged_ops.len() {
		for pending_prefix_len in 1..=pending_ops.len() {
			let mut candidate = snapshot_rope.clone();
			apply_buffered_ops(&mut candidate, &logged_ops[..logged_ops.len().saturating_sub(logged_suffix_len)]);
			apply_buffered_ops(&mut candidate, &pending_ops[pending_prefix_len..]);
			if candidate == *current_rope {
				let should_replace = best.is_none_or(|(best_logged, best_pending)| {
					logged_suffix_len + pending_prefix_len > best_logged + best_pending
				});
				if should_replace {
					best = Some((logged_suffix_len, pending_prefix_len));
				}
			}
		}
	}

	let Some((logged_suffix_len, pending_prefix_len)) = best else {
		return LoggedPendingCompactionPlan {
			retained_logged_len: logged_ops.len(),
			pending_start:       0,
			compacted:           false,
		};
	};

	LoggedPendingCompactionPlan {
		retained_logged_len: logged_ops.len().saturating_sub(logged_suffix_len),
		pending_start:       pending_prefix_len,
		compacted:           true,
	}
}

#[derive(Debug, Clone, Copy)]
struct LoggedPendingCompactionPlan {
	retained_logged_len: usize,
	pending_start:       usize,
	compacted:           bool,
}

pub(super) fn compact_delete_against_insert_tail(
	ops: &mut Vec<BufferedSwapOp>,
	pos: usize,
	len: usize,
) -> TailCompaction {
	let Some(BufferedSwapOp { op: SwapEditOp::Insert { pos: insert_pos, text: insert_text }, .. }) =
		ops.last_mut()
	else {
		return TailCompaction::None;
	};

	let insert_len = insert_text.chars().count();
	let insert_end = insert_pos.saturating_add(insert_len);
	let delete_end = pos.saturating_add(len);
	if pos < *insert_pos || delete_end > insert_end {
		return TailCompaction::None;
	}

	let relative_start = pos - *insert_pos;
	let relative_end = relative_start + len;
	remove_string_char_range(insert_text, relative_start, relative_end);
	if insert_text.is_empty() {
		ops.pop();
		return TailCompaction::RemovedLast;
	}
	TailCompaction::MutatedLast
}

fn preview_compact_delete_against_insert_tail(
	ops: &[BufferedSwapOp],
	pos: usize,
	len: usize,
) -> TailCompaction {
	let Some(BufferedSwapOp { op: SwapEditOp::Insert { pos: insert_pos, text: insert_text }, .. }) = ops.last()
	else {
		return TailCompaction::None;
	};

	let insert_len = insert_text.chars().count();
	let insert_end = insert_pos.saturating_add(insert_len);
	let delete_end = pos.saturating_add(len);
	if pos < *insert_pos || delete_end > insert_end {
		return TailCompaction::None;
	}
	if pos == *insert_pos && delete_end == insert_end {
		return TailCompaction::RemovedLast;
	}
	TailCompaction::MutatedLast
}

pub(super) fn compact_insert_against_delete_tail(
	ops: &mut Vec<BufferedSwapOp>,
	pos: usize,
	text: &str,
) -> TailCompaction {
	let Some(BufferedSwapOp {
		op: SwapEditOp::Delete { pos: delete_pos, len: delete_len },
		deleted_text: Some(deleted_text),
	}) = ops.last()
	else {
		return TailCompaction::None;
	};

	if *delete_pos != pos || *delete_len != text.chars().count() || deleted_text != text {
		return TailCompaction::None;
	}

	ops.pop();
	TailCompaction::RemovedLast
}

fn preview_compact_insert_against_delete_tail(
	ops: &[BufferedSwapOp],
	pos: usize,
	text: &str,
) -> TailCompaction {
	let Some(BufferedSwapOp {
		op: SwapEditOp::Delete { pos: delete_pos, len: delete_len },
		deleted_text: Some(deleted_text),
	}) = ops.last()
	else {
		return TailCompaction::None;
	};

	if *delete_pos != pos || *delete_len != text.chars().count() || deleted_text != text {
		return TailCompaction::None;
	}

	TailCompaction::RemovedLast
}

fn apply_buffered_ops(text: &mut Rope, ops: &[BufferedSwapOp]) {
	for op in ops {
		apply_swap_op(text, op.op.clone());
	}
}
