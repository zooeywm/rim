use std::{collections::HashMap, path::Path, time::Instant};

use rim_kernel::{action::{AppAction, FileAction, SwapConflictInfo}, state::BufferId};
use tracing::error;

use super::{FlushSchedule, StorageIoRequest, create_swap_session, get_or_create_swap_session, send_file_action};
use crate::{FLUSH_DEBOUNCE_WINDOW, swap_session::SwapSession};

pub(super) async fn handle_swap_request(
	request: StorageIoRequest,
	event_tx: &flume::Sender<AppAction>,
	swap_dir: &Path,
	pid: u32,
	username: &str,
	deadlines: &mut HashMap<BufferId, FlushSchedule>,
	sessions: &mut HashMap<BufferId, SwapSession>,
) -> bool {
	match request {
		StorageIoRequest::Open { buffer_id, source_path } => {
			let session =
				create_swap_session(buffer_id, source_path.as_path(), swap_dir, pid, username, "open").await;
			sessions.insert(buffer_id, session);
		}
		StorageIoRequest::DetectConflict { buffer_id, source_path } => {
			let session = get_or_create_swap_session(
				sessions,
				buffer_id,
				source_path.as_path(),
				swap_dir,
				pid,
				username,
				"detect_conflict",
			)
			.await;
			let result = match session.rebind_if_needed(source_path.as_path()).await {
				Ok(()) => session.detect_conflict().await.map(|conflict| {
					conflict.map(|(owner_pid, owner_username)| SwapConflictInfo {
						pid:      owner_pid,
						username: owner_username,
					})
				}),
				Err(err) => Err(err),
			};
			if !send_file_action(
				event_tx,
				FileAction::SwapConflictDetected { buffer_id, result },
				"SwapConflictDetected",
			) {
				return false;
			}
		}
		StorageIoRequest::Edit { buffer_id, source_path, op } => {
			let session = get_or_create_swap_session(
				sessions,
				buffer_id,
				source_path.as_path(),
				swap_dir,
				pid,
				username,
				"edit",
			)
			.await;
			if let Err(err) = session.rebind_if_needed(source_path.as_path()).await {
				error!("swap rebind before edit failed: {:#}", err);
				return true;
			}
			let now = Instant::now();
			if let Err(err) = session.apply_edit(op, now).await {
				error!("swap edit apply failed: {:#}", err);
			} else if let Some(generation) = session.schedule_flush_generation() {
				schedule_flush(deadlines, buffer_id, generation, now);
			}
		}
		StorageIoRequest::MarkClean { buffer_id, source_path } => {
			let session = get_or_create_swap_session(
				sessions,
				buffer_id,
				source_path.as_path(),
				swap_dir,
				pid,
				username,
				"mark_clean",
			)
			.await;
			if let Err(err) = session.rebind_if_needed(source_path.as_path()).await {
				error!("swap rebind before mark_clean failed: {:#}", err);
				return true;
			}
			if let Err(err) = session.mark_clean().await {
				error!("swap mark_clean failed: {:#}", err);
			}
		}
		StorageIoRequest::InitializeBase { buffer_id, source_path, base_text, delete_existing } => {
			let session = get_or_create_swap_session(
				sessions,
				buffer_id,
				source_path.as_path(),
				swap_dir,
				pid,
				username,
				"initialize_base",
			)
			.await;
			if let Err(err) = session.rebind_if_needed(source_path.as_path()).await {
				error!("swap initialize_base rebind failed: {:#}", err);
			} else if let Err(err) = session.initialize_base(base_text, delete_existing).await {
				error!("swap initialize_base failed: {:#}", err);
			}
		}
		StorageIoRequest::Recover { buffer_id, source_path, base_text } => {
			let session = get_or_create_swap_session(
				sessions,
				buffer_id,
				source_path.as_path(),
				swap_dir,
				pid,
				username,
				"recover",
			)
			.await;
			let had_swap_before_recover = compio::fs::metadata(&session.swap_path).await.is_ok();
			let result = match session.rebind_if_needed(source_path.as_path()).await {
				Ok(()) => session.recover(base_text).await,
				Err(err) => Err(err),
			};
			let should_send_callback = had_swap_before_recover || result.as_ref().is_err();
			if should_send_callback
				&& !send_file_action(
					event_tx,
					FileAction::SwapRecoverCompleted { buffer_id, result },
					"SwapRecoverCompleted",
				) {
				return false;
			}
		}
		StorageIoRequest::Close { buffer_id } => {
			deadlines.remove(&buffer_id);
			if let Some(session) = sessions.remove(&buffer_id)
				&& let Err(err) = session.close().await
			{
				error!("close swap session failed: {:#}", err);
			}
		}
		_ => unreachable!("non swap request routed to handle_swap_request"),
	}

	true
}

fn schedule_flush(
	deadlines: &mut HashMap<BufferId, FlushSchedule>,
	buffer_id: BufferId,
	generation: u64,
	now: Instant,
) {
	deadlines.insert(buffer_id, FlushSchedule { generation, due_at: now + FLUSH_DEBOUNCE_WINDOW });
}
