use rim_ports::{FileWatcherError, StorageIoError};
use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum ActionHandlerError {
	#[error("enqueue watch for opened file failed")]
	OpenFileWatch {
		#[source]
		source: FileWatcherError,
	},
	#[error("enqueue initial file load failed")]
	OpenFileLoad {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue external reload failed")]
	ExternalReload {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue watch after save failed")]
	SaveWatch {
		#[source]
		source: FileWatcherError,
	},
	#[error("enqueue unwatch for closed buffer failed")]
	CloseBufferUnwatch {
		#[source]
		source: FileWatcherError,
	},
	#[error("enqueue file save failed")]
	Save {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue file reload failed")]
	Reload {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue file save for :wa failed")]
	SaveAll {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence open failed")]
	PersistenceOpen {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence swap edit failed")]
	PersistenceSwapEdit {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence swap mark clean failed")]
	PersistenceSwapMarkClean {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence swap recover failed")]
	PersistenceSwapRecover {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence swap conflict detect failed")]
	PersistenceSwapDetectConflict {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence swap base initialization failed")]
	PersistenceSwapInitializeBase {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence swap close failed")]
	PersistenceSwapClose {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence history load failed")]
	PersistenceHistoryLoad {
		#[source]
		source: StorageIoError,
	},
	#[error("enqueue persistence history save failed")]
	PersistenceHistorySave {
		#[source]
		source: StorageIoError,
	},
}
