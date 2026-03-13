use super::*;

#[test]
fn apply_edit_should_merge_adjacent_insert_ops_within_window() {
	let swap_dir = make_tmp_dir("merge");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	session.rope = Rope::from_str("abc");
	run_async(session.ensure_snapshot_initialized()).expect("snapshot init failed");

	let now = Instant::now();
	run_async(session.apply_edit(SwapEditOp::Insert { pos: 1, text: "x".to_string() }, now))
		.expect("first edit failed");
	run_async(
		session.apply_edit(SwapEditOp::Insert { pos: 2, text: "y".to_string() }, now + Duration::from_millis(80)),
	)
	.expect("second edit failed");

	assert_eq!(session.pending_ops.len(), 1);
	assert_eq!(buffered_ops_to_plain(&session.pending_ops), vec![SwapEditOp::Insert {
		pos:  1,
		text: "xy".to_string(),
	}]);
}

#[test]
fn apply_edit_should_cancel_pending_insert_when_delete_reverts_it() {
	let swap_dir = make_tmp_dir("cancel-insert");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(session.recover("abc".to_string())).expect("recover init failed");

	let now = Instant::now();
	run_async(session.apply_edit(SwapEditOp::Insert { pos: 1, text: "xy".to_string() }, now))
		.expect("insert failed");
	run_async(session.apply_edit(SwapEditOp::Delete { pos: 1, len: 2 }, now + Duration::from_millis(40)))
		.expect("delete failed");

	assert!(session.pending_ops.is_empty());
	assert_eq!(session.rope.to_string(), "abc");
	assert!(!session.dirty);

	session.flush_if_due(now + Duration::from_millis(300)).expect("flush failed");
	let parsed = run_async(parse_swap_file(session.swap_path.as_path())).expect("parse swap failed");
	assert!(parsed.ops.is_empty());
	assert!(!parsed.dirty);
}

#[test]
fn apply_edit_should_shrink_pending_insert_when_delete_removes_part_of_it() {
	let swap_dir = make_tmp_dir("shrink-insert");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(session.recover("abc".to_string())).expect("recover init failed");

	let now = Instant::now();
	run_async(session.apply_edit(SwapEditOp::Insert { pos: 1, text: "xyz".to_string() }, now))
		.expect("insert failed");
	run_async(session.apply_edit(SwapEditOp::Delete { pos: 2, len: 1 }, now + Duration::from_millis(40)))
		.expect("delete failed");

	assert_eq!(session.rope.to_string(), "axzbc");
	assert_eq!(buffered_ops_to_plain(&session.pending_ops), vec![SwapEditOp::Insert {
		pos:  1,
		text: "xz".to_string(),
	}]);
	assert!(session.dirty);
}

#[test]
fn apply_edit_should_cancel_pending_delete_when_insert_reverts_it() {
	let swap_dir = make_tmp_dir("cancel-delete");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(session.recover("abc".to_string())).expect("recover init failed");

	let now = Instant::now();
	run_async(session.apply_edit(SwapEditOp::Delete { pos: 1, len: 1 }, now)).expect("delete failed");
	run_async(
		session.apply_edit(SwapEditOp::Insert { pos: 1, text: "b".to_string() }, now + Duration::from_millis(40)),
	)
	.expect("insert failed");

	assert!(session.pending_ops.is_empty());
	assert_eq!(session.rope.to_string(), "abc");
	assert!(!session.dirty);

	session.flush_if_due(now + Duration::from_millis(300)).expect("flush failed");
	let parsed = run_async(parse_swap_file(session.swap_path.as_path())).expect("parse swap failed");
	assert!(parsed.ops.is_empty());
	assert!(!parsed.dirty);
}

#[test]
fn flush_pending_should_remove_logged_insert_when_later_delete_reverts_it() {
	let swap_dir = make_tmp_dir("rewrite-logged-insert");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(session.recover("abc".to_string())).expect("recover init failed");

	let now = Instant::now();
	run_async(session.apply_edit(SwapEditOp::Insert { pos: 1, text: "x".to_string() }, now))
		.expect("insert failed");
	session.flush_if_due(now + Duration::from_millis(300)).expect("first flush failed");

	let parsed_after_insert =
		run_async(parse_swap_file(session.swap_path.as_path())).expect("parse after insert failed");
	assert_eq!(parsed_after_insert.ops, vec![SwapEditOp::Insert { pos: 1, text: "x".to_string() }]);
	assert!(parsed_after_insert.dirty || !parsed_after_insert.ops.is_empty());
	assert_eq!(session.logged_end_offsets.len(), 1);
	let logged_len_after_insert = metadata_len(session.swap_path.as_path());
	assert!(logged_len_after_insert > session.snapshot_len);

	run_async(session.apply_edit(SwapEditOp::Delete { pos: 1, len: 1 }, now + Duration::from_millis(400)))
		.expect("delete failed");
	session.flush_if_due(now + Duration::from_millis(700)).expect("second flush failed");

	let parsed_after_delete =
		run_async(parse_swap_file(session.swap_path.as_path())).expect("parse after delete failed");
	assert!(parsed_after_delete.ops.is_empty());
	assert!(!parsed_after_delete.dirty);
	assert_eq!(session.rope.to_string(), "abc");
	assert!(!session.dirty);
	assert!(session.logged_end_offsets.is_empty());
	let logged_len_after_delete = metadata_len(session.swap_path.as_path());
	assert_eq!(logged_len_after_delete, session.snapshot_len);
}

