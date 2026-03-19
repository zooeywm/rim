use std::{ops::ControlFlow, path::PathBuf, time::{Duration, Instant}};

use rim_ports::{PluginAction as RequestedPluginAction, PluginCommandResponse, PluginEffect};

use super::support::{FilePickerPorts, RecordingPorts, SwapDecisionPorts, dispatch_test_action, normalize_test_path};
use crate::{action::{AppAction, EditorAction, FileAction, KeyCode, KeyEvent, KeyModifiers, PluginRuntimeAction, SwapConflictCheckResult, SwapConflictInfo, SystemAction}, command::{CommandAliasConfig, CommandAliasSection, CommandConfigFile, PluginCommandRegistration}, state::{BufferEditSnapshot, BufferHistoryEntry, CursorState, PendingSwapDecision, PersistedBufferHistory, RimState, WorkspaceBufferHistorySnapshot, WorkspaceBufferSnapshot, WorkspaceSessionSnapshot, WorkspaceTabSnapshot, WorkspaceWindowBufferViewSnapshot, WorkspaceWindowSnapshot}};

fn register_yazi_plugin_command(state: &mut RimState) {
	state
		.register_plugin_command(PluginCommandRegistration {
			id:           "plugin.yazi.yazi".to_string(),
			default_name: "Yazi".to_string(),
			plugin_id:    "yazi".to_string(),
			command_id:   "yazi".to_string(),
			category:     "Yazi Plugin".to_string(),
			description:  "Open the host file picker".to_string(),
			params:       Vec::new(),
		})
		.expect("yazi plugin command should register");
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
fn system_quit_should_enqueue_workspace_session_save() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "hello");
	state.bind_buffer_to_active_window(buffer_id);

	let flow = state.apply_action(&ports, AppAction::System(SystemAction::Quit));

	assert!(matches!(flow, ControlFlow::Break(())));
	assert_eq!(ports.session_saves.borrow().len(), 1);
}

#[test]
fn workspace_session_loaded_should_restore_state_and_enqueue_runtime_bindings() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let source_path = normalize_test_path("a.rs");
	let snapshot = WorkspaceSessionSnapshot {
		version:          1,
		buffers:          vec![
			WorkspaceBufferSnapshot {
				path:       Some(source_path.clone()),
				text:       "alpha\nbeta\n".to_string(),
				clean_text: "alpha\nbeta\n".to_string(),
				history:    None,
			},
			WorkspaceBufferSnapshot {
				path:       None,
				text:       "scratch".to_string(),
				clean_text: "scratch".to_string(),
				history:    Some(WorkspaceBufferHistorySnapshot {
					undo_stack: vec![BufferHistoryEntry {
						edits:         vec![BufferEditSnapshot {
							start_byte:    0,
							deleted_text:  String::new(),
							inserted_text: "scratch".to_string(),
						}],
						before_cursor: CursorState { row: 1, col: 1 },
						after_cursor:  CursorState { row: 1, col: 8 },
					}],
					redo_stack: Vec::new(),
				}),
			},
		],
		buffer_order:     vec![0, 1],
		tabs:             vec![WorkspaceTabSnapshot {
			windows:             vec![WorkspaceWindowSnapshot {
				buffer_index: Some(0),
				x:            0,
				y:            0,
				width:        80,
				height:       20,
				views:        vec![WorkspaceWindowBufferViewSnapshot {
					buffer_index: 0,
					cursor:       CursorState { row: 2, col: 3 },
					scroll_x:     1,
					scroll_y:     4,
				}],
			}],
			active_window_index: 0,
			buffer_order:        vec![0],
		}],
		active_tab_index: 0,
	};

	let flow = state
		.apply_action(&ports, AppAction::File(FileAction::WorkspaceSessionLoaded { result: Ok(Some(snapshot)) }));

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.workbench.status_bar.message, "session restored");
	assert_eq!(state.buffers.len(), 2);
	assert_eq!(ports.open_requests.borrow().len(), 1);
	assert_eq!(ports.watch_requests.borrow().len(), 1);
	assert_eq!(ports.swap_conflict_detects.borrow().len(), 1);
	assert!(ports.swap_recovers.borrow().is_empty());
	assert!(ports.initialize_bases.borrow().is_empty());
	assert_eq!(ports.history_loads.borrow().len(), 1);
	assert_eq!(ports.open_requests.borrow()[0].1, source_path);
	assert!(!ports.history_loads.borrow()[0].3);
	assert_eq!(state.active_cursor(), CursorState { row: 2, col: 3 });
	let active_window = state.windows.get(state.active_window_id()).expect("active window should exist");
	assert_eq!(active_window.scroll_x, 1);
	assert_eq!(active_window.scroll_y, 4);
	let scratch_buffer_id = state
		.buffers
		.iter()
		.find_map(|(buffer_id, buffer)| buffer.path.is_none().then_some(buffer_id))
		.expect("scratch buffer should exist");
	let scratch_buffer = state.buffers.get(scratch_buffer_id).expect("scratch buffer should exist");
	assert_eq!(scratch_buffer.undo_stack.len(), 1);
}

