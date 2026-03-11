use std::{cell::RefCell, ops::ControlFlow, path::{Path, PathBuf}};

use super::super::mode_flow::{SequenceMatch, resolve_normal_sequence_with_registry};
use crate::{action::{AppAction, KeyEvent}, command::CommandRegistry, ports::{FilePicker, FilePickerError, FileWatcher, FileWatcherError, StorageIo, StorageIoError, SwapEditOp}, state::{BufferId, NormalSequenceKey, PersistedBufferHistory, RimState, WorkspaceSessionSnapshot}};

pub(super) struct TestPorts;

impl FileWatcher for TestPorts {
	fn enqueue_watch(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileWatcherError> { Ok(()) }

	fn enqueue_unwatch(&self, _buffer_id: BufferId) -> Result<(), FileWatcherError> { Ok(()) }
}

impl FilePicker for TestPorts {
	fn pick_open_path(&self) -> Result<Option<PathBuf>, FilePickerError> { Ok(None) }
}

impl StorageIo for TestPorts {
	fn enqueue_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), StorageIoError> { Ok(()) }

	fn enqueue_save(&self, _buffer_id: BufferId, _path: PathBuf, _text: String) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_external_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_open(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), StorageIoError> { Ok(()) }

	fn enqueue_detect_conflict(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_edit(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_op: SwapEditOp,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_mark_clean(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_initialize_base(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_base_text: String,
		_delete_existing: bool,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_recover(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_base_text: String,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_load_history(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_expected_text: String,
		_restore_view: bool,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_save_history(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_history: PersistedBufferHistory,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_close(&self, _buffer_id: BufferId) -> Result<(), StorageIoError> { Ok(()) }
}

pub(super) fn dispatch_test_action(state: &mut RimState, action: AppAction) -> ControlFlow<()> {
	let ports = TestPorts;
	state.apply_action(&ports, action)
}

#[derive(Default)]
pub(super) struct RecordingPorts {
	pub(super) file_loads:       RefCell<Vec<(BufferId, PathBuf)>>,
	pub(super) external_loads:   RefCell<Vec<(BufferId, PathBuf)>>,
	pub(super) swap_edits:       RefCell<Vec<(BufferId, PathBuf, SwapEditOp)>>,
	pub(super) history_loads:    RefCell<Vec<(BufferId, PathBuf, String, bool)>>,
	pub(super) history_saves:    RefCell<Vec<(BufferId, PathBuf, PersistedBufferHistory)>>,
	pub(super) unwatches:        RefCell<Vec<BufferId>>,
	pub(super) closes:           RefCell<Vec<BufferId>>,
	pub(super) open_requests:    RefCell<Vec<(BufferId, PathBuf)>>,
	pub(super) watch_requests:   RefCell<Vec<(BufferId, PathBuf)>>,
	pub(super) initialize_bases: RefCell<Vec<(BufferId, PathBuf, String, bool)>>,
	pub(super) session_saves:    RefCell<Vec<WorkspaceSessionSnapshot>>,
}

impl FileWatcher for RecordingPorts {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherError> {
		self.watch_requests.borrow_mut().push((buffer_id, path));
		Ok(())
	}

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError> {
		self.unwatches.borrow_mut().push(buffer_id);
		Ok(())
	}
}

impl FilePicker for RecordingPorts {
	fn pick_open_path(&self) -> Result<Option<PathBuf>, FilePickerError> { Ok(None) }
}

impl StorageIo for RecordingPorts {
	fn enqueue_save_workspace_session(&self, snapshot: WorkspaceSessionSnapshot) -> Result<(), StorageIoError> {
		self.session_saves.borrow_mut().push(snapshot);
		Ok(())
	}

	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError> {
		self.file_loads.borrow_mut().push((buffer_id, path));
		Ok(())
	}

	fn enqueue_save(&self, _buffer_id: BufferId, _path: PathBuf, _text: String) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError> {
		self.external_loads.borrow_mut().push((buffer_id, path));
		Ok(())
	}

	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError> {
		self.open_requests.borrow_mut().push((buffer_id, source_path));
		Ok(())
	}

	fn enqueue_detect_conflict(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), StorageIoError> {
		self.swap_edits.borrow_mut().push((buffer_id, source_path, op));
		Ok(())
	}

	fn enqueue_mark_clean(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), StorageIoError> {
		self.initialize_bases.borrow_mut().push((buffer_id, source_path, base_text, delete_existing));
		Ok(())
	}

	fn enqueue_recover(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_base_text: String,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_load_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		expected_text: String,
		restore_view: bool,
	) -> Result<(), StorageIoError> {
		self.history_loads.borrow_mut().push((buffer_id, source_path, expected_text, restore_view));
		Ok(())
	}

	fn enqueue_save_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		history: PersistedBufferHistory,
	) -> Result<(), StorageIoError> {
		self.history_saves.borrow_mut().push((buffer_id, source_path, history));
		Ok(())
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), StorageIoError> {
		self.closes.borrow_mut().push(buffer_id);
		Ok(())
	}
}

#[derive(Default)]
pub(super) struct SwapDecisionPorts {
	pub(super) swap_conflict_detects: RefCell<Vec<(BufferId, PathBuf)>>,
	pub(super) swap_recovers:         RefCell<Vec<(BufferId, PathBuf, String)>>,
	pub(super) swap_inits:            RefCell<Vec<(BufferId, PathBuf, String, bool)>>,
	pub(super) unwatches:             RefCell<Vec<BufferId>>,
	pub(super) swap_closes:           RefCell<Vec<BufferId>>,
}

impl FileWatcher for SwapDecisionPorts {
	fn enqueue_watch(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileWatcherError> { Ok(()) }

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError> {
		self.unwatches.borrow_mut().push(buffer_id);
		Ok(())
	}
}

impl FilePicker for SwapDecisionPorts {
	fn pick_open_path(&self) -> Result<Option<PathBuf>, FilePickerError> { Ok(None) }
}

#[derive(Default)]
pub(super) struct FilePickerPorts {
	pub(super) picked_path: RefCell<Option<PathBuf>>,
	pub(super) file_loads:  RefCell<Vec<(BufferId, PathBuf)>>,
}

impl FileWatcher for FilePickerPorts {
	fn enqueue_watch(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileWatcherError> { Ok(()) }

	fn enqueue_unwatch(&self, _buffer_id: BufferId) -> Result<(), FileWatcherError> { Ok(()) }
}

impl FilePicker for FilePickerPorts {
	fn pick_open_path(&self) -> Result<Option<PathBuf>, FilePickerError> {
		Ok(self.picked_path.borrow().clone())
	}
}

impl StorageIo for FilePickerPorts {
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), StorageIoError> {
		self.file_loads.borrow_mut().push((buffer_id, path));
		Ok(())
	}

	fn enqueue_save(&self, _buffer_id: BufferId, _path: PathBuf, _text: String) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_external_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_open(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), StorageIoError> { Ok(()) }

	fn enqueue_detect_conflict(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_edit(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_op: SwapEditOp,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_mark_clean(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_initialize_base(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_base_text: String,
		_delete_existing: bool,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_recover(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_base_text: String,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_load_history(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_expected_text: String,
		_restore_view: bool,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_save_history(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_history: PersistedBufferHistory,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_close(&self, _buffer_id: BufferId) -> Result<(), StorageIoError> { Ok(()) }
}

impl StorageIo for SwapDecisionPorts {
	fn enqueue_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), StorageIoError> { Ok(()) }

	fn enqueue_save(&self, _buffer_id: BufferId, _path: PathBuf, _text: String) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_external_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_open(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), StorageIoError> { Ok(()) }

	fn enqueue_detect_conflict(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), StorageIoError> {
		self.swap_conflict_detects.borrow_mut().push((buffer_id, source_path));
		Ok(())
	}

	fn enqueue_edit(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_op: SwapEditOp,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_mark_clean(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), StorageIoError> {
		self.swap_inits.borrow_mut().push((buffer_id, source_path, base_text, delete_existing));
		Ok(())
	}

	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), StorageIoError> {
		self.swap_recovers.borrow_mut().push((buffer_id, source_path, base_text));
		Ok(())
	}

	fn enqueue_load_history(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_expected_text: String,
		_restore_view: bool,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_save_history(
		&self,
		_buffer_id: BufferId,
		_source_path: PathBuf,
		_history: PersistedBufferHistory,
	) -> Result<(), StorageIoError> {
		Ok(())
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), StorageIoError> {
		self.swap_closes.borrow_mut().push(buffer_id);
		Ok(())
	}
}

pub(super) fn normalize_test_path(path: &str) -> PathBuf {
	let path = Path::new(path);
	let absolute = if path.is_absolute() {
		path.to_path_buf()
	} else {
		std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
	};
	std::fs::canonicalize(&absolute).unwrap_or(absolute)
}

pub(super) fn map_normal_key(state: &RimState, key: KeyEvent) -> Option<NormalSequenceKey> {
	RimState::to_normal_key(state, key)
}

pub(super) fn resolve_keys(keys: &[NormalSequenceKey]) -> SequenceMatch {
	resolve_normal_sequence_with_registry(&CommandRegistry::with_defaults(), keys)
}
