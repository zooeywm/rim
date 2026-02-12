use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use notify::event::EventKind;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;
use tracing::error;

use crate::action::{AppAction, FileAction};
use crate::state::BufferId;

#[derive(Clone, dep_inj::DepInj)]
#[target(FileWatcherImpl)]
pub struct FileWatcherState {
    watch_tx: flume::Sender<WatchRequest>,
}

#[derive(Debug, Error)]
pub enum FileWatcherServiceError {
    #[error("watch request channel disconnected while enqueueing {operation}")]
    RequestChannelDisconnected { operation: &'static str },
}

pub trait FileWatcher {
    fn enqueue_watch(
        &self,
        buffer_id: BufferId,
        path: PathBuf,
    ) -> Result<(), FileWatcherServiceError>;
    fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherServiceError>;
}

impl<Deps> FileWatcher for FileWatcherImpl<Deps>
where
    Deps: AsRef<FileWatcherState>,
{
    fn enqueue_watch(
        &self,
        buffer_id: BufferId,
        path: PathBuf,
    ) -> Result<(), FileWatcherServiceError> {
        self.watch_tx
            .send(WatchRequest::WatchBufferPath { buffer_id, path })
            .map_err(|err| {
                error!(
                    "enqueue_watch failed: watch request channel is disconnected: {}",
                    err
                );
                FileWatcherServiceError::RequestChannelDisconnected { operation: "watch" }
            })
    }

    fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherServiceError> {
        self.watch_tx
            .send(WatchRequest::UnwatchBuffer { buffer_id })
            .map_err(|err| {
                error!(
                    "enqueue_unwatch failed: watch request channel is disconnected: {}",
                    err
                );
                FileWatcherServiceError::RequestChannelDisconnected {
                    operation: "unwatch",
                }
            })
    }
}

impl FileWatcherState {
    pub(crate) fn start(event_tx: flume::Sender<AppAction>) -> Self {
        let (watch_tx, watch_rx) = flume::unbounded();
        thread::spawn(move || Self::run_watch_worker(watch_rx, event_tx));
        Self { watch_tx }
    }

    fn run_watch_worker(
        watch_rx: flume::Receiver<WatchRequest>,
        event_tx: flume::Sender<AppAction>,
    ) {
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

        let mut file_to_buffers: HashMap<PathBuf, HashSet<BufferId>> = HashMap::new();
        let mut buffer_to_file: HashMap<BufferId, PathBuf> = HashMap::new();
        let mut watch_target_to_buffers: HashMap<PathBuf, HashSet<BufferId>> = HashMap::new();

        loop {
            while let Ok(request) = watch_rx.try_recv() {
                Self::apply_watch_request(
                    request,
                    &mut watcher,
                    &mut file_to_buffers,
                    &mut buffer_to_file,
                    &mut watch_target_to_buffers,
                );
            }

            match watch_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(request) => Self::apply_watch_request(
                    request,
                    &mut watcher,
                    &mut file_to_buffers,
                    &mut buffer_to_file,
                    &mut watch_target_to_buffers,
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
                    if let Some(buffer_ids) = file_to_buffers.get(&normalized_path) {
                        affected_buffers.extend(buffer_ids.iter().copied());
                    }
                }

                for buffer_id in affected_buffers {
                    let Some(path) = buffer_to_file.get(&buffer_id).cloned() else {
                        continue;
                    };
                    if event_tx
                        .send(AppAction::File(FileAction::ExternalChangeDetected {
                            buffer_id,
                            path,
                        }))
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
    }

    fn apply_watch_request(
        request: WatchRequest,
        watcher: &mut RecommendedWatcher,
        file_to_buffers: &mut HashMap<PathBuf, HashSet<BufferId>>,
        buffer_to_file: &mut HashMap<BufferId, PathBuf>,
        watch_target_to_buffers: &mut HashMap<PathBuf, HashSet<BufferId>>,
    ) {
        match request {
            WatchRequest::WatchBufferPath { buffer_id, path } => {
                let normalized_file = Self::normalize_watch_path(&path);
                let watch_target = Self::watch_target_for_file(&normalized_file);

                let old_file = buffer_to_file.insert(buffer_id, normalized_file.clone());
                if let Some(old_file) = old_file
                    && old_file != normalized_file
                {
                    Self::detach_buffer_from_file(
                        watcher,
                        file_to_buffers,
                        watch_target_to_buffers,
                        &old_file,
                        buffer_id,
                    );
                }

                file_to_buffers
                    .entry(normalized_file)
                    .or_default()
                    .insert(buffer_id);

                let is_first_for_target = !watch_target_to_buffers.contains_key(&watch_target);
                watch_target_to_buffers
                    .entry(watch_target.clone())
                    .or_default()
                    .insert(buffer_id);
                if is_first_for_target
                    && let Err(err) = watcher.watch(&watch_target, RecursiveMode::NonRecursive)
                {
                    error!(
                        "file watcher watch failed: path={} error={}",
                        watch_target.display(),
                        err
                    );
                }
            }
            WatchRequest::UnwatchBuffer { buffer_id } => {
                let Some(file_path) = buffer_to_file.remove(&buffer_id) else {
                    return;
                };
                Self::detach_buffer_from_file(
                    watcher,
                    file_to_buffers,
                    watch_target_to_buffers,
                    &file_path,
                    buffer_id,
                );
            }
        }
    }

    fn detach_buffer_from_file(
        watcher: &mut RecommendedWatcher,
        file_to_buffers: &mut HashMap<PathBuf, HashSet<BufferId>>,
        watch_target_to_buffers: &mut HashMap<PathBuf, HashSet<BufferId>>,
        file_path: &PathBuf,
        buffer_id: BufferId,
    ) {
        if let Some(buffer_ids) = file_to_buffers.get_mut(file_path) {
            buffer_ids.remove(&buffer_id);
            if buffer_ids.is_empty() {
                file_to_buffers.remove(file_path);
            }
        }

        let watch_target = Self::watch_target_for_file(file_path);
        let Some(buffer_ids) = watch_target_to_buffers.get_mut(&watch_target) else {
            return;
        };
        buffer_ids.remove(&buffer_id);
        if !buffer_ids.is_empty() {
            return;
        }

        watch_target_to_buffers.remove(&watch_target);
        if let Err(err) = watcher.unwatch(&watch_target) {
            error!(
                "file watcher unwatch failed: path={} error={}",
                watch_target.display(),
                err
            );
        }
    }

    fn normalize_watch_path(path: &PathBuf) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.clone())
    }

    fn watch_target_for_file(path: &Path) -> PathBuf {
        path.parent()
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
    use super::FileWatcherState;
    use std::path::Path;

    #[test]
    fn watch_target_for_file_should_use_parent_directory() {
        let path = Path::new("/tmp/demo/sample.txt");
        let target = FileWatcherState::watch_target_for_file(path);
        assert_eq!(target, Path::new("/tmp/demo"));
    }
}
