use std::path::PathBuf;

use rim_kernel::state::{CursorState, WorkspaceBufferSnapshot, WorkspaceSessionSnapshot, WorkspaceTabSnapshot, WorkspaceWindowBufferViewSnapshot, WorkspaceWindowSnapshot};

use super::{create_dir_all, make_tmp_dir, path_exists, read_to_string, run_async};
use crate::session::{load_workspace_session, save_workspace_session};

#[test]
fn workspace_session_should_roundtrip_on_disk() {
	let session_dir = make_tmp_dir("workspace-session");
	create_dir_all(session_dir.as_path());
	let snapshot = WorkspaceSessionSnapshot {
		version:          1,
		buffers:          vec![WorkspaceBufferSnapshot {
			path:       Some(PathBuf::from("sample.rs")),
			text:       "fn main() {}\n".to_string(),
			clean_text: "fn main() {}\n".to_string(),
			undo_stack: Vec::new(),
			redo_stack: Vec::new(),
		}],
		buffer_order:     vec![0],
		tabs:             vec![WorkspaceTabSnapshot {
			windows:             vec![WorkspaceWindowSnapshot {
				buffer_index: Some(0),
				x:            0,
				y:            0,
				width:        120,
				height:       30,
				views:        vec![WorkspaceWindowBufferViewSnapshot {
					buffer_index: 0,
					cursor:       CursorState { row: 3, col: 5 },
					scroll_x:     2,
					scroll_y:     7,
				}],
			}],
			active_window_index: 0,
		}],
		active_tab_index: 0,
	};

	run_async(async {
		save_workspace_session(session_dir.as_path(), &snapshot)
			.await
			.expect("save workspace session should succeed");
	});

	let session_path = session_dir.join("last-session.json");
	assert!(path_exists(session_path.as_path()));
	assert!(read_to_string(session_path.as_path()).contains("\"version\": 1"));

	let restored = run_async(async {
		load_workspace_session(session_dir.as_path()).await.expect("load workspace session should succeed")
	});

	assert_eq!(restored, Some(snapshot));
}
