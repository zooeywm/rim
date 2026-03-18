mod protocol;
mod session_flow;

use anyhow::{Result, bail};
use protocol::{UndoEditSnapshot, UndoHistoryEntry};
use rim_domain::model::{CursorState, PersistedBufferHistory};
use ropey::Rope;
pub(crate) use session_flow::{load_undo_history, save_undo_history};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UndoHistorySession {
	base_text:         String,
	entries:           Vec<UndoHistoryEntry>,
	head:              usize,
	current_cursor:    CursorState,
	current_text:      String,
	entry_end_offsets: Vec<u64>,
}

impl UndoHistorySession {
	fn empty(current_text: String) -> Self {
		Self {
			base_text: current_text.clone(),
			entries: Vec::new(),
			head: 0,
			current_cursor: CursorState::default(),
			current_text,
			entry_end_offsets: Vec::new(),
		}
	}

	fn from_persisted_history(history: PersistedBufferHistory) -> Result<Self> {
		let entries = linear_history_entries_from_snapshot(&history);
		let head = history.undo_stack.len();
		let base_text = derive_base_text_from_snapshot(&history)?;
		Ok(Self {
			base_text,
			entries,
			head,
			current_cursor: history.cursor,
			current_text: history.current_text,
			entry_end_offsets: Vec::new(),
		})
	}

	fn to_persisted_history(&self) -> PersistedBufferHistory {
		let undo_stack = self.entries[..self.head].iter().cloned().map(Into::into).collect::<Vec<_>>();
		let redo_stack = self.entries[self.head..].iter().rev().cloned().map(Into::into).collect::<Vec<_>>();
		PersistedBufferHistory {
			current_text: self.current_text.clone(),
			cursor: self.current_cursor,
			undo_stack,
			redo_stack,
		}
	}
}

fn linear_history_entries_from_snapshot(history: &PersistedBufferHistory) -> Vec<UndoHistoryEntry> {
	let mut entries = history.undo_stack.iter().cloned().map(Into::into).collect::<Vec<_>>();
	entries.extend(history.redo_stack.iter().rev().cloned().map(Into::into));
	entries
}

fn derive_base_text_from_snapshot(history: &PersistedBufferHistory) -> Result<String> {
	let mut base_rope = Rope::from_str(history.current_text.as_str());
	for entry in history.undo_stack.iter().rev() {
		for edit in entry.edits.iter().rev() {
			apply_undo_edit_to_rope_undo(&mut base_rope, &UndoEditSnapshot::from(edit));
		}
	}

	let base_text = base_rope.to_string();
	if !is_base_text_consistent(
		base_text.as_str(),
		linear_history_entries_from_snapshot(history).as_slice(),
		history.undo_stack.len(),
		history.current_text.as_str(),
	)? {
		bail!("persisted undo history is internally inconsistent");
	}
	Ok(base_text)
}

fn is_base_text_consistent(
	base_text: &str,
	entries: &[UndoHistoryEntry],
	head: usize,
	current_text: &str,
) -> Result<bool> {
	if head > entries.len() {
		return Ok(false);
	}
	Ok(replay_undo_entries(base_text, &entries[..head])? == current_text)
}

fn longest_common_undo_entry_prefix(lhs: &[UndoHistoryEntry], rhs: &[UndoHistoryEntry]) -> usize {
	lhs.iter().zip(rhs.iter()).take_while(|(left, right)| left == right).count()
}

fn replay_undo_entries(base_text: &str, entries: &[UndoHistoryEntry]) -> Result<String> {
	let mut rope = Rope::from_str(base_text);
	for entry in entries {
		for edit in &entry.edits {
			apply_undo_edit_to_rope_redo(&mut rope, edit);
		}
	}
	Ok(rope.to_string())
}

fn apply_undo_edit_to_rope_undo(text: &mut Rope, delta: &UndoEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let inserted_end_byte = delta.start_byte.saturating_add(delta.inserted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(inserted_end_byte);
	text.remove(start_char..end_char);
	if !delta.deleted_text.is_empty() {
		text.insert(start_char, delta.deleted_text.as_str());
	}
}

fn apply_undo_edit_to_rope_redo(text: &mut Rope, delta: &UndoEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let deleted_end_byte = delta.start_byte.saturating_add(delta.deleted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(deleted_end_byte);
	text.remove(start_char..end_char);
	if !delta.inserted_text.is_empty() {
		text.insert(start_char, delta.inserted_text.as_str());
	}
}
