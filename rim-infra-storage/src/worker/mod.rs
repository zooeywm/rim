use std::{collections::HashMap, path::{Path, PathBuf}, time::Instant};

use anyhow::{Context, Result};
use rim_kernel::{action::{AppAction, FileAction, FileLoadSource}, ports::SwapEditOp, state::{BufferId, PersistedBufferHistory}};
use tracing::error;

mod file_transfer;
mod history_flow;
mod swap_flow;

use file_transfer::handle_file_transfer_request;
use history_flow::handle_history_request;
use swap_flow::handle_swap_request;

use crate::{swap_session::SwapSession, undo_history::UndoHistorySession};

pub(super) fn run_worker(
	request_rx: flume::Receiver<StorageIoRequest>,
	event_tx: flume::Sender<AppAction>,
	swap_dir: PathBuf,
	undo_dir: PathBuf,
) -> Result<()> {
	let runtime = compio::runtime::Runtime::new().context("storage worker runtime init failed")?;
	runtime.block_on(async move {
		const MAX_IN_FLIGHT: usize = 64;

		compio::fs::create_dir_all(&swap_dir)
			.await
			.with_context(|| format!("create swap dir failed: {}", swap_dir.display()))?;
		compio::fs::create_dir_all(&undo_dir)
			.await
			.with_context(|| format!("create undo dir failed: {}", undo_dir.display()))?;

		let mut deadlines: HashMap<BufferId, FlushSchedule> = HashMap::new();
		let mut sessions: HashMap<BufferId, SwapSession> = HashMap::new();
		let mut undo_sessions: HashMap<PathBuf, UndoHistorySession> = HashMap::new();
		let mut in_flight: Vec<compio::runtime::JoinHandle<()>> = Vec::new();
		let pid = std::process::id();
		let username = current_username();

		loop {
			dispatch_due_flushes(&mut deadlines, &mut sessions, Instant::now()).await;

			let next_due = deadlines.values().map(|schedule| schedule.due_at).min();
			let request = match next_due {
				Some(next_due_at) => match compio::time::timeout_at(next_due_at, request_rx.recv_async()).await {
					Ok(Ok(request)) => Some(request),
					Ok(Err(_)) => None,
					Err(_) => continue,
				},
				None => request_rx.recv_async().await.ok(),
			};

			let Some(request) = request else {
				break;
			};
			let keep_running = handle_request(request, StorageIoContext {
				event_tx: &event_tx,
				swap_dir: &swap_dir,
				undo_dir: &undo_dir,
				pid,
				username: username.as_str(),
				deadlines: &mut deadlines,
				sessions: &mut sessions,
				undo_sessions: &mut undo_sessions,
				in_flight: &mut in_flight,
			})
			.await;
			if in_flight.len() >= MAX_IN_FLIGHT {
				let oldest = in_flight.remove(0);
				let _ = oldest.await;
			}
			if !keep_running {
				break;
			}
		}

		for (_, session) in sessions {
			if let Err(err) = session.close().await {
				error!("close swap session during worker shutdown failed: {:#}", err);
			}
		}

		for task in in_flight {
			let _ = task.await;
		}
		Ok::<(), anyhow::Error>(())
	})?;

	Ok(())
}

#[derive(Debug, Clone, Copy)]
struct FlushSchedule {
	generation: u64,
	due_at:     Instant,
}

#[derive(Debug)]
pub(super) enum StorageIoRequest {
	Shutdown,
	LoadFile {
		buffer_id: BufferId,
		path:      PathBuf,
		source:    FileLoadSource,
	},
	SaveFile {
		buffer_id: BufferId,
		path:      PathBuf,
		text:      String,
	},
	Open {
		buffer_id:   BufferId,
		source_path: PathBuf,
	},
	DetectConflict {
		buffer_id:   BufferId,
		source_path: PathBuf,
	},
	Edit {
		buffer_id:   BufferId,
		source_path: PathBuf,
		op:          SwapEditOp,
	},
	MarkClean {
		buffer_id:   BufferId,
		source_path: PathBuf,
	},
	InitializeBase {
		buffer_id:       BufferId,
		source_path:     PathBuf,
		base_text:       String,
		delete_existing: bool,
	},
	Recover {
		buffer_id:   BufferId,
		source_path: PathBuf,
		base_text:   String,
	},
	LoadHistory {
		buffer_id:     BufferId,
		source_path:   PathBuf,
		expected_text: String,
	},
	SaveHistory {
		source_path: PathBuf,
		history:     PersistedBufferHistory,
	},
	Close {
		buffer_id: BufferId,
	},
}

