use std::{collections::{HashMap, HashSet, hash_map::DefaultHasher}, fs, hash::{Hash, Hasher}, path::{Path, PathBuf}, thread, time::{Duration, Instant, SystemTime}};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher, event::{EventKind, ModifyKind}};
use rim_application::action::{AppAction, FileAction, SystemAction};
use rim_domain::model::BufferId;
use rim_ports::{FileWatcher, FileWatcherError};
use tracing::error;

#[derive(dep_inj::DepInj)]
#[target(FileWatcherImpl)]
pub struct FileWatcherState {
	worker_tx: flume::Sender<WatchWorkerEvent>,
	worker_rx: flume::Receiver<WatchWorkerEvent>,
	event_tx:  flume::Sender<AppAction>,
}

impl AsRef<FileWatcherState> for FileWatcherState {
	fn as_ref(&self) -> &FileWatcherState { self }
}

impl<Deps> FileWatcher for FileWatcherImpl<Deps>
where Deps: AsRef<FileWatcherState>
{
	type BufferId = BufferId;

	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherError> {
		self.worker_tx.send(WatchWorkerEvent::Command(WatchRequest::WatchBufferPath { buffer_id, path })).map_err(
			|err| {
				error!("enqueue_watch failed: watch request channel is disconnected: {}", err);
				FileWatcherError::RequestChannelDisconnected { operation: "watch" }
			},
		)
	}

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError> {
		self.worker_tx.send(WatchWorkerEvent::Command(WatchRequest::UnwatchBuffer { buffer_id })).map_err(|err| {
			error!("enqueue_unwatch failed: watch request channel is disconnected: {}", err);
			FileWatcherError::RequestChannelDisconnected { operation: "unwatch" }
		})
	}

	fn enqueue_watch_workspace_root(&self, path: PathBuf) -> Result<(), FileWatcherError> {
		self.worker_tx.send(WatchWorkerEvent::Command(WatchRequest::WatchWorkspaceRoot { path })).map_err(|err| {
			error!("enqueue_watch_workspace_root failed: watch request channel is disconnected: {}", err);
			FileWatcherError::RequestChannelDisconnected { operation: "watch_workspace_root" }
		})
	}
}

impl FileWatcherState {
	const EVENT_COALESCE_WINDOW: Duration = Duration::from_millis(10);

	pub fn new(event_tx: flume::Sender<AppAction>) -> Self {
		let (worker_tx, worker_rx) = flume::unbounded();
		Self { worker_tx, worker_rx, event_tx }
	}

	pub fn start(&self) {
		let worker_rx = self.worker_rx.clone();
		let worker_tx = self.worker_tx.clone();
		let event_tx = self.event_tx.clone();
		thread::spawn(move || Self::run(worker_rx, worker_tx, event_tx));
	}

	pub fn enqueue_watch_config(&self, path: PathBuf) -> Result<(), FileWatcherError> {
		self.worker_tx.send(WatchWorkerEvent::Command(WatchRequest::WatchConfigPath { path })).map_err(|err| {
			error!("enqueue_watch_config failed: watch request channel is disconnected: {}", err);
			FileWatcherError::RequestChannelDisconnected { operation: "watch_config" }
		})
	}

	pub fn enqueue_watch_workspace_root(&self, path: PathBuf) -> Result<(), FileWatcherError> {
		self.worker_tx.send(WatchWorkerEvent::Command(WatchRequest::WatchWorkspaceRoot { path })).map_err(|err| {
			error!("enqueue_watch_workspace_root failed: watch request channel is disconnected: {}", err);
			FileWatcherError::RequestChannelDisconnected { operation: "watch_workspace_root" }
		})
	}

