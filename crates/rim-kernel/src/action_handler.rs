use std::{hash::{Hash, Hasher}, ops::ControlFlow, path::{Path, PathBuf}};

use thiserror::Error;
use tracing::{error, info};

use crate::{action::{AppAction, BufferAction, EditorAction, FileAction, FileLoadSource, KeyCode, KeyEvent, KeyModifiers, LayoutAction, SwapConflictInfo, SystemAction, TabAction, WindowAction}, ports::{FileIo, FileIoError, FileWatcher, FileWatcherError, SwapEditOp, SwapIo, SwapIoError}, state::{BufferId, BufferSwitchDirection, EditorMode, FocusDirection, NormalSequenceKey, PendingSwapDecision, RimState, SplitAxis}};

type NormalKey = NormalSequenceKey;

#[derive(Debug)]
enum SequenceMatch {
	Action(AppAction),
	Pending,
	NoMatch,
}

#[derive(Debug, Clone)]
struct BufferTextSnapshot {
	buffer_id: BufferId,
	text:      String,
	cursor:    crate::state::CursorState,
}

#[derive(Debug, Error)]
enum ActionHandlerError {
	#[error("enqueue watch for opened file failed")]
	OpenFileWatch {
		#[source]
		source: FileWatcherError,
	},
	#[error("enqueue initial file load failed")]
	OpenFileLoad {
		#[source]
		source: FileIoError,
	},
	#[error("enqueue external reload failed")]
	ExternalReload {
		#[source]
		source: FileIoError,
	},
	#[error("enqueue watch after save failed")]
	SaveWatch {
		#[source]
		source: FileWatcherError,
	},
	#[error("enqueue unwatch for closed buffer failed")]
	CloseBufferUnwatch {
		#[source]
		source: FileWatcherError,
	},
	#[error("enqueue file save failed")]
	Save {
		#[source]
		source: FileIoError,
	},
	#[error("enqueue file reload failed")]
	Reload {
		#[source]
		source: FileIoError,
	},
	#[error("enqueue file save for :wa failed")]
	SaveAll {
		#[source]
		source: FileIoError,
	},
	#[error("enqueue swap open failed")]
	SwapOpen {
		#[source]
		source: SwapIoError,
	},
	#[error("enqueue swap edit failed")]
	SwapEdit {
		#[source]
		source: SwapIoError,
	},
	#[error("enqueue swap mark clean failed")]
	SwapMarkClean {
		#[source]
		source: SwapIoError,
	},
	#[error("enqueue swap recover failed")]
	SwapRecover {
		#[source]
		source: SwapIoError,
	},
	#[error("enqueue swap conflict detect failed")]
	SwapDetectConflict {
		#[source]
		source: SwapIoError,
	},
	#[error("enqueue swap base initialization failed")]
	SwapInitializeBase {
		#[source]
		source: SwapIoError,
	},
	#[error("enqueue swap close failed")]
	SwapClose {
		#[source]
		source: SwapIoError,
	},
}

impl RimState {
	pub fn apply_action<P>(&mut self, ports: &P, action: AppAction) -> ControlFlow<()>
	where P: FileIo + FileWatcher + SwapIo {
		Self::dispatch_internal(ports, self, action)
	}
}

impl RimState {
	fn text_fingerprint(text: &str) -> u64 {
		let mut hasher = std::collections::hash_map::DefaultHasher::new();
		text.hash(&mut hasher);
		hasher.finish()
	}

	fn mark_internal_save(state: &mut RimState, buffer_id: BufferId, text: &str) {
		state.last_internal_save_fingerprint.insert(buffer_id, Self::text_fingerprint(text));
	}

	fn should_ignore_external_reload(state: &mut RimState, buffer_id: BufferId, text: &str) -> bool {
		let incoming = Self::text_fingerprint(text);
		let Some(expected) = state.last_internal_save_fingerprint.get(&buffer_id).copied() else {
			return false;
		};
		if expected == incoming {
			state.last_internal_save_fingerprint.remove(&buffer_id);
			return true;
		}
		state.last_internal_save_fingerprint.remove(&buffer_id);
		false
	}

	fn capture_active_buffer_text_snapshot(state: &RimState) -> Option<BufferTextSnapshot> {
		let buffer_id = state.active_buffer_id()?;
		let buffer = state.buffers.get(buffer_id)?;
		Some(BufferTextSnapshot { buffer_id, text: buffer.text.clone(), cursor: buffer.cursor })
	}

	fn enqueue_swap_ops<P>(ports: &P, state: &RimState, buffer_id: BufferId, ops: Vec<SwapEditOp>)
	where P: SwapIo {
		if ops.is_empty() {
			return;
		}
		let Some(source_path) = state.buffers.get(buffer_id).and_then(|buffer| buffer.path.clone()) else {
			return;
		};
		for op in ops {
			if let Err(source) = ports.enqueue_edit(buffer_id, source_path.clone(), op) {
				let err = ActionHandlerError::SwapEdit { source };
				error!("swap worker unavailable while enqueueing swap edit: {}", err);
				break;
			}
		}
	}

	fn swap_ops_from_text_diff(before: &str, after: &str) -> Vec<SwapEditOp> {
		if before == after {
			return Vec::new();
		}

		let mut common_prefix_bytes = 0usize;
		for (before_ch, after_ch) in before.chars().zip(after.chars()) {
			if before_ch != after_ch {
				break;
			}
			common_prefix_bytes = common_prefix_bytes.saturating_add(before_ch.len_utf8());
		}

		let before_tail = &before[common_prefix_bytes..];
		let after_tail = &after[common_prefix_bytes..];
		let mut common_suffix_bytes = 0usize;
		for (before_ch, after_ch) in before_tail.chars().rev().zip(after_tail.chars().rev()) {
			if before_ch != after_ch {
				break;
			}
			common_suffix_bytes = common_suffix_bytes.saturating_add(before_ch.len_utf8());
		}

		let before_mid_end = before.len().saturating_sub(common_suffix_bytes);
		let after_mid_end = after.len().saturating_sub(common_suffix_bytes);
		let deleted_text = &before[common_prefix_bytes..before_mid_end];
		let inserted_text = &after[common_prefix_bytes..after_mid_end];
		let pos = before[..common_prefix_bytes].chars().count();
		let delete_len = deleted_text.chars().count();

		let mut ops = Vec::new();
		if delete_len > 0 {
			ops.push(SwapEditOp::Delete { pos, len: delete_len });
		}
		if !inserted_text.is_empty() {
			ops.push(SwapEditOp::Insert { pos, text: inserted_text.to_string() });
		}
		ops
	}

	fn enqueue_swap_ops_from_text_diff<P>(ports: &P, state: &RimState, before: Option<BufferTextSnapshot>)
	where P: SwapIo {
		let Some(before) = before else {
			return;
		};
		let Some(after_buffer) = state.buffers.get(before.buffer_id) else {
			return;
		};
		let ops = Self::swap_ops_from_text_diff(before.text.as_str(), after_buffer.text.as_str());
		Self::enqueue_swap_ops(ports, state, before.buffer_id, ops);
	}

	fn predicted_normal_mode_editor_action_for_key(state: &RimState, key: KeyEvent) -> Option<EditorAction> {
		let normal_key = Self::to_normal_key(state, key)?;
		let mut keys = state.normal_sequence.clone();
		keys.push(normal_key);
		match Self::resolve_normal_sequence(&keys) {
			SequenceMatch::Action(AppAction::Editor(editor_action)) => Some(editor_action),
			_ => None,
		}
	}

	fn enqueue_swap_recover<P>(ports: &P, buffer_id: BufferId, source_path: PathBuf, base_text: String)
	where P: SwapIo {
		if let Err(source) = ports.enqueue_recover(buffer_id, source_path, base_text) {
			let err = ActionHandlerError::SwapRecover { source };
			error!("swap worker unavailable while enqueueing swap recover: {}", err);
		}
	}

	fn swap_conflict_prompt_message(conflict: &SwapConflictInfo) -> String {
		format!(
			"swap exists (pid {}, user {}): [r]ecover [d]elete [e]dit anyway [a]bort",
			conflict.pid, conflict.username
		)
	}

