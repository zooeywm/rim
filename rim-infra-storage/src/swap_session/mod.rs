mod compaction;
mod lease;
mod protocol;
use std::{path::{Path, PathBuf}, time::Instant};

use anyhow::{Context, Result};
use compaction::{TailCompaction, compact_delete_against_insert_tail, compact_insert_against_delete_tail};
#[cfg(test)]
pub(crate) use lease::touch_swap_lease_file;
use lease::{has_other_swap_leases, remove_swap_lease_file};
#[cfg(test)]
pub(crate) use protocol::append_swap_ops;
use protocol::apply_swap_op;
pub(crate) use protocol::{parse_swap_file, write_swap_snapshot};
use rim_kernel::{ports::SwapEditOp, state::BufferId};
use ropey::Rope;
use tracing::{error, info};

#[cfg(test)]
use crate::FLUSH_DEBOUNCE_WINDOW;
use crate::{INSERT_MERGE_WINDOW, path_codec::{normalize_source_path_for_persistence, swap_lease_path_for_source, swap_path_for_source}};

#[derive(Debug)]
pub(super) struct SwapSession {
	buffer_id:                     BufferId,
	swap_dir:                      PathBuf,
	pub(super) source_path:        PathBuf,
	pub(super) swap_path:          PathBuf,
	lease_path:                    PathBuf,
	pid:                           u32,
	username:                      String,
	pub(super) rope:               Rope,
	clean_rope:                    Option<Rope>,
	snapshot_rope:                 Option<Rope>,
	pub(super) dirty:              bool,
	logged_ops:                    Vec<BufferedSwapOp>,
	pub(super) logged_end_offsets: Vec<u64>,
	pub(super) pending_ops:        Vec<BufferedSwapOp>,
	flush_generation:              u64,
	last_pending_at:               Option<Instant>,
	last_insert_at:                Option<Instant>,
	snapshot_ready:                bool,
	pub(super) snapshot_len:       u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BufferedSwapOp {
	pub(super) op: SwapEditOp,
	deleted_text:  Option<String>,
}

impl BufferedSwapOp {
	fn insert(pos: usize, text: String) -> Self {
		Self { op: SwapEditOp::Insert { pos, text }, deleted_text: None }
	}

	fn delete(pos: usize, len: usize, deleted_text: String) -> Self {
		Self { op: SwapEditOp::Delete { pos, len }, deleted_text: Some(deleted_text) }
	}
}

impl SwapSession {
	pub(super) fn new(
		buffer_id: BufferId,
		source_path: &Path,
		swap_dir: &Path,
		pid: u32,
		username: String,
	) -> Self {
		let source_path = normalize_source_path_for_persistence(source_path);
		let swap_path = swap_path_for_source(swap_dir, source_path.as_path());
		let lease_path = swap_lease_path_for_source(swap_dir, source_path.as_path(), pid);
		Self {
			buffer_id,
			swap_dir: swap_dir.to_path_buf(),
			source_path,
			swap_path,
			lease_path,
			pid,
			username,
			rope: Rope::new(),
			clean_rope: None,
			snapshot_rope: None,
			dirty: false,
			logged_ops: Vec::new(),
			logged_end_offsets: Vec::new(),
			pending_ops: Vec::new(),
			flush_generation: 0,
			last_pending_at: None,
			last_insert_at: None,
			snapshot_ready: false,
			snapshot_len: 0,
		}
	}

	pub(super) async fn bind_lease(&mut self) -> Result<()> {
		lease::touch_swap_lease_file(self.lease_path.as_path()).await
	}

	pub(super) async fn rebind_if_needed(&mut self, source_path: &Path) -> Result<()> {
		let source_path = normalize_source_path_for_persistence(source_path);
		if self.source_path == source_path {
			return Ok(());
		}

		let old_swap = self.swap_path.clone();
		self.source_path = source_path;
		self.swap_path = swap_path_for_source(self.swap_dir.as_path(), self.source_path.as_path());
		let old_lease = self.lease_path.clone();
		self.lease_path =
			swap_lease_path_for_source(self.swap_dir.as_path(), self.source_path.as_path(), self.pid);
		remove_swap_lease_file(old_lease.as_path()).await;
		self.bind_lease().await?;
		self.snapshot_ready = false;
		self.clean_rope = None;
		self.snapshot_rope = None;
		self.logged_ops.clear();
		self.logged_end_offsets.clear();
		self.pending_ops.clear();
		self.last_pending_at = None;
		self.last_insert_at = None;
		self.write_snapshot(self.rope.to_string().as_str(), self.dirty).await?;
		if let Err(err) = compio::fs::remove_file(&old_swap).await
			&& err.kind() != std::io::ErrorKind::NotFound
		{
			error!("remove old swap during rebind failed: path={} error={}", old_swap.display(), err);
		}
		Ok(())
	}

