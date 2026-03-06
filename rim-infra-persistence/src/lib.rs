#[cfg(unix)]
use std::process::{Command, Stdio};
use std::{borrow::Cow, collections::HashMap, fs::OpenOptions, io::{BufWriter, Write}, path::{Path, PathBuf}, sync::Mutex, thread, time::{Duration, Instant}};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use rim_kernel::{action::{AppAction, FileAction, SwapConflictInfo}, ports::{PersistenceIo, PersistenceIoError, SwapEditOp}, state::{BufferEditSnapshot, BufferHistoryEntry, BufferId, CursorState, PersistedBufferHistory}};
use rim_paths::user_state_root;
use ropey::Rope;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

const SWAP_FILE_MAGIC: &str = "RIMSWP\t1";
const UNDO_FILE_VERSION: u32 = 1;
const REQUEST_POLL_INTERVAL: Duration = Duration::from_millis(40);
const FLUSH_DEBOUNCE_WINDOW: Duration = Duration::from_millis(180);
const INSERT_MERGE_WINDOW: Duration = Duration::from_millis(350);

#[derive(dep_inj::DepInj)]
#[target(PersistenceIoImpl)]
pub struct PersistenceIoState {
	request_tx:  flume::Sender<PersistenceRequest>,
	request_rx:  flume::Receiver<PersistenceRequest>,
	event_tx:    flume::Sender<AppAction>,
	swap_dir:    PathBuf,
	undo_dir:    PathBuf,
	worker_join: Mutex<Option<thread::JoinHandle<()>>>,
}

impl AsRef<PersistenceIoState> for PersistenceIoState {
	fn as_ref(&self) -> &PersistenceIoState { self }
}

impl<Deps> PersistenceIo for PersistenceIoImpl<Deps>
where Deps: AsRef<PersistenceIoState>
{
	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), PersistenceIoError> {
		self.request_tx.send(PersistenceRequest::Open { buffer_id, source_path }).map_err(|err| {
			error!("enqueue_open failed: swap request channel is disconnected: {}", err);
			PersistenceIoError::RequestChannelDisconnected { operation: "open" }
		})
	}

	fn enqueue_detect_conflict(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
	) -> Result<(), PersistenceIoError> {
		self.request_tx.send(PersistenceRequest::DetectConflict { buffer_id, source_path }).map_err(|err| {
			error!("enqueue_detect_conflict failed: swap request channel is disconnected: {}", err);
			PersistenceIoError::RequestChannelDisconnected { operation: "detect_conflict" }
		})
	}

	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), PersistenceIoError> {
		self.request_tx.send(PersistenceRequest::Edit { buffer_id, source_path, op }).map_err(|err| {
			error!("enqueue_edit failed: swap request channel is disconnected: {}", err);
			PersistenceIoError::RequestChannelDisconnected { operation: "edit" }
		})
	}

	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), PersistenceIoError> {
		self.request_tx.send(PersistenceRequest::MarkClean { buffer_id, source_path }).map_err(|err| {
			error!("enqueue_mark_clean failed: swap request channel is disconnected: {}", err);
			PersistenceIoError::RequestChannelDisconnected { operation: "mark_clean" }
		})
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), PersistenceIoError> {
		self
			.request_tx
			.send(PersistenceRequest::InitializeBase { buffer_id, source_path, base_text, delete_existing })
			.map_err(|err| {
				error!("enqueue_initialize_base failed: swap request channel is disconnected: {}", err);
				PersistenceIoError::RequestChannelDisconnected { operation: "initialize_base" }
			})
	}

	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), PersistenceIoError> {
		self.request_tx.send(PersistenceRequest::Recover { buffer_id, source_path, base_text }).map_err(|err| {
			error!("enqueue_recover failed: swap request channel is disconnected: {}", err);
			PersistenceIoError::RequestChannelDisconnected { operation: "recover" }
		})
	}

	fn enqueue_load_history(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		expected_text: String,
	) -> Result<(), PersistenceIoError> {
		self.request_tx.send(PersistenceRequest::LoadHistory { buffer_id, source_path, expected_text }).map_err(
			|err| {
				error!("enqueue_load_history failed: persistence request channel is disconnected: {}", err);
				PersistenceIoError::RequestChannelDisconnected { operation: "load_history" }
			},
		)
	}

	fn enqueue_save_history(
		&self,
		_buffer_id: BufferId,
		source_path: PathBuf,
		history: PersistedBufferHistory,
	) -> Result<(), PersistenceIoError> {
		self.request_tx.send(PersistenceRequest::SaveHistory { source_path, history }).map_err(|err| {
			error!("enqueue_save_history failed: persistence request channel is disconnected: {}", err);
			PersistenceIoError::RequestChannelDisconnected { operation: "save_history" }
		})
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), PersistenceIoError> {
		self.request_tx.send(PersistenceRequest::Close { buffer_id }).map_err(|err| {
			error!("enqueue_close failed: swap request channel is disconnected: {}", err);
			PersistenceIoError::RequestChannelDisconnected { operation: "close" }
		})
	}
}

impl PersistenceIoState {
	pub fn new(event_tx: flume::Sender<AppAction>) -> Self {
		let (request_tx, request_rx) = flume::unbounded();
		Self {
			request_tx,
			request_rx,
			event_tx,
			swap_dir: user_swap_dir(),
			undo_dir: user_undo_dir(),
			worker_join: Mutex::new(None),
		}
	}

	pub fn start(&self) {
		let mut guard = self.worker_join.lock().expect("swap worker mutex poisoned");
		if guard.is_some() {
			return;
		}
		let request_rx = self.request_rx.clone();
		let event_tx = self.event_tx.clone();
		let swap_dir = self.swap_dir.clone();
		let undo_dir = self.undo_dir.clone();
		let join = thread::spawn(move || {
			if let Err(err) = Self::run(request_rx, event_tx, swap_dir, undo_dir) {
				error!("swap worker exited with error: {:#}", err);
			}
		});
		*guard = Some(join);
	}

	fn run(
		request_rx: flume::Receiver<PersistenceRequest>,
		event_tx: flume::Sender<AppAction>,
		swap_dir: PathBuf,
		undo_dir: PathBuf,
	) -> Result<()> {
		std::fs::create_dir_all(&swap_dir)
			.with_context(|| format!("create swap dir failed: {}", swap_dir.display()))?;
		std::fs::create_dir_all(&undo_dir)
			.with_context(|| format!("create undo dir failed: {}", undo_dir.display()))?;

		let mut sessions: HashMap<BufferId, SwapSession> = HashMap::new();
		let mut undo_sessions: HashMap<PathBuf, UndoHistorySession> = HashMap::new();
		let pid = std::process::id();
		let username = current_username();

		loop {
			match request_rx.recv_timeout(REQUEST_POLL_INTERVAL) {
				Ok(request) => {
					// Bundle worker dependencies into a single context so the request
					// dispatcher does not grow an unstable argument list.
					let keep_running = Self::handle_request(request, PersistenceWorkerContext {
						event_tx: &event_tx,
						swap_dir: &swap_dir,
						undo_dir: &undo_dir,
						pid,
						username: username.as_str(),
						sessions: &mut sessions,
						undo_sessions: &mut undo_sessions,
					});
					if !keep_running {
						break;
					}
				}
				Err(flume::RecvTimeoutError::Timeout) => {}
				Err(flume::RecvTimeoutError::Disconnected) => break,
			}

			let now = Instant::now();
			for session in sessions.values_mut() {
				if let Err(err) = session.flush_if_due(now) {
					error!(
						"swap flush failed: source={} swap={} error={:#}",
						session.source_path.display(),
						session.swap_path.display(),
						err
					);
				}
			}
		}

		Ok(())
	}

