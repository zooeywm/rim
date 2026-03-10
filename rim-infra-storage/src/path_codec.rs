use std::{borrow::Cow, path::{Path, PathBuf}};

use anyhow::{Result, anyhow};
use rim_paths::user_state_root;

pub(super) fn swap_path_for_source(swap_dir: &Path, source_path: &Path) -> PathBuf {
	swap_dir.join(format!("{}.swp", encode_source_path_for_file_name(source_path)))
}

pub(super) fn undo_log_path_for_source(undo_dir: &Path, source_path: &Path) -> PathBuf {
	undo_dir.join(format!("{}.undo.log", encode_source_path_for_file_name(source_path)))
}

pub(super) fn undo_meta_path_for_source(undo_dir: &Path, source_path: &Path) -> PathBuf {
	undo_dir.join(format!("{}.undo.meta", encode_source_path_for_file_name(source_path)))
}

pub(super) fn undo_legacy_path_for_source(undo_dir: &Path, source_path: &Path) -> PathBuf {
	undo_dir.join(format!("{}.undo", encode_source_path_for_file_name(source_path)))
}

pub(super) fn swap_lease_path_for_source(swap_dir: &Path, source_path: &Path, pid: u32) -> PathBuf {
	swap_dir.join(format!("{}.{}.lease", encode_source_path_for_file_name(source_path), pid))
}

pub(super) fn encode_source_path_for_file_name(source_path: &Path) -> String {
	let normalized = normalize_source_path_for_persistence(source_path);
	let raw = normalized.to_string_lossy();
	if raw.is_empty() {
		return "buffer".to_string();
	}

	let mut encoded = String::with_capacity(raw.len());
	let mut last_was_path_syntax = false;
	for ch in raw.chars() {
		if ch == '_' {
			encoded.push_str("__");
			last_was_path_syntax = false;
			continue;
		}
		if is_path_syntax_char(ch) {
			if !last_was_path_syntax {
				encoded.push('_');
				last_was_path_syntax = true;
			}
			continue;
		}
		encoded.push(ch);
		last_was_path_syntax = false;
	}
	encoded
}

fn is_path_syntax_char(ch: char) -> bool {
	matches!(ch, '/' | '\\' | ':' | '?' | '*' | '"' | '<' | '>' | '|')
}

pub(super) fn normalize_source_path_for_persistence(source_path: &Path) -> PathBuf {
	let rendered = source_path.to_string_lossy();
	let normalized = normalize_source_path_text(rendered.as_ref());
	PathBuf::from(normalized.as_ref())
}

pub(super) fn normalize_source_path_text(raw: &str) -> Cow<'_, str> {
	if let Some(remainder) = raw.strip_prefix(r"\\?\").or_else(|| raw.strip_prefix(r"//?/")) {
		return Cow::Borrowed(remainder);
	}

	Cow::Borrowed(raw)
}

pub(super) fn swap_lease_file_prefix(source_path: &Path) -> String {
	format!("{}.", encode_source_path_for_file_name(source_path))
}

pub(super) fn source_path_from_swap_storage_path(storage_path: &Path) -> Result<PathBuf> {
	source_path_from_flat_swap_storage_path(storage_path)
}

fn source_path_from_flat_swap_storage_path(storage_path: &Path) -> Result<PathBuf> {
	let file_name = storage_path
		.file_name()
		.and_then(|name| name.to_str())
		.ok_or_else(|| anyhow!("swap path missing file name: {}", storage_path.display()))?;

	let encoded = if let Some(encoded) = file_name.strip_suffix(".swp") {
		encoded
	} else if let Some(without_lease) = file_name.strip_suffix(".lease") {
		without_lease
			.rsplit_once('.')
			.map(|(stem, _pid)| stem)
			.ok_or_else(|| anyhow!("invalid lease file name: {}", storage_path.display()))?
	} else {
		file_name
	};

	if let Some(decoded) = decode_underscore_flat_source_path(encoded)? {
		return Ok(decoded);
	}

	Err(anyhow!("unsupported swap storage path format: {}", storage_path.display()))
}

fn decode_underscore_flat_source_path(encoded: &str) -> Result<Option<PathBuf>> {
	if !encoded.contains('_') {
		return Ok(None);
	}

	let path_separator = std::path::MAIN_SEPARATOR;
	let windows_style_target = path_separator == '\\';
	let mut decoded = String::new();
	let mut chars = encoded.chars().peekable();
	let bytes = encoded.as_bytes();
	if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b'_' {
		decoded.push(char::from(bytes[0]));
		decoded.push(':');
		decoded.push(path_separator);
		let _ = chars.next();
		let _ = chars.next();
	} else if encoded.starts_with('_') {
		decoded.push(path_separator);
		if windows_style_target {
			decoded.push(path_separator);
		}
		let _ = chars.next();
	}

	while let Some(ch) = chars.next() {
		if ch != '_' {
			decoded.push(ch);
			continue;
		}
		if matches!(chars.peek(), Some('_')) {
			let _ = chars.next();
			decoded.push('_');
		} else {
			decoded.push(path_separator);
		}
	}

	Ok(Some(PathBuf::from(decoded)))
}

pub(super) fn user_swap_dir() -> PathBuf { user_state_root().join("swp") }

pub(super) fn user_undo_dir() -> PathBuf { user_state_root().join("undo") }

pub(super) fn user_session_dir() -> PathBuf { user_state_root().join("session") }
