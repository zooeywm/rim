use std::{ops::ControlFlow, path::{Path, PathBuf}};

use tracing::error;

use super::{ActionHandlerError, RimState};
use crate::{action::{FileAction, KeyCode, KeyEvent, KeyModifiers, SwapConflictCheckResult, SwapConflictInfo}, ports::{FilePicker, FileWatcher, StorageIo}, state::{BufferId, PendingSwapDecision, PersistedBufferHistory}};

pub(super) fn enqueue_swap_recover<P>(
	ports: &P,
	buffer_id: BufferId,
	source_path: PathBuf,
	base_text: String,
) where
	P: StorageIo,
{
	if let Err(source) = ports.enqueue_recover(buffer_id, source_path, base_text) {
		let err = ActionHandlerError::PersistenceSwapRecover { source };
		error!("persistence worker unavailable while enqueueing swap recover: {}", err);
	}
}

pub(super) fn enqueue_history_load<P>(
	ports: &P,
	buffer_id: BufferId,
	source_path: PathBuf,
	expected_text: String,
	restore_view: bool,
) where
	P: StorageIo,
{
	if let Err(source) = ports.enqueue_load_history(buffer_id, source_path, expected_text, restore_view) {
		let err = ActionHandlerError::PersistenceHistoryLoad { source };
		error!("persistence worker unavailable while enqueueing history load: {}", err);
	}
}

pub(super) fn enqueue_history_save<P>(
	ports: &P,
	buffer_id: BufferId,
	source_path: PathBuf,
	history: PersistedBufferHistory,
) where
	P: StorageIo,
{
	if let Err(source) = ports.enqueue_save_history(buffer_id, source_path, history) {
		let err = ActionHandlerError::PersistenceHistorySave { source };
		error!("persistence worker unavailable while enqueueing history save: {}", err);
	}
}

pub(super) fn enqueue_history_load_for_buffer<P>(
	ports: &P,
	state: &RimState,
	buffer_id: BufferId,
	restore_view: bool,
) where
	P: StorageIo,
{
	let Some(buffer) = state.buffers.get(buffer_id) else {
		return;
	};
	let Some(source_path) = buffer.path.clone() else {
		return;
	};
	enqueue_history_load(ports, buffer_id, source_path, buffer.text.to_string(), restore_view);
}

pub(super) fn enqueue_history_save_for_buffer<P>(ports: &P, state: &RimState, buffer_id: BufferId)
where P: StorageIo {
	let Some(buffer) = state.buffers.get(buffer_id) else {
		return;
	};
	let Some(source_path) = buffer.path.clone() else {
		return;
	};
	let Some(history) = state.buffer_persisted_history_snapshot(buffer_id) else {
		return;
	};
	enqueue_history_save(ports, buffer_id, source_path, history);
}

fn enqueue_workspace_runtime_bindings<P>(ports: &P, state: &RimState)
where P: StorageIo + FileWatcher {
	for (buffer_id, buffer) in &state.buffers {
		let Some(source_path) = buffer.path.clone() else {
			continue;
		};
		if let Err(source) = ports.enqueue_open(buffer_id, source_path.clone()) {
			let err = ActionHandlerError::PersistenceOpen { source };
			error!("session restore enqueue_open failed: {}", err);
		}
		if let Err(source) = ports.enqueue_watch(buffer_id, source_path.clone()) {
			let err = ActionHandlerError::OpenFileWatch { source };
			error!("session restore enqueue_watch failed: {}", err);
		}
		if let Err(source) = ports.enqueue_detect_conflict(buffer_id, source_path) {
			let err = ActionHandlerError::PersistenceSwapDetectConflict { source };
			error!("session restore enqueue_detect_conflict failed: {}", err);
		}
		enqueue_history_load_for_buffer(ports, state, buffer_id, false);
	}
}

