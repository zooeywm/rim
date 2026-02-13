use std::{path::PathBuf, thread};

use anyhow::{Context, Result as AnyhowResult, anyhow};
use thiserror::Error;
use tracing::error;

use crate::{action::{AppAction, FileAction, FileLoadSource}, state::BufferId};

#[derive(Debug, Error)]
pub enum FileIoServiceError {
	#[error("io request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

#[derive(dep_inj::DepInj)]
#[target(FileIoImpl)]
pub struct FileIoState {
	request_tx: flume::Sender<FileIoRequest>,
}

pub trait FileIo {
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError>;
	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), FileIoServiceError>;
	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError>;
}

impl<Deps> FileIo for FileIoImpl<Deps>
where Deps: AsRef<FileIoState>
{
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError> {
		self.request_tx.send(FileIoRequest::LoadFile { buffer_id, path, source: FileLoadSource::Open }).map_err(
			|err| {
				error!("enqueue_load failed: io request channel is disconnected: {}", err);
				FileIoServiceError::RequestChannelDisconnected { operation: "load" }
			},
		)
	}

	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), FileIoServiceError> {
		self.request_tx.send(FileIoRequest::Save { buffer_id, path, text }).map_err(|err| {
			error!("enqueue_save failed: io request channel is disconnected: {}", err);
			FileIoServiceError::RequestChannelDisconnected { operation: "save" }
		})
	}

	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError> {
		self
			.request_tx
			.send(FileIoRequest::LoadFile { buffer_id, path, source: FileLoadSource::External })
			.map_err(|err| {
				error!("enqueue_external_load failed: io request channel is disconnected: {}", err);
				FileIoServiceError::RequestChannelDisconnected { operation: "reload" }
			})
	}
}

impl FileIoState {
	pub(crate) fn start(event_tx: flume::Sender<AppAction>) -> Self {
		let (request_tx, request_rx) = flume::unbounded();
		thread::spawn(move || {
			let worker = || -> AnyhowResult<()> {
				let runtime = match compio::runtime::Runtime::new() {
					Ok(runtime) => runtime,
					Err(err) => {
						let runtime_error = anyhow!("io worker runtime init failed: {}", err);
						while let Ok(request) = request_rx.recv() {
							match request {
								FileIoRequest::LoadFile { buffer_id, source, .. } => {
									event_tx
										.send(AppAction::File(FileAction::LoadCompleted {
											buffer_id,
											source,
											result: Err(anyhow!("{}", runtime_error)),
										}))
										.context("failed to send LoadCompleted after runtime init failure")?;
								}
								FileIoRequest::Save { buffer_id, .. } => {
									event_tx
										.send(AppAction::File(FileAction::SaveCompleted {
											buffer_id,
											result: Err(anyhow!("{}", runtime_error)),
										}))
										.context("failed to send SaveCompleted after runtime init failure")?;
								}
							}
						}
						return Err(runtime_error);
					}
				};

				while let Ok(request) = request_rx.recv() {
					match request {
						FileIoRequest::LoadFile { buffer_id, path, source } => {
							let result = runtime.block_on(async {
								let display_path = path.display().to_string();
								let file_bytes = compio::fs::read(&path)
									.await
									.with_context(|| format!("read file failed: {}", display_path))?;
								String::from_utf8(file_bytes)
									.with_context(|| format!("decode utf-8 failed: {}", display_path))
							});
							event_tx
								.send(AppAction::File(FileAction::LoadCompleted { buffer_id, source, result }))
								.context("failed to send LoadCompleted from io worker")?;
						}
						FileIoRequest::Save { buffer_id, path, text } => {
							let result = runtime.block_on(async {
								let display_path = path.display().to_string();
								let write_result = compio::fs::write(&path, text.into_bytes()).await.0;
								write_result.with_context(|| format!("write file failed: {}", display_path)).map(|_| ())
							});
							event_tx
								.send(AppAction::File(FileAction::SaveCompleted { buffer_id, result }))
								.context("failed to send SaveCompleted from io worker")?;
						}
					}
				}

				Ok(())
			};

			if let Err(err) = worker() {
				error!("io worker exited with error: {:#}", err);
			}
		});
		Self { request_tx }
	}
}

enum FileIoRequest {
	LoadFile { buffer_id: BufferId, path: PathBuf, source: FileLoadSource },
	Save { buffer_id: BufferId, path: PathBuf, text: String },
}
