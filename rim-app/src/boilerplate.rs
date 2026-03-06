use std::path::PathBuf;

use rim_infra_file_io::FileIoImpl;
use rim_infra_file_watcher::FileWatcherImpl;
use rim_infra_persistence::PersistenceIoImpl;
use rim_kernel::{ports::{FileIo, FileIoError, FileWatcher, FileWatcherError, PersistenceIo, PersistenceIoError, SwapEditOp}, state::{BufferId, PersistedBufferHistory}};

use crate::app::{App, AppPorts};

impl FileIo for App {
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self).enqueue_load(buffer_id, path)
	}

	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self).enqueue_save(buffer_id, path, text)
	}

	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self).enqueue_external_load(buffer_id, path)
	}
}

impl FileWatcher for App {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self).enqueue_watch(buffer_id, path)
	}

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self).enqueue_unwatch(buffer_id)
	}
}

impl PersistenceIo for App {
	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_open(buffer_id, source_path)
	}

	fn enqueue_detect_conflict(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_detect_conflict(buffer_id, source_path)
	}

	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_edit(buffer_id, source_path, op)
	}

	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_mark_clean(buffer_id, source_path)
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_initialize_base(
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
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_recover(buffer_id, source_path, base_text)
	}

	fn enqueue_load_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		expected_text: String,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_load_history(buffer_id, source_path, expected_text)
	}

	fn enqueue_save_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		history: PersistedBufferHistory,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_save_history(buffer_id, source_path, history)
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self).enqueue_close(buffer_id)
	}
}

impl FileIo for AppPorts<'_> {
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self.file_io).enqueue_load(buffer_id, path)
	}

	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self.file_io).enqueue_save(buffer_id, path, text)
	}

	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self.file_io).enqueue_external_load(buffer_id, path)
	}
}

impl FileWatcher for AppPorts<'_> {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self.file_watcher).enqueue_watch(buffer_id, path)
	}

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self.file_watcher).enqueue_unwatch(buffer_id)
	}
}

impl PersistenceIo for AppPorts<'_> {
	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_open(buffer_id, source_path)
	}

	fn enqueue_detect_conflict(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_detect_conflict(buffer_id, source_path)
	}

	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_edit(buffer_id, source_path, op)
	}

	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_mark_clean(buffer_id, source_path)
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_initialize_base(
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
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_recover(buffer_id, source_path, base_text)
	}

	fn enqueue_load_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		expected_text: String,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_load_history(
			buffer_id,
			source_path,
			expected_text,
		)
	}

	fn enqueue_save_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		history: PersistedBufferHistory,
	) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_save_history(buffer_id, source_path, history)
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), PersistenceIoError> {
		PersistenceIoImpl::inj_ref(self.persistence_io).enqueue_close(buffer_id)
	}
}