#[test]
fn flush_pending_should_remove_logged_delete_when_later_insert_reverts_it() {
	let swap_dir = make_tmp_dir("rewrite-logged-delete");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(session.recover("abc".to_string())).expect("recover init failed");

	let now = Instant::now();
	run_async(session.apply_edit(SwapEditOp::Delete { pos: 1, len: 1 }, now)).expect("delete failed");
	session.flush_if_due(now + Duration::from_millis(300)).expect("first flush failed");

	let parsed_after_delete =
		run_async(parse_swap_file(session.swap_path.as_path())).expect("parse after delete failed");
	assert_eq!(parsed_after_delete.ops, vec![SwapEditOp::Delete { pos: 1, len: 1 }]);
	assert!(parsed_after_delete.dirty || !parsed_after_delete.ops.is_empty());

	run_async(
		session
			.apply_edit(SwapEditOp::Insert { pos: 1, text: "b".to_string() }, now + Duration::from_millis(400)),
	)
	.expect("insert failed");
	session.flush_if_due(now + Duration::from_millis(700)).expect("second flush failed");

	let parsed_after_insert =
		run_async(parse_swap_file(session.swap_path.as_path())).expect("parse after insert failed");
	assert!(parsed_after_insert.ops.is_empty());
	assert!(!parsed_after_insert.dirty);
	assert_eq!(session.rope.to_string(), "abc");
	assert!(!session.dirty);
}

#[test]
fn flush_pending_should_remove_logged_block_insert_batch_when_undo_emits_multiple_deletes() {
	let swap_dir = make_tmp_dir("rewrite-logged-block-insert");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(session.recover("abc\ndef".to_string())).expect("recover init failed");

	let now = Instant::now();
	run_async(session.apply_edit(SwapEditOp::Insert { pos: 1, text: "X".to_string() }, now))
		.expect("first insert failed");
	run_async(
		session.apply_edit(SwapEditOp::Insert { pos: 6, text: "X".to_string() }, now + Duration::from_millis(20)),
	)
	.expect("second insert failed");
	session.flush_if_due(now + Duration::from_millis(300)).expect("first flush failed");

	let parsed_after_insert =
		run_async(parse_swap_file(session.swap_path.as_path())).expect("parse after insert failed");
	assert_eq!(parsed_after_insert.ops, vec![
		SwapEditOp::Insert { pos: 1, text: "X".to_string() },
		SwapEditOp::Insert { pos: 6, text: "X".to_string() },
	]);

	run_async(session.apply_edit(SwapEditOp::Delete { pos: 1, len: 1 }, now + Duration::from_millis(400)))
		.expect("first delete failed");
	run_async(session.apply_edit(SwapEditOp::Delete { pos: 5, len: 1 }, now + Duration::from_millis(420)))
		.expect("second delete failed");
	session.flush_if_due(now + Duration::from_millis(700)).expect("second flush failed");

	let parsed_after_undo =
		run_async(parse_swap_file(session.swap_path.as_path())).expect("parse after undo failed");
	assert!(parsed_after_undo.ops.is_empty());
	assert!(!parsed_after_undo.dirty);
	assert_eq!(session.rope.to_string(), "abc\ndef");
	assert!(!session.dirty);
}

#[test]
fn flush_if_due_should_only_flush_after_debounce_window() {
	let swap_dir = make_tmp_dir("debounce");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(session.recover("abc".to_string())).expect("recover init failed");

	let now = Instant::now();
	run_async(session.apply_edit(SwapEditOp::Insert { pos: 3, text: "!".to_string() }, now))
		.expect("apply edit failed");

	session.flush_if_due(now + Duration::from_millis(60)).expect("early flush failed");
	let parsed_before =
		run_async(parse_swap_file(session.swap_path.as_path())).expect("parse before flush failed");
	assert!(parsed_before.ops.is_empty());

	session.flush_if_due(now + Duration::from_millis(300)).expect("flush after debounce failed");
	let parsed_after =
		run_async(parse_swap_file(session.swap_path.as_path())).expect("parse after flush failed");
	assert_eq!(parsed_after.ops, vec![SwapEditOp::Insert { pos: 3, text: "!".to_string() }]);
}

