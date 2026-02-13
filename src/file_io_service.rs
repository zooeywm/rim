use std::{path::PathBuf, thread};

use anyhow::Context;
use thiserror::Error;
use tracing::error;

use crate::{action::{AppAction, FileAction, FileLoadSource}, state::BufferId};

#[derive(Debug, Error)]
pub enum FileIoServiceError {
	#[error("io request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

pub trait FileIo {
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError>;
	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), FileIoServiceError>;
	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError>;
}

#[derive(dep_inj::DepInj)]
#[target(FileIoImpl)]
pub struct FileIoState {
	request_tx: flume::Sender<FileIoRequest>,
	request_rx: flume::Receiver<FileIoRequest>,
	event_tx:   flume::Sender<AppAction>,
}

impl<Deps> FileIo for FileIoImpl<Deps>
where Deps: AsRef<FileIoState>
{
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoServiceError> {
		self
			.request_tx
			.send(FileIoRequest::LoadFile { buffer_id, path, source: FileLoadSource::Open })
			.map_err(|err| {
				error!("enqueue_load failed: io request channel is disconnected: {}", err);
				FileIoServiceError::RequestChannelDisconnected { operation: "load" }
			})
	}

	fn enqueue_save(
		&self,
		buffer_id: BufferId,
		path: PathBuf,
		text: String,
	) -> Result<(), FileIoServiceError> {
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
	pub fn new(event_tx: flume::Sender<AppAction>) -> Self {
		let (request_tx, request_rx) = flume::unbounded();
		Self { request_tx, request_rx, event_tx }
	}

	pub fn start(&self) {
		let request_rx = self.request_rx.clone();
		let event_tx = self.event_tx.clone();
		thread::spawn(move || {
			if let Err(err) = Self::run(request_rx, event_tx) {
				error!("io worker exited with error: {:#}", err);
			}
		});
	}

	fn run(
		request_rx: flume::Receiver<FileIoRequest>,
		event_tx: flume::Sender<AppAction>,
	) -> anyhow::Result<()> {
		let runtime = compio::runtime::Runtime::new().context("io worker runtime init failed")?;
		const MAX_IN_FLIGHT: usize = 64;

		runtime.block_on(async move {
			let mut in_flight = Vec::new();
			while let Ok(request) = request_rx.recv_async().await {
                let event_tx = event_tx.clone();
				let task = compio::runtime::spawn(async move {
					match request {
						FileIoRequest::LoadFile { buffer_id, path, source } => {
							let result = async {
								let display_path = path.display().to_string();
								let file_bytes = compio::fs::read(&path)
									.await
									.with_context(|| format!("read file failed: {}", display_path))?;
								String::from_utf8(file_bytes)
									.with_context(|| format!("decode utf-8 failed: {}", display_path))
							}
							.await;
							if let Err(err) =
								event_tx.send(AppAction::File(FileAction::LoadCompleted { buffer_id, source, result }))
							{
								error!("failed to send LoadCompleted from io worker: {}", err);
							}
						}
						FileIoRequest::Save { buffer_id, path, text } => {
							let result = async {
								let display_path = path.display().to_string();
								let write_result = compio::fs::write(&path, text.into_bytes()).await.0;
								write_result.with_context(|| format!("write file failed: {}", display_path)).map(|_| ())
							}
							.await;
							if let Err(err) =
								event_tx.send(AppAction::File(FileAction::SaveCompleted { buffer_id, result }))
							{
								error!("failed to send SaveCompleted from io worker: {}", err);
							}
						}
					}
				});
				in_flight.push(task);

				if in_flight.len() >= MAX_IN_FLIGHT {
					let oldest = in_flight.remove(0);
					let _ = oldest.await;
				}
			}

			for task in in_flight {
				let _ = task.await;
			}
		});

		Ok(())
	}
}

enum FileIoRequest {
	LoadFile { buffer_id: BufferId, path: PathBuf, source: FileLoadSource },
	Save { buffer_id: BufferId, path: PathBuf, text: String },
}
