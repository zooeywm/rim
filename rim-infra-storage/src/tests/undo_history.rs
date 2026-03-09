use super::*;

#[test]
fn save_and_load_undo_history_should_roundtrip() {
	let undo_dir = make_tmp_dir("undo-roundtrip");
	let source_path = undo_dir.join("sample.txt");
	let mut undo_sessions = HashMap::new();
	let history = PersistedBufferHistory {
		current_text: "axbc".to_string(),
		cursor:       CursorState { row: 1, col: 3 },
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

	run_async(save_undo_history(undo_dir.as_path(), source_path.as_path(), &history, &mut undo_sessions))
		.expect("save undo history failed");
	assert!(path_exists(&undo_log_path_for_source(undo_dir.as_path(), source_path.as_path())));
	assert!(path_exists(&undo_meta_path_for_source(undo_dir.as_path(), source_path.as_path())));

	let loaded =
		run_async(load_undo_history(undo_dir.as_path(), source_path.as_path(), "axbc", &mut undo_sessions))
			.expect("load undo history failed")
			.expect("undo history should exist");

	assert_eq!(loaded, history);
}

#[test]
fn save_undo_history_should_truncate_redo_tail_before_appending_new_branch() {
	let undo_dir = make_tmp_dir("undo-branch-truncate");
	let source_path = undo_dir.join("sample.txt");
	let log_path = undo_log_path_for_source(undo_dir.as_path(), source_path.as_path());
	let mut undo_sessions = HashMap::new();
	let original = PersistedBufferHistory {
		current_text: "axbc".to_string(),
		cursor:       CursorState { row: 1, col: 3 },
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
	run_async(save_undo_history(undo_dir.as_path(), source_path.as_path(), &original, &mut undo_sessions))
		.expect("seed undo history failed");
	let log_len_before = metadata_len(&log_path);

	let branched = PersistedBufferHistory {
		current_text: "aybc".to_string(),
		cursor:       CursorState { row: 1, col: 3 },
		undo_stack:   vec![BufferHistoryEntry {
			edits:         vec![BufferEditSnapshot {
				start_byte:    1,
				deleted_text:  String::new(),
				inserted_text: "y".to_string(),
			}],
			before_cursor: CursorState { row: 1, col: 2 },
			after_cursor:  CursorState { row: 1, col: 3 },
		}],
		redo_stack:   Vec::new(),
	};
	run_async(save_undo_history(undo_dir.as_path(), source_path.as_path(), &branched, &mut undo_sessions))
		.expect("save branched undo history failed");

	let raw_log = read_to_string(&log_path);
	assert!(!raw_log.contains("\"x\""));
	assert!(raw_log.contains("\"y\""));
	let log_len_after = metadata_len(&log_path);
	assert!(log_len_after <= log_len_before);

	let loaded =
		run_async(load_undo_history(undo_dir.as_path(), source_path.as_path(), "aybc", &mut undo_sessions))
			.expect("load branched undo history failed")
			.expect("branched undo history should exist");
	assert_eq!(loaded, branched);
}

#[test]
fn save_undo_history_should_remove_file_when_history_is_empty() {
	let undo_dir = make_tmp_dir("undo-empty");
	let source_path = undo_dir.join("sample.txt");
	let undo_log_path = undo_log_path_for_source(undo_dir.as_path(), source_path.as_path());
	let undo_meta_path = undo_meta_path_for_source(undo_dir.as_path(), source_path.as_path());
	let mut undo_sessions = HashMap::new();
	let history = PersistedBufferHistory {
		current_text: "abc".to_string(),
		cursor:       CursorState { row: 1, col: 1 },
		undo_stack:   vec![BufferHistoryEntry {
			edits:         vec![BufferEditSnapshot {
				start_byte:    0,
				deleted_text:  String::new(),
				inserted_text: "a".to_string(),
			}],
			before_cursor: CursorState { row: 1, col: 1 },
			after_cursor:  CursorState { row: 1, col: 2 },
		}],
		redo_stack:   Vec::new(),
	};
	run_async(save_undo_history(undo_dir.as_path(), source_path.as_path(), &history, &mut undo_sessions))
		.expect("seed undo file failed");
	assert!(path_exists(&undo_log_path));
	assert!(path_exists(&undo_meta_path));

	run_async(save_undo_history(
		undo_dir.as_path(),
		source_path.as_path(),
		&PersistedBufferHistory {
			current_text: "abc".to_string(),
			cursor:       CursorState { row: 1, col: 1 },
			undo_stack:   Vec::new(),
			redo_stack:   Vec::new(),
		},
		&mut undo_sessions,
	))
	.expect("clear undo file failed");

	assert!(!path_exists(&undo_log_path));
	assert!(!path_exists(&undo_meta_path));
}