#[test]
fn recover_should_replay_existing_swap_edit_log() {
	let swap_dir = make_tmp_dir("recover");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);

	run_async(write_swap_snapshot(session.swap_path.as_path(), 999, "old-user", true, "abc"))
		.expect("write test snapshot failed");
	run_async(append_swap_ops(session.swap_path.as_path(), &[
		SwapEditOp::Delete { pos: 1, len: 1 },
		SwapEditOp::Insert { pos: 2, text: "Z".to_string() },
	]))
	.expect("append test swap ops failed");

	let recovered = run_async(session.recover("abc".to_string())).expect("recover failed");
	assert_eq!(recovered, Some("acZ".to_string()));
}

#[test]
fn detect_conflict_should_ignore_swap_owned_by_current_process() {
	let swap_dir = make_tmp_dir("conflict-self");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(write_swap_snapshot(session.swap_path.as_path(), 123, "tester", true, "abc"))
		.expect("write test snapshot failed");

	let conflict = run_async(session.detect_conflict()).expect("detect conflict failed");
	assert_eq!(conflict, SwapConflictCheckResult::NoSwapActionNeeded);
}

#[test]
fn detect_conflict_should_report_swap_owned_by_other_process() {
	let swap_dir = make_tmp_dir("conflict-peer");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	let stale_peer_pid = 999_999;
	run_async(write_swap_snapshot(session.swap_path.as_path(), stale_peer_pid, "peer", true, "abc"))
		.expect("write test snapshot failed");

	let conflict = run_async(session.detect_conflict()).expect("detect conflict failed");
	assert_eq!(
		conflict,
		SwapConflictCheckResult::Conflict(SwapConflictInfo {
			pid:      stale_peer_pid,
			username: "peer".to_string(),
		})
	);
}

#[test]
fn detect_conflict_should_ignore_swap_owned_by_alive_other_process() {
	let swap_dir = make_tmp_dir("conflict-peer-alive");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	let alive_peer_pid = std::process::id();
	run_async(write_swap_snapshot(session.swap_path.as_path(), alive_peer_pid, "peer", true, "abc"))
		.expect("write test snapshot failed");

	let conflict = run_async(session.detect_conflict()).expect("detect conflict failed");
	assert_eq!(
		conflict,
		SwapConflictCheckResult::Conflict(SwapConflictInfo {
			pid:      alive_peer_pid,
			username: "peer".to_string(),
		})
	);
}

#[test]
fn close_should_remove_swap_file() {
	let swap_dir = make_tmp_dir("drop");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(session.recover("hello".to_string())).expect("recover init failed");
	let swap_path = session.swap_path.clone();
	assert!(path_exists(&swap_path));

	run_async(session.close()).expect("close session failed");
	assert!(!path_exists(&swap_path));
}

#[test]
fn swap_io_state_drop_should_shutdown_worker_and_cleanup_swap_file() {
	let swap_dir = make_tmp_dir("state-drop");
	let source_path = swap_dir.join("sample.txt");
	let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());
	let (event_tx, _event_rx) = flume::unbounded();
	let mut state = StorageIoState::new(event_tx);
	state.swap_dir = swap_dir;
	state.start();

	state
		.request_tx
		.send(StorageIoRequest::Open { buffer_id: BufferId::default(), source_path: source_path.clone() })
		.expect("send open failed");
	state
		.request_tx
		.send(StorageIoRequest::InitializeBase {
			buffer_id: BufferId::default(),
			source_path,
			base_text: "hello".to_string(),
			delete_existing: false,
		})
		.expect("send initialize base failed");
	for _ in 0..50 {
		if path_exists(&swap_path) {
			break;
		}
		std::thread::sleep(Duration::from_millis(20));
	}
	assert!(path_exists(&swap_path));

	drop(state);
	assert!(!path_exists(&swap_path));
}

#[test]
fn recover_without_existing_swap_should_not_emit_recover_completed_event() {
	let swap_dir = make_tmp_dir("recover-no-swap-event");
	let source_path = swap_dir.join("sample.txt");
	let (event_tx, event_rx) = flume::unbounded();
	let mut state = StorageIoState::new(event_tx);
	state.swap_dir = swap_dir;
	state.start();

	state
		.request_tx
		.send(StorageIoRequest::Recover {
			buffer_id: BufferId::default(),
			source_path,
			base_text: "hello".to_string(),
		})
		.expect("send recover failed");

	let result = event_rx.recv_timeout(Duration::from_millis(200));
	assert!(matches!(result, Err(flume::RecvTimeoutError::Timeout)));
}

#[test]
fn close_should_keep_swap_file_when_owned_by_other_process() {
	let swap_dir = make_tmp_dir("drop-foreign-owner");
	let source_path = swap_dir.join("sample.txt");
	let mut session = SwapSession::new(
		BufferId::default(),
		source_path.as_path(),
		swap_dir.as_path(),
		123,
		"tester".to_string(),
	);
	run_async(write_swap_snapshot(session.swap_path.as_path(), 456, "peer", true, "hello"))
		.expect("write swap snapshot failed");
	let swap_path = session.swap_path.clone();
	assert!(path_exists(&swap_path));

	run_async(session.close()).expect("close session failed");
	assert!(path_exists(&swap_path));
	remove_file(&swap_path);
}
