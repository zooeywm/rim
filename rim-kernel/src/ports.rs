use std::path::PathBuf;

use thiserror::Error;

use crate::state::{BufferId, PersistedBufferHistory, WorkspaceSessionSnapshot};

/// Error contract for the storage I/O port.
#[derive(Debug, Error)]
pub enum StorageIoError {
	#[error("storage request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

/// Error contract for the file watcher port.
#[derive(Debug, Error)]
pub enum FileWatcherError {
	#[error("watch request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

/// Error contract for host-provided file picker integrations.
#[derive(Debug, Error)]
pub enum FilePickerError {
	#[error("file picker unavailable: {message}")]
	Unavailable { message: &'static str },
	#[error("file picker failed: {message}")]
	Failed { message: String },
}

/// Outbound port for subscribing/unsubscribing file change notifications.
pub trait FileWatcher {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherError>;
	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError>;
}

/// Host capability for showing a native file picker and returning one selected
/// path.
pub trait FilePicker {
	fn pick_open_path(&self) -> Result<Option<PathBuf>, FilePickerError>;
}

/// Char-offset edit operation for swap log replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapEditOp {
	Insert { pos: usize, text: String },
	Delete { pos: usize, len: usize },
}

/// Outbound port for async file load/save plus swap/undo lifecycle callbacks.
pub trait StorageIo {
	fn enqueue_load_workspace_session(&self) -> Result<(), StorageIoError> { Ok(()) }
	fn enqueue_save_workspace_session(
		&self,
		_snapshot: WorkspaceSessionSnapshot,
	) -> Result<(), StorageIoError> {
		Ok(())
	}
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError>;
	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), StorageIoError>;
	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError>;
	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError>;
	fn enqueue_detect_conflict(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError>;
	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), StorageIoError>;
	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError>;
	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), StorageIoError>;
	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), StorageIoError>;
	fn enqueue_load_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		expected_text: String,
		restore_view: bool,
	) -> Result<(), StorageIoError>;
	fn enqueue_save_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		history: PersistedBufferHistory,
	) -> Result<(), StorageIoError>;
	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), StorageIoError>;
}
