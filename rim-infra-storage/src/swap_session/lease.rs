use std::path::Path;
#[cfg(unix)]
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use tracing::error;

use crate::path_codec::swap_lease_file_prefix;

pub(crate) async fn touch_swap_lease_file(lease_path: &Path) -> Result<()> {
	if let Some(parent) = lease_path.parent() {
		compio::fs::create_dir_all(parent)
			.await
			.with_context(|| format!("create lease dir failed: {}", parent.display()))?;
	}
	compio::fs::write(lease_path, std::process::id().to_string())
		.await
		.0
		.with_context(|| format!("write lease file failed: {}", lease_path.display()))?;
	Ok(())
}

pub(super) async fn remove_swap_lease_file(lease_path: &Path) {
	if let Err(err) = compio::fs::remove_file(lease_path).await
		&& err.kind() != std::io::ErrorKind::NotFound
	{
		error!("remove lease file failed: {} error={}", lease_path.display(), err);
	}
}

pub(super) async fn has_other_swap_leases(self_lease_path: &Path, source_path: &Path) -> bool {
	let Some(lease_dir) = self_lease_path.parent() else {
		return true;
	};
	let self_lease_name =
		self_lease_path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
	let lease_prefix = swap_lease_file_prefix(source_path);

	let entries = match compio::runtime::spawn_blocking({
		let lease_dir = lease_dir.to_path_buf();
		move || {
			std::fs::read_dir(lease_dir)
				.map(|entries| entries.filter_map(|entry| entry.ok().map(|entry| entry.path())).collect::<Vec<_>>())
		}
	})
	.await
	{
		Ok(Ok(entries)) => entries,
		Ok(Err(err)) => {
			error!("read lease dir failed: {} error={}", lease_dir.display(), err);
			return true;
		}
		Err(err) => {
			error!("read lease dir task failed: {} error={:?}", lease_dir.display(), err);
			return true;
		}
	};

	for lease_path in entries {
		let Some(file_name) = lease_path.file_name() else {
			continue;
		};
		let file_name = file_name.to_string_lossy();
		if !file_name.starts_with(lease_prefix.as_str()) || !file_name.ends_with(".lease") {
			continue;
		}
		if file_name == self_lease_name {
			continue;
		}
		let Some(pid) = parse_pid_from_lease_name(file_name.as_ref(), lease_prefix.as_str()) else {
			return true;
		};
		if is_process_alive(pid) {
			return true;
		}
		remove_swap_lease_file(lease_path.as_path()).await;
	}

	false
}

fn parse_pid_from_lease_name(file_name: &str, prefix: &str) -> Option<u32> {
	if !file_name.starts_with(prefix) || !file_name.ends_with(".lease") {
		return None;
	}
	let pid_text = file_name.strip_prefix(prefix)?.strip_suffix(".lease")?;
	pid_text.parse::<u32>().ok()
}

fn is_process_alive(pid: u32) -> bool {
	#[cfg(unix)]
	{
		Command::new("kill")
			.arg("-0")
			.arg(pid.to_string())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.status()
			.map(|status| status.success())
			.unwrap_or(true)
	}

	#[cfg(not(unix))]
	{
		let _ = pid;
		true
	}
}