#[test]
fn workspace_session_loaded_none_should_create_untitled_buffer() {
	let mut state = RimState::new();

	let flow = dispatch_test_action(
		&mut state,
		AppAction::File(FileAction::WorkspaceSessionLoaded { result: Ok(None) }),
	);

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.buffers.len(), 1);
	assert_eq!(state.workbench.status_bar.message, "new file");
	assert!(state.active_buffer_id().is_some());
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
	assert_eq!(state.workbench.status_bar.message, "file saved");
}

#[test]
fn external_change_detected_should_reload_after_ignore_window_expires() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let path = PathBuf::from("a.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "old");
	state.bind_buffer_to_active_window(buffer_id);
	state.workbench.ignore_external_change_until.insert(buffer_id, Instant::now() - Duration::from_millis(1));

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
	assert_eq!(state.workbench.status_bar.message, "file saved");

	let _ = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::ExternalChangeDetected { buffer_id, path }),
	);
	assert_eq!(state.workbench.status_bar.message, "file saved");
	assert!(ports.external_loads.borrow().is_empty());
}

#[test]
fn external_change_detected_should_be_ignored_while_internal_save_in_flight() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let path = PathBuf::from("a.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "old");
	state.bind_buffer_to_active_window(buffer_id);
	state.workbench.in_flight_internal_saves.insert(buffer_id);

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
	assert_eq!(state.workbench.status_bar.message, "quit blocked: unsaved changes (use :q!)");
}

#[test]
fn command_qa_should_be_blocked_when_any_buffer_is_dirty() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abc");
	state.bind_buffer_to_active_window(buffer_id);
	state.set_buffer_dirty(buffer_id, true);
	state.enter_command_mode();
	state.push_command_char('q');
	state.push_command_char('a');

	let flow = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.workbench.status_bar.message, "quit all blocked: unsaved changes");
}

#[test]
fn command_qa_bang_should_force_quit_when_buffers_are_dirty() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "abc");
	state.bind_buffer_to_active_window(buffer_id);
	state.set_buffer_dirty(buffer_id, true);
	state.enter_command_mode();
	state.push_command_char('q');
	state.push_command_char('a');
	state.push_command_char('!');

	let flow = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Break(())));
}

#[test]
fn configured_command_alias_should_execute_registered_command() {
	let mut state = RimState::new();
	let errors = state.apply_command_config(&CommandConfigFile {
		command: CommandAliasSection {
			commands: vec![CommandAliasConfig {
				name: "qq".to_string(),
				run:  "core.quit_all".into(),
				args: Vec::new(),
				desc: Some("custom".to_string()),
			}],
		},
		..CommandConfigFile::default()
	});
	state.enter_command_mode();
	state.push_command_char('q');
	state.push_command_char('q');

	assert!(errors.is_empty());
	let flow = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Break(())));
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
fn command_q_bang_should_trim_dirty_text_from_session_snapshot() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let path = normalize_test_path("qbang_trim.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "clean");
	state.bind_buffer_to_active_window(buffer_id);
	state.replace_buffer_text_preserving_cursor(buffer_id, "dirty".to_string());
	state.set_buffer_dirty(buffer_id, true);
	state.enter_command_mode();
	state.push_command_char('q');
	state.push_command_char('!');

	let flow = state.apply_action(
		&ports,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Break(())));
	assert_eq!(ports.session_saves.borrow().len(), 1);
	let snapshot = ports.session_saves.borrow()[0].clone();
	let file_buffer = snapshot
		.buffers
		.iter()
		.find(|buffer| buffer.path.as_ref() == Some(&path))
		.expect("file-backed buffer should be saved");
	assert_eq!(file_buffer.text, file_buffer.clean_text);
	assert_eq!(file_buffer.text, "clean");
}

