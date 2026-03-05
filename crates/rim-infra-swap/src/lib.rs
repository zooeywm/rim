use std::{collections::HashMap, fs::OpenOptions, hash::{Hash, Hasher}, io::{BufWriter, Write}, path::{Path, PathBuf}, process::{Command, Stdio}, sync::Mutex, thread, time::{Duration, Instant}};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use rim_kernel::{action::{AppAction, FileAction, SwapConflictInfo}, ports::{SwapEditOp, SwapIo, SwapIoError}, state::BufferId};
use ropey::Rope;
use tracing::{error, info};

const SWAP_FILE_MAGIC: &str = "RIMSWP\t1";
const REQUEST_POLL_INTERVAL: Duration = Duration::from_millis(40);
const FLUSH_DEBOUNCE_WINDOW: Duration = Duration::from_millis(180);
const INSERT_MERGE_WINDOW: Duration = Duration::from_millis(350);

#[derive(dep_inj::DepInj)]
#[target(SwapIoImpl)]
pub struct SwapIoState {
	request_tx:  flume::Sender<SwapRequest>,
	request_rx:  flume::Receiver<SwapRequest>,
	event_tx:    flume::Sender<AppAction>,
	swap_dir:    PathBuf,
	worker_join: Mutex<Option<thread::JoinHandle<()>>>,
}

impl AsRef<SwapIoState> for SwapIoState {
	fn as_ref(&self) -> &SwapIoState { self }
}

impl<Deps> SwapIo for SwapIoImpl<Deps>
where Deps: AsRef<SwapIoState>
{
	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError> {
		self.request_tx.send(SwapRequest::Open { buffer_id, source_path }).map_err(|err| {
			error!("enqueue_open failed: swap request channel is disconnected: {}", err);
			SwapIoError::RequestChannelDisconnected { operation: "open" }
		})
	}

	fn enqueue_detect_conflict(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError> {
		self.request_tx.send(SwapRequest::DetectConflict { buffer_id, source_path }).map_err(|err| {
			error!("enqueue_detect_conflict failed: swap request channel is disconnected: {}", err);
			SwapIoError::RequestChannelDisconnected { operation: "detect_conflict" }
		})
	}

	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), SwapIoError> {
		self.request_tx.send(SwapRequest::Edit { buffer_id, source_path, op }).map_err(|err| {
			error!("enqueue_edit failed: swap request channel is disconnected: {}", err);
			SwapIoError::RequestChannelDisconnected { operation: "edit" }
		})
	}

	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError> {
		self.request_tx.send(SwapRequest::MarkClean { buffer_id, source_path }).map_err(|err| {
			error!("enqueue_mark_clean failed: swap request channel is disconnected: {}", err);
			SwapIoError::RequestChannelDisconnected { operation: "mark_clean" }
		})
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), SwapIoError> {
		self
			.request_tx
			.send(SwapRequest::InitializeBase { buffer_id, source_path, base_text, delete_existing })
			.map_err(|err| {
				error!("enqueue_initialize_base failed: swap request channel is disconnected: {}", err);
				SwapIoError::RequestChannelDisconnected { operation: "initialize_base" }
			})
	}

	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), SwapIoError> {
		self.request_tx.send(SwapRequest::Recover { buffer_id, source_path, base_text }).map_err(|err| {
			error!("enqueue_recover failed: swap request channel is disconnected: {}", err);
			SwapIoError::RequestChannelDisconnected { operation: "recover" }
		})
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), SwapIoError> {
		self.request_tx.send(SwapRequest::Close { buffer_id }).map_err(|err| {
			error!("enqueue_close failed: swap request channel is disconnected: {}", err);
			SwapIoError::RequestChannelDisconnected { operation: "close" }
		})
	}
}

impl SwapIoState {
	pub fn new(event_tx: flume::Sender<AppAction>) -> Self {
		let (request_tx, request_rx) = flume::unbounded();
		Self { request_tx, request_rx, event_tx, swap_dir: user_swap_dir(), worker_join: Mutex::new(None) }
	}

