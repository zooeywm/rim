use std::{collections::HashMap, path::{Path, PathBuf}};

use rim_kernel::action::{AppAction, FileAction};

use super::{StorageIoRequest, send_file_action};
use crate::undo_history::{UndoHistorySession, load_undo_history, save_undo_history};

pub(super) async fn handle_history_request(
	request: StorageIoRequest,
	event_tx: &flume::Sender<AppAction>,
	undo_dir: &Path,
	undo_sessions: &mut HashMap<PathBuf, UndoHistorySession>,
) -> bool {
	match request {
		StorageIoRequest::LoadHistory { buffer_id, source_path, expected_text, restore_view } => {
			let result =
				load_undo_history(undo_dir, source_path.as_path(), expected_text.as_str(), undo_sessions).await;
			if !send_file_action(
				event_tx,
				FileAction::UndoHistoryLoaded { buffer_id, source_path, expected_text, restore_view, result },
				"UndoHistoryLoaded",
			) {
				return false;
			}
		}
		StorageIoRequest::SaveHistory { source_path, history } => {
			if let Err(err) = save_undo_history(undo_dir, source_path.as_path(), &history, undo_sessions).await {
				tracing::error!("save undo history failed: source={} error={:#}", source_path.display(), err);
			}
		}
		_ => unreachable!("non history request routed to handle_history_request"),
	}

	true
}