#[test]
fn command_qa_bang_should_trim_dirty_text_for_all_buffers_in_session_snapshot() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let file_path = normalize_test_path("qab_trim.txt");
	let file_buffer_id = state.create_buffer(Some(file_path.clone()), "file-clean");
	let untitled_buffer_id = state.create_buffer(None, "untitled-clean");
	state.bind_buffer_to_active_window(file_buffer_id);
	state.replace_buffer_text_preserving_cursor(file_buffer_id, "file-dirty".to_string());
	state.set_buffer_dirty(file_buffer_id, true);
	state.replace_buffer_text_preserving_cursor(untitled_buffer_id, "untitled-dirty".to_string());
	state.set_buffer_dirty(untitled_buffer_id, true);
	state.enter_command_mode();
	state.push_command_char('q');
	state.push_command_char('a');
	state.push_command_char('!');

	let flow = state.apply_action(
		&ports,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Break(())));
	assert_eq!(ports.session_saves.borrow().len(), 1);
	let snapshot = ports.session_saves.borrow()[0].clone();
	let file_buffer = snapshot
		.buffers
		.iter()
		.find(|buffer| buffer.path.as_ref() == Some(&file_path))
		.expect("file-backed buffer should be saved");
	assert_eq!(file_buffer.text, "file-clean");
	assert_eq!(file_buffer.text, file_buffer.clean_text);
	let untitled_buffer =
		snapshot.buffers.iter().find(|buffer| buffer.path.is_none()).expect("untitled buffer should be saved");
	assert_eq!(untitled_buffer.text, "untitled-clean");
	assert_eq!(untitled_buffer.text, untitled_buffer.clean_text);
}

#[test]
fn command_wq_should_break_after_save_completed_and_save_workspace_session() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let buffer_id = state.create_buffer(Some(PathBuf::from("a.txt")), "abc");
	state.bind_buffer_to_active_window(buffer_id);
	state.enter_command_mode();
	state.push_command_char('w');
	state.push_command_char('q');

	let flow = state.apply_action(
		&ports,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);
	assert!(matches!(flow, ControlFlow::Continue(())));
	assert!(state.workbench.quit_after_save);

	let flow = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id, result: Ok(()) }),
	);

	assert!(matches!(flow, ControlFlow::Break(())));
	assert_eq!(ports.session_saves.borrow().len(), 1);
}

#[test]
fn command_wqa_should_enqueue_save_all_and_quit_after_last_save() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let first = state.create_buffer(Some(PathBuf::from("a.txt")), "a");
	let second = state.create_buffer(Some(PathBuf::from("b.txt")), "b");
	state.bind_buffer_to_active_window(first);
	state.set_buffer_dirty(first, true);
	state.set_buffer_dirty(second, true);
	state.enter_command_mode();
	state.push_command_char('w');
	state.push_command_char('q');
	state.push_command_char('a');

	let flow = state.apply_action(
		&ports,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);
	assert!(matches!(flow, ControlFlow::Continue(())));
	assert!(state.workbench.quit_after_save);
	assert_eq!(ports.file_loads.borrow().len(), 0);
	assert_eq!(state.workbench.in_flight_internal_saves.len(), 2);

	let flow = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id: first, result: Ok(()) }),
	);
	assert!(matches!(flow, ControlFlow::Continue(())));
	assert!(state.workbench.quit_after_save);
	assert_eq!(ports.session_saves.borrow().len(), 0);

	let flow = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id: second, result: Ok(()) }),
	);
	assert!(matches!(flow, ControlFlow::Break(())));
	assert_eq!(ports.session_saves.borrow().len(), 1);
}

#[test]
fn command_wqa_should_be_blocked_when_any_buffer_has_no_path() {
	let mut state = RimState::new();
	let file_backed = state.create_buffer(Some(PathBuf::from("a.txt")), "a");
	let untitled = state.create_buffer(None, "b");
	state.bind_buffer_to_active_window(file_backed);
	state.set_buffer_dirty(file_backed, true);
	state.set_buffer_dirty(untitled, true);
	state.enter_command_mode();
	state.push_command_char('w');
	state.push_command_char('q');
	state.push_command_char('a');

	let flow = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.workbench.status_bar.message, "save all failed: 1 buffer(s) have no file path");
	assert!(!state.workbench.quit_after_save);
}

