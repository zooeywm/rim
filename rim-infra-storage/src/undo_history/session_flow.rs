use std::{collections::HashMap, path::{Path, PathBuf}};

use anyhow::{Result, bail};
use rim_kernel::state::PersistedBufferHistory;

use super::{UndoHistorySession, derive_base_text_from_snapshot, is_base_text_consistent, linear_history_entries_from_snapshot, longest_common_undo_entry_prefix, protocol::{UndoMetaDocument, append_undo_log_entries_with_offsets, read_legacy_undo_document, read_undo_log_entries, read_undo_meta, remove_legacy_undo_file, remove_undo_history_files, rewrite_undo_log, truncate_undo_log, undo_log_truncate_offset, write_undo_meta}, replay_undo_entries};

pub(crate) async fn load_undo_history(
	undo_dir: &Path,
	source_path: &Path,
	expected_text: &str,
	undo_sessions: &mut HashMap<PathBuf, UndoHistorySession>,
) -> Result<Option<PersistedBufferHistory>> {
	let key = source_path.to_path_buf();
	let session = match undo_sessions.entry(key) {
		std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
		std::collections::hash_map::Entry::Vacant(entry) => {
			if let Some(loaded) = load_undo_session_from_disk(undo_dir, source_path).await? {
				entry.insert(loaded)
			} else {
				entry.insert(UndoHistorySession::empty(expected_text.to_string()));
				return Ok(None);
			}
		}
	};

	if session.current_text != expected_text {
		*session = UndoHistorySession::empty(expected_text.to_string());
		return Ok(None);
	}

	Ok(Some(session.to_persisted_history()))
}

pub(crate) async fn save_undo_history(
	undo_dir: &Path,
	source_path: &Path,
	history: &PersistedBufferHistory,
	undo_sessions: &mut HashMap<PathBuf, UndoHistorySession>,
) -> Result<()> {
	let key = source_path.to_path_buf();
	let mut session = if let Some(session) = undo_sessions.remove(key.as_path()) {
		session
	} else {
		load_undo_session_from_disk(undo_dir, source_path)
			.await?
			.unwrap_or_else(|| UndoHistorySession::empty(history.current_text.clone()))
	};

	sync_undo_history_session(undo_dir, source_path, &mut session, history).await?;
	undo_sessions.insert(key, session);
	Ok(())
}

async fn load_undo_session_from_disk(
	undo_dir: &Path,
	source_path: &Path,
) -> Result<Option<UndoHistorySession>> {
	if let Some(session) = load_undo_session_from_new_format(undo_dir, source_path).await? {
		return Ok(Some(session));
	}
	if let Some(session) = load_legacy_undo_history(undo_dir, source_path).await? {
		return Ok(Some(session));
	}
	Ok(None)
}

async fn load_undo_session_from_new_format(
	undo_dir: &Path,
	source_path: &Path,
) -> Result<Option<UndoHistorySession>> {
	let meta_path = crate::path_codec::undo_meta_path_for_source(undo_dir, source_path);
	let log_path = crate::path_codec::undo_log_path_for_source(undo_dir, source_path);
	let meta_exists = compio::fs::metadata(&meta_path).await.is_ok();
	let log_exists = compio::fs::metadata(&log_path).await.is_ok();
	if !meta_exists && !log_exists {
		return Ok(None);
	}
	if !meta_exists || !log_exists {
		bail!("incomplete undo persistence files: meta={} log={}", meta_path.display(), log_path.display());
	}

	let meta = read_undo_meta(meta_path.as_path()).await?;

	let (entries, entry_end_offsets) = read_undo_log_entries(log_path.as_path()).await?;
	if entries.len() != meta.entry_count {
		bail!(
			"undo entry count mismatch: meta={} log={} file={}",
			meta.entry_count,
			entries.len(),
			source_path.display()
		);
	}
	if meta.head > entries.len() {
		bail!("undo head {} exceeds entry count {} for {}", meta.head, entries.len(), source_path.display());
	}

	let current_text = replay_undo_entries(meta.base_text.as_str(), &entries[..meta.head])?;
	Ok(Some(UndoHistorySession {
		base_text: meta.base_text,
		entries,
		head: meta.head,
		current_cursor: meta.current_cursor.into(),
		current_text,
		entry_end_offsets,
	}))
}

async fn load_legacy_undo_history(undo_dir: &Path, source_path: &Path) -> Result<Option<UndoHistorySession>> {
	let legacy_path = crate::path_codec::undo_legacy_path_for_source(undo_dir, source_path);
	let Some(document) = read_legacy_undo_document(legacy_path.as_path()).await? else {
		return Ok(None);
	};
	Ok(Some(UndoHistorySession::from_persisted_history(document.into())?))
}

async fn sync_undo_history_session(
	undo_dir: &Path,
	source_path: &Path,
	session: &mut UndoHistorySession,
	history: &PersistedBufferHistory,
) -> Result<()> {
	let new_entries = linear_history_entries_from_snapshot(history);
	let new_head = history.undo_stack.len();
	if new_entries.is_empty() {
		remove_undo_history_files(undo_dir, source_path).await?;
		let current_text = history.current_text.clone();
		*session = UndoHistorySession {
			base_text: current_text.clone(),
			entries: Vec::new(),
			head: 0,
			current_cursor: history.cursor,
			current_text,
			entry_end_offsets: Vec::new(),
		};
		return Ok(());
	}

	let base_text = if is_base_text_consistent(
		session.base_text.as_str(),
		new_entries.as_slice(),
		new_head,
		history.current_text.as_str(),
	)? {
		session.base_text.clone()
	} else {
		derive_base_text_from_snapshot(history)?
	};

	let common_prefix = longest_common_undo_entry_prefix(session.entries.as_slice(), new_entries.as_slice());
	let can_truncate_existing_tail = session.entry_end_offsets.len() == session.entries.len();
	if session.base_text != base_text
		|| (!session.entries.is_empty() && common_prefix < session.entries.len() && !can_truncate_existing_tail)
	{
		session.entries = new_entries;
		session.entry_end_offsets = rewrite_undo_log(undo_dir, source_path, session.entries.as_slice()).await?;
	} else {
		if common_prefix < session.entries.len() {
			let truncate_len = undo_log_truncate_offset(session.entry_end_offsets.as_slice(), common_prefix);
			truncate_undo_log(
				crate::path_codec::undo_log_path_for_source(undo_dir, source_path).as_path(),
				truncate_len,
			)
			.await?;
			session.entries.truncate(common_prefix);
			session.entry_end_offsets.truncate(common_prefix);
		}
		if common_prefix < new_entries.len() {
			let mut appended = new_entries;
			let appended = appended.split_off(common_prefix);
			let appended_offsets = append_undo_log_entries_with_offsets(
				crate::path_codec::undo_log_path_for_source(undo_dir, source_path).as_path(),
				appended.as_slice(),
			)
			.await?;
			session.entries.extend(appended);
			session.entry_end_offsets.extend(appended_offsets);
		}
	}

	session.base_text = base_text.clone();
	session.head = new_head;
	session.current_cursor = history.cursor;
	session.current_text = history.current_text.clone();
	write_undo_meta(
		crate::path_codec::undo_meta_path_for_source(undo_dir, source_path).as_path(),
		&UndoMetaDocument {
			version: crate::UNDO_FILE_VERSION,
			base_text,
			head: new_head,
			entry_count: session.entries.len(),
			current_cursor: history.cursor.into(),
		},
	)
	.await?;
	remove_legacy_undo_file(undo_dir, source_path).await?;
	Ok(())
}