	fn handle_pending_swap_decision_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
	where P: FileIo + FileWatcher + SwapIo {
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
			state.status_bar.message = Self::swap_conflict_prompt_message(&SwapConflictInfo {
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
				Self::enqueue_swap_recover(ports, pending.buffer_id, pending.source_path.clone(), pending.base_text);
				state.status_bar.message = "recovering from swap...".to_string();
			}
			'd' => {
				if let Err(source) =
					ports.enqueue_initialize_base(pending.buffer_id, pending.source_path, pending.base_text, true)
				{
					let err = ActionHandlerError::SwapInitializeBase { source };
					error!("swap worker unavailable while enqueueing base init: {}", err);
					state.status_bar.message = "swap delete failed: swap worker unavailable".to_string();
				} else {
					state.status_bar.message = "swap deleted".to_string();
				}
			}
			'e' => {
				if let Err(source) =
					ports.enqueue_initialize_base(pending.buffer_id, pending.source_path, pending.base_text, false)
				{
					let err = ActionHandlerError::SwapInitializeBase { source };
					error!("swap worker unavailable while enqueueing base init: {}", err);
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
					let err = ActionHandlerError::SwapClose { source };
					error!("swap worker unavailable while enqueueing swap close: {}", err);
				}
				state.status_bar.message = format!("open aborted: {}", pending.source_path.display());
			}
			_ => {}
		}

		ControlFlow::Continue(())
	}