#[test]
fn command_wqa_should_be_blocked_when_any_buffer_was_changed_externally() {
	let mut state = RimState::new();
	let first = state.create_buffer(Some(PathBuf::from("a.txt")), "a");
	let second = state.create_buffer(Some(PathBuf::from("b.txt")), "b");
	state.bind_buffer_to_active_window(first);
	state.set_buffer_dirty(first, true);
	state.set_buffer_dirty(second, true);
	state.set_buffer_externally_modified(second, true);
	state.enter_command_mode();
	state.push_command_char('w');
	state.push_command_char('q');
	state.push_command_char('a');

	let flow = dispatch_test_action(
		&mut state,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(
		state.workbench.status_bar.message,
		"save all blocked: file changed externally (use :wqa! to overwrite)"
	);
	assert!(!state.workbench.quit_after_save);
}

#[test]
fn command_wqa_bang_should_force_save_all_and_quit() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let first = state.create_buffer(Some(PathBuf::from("a.txt")), "a");
	let second = state.create_buffer(Some(PathBuf::from("b.txt")), "b");
	state.bind_buffer_to_active_window(first);
	state.set_buffer_dirty(first, true);
	state.set_buffer_dirty(second, true);
	state.set_buffer_externally_modified(second, true);
	state.enter_command_mode();
	state.push_command_char('w');
	state.push_command_char('q');
	state.push_command_char('a');
	state.push_command_char('!');

	let flow = state.apply_action(
		&ports,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);
	assert!(matches!(flow, ControlFlow::Continue(())));
	assert!(state.workbench.quit_after_save);
	assert_eq!(state.workbench.in_flight_internal_saves.len(), 2);

	let _ = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id: first, result: Ok(()) }),
	);
	let flow = state.apply_action(
		&ports,
		AppAction::File(crate::action::FileAction::SaveCompleted { buffer_id: second, result: Ok(()) }),
	);

	assert!(matches!(flow, ControlFlow::Break(())));
	assert_eq!(ports.session_saves.borrow().len(), 1);
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
fn command_yazi_should_open_selected_file() {
	let mut state = RimState::new();
	register_yazi_plugin_command(&mut state);
	let ports = FilePickerPorts::default();
	ports.picked_path.replace(Some(PathBuf::from("Cargo.toml")));
	state.enter_command_mode();
	state.push_command_char('y');
	state.push_command_char('a');
	state.push_command_char('z');
	state.push_command_char('i');

	let flow = state.apply_action(
		&ports,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Continue(())));
	let flow = state.apply_action(
		&ports,
		AppAction::Plugin(PluginRuntimeAction::CommandCompleted {
			command_id: "plugin.yazi.yazi".to_string(),
			result:     Ok(PluginCommandResponse {
				effects: vec![PluginEffect::RequestAction(RequestedPluginAction::PickFile)],
			}),
		}),
	);
	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(ports.file_loads.borrow().len(), 1);
	assert_eq!(ports.file_loads.borrow()[0].1, normalize_test_path("Cargo.toml"));
}

#[test]
fn command_yazi_should_report_cancelled_when_no_file_is_selected() {
	let mut state = RimState::new();
	register_yazi_plugin_command(&mut state);
	let ports = FilePickerPorts::default();
	state.enter_command_mode();
	state.push_command_char('y');
	state.push_command_char('a');
	state.push_command_char('z');
	state.push_command_char('i');

	let flow = state.apply_action(
		&ports,
		AppAction::Editor(EditorAction::KeyPressed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))),
	);

	assert!(matches!(flow, ControlFlow::Continue(())));
	let flow = state.apply_action(
		&ports,
		AppAction::Plugin(PluginRuntimeAction::CommandCompleted {
			command_id: "plugin.yazi.yazi".to_string(),
			result:     Ok(PluginCommandResponse {
				effects: vec![PluginEffect::RequestAction(RequestedPluginAction::PickFile)],
			}),
		}),
	);
	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.workbench.status_bar.message, "open cancelled");
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
	assert_eq!(
		state.workbench.status_bar.message,
		"file changed externally; use :w! to overwrite or :e! to reload"
	);
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
	assert_eq!(
		state.workbench.status_bar.message,
		"save blocked: file changed externally (use :w! to overwrite)"
	);
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
	assert_eq!(state.workbench.status_bar.message, "reload blocked: buffer is dirty (use :e! to force reload)");
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
	assert_eq!(state.workbench.status_bar.message, format!("loading {}", path.display()));
}