	fn run(
		worker_rx: flume::Receiver<WatchWorkerEvent>,
		worker_tx: flume::Sender<WatchWorkerEvent>,
		event_tx: flume::Sender<AppAction>,
	) {
		let mut watcher = match RecommendedWatcher::new(
			move |result| {
				let _ = worker_tx.send(WatchWorkerEvent::Notify(result));
			},
			Config::default(),
		) {
			Ok(watcher) => watcher,
			Err(err) => {
				error!("file watcher init failed: {}", err);
				return;
			}
		};

		let mut file_to_buffer: HashMap<PathBuf, BufferId> = HashMap::new();
		let mut buffer_to_file: HashMap<BufferId, PathBuf> = HashMap::new();
		let mut config_paths: HashSet<PathBuf> = HashSet::new();
		let mut workspace_roots: HashSet<PathBuf> = HashSet::new();
		let mut watch_target_ref_counts: HashMap<PathBuf, usize> = HashMap::new();
		let mut semantic_tracker = SemanticTracker::default();
		let mut pending_changes = PendingChanges::default();

		loop {
			if pending_changes.is_due(Instant::now())
				&& !Self::flush_pending_changes(
					&mut pending_changes,
					&mut semantic_tracker,
					&file_to_buffer,
					&config_paths,
					&event_tx,
				) {
				return;
			}

			let event = match pending_changes.next_timeout(Instant::now()) {
				Some(timeout) => match worker_rx.recv_timeout(timeout) {
					Ok(event) => event,
					Err(flume::RecvTimeoutError::Timeout) => continue,
					Err(flume::RecvTimeoutError::Disconnected) => return,
				},
				None => match worker_rx.recv() {
					Ok(event) => event,
					Err(_) => return,
				},
			};

			match event {
				WatchWorkerEvent::Command(request) => {
					Self::apply_watch_request(
						request,
						&mut watcher,
						&mut file_to_buffer,
						&mut buffer_to_file,
						&mut config_paths,
						&mut workspace_roots,
						&mut watch_target_ref_counts,
					);
					semantic_tracker.sync_tracked_paths(file_to_buffer.keys(), config_paths.iter());
				}
				WatchWorkerEvent::Notify(result) => {
					let outcome = Self::handle_notify_event(result, &file_to_buffer, &config_paths, &workspace_roots);
					if !outcome.should_continue {
						return;
					}
					if outcome.has_changes() {
						pending_changes.merge(outcome, Instant::now(), Self::EVENT_COALESCE_WINDOW);
					}
				}
			}
		}
	}

	fn flush_pending_changes(
		pending_changes: &mut PendingChanges,
		semantic_tracker: &mut SemanticTracker,
		file_to_buffer: &HashMap<PathBuf, BufferId>,
		config_paths: &HashSet<PathBuf>,
		event_tx: &flume::Sender<AppAction>,
	) -> bool {
		let due = pending_changes.take_due();
		if due.is_empty() {
			return true;
		}

		let changed_files = semantic_tracker.collect_changed_files(&due.file_paths);
		let mut should_reload_config = false;
		for changed_path in changed_files {
			if config_paths.contains(&changed_path) {
				should_reload_config = true;
			}
			if let Some(buffer_id) = file_to_buffer.get(&changed_path).copied()
				&& event_tx
					.send(AppAction::File(FileAction::ExternalChangeDetected { buffer_id, path: changed_path.clone() }))
					.is_err()
			{
				return false;
			}
		}

		if should_reload_config && event_tx.send(AppAction::System(SystemAction::ReloadConfig)).is_err() {
			return false;
		}

		for workspace_root in due.workspace_roots {
			if event_tx.send(AppAction::File(FileAction::WorkspaceFilesChanged { workspace_root })).is_err() {
				return false;
			}
		}

		true
	}

	fn handle_notify_event(
		result: notify::Result<notify::Event>,
		file_to_buffer: &HashMap<PathBuf, BufferId>,
		config_paths: &HashSet<PathBuf>,
		workspace_roots: &HashSet<PathBuf>,
	) -> NotifyEventOutcome {
		let event = match result {
			Ok(event) => event,
			Err(err) => {
				error!("file watcher event error: {}", err);
				return NotifyEventOutcome::cont();
			}
		};
		if matches!(event.kind, EventKind::Access(_)) {
			return NotifyEventOutcome::cont();
		}

		let workspace_listing_changed = matches!(
			event.kind,
			EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
		);
		let directory_level_change = matches!(
			event.kind,
			EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
		);

		let mut dirty_files = HashSet::new();
		let mut dirty_workspace_roots = HashSet::new();
		let mut touched_parent_dirs = HashSet::new();
		for path in event.paths {
			let normalized_path = Self::normalize_event_path(&path);
			if file_to_buffer.contains_key(&normalized_path) || config_paths.contains(&normalized_path) {
				dirty_files.insert(normalized_path.clone());
			}
			if directory_level_change && let Some(parent) = normalized_path.parent() {
				touched_parent_dirs.insert(parent.to_path_buf());
			}
			if workspace_listing_changed {
				for workspace_root in workspace_roots {
					if normalized_path.starts_with(workspace_root) {
						dirty_workspace_roots.insert(workspace_root.clone());
					}
				}
			}
		}
		if directory_level_change {
			for parent in touched_parent_dirs {
				for tracked_path in file_to_buffer.keys() {
					if tracked_path.parent() == Some(parent.as_path()) {
						dirty_files.insert(tracked_path.clone());
					}
				}
				for tracked_path in config_paths {
					if tracked_path.parent() == Some(parent.as_path()) {
						dirty_files.insert(tracked_path.clone());
					}
				}
			}
		}

		NotifyEventOutcome::with_changes(dirty_files, dirty_workspace_roots)
	}