	fn handle_request(request: PersistenceRequest, context: PersistenceWorkerContext<'_>) -> bool {
		let PersistenceWorkerContext { event_tx, swap_dir, undo_dir, pid, username, sessions, undo_sessions } =
			context;
		match request {
			PersistenceRequest::Shutdown => return false,
			PersistenceRequest::Open { buffer_id, source_path } => {
				sessions
					.insert(buffer_id, SwapSession::new(buffer_id, source_path, swap_dir, pid, username.to_string()));
			}
			PersistenceRequest::DetectConflict { buffer_id, source_path } => {
				let session = sessions.entry(buffer_id).or_insert_with(|| {
					SwapSession::new(buffer_id, source_path.clone(), swap_dir, pid, username.to_string())
				});
				let result =
					session.rebind_if_needed(source_path).and_then(|_| session.detect_conflict()).map(|conflict| {
						conflict.map(|(owner_pid, owner_username)| SwapConflictInfo {
							pid:      owner_pid,
							username: owner_username,
						})
					});
				if let Err(err) =
					event_tx.send(AppAction::File(FileAction::SwapConflictDetected { buffer_id, result }))
				{
					error!("send SwapConflictDetected failed: {}", err);
					return false;
				}
			}
			PersistenceRequest::Edit { buffer_id, source_path, op } => {
				let session = sessions.entry(buffer_id).or_insert_with(|| {
					SwapSession::new(buffer_id, source_path.clone(), swap_dir, pid, username.to_string())
				});
				if let Err(err) = session.rebind_if_needed(source_path) {
					error!("swap rebind before edit failed: {:#}", err);
					return true;
				}
				if let Err(err) = session.apply_edit(op, Instant::now()) {
					error!("swap edit apply failed: {:#}", err);
				}
			}
			PersistenceRequest::MarkClean { buffer_id, source_path } => {
				let session = sessions.entry(buffer_id).or_insert_with(|| {
					SwapSession::new(buffer_id, source_path.clone(), swap_dir, pid, username.to_string())
				});
				if let Err(err) = session.rebind_if_needed(source_path) {
					error!("swap rebind before mark_clean failed: {:#}", err);
					return true;
				}
				if let Err(err) = session.mark_clean() {
					error!("swap mark_clean failed: {:#}", err);
				}
			}
			PersistenceRequest::InitializeBase { buffer_id, source_path, base_text, delete_existing } => {
				let session = sessions.entry(buffer_id).or_insert_with(|| {
					SwapSession::new(buffer_id, source_path.clone(), swap_dir, pid, username.to_string())
				});
				if let Err(err) = session
					.rebind_if_needed(source_path)
					.and_then(|_| session.initialize_base(base_text, delete_existing))
				{
					error!("swap initialize_base failed: {:#}", err);
				}
			}
			PersistenceRequest::Recover { buffer_id, source_path, base_text } => {
				let session = sessions.entry(buffer_id).or_insert_with(|| {
					SwapSession::new(buffer_id, source_path.clone(), swap_dir, pid, username.to_string())
				});
				let had_swap_before_recover = session.swap_path.exists();
				let result = session.rebind_if_needed(source_path).and_then(|_| session.recover(base_text));
				let should_send_callback = had_swap_before_recover || result.as_ref().is_err();
				if should_send_callback
					&& let Err(err) =
						event_tx.send(AppAction::File(FileAction::SwapRecoverCompleted { buffer_id, result }))
				{
					error!("send SwapRecoverCompleted failed: {}", err);
					return false;
				}
			}
			PersistenceRequest::LoadHistory { buffer_id, source_path, expected_text } => {
				let result =
					load_undo_history(undo_dir, source_path.as_path(), expected_text.as_str(), undo_sessions);
				if let Err(err) = event_tx.send(AppAction::File(FileAction::UndoHistoryLoaded {
					buffer_id,
					source_path,
					expected_text,
					result,
				})) {
					error!("send UndoHistoryLoaded failed: {}", err);
					return false;
				}
			}
			PersistenceRequest::SaveHistory { source_path, history } => {
				if let Err(err) = save_undo_history(undo_dir, source_path.as_path(), &history, undo_sessions) {
					error!("save undo history failed: source={} error={:#}", source_path.display(), err);
				}
			}
			PersistenceRequest::Close { buffer_id } => {
				sessions.remove(&buffer_id);
			}
		}

		true
	}
}

#[derive(Debug)]
enum PersistenceRequest {
	Shutdown,
	Open {
		buffer_id:   BufferId,
		source_path: PathBuf,
	},
	DetectConflict {
		buffer_id:   BufferId,
		source_path: PathBuf,
	},
	Edit {
		buffer_id:   BufferId,
		source_path: PathBuf,
		op:          SwapEditOp,
	},
	MarkClean {
		buffer_id:   BufferId,
		source_path: PathBuf,
	},
	InitializeBase {
		buffer_id:       BufferId,
		source_path:     PathBuf,
		base_text:       String,
		delete_existing: bool,
	},
	Recover {
		buffer_id:   BufferId,
		source_path: PathBuf,
		base_text:   String,
	},
	LoadHistory {
		buffer_id:     BufferId,
		source_path:   PathBuf,
		expected_text: String,
	},
	SaveHistory {
		source_path: PathBuf,
		history:     PersistedBufferHistory,
	},
	Close {
		buffer_id: BufferId,
	},
}

struct PersistenceWorkerContext<'a> {
	event_tx:      &'a flume::Sender<AppAction>,
	swap_dir:      &'a Path,
	undo_dir:      &'a Path,
	pid:           u32,
	username:      &'a str,
	sessions:      &'a mut HashMap<BufferId, SwapSession>,
	undo_sessions: &'a mut HashMap<PathBuf, UndoHistorySession>,
}

#[derive(Debug)]
struct SwapSession {
	buffer_id:          BufferId,
	swap_dir:           PathBuf,
	source_path:        PathBuf,
	swap_path:          PathBuf,
	lease_path:         PathBuf,
	pid:                u32,
	username:           String,
	rope:               Rope,
	clean_rope:         Option<Rope>,
	snapshot_rope:      Option<Rope>,
	dirty:              bool,
	logged_ops:         Vec<BufferedSwapOp>,
	logged_end_offsets: Vec<u64>,
	pending_ops:        Vec<BufferedSwapOp>,
	last_pending_at:    Option<Instant>,
	last_insert_at:     Option<Instant>,
	snapshot_ready:     bool,
	snapshot_len:       u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferedSwapOp {
	op:           SwapEditOp,
	deleted_text: Option<String>,
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
	fn new(buffer_id: BufferId, source_path: PathBuf, swap_dir: &Path, pid: u32, username: String) -> Self {
		let source_path = normalize_source_path_for_persistence(source_path.as_path());
		let swap_path = swap_path_for_source(swap_dir, source_path.as_path());
		let lease_path = swap_lease_path_for_source(swap_dir, source_path.as_path(), pid);
		touch_swap_lease_file(lease_path.as_path());
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
			last_pending_at: None,
			last_insert_at: None,
			snapshot_ready: false,
			snapshot_len: 0,
		}
	}

	fn rebind_if_needed(&mut self, source_path: PathBuf) -> Result<()> {
		let source_path = normalize_source_path_for_persistence(source_path.as_path());
		if self.source_path == source_path {
			return Ok(());
		}

		let old_swap = self.swap_path.clone();
		self.source_path = source_path;
		self.swap_path = swap_path_for_source(self.swap_dir.as_path(), self.source_path.as_path());
		let old_lease = self.lease_path.clone();
		self.lease_path =
			swap_lease_path_for_source(self.swap_dir.as_path(), self.source_path.as_path(), self.pid);
		remove_swap_lease_file(old_lease.as_path());
		touch_swap_lease_file(self.lease_path.as_path());
		self.snapshot_ready = false;
		self.clean_rope = None;
		self.snapshot_rope = None;
		self.logged_ops.clear();
		self.logged_end_offsets.clear();
		self.pending_ops.clear();
		self.last_pending_at = None;
		self.last_insert_at = None;
		self.write_snapshot(self.rope.to_string().as_str(), self.dirty)?;
		if let Err(err) = std::fs::remove_file(&old_swap)
			&& err.kind() != std::io::ErrorKind::NotFound
		{
			error!("remove old swap during rebind failed: path={} error={}", old_swap.display(), err);
		}
		Ok(())
	}

	fn detect_conflict(&self) -> Result<Option<(u32, String)>> {
		if !self.swap_path.exists() {
			return Ok(None);
		}
		let parsed = parse_swap_file(self.swap_path.as_path())?;
		if parsed.source_path != self.source_path {
			return Ok(None);
		}
		Ok(Some((parsed.pid, parsed.username)))
	}

