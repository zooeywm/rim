use rim_kernel::action::{AppAction, FileAction};

use super::{StorageIoRequest, load_file, save_file, send_file_action_async};

pub(super) fn handle_file_transfer_request(
	request: StorageIoRequest,
	event_tx: &flume::Sender<AppAction>,
	in_flight: &mut Vec<compio::runtime::JoinHandle<()>>,
) {
	match request {
		StorageIoRequest::LoadFile { buffer_id, path, source } => {
			spawn_file_action(in_flight, event_tx, "LoadCompleted", async move {
				FileAction::LoadCompleted { buffer_id, source, result: load_file(path).await }
			});
		}
		StorageIoRequest::SaveFile { buffer_id, path, text } => {
			spawn_file_action(in_flight, event_tx, "SaveCompleted", async move {
				FileAction::SaveCompleted { buffer_id, result: save_file(path, text).await }
			});
		}
		_ => unreachable!("non file transfer request routed to handle_file_transfer_request"),
	}
}

fn spawn_file_action<F>(
	in_flight: &mut Vec<compio::runtime::JoinHandle<()>>,
	event_tx: &flume::Sender<AppAction>,
	action_name: &'static str,
	future: F,
) where
	F: std::future::Future<Output = FileAction> + 'static,
{
	let event_tx = event_tx.clone();
	in_flight.push(compio::runtime::spawn(async move {
		let action = future.await;
		let _ = send_file_action_async(event_tx, action, action_name).await;
	}));
}