	fn apply_watch_request(
		request: WatchRequest,
		watcher: &mut RecommendedWatcher,
		file_to_buffer: &mut HashMap<PathBuf, BufferId>,
		buffer_to_file: &mut HashMap<BufferId, PathBuf>,
		config_paths: &mut HashSet<PathBuf>,
		workspace_roots: &mut HashSet<PathBuf>,
		watch_target_ref_counts: &mut HashMap<PathBuf, usize>,
	) {
		match request {
			WatchRequest::WatchBufferPath { buffer_id, path } => {
				let normalized_file = Self::normalize_watch_path(&path);
				let watch_target = Self::watch_target_for_file(&normalized_file);

				let old_file = buffer_to_file.insert(buffer_id, normalized_file.clone());
				let should_attach_watch_target = !matches!(old_file.as_ref(), Some(old) if old == &normalized_file);
				if let Some(old_file) = old_file
					&& old_file != normalized_file
				{
					Self::detach_buffer_from_file(
						watcher,
						file_to_buffer,
						watch_target_ref_counts,
						&old_file,
						buffer_id,
					);
				}

				if let Some(old_owner) = file_to_buffer.get(&normalized_file).copied()
					&& old_owner != buffer_id
				{
					buffer_to_file.remove(&old_owner);
					Self::detach_buffer_from_file(
						watcher,
						file_to_buffer,
						watch_target_ref_counts,
						&normalized_file,
						old_owner,
					);
				}
				file_to_buffer.insert(normalized_file, buffer_id);

				if should_attach_watch_target {
					let count = watch_target_ref_counts.entry(watch_target.clone()).or_insert(0);
					let is_first_for_target = *count == 0;
					*count = count.saturating_add(1);
					if is_first_for_target && let Err(err) = watcher.watch(&watch_target, RecursiveMode::NonRecursive) {
						error!("file watcher watch failed: path={} error={}", watch_target.display(), err);
					}
				}
			}
			WatchRequest::UnwatchBuffer { buffer_id } => {
				let Some(file_path) = buffer_to_file.remove(&buffer_id) else {
					return;
				};
				Self::detach_buffer_from_file(
					watcher,
					file_to_buffer,
					watch_target_ref_counts,
					&file_path,
					buffer_id,
				);
			}
			WatchRequest::WatchConfigPath { path } => {
				let normalized_file = Self::normalize_watch_path(&path);
				let watch_target = Self::watch_target_for_file(&normalized_file);
				if config_paths.insert(normalized_file) {
					let count = watch_target_ref_counts.entry(watch_target.clone()).or_insert(0);
					let is_first_for_target = *count == 0;
					*count = count.saturating_add(1);
					if is_first_for_target && let Err(err) = watcher.watch(&watch_target, RecursiveMode::NonRecursive) {
						error!("file watcher watch failed: path={} error={}", watch_target.display(), err);
					}
				}
			}
			WatchRequest::WatchWorkspaceRoot { path } => {
				let normalized_path = Self::normalize_watch_path(&path);
				if workspace_roots.insert(normalized_path.clone()) {
					let count = watch_target_ref_counts.entry(normalized_path.clone()).or_insert(0);
					let is_first_for_target = *count == 0;
					*count = count.saturating_add(1);
					if is_first_for_target && let Err(err) = watcher.watch(&normalized_path, RecursiveMode::Recursive) {
						error!("file watcher watch failed: path={} error={}", normalized_path.display(), err);
					}
				}
			}
		}
	}

	fn detach_buffer_from_file(
		watcher: &mut RecommendedWatcher,
		file_to_buffer: &mut HashMap<PathBuf, BufferId>,
		watch_target_ref_counts: &mut HashMap<PathBuf, usize>,
		file_path: &PathBuf,
		buffer_id: BufferId,
	) {
		let removed = if let Some(owner) = file_to_buffer.get(file_path).copied() {
			if owner == buffer_id {
				file_to_buffer.remove(file_path);
				true
			} else {
				false
			}
		} else {
			false
		};
		if !removed {
			return;
		}

		let watch_target = Self::watch_target_for_file(file_path);
		let Some(count) = watch_target_ref_counts.get_mut(&watch_target) else {
			return;
		};
		*count = count.saturating_sub(1);
		if *count > 0 {
			return;
		}

		watch_target_ref_counts.remove(&watch_target);
		if let Err(err) = watcher.unwatch(&watch_target) {
			error!("file watcher unwatch failed: path={} error={}", watch_target.display(), err);
		}
	}

