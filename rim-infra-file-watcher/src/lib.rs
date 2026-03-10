use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}, thread};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher, event::EventKind};
use rim_kernel::{action::{AppAction, FileAction, SystemAction}, ports::{FileWatcher, FileWatcherError}, state::BufferId};
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
}

impl FileWatcherState {
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
		let mut watch_target_ref_counts: HashMap<PathBuf, usize> = HashMap::new();

		while let Ok(event) = worker_rx.recv() {
			match event {
				WatchWorkerEvent::Command(request) => {
					Self::apply_watch_request(
						request,
						&mut watcher,
						&mut file_to_buffer,
						&mut buffer_to_file,
						&mut config_paths,
						&mut watch_target_ref_counts,
					);
				}
				WatchWorkerEvent::Notify(result) => {
					if !Self::handle_notify_event(result, &file_to_buffer, &buffer_to_file, &config_paths, &event_tx) {
						return;
					}
				}
			}
		}
	}

	fn handle_notify_event(
		result: notify::Result<notify::Event>,
		file_to_buffer: &HashMap<PathBuf, BufferId>,
		buffer_to_file: &HashMap<BufferId, PathBuf>,
		config_paths: &HashSet<PathBuf>,
		event_tx: &flume::Sender<AppAction>,
	) -> bool {
		let event = match result {
			Ok(event) => event,
			Err(err) => {
				error!("file watcher event error: {}", err);
				return true;
			}
		};
		if matches!(event.kind, EventKind::Access(_)) {
			return true;
		}

		let mut affected_buffers = HashSet::new();
		let mut config_changed = false;
		for path in event.paths {
			let normalized_path = Self::normalize_watch_path(&path);
			if let Some(buffer_id) = file_to_buffer.get(&normalized_path) {
				affected_buffers.insert(*buffer_id);
			}
			if config_paths.contains(&normalized_path) {
				config_changed = true;
			}
		}

		for buffer_id in affected_buffers {
			let Some(path) = buffer_to_file.get(&buffer_id).cloned() else {
				continue;
			};
			if event_tx.send(AppAction::File(FileAction::ExternalChangeDetected { buffer_id, path })).is_err() {
				return false;
			}
		}
		if config_changed && event_tx.send(AppAction::System(SystemAction::ReloadCommandConfig)).is_err() {
			return false;
		}

		true
	}

	fn apply_watch_request(
		request: WatchRequest,
		watcher: &mut RecommendedWatcher,
		file_to_buffer: &mut HashMap<PathBuf, BufferId>,
		buffer_to_file: &mut HashMap<BufferId, PathBuf>,
		config_paths: &mut HashSet<PathBuf>,
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
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use super::FileWatcherState;

	#[test]
	fn watch_target_for_file_should_use_parent_directory() {
		let path = Path::new("/tmp/demo/sample.txt");
		let target = FileWatcherState::watch_target_for_file(path);
		assert_eq!(target, Path::new("/tmp/demo"));
	}
}
