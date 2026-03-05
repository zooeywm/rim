use std::path::PathBuf;

use thiserror::Error;

use crate::state::BufferId;

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

/// Error contract for the swap log I/O port.
#[derive(Debug, Error)]
pub enum SwapIoError {
	#[error("swap request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

/// Outbound port for async swap log lifecycle and recovery callbacks.
pub trait SwapIo {
	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError>;
	fn enqueue_detect_conflict(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError>;
	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), SwapIoError>;
	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError>;
	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), SwapIoError>;
	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), SwapIoError>;
	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), SwapIoError>;
}
