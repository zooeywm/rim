use std::path::PathBuf;

use thiserror::Error;

use crate::state::{BufferId, PersistedBufferHistory};

/// Error contract for the file I/O port.
#[derive(Debug, Error)]
pub enum FileIoError {
	#[error("io request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

/// Outbound port for asynchronous file read/write requests.
pub trait FileIo {
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError>;
	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), FileIoError>;
	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError>;
}

/// Error contract for the file watcher port.
#[derive(Debug, Error)]
pub enum FileWatcherError {
	#[error("watch request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

/// Outbound port for subscribing/unsubscribing file change notifications.
pub trait FileWatcher {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherError>;
	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError>;
}

/// Char-offset edit operation for swap log replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapEditOp {
	Insert { pos: usize, text: String },
	Delete { pos: usize, len: usize },
}

/// Error contract for the persistence I/O port.
#[derive(Debug, Error)]
pub enum PersistenceIoError {
	#[error("persistence request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

/// Outbound port for async swap/undofile lifecycle and recovery callbacks.
pub trait PersistenceIo {
	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), PersistenceIoError>;
	fn enqueue_detect_conflict(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
	) -> Result<(), PersistenceIoError>;
	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), PersistenceIoError>;
	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), PersistenceIoError>;
	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), PersistenceIoError>;
	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), PersistenceIoError>;
	fn enqueue_load_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		expected_text: String,
	) -> Result<(), PersistenceIoError>;
	fn enqueue_save_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		history: PersistedBufferHistory,
	) -> Result<(), PersistenceIoError>;
	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), PersistenceIoError>;
}