	fn dispatch_internal<P>(ports: &P, state: &mut RimState, action: AppAction) -> ControlFlow<()>
	where P: FileIo + FileWatcher + SwapIo {
		match action {
			AppAction::Editor(EditorAction::KeyPressed(key)) => {
				return Self::handle_key(ports, state, key);
			}
			AppAction::Editor(editor_action) => {
				Self::apply_editor_action(ports, state, editor_action);
			}
			AppAction::Layout(LayoutAction::SplitHorizontal) => {
				state.split_active_window(SplitAxis::Horizontal);
			}
			AppAction::Layout(LayoutAction::SplitVertical) => {
				state.split_active_window(SplitAxis::Vertical);
			}
			AppAction::Layout(LayoutAction::ViewportResized { .. }) => {}
			AppAction::Window(WindowAction::FocusLeft) => state.focus_window(FocusDirection::Left),
			AppAction::Window(WindowAction::FocusDown) => state.focus_window(FocusDirection::Down),
			AppAction::Window(WindowAction::FocusUp) => state.focus_window(FocusDirection::Up),
			AppAction::Window(WindowAction::FocusRight) => state.focus_window(FocusDirection::Right),
			AppAction::Window(WindowAction::CloseActive) => state.close_active_window(),
			AppAction::Buffer(BufferAction::SwitchPrev) => {
				state.switch_active_window_buffer(BufferSwitchDirection::Prev);
			}
			AppAction::Buffer(BufferAction::SwitchNext) => {
				state.switch_active_window_buffer(BufferSwitchDirection::Next);
			}
			AppAction::Tab(TabAction::New) => {
				state.open_new_tab();
			}
			AppAction::Tab(TabAction::CloseCurrent) => {
				state.close_current_tab();
			}
			AppAction::Tab(TabAction::SwitchPrev) => {
				state.switch_to_prev_tab();
			}
			AppAction::Tab(TabAction::SwitchNext) => {
				state.switch_to_next_tab();
			}
			AppAction::File(FileAction::SwapConflictDetected { buffer_id, result }) => match result {
				Ok(Some(conflict)) => {
					let Some((source_path, base_text)) = state
						.buffers
						.get(buffer_id)
						.and_then(|buffer| buffer.path.clone().map(|path| (path, buffer.text.clone())))
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
					state.status_bar.message = Self::swap_conflict_prompt_message(&conflict);
				}
				Ok(None) => {
					let Some((source_path, base_text)) = state
						.buffers
						.get(buffer_id)
						.and_then(|buffer| buffer.path.clone().map(|path| (path, buffer.text.clone())))
					else {
						error!("swap conflict check returned for unknown buffer path: buffer_id={:?}", buffer_id);
						return ControlFlow::Continue(());
					};
					Self::enqueue_swap_recover(ports, buffer_id, source_path, base_text);
				}
				Err(err) => {
					error!("swap conflict check failed: buffer_id={:?}, error={}", buffer_id, err);
					state.status_bar.message = "swap check failed".to_string();
				}
			},
			AppAction::File(FileAction::SwapRecoverCompleted { buffer_id, result }) => match result {
				Ok(Some(recovered_text)) => {
					state.replace_buffer_text_preserving_cursor(buffer_id, recovered_text);
					state.set_buffer_dirty(buffer_id, true);
					state.set_buffer_externally_modified(buffer_id, false);
					state.status_bar.message = "swap recovered: unsaved edits restored".to_string();
				}
				Ok(None) => {
					state.set_buffer_externally_modified(buffer_id, false);
					state.status_bar.message = "swap clean: no recovery needed".to_string();
				}
				Err(err) => {
					error!("swap recover failed: buffer_id={:?}, error={}", buffer_id, err);
				}
			},
			AppAction::File(FileAction::LoadCompleted { buffer_id, source, result }) => {
				match (source, result) {
					(FileLoadSource::Open, Ok(text)) => {
						if let Some(buffer) = state.buffers.get_mut(buffer_id) {
							buffer.text = text.clone();
						} else {
							error!("load completed for unknown buffer: buffer_id={:?}", buffer_id);
						}
						state.set_buffer_dirty(buffer_id, false);
						state.set_buffer_externally_modified(buffer_id, false);
						state.status_bar.message = "file loaded".to_string();
						if let Some(source_path) = state.buffers.get(buffer_id).and_then(|buffer| buffer.path.clone())
							&& let Err(source) = ports.enqueue_detect_conflict(buffer_id, source_path)
						{
							let err = ActionHandlerError::SwapDetectConflict { source };
							error!("swap worker unavailable while enqueueing swap conflict check: {}", err);
						}
					}
					(FileLoadSource::Open, Err(err)) => {
						error!("file load failed: buffer_id={:?}, error={}", buffer_id, err);
						state.status_bar.message = format!("load failed: {}", err);
					}
					(FileLoadSource::External, Ok(text)) => {
						let is_active = state.active_buffer_id() == Some(buffer_id);
						let Some((current_fingerprint, is_dirty, name)) = state.buffers.get(buffer_id).map(|buffer| {
							(Self::text_fingerprint(buffer.text.as_str()), buffer.dirty, buffer.name.clone())
						}) else {
							error!("external changed for unknown buffer: buffer_id={:?}", buffer_id);
							return ControlFlow::Continue(());
						};
						let incoming_fingerprint = Self::text_fingerprint(&text);
						if current_fingerprint == incoming_fingerprint {
							state.set_buffer_externally_modified(buffer_id, false);
							if is_active && state.status_bar.message.starts_with("reloading ") {
								state.status_bar.message = "file saved".to_string();
							}
							return ControlFlow::Continue(());
						}
						if is_dirty {
							state.set_buffer_externally_modified(buffer_id, true);
							if is_active {
								state.status_bar.message =
									"file changed externally; use :w! to overwrite or :e! to reload".to_string();
							}
							return ControlFlow::Continue(());
						}
						if Self::should_ignore_external_reload(state, buffer_id, &text) {
							state.set_buffer_externally_modified(buffer_id, false);
							if is_active {
								state.status_bar.message = "file saved".to_string();
							}
							return ControlFlow::Continue(());
						}
						state.replace_buffer_text_preserving_cursor(buffer_id, text);
						state.set_buffer_dirty(buffer_id, false);
						state.set_buffer_externally_modified(buffer_id, false);
						if is_active {
							state.status_bar.message = format!("reloaded {}", name);
						}
					}
					(FileLoadSource::External, Err(err)) => {
						error!("external change reload failed: buffer_id={:?}, error={}", buffer_id, err);
					}
				}
			}
			AppAction::File(FileAction::OpenRequested { path }) => {
				info!("open_file: {}", path.display());
				let normalized_path = normalize_file_path(path.as_path());
				if let Some(buffer_id) = state.find_buffer_by_path(normalized_path.as_path()) {
					state.bind_buffer_to_active_window(buffer_id);
					state.status_bar.message = format!("switched {}", path.display());
					return ControlFlow::Continue(());
				}
				let buffer_id = state.create_buffer(Some(normalized_path.clone()), String::new());
				state.bind_buffer_to_active_window(buffer_id);
				if let Err(source) = ports.enqueue_open(buffer_id, normalized_path.clone()) {
					let err = ActionHandlerError::SwapOpen { source };
					error!("swap worker unavailable while enqueueing swap open: {}", err);
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
			AppAction::File(FileAction::ExternalChangeDetected { buffer_id, path }) => {
				if state.in_flight_internal_saves.contains(&buffer_id) {
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
			AppAction::File(FileAction::SaveCompleted { buffer_id, result }) => match result {
				Ok(()) => {
					state.in_flight_internal_saves.remove(&buffer_id);
					if !state.last_internal_save_fingerprint.contains_key(&buffer_id) {
						let text = state.buffers.get(buffer_id).map(|buffer| buffer.text.clone());
						if let Some(text) = text {
							Self::mark_internal_save(state, buffer_id, &text);
						}
					}
					state.apply_pending_save_path_if_matches(buffer_id);
					if let Some(path) = state.buffers.get(buffer_id).and_then(|buffer| buffer.path.clone()) {
						if let Err(source) = ports.enqueue_watch(buffer_id, path.clone()) {
							let err = ActionHandlerError::SaveWatch { source };
							error!("watch worker unavailable while enqueueing file watch: {}", err);
						}
						if let Err(source) = ports.enqueue_mark_clean(buffer_id, path) {
							let err = ActionHandlerError::SwapMarkClean { source };
							error!("swap worker unavailable while enqueueing swap mark clean: {}", err);
						}
					}
					state.set_buffer_dirty(buffer_id, false);
					state.set_buffer_externally_modified(buffer_id, false);
					state.status_bar.message = "file saved".to_string();
					if state.quit_after_save {
						state.quit_after_save = false;
						return ControlFlow::Break(());
					}
				}
				Err(err) => {
					state.in_flight_internal_saves.remove(&buffer_id);
					state.last_internal_save_fingerprint.remove(&buffer_id);
					state.quit_after_save = false;
					state.clear_pending_save_path_if_matches(buffer_id);
					error!("file save failed: buffer_id={:?} error={}", buffer_id, err);
					state.status_bar.message = format!("save failed: {}", err);
				}
			},
			AppAction::System(SystemAction::Quit) => return ControlFlow::Break(()),
		}
		ControlFlow::Continue(())
	}

	fn handle_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
	where P: FileIo + FileWatcher + SwapIo {
		if state.pending_swap_decision.is_some() {
			return Self::handle_pending_swap_decision_key(ports, state, key);
		}

		if !state.is_visual_mode() {
			state.visual_g_pending = false;
		}

		if key.modifiers.contains(KeyModifiers::ALT) {
			state.normal_sequence.clear();
			state.status_bar.key_sequence.clear();
			return ControlFlow::Continue(());
		}

		let mode_before = state.mode;
		let pre_text_snapshot = Self::capture_active_buffer_text_snapshot(state);
		let predicted_editor_action =
			if !state.is_command_mode() && !state.is_insert_mode() && !state.is_visual_mode() {
				Self::predicted_normal_mode_editor_action_for_key(state, key)
			} else {
				None
			};
		let skip_history = matches!(predicted_editor_action, Some(EditorAction::Undo | EditorAction::Redo));

		let flow = if state.is_command_mode() {
			state.normal_sequence.clear();
			state.status_bar.key_sequence.clear();
			Self::handle_command_mode_key(ports, state, key)
		} else if state.is_visual_mode() {
			state.normal_sequence.clear();
			state.status_bar.key_sequence.clear();
			Self::handle_visual_mode_key(state, key)
		} else if state.is_insert_mode() {
			state.normal_sequence.clear();
			state.status_bar.key_sequence.clear();
			Self::handle_insert_mode_key(state, key)
		} else {
			Self::handle_normal_mode_key(ports, state, key)
		};

		if let Some(snapshot) = pre_text_snapshot.as_ref() {
			state.record_history_from_text_diff(
				snapshot.buffer_id,
				snapshot.text.as_str(),
				snapshot.cursor,
				mode_before,
				skip_history,
			);
		}
		if mode_before == EditorMode::Insert && state.mode != EditorMode::Insert {
			state.commit_insert_history_group();
		}
		Self::enqueue_swap_ops_from_text_diff(ports, state, pre_text_snapshot);

		flow
	}

	fn handle_normal_mode_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
	where P: FileIo + FileWatcher + SwapIo {
		let Some(normal_key) = Self::to_normal_key(state, key) else {
			state.normal_sequence.clear();
			state.status_bar.key_sequence.clear();
			return ControlFlow::Continue(());
		};

		state.normal_sequence.push(normal_key);

		loop {
			match Self::resolve_normal_sequence(&state.normal_sequence) {
				SequenceMatch::Action(action) => {
					state.normal_sequence.clear();
					state.status_bar.key_sequence.clear();
					return Self::dispatch_internal(ports, state, action);
				}
				SequenceMatch::Pending => {
					state.status_bar.key_sequence = Self::render_normal_sequence(&state.normal_sequence);
					return ControlFlow::Continue(());
				}
				SequenceMatch::NoMatch => {
					if state.normal_sequence.len() <= 1 {
						state.normal_sequence.clear();
						state.status_bar.key_sequence.clear();
						return ControlFlow::Continue(());
					}
					let last = *state.normal_sequence.last().expect("normal sequence has at least one key");
					state.normal_sequence.clear();
					state.normal_sequence.push(last);
					state.status_bar.key_sequence = Self::render_normal_sequence(&state.normal_sequence);
				}
			}
		}
	}

	fn to_normal_key(state: &RimState, key: KeyEvent) -> Option<NormalKey> {
		if key.modifiers.contains(KeyModifiers::ALT) {
			return None;
		}

		if key.modifiers.contains(KeyModifiers::CONTROL) {
			if let KeyCode::Char(ch) = key.code {
				return Some(NormalKey::Ctrl(ch.to_ascii_lowercase()));
			}
			return None;
		}

		if let KeyCode::Char(ch) = key.code {
			if ch == state.leader_key {
				return Some(NormalKey::Leader);
			}
			let normalized = if key.modifiers.contains(KeyModifiers::SHIFT) && ch.is_ascii_lowercase() {
				ch.to_ascii_uppercase()
			} else {
				ch
			};
			return Some(NormalKey::Char(normalized));
		}

		if key.code == KeyCode::Tab {
			return Some(NormalKey::Tab);
		}

		None
	}

	fn resolve_normal_sequence(keys: &[NormalKey]) -> SequenceMatch {
		use NormalKey as K;

		match keys {
			[K::Leader] => SequenceMatch::Pending,
			[K::Leader, K::Char('w')] => SequenceMatch::Pending,
			[K::Leader, K::Char('w'), K::Char('v')] => {
				SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitVertical))
			}
			[K::Leader, K::Char('w'), K::Char('h')] => {
				SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitHorizontal))
			}
			[K::Leader, K::Tab] => SequenceMatch::Pending,
			[K::Leader, K::Tab, K::Char('n')] => SequenceMatch::Action(AppAction::Tab(TabAction::New)),
			[K::Leader, K::Tab, K::Char('d')] => SequenceMatch::Action(AppAction::Tab(TabAction::CloseCurrent)),
			[K::Leader, K::Tab, K::Char('[')] => SequenceMatch::Action(AppAction::Tab(TabAction::SwitchPrev)),
			[K::Leader, K::Tab, K::Char(']')] => SequenceMatch::Action(AppAction::Tab(TabAction::SwitchNext)),
			[K::Leader, K::Char('b')] => SequenceMatch::Pending,
			[K::Leader, K::Char('b'), K::Char('d')] => {
				SequenceMatch::Action(AppAction::Editor(EditorAction::CloseActiveBuffer))
			}
			[K::Leader, K::Char('b'), K::Char('n')] => {
				SequenceMatch::Action(AppAction::Editor(EditorAction::NewEmptyBuffer))
			}
			[K::Char('d')] => SequenceMatch::Pending,
			[K::Char('d'), K::Char('d')] => {
				SequenceMatch::Action(AppAction::Editor(EditorAction::DeleteCurrentLineToSlot))
			}
			[K::Char('i')] => SequenceMatch::Action(AppAction::Editor(EditorAction::EnterInsert)),
			[K::Char('a')] => SequenceMatch::Action(AppAction::Editor(EditorAction::AppendInsert)),
			[K::Char('o')] => SequenceMatch::Action(AppAction::Editor(EditorAction::OpenLineBelowInsert)),
			[K::Char('O')] => SequenceMatch::Action(AppAction::Editor(EditorAction::OpenLineAboveInsert)),
			[K::Char(':')] => SequenceMatch::Action(AppAction::Editor(EditorAction::EnterCommandMode)),
			[K::Char('v')] => SequenceMatch::Action(AppAction::Editor(EditorAction::EnterVisualMode)),
			[K::Char('V')] => SequenceMatch::Action(AppAction::Editor(EditorAction::EnterVisualLineMode)),
			[K::Char('u')] => SequenceMatch::Action(AppAction::Editor(EditorAction::Undo)),
			[K::Char('h')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveLeft)),
			[K::Char('0')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveLineStart)),
			[K::Char('$')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveLineEnd)),
			[K::Char('j')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveDown)),
			[K::Char('k')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveUp)),
			[K::Char('l')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveRight)),
			[K::Char('g')] => SequenceMatch::Pending,
			[K::Char('g'), K::Char('g')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileStart)),
			[K::Char('G')] => SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileEnd)),
			[K::Char('J')] => SequenceMatch::Action(AppAction::Editor(EditorAction::JoinLineBelow)),
			[K::Char('x')] => SequenceMatch::Action(AppAction::Editor(EditorAction::CutCharToSlot)),
			[K::Char('p')] => SequenceMatch::Action(AppAction::Editor(EditorAction::PasteSlotAfterCursor)),
			[K::Char('H')] => SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchPrev)),
			[K::Char('L')] => SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchNext)),
			[K::Char('{')] => SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchPrev)),
			[K::Char('}')] => SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchNext)),
			[K::Ctrl('h')] => SequenceMatch::Action(AppAction::Window(WindowAction::FocusLeft)),
			[K::Ctrl('j')] => SequenceMatch::Action(AppAction::Window(WindowAction::FocusDown)),
			[K::Ctrl('k')] => SequenceMatch::Action(AppAction::Window(WindowAction::FocusUp)),
			[K::Ctrl('l')] => SequenceMatch::Action(AppAction::Window(WindowAction::FocusRight)),
			[K::Ctrl('e')] => SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewDown)),
			[K::Ctrl('y')] => SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewUp)),
			[K::Ctrl('d')] => SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageDown)),
			[K::Ctrl('u')] => SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageUp)),
			[K::Ctrl('r')] => SequenceMatch::Action(AppAction::Editor(EditorAction::Redo)),
			_ => SequenceMatch::NoMatch,
		}
	}

	fn render_normal_sequence(keys: &[NormalKey]) -> String {
		keys
			.iter()
			.map(|key| match key {
				NormalKey::Leader => "<leader>".to_string(),
				NormalKey::Tab => "<tab>".to_string(),
				NormalKey::Char(ch) => ch.to_string(),
				NormalKey::Ctrl(ch) => format!("<C-{}>", ch),
			})
			.collect::<Vec<_>>()
			.join("")
	}

	fn apply_editor_action<P>(ports: &P, state: &mut RimState, action: EditorAction)
	where P: FileIo + FileWatcher + SwapIo {
		match action {
			EditorAction::KeyPressed(_) => {}
			EditorAction::EnterInsert => {
				state.begin_insert_history_group();
				state.enter_insert_mode();
			}
			EditorAction::AppendInsert => {
				state.begin_insert_history_group();
				state.move_cursor_right_for_insert();
				state.enter_insert_mode();
			}
			EditorAction::OpenLineBelowInsert => {
				state.begin_insert_history_group();
				state.open_line_below_at_cursor();
				state.enter_insert_mode();
			}
			EditorAction::OpenLineAboveInsert => {
				state.begin_insert_history_group();
				state.open_line_above_at_cursor();
				state.enter_insert_mode();
			}
			EditorAction::EnterCommandMode => state.enter_command_mode(),
			EditorAction::EnterVisualMode => state.enter_visual_mode(),
			EditorAction::EnterVisualLineMode => state.enter_visual_line_mode(),
			EditorAction::MoveLeft => state.move_cursor_left(),
			EditorAction::MoveLineStart => state.move_cursor_line_start(),
			EditorAction::MoveLineEnd => state.move_cursor_line_end(),
			EditorAction::MoveDown => state.move_cursor_down(),
			EditorAction::MoveUp => state.move_cursor_up(),
			EditorAction::MoveRight => state.move_cursor_right(),
			EditorAction::MoveFileStart => state.move_cursor_file_start(),
			EditorAction::MoveFileEnd => state.move_cursor_file_end(),
			EditorAction::ScrollViewDown => state.scroll_view_down_one_line(),
			EditorAction::ScrollViewUp => state.scroll_view_up_one_line(),
			EditorAction::ScrollViewHalfPageDown => state.scroll_view_down_half_page(),
			EditorAction::ScrollViewHalfPageUp => state.scroll_view_up_half_page(),
			EditorAction::Undo => state.undo_active_buffer_edit(),
			EditorAction::Redo => state.redo_active_buffer_edit(),
			EditorAction::JoinLineBelow => state.join_line_below_at_cursor(),
			EditorAction::CutCharToSlot => state.cut_current_char_to_slot(),
			EditorAction::PasteSlotAfterCursor => state.paste_slot_at_cursor(),
			EditorAction::DeleteCurrentLineToSlot => state.delete_current_line_to_slot(),
			EditorAction::CloseActiveBuffer => {
				let closed_buffer_id = state.active_buffer_id();
				state.close_active_buffer();
				if let Some(buffer_id) = closed_buffer_id
					&& let Err(source) = ports.enqueue_unwatch(buffer_id)
				{
					let err = ActionHandlerError::CloseBufferUnwatch { source };
					error!("watch worker unavailable while enqueueing file unwatch: {}", err);
				}
				if let Some(buffer_id) = closed_buffer_id
					&& let Err(source) = ports.enqueue_close(buffer_id)
				{
					let err = ActionHandlerError::SwapClose { source };
					error!("swap worker unavailable while enqueueing swap close: {}", err);
				}
			}
			EditorAction::NewEmptyBuffer => {
				state.create_untitled_buffer();
			}
		}
	}

	fn handle_insert_mode_key(state: &mut RimState, key: KeyEvent) -> ControlFlow<()> {
		if key.modifiers.contains(KeyModifiers::CONTROL) {
			return ControlFlow::Continue(());
		}

		match key.code {
			KeyCode::Esc => {
				state.exit_insert_mode();
			}
			KeyCode::Enter => {
				state.insert_newline_at_cursor();
			}
			KeyCode::Backspace => {
				state.backspace_at_cursor();
			}
			KeyCode::Left => state.move_cursor_left(),
			KeyCode::Down => state.move_cursor_down(),
			KeyCode::Up => state.move_cursor_up(),
			KeyCode::Right => state.move_cursor_right_for_insert(),
			KeyCode::Tab => state.insert_char_at_cursor('\t'),
			KeyCode::Char(ch) => {
				state.insert_char_at_cursor(ch);
			}
		}

		ControlFlow::Continue(())
	}

	fn handle_visual_mode_key(state: &mut RimState, key: KeyEvent) -> ControlFlow<()> {
		if key.modifiers.contains(KeyModifiers::CONTROL) {
			state.visual_g_pending = false;
			match key.code {
				KeyCode::Char('e') => state.scroll_view_down_one_line(),
				KeyCode::Char('y') => state.scroll_view_up_one_line(),
				KeyCode::Char('d') => state.scroll_view_down_half_page(),
				KeyCode::Char('u') => state.scroll_view_up_half_page(),
				_ => {}
			}
			return ControlFlow::Continue(());
		}

		match key.code {
			KeyCode::Esc => {
				state.visual_g_pending = false;
				state.exit_visual_mode();
			}
			KeyCode::Char('v') => state.enter_visual_line_mode(),
			KeyCode::Char('V') => state.enter_visual_line_mode(),
			KeyCode::Char('d') => state.delete_visual_selection_to_slot(),
			KeyCode::Char('y') => state.yank_visual_selection_to_slot(),
			KeyCode::Char('p') => state.replace_visual_selection_with_slot(),
			KeyCode::Char('h') => {
				if state.is_visual_line_mode() {
					state.move_cursor_left();
				} else {
					state.move_cursor_left_for_visual_char();
				}
			}
			KeyCode::Char('j') => state.move_cursor_down(),
			KeyCode::Char('k') => state.move_cursor_up(),
			KeyCode::Char('l') => {
				if state.is_visual_line_mode() {
					state.move_cursor_right();
				} else {
					state.move_cursor_right_for_visual_char();
				}
			}
			KeyCode::Char('0') => state.move_cursor_line_start(),
			KeyCode::Char('$') => state.move_cursor_line_end(),
			KeyCode::Char('g') => {
				if state.visual_g_pending {
					state.visual_g_pending = false;
					state.move_cursor_file_start();
				} else {
					state.visual_g_pending = true;
				}
				return ControlFlow::Continue(());
			}
			KeyCode::Char('G') => state.move_cursor_file_end(),
			_ => {}
		}
		state.visual_g_pending = false;
		ControlFlow::Continue(())
	}

	fn handle_command_mode_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
	where P: FileIo + FileWatcher + SwapIo {
		if key.modifiers.contains(KeyModifiers::CONTROL) {
			return ControlFlow::Continue(());
		}

		match key.code {
			KeyCode::Esc => state.exit_command_mode(),
			KeyCode::Enter => {
				let command = state.take_command_line();
				match command.as_str() {
					"" => {}
					"qa" => {
						return Self::dispatch_internal(ports, state, AppAction::System(SystemAction::Quit));
					}
					"q!" | "quit!" => {
						if state.active_tab_window_ids().len() > 1 {
							return Self::dispatch_internal(ports, state, AppAction::Window(WindowAction::CloseActive));
						} else if state.tabs.len() > 1 {
							return Self::dispatch_internal(ports, state, AppAction::Tab(TabAction::CloseCurrent));
						} else {
							return Self::dispatch_internal(ports, state, AppAction::System(SystemAction::Quit));
						}
					}
					"q" | "quit" => {
						if state.has_dirty_buffers() {
							state.status_bar.message = "quit blocked: unsaved changes (use :q!)".to_string();
							return ControlFlow::Continue(());
						}
						if state.active_tab_window_ids().len() > 1 {
							return Self::dispatch_internal(ports, state, AppAction::Window(WindowAction::CloseActive));
						} else if state.tabs.len() > 1 {
							return Self::dispatch_internal(ports, state, AppAction::Tab(TabAction::CloseCurrent));
						} else {
							return Self::dispatch_internal(ports, state, AppAction::System(SystemAction::Quit));
						}
					}
					"w" => {
						Self::enqueue_save_active_buffer(ports, state, false, false, None);
					}
					"w!" => {
						Self::enqueue_save_active_buffer(ports, state, false, true, None);
					}
					"wa" => {
						Self::enqueue_save_all_buffers(ports, state);
					}
					"wq" => {
						Self::enqueue_save_active_buffer(ports, state, true, false, None);
					}
					"wq!" => {
						Self::enqueue_save_active_buffer(ports, state, true, true, None);
					}
					"e" => {
						Self::enqueue_reload_active_buffer(ports, state, false);
					}
					"e!" => {
						Self::enqueue_reload_active_buffer(ports, state, true);
					}
					_ if command.starts_with("e ") => {
						let path = command[2..].trim();
						if path.is_empty() {
							state.status_bar.message = "open failed: empty path".to_string();
						} else {
							return Self::dispatch_internal(
								ports,
								state,
								AppAction::File(FileAction::OpenRequested { path: PathBuf::from(path) }),
							);
						}
					}
					_ if command.starts_with("w ") => {
						let path = command[2..].trim();
						if path.is_empty() {
							state.status_bar.message = "save failed: empty path".to_string();
						} else {
							Self::enqueue_save_active_buffer(ports, state, false, false, Some(PathBuf::from(path)));
						}
					}
					_ if command.starts_with("w! ") => {
						let path = command[3..].trim();
						if path.is_empty() {
							state.status_bar.message = "save failed: empty path".to_string();
						} else {
							Self::enqueue_save_active_buffer(ports, state, false, true, Some(PathBuf::from(path)));
						}
					}
					_ if command.starts_with("wq ") => {
						let path = command[3..].trim();
						if path.is_empty() {
							state.status_bar.message = "save failed: empty path".to_string();
						} else {
							Self::enqueue_save_active_buffer(ports, state, true, false, Some(PathBuf::from(path)));
						}
					}
					_ if command.starts_with("wq! ") => {
						let path = command[4..].trim();
						if path.is_empty() {
							state.status_bar.message = "save failed: empty path".to_string();
						} else {
							Self::enqueue_save_active_buffer(ports, state, true, true, Some(PathBuf::from(path)));
						}
					}
					_ => {
						state.status_bar.message = format!("unknown command: {}", command);
					}
				}
			}
			KeyCode::Backspace => state.pop_command_char(),
			KeyCode::Char(ch) => state.push_command_char(ch),
			_ => {}
		}
		ControlFlow::Continue(())
	}

	fn enqueue_save_active_buffer<P>(
		ports: &P,
		state: &mut RimState,
		quit_after_save: bool,
		force_overwrite: bool,
		path_override: Option<PathBuf>,
	) where
		P: FileIo + FileWatcher + SwapIo,
	{
		if !force_overwrite
			&& path_override.is_none()
			&& matches!(state.active_buffer_is_externally_modified(), Some(true))
		{
			state.status_bar.message = "save blocked: file changed externally (use :w! to overwrite)".to_string();
			state.quit_after_save = false;
			return;
		}

		let bind_override_path =
			matches!((path_override.as_ref(), state.active_buffer_has_path()), (Some(_), Some(false)));
		let (buffer_id, path, text) = match state.active_buffer_save_snapshot(path_override.clone()) {
			Ok(snapshot) => snapshot,
			Err(reason) => {
				state.status_bar.message = format!("save failed: {}", reason);
				state.quit_after_save = false;
				return;
			}
		};

		Self::mark_internal_save(state, buffer_id, &text);
		state.in_flight_internal_saves.insert(buffer_id);
		if let Err(source) = ports.enqueue_save(buffer_id, path, text) {
			let err = ActionHandlerError::Save { source };
			error!("io worker unavailable while enqueueing file save: {}", err);
			state.status_bar.message = "save failed: io worker unavailable".to_string();
			state.in_flight_internal_saves.remove(&buffer_id);
			state.last_internal_save_fingerprint.remove(&buffer_id);
			state.quit_after_save = false;
			return;
		}

		if bind_override_path {
			state.set_pending_save_path(buffer_id, path_override);
		} else {
			state.set_pending_save_path(buffer_id, None);
		}
		state.quit_after_save = quit_after_save;
		state.status_bar.message = "saving...".to_string();
	}

	fn enqueue_reload_active_buffer<P>(ports: &P, state: &mut RimState, force_reload: bool)
	where P: FileIo + FileWatcher + SwapIo {
		let active_is_dirty = state
			.active_buffer_id()
			.and_then(|id| state.buffers.get(id))
			.map(|buffer| buffer.dirty)
			.unwrap_or(false);
		if !force_reload && active_is_dirty {
			state.status_bar.message = "reload blocked: buffer is dirty (use :e! to force reload)".to_string();
			return;
		}

		let (buffer_id, path) = match state.active_buffer_load_target() {
			Ok(target) => target,
			Err(reason) => {
				state.status_bar.message = format!("reload failed: {}", reason);
				return;
			}
		};

		if let Err(source) = ports.enqueue_load(buffer_id, path.clone()) {
			let err = ActionHandlerError::Reload { source };
			error!("io worker unavailable while enqueueing file load: {}", err);
			state.status_bar.message = "reload failed: io worker unavailable".to_string();
			return;
		}
		state.status_bar.message = format!("loading {}", path.display());
	}

	fn enqueue_save_all_buffers<P>(ports: &P, state: &mut RimState)
	where P: FileIo + FileWatcher + SwapIo {
		let (snapshots, missing_path) = state.all_buffer_save_snapshots();
		if snapshots.is_empty() {
			if missing_path > 0 {
				state.status_bar.message = "save failed: no buffer has file path".to_string();
			} else {
				state.status_bar.message = "nothing to save".to_string();
			}
			return;
		}

		let mut enqueued = 0usize;
		for (buffer_id, path, text) in snapshots {
			Self::mark_internal_save(state, buffer_id, &text);
			state.in_flight_internal_saves.insert(buffer_id);
			if let Err(source) = ports.enqueue_save(buffer_id, path, text) {
				let err = ActionHandlerError::SaveAll { source };
				error!("io worker unavailable while enqueueing file save: {}", err);
				state.status_bar.message = "save failed: io worker unavailable".to_string();
				state.in_flight_internal_saves.remove(&buffer_id);
				state.last_internal_save_fingerprint.remove(&buffer_id);
				state.quit_after_save = false;
				return;
			}
			enqueued = enqueued.saturating_add(1);
		}

		state.quit_after_save = false;
		if missing_path > 0 {
			state.status_bar.message = format!("saving {} buffers ({} skipped: no path)", enqueued, missing_path);
		} else {
			state.status_bar.message = format!("saving {} buffers...", enqueued);
		}
	}
}

