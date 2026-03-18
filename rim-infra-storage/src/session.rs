use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rim_domain::model::WorkspaceSessionSnapshot;

const WORKSPACE_SESSION_FILE_NAME: &str = "last-session.json";

pub(crate) async fn load_workspace_session(session_dir: &Path) -> Result<Option<WorkspaceSessionSnapshot>> {
	let session_path = workspace_session_path(session_dir);
	let session_bytes = match compio::fs::read(session_path.as_path()).await {
		Ok(bytes) => bytes,
		Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
		Err(err) => {
			return Err(err).with_context(|| format!("read workspace session failed: {}", session_path.display()));
		}
	};
	let snapshot = serde_json::from_slice::<WorkspaceSessionSnapshot>(session_bytes.as_slice())
		.with_context(|| format!("decode workspace session failed: {}", session_path.display()))?;
	Ok(Some(snapshot))
}

pub(crate) async fn save_workspace_session(
	session_dir: &Path,
	snapshot: &WorkspaceSessionSnapshot,
) -> Result<()> {
	let session_path = workspace_session_path(session_dir);
	let encoded = serde_json::to_vec_pretty(snapshot)
		.with_context(|| format!("encode workspace session failed: {}", session_path.display()))?;
	compio::fs::write(session_path.as_path(), encoded)
		.await
		.0
		.with_context(|| format!("write workspace session failed: {}", session_path.display()))?;
	Ok(())
}

fn workspace_session_path(session_dir: &Path) -> PathBuf { session_dir.join(WORKSPACE_SESSION_FILE_NAME) }
