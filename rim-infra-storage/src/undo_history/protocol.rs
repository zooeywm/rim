use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use compio::{fs::OpenOptions, io::AsyncWriteAtExt};
use rim_domain::model::{BufferEditSnapshot, BufferHistoryEntry, CursorState, PersistedBufferHistory};
use serde::{Deserialize, Serialize};

use crate::{UNDO_FILE_VERSION, path_codec::{undo_legacy_path_for_source, undo_log_path_for_source, undo_meta_path_for_source}};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct LegacyUndoFileDocument {
	pub(super) version:      u32,
	pub(super) current_text: String,
	pub(super) cursor:       UndoCursor,
	pub(super) undo_stack:   Vec<UndoHistoryEntry>,
	pub(super) redo_stack:   Vec<UndoHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct UndoMetaDocument {
	pub(super) version:        u32,
	pub(super) base_text:      String,
	pub(super) head:           usize,
	pub(super) entry_count:    usize,
	pub(super) current_cursor: UndoCursor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct UndoCursor {
	pub(super) row: u16,
	pub(super) col: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct UndoHistoryEntry {
	pub(super) edits:         Vec<UndoEditSnapshot>,
	pub(super) before_cursor: UndoCursor,
	pub(super) after_cursor:  UndoCursor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct UndoEditSnapshot {
	pub(super) start_byte:    usize,
	pub(super) deleted_text:  String,
	pub(super) inserted_text: String,
}

impl From<CursorState> for UndoCursor {
	fn from(cursor: CursorState) -> Self { Self { row: cursor.row, col: cursor.col } }
}

impl From<UndoCursor> for CursorState {
	fn from(cursor: UndoCursor) -> Self { Self { row: cursor.row, col: cursor.col } }
}

impl From<BufferEditSnapshot> for UndoEditSnapshot {
	fn from(snapshot: BufferEditSnapshot) -> Self {
		Self {
			start_byte:    snapshot.start_byte,
			deleted_text:  snapshot.deleted_text,
			inserted_text: snapshot.inserted_text,
		}
	}
}

impl From<&BufferEditSnapshot> for UndoEditSnapshot {
	fn from(snapshot: &BufferEditSnapshot) -> Self {
		Self {
			start_byte:    snapshot.start_byte,
			deleted_text:  snapshot.deleted_text.clone(),
			inserted_text: snapshot.inserted_text.clone(),
		}
	}
}

impl From<UndoEditSnapshot> for BufferEditSnapshot {
	fn from(snapshot: UndoEditSnapshot) -> Self {
		Self {
			start_byte:    snapshot.start_byte,
			deleted_text:  snapshot.deleted_text,
			inserted_text: snapshot.inserted_text,
		}
	}
}

impl From<BufferHistoryEntry> for UndoHistoryEntry {
	fn from(entry: BufferHistoryEntry) -> Self {
		Self {
			edits:         entry.edits.into_iter().map(Into::into).collect(),
			before_cursor: entry.before_cursor.into(),
			after_cursor:  entry.after_cursor.into(),
		}
	}
}

impl From<UndoHistoryEntry> for BufferHistoryEntry {
	fn from(entry: UndoHistoryEntry) -> Self {
		Self {
			edits:         entry.edits.into_iter().map(Into::into).collect(),
			before_cursor: entry.before_cursor.into(),
			after_cursor:  entry.after_cursor.into(),
		}
	}
}

impl From<PersistedBufferHistory> for LegacyUndoFileDocument {
	fn from(history: PersistedBufferHistory) -> Self {
		Self {
			version:      UNDO_FILE_VERSION,
			current_text: history.current_text,
			cursor:       history.cursor.into(),
			undo_stack:   history.undo_stack.into_iter().map(Into::into).collect(),
			redo_stack:   history.redo_stack.into_iter().map(Into::into).collect(),
		}
	}
}

impl From<LegacyUndoFileDocument> for PersistedBufferHistory {
	fn from(document: LegacyUndoFileDocument) -> Self {
		Self {
			current_text: document.current_text,
			cursor:       document.cursor.into(),
			undo_stack:   document.undo_stack.into_iter().map(Into::into).collect(),
			redo_stack:   document.redo_stack.into_iter().map(Into::into).collect(),
		}
	}
}

pub(super) async fn rewrite_undo_log(
	undo_dir: &Path,
	source_path: &Path,
	entries: &[UndoHistoryEntry],
) -> Result<Vec<u64>> {
	let log_path = undo_log_path_for_source(undo_dir, source_path);
	if let Some(parent) = log_path.parent() {
		compio::fs::create_dir_all(parent)
			.await
			.with_context(|| format!("create undo log dir failed: {}", parent.display()))?;
	}

	let mut content = String::new();
	let mut offsets = Vec::with_capacity(entries.len());
	let mut current_len = 0u64;
	for entry in entries {
		let line = serde_json::to_string(entry).context("serialize undo entry failed")?;
		content.push_str(line.as_str());
		content.push('\n');
		current_len = current_len.saturating_add(line.len() as u64 + 1);
		offsets.push(current_len);
	}
	let write_result = compio::fs::write(&log_path, content.into_bytes()).await.0;
	write_result.with_context(|| format!("rewrite undo log failed: {}", log_path.display()))?;
	Ok(offsets)
}

pub(super) fn undo_log_truncate_offset(entry_end_offsets: &[u64], retained_entries: usize) -> u64 {
	if retained_entries == 0 {
		return 0;
	}
	entry_end_offsets.get(retained_entries.saturating_sub(1)).copied().unwrap_or(0)
}

pub(super) async fn truncate_undo_log(path: &Path, len: u64) -> Result<()> {
	let file = OpenOptions::new()
		.create(true)
		.write(true)
		.truncate(false)
		.open(path)
		.await
		.with_context(|| format!("open undo log for truncate failed: {}", path.display()))?;
	file.set_len(len).await.with_context(|| format!("truncate undo log failed: {}", path.display()))?;
	Ok(())
}

pub(super) async fn append_undo_log_entries_with_offsets(
	path: &Path,
	entries: &[UndoHistoryEntry],
) -> Result<Vec<u64>> {
	if entries.is_empty() {
		return Ok(Vec::new());
	}

	let mut payload = String::new();
	let mut entry_sizes = Vec::with_capacity(entries.len());
	for entry in entries {
		let line = serde_json::to_string(entry).context("serialize undo entry failed")?;
		payload.push_str(line.as_str());
		payload.push('\n');
		entry_sizes.push(line.len() as u64 + 1);
	}
	let mut file = OpenOptions::new()
		.create(true)
		.write(true)
		.open(path)
		.await
		.with_context(|| format!("open undo log for append failed: {}", path.display()))?;
	let initial_len =
		file.metadata().await.with_context(|| format!("stat undo log failed: {}", path.display()))?.len();
	let compio::BufResult(write_result, _) = file.write_all_at(payload.into_bytes(), initial_len).await;
	write_result.with_context(|| format!("append undo log failed: {}", path.display()))?;
	let mut offsets = Vec::with_capacity(entries.len());
	let mut current_len = initial_len;
	for entry_size in entry_sizes {
		current_len = current_len.saturating_add(entry_size);
		offsets.push(current_len);
	}
	file.sync_data().await.with_context(|| format!("sync undo log append failed: {}", path.display()))?;
	Ok(offsets)
}

pub(super) async fn write_undo_meta(path: &Path, meta: &UndoMetaDocument) -> Result<()> {
	if let Some(parent) = path.parent() {
		compio::fs::create_dir_all(parent)
			.await
			.with_context(|| format!("create undo meta dir failed: {}", parent.display()))?;
	}
	let content = serde_json::to_string(meta).context("serialize undo meta failed")?;
	let write_result = compio::fs::write(path, content.into_bytes()).await.0;
	write_result.with_context(|| format!("write undo meta failed: {}", path.display()))?;
	Ok(())
}

pub(super) async fn remove_undo_history_files(undo_dir: &Path, source_path: &Path) -> Result<()> {
	remove_optional_file(undo_log_path_for_source(undo_dir, source_path).as_path(), "remove undo log failed")
		.await?;
	remove_optional_file(undo_meta_path_for_source(undo_dir, source_path).as_path(), "remove undo meta failed")
		.await?;
	remove_optional_file(
		undo_legacy_path_for_source(undo_dir, source_path).as_path(),
		"remove legacy undo file failed",
	)
	.await?;
	Ok(())
}

pub(super) async fn remove_legacy_undo_file(undo_dir: &Path, source_path: &Path) -> Result<()> {
	remove_optional_file(
		undo_legacy_path_for_source(undo_dir, source_path).as_path(),
		"remove legacy undo file failed",
	)
	.await
}

async fn remove_optional_file(path: &Path, context: &str) -> Result<()> {
	match compio::fs::remove_file(path).await {
		Ok(()) => Ok(()),
		Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
		Err(err) => Err(err).with_context(|| format!("{}: {}", context, path.display())),
	}
}

pub(super) async fn read_undo_log_entries(path: &Path) -> Result<(Vec<UndoHistoryEntry>, Vec<u64>)> {
	let content = String::from_utf8(
		compio::fs::read(path).await.with_context(|| format!("read undo log failed: {}", path.display()))?,
	)
	.with_context(|| format!("decode undo log failed: {}", path.display()))?;
	let mut entries = Vec::new();
	let mut entry_end_offsets = Vec::new();
	let mut current_len = 0u64;
	for raw_line in content.split_inclusive('\n') {
		current_len = current_len.saturating_add(raw_line.len() as u64);
		let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
		if line.is_empty() {
			continue;
		}
		let entry = serde_json::from_str::<UndoHistoryEntry>(line)
			.with_context(|| format!("parse undo log entry failed: {}", path.display()))?;
		entries.push(entry);
		entry_end_offsets.push(current_len);
	}
	Ok((entries, entry_end_offsets))
}

pub(super) async fn read_undo_meta(path: &Path) -> Result<UndoMetaDocument> {
	let meta_text = String::from_utf8(
		compio::fs::read(path).await.with_context(|| format!("read undo meta failed: {}", path.display()))?,
	)
	.with_context(|| format!("decode undo meta failed: {}", path.display()))?;
	let meta: UndoMetaDocument = serde_json::from_str(meta_text.as_str())
		.with_context(|| format!("parse undo meta failed: {}", path.display()))?;
	if meta.version != UNDO_FILE_VERSION {
		bail!("unsupported undo meta version {} in {}", meta.version, path.display());
	}
	Ok(meta)
}

pub(super) async fn read_legacy_undo_document(path: &Path) -> Result<Option<LegacyUndoFileDocument>> {
	if compio::fs::metadata(path).await.is_err() {
		return Ok(None);
	}

	let content = String::from_utf8(
		compio::fs::read(path)
			.await
			.with_context(|| format!("read legacy undo file failed: {}", path.display()))?,
	)
	.with_context(|| format!("decode legacy undo file failed: {}", path.display()))?;
	let document: LegacyUndoFileDocument = serde_json::from_str(content.as_str())
		.with_context(|| format!("parse legacy undo file failed: {}", path.display()))?;
	if document.version != UNDO_FILE_VERSION {
		return Err(anyhow!("unsupported legacy undo file version {} in {}", document.version, path.display()));
	}
	Ok(Some(document))
}
