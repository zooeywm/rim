use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use compio::{fs::OpenOptions, io::AsyncWriteAtExt};
use rim_application::ports::SwapEditOp;
use ropey::Rope;

use super::BufferedSwapOp;
use crate::{SWAP_FILE_MAGIC, path_codec::source_path_from_swap_storage_path};

#[derive(Debug)]
pub(crate) struct ParsedSwapFile {
	pub(crate) pid:         u32,
	pub(crate) username:    String,
	pub(crate) source_path: PathBuf,
	pub(crate) dirty:       bool,
	pub(crate) base_text:   String,
	pub(crate) ops:         Vec<SwapEditOp>,
}

pub(crate) async fn write_swap_snapshot(
	path: &Path,
	pid: u32,
	username: &str,
	dirty: bool,
	base_text: &str,
) -> Result<()> {
	if let Some(parent) = path.parent() {
		compio::fs::create_dir_all(parent)
			.await
			.with_context(|| format!("create swap parent dir failed: {}", parent.display()))?;
	}
	let content = format!(
		"{}\nMETA\tpid={}\tuser={}\tdirty={}\nBASE\t{}\n",
		SWAP_FILE_MAGIC,
		pid,
		encode_text_field(username),
		if dirty { "1" } else { "0" },
		encode_text_field(base_text),
	);
	let write_result = compio::fs::write(path, content.into_bytes()).await.0;
	write_result.with_context(|| format!("write swap snapshot failed: {}", path.display()))?;
	Ok(())
}

#[cfg(test)]
pub(crate) async fn append_swap_ops(path: &Path, ops: &[SwapEditOp]) -> Result<()> {
	if ops.is_empty() {
		return Ok(());
	}
	let _ = append_swap_ops_with_offsets(path, ops.iter()).await?;
	Ok(())
}

async fn append_swap_ops_with_offsets<'a>(
	path: &Path,
	ops: impl IntoIterator<Item = &'a SwapEditOp>,
) -> Result<Vec<u64>> {
	let mut payload = String::new();
	let mut line_sizes = Vec::new();
	for op in ops {
		let line = match op {
			SwapEditOp::Insert { pos, text } => format!("I\t{}\t{}\n", pos, encode_text_field(text)),
			SwapEditOp::Delete { pos, len } => format!("D\t{}\t{}\n", pos, len),
		};
		line_sizes.push(line.len() as u64);
		payload.push_str(line.as_str());
	}
	let mut file = OpenOptions::new()
		.create(true)
		.write(true)
		.open(path)
		.await
		.with_context(|| format!("open swap file for append failed: {}", path.display()))?;
	let initial_len =
		file.metadata().await.with_context(|| format!("stat swap append file failed: {}", path.display()))?.len();
	let compio::BufResult(write_result, _) = file.write_all_at(payload.into_bytes(), initial_len).await;
	write_result.with_context(|| format!("append swap op failed: {}", path.display()))?;
	file.sync_data().await.with_context(|| format!("sync swap append failed: {}", path.display()))?;
	let mut offsets = Vec::new();
	let mut current_len = initial_len;
	for line_size in line_sizes {
		current_len = current_len.saturating_add(line_size);
		offsets.push(current_len);
	}
	Ok(offsets)
}

pub(crate) async fn append_buffered_swap_ops(path: &Path, ops: &[BufferedSwapOp]) -> Result<Vec<u64>> {
	if ops.is_empty() {
		return Ok(Vec::new());
	}
	append_buffered_swap_ops_iter(path, ops.iter()).await
}

pub(crate) async fn append_buffered_swap_ops_iter<'a>(
	path: &Path,
	ops: impl IntoIterator<Item = &'a BufferedSwapOp>,
) -> Result<Vec<u64>> {
	append_swap_ops_with_offsets(path, ops.into_iter().map(|op| &op.op)).await
}

pub(crate) async fn truncate_swap_file(path: &Path, len: u64) -> Result<()> {
	let file = OpenOptions::new()
		.write(true)
		.open(path)
		.await
		.with_context(|| format!("open swap file for truncate failed: {}", path.display()))?;
	file.set_len(len).await.with_context(|| format!("truncate swap file failed: {}", path.display()))?;
	Ok(())
}