	fn normalize_watch_path(path: &PathBuf) -> PathBuf {
		let absolute = if path.is_absolute() {
			path.clone()
		} else {
			std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.clone())
		};
		std::fs::canonicalize(&absolute).unwrap_or(absolute)
	}

	fn normalize_event_path(path: &Path) -> PathBuf {
		let absolute = if path.is_absolute() {
			path.to_path_buf()
		} else {
			std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
		};
		std::fs::canonicalize(&absolute).unwrap_or(absolute)
	}

	fn watch_target_for_file(path: &Path) -> PathBuf {
		path
			.parent()
			.filter(|parent| !parent.as_os_str().is_empty())
			.map(Path::to_path_buf)
			.unwrap_or_else(|| path.to_path_buf())
	}
}

#[derive(Debug)]
enum WatchWorkerEvent {
	Command(WatchRequest),
	Notify(notify::Result<notify::Event>),
}

#[derive(Debug)]
enum WatchRequest {
	WatchBufferPath { buffer_id: BufferId, path: PathBuf },
	UnwatchBuffer { buffer_id: BufferId },
	WatchConfigPath { path: PathBuf },
	WatchWorkspaceRoot { path: PathBuf },
}

#[derive(Debug, Default)]
struct PendingChanges {
	file_paths:      HashSet<PathBuf>,
	workspace_roots: HashSet<PathBuf>,
	deadline:        Option<Instant>,
}

impl PendingChanges {
	fn is_due(&self, now: Instant) -> bool { self.deadline.is_some_and(|deadline| now >= deadline) }

	fn next_timeout(&self, now: Instant) -> Option<Duration> {
		self.deadline.map(|deadline| deadline.saturating_duration_since(now))
	}

	fn merge(&mut self, outcome: NotifyEventOutcome, now: Instant, window: Duration) {
		self.file_paths.extend(outcome.dirty_files);
		self.workspace_roots.extend(outcome.dirty_workspace_roots);
		self.deadline = Some(now.checked_add(window).unwrap_or(now));
	}

	fn take_due(&mut self) -> DueChanges {
		self.deadline = None;
		DueChanges {
			file_paths:      std::mem::take(&mut self.file_paths),
			workspace_roots: std::mem::take(&mut self.workspace_roots),
		}
	}
}

#[derive(Debug, Default)]
struct DueChanges {
	file_paths:      HashSet<PathBuf>,
	workspace_roots: HashSet<PathBuf>,
}

impl DueChanges {
	fn is_empty(&self) -> bool { self.file_paths.is_empty() && self.workspace_roots.is_empty() }
}

#[derive(Debug, Clone)]
struct NotifyEventOutcome {
	should_continue:       bool,
	dirty_files:           HashSet<PathBuf>,
	dirty_workspace_roots: HashSet<PathBuf>,
}

impl NotifyEventOutcome {
	fn cont() -> Self {
		Self {
			should_continue:       true,
			dirty_files:           HashSet::new(),
			dirty_workspace_roots: HashSet::new(),
		}
	}

	fn with_changes(dirty_files: HashSet<PathBuf>, dirty_workspace_roots: HashSet<PathBuf>) -> Self {
		Self { should_continue: true, dirty_files, dirty_workspace_roots }
	}

	fn has_changes(&self) -> bool { !self.dirty_files.is_empty() || !self.dirty_workspace_roots.is_empty() }
}

#[derive(Debug, Default)]
struct SemanticTracker {
	snapshots: HashMap<PathBuf, FileSnapshot>,
}

impl SemanticTracker {
	fn sync_tracked_paths<'a, I1, I2>(&mut self, buffer_paths: I1, config_paths: I2)
	where
		I1: Iterator<Item = &'a PathBuf>,
		I2: Iterator<Item = &'a PathBuf>,
	{
		let mut tracked = HashSet::new();
		tracked.extend(buffer_paths.cloned());
		tracked.extend(config_paths.cloned());

		self.snapshots.retain(|path, _| tracked.contains(path));
		for path in tracked {
			self.snapshots.entry(path.clone()).or_insert_with(|| FileSnapshot::from_path(&path));
		}
	}