fn normalize_file_path(path: &Path) -> PathBuf {
	let absolute = if path.is_absolute() {
		path.to_path_buf()
	} else {
		std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
	};
	std::fs::canonicalize(&absolute).unwrap_or(absolute)
}

#[cfg(test)]
mod tests {
	use std::{cell::RefCell, ops::ControlFlow, path::{Path, PathBuf}};

	use super::{NormalKey, SequenceMatch};
	use crate::{action::{AppAction, BufferAction, EditorAction, FileAction, KeyCode, KeyEvent, KeyModifiers, LayoutAction, SwapConflictInfo, TabAction}, ports::{FileIo, FileIoError, FileWatcher, FileWatcherError, SwapEditOp, SwapIo, SwapIoError}, state::{BufferId, PendingSwapDecision, RimState}};

	struct TestPorts;

	impl FileIo for TestPorts {
		fn enqueue_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileIoError> { Ok(()) }

		fn enqueue_save(&self, _buffer_id: BufferId, _path: PathBuf, _text: String) -> Result<(), FileIoError> {
			Ok(())
		}

		fn enqueue_external_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileIoError> {
			Ok(())
		}
	}

	impl FileWatcher for TestPorts {
		fn enqueue_watch(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileWatcherError> { Ok(()) }

		fn enqueue_unwatch(&self, _buffer_id: BufferId) -> Result<(), FileWatcherError> { Ok(()) }
	}