#[test]
fn command_e_with_path_should_replace_clean_single_untitled_buffer() {
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
	assert_eq!(active_id, initial);
	let buffer = state.buffers.get(active_id).expect("buffer exists");
	let expected = normalize_test_path("b.txt");
	assert_eq!(buffer.path.as_deref(), Some(expected.as_path()));
	assert_eq!(state.workbench.status_bar.message, "new b.txt");
}

#[test]
fn open_requested_should_replace_clean_single_untitled_buffer_in_tab() {
	let mut state = RimState::new();
	state.create_untitled_buffer();
	let ports = RecordingPorts::default();
	let original_buffer_id = state.active_buffer_id().expect("active buffer should exist");
	let path = normalize_test_path("Cargo.toml");

	let flow = state.apply_action(&ports, AppAction::File(FileAction::OpenRequested { path: path.clone() }));

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.buffers.len(), 1);
	assert_eq!(state.active_buffer_id(), Some(original_buffer_id));
	let buffer = state.buffers.get(original_buffer_id).expect("buffer should exist");
	assert_eq!(buffer.path.as_ref(), Some(&path));
	assert_eq!(buffer.name, "Cargo.toml");
	assert_eq!(state.active_tab_buffer_ids(), vec![original_buffer_id]);
	assert_eq!(ports.file_loads.borrow().as_slice(), &[(original_buffer_id, path.clone())]);
	assert_eq!(ports.open_requests.borrow().as_slice(), &[(original_buffer_id, path.clone())]);
	assert_eq!(ports.watch_requests.borrow().as_slice(), &[(original_buffer_id, path)]);
}

#[test]
fn open_requested_should_drop_clean_single_untitled_when_switching_to_existing_buffer() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();
	let shared_path = normalize_test_path("shared.rs");
	let shared = state.create_buffer(Some(shared_path.clone()), "shared");
	let second_tab = state.open_new_tab();
	let untitled = state.active_buffer_id().expect("active buffer should exist");

	let flow =
		state.apply_action(&ports, AppAction::File(FileAction::OpenRequested { path: shared_path.clone() }));

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.active_tab, second_tab);
	assert_eq!(state.active_buffer_id(), Some(shared));
	assert!(!state.buffers.contains_key(untitled));
	assert_eq!(state.active_tab_buffer_ids(), vec![shared]);
	assert!(ports.file_loads.borrow().is_empty());
	assert!(ports.open_requests.borrow().is_empty());
	assert!(ports.watch_requests.borrow().is_empty());
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
	assert_eq!(state.workbench.status_bar.message, "switched b.txt");
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
	assert_eq!(state.workbench.status_bar.message, "switched b.txt");
	let expected = normalize_test_path("b.txt");
	let count =
		state.buffers.iter().filter(|(_, buffer)| buffer.path.as_deref() == Some(expected.as_path())).count();
	assert_eq!(count, 1);
}

#[test]
fn open_requested_should_enqueue_file_load() {
	let mut state = RimState::new();
	let ports = RecordingPorts::default();

	let path = PathBuf::from("Cargo.toml");
	let _ = state.apply_action(&ports, AppAction::File(crate::action::FileAction::OpenRequested { path }));

	assert_eq!(ports.file_loads.borrow().len(), 1);
}

