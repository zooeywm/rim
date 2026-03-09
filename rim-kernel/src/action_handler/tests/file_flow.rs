use std::{ops::ControlFlow, path::PathBuf, time::{Duration, Instant}};

use super::support::{RecordingPorts, SwapDecisionPorts, dispatch_test_action, normalize_test_path};
use crate::{action::{AppAction, EditorAction, FileAction, KeyCode, KeyEvent, KeyModifiers, SwapConflictInfo}, state::{BufferEditSnapshot, BufferHistoryEntry, CursorState, PendingSwapDecision, PersistedBufferHistory, RimState}};

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
fn external_change_detected_should_be_ignored_briefly_after_save() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let path = PathBuf::from("a.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "old");
	state.bind_buffer_to_active_window(buffer_id);

	let _ = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id, result: Ok(()) }),
	);
	let _ = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path }),
	);

	assert!(ports.external_loads.borrow().is_empty());
	assert_eq!(state.status_bar.message, "file saved");
}

#[test]
fn external_change_detected_should_reload_after_ignore_window_expires() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let path = PathBuf::from("a.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "old");
	state.bind_buffer_to_active_window(buffer_id);
	state.ignore_external_change_until.insert(buffer_id, Instant::now() - Duration::from_millis(1));

	let _ = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path: path.clone() }),
	);

	assert_eq!(ports.external_loads.borrow().as_slice(), &[(buffer_id, path)]);
}

#[test]
fn internal_save_echo_should_not_leave_reloading_message() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let path = PathBuf::from("a.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "old");
	state.bind_buffer_to_active_window(buffer_id);

	let _ = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id, result: Ok(()) }),
	);
	assert_eq!(state.status_bar.message, "file saved");

	let _ = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path }),
	);
	assert_eq!(state.status_bar.message, "file saved");
	assert!(ports.external_loads.borrow().is_empty());
}

#[test]
fn external_change_detected_should_be_ignored_while_internal_save_in_flight() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let path = PathBuf::from("a.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "old");
	state.bind_buffer_to_active_window(buffer_id);
	state.in_flight_internal_saves.insert(buffer_id);

	let _ = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path }),
	);

	assert!(ports.external_loads.borrow().is_empty());
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
	assert_eq!(buffer.text.to_string(), "new");
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
	assert_eq!(buffer.text.to_string(), "old");
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
fn open_requested_should_enqueue_file_load() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();

	let path = PathBuf::from("a.txt");
	let _ = state.apply_action(&ports, AppAction::File(crate::action::FileAction::OpenRequested { path }));

	assert_eq!(ports.file_loads.borrow().len(), 1);
}

#[test]
fn undo_history_loaded_should_restore_buffer_history_when_text_matches() {
	let mut state = RimState::new();
	let path = normalize_test_path("undo_restore.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "abc");
	state.bind_buffer_to_active_window(buffer_id);
	let history = PersistedBufferHistory {
		current_text: "abc".to_string(),
		cursor:       CursorState { row: 1, col: 2 },
		undo_stack:   vec![BufferHistoryEntry {
			edits:         vec![BufferEditSnapshot {
				start_byte:    1,
				deleted_text:  String::new(),
				inserted_text: "x".to_string(),
			}],
			before_cursor: CursorState { row: 1, col: 2 },
			after_cursor:  CursorState { row: 1, col: 3 },
		}],
		redo_stack:   vec![BufferHistoryEntry {
			edits:         vec![BufferEditSnapshot {
				start_byte:    1,
				deleted_text:  "x".to_string(),
				inserted_text: String::new(),
			}],
			before_cursor: CursorState { row: 1, col: 3 },
			after_cursor:  CursorState { row: 1, col: 2 },
		}],
	};

	let _ = dispatch_test_action(
		&mut state,
		AppAction::File(FileAction::UndoHistoryLoaded {
			buffer_id,
			source_path: path,
			expected_text: "abc".to_string(),
			result: Ok(Some(history.clone())),
		}),
	);

	let buffer = state.buffers.get(buffer_id).expect("buffer should exist");
	assert_eq!(buffer.undo_stack, history.undo_stack);
	assert_eq!(buffer.redo_stack, history.redo_stack);
	assert_eq!(state.active_cursor(), history.cursor);
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
