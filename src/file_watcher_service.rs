use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}, thread, time::Duration};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher, event::EventKind};
use thiserror::Error;
use tracing::error;

use crate::{action::{AppAction, FileAction}, state::BufferId};

#[derive(dep_inj::DepInj)]
#[target(FileWatcherImpl)]
pub struct FileWatcherState {
	watch_tx: flume::Sender<WatchRequest>,
	watch_rx: flume::Receiver<WatchRequest>,
	event_tx: flume::Sender<AppAction>,
}

#[derive(Debug, Error)]
pub enum FileWatcherServiceError {
	#[error("watch request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

pub trait FileWatcher {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherServiceError>;
	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherServiceError>;
}

impl<Deps> FileWatcher for FileWatcherImpl<Deps>
where Deps: AsRef<FileWatcherState>
{
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherServiceError> {
		self.watch_tx.send(WatchRequest::WatchBufferPath { buffer_id, path }).map_err(|err| {
			error!("enqueue_watch failed: watch request channel is disconnected: {}", err);
			FileWatcherServiceError::RequestChannelDisconnected { operation: "watch" }
		})
	}

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherServiceError> {
		self.watch_tx.send(WatchRequest::UnwatchBuffer { buffer_id }).map_err(|err| {
			error!("enqueue_unwatch failed: watch request channel is disconnected: {}", err);
			FileWatcherServiceError::RequestChannelDisconnected { operation: "unwatch" }
		})
	}
}

impl FileWatcherState {
	pub fn new(event_tx: flume::Sender<AppAction>) -> Self {
		let (watch_tx, watch_rx) = flume::unbounded();
		Self { watch_tx, watch_rx, event_tx }
	}

	pub fn start(&self) {
		let watch_rx = self.watch_rx.clone();
		let event_tx = self.event_tx.clone();
		thread::spawn(move || Self::run(watch_rx, event_tx));
	}

	fn run(watch_rx: flume::Receiver<WatchRequest>, event_tx: flume::Sender<AppAction>) {
		let (notify_tx, notify_rx) = std::sync::mpsc::channel();
		let mut watcher = match RecommendedWatcher::new(
			move |res| {
				let _ = notify_tx.send(res);
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
		let mut watch_target_ref_counts: HashMap<PathBuf, usize> = HashMap::new();

		loop {
			while let Ok(request) = watch_rx.try_recv() {
				Self::apply_watch_request(
					request,
					&mut watcher,
					&mut file_to_buffer,
					&mut buffer_to_file,
					&mut watch_target_ref_counts,
				);
			}

			match watch_rx.recv_timeout(Duration::from_millis(50)) {
				Ok(request) => Self::apply_watch_request(
					request,
					&mut watcher,
					&mut file_to_buffer,
					&mut buffer_to_file,
					&mut watch_target_ref_counts,
				),
				Err(flume::RecvTimeoutError::Timeout) => {}
				Err(flume::RecvTimeoutError::Disconnected) => break,
			}

			for result in notify_rx.try_iter() {
				let event = match result {
					Ok(event) => event,
					Err(err) => {
						error!("file watcher event error: {}", err);
						continue;
					}
				};
				if matches!(event.kind, EventKind::Access(_)) {
					continue;
				}

				let mut affected_buffers = HashSet::new();
				for path in event.paths {
					let normalized_path = Self::normalize_watch_path(&path);
					if let Some(buffer_id) = file_to_buffer.get(&normalized_path) {
						affected_buffers.insert(*buffer_id);
					}
				}

				for buffer_id in affected_buffers {
					let Some(path) = buffer_to_file.get(&buffer_id).cloned() else {
						continue;
					};
					if event_tx.send(AppAction::File(FileAction::ExternalChangeDetected { buffer_id, path })).is_err() {
						return;
					}
				}
			}
		}
	}

	fn apply_watch_request(
		request: WatchRequest,
		watcher: &mut RecommendedWatcher,
		file_to_buffer: &mut HashMap<PathBuf, BufferId>,
		buffer_to_file: &mut HashMap<BufferId, PathBuf>,
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

enum WatchRequest {
	WatchBufferPath { buffer_id: BufferId, path: PathBuf },
	UnwatchBuffer { buffer_id: BufferId },
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