	pub(super) async fn detect_conflict(&self) -> Result<Option<(u32, String)>> {
		if compio::fs::metadata(&self.swap_path).await.is_err() {
			return Ok(None);
		}
		let parsed = parse_swap_file(self.swap_path.as_path()).await?;
		if parsed.source_path != self.source_path {
			return Ok(None);
		}
		Ok(Some((parsed.pid, parsed.username)))
	}

	pub(super) async fn initialize_base(&mut self, base_text: String, delete_existing: bool) -> Result<()> {
		if delete_existing {
			match compio::fs::remove_file(&self.swap_path).await {
				Ok(()) => {}
				Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
				Err(err) => {
					return Err(err).with_context(|| {
						format!("remove existing swap failed before init: {}", self.swap_path.display())
					});
				}
			}
		}

		let base_rope = Rope::from_str(base_text.as_str());
		self.rope = base_rope.clone();
		self.clean_rope = Some(base_rope);
		self.snapshot_rope = Some(self.rope.clone());
		self.logged_ops.clear();
		self.logged_end_offsets.clear();
		self.pending_ops.clear();
		self.last_pending_at = None;
		self.last_insert_at = None;
		self.refresh_dirty_from_clean_base();
		self.write_snapshot(base_text.as_str(), false).await
	}

	pub(super) async fn recover(&mut self, base_text: String) -> Result<Option<String>> {
		let mut recovered_rope = None;
		if compio::fs::metadata(&self.swap_path).await.is_ok() {
			let parsed = parse_swap_file(self.swap_path.as_path()).await?;
			if parsed.source_path == self.source_path {
				if parsed.dirty || !parsed.ops.is_empty() {
					info!(
						"swap recovery replay: source={} owner_pid={} owner_user={}",
						self.source_path.display(),
						parsed.pid,
						parsed.username
					);
					let mut rope = Rope::from_str(parsed.base_text.as_str());
					for op in parsed.ops {
						apply_swap_op(&mut rope, op);
					}
					recovered_rope = Some(rope);
				}
			} else {
				error!(
					"swap source path mismatch: swap={} parsed={} expected={}",
					self.swap_path.display(),
					parsed.source_path.display(),
					self.source_path.display()
				);
			}
		}

		let clean_rope = Rope::from_str(base_text.as_str());
		self.clean_rope = Some(clean_rope.clone());
		self.rope = recovered_rope.unwrap_or(clean_rope);
		self.snapshot_rope = Some(self.rope.clone());
		self.logged_ops.clear();
		self.logged_end_offsets.clear();
		self.pending_ops.clear();
		self.last_pending_at = None;
		self.last_insert_at = None;
		self.refresh_dirty_from_clean_base();
		if self.dirty {
			let recovered_text = self.rope.to_string();
			self.write_snapshot(recovered_text.as_str(), true).await?;
			return Ok(Some(recovered_text));
		}
		self.write_snapshot(base_text.as_str(), false).await?;

		Ok(None)
	}

	pub(super) async fn apply_edit(&mut self, op: SwapEditOp, now: Instant) -> Result<()> {
		self.ensure_snapshot_initialized().await?;
		match op {
			SwapEditOp::Insert { pos, text } => {
				if text.is_empty() {
					return Ok(());
				}
				let clamped_pos = pos.min(self.rope.len_chars());
				self.rope.insert(clamped_pos, text.as_str());
				self.push_insert_with_merge(clamped_pos, text, now);
				self.mark_dirty_after_edit();
				self.last_pending_at = Some(now);
			}
			SwapEditOp::Delete { pos, len } => {
				if len == 0 {
					return Ok(());
				}
				let start = pos.min(self.rope.len_chars());
				if start >= self.rope.len_chars() {
					return Ok(());
				}
				let end = start.saturating_add(len).min(self.rope.len_chars());
				if end <= start {
					return Ok(());
				}
				let deleted_text = self.rope.slice(start..end).to_string();
				self.rope.remove(start..end);
				self.push_delete_with_compaction(start, end - start, deleted_text);
				self.mark_dirty_after_edit();
				self.last_pending_at = if self.pending_ops.is_empty() { None } else { Some(now) };
			}
		}
		Ok(())
	}

