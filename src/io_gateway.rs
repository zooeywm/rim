use std::io;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::thread;

use tracing::error;

use crate::action::{AppAction, FileAction};
use crate::state::BufferId;

pub(crate) struct IoGateway {
    request_tx: flume::Sender<IoRequest>,
}

impl IoGateway {
    pub(crate) fn start(event_tx: flume::Sender<AppAction>) -> Self {
        let (request_tx, request_rx) = flume::unbounded();

        thread::spawn(move || IoWorker::run(request_rx, event_tx));

        Self { request_tx }
    }

    pub(crate) fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> io::Result<()> {
        self.request_tx
            .send(IoRequest::LoadFile { buffer_id, path })
            .map_err(|err| {
                error!(
                    "enqueue_load failed: io request channel is disconnected: {}",
                    err
                );
                io::Error::from(io::ErrorKind::BrokenPipe)
            })
    }
}

struct IoWorker;

impl IoWorker {
    fn run(request_rx: flume::Receiver<IoRequest>, event_tx: flume::Sender<AppAction>) {
        let runtime = match compio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(err) => {
                error!("io worker runtime init failed: {}", err);
                while let Ok(request) = request_rx.recv() {
                    match request {
                        IoRequest::LoadFile { buffer_id, .. } => {
                            if let Err(send_err) =
                                event_tx.send(AppAction::File(FileAction::LoadCompleted {
                                    buffer_id,
                                    result: Err(io::Error::new(err.kind(), err.to_string())),
                                }))
                            {
                                error!(
                                    "io worker failed to report runtime init failure to app: {}",
                                    send_err
                                );
                                return;
                            }
                        }
                    }
                }
                return;
            }
        };

        while let Ok(request) = request_rx.recv() {
            match request {
                IoRequest::LoadFile { buffer_id, path } => {
                    let result = runtime.block_on(Self::read_file_text(path));
                    if let Err(err) = event_tx.send(AppAction::File(FileAction::LoadCompleted {
                        buffer_id,
                        result,
                    })) {
                        error!("io worker failed to send load completion event: {}", err);
                        return;
                    }
                }
            }
        }
    }

    async fn read_file_text(path: PathBuf) -> io::Result<String> {
        let file_bytes = compio::fs::read(path).await?;
        String::from_utf8(file_bytes).map_err(|err| io::Error::new(ErrorKind::InvalidData, err))
    }
}

enum IoRequest {
    LoadFile { buffer_id: BufferId, path: PathBuf },
}