#[test]
fn open_requested_should_prepare_new_buffer_when_path_missing() {
	let mut state = RimState::new();
	state.create_untitled_buffer();
	let ports = RecordingPorts::default();
	let nanos = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_nanos())
		.unwrap_or(0);
	let missing = normalize_test_path(format!("target-missing-{}.rs", nanos).as_str());

	let flow = state.apply_action(&ports, AppAction::File(FileAction::OpenRequested { path: missing.clone() }));

	assert!(matches!(flow, ControlFlow::Continue(())));
	let active = state.active_buffer_id().expect("active buffer should exist");
	let buffer = state.buffers.get(active).expect("buffer should exist");
	assert_eq!(buffer.path.as_ref(), Some(&missing));
	assert_eq!(buffer.text.to_string(), "");
	assert_eq!(state.workbench.status_bar.message, format!("new {}", missing.display()));
	assert!(ports.file_loads.borrow().is_empty());
	assert!(ports.open_requests.borrow().is_empty());
	assert!(ports.watch_requests.borrow().is_empty());
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
			restore_view: true,
			result: Ok(Some(history.clone())),
		}),
	);

	let buffer = state.buffers.get(buffer_id).expect("buffer should exist");
	assert_eq!(buffer.undo_stack, history.undo_stack);
	assert_eq!(buffer.redo_stack, history.redo_stack);
	assert_eq!(state.active_cursor(), history.cursor);
}

#[test]
fn undo_history_loaded_without_restore_view_should_preserve_current_cursor() {
	let mut state = RimState::new();
	let path = normalize_test_path("undo_restore_preserve_view.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "abc");
	state.bind_buffer_to_active_window(buffer_id);
	let active_window_id = state.active_window_id();
	{
		let window = state.windows.get_mut(active_window_id).expect("window should exist");
		window.cursor = CursorState { row: 1, col: 3 };
		window.scroll_y = 4;
	}
	state.sync_window_view_binding(active_window_id);
	let history = PersistedBufferHistory {
		current_text: "abc".to_string(),
		cursor:       CursorState { row: 1, col: 1 },
		undo_stack:   Vec::new(),
		redo_stack:   Vec::new(),
	};

	let _ = dispatch_test_action(
		&mut state,
		AppAction::File(FileAction::UndoHistoryLoaded {
			buffer_id,
			source_path: path,
			expected_text: "abc".to_string(),
			restore_view: false,
			result: Ok(Some(history)),
		}),
	);

	assert_eq!(state.active_cursor(), CursorState { row: 1, col: 3 });
	let window = state.windows.get(active_window_id).expect("window should exist");
	assert_eq!(window.scroll_y, 4);
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

	assert!(state.workbench.pending_swap_decision.is_none());
	assert_eq!(ports.swap_recovers.borrow().len(), 1);
	assert_eq!(state.workbench.status_bar.message, "recovering from swap...");
}

#[test]
fn swap_recover_completed_with_no_changes_should_update_status_message() {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(None, "base");
	state.bind_buffer_to_active_window(buffer_id);
	state.workbench.status_bar.message = "recovering from swap...".to_string();

	let _ = dispatch_test_action(
		&mut state,
		AppAction::File(FileAction::SwapRecoverCompleted { buffer_id, result: Ok(None) }),
	);

	assert_eq!(state.workbench.status_bar.message, "file reloaded");
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

	assert!(state.workbench.pending_swap_decision.is_none());
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
			result: Ok(SwapConflictCheckResult::Conflict(SwapConflictInfo {
				pid:      99,
				username: "other".to_string(),
			})),
		}),
	);

	let pending = state.workbench.pending_swap_decision.as_ref().expect("pending decision should exist");
	assert_eq!(pending.buffer_id, buffer_id);
	assert_eq!(pending.source_path, path);
	assert_eq!(pending.base_text, "base");
	assert_eq!(pending.owner_pid, 99);
	assert_eq!(pending.owner_username, "other");
	assert!(state.workbench.status_bar.message.contains("[r]ecover"));
}

#[test]
fn swap_conflict_detected_without_conflict_should_initialize_swap_base() {
	let mut state = RimState::new();
	let ports = SwapDecisionPorts::default();
	let path = normalize_test_path("swap_base_init.txt");
	let buffer_id = state.create_buffer(Some(path.clone()), "base-text");
	state.bind_buffer_to_active_window(buffer_id);

	let _ = state.apply_action(
		&ports,
		AppAction::File(FileAction::SwapConflictDetected {
			buffer_id,
			result: Ok(SwapConflictCheckResult::NoSwapActionNeeded),
		}),
	);

	assert_eq!(ports.swap_inits.borrow().len(), 1);
	let (init_buffer_id, init_path, init_text, delete_existing) = &ports.swap_inits.borrow()[0];
	assert_eq!(*init_buffer_id, buffer_id);
	assert_eq!(init_path, &path);
	assert_eq!(init_text, "base-text");
	assert!(!delete_existing);
}
