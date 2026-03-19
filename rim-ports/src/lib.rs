use std::path::PathBuf;

use thiserror::Error;

mod plugin;

pub use plugin::{PluginAction, PluginCapability, PluginCommandError, PluginCommandMetadata, PluginCommandParamKind, PluginCommandParamSpec, PluginCommandRequest, PluginCommandResponse, PluginDiscoveryResult, PluginEffect, PluginInvocationError, PluginLoadFailure, PluginMetadata, PluginNotification, PluginNotificationLevel, PluginPanel, PluginRegistration, PluginResolvedParam, PluginRuntime, PluginRuntimeError, PluginRuntimeFailure};

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
	type BufferId: Copy;

	fn enqueue_watch(&self, buffer_id: Self::BufferId, path: PathBuf) -> Result<(), FileWatcherError>;
	fn enqueue_unwatch(&self, buffer_id: Self::BufferId) -> Result<(), FileWatcherError>;
	fn enqueue_watch_workspace_root(&self, _path: PathBuf) -> Result<(), FileWatcherError> { Ok(()) }
}

/// Host capability for showing a native file picker and returning one selected
/// path.
pub trait FilePicker {
	fn pick_open_path(
		&self,
		command: &[String],
		chooser_file_arg_index: usize,
	) -> Result<Option<PathBuf>, FilePickerError>;
}

/// Outbound port for async file load/save plus swap/undo lifecycle callbacks.
pub trait StorageIo {
	type BufferId: Copy;
	type PersistedBufferHistory;
	type WorkspaceSessionSnapshot;
	type EditOp;

	fn enqueue_load_workspace_session(&self) -> Result<(), StorageIoError> { Ok(()) }
	fn enqueue_save_workspace_session(
		&self,
		_snapshot: Self::WorkspaceSessionSnapshot,
	) -> Result<(), StorageIoError> {
		Ok(())
	}
	fn enqueue_load(&self, buffer_id: Self::BufferId, path: PathBuf) -> Result<(), StorageIoError>;
	fn enqueue_list_workspace_files(&self, _workspace_root: PathBuf) -> Result<(), StorageIoError> { Ok(()) }
	fn enqueue_load_workspace_file_preview(&self, _path: PathBuf) -> Result<(), StorageIoError> { Ok(()) }
	fn enqueue_save(
		&self,
		buffer_id: Self::BufferId,
		path: PathBuf,
		text: String,
	) -> Result<(), StorageIoError>;
	fn enqueue_external_load(&self, buffer_id: Self::BufferId, path: PathBuf) -> Result<(), StorageIoError>;
	fn enqueue_open(&self, buffer_id: Self::BufferId, source_path: PathBuf) -> Result<(), StorageIoError>;
	fn enqueue_detect_conflict(
		&self,
		buffer_id: Self::BufferId,
		source_path: PathBuf,
	) -> Result<(), StorageIoError>;
	fn enqueue_edit(
		&self,
		buffer_id: Self::BufferId,
		source_path: PathBuf,
		op: Self::EditOp,
	) -> Result<(), StorageIoError>;
	fn enqueue_mark_clean(&self, buffer_id: Self::BufferId, source_path: PathBuf)
	-> Result<(), StorageIoError>;
	fn enqueue_initialize_base(
		&self,
		buffer_id: Self::BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), StorageIoError>;
	fn enqueue_recover(
		&self,
		buffer_id: Self::BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), StorageIoError>;
	fn enqueue_load_history(
		&self,
		buffer_id: Self::BufferId,
		source_path: PathBuf,
		expected_text: String,
		restore_view: bool,
	) -> Result<(), StorageIoError>;
	fn enqueue_save_history(
		&self,
		buffer_id: Self::BufferId,
		source_path: PathBuf,
		history: Self::PersistedBufferHistory,
	) -> Result<(), StorageIoError>;
	fn enqueue_close(&self, buffer_id: Self::BufferId) -> Result<(), StorageIoError>;
}
