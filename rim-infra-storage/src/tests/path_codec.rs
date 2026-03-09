use super::*;

#[test]
fn parse_swap_file_should_read_metadata() {
	let swap_dir = make_tmp_dir("meta");
	let source_path = swap_dir.join("sample.txt");
	let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());
	run_async(write_swap_snapshot(swap_path.as_path(), 77, "user-a", false, "base"))
		.expect("write snapshot failed");

	let parsed = run_async(parse_swap_file(swap_path.as_path())).expect("parse swap file failed");
	assert_eq!(parsed.pid, 77);
	assert_eq!(parsed.username, "user-a");
	assert_eq!(parsed.source_path, source_path);
	assert!(!parsed.dirty);
	assert_eq!(parsed.base_text, "base");
}

#[test]
fn swap_path_should_embed_source_path_components() {
	let swap_dir = make_tmp_dir("path-layout");
	let source_path = swap_dir.join("nested").join("sample.txt");
	let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());
	let relative = swap_path.strip_prefix(&swap_dir).expect("swap path should stay under swap dir");

	assert_eq!(relative.components().count(), 1);
	let file_name = relative.file_name().and_then(|name| name.to_str()).expect("swap file name should exist");
	assert!(file_name.ends_with(".swp"));
	assert!(file_name.contains("_nested_sample.txt"));
	assert!(!file_name.contains(':'));
}

#[test]
fn encode_windows_source_path_should_use_readable_drive_prefix() {
	let encoded = encode_source_path_for_file_name(PathBuf::from(r"C:\Users\tester\sample.txt").as_path());

	assert_eq!(encoded, r"C_Users_tester_sample.txt");
	assert!(!encoded.contains(':'));
}

#[test]
fn encode_windows_source_path_should_use_readable_network_share_prefix() {
	let encoded = encode_source_path_for_file_name(PathBuf::from(r"\\server\share\sample.txt").as_path());

	assert_eq!(encoded, r"_server_share_sample.txt");
	assert!(!encoded.contains(':'));
}

#[test]
fn encode_windows_source_path_should_strip_extended_drive_prefix() {
	let normalized = normalize_source_path_text(r"\\?\C:\Users\tester\sample.txt");
	let encoded = encode_source_path_for_file_name(PathBuf::from(normalized.as_ref()).as_path());

	assert_eq!(encoded, r"C_Users_tester_sample.txt");
	assert!(!encoded.contains('?'));
	assert!(!encoded.contains(':'));
}

#[cfg(target_os = "windows")]
#[test]
fn source_path_from_swap_storage_path_should_decode_windows_readable_drive_encoding() {
	let swap_path = PathBuf::from(r"C_Users_tester_sample.txt.swp");

	let decoded = source_path_from_swap_storage_path(swap_path.as_path())
		.expect("decode windows readable swap path failed");

	assert_eq!(decoded, PathBuf::from(r"C:\Users\tester\sample.txt"));
}

#[cfg(target_os = "windows")]
#[test]
fn source_path_from_swap_storage_path_should_decode_windows_readable_network_share_encoding() {
	let swap_path = PathBuf::from(r"_server_share_sample.txt.swp");

	let decoded = source_path_from_swap_storage_path(swap_path.as_path())
		.expect("decode windows readable network share path failed");

	assert_eq!(decoded, PathBuf::from(r"\\server\share\sample.txt"));
}

#[test]
fn swap_file_should_use_readable_json_escaped_fields() {
	let swap_dir = make_tmp_dir("readable");
	let source_path = swap_dir.join("sample.txt");
	let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());

	run_async(write_swap_snapshot(swap_path.as_path(), 42, "tester", true, "a\nb\t中"))
		.expect("write snapshot failed");
	run_async(append_swap_ops(swap_path.as_path(), &[
		SwapEditOp::Insert { pos: 3, text: "xy\n\t".to_string() },
		SwapEditOp::Delete { pos: 1, len: 1 },
	]))
	.expect("append ops failed");

	let raw = read_to_string(&swap_path);
	assert!(raw.contains("META\tpid=42\tuser=\"tester\"\tdirty=1"));
	assert!(raw.lines().any(|line| line.starts_with("BASE\t\"")));
	assert!(raw.lines().any(|line| line.starts_with("I\t3\t\"")));
	assert!(raw.lines().any(|line| line == "D\t1\t1"));
}

#[test]
fn parse_swap_file_should_support_legacy_base64_format() {
	let swap_dir = make_tmp_dir("legacy");
	let source_path = swap_dir.join("sample.txt");
	let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());
	create_dir_all(swap_path.parent().expect("legacy swap parent should exist"));

	let legacy = format!(
		"{}\nMETA\t7\t{}\t{}\t1\nBASE\t{}\nI\t3\t{}\nD\t1\t1\n",
		SWAP_FILE_MAGIC,
		STANDARD_NO_PAD.encode("legacy-user".as_bytes()),
		STANDARD_NO_PAD.encode(source_path.display().to_string().as_bytes()),
		STANDARD_NO_PAD.encode("abc".as_bytes()),
		STANDARD_NO_PAD.encode("Z".as_bytes()),
	);
	write_string(&swap_path, legacy);

	let parsed = run_async(parse_swap_file(swap_path.as_path())).expect("parse legacy swap failed");
	assert_eq!(parsed.pid, 7);
	assert_eq!(parsed.username, "legacy-user");
	assert_eq!(parsed.source_path, source_path);
	assert!(parsed.dirty);
	assert_eq!(parsed.base_text, "abc");
	assert_eq!(parsed.ops, vec![SwapEditOp::Insert { pos: 3, text: "Z".to_string() }, SwapEditOp::Delete {
		pos: 1,
		len: 1,
	}]);
}
