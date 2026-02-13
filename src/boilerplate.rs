use std::path::PathBuf;

use crate::{app::App, file_io_service::{FileIo, FileIoImpl, FileIoServiceError}, file_watcher_service::{FileWatcher, FileWatcherImpl, FileWatcherServiceError}, state::BufferId};

impl FileIo for App {
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError> {
		FileIoImpl::inj_ref(self).enqueue_load(buffer_id, path)
	}

	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), FileIoServiceError> {
		FileIoImpl::inj_ref(self).enqueue_save(buffer_id, path, text)
	}

	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError> {
		FileIoImpl::inj_ref(self).enqueue_external_load(buffer_id, path)
	}
}

impl FileWatcher for App {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherServiceError> {
		FileWatcherImpl::inj_ref(self).enqueue_watch(buffer_id, path)
	}

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherServiceError> {
		FileWatcherImpl::inj_ref(self).enqueue_unwatch(buffer_id)
	}
}
