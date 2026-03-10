use std::{collections::HashMap, future::Future, path::{Path, PathBuf}, time::{Instant, SystemTime, UNIX_EPOCH}};

use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use rim_kernel::state::{BufferEditSnapshot, BufferHistoryEntry, CursorState};
use ropey::Rope;

use super::*;
#[cfg(target_os = "windows")]
use crate::path_codec::source_path_from_swap_storage_path;
use crate::{path_codec::{encode_source_path_for_file_name, normalize_source_path_text, swap_lease_path_for_source, swap_path_for_source, undo_log_path_for_source, undo_meta_path_for_source}, swap_session::{BufferedSwapOp, SwapSession, append_swap_ops, parse_swap_file, touch_swap_lease_file, write_swap_snapshot}, undo_history::{load_undo_history, save_undo_history}};

mod path_codec;
mod session;
mod swap_session;
mod undo_history;

fn run_async<Output>(future: impl Future<Output = Output>) -> Output { block_on_test(future) }

fn create_dir_all(path: &Path) {
	run_async(async {
		compio::fs::create_dir_all(path).await.expect("create test dir failed");
	});
}

fn path_exists(path: &Path) -> bool { run_async(async { compio::fs::metadata(path).await.is_ok() }) }

fn metadata_len(path: &Path) -> u64 {
	run_async(async { compio::fs::metadata(path).await.expect("stat file failed").len() })
}

fn read_to_string(path: &Path) -> String {
	let bytes = run_async(async { compio::fs::read(path).await.expect("read file failed") });
	String::from_utf8(bytes).expect("test file should be utf-8")
}

fn write_string(path: &Path, text: String) {
	run_async(async {
		compio::fs::write(path, text).await.0.expect("write test file failed");
	});
}

fn remove_file(path: &Path) {
	run_async(async {
		compio::fs::remove_file(path).await.expect("remove test file failed");
	});
}

fn make_tmp_dir(test_name: &str) -> PathBuf {
	let nanos =
		SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock before unix epoch").as_nanos();
	let dir =
		std::env::temp_dir().join(format!("rim-swap-test-{}-{}-{}", test_name, std::process::id(), nanos));
	create_dir_all(&dir);
	dir
}

fn buffered_ops_to_plain(ops: &[BufferedSwapOp]) -> Vec<SwapEditOp> {
	ops.iter().map(|op| op.op.clone()).collect()
}
