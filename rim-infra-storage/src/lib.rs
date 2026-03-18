use std::{path::PathBuf, sync::Mutex, thread, time::Duration};

use anyhow::Result;
use rim_application::{action::{AppAction, FileLoadSource}, ports::SwapEditOp};
use rim_domain::model::{BufferId, PersistedBufferHistory, WorkspaceSessionSnapshot};
use rim_ports::{StorageIo, StorageIoError};
use tracing::error;

mod path_codec;
mod session;
mod swap_session;
mod undo_history;
mod worker;

use path_codec::{user_session_dir, user_swap_dir, user_undo_dir};
use worker::{StorageIoRequest, run_worker};

const SWAP_FILE_MAGIC: &str = "RIMSWP\t1";
const UNDO_FILE_VERSION: u32 = 1;
const FLUSH_DEBOUNCE_WINDOW: Duration = Duration::from_millis(180);
const INSERT_MERGE_WINDOW: Duration = Duration::from_millis(350);

#[derive(dep_inj::DepInj)]
#[target(StorageIoImpl)]
pub struct StorageIoState {
	request_tx:   flume::Sender<StorageIoRequest>,
	request_rx:   flume::Receiver<StorageIoRequest>,
	app_event_tx: flume::Sender<AppAction>,
	swap_dir:     PathBuf,
	undo_dir:     PathBuf,
	session_dir:  PathBuf,
	worker_join:  Mutex<Option<thread::JoinHandle<()>>>,
}

impl AsRef<StorageIoState> for StorageIoState {
	fn as_ref(&self) -> &StorageIoState { self }
}

impl<Deps> StorageIo for StorageIoImpl<Deps>
where Deps: AsRef<StorageIoState>
{
	type BufferId = BufferId;
	type EditOp = SwapEditOp;
	type PersistedBufferHistory = PersistedBufferHistory;
	type WorkspaceSessionSnapshot = WorkspaceSessionSnapshot;

	fn enqueue_load_workspace_session(&self) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::LoadWorkspaceSession,
			"enqueue_load_workspace_session",
			"load_workspace_session",
		)
	}

	fn enqueue_save_workspace_session(&self, snapshot: WorkspaceSessionSnapshot) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::SaveWorkspaceSession { snapshot },
			"enqueue_save_workspace_session",
			"save_workspace_session",
		)
	}

	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::LoadFile { buffer_id, path, source: FileLoadSource::Open },
			"enqueue_load",
			"load",
		)
	}

	fn enqueue_list_workspace_files(&self, workspace_root: PathBuf) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::ListWorkspaceFiles { workspace_root },
			"enqueue_list_workspace_files",
			"list_workspace_files",
		)
	}

	fn enqueue_load_workspace_file_preview(&self, path: PathBuf) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::LoadWorkspaceFilePreview { path },
			"enqueue_load_workspace_file_preview",
			"load_workspace_file_preview",
		)
	}

	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::SaveFile { buffer_id, path, text },
			"enqueue_save",
			"save",
		)
	}

	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::LoadFile { buffer_id, path, source: FileLoadSource::External },
			"enqueue_external_load",
			"reload",
		)
	}

	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError> {
		send_request(&self.request_tx, StorageIoRequest::Open { buffer_id, source_path }, "enqueue_open", "open")
	}

	fn enqueue_detect_conflict(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::DetectConflict { buffer_id, source_path },
			"enqueue_detect_conflict",
			"detect_conflict",
		)
	}

	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::Edit { buffer_id, source_path, op },
			"enqueue_edit",
			"edit",
		)
	}

	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::MarkClean { buffer_id, source_path },
			"enqueue_mark_clean",
			"mark_clean",
		)
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::InitializeBase { buffer_id, source_path, base_text, delete_existing },
			"enqueue_initialize_base",
			"initialize_base",
		)
	}

	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::Recover { buffer_id, source_path, base_text },
			"enqueue_recover",
			"recover",
		)
	}

	fn enqueue_load_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		expected_text: String,
		restore_view: bool,
	) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::LoadHistory { buffer_id, source_path, expected_text, restore_view },
			"enqueue_load_history",
			"load_history",
		)
	}

	fn enqueue_save_history(
		&self,
		_buffer_id: BufferId,
		source_path: PathBuf,
		history: PersistedBufferHistory,
	) -> Result<(), StorageIoError> {
		send_request(
			&self.request_tx,
			StorageIoRequest::SaveHistory { source_path, history },
			"enqueue_save_history",
			"save_history",
		)
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), StorageIoError> {
		send_request(&self.request_tx, StorageIoRequest::Close { buffer_id }, "enqueue_close", "close")
	}
}

fn send_request(
	request_tx: &flume::Sender<StorageIoRequest>,
	request: StorageIoRequest,
	log_name: &'static str,
	operation: &'static str,
) -> Result<(), StorageIoError> {
	request_tx.send(request).map_err(|err| {
		error!("{} failed: storage worker channel is disconnected: {}", log_name, err);
		StorageIoError::RequestChannelDisconnected { operation }
	})
}

impl StorageIoState {
	pub fn new(event_tx: flume::Sender<AppAction>) -> Self {
		let (request_tx, request_rx) = flume::unbounded();
		Self {
			request_tx,
			request_rx,
			app_event_tx: event_tx,
			swap_dir: user_swap_dir(),
			undo_dir: user_undo_dir(),
			session_dir: user_session_dir(),
			worker_join: Mutex::new(None),
		}
	}

	pub fn start(&self) {
		let mut worker_guard = self.worker_join.lock().expect("storage worker mutex poisoned");
		if worker_guard.is_some() {
			return;
		}
		let request_rx = self.request_rx.clone();
		let event_tx = self.app_event_tx.clone();
		let swap_dir = self.swap_dir.clone();
		let undo_dir = self.undo_dir.clone();
		let session_dir = self.session_dir.clone();
		let join = thread::spawn(move || {
			if let Err(err) = run_worker(request_rx, event_tx, swap_dir, undo_dir, session_dir) {
				error!("storage worker exited with error: {:#}", err);
			}
		});
		*worker_guard = Some(join);
	}
}

impl Drop for StorageIoState {
	fn drop(&mut self) {
		let _ = self.request_tx.send(StorageIoRequest::Shutdown);
		if let Ok(mut guard) = self.worker_join.lock()
			&& let Some(join) = guard.take()
		{
			let _ = join.join();
		}
	}
}

#[cfg(test)]
fn block_on_test<Output>(future: impl std::future::Future<Output = Output>) -> Output {
	thread_local! {
		static TEST_RUNTIME: std::cell::RefCell<Option<compio::runtime::Runtime>> = const {
			std::cell::RefCell::new(None)
		};
	}

	TEST_RUNTIME.with(|cell| {
		let mut runtime = cell.borrow_mut();
		let runtime =
			runtime.get_or_insert_with(|| compio::runtime::Runtime::new().expect("create test runtime failed"));
		runtime.block_on(future)
	})
}

#[cfg(test)]
mod tests;