struct StorageIoContext<'a> {
	event_tx:      &'a flume::Sender<AppAction>,
	swap_dir:      &'a Path,
	undo_dir:      &'a Path,
	pid:           u32,
	username:      &'a str,
	deadlines:     &'a mut HashMap<BufferId, FlushSchedule>,
	sessions:      &'a mut HashMap<BufferId, SwapSession>,
	undo_sessions: &'a mut HashMap<PathBuf, UndoHistorySession>,
	in_flight:     &'a mut Vec<compio::runtime::JoinHandle<()>>,
}

async fn dispatch_due_flushes(
	deadlines: &mut HashMap<BufferId, FlushSchedule>,
	sessions: &mut HashMap<BufferId, SwapSession>,
	now: Instant,
) {
	let due_buffers = deadlines
		.iter()
		.filter_map(|(buffer_id, schedule)| (schedule.due_at <= now).then_some((*buffer_id, schedule.generation)))
		.collect::<Vec<_>>();
	for (buffer_id, generation) in due_buffers {
		deadlines.remove(&buffer_id);
		handle_flush_due(buffer_id, generation, sessions).await;
	}
}

async fn handle_request(request: StorageIoRequest, context: StorageIoContext<'_>) -> bool {
	let StorageIoContext {
		event_tx,
		swap_dir,
		undo_dir,
		pid,
		username,
		deadlines,
		sessions,
		undo_sessions,
		in_flight,
	} = context;
	match request {
		StorageIoRequest::Shutdown => return false,
		StorageIoRequest::LoadFile { .. } | StorageIoRequest::SaveFile { .. } => {
			handle_file_transfer_request(request, event_tx, in_flight);
		}
		StorageIoRequest::Open { .. }
		| StorageIoRequest::DetectConflict { .. }
		| StorageIoRequest::Edit { .. }
		| StorageIoRequest::MarkClean { .. }
		| StorageIoRequest::InitializeBase { .. }
		| StorageIoRequest::Recover { .. }
		| StorageIoRequest::Close { .. } => {
			return handle_swap_request(request, event_tx, swap_dir, pid, username, deadlines, sessions).await;
		}
		StorageIoRequest::LoadHistory { .. } | StorageIoRequest::SaveHistory { .. } => {
			return handle_history_request(request, event_tx, undo_dir, undo_sessions).await;
		}
	}

	true
}

async fn handle_flush_due(
	buffer_id: BufferId,
	generation: u64,
	sessions: &mut HashMap<BufferId, SwapSession>,
) {
	let Some(session) = sessions.get_mut(&buffer_id) else {
		return;
	};
	if !session.should_flush_generation(generation) {
		return;
	}
	if let Err(err) = session.flush_pending().await {
		error!(
			"swap flush failed: source={} swap={} error={:#}",
			session.source_path.display(),
			session.swap_path.display(),
			err
		);
	}
}

async fn send_file_action_async(
	event_tx: flume::Sender<AppAction>,
	action: FileAction,
	action_name: &'static str,
) -> bool {
	send_file_action(&event_tx, action, action_name)
}

fn send_file_action(
	event_tx: &flume::Sender<AppAction>,
	action: FileAction,
	action_name: &'static str,
) -> bool {
	if let Err(err) = event_tx.send(AppAction::File(action)) {
		error!("send {} failed: {}", action_name, err);
		return false;
	}
	true
}

async fn get_or_create_swap_session<'a>(
	sessions: &'a mut HashMap<BufferId, SwapSession>,
	buffer_id: BufferId,
	source_path: &Path,
	swap_dir: &Path,
	pid: u32,
	username: &str,
	operation: &'static str,
) -> &'a mut SwapSession {
	match sessions.entry(buffer_id) {
		std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
		std::collections::hash_map::Entry::Vacant(entry) => {
			let session = create_swap_session(buffer_id, source_path, swap_dir, pid, username, operation).await;
			entry.insert(session)
		}
	}
}

async fn create_swap_session(
	buffer_id: BufferId,
	source_path: &Path,
	swap_dir: &Path,
	pid: u32,
	username: &str,
	operation: &'static str,
) -> SwapSession {
	let mut session = SwapSession::new(buffer_id, source_path, swap_dir, pid, username.to_string());
	if let Err(err) = session.bind_lease().await {
		error!("swap session {} lease bind failed: {:#}", operation, err);
	}
	session
}

async fn load_file(path: PathBuf) -> Result<String> {
	let file_bytes =
		compio::fs::read(&path).await.with_context(|| format!("read file failed: {}", path.display()))?;
	String::from_utf8(file_bytes).with_context(|| format!("decode utf-8 failed: {}", path.display()))
}

async fn save_file(path: PathBuf, text: String) -> Result<()> {
	let write_result = compio::fs::write(&path, text.into_bytes()).await.0;
	write_result.with_context(|| format!("write file failed: {}", path.display())).map(|_| ())
}

fn current_username() -> String {
	std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "unknown".to_string())
}
