use std::path::PathBuf;

use rim_infra_file_watcher::FileWatcherImpl;
use rim_infra_storage::StorageIoImpl;
use rim_kernel::{ports::{FileWatcher, FileWatcherError, StorageIo, StorageIoError, SwapEditOp}, state::{BufferId, PersistedBufferHistory, WorkspaceSessionSnapshot}};

use crate::app::AppPorts;

impl StorageIo for AppPorts<'_> {
	fn enqueue_load_workspace_session(&self) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_load_workspace_session()
	}

	fn enqueue_save_workspace_session(&self, snapshot: WorkspaceSessionSnapshot) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_save_workspace_session(snapshot)
	}

	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_load(buffer_id, path)
	}

	fn enqueue_list_workspace_files(&self, workspace_root: PathBuf) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_list_workspace_files(workspace_root)
	}

	fn enqueue_load_workspace_file_preview(&self, path: PathBuf) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_load_workspace_file_preview(path)
	}

	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_save(buffer_id, path, text)
	}

	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_external_load(buffer_id, path)
	}

	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_open(buffer_id, source_path)
	}

	fn enqueue_detect_conflict(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_detect_conflict(buffer_id, source_path)
	}

	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_edit(buffer_id, source_path, op)
	}

	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_mark_clean(buffer_id, source_path)
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_initialize_base(
			buffer_id,
			source_path,
			base_text,
			delete_existing,
		)
	}

	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_recover(buffer_id, source_path, base_text)
	}

	fn enqueue_load_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		expected_text: String,
		restore_view: bool,
	) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_load_history(
			buffer_id,
			source_path,
			expected_text,
			restore_view,
		)
	}

	fn enqueue_save_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		history: PersistedBufferHistory,
	) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_save_history(buffer_id, source_path, history)
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), StorageIoError> {
		StorageIoImpl::inj_ref(self.storage_io).enqueue_close(buffer_id)
	}
}

impl FileWatcher for AppPorts<'_> {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self.file_watcher).enqueue_watch(buffer_id, path)
	}

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self.file_watcher).enqueue_unwatch(buffer_id)
	}

	fn enqueue_watch_workspace_root(&self, path: PathBuf) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self.file_watcher).enqueue_watch_workspace_root(path)
	}
}