pub(crate) async fn parse_swap_file(path: &Path) -> Result<ParsedSwapFile> {
	let content = String::from_utf8(
		compio::fs::read(path).await.with_context(|| format!("read swap file failed: {}", path.display()))?,
	)
	.with_context(|| format!("decode swap file failed: {}", path.display()))?;
	let mut lines = content.lines();

	let Some(magic_line) = lines.next() else {
		bail!("invalid swap file (empty): {}", path.display());
	};
	if magic_line != SWAP_FILE_MAGIC {
		bail!("invalid swap magic: {}", path.display());
	}

	let Some(meta_line) = lines.next() else {
		bail!("invalid swap file (missing meta): {}", path.display());
	};
	let (pid, username, source_path_text, dirty) = parse_meta_line(meta_line, path)?;

	let Some(base_line) = lines.next() else {
		bail!("invalid swap file (missing base): {}", path.display());
	};
	let base_fields = base_line.split('\t').collect::<Vec<_>>();
	if base_fields.len() != 2 || base_fields[0] != "BASE" {
		bail!("invalid swap base line: {}", path.display());
	}
	let base_text = decode_text_field(base_fields[1])
		.with_context(|| format!("invalid swap base text in {}", path.display()))?;

	let mut ops = Vec::new();
	for line in lines {
		if line.is_empty() {
			continue;
		}
		let fields = line.split('\t').collect::<Vec<_>>();
		match fields.first().copied() {
			Some("I") if fields.len() == 3 => {
				let pos = fields[1]
					.parse::<usize>()
					.with_context(|| format!("invalid swap insert pos in {}", path.display()))?;
				let text = decode_text_field(fields[2])
					.with_context(|| format!("invalid swap insert text in {}", path.display()))?;
				ops.push(SwapEditOp::Insert { pos, text });
			}
			Some("D") if fields.len() == 3 => {
				let pos = fields[1]
					.parse::<usize>()
					.with_context(|| format!("invalid swap delete pos in {}", path.display()))?;
				let len = fields[2]
					.parse::<usize>()
					.with_context(|| format!("invalid swap delete len in {}", path.display()))?;
				ops.push(SwapEditOp::Delete { pos, len });
			}
			_ => bail!("invalid swap operation line in {}", path.display()),
		}
	}

	let source_path = if let Some(source_path_text) = source_path_text {
		PathBuf::from(source_path_text)
	} else {
		source_path_from_swap_storage_path(path)?
	};
	Ok(ParsedSwapFile { pid, username, source_path, dirty, base_text, ops })
}

pub(crate) fn apply_swap_op(rope: &mut Rope, op: SwapEditOp) {
	match op {
		SwapEditOp::Insert { pos, text } => {
			if text.is_empty() {
				return;
			}
			let start = pos.min(rope.len_chars());
			rope.insert(start, text.as_str());
		}
		SwapEditOp::Delete { pos, len } => {
			if len == 0 {
				return;
			}
			let start = pos.min(rope.len_chars());
			if start >= rope.len_chars() {
				return;
			}
			let end = start.saturating_add(len).min(rope.len_chars());
			if end > start {
				rope.remove(start..end);
			}
		}
	}
}

fn parse_meta_line(meta_line: &str, path: &Path) -> Result<(u32, String, Option<String>, bool)> {
	let meta_fields = meta_line.split('\t').collect::<Vec<_>>();
	if meta_fields.first().copied() != Some("META") {
		bail!("invalid swap meta line: {}", path.display());
	}

	if meta_fields.len() == 5 && !meta_fields[1].contains('=') {
		let pid =
			meta_fields[1].parse::<u32>().with_context(|| format!("invalid swap pid in {}", path.display()))?;
		let username = decode_b64(meta_fields[2])
			.with_context(|| format!("invalid legacy swap username in {}", path.display()))?;
		let source_path_text = decode_b64(meta_fields[3])
			.with_context(|| format!("invalid legacy swap source path in {}", path.display()))?;
		let dirty = parse_dirty_flag(meta_fields[4], path)?;
		return Ok((pid, username, Some(source_path_text), dirty));
	}

	if meta_fields.len() != 4 && meta_fields.len() != 5 {
		bail!("invalid swap meta field count in {}", path.display());
	}

	let pid_raw = meta_fields[1]
		.strip_prefix("pid=")
		.ok_or_else(|| anyhow!("invalid swap pid field in {}", path.display()))?;
	let user_raw = meta_fields[2]
		.strip_prefix("user=")
		.ok_or_else(|| anyhow!("invalid swap user field in {}", path.display()))?;
	let (source_path_text, dirty_raw) = if meta_fields.len() == 5 {
		let source_raw = meta_fields[3]
			.strip_prefix("source=")
			.ok_or_else(|| anyhow!("invalid swap source field in {}", path.display()))?;
		(
			Some(
				decode_text_field(source_raw)
					.with_context(|| format!("invalid swap source path in {}", path.display()))?,
			),
			meta_fields[4]
				.strip_prefix("dirty=")
				.ok_or_else(|| anyhow!("invalid swap dirty field in {}", path.display()))?,
		)
	} else {
		(
			None,
			meta_fields[3]
				.strip_prefix("dirty=")
				.ok_or_else(|| anyhow!("invalid swap dirty field in {}", path.display()))?,
		)
	};

	let pid = pid_raw.parse::<u32>().with_context(|| format!("invalid swap pid in {}", path.display()))?;
	let username =
		decode_text_field(user_raw).with_context(|| format!("invalid swap username in {}", path.display()))?;
	let dirty = parse_dirty_flag(dirty_raw, path)?;
	Ok((pid, username, source_path_text, dirty))
}

fn decode_b64(encoded: &str) -> Result<String> {
	let bytes = STANDARD_NO_PAD.decode(encoded).context("base64 decode failed")?;
	String::from_utf8(bytes).context("decoded text is not utf-8")
}

fn parse_dirty_flag(raw: &str, path: &Path) -> Result<bool> {
	match raw {
		"0" | "false" => Ok(false),
		"1" | "true" => Ok(true),
		_ => bail!("invalid swap dirty flag in {}", path.display()),
	}
}

fn encode_text_field(text: &str) -> String {
	serde_json::to_string(text).expect("swap json encoding should never fail")
}

fn decode_text_field(encoded: &str) -> Result<String> {
	if encoded.starts_with('"') {
		return serde_json::from_str::<String>(encoded).context("swap json decode failed");
	}
	if let Ok(decoded) = decode_b64(encoded) {
		return Ok(decoded);
	}
	Ok(encoded.to_string())
}