	fn push_insert_with_merge(&mut self, pos: usize, text: String, now: Instant) {
		if compact_insert_against_delete_tail(&mut self.pending_ops, pos, text.as_str()) != TailCompaction::None {
			self.last_insert_at = None;
			return;
		}

		if let Some(BufferedSwapOp { op: SwapEditOp::Insert { pos: last_pos, text: last_text }, .. }) =
			self.pending_ops.last_mut()
			&& let Some(last_insert_at) = self.last_insert_at
			&& now.duration_since(last_insert_at) <= INSERT_MERGE_WINDOW
		{
			let expected_pos = last_pos.saturating_add(last_text.chars().count());
			if pos == expected_pos {
				last_text.push_str(text.as_str());
				self.last_insert_at = Some(now);
				return;
			}
		}
		self.pending_ops.push(BufferedSwapOp::insert(pos, text));
		self.last_insert_at = Some(now);
	}

	fn push_delete_with_compaction(&mut self, pos: usize, len: usize, deleted_text: String) {
		if self.try_compact_delete_against_pending_insert(pos, len) {
			self.last_insert_at = None;
			return;
		}
		self.pending_ops.push(BufferedSwapOp::delete(pos, len, deleted_text));
		self.last_insert_at = None;
	}

	fn try_compact_delete_against_pending_insert(&mut self, pos: usize, len: usize) -> bool {
		compact_delete_against_insert_tail(&mut self.pending_ops, pos, len) != TailCompaction::None
	}

	pub(super) fn schedule_flush_generation(&mut self) -> Option<u64> {
		if self.pending_ops.is_empty() {
			return None;
		}
		self.flush_generation = self.flush_generation.saturating_add(1);
		Some(self.flush_generation)
	}

	pub(super) fn should_flush_generation(&self, generation: u64) -> bool {
		!self.pending_ops.is_empty() && self.flush_generation == generation
	}

	#[cfg(test)]
	pub(super) fn flush_if_due(&mut self, now: Instant) -> Result<()> {
		if self.pending_ops.is_empty() {
			return Ok(());
		}
		let Some(last_pending_at) = self.last_pending_at else {
			return Ok(());
		};
		if now.duration_since(last_pending_at) < FLUSH_DEBOUNCE_WINDOW {
			return Ok(());
		}
		super::block_on_test(self.flush_pending())
	}

	pub(super) async fn mark_clean(&mut self) -> Result<()> {
		self.ensure_snapshot_initialized().await?;
		self.flush_pending().await?;
		self.clean_rope = Some(self.rope.clone());
		self.snapshot_rope = Some(self.rope.clone());
		self.logged_ops.clear();
		self.logged_end_offsets.clear();
		self.refresh_dirty_from_clean_base();
		self.write_snapshot(self.rope.to_string().as_str(), false).await
	}

	pub(super) async fn ensure_snapshot_initialized(&mut self) -> Result<()> {
		if self.snapshot_ready {
			return Ok(());
		}
		if self.snapshot_rope.is_none() {
			self.snapshot_rope = Some(self.rope.clone());
			self.logged_ops.clear();
			self.logged_end_offsets.clear();
		}
		self.write_snapshot(self.rope.to_string().as_str(), self.dirty).await
	}

	async fn write_snapshot(&mut self, base_text: &str, dirty: bool) -> Result<()> {
		write_swap_snapshot(self.swap_path.as_path(), self.pid, self.username.as_str(), dirty, base_text).await?;
		self.snapshot_len = compio::fs::metadata(&self.swap_path)
			.await
			.with_context(|| format!("stat swap snapshot failed: {}", self.swap_path.display()))?
			.len();
		self.logged_end_offsets.clear();
		self.snapshot_ready = true;
		Ok(())
	}

	fn mark_dirty_after_edit(&mut self) {
		if self.clean_rope.is_some() {
			self.refresh_dirty_from_clean_base();
		} else {
			self.dirty = true;
		}
	}

	fn refresh_dirty_from_clean_base(&mut self) {
		if let Some(clean_rope) = self.clean_rope.as_ref() {
			self.dirty = clean_rope != &self.rope;
		}
	}

	pub(super) async fn close(self) -> Result<()> {
		remove_swap_lease_file(self.lease_path.as_path()).await;
		if compio::fs::metadata(&self.swap_path).await.is_err() {
			return Ok(());
		}
		if has_other_swap_leases(self.lease_path.as_path(), self.source_path.as_path()).await {
			return Ok(());
		}
		if let Err(err) = compio::fs::remove_file(&self.swap_path).await
			&& err.kind() != std::io::ErrorKind::NotFound
		{
			return Err(err).with_context(|| {
				format!(
					"close swap session remove file failed: buffer={:?} swap={}",
					self.buffer_id,
					self.swap_path.display()
				)
			});
		}
		Ok(())
	}
}