pub(super) fn swap_conflict_prompt_message(conflict: &SwapConflictInfo) -> String {
	format!(
		"swap exists (pid {}, user {}): [r]ecover [d]elete [e]dit anyway [a]bort",
		conflict.pid, conflict.username
	)
}

pub(super) fn handle_pending_swap_decision_key<P>(
	ports: &P,
	state: &mut RimState,
	key: KeyEvent,
) -> ControlFlow<()>
where
	P: StorageIo + FileWatcher + FilePicker,
{
	state.normal_sequence.clear();
	state.status_bar.key_sequence.clear();

	let Some(pending) = state.pending_swap_decision.as_ref() else {
		return ControlFlow::Continue(());
	};

	let is_plain_escape = key.code == KeyCode::Esc
		&& !key.modifiers.contains(KeyModifiers::CONTROL)
		&& !key.modifiers.contains(KeyModifiers::ALT);
	let selected = match (key.code, key.modifiers) {
		(KeyCode::Char(ch), mods)
			if !mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::ALT) =>
		{
			Some(ch.to_ascii_lowercase())
		}
		_ if is_plain_escape => Some('a'),
		_ => None,
	};

	let Some(selected) = selected else {
		return ControlFlow::Continue(());
	};
	if !matches!(selected, 'r' | 'd' | 'e' | 'a') {
		state.status_bar.message = swap_conflict_prompt_message(&SwapConflictInfo {
			pid:      pending.owner_pid,
			username: pending.owner_username.clone(),
		});
		return ControlFlow::Continue(());
	}

	let Some(pending) = state.take_pending_swap_decision() else {
		return ControlFlow::Continue(());
	};

	match selected {
		'r' => {
			enqueue_swap_recover(ports, pending.buffer_id, pending.source_path.clone(), pending.base_text);
			state.status_bar.message = "recovering from swap...".to_string();
		}
		'd' => {
			if let Err(source) =
				ports.enqueue_initialize_base(pending.buffer_id, pending.source_path, pending.base_text, true)
			{
				let err = ActionHandlerError::PersistenceSwapInitializeBase { source };
				error!("persistence worker unavailable while enqueueing base init: {}", err);
				state.status_bar.message = "swap delete failed: swap worker unavailable".to_string();
			} else {
				state.status_bar.message = "swap deleted".to_string();
			}
		}
		'e' => {
			if let Err(source) =
				ports.enqueue_initialize_base(pending.buffer_id, pending.source_path, pending.base_text, false)
			{
				let err = ActionHandlerError::PersistenceSwapInitializeBase { source };
				error!("persistence worker unavailable while enqueueing base init: {}", err);
				state.status_bar.message = "swap ignore failed: swap worker unavailable".to_string();
			} else {
				state.status_bar.message = "editing without swap recovery".to_string();
			}
		}
		'a' => {
			state.close_buffer(pending.buffer_id);
			if let Err(source) = ports.enqueue_unwatch(pending.buffer_id) {
				let err = ActionHandlerError::CloseBufferUnwatch { source };
				error!("watch worker unavailable while enqueueing file unwatch: {}", err);
			}
			if let Err(source) = ports.enqueue_close(pending.buffer_id) {
				let err = ActionHandlerError::PersistenceSwapClose { source };
				error!("persistence worker unavailable while enqueueing swap close: {}", err);
			}
			state.status_bar.message = format!("open aborted: {}", pending.source_path.display());
		}
		_ => {}
	}

	ControlFlow::Continue(())
}