	pub fn start(&self) {
		let mut guard = self.worker_join.lock().expect("swap worker mutex poisoned");
		if guard.is_some() {
			return;
		}
		let request_rx = self.request_rx.clone();
		let event_tx = self.event_tx.clone();
		let swap_dir = self.swap_dir.clone();
		let join = thread::spawn(move || {
			if let Err(err) = Self::run(request_rx, event_tx, swap_dir) {
				error!("swap worker exited with error: {:#}", err);
			}
		});
		*guard = Some(join);
	}

	fn run(
		request_rx: flume::Receiver<SwapRequest>,
		event_tx: flume::Sender<AppAction>,
		swap_dir: PathBuf,
	) -> Result<()> {
		std::fs::create_dir_all(&swap_dir)
			.with_context(|| format!("create swap dir failed: {}", swap_dir.display()))?;

		let mut sessions: HashMap<BufferId, SwapSession> = HashMap::new();
		let pid = std::process::id();
		let username = current_username();

		loop {
			match request_rx.recv_timeout(REQUEST_POLL_INTERVAL) {
				Ok(request) => {
					let keep_running =
						Self::handle_request(request, &event_tx, &swap_dir, pid, username.as_str(), &mut sessions);
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

	fn handle_request(
		request: SwapRequest,
		event_tx: &flume::Sender<AppAction>,
		swap_dir: &Path,
		pid: u32,
		username: &str,
		sessions: &mut HashMap<BufferId, SwapSession>,
	) -> bool {
		match request {
			SwapRequest::Shutdown => return false,
			SwapRequest::Open { buffer_id, source_path } => {
				sessions
					.insert(buffer_id, SwapSession::new(buffer_id, source_path, swap_dir, pid, username.to_string()));
			}
			SwapRequest::DetectConflict { buffer_id, source_path } => {
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
			SwapRequest::Edit { buffer_id, source_path, op } => {
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
			SwapRequest::MarkClean { buffer_id, source_path } => {
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
			SwapRequest::InitializeBase { buffer_id, source_path, base_text, delete_existing } => {
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
			SwapRequest::Recover { buffer_id, source_path, base_text } => {
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
			SwapRequest::Close { buffer_id } => {
				sessions.remove(&buffer_id);
			}
		}

		true
	}
}

#[derive(Debug)]
enum SwapRequest {
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
	Close {
		buffer_id: BufferId,
	},
}

#[derive(Debug)]
struct SwapSession {
	buffer_id:       BufferId,
	source_path:     PathBuf,
	swap_path:       PathBuf,
	lease_path:      PathBuf,
	pid:             u32,
	username:        String,
	rope:            Rope,
	dirty:           bool,
	pending_ops:     Vec<SwapEditOp>,
	last_pending_at: Option<Instant>,
	last_insert_at:  Option<Instant>,
	snapshot_ready:  bool,
}

impl SwapSession {
	fn new(buffer_id: BufferId, source_path: PathBuf, swap_dir: &Path, pid: u32, username: String) -> Self {
		let swap_path = swap_path_for_source(swap_dir, source_path.as_path());
		let lease_path = swap_lease_path_for_source(swap_dir, source_path.as_path(), pid);
		touch_swap_lease_file(lease_path.as_path(), source_path.as_path());
		Self {
			buffer_id,
			source_path,
			swap_path,
			lease_path,
			pid,
			username,
			rope: Rope::new(),
			dirty: false,
			pending_ops: Vec::new(),
			last_pending_at: None,
			last_insert_at: None,
			snapshot_ready: false,
		}
	}

	fn rebind_if_needed(&mut self, source_path: PathBuf) -> Result<()> {
		if self.source_path == source_path {
			return Ok(());
		}

		let old_swap = self.swap_path.clone();
		self.source_path = source_path;
		self.swap_path = swap_path_for_source(
			old_swap.parent().ok_or_else(|| anyhow!("swap dir missing for rebind"))?,
			self.source_path.as_path(),
		);
		let old_lease = self.lease_path.clone();
		self.lease_path = swap_lease_path_for_source(
			old_swap.parent().ok_or_else(|| anyhow!("swap dir missing for rebind"))?,
			self.source_path.as_path(),
			self.pid,
		);
		remove_swap_lease_file(old_lease.as_path());
		touch_swap_lease_file(self.lease_path.as_path(), self.source_path.as_path());
		self.snapshot_ready = false;
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

		self.rope = Rope::from_str(base_text.as_str());
		self.pending_ops.clear();
		self.last_pending_at = None;
		self.last_insert_at = None;
		self.dirty = false;
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

		self.rope = Rope::from_str(recovered_text.as_str());
		self.pending_ops.clear();
		self.last_pending_at = None;
		self.last_insert_at = None;
		self.dirty = recovered_text != base_text;
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
				self.dirty = true;
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
				self.rope.remove(start..end);
				self.pending_ops.push(SwapEditOp::Delete { pos: start, len: end - start });
				self.dirty = true;
				self.last_pending_at = Some(now);
				self.last_insert_at = None;
			}
		}
		Ok(())
	}

	fn push_insert_with_merge(&mut self, pos: usize, text: String, now: Instant) {
		if let Some(SwapEditOp::Insert { pos: last_pos, text: last_text }) = self.pending_ops.last_mut()
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
		self.pending_ops.push(SwapEditOp::Insert { pos, text });
		self.last_insert_at = Some(now);
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
		append_swap_ops(self.swap_path.as_path(), &self.pending_ops)?;
		self.pending_ops.clear();
		self.last_pending_at = None;
		Ok(())
	}

	fn mark_clean(&mut self) -> Result<()> {
		self.ensure_snapshot_initialized()?;
		self.flush_pending()?;
		self.dirty = false;
		self.write_snapshot(self.rope.to_string().as_str(), false)
	}

	fn ensure_snapshot_initialized(&mut self) -> Result<()> {
		if self.snapshot_ready {
			return Ok(());
		}
		self.write_snapshot(self.rope.to_string().as_str(), self.dirty)
	}

	fn write_snapshot(&mut self, base_text: &str, dirty: bool) -> Result<()> {
		write_swap_snapshot(
			self.swap_path.as_path(),
			self.pid,
			self.username.as_str(),
			self.source_path.as_path(),
			dirty,
			base_text,
		)?;
		self.snapshot_ready = true;
		Ok(())
	}

	fn should_remove_swap_on_drop(&self) -> bool {
		if !self.swap_path.exists() {
			return false;
		}
		let Some(swap_dir) = self.swap_path.parent() else {
			return false;
		};
		!has_other_swap_leases(swap_dir, self.source_path.as_path(), self.pid)
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

impl Drop for SwapIoState {
	fn drop(&mut self) {
		let _ = self.request_tx.send(SwapRequest::Shutdown);
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

fn write_swap_snapshot(
	path: &Path,
	pid: u32,
	username: &str,
	source_path: &Path,
	dirty: bool,
	base_text: &str,
) -> Result<()> {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)
			.with_context(|| format!("create swap parent dir failed: {}", parent.display()))?;
	}
	let source_path_text = source_path.display().to_string();
	let content = format!(
		"{}\nMETA\tpid={}\tuser={}\tsource={}\tdirty={}\nBASE\t{}\n",
		SWAP_FILE_MAGIC,
		pid,
		encode_text_field(username),
		encode_text_field(source_path_text.as_str()),
		if dirty { "1" } else { "0" },
		encode_text_field(base_text),
	);
	std::fs::write(path, content).with_context(|| format!("write swap snapshot failed: {}", path.display()))?;
	Ok(())
}

fn append_swap_ops(path: &Path, ops: &[SwapEditOp]) -> Result<()> {
	if ops.is_empty() {
		return Ok(());
	}
	let file = OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.with_context(|| format!("open swap file for append failed: {}", path.display()))?;
	let mut writer = BufWriter::new(file);
	for op in ops {
		match op {
			SwapEditOp::Insert { pos, text } => {
				writeln!(writer, "I\t{}\t{}", pos, encode_text_field(text))
					.with_context(|| format!("append swap insert failed: {}", path.display()))?;
			}
			SwapEditOp::Delete { pos, len } => {
				writeln!(writer, "D\t{}\t{}", pos, len)
					.with_context(|| format!("append swap delete failed: {}", path.display()))?;
			}
		}
	}
	writer.flush().with_context(|| format!("flush swap append failed: {}", path.display()))?;
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

	Ok(ParsedSwapFile { pid, username, source_path: PathBuf::from(source_path_text), dirty, base_text, ops })
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

fn touch_swap_lease_file(lease_path: &Path, source_path: &Path) {
	if let Some(parent) = lease_path.parent()
		&& let Err(err) = std::fs::create_dir_all(parent)
	{
		error!("create lease dir failed: {} error={}", parent.display(), err);
		return;
	}
	let content = format!("source={}\npid={}\n", source_path.display(), std::process::id());
	if let Err(err) = std::fs::write(lease_path, content) {
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

fn has_other_swap_leases(swap_dir: &Path, source_path: &Path, self_pid: u32) -> bool {
	let basename = swap_file_basename_for_source(source_path);
	let self_lease_name = format!("{}.{}.lease", basename, self_pid);
	let prefix = format!("{}.", basename);

	let entries = match std::fs::read_dir(swap_dir) {
		Ok(entries) => entries,
		Err(err) => {
			error!("read lease dir failed: {} error={}", swap_dir.display(), err);
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
		if !file_name.starts_with(prefix.as_str()) || !file_name.ends_with(".lease") {
			continue;
		}
		if file_name == self_lease_name {
			continue;
		}
		let lease_path = entry.path();
		let Some(pid) = parse_pid_from_lease_name(file_name.as_ref(), prefix.as_str()) else {
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
	let basename = swap_file_basename_for_source(source_path);
	swap_dir.join(format!("{}.swp", basename))
}

fn swap_lease_path_for_source(swap_dir: &Path, source_path: &Path, pid: u32) -> PathBuf {
	let basename = swap_file_basename_for_source(source_path);
	swap_dir.join(format!("{}.{}.lease", basename, pid))
}

fn swap_file_basename_for_source(source_path: &Path) -> String {
	let source_text = source_path.display().to_string();
	let mut hasher = std::collections::hash_map::DefaultHasher::new();
	source_text.hash(&mut hasher);
	let hash = hasher.finish();
	let stem = source_path.file_name().and_then(|name| name.to_str()).unwrap_or("buffer");
	let sanitized_stem = sanitize_file_stem(stem);
	format!("{}-{:016x}", sanitized_stem, hash)
}

fn sanitize_file_stem(stem: &str) -> String {
	let mut normalized = stem
		.chars()
		.map(|ch| if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') { ch } else { '_' })
		.collect::<String>();
	if normalized.is_empty() {
		normalized = "buffer".to_string();
	}
	if normalized.len() > 32 {
		normalized.truncate(32);
	}
	normalized
}

fn parse_meta_line(meta_line: &str, path: &Path) -> Result<(u32, String, String, bool)> {
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
		return Ok((pid, username, source_path_text, dirty));
	}

	// Current readable format:
	// META pid=<n> user=<json_str> source=<json_str> dirty=<0|1>
	if meta_fields.len() != 5 {
		bail!("invalid swap meta field count in {}", path.display());
	}

	let pid_raw = meta_fields[1]
		.strip_prefix("pid=")
		.ok_or_else(|| anyhow!("invalid swap pid field in {}", path.display()))?;
	let user_raw = meta_fields[2]
		.strip_prefix("user=")
		.ok_or_else(|| anyhow!("invalid swap user field in {}", path.display()))?;
	let source_raw = meta_fields[3]
		.strip_prefix("source=")
		.ok_or_else(|| anyhow!("invalid swap source field in {}", path.display()))?;
	let dirty_raw = meta_fields[4]
		.strip_prefix("dirty=")
		.ok_or_else(|| anyhow!("invalid swap dirty field in {}", path.display()))?;

	let pid = pid_raw.parse::<u32>().with_context(|| format!("invalid swap pid in {}", path.display()))?;
	let username =
		decode_text_field(user_raw).with_context(|| format!("invalid swap username in {}", path.display()))?;
	let source_path_text = decode_text_field(source_raw)
		.with_context(|| format!("invalid swap source path in {}", path.display()))?;
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

fn user_state_root() -> PathBuf {
	#[cfg(target_os = "windows")]
	{
		return std::env::var_os("LOCALAPPDATA")
			.map(PathBuf::from)
			.unwrap_or_else(std::env::temp_dir)
			.join("rim");
	}

	#[cfg(target_os = "macos")]
	{
		return std::env::var_os("HOME")
			.map(PathBuf::from)
			.unwrap_or_else(std::env::temp_dir)
			.join("Library")
			.join("Logs")
			.join("rim");
	}

	#[cfg(all(unix, not(target_os = "macos")))]
	{
		if let Some(state_home) = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from) {
			return state_home.join("rim");
		}
		std::env::var_os("HOME")
			.map(PathBuf::from)
			.unwrap_or_else(std::env::temp_dir)
			.join(".local")
			.join("state")
			.join("rim")
	}
}

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
		assert_eq!(session.pending_ops[0], SwapEditOp::Insert { pos: 1, text: "xy".to_string() });
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

		write_swap_snapshot(session.swap_path.as_path(), 999, "old-user", source_path.as_path(), true, "abc")
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
		let mut state = SwapIoState::new(event_tx);
		state.swap_dir = swap_dir.clone();
		state.start();

		state
			.request_tx
			.send(SwapRequest::Open { buffer_id: BufferId::default(), source_path: source_path.clone() })
			.expect("send open failed");
		state
			.request_tx
			.send(SwapRequest::InitializeBase {
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
		let mut state = SwapIoState::new(event_tx);
		state.swap_dir = swap_dir;
		state.start();

		state
			.request_tx
			.send(SwapRequest::Recover {
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
		touch_swap_lease_file(peer_lease.as_path(), source_path.as_path());

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
		touch_swap_lease_file(stale_lease.as_path(), source_path.as_path());
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
		write_swap_snapshot(swap_path.as_path(), 77, "user-a", source_path.as_path(), false, "base")
			.expect("write snapshot failed");

		let parsed = parse_swap_file(swap_path.as_path()).expect("parse swap file failed");
		assert_eq!(parsed.pid, 77);
		assert_eq!(parsed.username, "user-a");
		assert_eq!(parsed.source_path, source_path);
		assert!(!parsed.dirty);
		assert_eq!(parsed.base_text, "base");
	}

	#[test]
	fn swap_file_should_use_readable_json_escaped_fields() {
		let swap_dir = make_tmp_dir("readable");
		let source_path = swap_dir.join("sample.txt");
		let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());

		write_swap_snapshot(swap_path.as_path(), 42, "tester", source_path.as_path(), true, "a\nb\t中")
			.expect("write snapshot failed");
		append_swap_ops(swap_path.as_path(), &[
			SwapEditOp::Insert { pos: 3, text: "xy\n\t".to_string() },
			SwapEditOp::Delete { pos: 1, len: 1 },
		])
		.expect("append ops failed");

		let raw = std::fs::read_to_string(&swap_path).expect("read raw swap failed");
		assert!(raw.contains("META\tpid=42\tuser=\"tester\"\tsource=\""));
		assert!(raw.lines().any(|line| line.starts_with("BASE\t\"")));
		assert!(raw.lines().any(|line| line.starts_with("I\t3\t\"")));
		assert!(raw.lines().any(|line| line == "D\t1\t1"));
	}

	#[test]
	fn parse_swap_file_should_support_legacy_base64_format() {
		let swap_dir = make_tmp_dir("legacy");
		let source_path = swap_dir.join("sample.txt");
		let swap_path = swap_path_for_source(swap_dir.as_path(), source_path.as_path());

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
}