	impl SwapIo for TestPorts {
		fn enqueue_open(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), SwapIoError> { Ok(()) }

		fn enqueue_detect_conflict(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_edit(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
			_op: SwapEditOp,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_mark_clean(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_initialize_base(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
			_base_text: String,
			_delete_existing: bool,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_recover(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
			_base_text: String,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_close(&self, _buffer_id: BufferId) -> Result<(), SwapIoError> { Ok(()) }
	}

	fn dispatch_test_action(state: &mut RimState, action: AppAction) -> ControlFlow<()> {
		let ports = TestPorts;
		state.apply_action(&ports, action)
	}

	#[derive(Default)]
	struct RecordingPorts {
		file_loads: RefCell<Vec<(BufferId, PathBuf)>>,
	}

	impl FileIo for RecordingPorts {
		fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError> {
			self.file_loads.borrow_mut().push((buffer_id, path));
			Ok(())
		}

		fn enqueue_save(&self, _buffer_id: BufferId, _path: PathBuf, _text: String) -> Result<(), FileIoError> {
			Ok(())
		}

		fn enqueue_external_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileIoError> {
			Ok(())
		}
	}

	impl FileWatcher for RecordingPorts {
		fn enqueue_watch(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileWatcherError> { Ok(()) }

		fn enqueue_unwatch(&self, _buffer_id: BufferId) -> Result<(), FileWatcherError> { Ok(()) }
	}

	impl SwapIo for RecordingPorts {
		fn enqueue_open(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), SwapIoError> { Ok(()) }

		fn enqueue_detect_conflict(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_edit(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
			_op: SwapEditOp,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_mark_clean(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_initialize_base(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
			_base_text: String,
			_delete_existing: bool,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_recover(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
			_base_text: String,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_close(&self, _buffer_id: BufferId) -> Result<(), SwapIoError> { Ok(()) }
	}

	#[derive(Default)]
	struct SwapDecisionPorts {
		swap_conflict_detects: RefCell<Vec<(BufferId, PathBuf)>>,
		swap_recovers:         RefCell<Vec<(BufferId, PathBuf, String)>>,
		swap_inits:            RefCell<Vec<(BufferId, PathBuf, String, bool)>>,
		unwatches:             RefCell<Vec<BufferId>>,
		swap_closes:           RefCell<Vec<BufferId>>,
	}

	impl FileIo for SwapDecisionPorts {
		fn enqueue_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileIoError> { Ok(()) }

		fn enqueue_save(&self, _buffer_id: BufferId, _path: PathBuf, _text: String) -> Result<(), FileIoError> {
			Ok(())
		}

		fn enqueue_external_load(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileIoError> {
			Ok(())
		}
	}

	impl FileWatcher for SwapDecisionPorts {
		fn enqueue_watch(&self, _buffer_id: BufferId, _path: PathBuf) -> Result<(), FileWatcherError> { Ok(()) }

		fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError> {
			self.unwatches.borrow_mut().push(buffer_id);
			Ok(())
		}
	}

	impl SwapIo for SwapDecisionPorts {
		fn enqueue_open(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), SwapIoError> { Ok(()) }

		fn enqueue_detect_conflict(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError> {
			self.swap_conflict_detects.borrow_mut().push((buffer_id, source_path));
			Ok(())
		}

		fn enqueue_edit(
			&self,
			_buffer_id: BufferId,
			_source_path: PathBuf,
			_op: SwapEditOp,
		) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_mark_clean(&self, _buffer_id: BufferId, _source_path: PathBuf) -> Result<(), SwapIoError> {
			Ok(())
		}

		fn enqueue_initialize_base(
			&self,
			buffer_id: BufferId,
			source_path: PathBuf,
			base_text: String,
			delete_existing: bool,
		) -> Result<(), SwapIoError> {
			self.swap_inits.borrow_mut().push((buffer_id, source_path, base_text, delete_existing));
			Ok(())
		}

		fn enqueue_recover(
			&self,
			buffer_id: BufferId,
			source_path: PathBuf,
			base_text: String,
		) -> Result<(), SwapIoError> {
			self.swap_recovers.borrow_mut().push((buffer_id, source_path, base_text));
			Ok(())
		}

		fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), SwapIoError> {
			self.swap_closes.borrow_mut().push(buffer_id);
			Ok(())
		}
	}

	fn normalize_test_path(path: &str) -> PathBuf {
		let path = Path::new(path);
		let absolute = if path.is_absolute() {
			path.to_path_buf()
		} else {
			std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
		};
		std::fs::canonicalize(&absolute).unwrap_or(absolute)
	}

	fn map_normal_key(state: &RimState, key: KeyEvent) -> Option<NormalKey> {
		RimState::to_normal_key(state, key)
	}

	fn resolve_keys(keys: &[NormalKey]) -> SequenceMatch { RimState::resolve_normal_sequence(keys) }

	#[test]
	fn to_normal_key_should_map_leader_char_to_leader_token() {
		let mut state = RimState::new();
		state.leader_key = ' ';
		let key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);

		let mapped = map_normal_key(&state, key);
		assert_eq!(mapped, Some(NormalKey::Leader));
	}

	#[test]
	fn resolve_normal_sequence_should_keep_leader_w_pending() {
		let seq = vec![NormalKey::Leader, NormalKey::Char('w')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Pending));
	}

	#[test]
	fn resolve_normal_sequence_should_map_leader_w_v_to_split_vertical() {
		let seq = vec![NormalKey::Leader, NormalKey::Char('w'), NormalKey::Char('v')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitVertical))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_leader_w_h_to_split_horizontal() {
		let seq = vec![NormalKey::Leader, NormalKey::Char('w'), NormalKey::Char('h')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Layout(LayoutAction::SplitHorizontal))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_leader_tab_n_to_new_tab() {
		let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('n')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Tab(TabAction::New))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_leader_tab_d_to_close_tab() {
		let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('d')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Tab(TabAction::CloseCurrent))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_leader_tab_left_bracket_to_prev_tab() {
		let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char('[')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Tab(TabAction::SwitchPrev))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_leader_tab_right_bracket_to_next_tab() {
		let seq = vec![NormalKey::Leader, NormalKey::Tab, NormalKey::Char(']')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Tab(TabAction::SwitchNext))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_upper_h_to_prev_buffer() {
		let seq = vec![NormalKey::Char('H')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchPrev))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_upper_l_to_next_buffer() {
		let seq = vec![NormalKey::Char('L')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Buffer(BufferAction::SwitchNext))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_gg_to_move_file_start() {
		let seq = vec![NormalKey::Char('g'), NormalKey::Char('g')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileStart))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_upper_g_to_move_file_end() {
		let seq = vec![NormalKey::Char('G')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::MoveFileEnd))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_upper_j_to_join_line_below() {
		let seq = vec![NormalKey::Char('J')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::JoinLineBelow))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_upper_v_to_enter_visual_line_mode() {
		let seq = vec![NormalKey::Char('V')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::EnterVisualLineMode))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_u_to_undo() {
		let seq = vec![NormalKey::Char('u')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::Undo))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_ctrl_e_to_scroll_view_down() {
		let seq = vec![NormalKey::Ctrl('e')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewDown))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_ctrl_y_to_scroll_view_up() {
		let seq = vec![NormalKey::Ctrl('y')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewUp))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_ctrl_d_to_scroll_view_half_page_down() {
		let seq = vec![NormalKey::Ctrl('d')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(
			resolved,
			SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageDown))
		));
	}

	#[test]
	fn resolve_normal_sequence_should_map_ctrl_u_to_scroll_view_half_page_up() {
		let seq = vec![NormalKey::Ctrl('u')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::ScrollViewHalfPageUp))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_ctrl_r_to_redo() {
		let seq = vec![NormalKey::Ctrl('r')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::Redo))));
	}

	#[test]
	fn to_normal_key_should_map_shift_g_to_upper_g() {
		let state = RimState::new();
		let key = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SHIFT);
		let mapped = map_normal_key(&state, key);
		assert_eq!(mapped, Some(NormalKey::Char('G')));
	}

	#[test]
	fn resolve_normal_sequence_should_map_leader_b_d_to_close_active_buffer() {
		let seq = vec![NormalKey::Leader, NormalKey::Char('b'), NormalKey::Char('d')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::CloseActiveBuffer))));
	}

	#[test]
	fn resolve_normal_sequence_should_map_leader_b_n_to_new_empty_buffer() {
		let seq = vec![NormalKey::Leader, NormalKey::Char('b'), NormalKey::Char('n')];
		let resolved = resolve_keys(&seq);
		assert!(matches!(resolved, SequenceMatch::Action(AppAction::Editor(EditorAction::NewEmptyBuffer))));
	}

	#[test]
	fn file_load_completed_should_mark_buffer_clean() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, true);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::LoadCompleted {
				buffer_id,
				source: crate::action::FileLoadSource::Open,
				result: Ok("loaded".to_string()),
			}),
		);

		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert!(!buffer.dirty);
	}