pub(super) fn handle_file_action<P>(ports: &P, state: &mut RimState, action: FileAction) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	match action {
		FileAction::SwapConflictDetected { buffer_id, result } => match result {
			Ok(SwapConflictCheckResult::Conflict(conflict)) => {
				let Some((source_path, base_text)) = state
					.buffers
					.get(buffer_id)
					.and_then(|buffer| buffer.path.clone().map(|path| (path, buffer.text.to_string())))
				else {
					error!("swap conflict detected for unknown buffer path: buffer_id={:?}", buffer_id);
					return ControlFlow::Continue(());
				};
				state.set_pending_swap_decision(PendingSwapDecision {
					buffer_id,
					source_path,
					base_text,
					owner_pid: conflict.pid,
					owner_username: conflict.username.clone(),
				});
				state.normal_sequence.clear();
				state.status_bar.key_sequence.clear();
				state.status_bar.message = swap_conflict_prompt_message(&conflict);
			}
			Ok(SwapConflictCheckResult::NoSwapActionNeeded) => {
				let Some((source_path, base_text)) = state
					.buffers
					.get(buffer_id)
					.and_then(|buffer| buffer.path.clone().map(|path| (path, buffer.text.to_string())))
				else {
					error!("swap conflict check returned for unknown buffer path: buffer_id={:?}", buffer_id);
					return ControlFlow::Continue(());
				};
				if let Err(source) = ports.enqueue_initialize_base(buffer_id, source_path, base_text, false) {
					let err = ActionHandlerError::PersistenceSwapInitializeBase { source };
					error!("persistence worker unavailable while enqueueing base init: {}", err);
				}
			}
			Err(err) => {
				error!("swap conflict check failed: buffer_id={:?}, error={}", buffer_id, err);
				state.status_bar.message = "swap check failed".to_string();
			}
		},
		FileAction::SwapRecoverCompleted { buffer_id, result } => match result {
			Ok(Some(recovered_text)) => {
				state.replace_buffer_text_preserving_cursor(buffer_id, recovered_text);
				state.clear_buffer_history(buffer_id);
				state.refresh_buffer_dirty(buffer_id);
				state.set_buffer_externally_modified(buffer_id, false);
				enqueue_history_load_for_buffer(ports, state, buffer_id, true);
				state.status_bar.message = "swap recovered: unsaved edits restored".to_string();
			}
			Ok(None) => {
				state.set_buffer_externally_modified(buffer_id, false);
				state.status_bar.message = "file reloaded".to_string();
			}
			Err(err) => {
				error!("swap recover failed: buffer_id={:?}, error={}", buffer_id, err);
			}
		},
		FileAction::UndoHistoryLoaded { buffer_id, source_path, expected_text, restore_view, result } => {
			let Some(is_still_current) = state
				.buffers
				.get(buffer_id)
				.map(|buffer| buffer.path.as_ref() == Some(&source_path) && buffer.text == expected_text.as_str())
			else {
				return ControlFlow::Continue(());
			};
			if !is_still_current {
				return ControlFlow::Continue(());
			}

			match result {
				Ok(Some(history)) => {
					if !state.restore_buffer_persisted_history(buffer_id, history, restore_view) {
						error!("restore persisted history failed: buffer_id={:?}", buffer_id);
					}
				}
				Ok(None) => {}
				Err(err) => {
					error!("history load failed: buffer_id={:?}, error={}", buffer_id, err);
				}
			}
		}
		FileAction::WorkspaceSessionLoaded { result } => match result {
			Ok(Some(snapshot)) => {
				if state.restore_workspace_session(snapshot) {
					enqueue_workspace_runtime_bindings(ports, state);
				} else {
					state.create_untitled_buffer();
					state.status_bar.message = "session restore failed".to_string();
				}
			}
			Ok(None) => {
				state.create_untitled_buffer();
				state.status_bar.message = "new file".to_string();
			}
			Err(err) => {
				error!("workspace session load failed: {}", err);
				state.create_untitled_buffer();
				state.status_bar.message = format!("session load failed: {}", err);
			}
		},
		FileAction::WorkspaceFilesListed { workspace_root, result } => match result {
			Ok(paths) => {
				let entries = paths
					.into_iter()
					.filter_map(|path| {
						let relative_path = path
							.strip_prefix(workspace_root.as_path())
							.ok()
							.map(|relative| relative.to_string_lossy().replace('\\', "/"))?;
						Some(crate::state::WorkspaceFileEntry { absolute_path: path, relative_path })
					})
					.collect::<Vec<_>>();
				state.set_workspace_file_cache(entries.clone());
				if state.command_palette_showing_files()
					&& let Some(path) = state.selected_command_palette_file_path().map(PathBuf::from)
				{
					state.set_command_palette_preview_loading(path.as_path());
					if let Err(source) = ports.enqueue_load_workspace_file_preview(path.clone()) {
						let err = ActionHandlerError::Reload { source };
						error!("command palette preview enqueue failed: path={} error={}", path.display(), err);
						state.set_command_palette_preview(path.as_path(), format!("<preview error: {}>", err));
					}
				}
				if !state.workspace_file_picker_open() {
					return ControlFlow::Continue(());
				}
				if entries.is_empty() {
					state.close_workspace_file_picker();
					state.status_bar.message = "workspace file picker: no files found".to_string();
					return ControlFlow::Continue(());
				}
				state.replace_workspace_file_picker_entries(entries);
				if let Some(path) = state.selected_workspace_file_picker_path().map(PathBuf::from) {
					state.set_workspace_file_picker_preview_loading(path.as_path());
					if let Err(source) = ports.enqueue_load_workspace_file_preview(path.clone()) {
						let err = ActionHandlerError::Reload { source };
						error!("workspace preview enqueue failed: path={} error={}", path.display(), err);
						state.set_workspace_file_picker_preview(path.as_path(), format!("<preview error: {}>", err));
					}
				}
			}
			Err(err) => {
				error!("workspace file picker list failed: {}", err);
				state.fail_workspace_file_cache_loading();
				state.close_workspace_file_picker();
				state.status_bar.message = format!("workspace file picker failed: {}", err);
			}
		},
		FileAction::WorkspaceFilesChanged { workspace_root } => {
			if !state.has_workspace_file_cache()
				&& !state.workspace_file_picker_open()
				&& !state.command_palette().is_some_and(|palette| palette.showing_files)
			{
				return ControlFlow::Continue(());
			}
			state.begin_workspace_file_cache_loading();
			if state.workspace_file_picker_open() {
				state.open_workspace_file_picker_loading();
			}
			if let Err(source) = ports.enqueue_list_workspace_files(workspace_root) {
				let err = ActionHandlerError::Reload { source };
				error!("workspace file relist enqueue failed: {}", err);
				state.fail_workspace_file_cache_loading();
				if state.workspace_file_picker_open() {
					state.close_workspace_file_picker();
				}
				state.status_bar.message = format!("workspace file relist failed: {}", err);
			}
		}
		FileAction::WorkspaceFilePreviewLoaded { path, result } => match result {
			Ok(preview) => {
				state.set_workspace_file_picker_preview(path.as_path(), preview.clone());
				state.set_command_palette_preview(path.as_path(), preview);
			}
			Err(err) => {
				error!("workspace preview load failed: path={} error={}", path.display(), err);
				let error_message = format!("<preview error: {}>", err);
				state.set_workspace_file_picker_preview(path.as_path(), error_message.clone());
				state.set_command_palette_preview(path.as_path(), error_message);
			}
		},
		FileAction::LoadCompleted { buffer_id, source, result } => match (source, result) {
			(crate::action::FileLoadSource::Open, Ok(text)) => {
				if let Some(buffer) = state.buffers.get_mut(buffer_id) {
					buffer.text = text.into();
				} else {
					error!("load completed for unknown buffer: buffer_id={:?}", buffer_id);
				}
				state.clear_buffer_history(buffer_id);
				state.mark_buffer_clean(buffer_id);
				state.set_buffer_externally_modified(buffer_id, false);
				enqueue_history_load_for_buffer(ports, state, buffer_id, true);
				state.status_bar.message = "file loaded".to_string();
				if let Some(source_path) = state.buffers.get(buffer_id).and_then(|buffer| buffer.path.clone())
					&& let Err(source) = ports.enqueue_detect_conflict(buffer_id, source_path)
				{
					let err = ActionHandlerError::PersistenceSwapDetectConflict { source };
					error!("persistence worker unavailable while enqueueing swap conflict check: {}", err);
				}
			}
			(crate::action::FileLoadSource::Open, Err(err)) => {
				error!("file load failed: buffer_id={:?}, error={}", buffer_id, err);
				state.status_bar.message = format!("load failed: {}", err);
			}
			(crate::action::FileLoadSource::External, Ok(text)) => {
				let is_active = state.active_buffer_id() == Some(buffer_id);
				let Some((is_dirty, name)) =
					state.buffers.get(buffer_id).map(|buffer| (buffer.dirty, buffer.name.clone()))
				else {
					error!("external changed for unknown buffer: buffer_id={:?}", buffer_id);
					return ControlFlow::Continue(());
				};
				if is_dirty {
					state.set_buffer_externally_modified(buffer_id, true);
					if is_active {
						state.status_bar.message =
							"file changed externally; use :w! to overwrite or :e! to reload".to_string();
					}
					return ControlFlow::Continue(());
				}
				state.replace_buffer_text_preserving_cursor(buffer_id, text);
				state.clear_buffer_history(buffer_id);
				state.mark_buffer_clean(buffer_id);
				state.set_buffer_externally_modified(buffer_id, false);
				enqueue_history_load_for_buffer(ports, state, buffer_id, false);
				if is_active {
					state.status_bar.message = format!("reloaded {}", name);
				}
			}
			(crate::action::FileLoadSource::External, Err(err)) => {
				error!("external change reload failed: buffer_id={:?}, error={}", buffer_id, err);
			}
		},
		FileAction::OpenRequested { path } => {
			tracing::info!("open_file: {}", path.display());
			let normalized_path = normalize_file_path(state.workspace_root(), path.as_path());
			let replaceable_untitled = state.replaceable_active_untitled_buffer_id();
			if let Some(buffer_id) = state.find_buffer_by_path(normalized_path.as_path()) {
				if let Some(untitled_buffer_id) = replaceable_untitled
					&& untitled_buffer_id != buffer_id
				{
					state.bind_buffer_to_active_window(buffer_id);
					state.detach_buffer_from_active_tab_and_try_remove(untitled_buffer_id);
				} else {
					state.bind_buffer_to_active_window(buffer_id);
				}
				state.status_bar.message = format!("switched {}", path.display());
				return ControlFlow::Continue(());
			}
			if !normalized_path.exists() {
				let buffer_id = if let Some(untitled_buffer_id) = replaceable_untitled {
					state.prepare_buffer_for_open(untitled_buffer_id, normalized_path.clone());
					untitled_buffer_id
				} else {
					let buffer_id = state.create_buffer(Some(normalized_path.clone()), String::new());
					state.bind_buffer_to_active_window(buffer_id);
					buffer_id
				};
				state.bind_buffer_to_active_window(buffer_id);
				state.status_bar.message = format!("new {}", path.display());
				return ControlFlow::Continue(());
			}
			let buffer_id = if let Some(untitled_buffer_id) = replaceable_untitled {
				state.prepare_buffer_for_open(untitled_buffer_id, normalized_path.clone());
				untitled_buffer_id
			} else {
				let buffer_id = state.create_buffer(Some(normalized_path.clone()), String::new());
				state.bind_buffer_to_active_window(buffer_id);
				buffer_id
			};
			if let Err(source) = ports.enqueue_open(buffer_id, normalized_path.clone()) {
				let err = ActionHandlerError::PersistenceOpen { source };
				error!("persistence worker unavailable while enqueueing swap open: {}", err);
			}
			if let Err(source) = ports.enqueue_watch(buffer_id, normalized_path.clone()) {
				let err = ActionHandlerError::OpenFileWatch { source };
				error!("watch worker unavailable while enqueueing file watch: {}", err);
			}
			if let Err(source) = ports.enqueue_load(buffer_id, normalized_path.clone()) {
				let io_err = ActionHandlerError::OpenFileLoad { source };
				error!("io worker unavailable while enqueueing file load: {}", io_err);
				state.status_bar.message = "load failed: io worker unavailable".to_string();
			} else {
				state.status_bar.message = format!("loading {}", path.display());
			}
		}
		FileAction::ExternalChangeDetected { buffer_id, path } => {
			if state.in_flight_internal_saves.contains(&buffer_id) {
				return ControlFlow::Continue(());
			}
			if state.should_ignore_recent_external_change(buffer_id) {
				state.set_buffer_externally_modified(buffer_id, false);
				return ControlFlow::Continue(());
			}
			let Some(buffer) = state.buffers.get(buffer_id) else {
				error!("external change detected for unknown buffer: buffer_id={:?}", buffer_id);
				return ControlFlow::Continue(());
			};
			if buffer.dirty {
				state.set_buffer_externally_modified(buffer_id, true);
				if state.active_buffer_id() == Some(buffer_id) {
					state.status_bar.message =
						"file changed externally; use :w! to overwrite or :e! to reload".to_string();
				}
				return ControlFlow::Continue(());
			}
			if let Err(source) = ports.enqueue_external_load(buffer_id, path) {
				let err = ActionHandlerError::ExternalReload { source };
				error!("io worker unavailable while enqueueing external reload: {}", err);
				if state.active_buffer_id() == Some(buffer_id) {
					state.status_bar.message = "reload failed: io worker unavailable".to_string();
				}
				return ControlFlow::Continue(());
			}
		}
		FileAction::SaveCompleted { buffer_id, result } => match result {
			Ok(()) => {
				state.in_flight_internal_saves.remove(&buffer_id);
				state.mark_recent_internal_save(buffer_id);
				state.apply_pending_save_path_if_matches(buffer_id);
				if let Some(path) = state.buffers.get(buffer_id).and_then(|buffer| buffer.path.clone()) {
					if let Err(source) = ports.enqueue_watch(buffer_id, path.clone()) {
						let err = ActionHandlerError::SaveWatch { source };
						error!("watch worker unavailable while enqueueing file watch: {}", err);
					}
					if let Err(source) = ports.enqueue_mark_clean(buffer_id, path) {
						let err = ActionHandlerError::PersistenceSwapMarkClean { source };
						error!("persistence worker unavailable while enqueueing swap mark clean: {}", err);
					}
				}
				state.mark_buffer_clean(buffer_id);
				state.set_buffer_externally_modified(buffer_id, false);
				enqueue_history_save_for_buffer(ports, state, buffer_id);
				state.status_bar.message = "file saved".to_string();
				if state.quit_after_save && state.in_flight_internal_saves.is_empty() {
					state.quit_after_save = false;
					return RimState::dispatch_internal(
						ports,
						state,
						crate::action::AppAction::System(crate::action::SystemAction::Quit),
					);
				}
			}
			Err(err) => {
				state.in_flight_internal_saves.remove(&buffer_id);
				state.clear_recent_internal_save(buffer_id);
				state.quit_after_save = false;
				state.clear_pending_save_path_if_matches(buffer_id);
				error!("file save failed: buffer_id={:?} error={}", buffer_id, err);
				state.status_bar.message = format!("save failed: {}", err);
			}
		},
	}

	ControlFlow::Continue(())
}

fn normalize_file_path(workspace_root: &Path, path: &Path) -> PathBuf {
	let absolute = if path.is_absolute() { path.to_path_buf() } else { workspace_root.join(path) };
	std::fs::canonicalize(&absolute).unwrap_or(absolute)
}
