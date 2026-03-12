use rim_kernel::action::{AppAction, FileAction};

use super::{StorageIoRequest, list_workspace_files, load_file, load_workspace_file_preview, save_file, send_file_action_async};

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
		StorageIoRequest::ListWorkspaceFiles { workspace_root } => {
			spawn_file_action(in_flight, event_tx, "WorkspaceFilesListed", async move {
				FileAction::WorkspaceFilesListed {
					workspace_root: workspace_root.clone(),
					result:         list_workspace_files(workspace_root).await,
				}
			});
		}
		StorageIoRequest::LoadWorkspaceFilePreview { path, max_bytes } => {
			spawn_file_action(in_flight, event_tx, "WorkspaceFilePreviewLoaded", async move {
				FileAction::WorkspaceFilePreviewLoaded {
					path:   path.clone(),
					result: load_workspace_file_preview(path, max_bytes).await,
				}
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