	#[test]
	fn file_save_completed_should_mark_buffer_clean() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, true);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id, result: Ok(()) }),
		);

		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert!(!buffer.dirty);
	}

	#[test]
	fn external_changed_should_be_ignored_when_it_matches_internal_save_fingerprint() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "old");
		state.bind_buffer_to_active_window(buffer_id);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id, result: Ok(()) }),
		);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::LoadCompleted {
				buffer_id,
				source: crate::action::FileLoadSource::External,
				result: Ok("old".to_string()),
			}),
		);

		assert_eq!(state.status_bar.message, "file saved");
		assert_eq!(state.buffers.get(buffer_id).expect("buffer exists").text, "old");
	}

	#[test]
	fn external_changed_should_reload_when_content_differs_from_internal_save_fingerprint() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "old");
		state.bind_buffer_to_active_window(buffer_id);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id, result: Ok(()) }),
		);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::LoadCompleted {
				buffer_id,
				source: crate::action::FileLoadSource::External,
				result: Ok("new".to_string()),
			}),
		);

		assert_eq!(state.buffers.get(buffer_id).expect("buffer exists").text, "new");
	}

	#[test]
	fn internal_save_echo_should_not_leave_reloading_message() {
		let mut state = RimState::new();
		let path = PathBuf::from("a.txt");
		let buffer_id = state.create_buffer(Some(path.clone()), "old");
		state.bind_buffer_to_active_window(buffer_id);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id, result: Ok(()) }),
		);
		assert_eq!(state.status_bar.message, "file saved");

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path }),
		);
		assert_eq!(state.status_bar.message, "file saved");

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::LoadCompleted {
				buffer_id,
				source: crate::action::FileLoadSource::External,
				result: Ok("old".to_string()),
			}),
		);
		assert_eq!(state.status_bar.message, "file saved");
	}

	#[test]
	fn external_change_detected_should_be_ignored_while_internal_save_in_flight() {
		let mut state = RimState::new();
		let path = PathBuf::from("a.txt");
		let buffer_id = state.create_buffer(Some(path.clone()), "old");
		state.bind_buffer_to_active_window(buffer_id);
		state.in_flight_internal_saves.insert(buffer_id);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path }),
		);

		assert_ne!(state.status_bar.message, "reloading a.txt");
	}

	#[test]
	fn command_q_should_be_blocked_when_any_buffer_is_dirty() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "abc");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, true);
		state.enter_command_mode();
		state.push_command_char('q');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		assert_eq!(state.status_bar.message, "quit blocked: unsaved changes (use :q!)");
	}

	#[test]
	fn command_q_bang_should_force_quit_when_buffer_is_dirty() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "abc");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, true);
		state.enter_command_mode();
		state.push_command_char('q');
		state.push_command_char('!');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Break(())));
	}

	#[test]
	fn command_q_should_quit_when_all_buffers_are_clean() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "abc");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, false);
		state.enter_command_mode();
		state.push_command_char('q');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Break(())));
	}

	#[test]
	fn external_changed_should_reload_when_buffer_is_clean() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "old");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, false);

		let flow = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::LoadCompleted {
				buffer_id,
				source: crate::action::FileLoadSource::External,
				result: Ok("new".to_string()),
			}),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "new");
		assert!(!buffer.dirty);
	}

	#[test]
	fn external_changed_should_not_reload_when_buffer_is_dirty() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "old");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, true);

		let flow = dispatch_test_action(
			&mut state,
			AppAction::File(crate::action::FileAction::LoadCompleted {
				buffer_id,
				source: crate::action::FileLoadSource::External,
				result: Ok("new".to_string()),
			}),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "old");
		assert!(buffer.dirty);
		assert!(buffer.externally_modified);
		assert_eq!(state.status_bar.message, "file changed externally; use :w! to overwrite or :e! to reload");
	}

	#[test]
	fn command_w_should_be_blocked_when_file_was_changed_externally() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "old");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, true);
		state.set_buffer_externally_modified(buffer_id, true);
		state.enter_command_mode();
		state.push_command_char('w');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		assert_eq!(state.status_bar.message, "save blocked: file changed externally (use :w! to overwrite)");
	}

	#[test]
	fn command_e_should_be_blocked_when_buffer_is_dirty() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "old");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, true);
		state.enter_command_mode();
		state.push_command_char('e');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		assert_eq!(state.status_bar.message, "reload blocked: buffer is dirty (use :e! to force reload)");
	}

	#[test]
	fn command_e_bang_should_reload_even_when_buffer_is_dirty() {
		let mut state = RimState::new();
		let path = PathBuf::from("a.txt");
		let buffer_id = state.create_buffer(Some(path.clone()), "old");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_buffer_dirty(buffer_id, true);
		state.enter_command_mode();
		state.push_command_char('e');
		state.push_command_char('!');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		assert_eq!(state.status_bar.message, format!("loading {}", path.display()));
	}

	#[test]
	fn command_e_with_path_should_open_new_file_buffer() {
		let mut state = RimState::new();
		let initial = state.create_buffer(None, "old");
		state.bind_buffer_to_active_window(initial);
		state.enter_command_mode();
		state.push_command_char('e');
		state.push_command_char(' ');
		state.push_command_char('b');
		state.push_command_char('.');
		state.push_command_char('t');
		state.push_command_char('x');
		state.push_command_char('t');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		let active_id = state.active_buffer_id().expect("active buffer exists");
		assert_ne!(active_id, initial);
		let buffer = state.buffers.get(active_id).expect("buffer exists");
		let expected = normalize_test_path("b.txt");
		assert_eq!(buffer.path.as_deref(), Some(expected.as_path()));
		assert_eq!(state.status_bar.message, "loading b.txt");
	}

	#[test]
	fn command_e_with_same_path_should_reuse_existing_buffer_in_same_tab() {
		let mut state = RimState::new();
		let existing_path = normalize_test_path("b.txt");
		let existing = state.create_buffer(Some(existing_path), "old");
		state.bind_buffer_to_active_window(existing);
		state.create_untitled_buffer();
		state.enter_command_mode();
		state.push_command_char('e');
		state.push_command_char(' ');
		state.push_command_char('b');
		state.push_command_char('.');
		state.push_command_char('t');
		state.push_command_char('x');
		state.push_command_char('t');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		let active_id = state.active_buffer_id().expect("active buffer exists");
		assert_eq!(active_id, existing);
		assert_eq!(state.status_bar.message, "switched b.txt");
		let expected = normalize_test_path("b.txt");
		let count =
			state.buffers.iter().filter(|(_, buffer)| buffer.path.as_deref() == Some(expected.as_path())).count();
		assert_eq!(count, 1);
	}

	#[test]
	fn command_e_with_same_path_should_reuse_existing_buffer_across_tabs() {
		let mut state = RimState::new();
		let existing_path = normalize_test_path("b.txt");
		let existing = state.create_buffer(Some(existing_path), "old");
		state.bind_buffer_to_active_window(existing);
		state.open_new_tab();
		state.create_untitled_buffer();
		state.enter_command_mode();
		state.push_command_char('e');
		state.push_command_char(' ');
		state.push_command_char('b');
		state.push_command_char('.');
		state.push_command_char('t');
		state.push_command_char('x');
		state.push_command_char('t');

		let flow = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
		);

		assert!(matches!(flow, ControlFlow::Continue(())));
		let active_id = state.active_buffer_id().expect("active buffer exists");
		assert_eq!(active_id, existing);
		assert_eq!(state.status_bar.message, "switched b.txt");
		let expected = normalize_test_path("b.txt");
		let count =
			state.buffers.iter().filter(|(_, buffer)| buffer.path.as_deref() == Some(expected.as_path())).count();
		assert_eq!(count, 1);
	}

	#[test]
	fn visual_mode_should_support_ctrl_scroll_keys() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "a\nb\nc\nd");
		state.bind_buffer_to_active_window(buffer_id);
		state.enter_visual_mode();

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL))),
		);
		assert_eq!(state.active_cursor().row, 2);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL))),
		);
		assert_eq!(state.active_cursor().row, 1);
	}

	#[test]
	fn visual_mode_should_support_gg_and_g() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "a\nb\nc\nd");
		state.bind_buffer_to_active_window(buffer_id);
		state.move_cursor_down();
		state.move_cursor_down();
		state.enter_visual_mode();

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))),
		);
		assert_eq!(state.active_cursor().row, 3);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))),
		);
		assert_eq!(state.active_cursor().row, 1);

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT))),
		);
		assert_eq!(state.active_cursor().row, 4);
	}

	#[test]
	fn visual_delete_should_be_undoable_with_single_u() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "abcd");
		state.bind_buffer_to_active_window(buffer_id);

		for key in [
			KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
		] {
			let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
		}
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "cd");

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
		);
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "abcd");
	}

	#[test]
	fn visual_paste_should_be_undoable_with_single_u() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "abcd");
		state.bind_buffer_to_active_window(buffer_id);
		state.line_slot = Some("XY".to_string());
		state.line_slot_line_wise = false;

		for key in [
			KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
		] {
			let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
		}
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "XYcd");

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
		);
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "abcd");
	}

	#[test]
	fn normal_line_wise_paste_should_be_undoable_with_single_u() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "a\nc");
		state.bind_buffer_to_active_window(buffer_id);
		state.line_slot = Some("b".to_string());
		state.line_slot_line_wise = true;

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))),
		);
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "a\nb\nc");

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
		);
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "a\nc");
	}

	#[test]
	fn insert_typing_should_be_grouped_into_single_undo_step() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "");
		state.bind_buffer_to_active_window(buffer_id);

		for key in [
			KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
		] {
			let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
		}

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
		);
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "");

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))),
		);
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "use");
	}

	#[test]
	fn open_line_below_insert_should_be_grouped_into_single_undo_step() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "a");
		state.bind_buffer_to_active_window(buffer_id);

		for key in [
			KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
		] {
			let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
		}

		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "a\nuse");

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
		);
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "a");
	}

	#[test]
	fn open_line_above_insert_should_be_grouped_into_single_undo_step() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "a");
		state.bind_buffer_to_active_window(buffer_id);

		for key in [
			KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT),
			KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
			KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
		] {
			let _ = dispatch_test_action(&mut state, AppAction::Editor(EditorAction::KeyPressed(key)));
		}

		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "use\na");

		let _ = dispatch_test_action(
			&mut state,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))),
		);
		let buffer = state.buffers.get(buffer_id).expect("buffer exists");
		assert_eq!(buffer.text, "a");
	}

	#[test]
	fn open_requested_should_enqueue_file_load() {
		let mut state = RimState::new();
		let ports = RecordingPorts::default();

		let path = PathBuf::from("a.txt");
		let _ = state.apply_action(&ports, AppAction::File(crate::action::FileAction::OpenRequested { path }));

		assert_eq!(ports.file_loads.borrow().len(), 1);
	}

	#[test]
	fn open_load_should_detect_swap_conflict_before_recover() {
		let mut state = RimState::new();
		let ports = SwapDecisionPorts::default();
		let path = normalize_test_path("swap_conflict.txt");
		let buffer_id = state.create_buffer(Some(path.clone()), "");
		state.bind_buffer_to_active_window(buffer_id);

		let _ = state.apply_action(
			&ports,
			AppAction::File(crate::action::FileAction::LoadCompleted {
				buffer_id,
				source: crate::action::FileLoadSource::Open,
				result: Ok("base".to_string()),
			}),
		);

		assert_eq!(ports.swap_conflict_detects.borrow().len(), 1);
		assert!(ports.swap_recovers.borrow().is_empty());
	}

	#[test]
	fn swap_conflict_prompt_recover_key_should_enqueue_recover() {
		let mut state = RimState::new();
		let ports = SwapDecisionPorts::default();
		let path = normalize_test_path("swap_recover.txt");
		let buffer_id = state.create_buffer(Some(path.clone()), "base");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_pending_swap_decision(PendingSwapDecision {
			buffer_id,
			source_path: path.clone(),
			base_text: "base".to_string(),
			owner_pid: 42,
			owner_username: "tester".to_string(),
		});

		let _ = state.apply_action(
			&ports,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))),
		);

		assert!(state.pending_swap_decision.is_none());
		assert_eq!(ports.swap_recovers.borrow().len(), 1);
		assert_eq!(state.status_bar.message, "recovering from swap...");
	}

	#[test]
	fn swap_recover_completed_with_no_changes_should_update_status_message() {
		let mut state = RimState::new();
		let buffer_id = state.create_buffer(None, "base");
		state.bind_buffer_to_active_window(buffer_id);
		state.status_bar.message = "recovering from swap...".to_string();

		let _ = dispatch_test_action(
			&mut state,
			AppAction::File(FileAction::SwapRecoverCompleted { buffer_id, result: Ok(None) }),
		);

		assert_eq!(state.status_bar.message, "swap clean: no recovery needed");
	}

	#[test]
	fn swap_conflict_prompt_abort_key_should_close_buffer_and_cleanup_watchers() {
		let mut state = RimState::new();
		let ports = SwapDecisionPorts::default();
		let path = normalize_test_path("swap_abort.txt");
		let buffer_id = state.create_buffer(Some(path.clone()), "base");
		state.bind_buffer_to_active_window(buffer_id);
		state.set_pending_swap_decision(PendingSwapDecision {
			buffer_id,
			source_path: path.clone(),
			base_text: "base".to_string(),
			owner_pid: 7,
			owner_username: "owner".to_string(),
		});

		let _ = state.apply_action(
			&ports,
			AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))),
		);

		assert!(state.pending_swap_decision.is_none());
		assert!(!state.buffers.contains_key(buffer_id));
		assert_eq!(ports.unwatches.borrow().as_slice(), &[buffer_id]);
		assert_eq!(ports.swap_closes.borrow().as_slice(), &[buffer_id]);
	}

	#[test]
	fn swap_conflict_detected_should_enter_pending_prompt() {
		let mut state = RimState::new();
		let ports = SwapDecisionPorts::default();
		let path = normalize_test_path("swap_pending.txt");
		let buffer_id = state.create_buffer(Some(path.clone()), "base");
		state.bind_buffer_to_active_window(buffer_id);

		let _ = state.apply_action(
			&ports,
			AppAction::File(FileAction::SwapConflictDetected {
				buffer_id,
				result: Ok(Some(SwapConflictInfo { pid: 99, username: "other".to_string() })),
			}),
		);

		let pending = state.pending_swap_decision.as_ref().expect("pending decision should exist");
		assert_eq!(pending.buffer_id, buffer_id);
		assert_eq!(pending.source_path, path);
		assert_eq!(pending.base_text, "base");
		assert_eq!(pending.owner_pid, 99);
		assert_eq!(pending.owner_username, "other");
		assert!(state.status_bar.message.contains("[r]ecover"));
	}
}