	fn initialize_base(&mut self, base_text: String, delete_existing: bool) -> Result<()> {
		if delete_existing {
			match std::fs::remove_file(&self.swap_path) {
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
		self.write_snapshot(base_text.as_str(), false)
	}

	fn recover(&mut self, base_text: String) -> Result<Option<String>> {
		let mut recovered_text = base_text.clone();
		if self.swap_path.exists() {
			let parsed = parse_swap_file(self.swap_path.as_path())?;
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
					recovered_text = rope.to_string();
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

		self.clean_rope = Some(Rope::from_str(base_text.as_str()));
		self.rope = Rope::from_str(recovered_text.as_str());
		self.snapshot_rope = Some(self.rope.clone());
		self.logged_ops.clear();
		self.logged_end_offsets.clear();
		self.pending_ops.clear();
		self.last_pending_at = None;
		self.last_insert_at = None;
		self.refresh_dirty_from_clean_base();
		self.write_snapshot(recovered_text.as_str(), self.dirty)?;

		if self.dirty { Ok(Some(recovered_text)) } else { Ok(None) }
	}

	fn apply_edit(&mut self, op: SwapEditOp, now: Instant) -> Result<()> {
		self.ensure_snapshot_initialized()?;
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

	fn flush_if_due(&mut self, now: Instant) -> Result<()> {
		if self.pending_ops.is_empty() {
			return Ok(());
		}
		let Some(last_pending_at) = self.last_pending_at else {
			return Ok(());
		};
		if now.duration_since(last_pending_at) < FLUSH_DEBOUNCE_WINDOW {
			return Ok(());
		}
		self.flush_pending()
	}

	fn flush_pending(&mut self) -> Result<()> {
		if self.pending_ops.is_empty() {
			return Ok(());
		}
		let (mut rewritten_logged_ops, mut remaining_pending_ops, logged_suffix_compacted) =
			compact_logged_suffix_against_pending_prefix(
				self.snapshot_rope.as_ref(),
				self.logged_ops.as_slice(),
				self.pending_ops.as_slice(),
				&self.rope,
			);
		let mut ops_to_append = Vec::new();
		let mut tail_rewrite_start =
			if logged_suffix_compacted { rewritten_logged_ops.len() } else { self.logged_ops.len() };
		let mut should_rewrite_logged_tail = logged_suffix_compacted;

		for op in remaining_pending_ops.drain(..) {
			match compact_buffered_op_against_logged_ops(&mut rewritten_logged_ops, &op) {
				TailCompaction::None => ops_to_append.push(op),
				TailCompaction::RemovedLast => {
					tail_rewrite_start = tail_rewrite_start.min(rewritten_logged_ops.len());
					should_rewrite_logged_tail = true;
				}
				TailCompaction::MutatedLast => {
					tail_rewrite_start = tail_rewrite_start.min(rewritten_logged_ops.len().saturating_sub(1));
					should_rewrite_logged_tail = true;
				}
			}
		}

		if should_rewrite_logged_tail {
			self.logged_ops = rewritten_logged_ops;
			self.rewrite_logged_tail(tail_rewrite_start, ops_to_append.as_slice())?;
		} else {
			let appended_offsets = append_buffered_swap_ops(self.swap_path.as_path(), &ops_to_append)?;
			self.logged_ops.extend(ops_to_append);
			self.logged_end_offsets.extend(appended_offsets);
		}
		self.pending_ops.clear();
		self.last_pending_at = None;
		Ok(())
	}

	fn mark_clean(&mut self) -> Result<()> {
		self.ensure_snapshot_initialized()?;
		self.flush_pending()?;
		self.clean_rope = Some(self.rope.clone());
		self.snapshot_rope = Some(self.rope.clone());
		self.logged_ops.clear();
		self.logged_end_offsets.clear();
		self.refresh_dirty_from_clean_base();
		self.write_snapshot(self.rope.to_string().as_str(), false)
	}

	fn ensure_snapshot_initialized(&mut self) -> Result<()> {
		if self.snapshot_ready {
			return Ok(());
		}
		if self.snapshot_rope.is_none() {
			self.snapshot_rope = Some(self.rope.clone());
			self.logged_ops.clear();
			self.logged_end_offsets.clear();
		}
		self.write_snapshot(self.rope.to_string().as_str(), self.dirty)
	}

	fn write_snapshot(&mut self, base_text: &str, dirty: bool) -> Result<()> {
		write_swap_snapshot(self.swap_path.as_path(), self.pid, self.username.as_str(), dirty, base_text)?;
		self.snapshot_len = std::fs::metadata(&self.swap_path)
			.with_context(|| format!("stat swap snapshot failed: {}", self.swap_path.display()))?
			.len();
		self.logged_end_offsets.clear();
		self.snapshot_ready = true;
		Ok(())
	}

	fn rewrite_logged_tail(&mut self, tail_start: usize, ops_to_append: &[BufferedSwapOp]) -> Result<()> {
		let truncate_len = self.logged_truncate_offset(tail_start);
		truncate_swap_file(self.swap_path.as_path(), truncate_len)?;
		self.logged_end_offsets.truncate(tail_start);

		let mut tail_ops = self.logged_ops[tail_start..].to_vec();
		tail_ops.extend_from_slice(ops_to_append);
		if !tail_ops.is_empty() {
			let appended_offsets = append_buffered_swap_ops(self.swap_path.as_path(), tail_ops.as_slice())?;
			self.logged_end_offsets.extend(appended_offsets);
		}
		Ok(())
	}

	fn logged_truncate_offset(&self, retained_logged_len: usize) -> u64 {
		if retained_logged_len == 0 {
			return self.snapshot_len;
		}
		self.logged_end_offsets.get(retained_logged_len.saturating_sub(1)).copied().unwrap_or(self.snapshot_len)
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

	fn should_remove_swap_on_drop(&self) -> bool {
		if !self.swap_path.exists() {
			return false;
		}
		!has_other_swap_leases(self.lease_path.as_path(), self.source_path.as_path())
	}
}

impl Drop for SwapSession {
	fn drop(&mut self) {
		remove_swap_lease_file(self.lease_path.as_path());
		if !self.should_remove_swap_on_drop() {
			return;
		}
		if let Err(err) = std::fs::remove_file(&self.swap_path)
			&& err.kind() != std::io::ErrorKind::NotFound
		{
			error!(
				"drop swap session remove file failed: buffer={:?} swap={} error={}",
				self.buffer_id,
				self.swap_path.display(),
				err
			);
		}
	}
}

impl Drop for PersistenceIoState {
	fn drop(&mut self) {
		let _ = self.request_tx.send(PersistenceRequest::Shutdown);
		if let Ok(mut guard) = self.worker_join.lock()
			&& let Some(join) = guard.take()
		{
			let _ = join.join();
		}
	}
}

#[derive(Debug)]
struct ParsedSwapFile {
	pid:         u32,
	username:    String,
	source_path: PathBuf,
	dirty:       bool,
	base_text:   String,
	ops:         Vec<SwapEditOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LegacyUndoFileDocument {
	version:      u32,
	current_text: String,
	cursor:       UndoCursor,
	undo_stack:   Vec<UndoHistoryEntry>,
	redo_stack:   Vec<UndoHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UndoMetaDocument {
	version:        u32,
	base_text:      String,
	head:           usize,
	entry_count:    usize,
	current_cursor: UndoCursor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UndoHistorySession {
	base_text:         String,
	entries:           Vec<UndoHistoryEntry>,
	head:              usize,
	current_cursor:    CursorState,
	current_text:      String,
	entry_end_offsets: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UndoCursor {
	row: u16,
	col: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UndoHistoryEntry {
	edits:         Vec<UndoEditSnapshot>,
	before_cursor: UndoCursor,
	after_cursor:  UndoCursor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UndoEditSnapshot {
	start_byte:    usize,
	deleted_text:  String,
	inserted_text: String,
}

impl From<CursorState> for UndoCursor {
	fn from(cursor: CursorState) -> Self { Self { row: cursor.row, col: cursor.col } }
}

impl From<UndoCursor> for CursorState {
	fn from(cursor: UndoCursor) -> Self { Self { row: cursor.row, col: cursor.col } }
}

impl From<BufferEditSnapshot> for UndoEditSnapshot {
	fn from(snapshot: BufferEditSnapshot) -> Self {
		Self {
			start_byte:    snapshot.start_byte,
			deleted_text:  snapshot.deleted_text,
			inserted_text: snapshot.inserted_text,
		}
	}
}

impl From<UndoEditSnapshot> for BufferEditSnapshot {
	fn from(snapshot: UndoEditSnapshot) -> Self {
		Self {
			start_byte:    snapshot.start_byte,
			deleted_text:  snapshot.deleted_text,
			inserted_text: snapshot.inserted_text,
		}
	}
}

impl From<BufferHistoryEntry> for UndoHistoryEntry {
	fn from(entry: BufferHistoryEntry) -> Self {
		Self {
			edits:         entry.edits.into_iter().map(Into::into).collect(),
			before_cursor: entry.before_cursor.into(),
			after_cursor:  entry.after_cursor.into(),
		}
	}
}

impl From<UndoHistoryEntry> for BufferHistoryEntry {
	fn from(entry: UndoHistoryEntry) -> Self {
		Self {
			edits:         entry.edits.into_iter().map(Into::into).collect(),
			before_cursor: entry.before_cursor.into(),
			after_cursor:  entry.after_cursor.into(),
		}
	}
}

impl From<PersistedBufferHistory> for LegacyUndoFileDocument {
	fn from(history: PersistedBufferHistory) -> Self {
		Self {
			version:      UNDO_FILE_VERSION,
			current_text: history.current_text,
			cursor:       history.cursor.into(),
			undo_stack:   history.undo_stack.into_iter().map(Into::into).collect(),
			redo_stack:   history.redo_stack.into_iter().map(Into::into).collect(),
		}
	}
}

impl From<LegacyUndoFileDocument> for PersistedBufferHistory {
	fn from(document: LegacyUndoFileDocument) -> Self {
		Self {
			current_text: document.current_text,
			cursor:       document.cursor.into(),
			undo_stack:   document.undo_stack.into_iter().map(Into::into).collect(),
			redo_stack:   document.redo_stack.into_iter().map(Into::into).collect(),
		}
	}
}

impl UndoHistorySession {
	fn empty(current_text: String) -> Self {
		Self {
			base_text: current_text.clone(),
			entries: Vec::new(),
			head: 0,
			current_cursor: CursorState::default(),
			current_text,
			entry_end_offsets: Vec::new(),
		}
	}

	fn from_persisted_history(history: &PersistedBufferHistory) -> Result<Self> {
		let entries = linear_history_entries_from_snapshot(history);
		let head = history.undo_stack.len();
		let base_text = derive_base_text_from_snapshot(history)?;
		Ok(Self {
			base_text,
			entries,
			head,
			current_cursor: history.cursor,
			current_text: history.current_text.clone(),
			entry_end_offsets: Vec::new(),
		})
	}

	fn to_persisted_history(&self) -> PersistedBufferHistory {
		let undo_stack = self.entries[..self.head].iter().cloned().map(Into::into).collect::<Vec<_>>();
		let redo_stack = self.entries[self.head..].iter().rev().cloned().map(Into::into).collect::<Vec<_>>();
		PersistedBufferHistory {
			current_text: self.current_text.clone(),
			cursor: self.current_cursor,
			undo_stack,
			redo_stack,
		}
	}
}

fn linear_history_entries_from_snapshot(history: &PersistedBufferHistory) -> Vec<UndoHistoryEntry> {
	let mut entries = history.undo_stack.iter().cloned().map(Into::into).collect::<Vec<_>>();
	entries.extend(history.redo_stack.iter().rev().cloned().map(Into::into));
	entries
}

fn derive_base_text_from_snapshot(history: &PersistedBufferHistory) -> Result<String> {
	let mut base_rope = Rope::from_str(history.current_text.as_str());
	for entry in history.undo_stack.iter().rev() {
		for edit in entry.edits.iter().rev() {
			apply_undo_edit_to_rope_undo(&mut base_rope, &UndoEditSnapshot::from(edit.clone()));
		}
	}

	let base_text = base_rope.to_string();
	if !is_base_text_consistent(
		base_text.as_str(),
		linear_history_entries_from_snapshot(history).as_slice(),
		history.undo_stack.len(),
		history.current_text.as_str(),
	)? {
		bail!("persisted undo history is internally inconsistent");
	}
	Ok(base_text)
}

fn is_base_text_consistent(
	base_text: &str,
	entries: &[UndoHistoryEntry],
	head: usize,
	current_text: &str,
) -> Result<bool> {
	if head > entries.len() {
		return Ok(false);
	}
	Ok(replay_undo_entries(base_text, &entries[..head])? == current_text)
}

fn longest_common_undo_entry_prefix(lhs: &[UndoHistoryEntry], rhs: &[UndoHistoryEntry]) -> usize {
	lhs.iter().zip(rhs.iter()).take_while(|(left, right)| left == right).count()
}

fn rewrite_undo_log(undo_dir: &Path, source_path: &Path, entries: &[UndoHistoryEntry]) -> Result<Vec<u64>> {
	let log_path = undo_log_path_for_source(undo_dir, source_path);
	if let Some(parent) = log_path.parent() {
		std::fs::create_dir_all(parent)
			.with_context(|| format!("create undo log dir failed: {}", parent.display()))?;
	}

	let file = OpenOptions::new()
		.create(true)
		.write(true)
		.truncate(true)
		.open(&log_path)
		.with_context(|| format!("open undo log for rewrite failed: {}", log_path.display()))?;
	let mut writer = BufWriter::new(file);
	let mut offsets = Vec::with_capacity(entries.len());
	let mut current_len = 0u64;
	for entry in entries {
		let line = serde_json::to_string(entry).context("serialize undo entry failed")?;
		writer
			.write_all(line.as_bytes())
			.with_context(|| format!("write undo log entry failed: {}", log_path.display()))?;
		writer
			.write_all(b"\n")
			.with_context(|| format!("write undo log newline failed: {}", log_path.display()))?;
		current_len = current_len.saturating_add(line.len() as u64 + 1);
		offsets.push(current_len);
	}
	writer.flush().with_context(|| format!("flush undo log rewrite failed: {}", log_path.display()))?;
	Ok(offsets)
}

fn undo_log_truncate_offset(entry_end_offsets: &[u64], retained_entries: usize) -> u64 {
	if retained_entries == 0 {
		return 0;
	}
	entry_end_offsets.get(retained_entries.saturating_sub(1)).copied().unwrap_or(0)
}

fn truncate_undo_log(path: &Path, len: u64) -> Result<()> {
	let file = OpenOptions::new()
		.create(true)
		.write(true)
		.truncate(false)
		.open(path)
		.with_context(|| format!("open undo log for truncate failed: {}", path.display()))?;
	file.set_len(len).with_context(|| format!("truncate undo log failed: {}", path.display()))?;
	Ok(())
}

fn append_undo_log_entries_with_offsets(path: &Path, entries: &[UndoHistoryEntry]) -> Result<Vec<u64>> {
	if entries.is_empty() {
		return Ok(Vec::new());
	}

	let file = OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.with_context(|| format!("open undo log for append failed: {}", path.display()))?;
	let initial_len =
		file.metadata().with_context(|| format!("stat undo log failed: {}", path.display()))?.len();
	let mut writer = BufWriter::new(file);
	let mut offsets = Vec::with_capacity(entries.len());
	let mut current_len = initial_len;
	for entry in entries {
		let line = serde_json::to_string(entry).context("serialize undo entry failed")?;
		writer
			.write_all(line.as_bytes())
			.with_context(|| format!("append undo log entry failed: {}", path.display()))?;
		writer.write_all(b"\n").with_context(|| format!("append undo log newline failed: {}", path.display()))?;
		current_len = current_len.saturating_add(line.len() as u64 + 1);
		offsets.push(current_len);
	}
	writer.flush().with_context(|| format!("flush undo log append failed: {}", path.display()))?;
	Ok(offsets)
}

fn write_undo_meta(path: &Path, meta: &UndoMetaDocument) -> Result<()> {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)
			.with_context(|| format!("create undo meta dir failed: {}", parent.display()))?;
	}
	let content = serde_json::to_string(meta).context("serialize undo meta failed")?;
	std::fs::write(path, content).with_context(|| format!("write undo meta failed: {}", path.display()))?;
	Ok(())
}

fn remove_undo_history_files(undo_dir: &Path, source_path: &Path) -> Result<()> {
	remove_optional_file(undo_log_path_for_source(undo_dir, source_path).as_path(), "remove undo log failed")?;
	remove_optional_file(
		undo_meta_path_for_source(undo_dir, source_path).as_path(),
		"remove undo meta failed",
	)?;
	remove_optional_file(
		undo_legacy_path_for_source(undo_dir, source_path).as_path(),
		"remove legacy undo file failed",
	)?;
	Ok(())
}

fn remove_legacy_undo_file(undo_dir: &Path, source_path: &Path) -> Result<()> {
	remove_optional_file(
		undo_legacy_path_for_source(undo_dir, source_path).as_path(),
		"remove legacy undo file failed",
	)
}

fn remove_optional_file(path: &Path, context: &str) -> Result<()> {
	match std::fs::remove_file(path) {
		Ok(()) => Ok(()),
		Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
		Err(err) => Err(err).with_context(|| format!("{}: {}", context, path.display())),
	}
}

fn read_undo_log_entries(path: &Path) -> Result<(Vec<UndoHistoryEntry>, Vec<u64>)> {
	let content =
		std::fs::read_to_string(path).with_context(|| format!("read undo log failed: {}", path.display()))?;
	let mut entries = Vec::new();
	let mut entry_end_offsets = Vec::new();
	let mut current_len = 0u64;
	for raw_line in content.split_inclusive('\n') {
		current_len = current_len.saturating_add(raw_line.len() as u64);
		let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
		if line.is_empty() {
			continue;
		}
		let entry = serde_json::from_str::<UndoHistoryEntry>(line)
			.with_context(|| format!("parse undo log entry failed: {}", path.display()))?;
		entries.push(entry);
		entry_end_offsets.push(current_len);
	}
	Ok((entries, entry_end_offsets))
}

fn replay_undo_entries(base_text: &str, entries: &[UndoHistoryEntry]) -> Result<String> {
	let mut rope = Rope::from_str(base_text);
	for entry in entries {
		for edit in &entry.edits {
			apply_undo_edit_to_rope_redo(&mut rope, edit);
		}
	}
	Ok(rope.to_string())
}

fn apply_undo_edit_to_rope_undo(text: &mut Rope, delta: &UndoEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let inserted_end_byte = delta.start_byte.saturating_add(delta.inserted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(inserted_end_byte);
	text.remove(start_char..end_char);
	if !delta.deleted_text.is_empty() {
		text.insert(start_char, delta.deleted_text.as_str());
	}
}

fn apply_undo_edit_to_rope_redo(text: &mut Rope, delta: &UndoEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let deleted_end_byte = delta.start_byte.saturating_add(delta.deleted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(deleted_end_byte);
	text.remove(start_char..end_char);
	if !delta.inserted_text.is_empty() {
		text.insert(start_char, delta.inserted_text.as_str());
	}
}

fn write_swap_snapshot(path: &Path, pid: u32, username: &str, dirty: bool, base_text: &str) -> Result<()> {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)
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
	std::fs::write(path, content).with_context(|| format!("write swap snapshot failed: {}", path.display()))?;
	Ok(())
}

fn load_undo_history(
	undo_dir: &Path,
	source_path: &Path,
	expected_text: &str,
	undo_sessions: &mut HashMap<PathBuf, UndoHistorySession>,
) -> Result<Option<PersistedBufferHistory>> {
	let key = source_path.to_path_buf();
	let session = if let Some(session) = undo_sessions.get(key.as_path()).cloned() {
		session
	} else if let Some(loaded) = load_undo_session_from_disk(undo_dir, source_path)? {
		undo_sessions.insert(key.clone(), loaded.clone());
		loaded
	} else {
		let empty = UndoHistorySession::empty(expected_text.to_string());
		undo_sessions.insert(key, empty);
		return Ok(None);
	};

	if session.current_text != expected_text {
		undo_sessions.insert(source_path.to_path_buf(), UndoHistorySession::empty(expected_text.to_string()));
		return Ok(None);
	}

	Ok(Some(session.to_persisted_history()))
}

fn save_undo_history(
	undo_dir: &Path,
	source_path: &Path,
	history: &PersistedBufferHistory,
	undo_sessions: &mut HashMap<PathBuf, UndoHistorySession>,
) -> Result<()> {
	let key = source_path.to_path_buf();
	let mut session = if let Some(session) = undo_sessions.remove(key.as_path()) {
		session
	} else {
		load_undo_session_from_disk(undo_dir, source_path)?
			.unwrap_or_else(|| UndoHistorySession::empty(history.current_text.clone()))
	};

	sync_undo_history_session(undo_dir, source_path, &mut session, history)?;
	undo_sessions.insert(key, session);
	Ok(())
}

fn load_undo_session_from_disk(undo_dir: &Path, source_path: &Path) -> Result<Option<UndoHistorySession>> {
	if let Some(session) = load_undo_session_from_new_format(undo_dir, source_path)? {
		return Ok(Some(session));
	}
	if let Some(history) = load_legacy_undo_history(undo_dir, source_path)? {
		return Ok(Some(UndoHistorySession::from_persisted_history(&history)?));
	}
	Ok(None)
}

fn load_undo_session_from_new_format(
	undo_dir: &Path,
	source_path: &Path,
) -> Result<Option<UndoHistorySession>> {
	let meta_path = undo_meta_path_for_source(undo_dir, source_path);
	let log_path = undo_log_path_for_source(undo_dir, source_path);
	if !meta_path.exists() && !log_path.exists() {
		return Ok(None);
	}
	if !meta_path.exists() || !log_path.exists() {
		bail!("incomplete undo persistence files: meta={} log={}", meta_path.display(), log_path.display());
	}

	let meta_text = std::fs::read_to_string(&meta_path)
		.with_context(|| format!("read undo meta failed: {}", meta_path.display()))?;
	let meta: UndoMetaDocument = serde_json::from_str(meta_text.as_str())
		.with_context(|| format!("parse undo meta failed: {}", meta_path.display()))?;
	if meta.version != UNDO_FILE_VERSION {
		bail!("unsupported undo meta version {} in {}", meta.version, meta_path.display());
	}

	let (entries, entry_end_offsets) = read_undo_log_entries(log_path.as_path())?;
	if entries.len() != meta.entry_count {
		bail!(
			"undo entry count mismatch: meta={} log={} file={}",
			meta.entry_count,
			entries.len(),
			source_path.display()
		);
	}
	if meta.head > entries.len() {
		bail!("undo head {} exceeds entry count {} for {}", meta.head, entries.len(), source_path.display());
	}

	let current_text = replay_undo_entries(meta.base_text.as_str(), &entries[..meta.head])?;
	Ok(Some(UndoHistorySession {
		base_text: meta.base_text,
		entries,
		head: meta.head,
		current_cursor: meta.current_cursor.into(),
		current_text,
		entry_end_offsets,
	}))
}

fn load_legacy_undo_history(undo_dir: &Path, source_path: &Path) -> Result<Option<PersistedBufferHistory>> {
	let legacy_path = undo_legacy_path_for_source(undo_dir, source_path);
	if !legacy_path.exists() {
		return Ok(None);
	}

	let content = std::fs::read_to_string(&legacy_path)
		.with_context(|| format!("read legacy undo file failed: {}", legacy_path.display()))?;
	let document: LegacyUndoFileDocument = serde_json::from_str(content.as_str())
		.with_context(|| format!("parse legacy undo file failed: {}", legacy_path.display()))?;
	if document.version != UNDO_FILE_VERSION {
		bail!("unsupported legacy undo file version {} in {}", document.version, legacy_path.display());
	}
	Ok(Some(document.into()))
}

fn sync_undo_history_session(
	undo_dir: &Path,
	source_path: &Path,
	session: &mut UndoHistorySession,
	history: &PersistedBufferHistory,
) -> Result<()> {
	let new_entries = linear_history_entries_from_snapshot(history);
	let new_head = history.undo_stack.len();
	if new_entries.is_empty() {
		remove_undo_history_files(undo_dir, source_path)?;
		*session = UndoHistorySession {
			base_text:         history.current_text.clone(),
			entries:           Vec::new(),
			head:              0,
			current_cursor:    history.cursor,
			current_text:      history.current_text.clone(),
			entry_end_offsets: Vec::new(),
		};
		return Ok(());
	}

	let base_text = if is_base_text_consistent(
		session.base_text.as_str(),
		new_entries.as_slice(),
		new_head,
		history.current_text.as_str(),
	)? {
		session.base_text.clone()
	} else {
		derive_base_text_from_snapshot(history)?
	};

	let common_prefix = longest_common_undo_entry_prefix(session.entries.as_slice(), new_entries.as_slice());
	let can_truncate_existing_tail = session.entry_end_offsets.len() == session.entries.len();
	if session.base_text != base_text
		|| (!session.entries.is_empty() && common_prefix < session.entries.len() && !can_truncate_existing_tail)
	{
		session.entries = new_entries.to_vec();
		session.entry_end_offsets = rewrite_undo_log(undo_dir, source_path, session.entries.as_slice())?;
	} else {
		if common_prefix < session.entries.len() {
			let truncate_len = undo_log_truncate_offset(session.entry_end_offsets.as_slice(), common_prefix);
			truncate_undo_log(undo_log_path_for_source(undo_dir, source_path).as_path(), truncate_len)?;
			session.entries.truncate(common_prefix);
			session.entry_end_offsets.truncate(common_prefix);
		}
		if common_prefix < new_entries.len() {
			let appended = new_entries[common_prefix..].to_vec();
			let appended_offsets = append_undo_log_entries_with_offsets(
				undo_log_path_for_source(undo_dir, source_path).as_path(),
				appended.as_slice(),
			)?;
			session.entries.extend(appended);
			session.entry_end_offsets.extend(appended_offsets);
		}
	}

	session.base_text = base_text.clone();
	session.head = new_head;
	session.current_cursor = history.cursor;
	session.current_text = history.current_text.clone();
	write_undo_meta(undo_meta_path_for_source(undo_dir, source_path).as_path(), &UndoMetaDocument {
		version: UNDO_FILE_VERSION,
		base_text,
		head: new_head,
		entry_count: session.entries.len(),
		current_cursor: history.cursor.into(),
	})?;
	remove_legacy_undo_file(undo_dir, source_path)?;
	Ok(())
}

#[cfg(test)]
fn append_swap_ops(path: &Path, ops: &[SwapEditOp]) -> Result<()> {
	if ops.is_empty() {
		return Ok(());
	}
	let _ = append_swap_ops_with_offsets(path, ops.iter())?;
	Ok(())
}

fn append_swap_ops_with_offsets<'a>(
	path: &Path,
	ops: impl IntoIterator<Item = &'a SwapEditOp>,
) -> Result<Vec<u64>> {
	let file = OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.with_context(|| format!("open swap file for append failed: {}", path.display()))?;
	let initial_len =
		file.metadata().with_context(|| format!("stat swap append file failed: {}", path.display()))?.len();
	let mut writer = BufWriter::new(file);
	let mut offsets = Vec::new();
	let mut current_len = initial_len;
	for op in ops {
		let line = match op {
			SwapEditOp::Insert { pos, text } => format!("I\t{}\t{}\n", pos, encode_text_field(text)),
			SwapEditOp::Delete { pos, len } => format!("D\t{}\t{}\n", pos, len),
		};
		writer
			.write_all(line.as_bytes())
			.with_context(|| format!("append swap op failed: {}", path.display()))?;
		current_len = current_len.saturating_add(line.len() as u64);
		offsets.push(current_len);
	}
	writer.flush().with_context(|| format!("flush swap append failed: {}", path.display()))?;
	Ok(offsets)
}

fn append_buffered_swap_ops(path: &Path, ops: &[BufferedSwapOp]) -> Result<Vec<u64>> {
	if ops.is_empty() {
		return Ok(Vec::new());
	}
	append_swap_ops_with_offsets(path, ops.iter().map(|op| &op.op))
}

fn remove_string_char_range(text: &mut String, start_char: usize, end_char: usize) {
	let start_byte = char_index_to_byte_index(text.as_str(), start_char);
	let end_byte = char_index_to_byte_index(text.as_str(), end_char);
	text.replace_range(start_byte..end_byte, "");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TailCompaction {
	None,
	RemovedLast,
	MutatedLast,
}

fn compact_buffered_op_against_logged_ops(
	logged_ops: &mut Vec<BufferedSwapOp>,
	op: &BufferedSwapOp,
) -> TailCompaction {
	match &op.op {
		SwapEditOp::Delete { pos, len } => compact_delete_against_insert_tail(logged_ops, *pos, *len),
		SwapEditOp::Insert { pos, text } => compact_insert_against_delete_tail(logged_ops, *pos, text.as_str()),
	}
}

fn compact_logged_suffix_against_pending_prefix(
	snapshot_rope: Option<&Rope>,
	logged_ops: &[BufferedSwapOp],
	pending_ops: &[BufferedSwapOp],
	current_rope: &Rope,
) -> (Vec<BufferedSwapOp>, Vec<BufferedSwapOp>, bool) {
	let Some(snapshot_rope) = snapshot_rope else {
		return (logged_ops.to_vec(), pending_ops.to_vec(), false);
	};

	let mut best: Option<(usize, usize)> = None;
	for logged_suffix_len in 1..=logged_ops.len() {
		for pending_prefix_len in 1..=pending_ops.len() {
			let mut candidate = snapshot_rope.clone();
			apply_buffered_ops(&mut candidate, &logged_ops[..logged_ops.len().saturating_sub(logged_suffix_len)]);
			apply_buffered_ops(&mut candidate, &pending_ops[pending_prefix_len..]);
			if candidate == *current_rope {
				let should_replace = best.is_none_or(|(best_logged, best_pending)| {
					logged_suffix_len + pending_prefix_len > best_logged + best_pending
				});
				if should_replace {
					best = Some((logged_suffix_len, pending_prefix_len));
				}
			}
		}
	}

	let Some((logged_suffix_len, pending_prefix_len)) = best else {
		return (logged_ops.to_vec(), pending_ops.to_vec(), false);
	};

	(
		logged_ops[..logged_ops.len().saturating_sub(logged_suffix_len)].to_vec(),
		pending_ops[pending_prefix_len..].to_vec(),
		true,
	)
}

fn compact_delete_against_insert_tail(
	ops: &mut Vec<BufferedSwapOp>,
	pos: usize,
	len: usize,
) -> TailCompaction {
	let Some(BufferedSwapOp { op: SwapEditOp::Insert { pos: insert_pos, text: insert_text }, .. }) =
		ops.last_mut()
	else {
		return TailCompaction::None;
	};

	let insert_len = insert_text.chars().count();
	let insert_end = insert_pos.saturating_add(insert_len);
	let delete_end = pos.saturating_add(len);
	if pos < *insert_pos || delete_end > insert_end {
		return TailCompaction::None;
	}

	let relative_start = pos - *insert_pos;
	let relative_end = relative_start + len;
	remove_string_char_range(insert_text, relative_start, relative_end);
	if insert_text.is_empty() {
		ops.pop();
		return TailCompaction::RemovedLast;
	}
	TailCompaction::MutatedLast
}

fn compact_insert_against_delete_tail(
	ops: &mut Vec<BufferedSwapOp>,
	pos: usize,
	text: &str,
) -> TailCompaction {
	let Some(BufferedSwapOp {
		op: SwapEditOp::Delete { pos: delete_pos, len: delete_len },
		deleted_text: Some(deleted_text),
	}) = ops.last()
	else {
		return TailCompaction::None;
	};

	if *delete_pos != pos || *delete_len != text.chars().count() || deleted_text != text {
		return TailCompaction::None;
	}

	ops.pop();
	TailCompaction::RemovedLast
}

fn apply_buffered_ops(text: &mut Rope, ops: &[BufferedSwapOp]) {
	for op in ops {
		apply_swap_op(text, op.op.clone());
	}
}

fn char_index_to_byte_index(text: &str, char_index: usize) -> usize {
	if char_index == 0 {
		return 0;
	}
	text.char_indices().nth(char_index).map_or(text.len(), |(byte_index, _)| byte_index)
}

fn truncate_swap_file(path: &Path, len: u64) -> Result<()> {
	let file = OpenOptions::new()
		.write(true)
		.open(path)
		.with_context(|| format!("open swap file for truncate failed: {}", path.display()))?;
	file.set_len(len).with_context(|| format!("truncate swap file failed: {}", path.display()))?;
	Ok(())
}

fn parse_swap_file(path: &Path) -> Result<ParsedSwapFile> {
	let content =
		std::fs::read_to_string(path).with_context(|| format!("read swap file failed: {}", path.display()))?;
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

fn apply_swap_op(rope: &mut Rope, op: SwapEditOp) {
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

fn touch_swap_lease_file(lease_path: &Path) {
	if let Some(parent) = lease_path.parent()
		&& let Err(err) = std::fs::create_dir_all(parent)
	{
		error!("create lease dir failed: {} error={}", parent.display(), err);
		return;
	}
	if let Err(err) = std::fs::write(lease_path, std::process::id().to_string()) {
		error!("write lease file failed: {} error={}", lease_path.display(), err);
	}
}

fn remove_swap_lease_file(lease_path: &Path) {
	if let Err(err) = std::fs::remove_file(lease_path)
		&& err.kind() != std::io::ErrorKind::NotFound
	{
		error!("remove lease file failed: {} error={}", lease_path.display(), err);
	}
}

fn has_other_swap_leases(self_lease_path: &Path, source_path: &Path) -> bool {
	let Some(lease_dir) = self_lease_path.parent() else {
		return true;
	};
	let self_lease_name =
		self_lease_path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
	let lease_prefix = swap_lease_file_prefix(source_path);

	let entries = match std::fs::read_dir(lease_dir) {
		Ok(entries) => entries,
		Err(err) => {
			error!("read lease dir failed: {} error={}", lease_dir.display(), err);
			// Conservative: keep swap if lease scan fails.
			return true;
		}
	};

	for entry in entries {
		let Ok(entry) = entry else {
			continue;
		};
		let file_name = entry.file_name();
		let file_name = file_name.to_string_lossy();
		if !file_name.starts_with(lease_prefix.as_str()) || !file_name.ends_with(".lease") {
			continue;
		}
		if file_name == self_lease_name {
			continue;
		}
		let lease_path = entry.path();
		let Some(pid) = parse_pid_from_lease_name(file_name.as_ref(), lease_prefix.as_str()) else {
			return true;
		};
		if is_process_alive(pid) {
			return true;
		}
		remove_swap_lease_file(lease_path.as_path());
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

fn swap_path_for_source(swap_dir: &Path, source_path: &Path) -> PathBuf {
	swap_dir.join(format!("{}.swp", encode_source_path_for_file_name(source_path)))
}

fn undo_log_path_for_source(undo_dir: &Path, source_path: &Path) -> PathBuf {
	undo_dir.join(format!("{}.undo.log", encode_source_path_for_file_name(source_path)))
}

fn undo_meta_path_for_source(undo_dir: &Path, source_path: &Path) -> PathBuf {
	undo_dir.join(format!("{}.undo.meta", encode_source_path_for_file_name(source_path)))
}

fn undo_legacy_path_for_source(undo_dir: &Path, source_path: &Path) -> PathBuf {
	undo_dir.join(format!("{}.undo", encode_source_path_for_file_name(source_path)))
}

fn swap_lease_path_for_source(swap_dir: &Path, source_path: &Path, pid: u32) -> PathBuf {
	swap_dir.join(format!("{}.{}.lease", encode_source_path_for_file_name(source_path), pid))
}

fn encode_source_path_for_file_name(source_path: &Path) -> String {
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

fn normalize_source_path_for_persistence(source_path: &Path) -> PathBuf {
	let rendered = source_path.to_string_lossy();
	let normalized = normalize_source_path_text(rendered.as_ref());
	PathBuf::from(normalized.as_ref())
}

fn normalize_source_path_text(raw: &str) -> Cow<'_, str> {
	if let Some(remainder) = raw.strip_prefix(r"\\?\").or_else(|| raw.strip_prefix(r"//?/")) {
		return Cow::Borrowed(remainder);
	}

	Cow::Borrowed(raw)
}

fn swap_lease_file_prefix(source_path: &Path) -> String {
	format!("{}.", encode_source_path_for_file_name(source_path))
}

fn source_path_from_swap_storage_path(storage_path: &Path) -> Result<PathBuf> {
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

fn parse_meta_line(meta_line: &str, path: &Path) -> Result<(u32, String, Option<String>, bool)> {
	let meta_fields = meta_line.split('\t').collect::<Vec<_>>();
	if meta_fields.first().copied() != Some("META") {
		bail!("invalid swap meta line: {}", path.display());
	}

	// Legacy format:
	// META <pid> <username_b64> <source_path_b64> <dirty_flag>
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

	// Current format:
	// META pid=<n> user=<json_str> dirty=<0|1>
	// Backward-compatible readable format:
	// META pid=<n> user=<json_str> source=<json_str> dirty=<0|1>
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
	// Backward compatibility for legacy base64-encoded fields.
	if let Ok(decoded) = decode_b64(encoded) {
		return Ok(decoded);
	}
	Ok(encoded.to_string())
}

fn current_username() -> String {
	std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "unknown".to_string())
}

fn user_swap_dir() -> PathBuf { user_state_root().join("swp") }

fn user_undo_dir() -> PathBuf { user_state_root().join("undo") }

#[cfg(test)]
mod tests {
	use std::time::{SystemTime, UNIX_EPOCH};

	use super::*;

	fn make_tmp_dir(test_name: &str) -> PathBuf {
		let nanos =
			SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock before unix epoch").as_nanos();
		let dir =
			std::env::temp_dir().join(format!("rim-swap-test-{}-{}-{}", test_name, std::process::id(), nanos));
		std::fs::create_dir_all(&dir).expect("create temp test dir failed");
		dir
	}

	fn buffered_ops_to_plain(ops: &[BufferedSwapOp]) -> Vec<SwapEditOp> {
		ops.iter().map(|op| op.op.clone()).collect()
	}

	#[test]
	fn apply_edit_should_merge_adjacent_insert_ops_within_window() {
		let swap_dir = make_tmp_dir("merge");
		let source_path = swap_dir.join("sample.txt");
		let mut session = SwapSession::new(
			BufferId::default(),
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);
		session.rope = Rope::from_str("abc");
		session.ensure_snapshot_initialized().expect("snapshot init failed");

		let now = Instant::now();
		session.apply_edit(SwapEditOp::Insert { pos: 1, text: "x".to_string() }, now).expect("first edit failed");
		session
			.apply_edit(SwapEditOp::Insert { pos: 2, text: "y".to_string() }, now + Duration::from_millis(80))
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
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);
		session.recover("abc".to_string()).expect("recover init failed");

		let now = Instant::now();
		session.apply_edit(SwapEditOp::Insert { pos: 1, text: "xy".to_string() }, now).expect("insert failed");
		session
			.apply_edit(SwapEditOp::Delete { pos: 1, len: 2 }, now + Duration::from_millis(40))
			.expect("delete failed");

		assert!(session.pending_ops.is_empty());
		assert_eq!(session.rope.to_string(), "abc");
		assert!(!session.dirty);

		session.flush_if_due(now + Duration::from_millis(300)).expect("flush failed");
		let parsed = parse_swap_file(session.swap_path.as_path()).expect("parse swap failed");
		assert!(parsed.ops.is_empty());
		assert!(!parsed.dirty);
	}

	#[test]
	fn apply_edit_should_shrink_pending_insert_when_delete_removes_part_of_it() {
		let swap_dir = make_tmp_dir("shrink-insert");
		let source_path = swap_dir.join("sample.txt");
		let mut session = SwapSession::new(
			BufferId::default(),
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);
		session.recover("abc".to_string()).expect("recover init failed");

		let now = Instant::now();
		session.apply_edit(SwapEditOp::Insert { pos: 1, text: "xyz".to_string() }, now).expect("insert failed");
		session
			.apply_edit(SwapEditOp::Delete { pos: 2, len: 1 }, now + Duration::from_millis(40))
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
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);
		session.recover("abc".to_string()).expect("recover init failed");

		let now = Instant::now();
		session.apply_edit(SwapEditOp::Delete { pos: 1, len: 1 }, now).expect("delete failed");
		session
			.apply_edit(SwapEditOp::Insert { pos: 1, text: "b".to_string() }, now + Duration::from_millis(40))
			.expect("insert failed");

		assert!(session.pending_ops.is_empty());
		assert_eq!(session.rope.to_string(), "abc");
		assert!(!session.dirty);

		session.flush_if_due(now + Duration::from_millis(300)).expect("flush failed");
		let parsed = parse_swap_file(session.swap_path.as_path()).expect("parse swap failed");
		assert!(parsed.ops.is_empty());
		assert!(!parsed.dirty);
	}

	#[test]
	fn flush_pending_should_remove_logged_insert_when_later_delete_reverts_it() {
		let swap_dir = make_tmp_dir("rewrite-logged-insert");
		let source_path = swap_dir.join("sample.txt");
		let mut session = SwapSession::new(
			BufferId::default(),
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);
		session.recover("abc".to_string()).expect("recover init failed");

		let now = Instant::now();
		session.apply_edit(SwapEditOp::Insert { pos: 1, text: "x".to_string() }, now).expect("insert failed");
		session.flush_if_due(now + Duration::from_millis(300)).expect("first flush failed");

		let parsed_after_insert =
			parse_swap_file(session.swap_path.as_path()).expect("parse after insert failed");
		assert_eq!(parsed_after_insert.ops, vec![SwapEditOp::Insert { pos: 1, text: "x".to_string() }]);
		assert!(parsed_after_insert.dirty || !parsed_after_insert.ops.is_empty());
		assert_eq!(session.logged_end_offsets.len(), 1);
		let logged_len_after_insert =
			std::fs::metadata(session.swap_path.as_path()).expect("stat swap after insert failed").len();
		assert!(logged_len_after_insert > session.snapshot_len);

		session
			.apply_edit(SwapEditOp::Delete { pos: 1, len: 1 }, now + Duration::from_millis(400))
			.expect("delete failed");
		session.flush_if_due(now + Duration::from_millis(700)).expect("second flush failed");

		let parsed_after_delete =
			parse_swap_file(session.swap_path.as_path()).expect("parse after delete failed");
		assert!(parsed_after_delete.ops.is_empty());
		assert!(!parsed_after_delete.dirty);
		assert_eq!(session.rope.to_string(), "abc");
		assert!(!session.dirty);
		assert!(session.logged_end_offsets.is_empty());
		let logged_len_after_delete =
			std::fs::metadata(session.swap_path.as_path()).expect("stat swap after delete failed").len();
		assert_eq!(logged_len_after_delete, session.snapshot_len);
	}

	#[test]
	fn flush_pending_should_remove_logged_delete_when_later_insert_reverts_it() {
		let swap_dir = make_tmp_dir("rewrite-logged-delete");
		let source_path = swap_dir.join("sample.txt");
		let mut session = SwapSession::new(
			BufferId::default(),
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);
		session.recover("abc".to_string()).expect("recover init failed");

		let now = Instant::now();
		session.apply_edit(SwapEditOp::Delete { pos: 1, len: 1 }, now).expect("delete failed");
		session.flush_if_due(now + Duration::from_millis(300)).expect("first flush failed");

		let parsed_after_delete =
			parse_swap_file(session.swap_path.as_path()).expect("parse after delete failed");
		assert_eq!(parsed_after_delete.ops, vec![SwapEditOp::Delete { pos: 1, len: 1 }]);
		assert!(parsed_after_delete.dirty || !parsed_after_delete.ops.is_empty());

		session
			.apply_edit(SwapEditOp::Insert { pos: 1, text: "b".to_string() }, now + Duration::from_millis(400))
			.expect("insert failed");
		session.flush_if_due(now + Duration::from_millis(700)).expect("second flush failed");

		let parsed_after_insert =
			parse_swap_file(session.swap_path.as_path()).expect("parse after insert failed");
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
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);
		session.recover("abc\ndef".to_string()).expect("recover init failed");

		let now = Instant::now();
		session
			.apply_edit(SwapEditOp::Insert { pos: 1, text: "X".to_string() }, now)
			.expect("first insert failed");
		session
			.apply_edit(SwapEditOp::Insert { pos: 6, text: "X".to_string() }, now + Duration::from_millis(20))
			.expect("second insert failed");
		session.flush_if_due(now + Duration::from_millis(300)).expect("first flush failed");

		let parsed_after_insert =
			parse_swap_file(session.swap_path.as_path()).expect("parse after insert failed");
		assert_eq!(parsed_after_insert.ops, vec![
			SwapEditOp::Insert { pos: 1, text: "X".to_string() },
			SwapEditOp::Insert { pos: 6, text: "X".to_string() },
		]);

		session
			.apply_edit(SwapEditOp::Delete { pos: 1, len: 1 }, now + Duration::from_millis(400))
			.expect("first delete failed");
		session
			.apply_edit(SwapEditOp::Delete { pos: 5, len: 1 }, now + Duration::from_millis(420))
			.expect("second delete failed");
		session.flush_if_due(now + Duration::from_millis(700)).expect("second flush failed");

		let parsed_after_undo = parse_swap_file(session.swap_path.as_path()).expect("parse after undo failed");
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
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);
		session.recover("abc".to_string()).expect("recover init failed");

		let now = Instant::now();
		session.apply_edit(SwapEditOp::Insert { pos: 3, text: "!".to_string() }, now).expect("apply edit failed");

		session.flush_if_due(now + Duration::from_millis(60)).expect("early flush failed");
		let parsed_before = parse_swap_file(session.swap_path.as_path()).expect("parse before flush failed");
		assert!(parsed_before.ops.is_empty());

		session.flush_if_due(now + Duration::from_millis(300)).expect("flush after debounce failed");
		let parsed_after = parse_swap_file(session.swap_path.as_path()).expect("parse after flush failed");
		assert_eq!(parsed_after.ops, vec![SwapEditOp::Insert { pos: 3, text: "!".to_string() }]);
	}

	#[test]
	fn recover_should_replay_existing_swap_edit_log() {
		let swap_dir = make_tmp_dir("recover");
		let source_path = swap_dir.join("sample.txt");
		let mut session = SwapSession::new(
			BufferId::default(),
			source_path.clone(),
			swap_dir.as_path(),
			123,
			"tester".to_string(),
		);

		write_swap_snapshot(session.swap_path.as_path(), 999, "old-user", true, "abc")
			.expect("write test snapshot failed");
		append_swap_ops(session.swap_path.as_path(), &[
			SwapEditOp::Delete { pos: 1, len: 1 },
			SwapEditOp::Insert { pos: 2, text: "Z".to_string() },
		])
		.expect("append test swap ops failed");

		let recovered = session.recover("abc".to_string()).expect("recover failed");
		assert_eq!(recovered, Some("acZ".to_string()));
	}

	#[test]
	fn drop_should_remove_swap_file() {
		let swap_dir = make_tmp_dir("drop");
		let source_path = swap_dir.join("sample.txt");
		let swap_path;
		{
			let mut session = SwapSession::new(
				BufferId::default(),
				source_path.clone(),
				swap_dir.as_path(),
				123,
				"tester".to_string(),
			);
			session.recover("hello".to_string()).expect("recover init failed");
			swap_path = session.swap_path.clone();
			assert!(swap_path.exists());
		}

		assert!(!swap_path.exists());
	}

	#[test]
	fn swap_io_state_drop_should_shutdown_worker_and_cleanup_swap_file() {
		let swap_dir = make_tmp_dir("state-drop");
		let source_path = swap_dir.join("sample.txt");
		let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());
		let (event_tx, _event_rx) = flume::unbounded();
		let mut state = PersistenceIoState::new(event_tx);
		state.swap_dir = swap_dir.clone();
		state.start();

		state
			.request_tx
			.send(PersistenceRequest::Open { buffer_id: BufferId::default(), source_path: source_path.clone() })
			.expect("send open failed");
		state
			.request_tx
			.send(PersistenceRequest::InitializeBase {
				buffer_id:       BufferId::default(),
				source_path:     source_path.clone(),
				base_text:       "hello".to_string(),
				delete_existing: false,
			})
			.expect("send initialize base failed");
		for _ in 0..50 {
			if swap_path.exists() {
				break;
			}
			std::thread::sleep(Duration::from_millis(20));
		}
		assert!(swap_path.exists());

		drop(state);
		assert!(!swap_path.exists());
	}

	#[test]
	fn recover_without_existing_swap_should_not_emit_recover_completed_event() {
		let swap_dir = make_tmp_dir("recover-no-swap-event");
		let source_path = swap_dir.join("sample.txt");
		let (event_tx, event_rx) = flume::unbounded();
		let mut state = PersistenceIoState::new(event_tx);
		state.swap_dir = swap_dir;
		state.start();

		state
			.request_tx
			.send(PersistenceRequest::Recover {
				buffer_id: BufferId::default(),
				source_path,
				base_text: "hello".to_string(),
			})
			.expect("send recover failed");

		let result = event_rx.recv_timeout(Duration::from_millis(200));
		assert!(matches!(result, Err(flume::RecvTimeoutError::Timeout)));
	}

	#[test]
	fn drop_should_keep_swap_file_when_other_process_lease_exists() {
		let swap_dir = make_tmp_dir("drop-lease");
		let source_path = swap_dir.join("sample.txt");
		let swap_path;
		let peer_pid = std::process::id();
		let peer_lease = swap_lease_path_for_source(swap_dir.as_path(), source_path.as_path(), peer_pid);
		touch_swap_lease_file(peer_lease.as_path());

		{
			let mut session = SwapSession::new(
				BufferId::default(),
				source_path.clone(),
				swap_dir.as_path(),
				123,
				"tester".to_string(),
			);
			session.recover("hello".to_string()).expect("recover init failed");
			swap_path = session.swap_path.clone();
			assert!(swap_path.exists());
		}

		assert!(swap_path.exists());
		std::fs::remove_file(&swap_path).expect("cleanup swap file failed");
		std::fs::remove_file(&peer_lease).expect("cleanup lease file failed");
	}

	#[test]
	fn drop_should_remove_swap_file_when_only_stale_peer_lease_exists() {
		let swap_dir = make_tmp_dir("drop-stale-lease");
		let source_path = swap_dir.join("sample.txt");
		let stale_pid = 999_999;
		let stale_lease = swap_lease_path_for_source(swap_dir.as_path(), source_path.as_path(), stale_pid);
		touch_swap_lease_file(stale_lease.as_path());
		let swap_path;

		{
			let mut session = SwapSession::new(
				BufferId::default(),
				source_path.clone(),
				swap_dir.as_path(),
				123,
				"tester".to_string(),
			);
			session.recover("hello".to_string()).expect("recover init failed");
			swap_path = session.swap_path.clone();
			assert!(swap_path.exists());
		}

		assert!(!swap_path.exists());
		assert!(!stale_lease.exists());
	}

	#[test]
	fn parse_swap_file_should_read_metadata() {
		let swap_dir = make_tmp_dir("meta");
		let source_path = swap_dir.join("sample.txt");
		let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());
		write_swap_snapshot(swap_path.as_path(), 77, "user-a", false, "base").expect("write snapshot failed");

		let parsed = parse_swap_file(swap_path.as_path()).expect("parse swap file failed");
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

		write_swap_snapshot(swap_path.as_path(), 42, "tester", true, "a\nb\t中").expect("write snapshot failed");
		append_swap_ops(swap_path.as_path(), &[
			SwapEditOp::Insert { pos: 3, text: "xy\n\t".to_string() },
			SwapEditOp::Delete { pos: 1, len: 1 },
		])
		.expect("append ops failed");

		let raw = std::fs::read_to_string(&swap_path).expect("read raw swap failed");
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
		std::fs::create_dir_all(swap_path.parent().expect("legacy swap parent should exist"))
			.expect("create legacy swap parent failed");

		let legacy = format!(
			"{}\nMETA\t7\t{}\t{}\t1\nBASE\t{}\nI\t3\t{}\nD\t1\t1\n",
			SWAP_FILE_MAGIC,
			STANDARD_NO_PAD.encode("legacy-user".as_bytes()),
			STANDARD_NO_PAD.encode(source_path.display().to_string().as_bytes()),
			STANDARD_NO_PAD.encode("abc".as_bytes()),
			STANDARD_NO_PAD.encode("Z".as_bytes()),
		);
		std::fs::write(&swap_path, legacy).expect("write legacy swap failed");

		let parsed = parse_swap_file(swap_path.as_path()).expect("parse legacy swap failed");
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

		save_undo_history(undo_dir.as_path(), source_path.as_path(), &history, &mut undo_sessions)
			.expect("save undo history failed");
		assert!(undo_log_path_for_source(undo_dir.as_path(), source_path.as_path()).exists());
		assert!(undo_meta_path_for_source(undo_dir.as_path(), source_path.as_path()).exists());

		let loaded = load_undo_history(undo_dir.as_path(), source_path.as_path(), "axbc", &mut undo_sessions)
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
		save_undo_history(undo_dir.as_path(), source_path.as_path(), &original, &mut undo_sessions)
			.expect("seed undo history failed");
		let log_len_before = std::fs::metadata(&log_path).expect("stat undo log before branch failed").len();

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
		save_undo_history(undo_dir.as_path(), source_path.as_path(), &branched, &mut undo_sessions)
			.expect("save branched undo history failed");

		let raw_log = std::fs::read_to_string(&log_path).expect("read undo log after branch failed");
		assert!(!raw_log.contains("\"x\""));
		assert!(raw_log.contains("\"y\""));
		let log_len_after = std::fs::metadata(&log_path).expect("stat undo log after branch failed").len();
		assert!(log_len_after <= log_len_before);

		let loaded = load_undo_history(undo_dir.as_path(), source_path.as_path(), "aybc", &mut undo_sessions)
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
		save_undo_history(undo_dir.as_path(), source_path.as_path(), &history, &mut undo_sessions)
			.expect("seed undo file failed");
		assert!(undo_log_path.exists());
		assert!(undo_meta_path.exists());

		save_undo_history(
			undo_dir.as_path(),
			source_path.as_path(),
			&PersistedBufferHistory {
				current_text: "abc".to_string(),
				cursor:       CursorState { row: 1, col: 1 },
				undo_stack:   Vec::new(),
				redo_stack:   Vec::new(),
			},
			&mut undo_sessions,
		)
		.expect("clear undo file failed");

		assert!(!undo_log_path.exists());
		assert!(!undo_meta_path.exists());
	}
}