	fn collect_changed_files(&mut self, candidate_paths: &HashSet<PathBuf>) -> Vec<PathBuf> {
		let mut changed_paths = Vec::new();
		for path in candidate_paths {
			let next = FileSnapshot::from_path(path);
			let prev = self.snapshots.get(path).cloned().unwrap_or_else(|| FileSnapshot::from_path(path));
			if prev != next {
				changed_paths.push(path.clone());
			}
			self.snapshots.insert(path.clone(), next);
		}
		changed_paths
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FileSnapshot {
	Missing,
	File { content_hash: u64 },
	Other { len: u64, modified: Option<SystemTime> },
}

impl FileSnapshot {
	fn from_path(path: &Path) -> Self {
		let Ok(metadata) = fs::metadata(path) else {
			return Self::Missing;
		};
		if metadata.is_file() {
			let Ok(bytes) = fs::read(path) else {
				return Self::Other { len: metadata.len(), modified: metadata.modified().ok() };
			};
			return Self::File { content_hash: hash_bytes(&bytes) };
		}
		Self::Other { len: metadata.len(), modified: metadata.modified().ok() }
	}
}

fn hash_bytes(bytes: &[u8]) -> u64 {
	let mut hasher = DefaultHasher::new();
	bytes.hash(&mut hasher);
	hasher.finish()
}

#[cfg(test)]
mod tests {
	use std::{collections::{HashMap, HashSet}, fs, path::{Path, PathBuf}, time::Instant};

	use rim_application::action::{AppAction, SystemAction};

	use super::{FileWatcherState, PendingChanges, SemanticTracker};

	#[test]
	fn watch_target_for_file_should_use_parent_directory() {
		let path = Path::new("/tmp/demo/sample.txt");
		let target = FileWatcherState::watch_target_for_file(path);
		assert_eq!(target, Path::new("/tmp/demo"));
	}

	#[test]
	fn pending_changes_should_extend_deadline_on_burst_events() {
		let mut pending = PendingChanges::default();
		let start = Instant::now();
		let first_deadline = start.checked_add(FileWatcherState::EVENT_COALESCE_WINDOW).unwrap_or(start);
		pending.deadline = Some(first_deadline);

		pending.merge(super::NotifyEventOutcome::cont(), start, FileWatcherState::EVENT_COALESCE_WINDOW);
		assert!(pending.deadline.is_some());
	}

	#[test]
	fn semantic_tracker_should_only_report_real_file_content_changes() {
		let file_path = temp_test_file("watcher-semantic-tracker", "alpha");
		let mut tracker = SemanticTracker::default();
		tracker.sync_tracked_paths([&file_path].into_iter(), std::iter::empty());

		let mut candidates = std::collections::HashSet::new();
		candidates.insert(file_path.clone());

		assert!(tracker.collect_changed_files(&candidates).is_empty());

		fs::write(&file_path, "alpha").expect("rewrite same content");
		assert!(tracker.collect_changed_files(&candidates).is_empty());

		fs::write(&file_path, "beta").expect("write changed content");
		assert_eq!(tracker.collect_changed_files(&candidates), vec![file_path.clone()]);

		let _ = fs::remove_file(file_path);
	}

	#[test]
	fn flush_pending_changes_should_emit_single_reload_for_burst_events() {
		let config_path = temp_test_file("watcher-config-burst", "v1");
		let mut semantic_tracker = SemanticTracker::default();
		semantic_tracker.sync_tracked_paths(std::iter::empty(), [&config_path].into_iter());

		fs::write(&config_path, "v2").expect("update config content");

		let mut pending_changes = PendingChanges::default();
		let burst = super::NotifyEventOutcome::with_changes(HashSet::from([config_path.clone()]), HashSet::new());
		let now = Instant::now();
		pending_changes.merge(burst.clone(), now, FileWatcherState::EVENT_COALESCE_WINDOW);
		pending_changes.merge(
			burst,
			now.checked_add(FileWatcherState::EVENT_COALESCE_WINDOW).unwrap_or(now),
			FileWatcherState::EVENT_COALESCE_WINDOW,
		);

		let file_to_buffer = HashMap::new();
		let config_paths = HashSet::from([config_path.clone()]);
		let (tx, rx) = flume::unbounded();

		assert!(FileWatcherState::flush_pending_changes(
			&mut pending_changes,
			&mut semantic_tracker,
			&file_to_buffer,
			&config_paths,
			&tx
		));

		let reload_count =
			rx.try_iter().filter(|action| matches!(action, AppAction::System(SystemAction::ReloadConfig))).count();
		assert_eq!(reload_count, 1);

		let _ = fs::remove_file(config_path);
	}

	fn temp_test_file(prefix: &str, content: &str) -> PathBuf {
		let nanos = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|duration| duration.as_nanos())
			.unwrap_or(0);
		let unique = format!("{}-{}-{}", prefix, std::process::id(), nanos);
		let file_path = std::env::temp_dir().join(unique);
		fs::write(&file_path, content).expect("create temp test file");
		file_path
	}
}
