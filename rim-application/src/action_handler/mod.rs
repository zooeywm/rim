use std::ops::ControlFlow;

use rim_ports::{FilePicker, FileWatcher, PluginRuntime, StorageIo};

mod command_flow;
mod editor_flow;
mod errors;
mod file_flow;
mod mode_flow;
mod plugin_flow;
mod post_edit_flow;

use errors::ActionHandlerError;
use file_flow::{enqueue_history_save, enqueue_history_save_for_buffer, handle_file_action, handle_pending_swap_decision_key};
use mode_flow::SequenceMatch;
use plugin_flow::handle_plugin_runtime_action;

use crate::{action::{AppAction, BufferAction, EditorAction, KeyEvent, LayoutAction, SystemAction, TabAction, WindowAction}, ports::SwapEditOp, state::{BufferId, BufferSwitchDirection, FocusDirection, NormalSequenceKey, NotificationLevel, PersistedBufferHistory, RimState, SplitAxis, WorkspaceSessionSnapshot}};

#[doc(hidden)]
pub trait StoragePorts:
	StorageIo<
		BufferId = BufferId,
		PersistedBufferHistory = PersistedBufferHistory,
		WorkspaceSessionSnapshot = WorkspaceSessionSnapshot,
		EditOp = SwapEditOp,
	>
{
}

impl<T> StoragePorts for T where T: StorageIo<
			BufferId = BufferId,
			PersistedBufferHistory = PersistedBufferHistory,
			WorkspaceSessionSnapshot = WorkspaceSessionSnapshot,
			EditOp = SwapEditOp,
		>
{
}

#[doc(hidden)]
pub trait RuntimePorts: StoragePorts + FileWatcher<BufferId = BufferId> {}

impl<T> RuntimePorts for T where T: StoragePorts + FileWatcher<BufferId = BufferId> {}

#[doc(hidden)]
pub trait PluginPorts: PluginRuntime {}

impl<T> PluginPorts for T where T: PluginRuntime {}

#[doc(hidden)]
pub trait ActionPorts: RuntimePorts + FilePicker + PluginPorts {}

impl<T> ActionPorts for T where T: RuntimePorts + FilePicker + PluginPorts {}

impl RimState {
	pub fn apply_action<P>(&mut self, ports: &P, action: AppAction) -> ControlFlow<()>
	where P: ActionPorts {
		Self::dispatch_internal(ports, self, action)
	}
}

impl RimState {
	fn predicted_normal_mode_editor_action_for_key(state: &RimState, key: KeyEvent) -> Option<EditorAction> {
		let normal_key = Self::to_normal_key(state, key)?;
		let mut keys = state.workbench.normal_sequence.clone();
		keys.push(normal_key);
		match mode_flow::resolve_normal_sequence_with_registry(&state.workbench.command_registry, &keys) {
			SequenceMatch::Action(AppAction::Editor(editor_action)) => Some(editor_action),
			_ => None,
		}
	}

	fn dispatch_internal<P>(ports: &P, state: &mut RimState, action: AppAction) -> ControlFlow<()>
	where P: ActionPorts {
		let status_before = state.workbench.status_bar.message.clone();
		match action {
			AppAction::Editor(EditorAction::KeyPressed(key)) => {
				return Self::handle_key(ports, state, key);
			}
			AppAction::Editor(editor_action) => {
				editor_flow::apply_editor_action(ports, state, editor_action);
			}
			AppAction::Layout(LayoutAction::SplitHorizontal) => {
				state.split_active_window(SplitAxis::Horizontal);
			}
			AppAction::Layout(LayoutAction::SplitVertical) => {
				state.split_active_window(SplitAxis::Vertical);
			}
			AppAction::Layout(LayoutAction::ViewportResized { .. }) => {}
			AppAction::Window(WindowAction::FocusLeft) => state.focus_window(FocusDirection::Left),
			AppAction::Window(WindowAction::FocusDown) => state.focus_window(FocusDirection::Down),
			AppAction::Window(WindowAction::FocusUp) => state.focus_window(FocusDirection::Up),
			AppAction::Window(WindowAction::FocusRight) => state.focus_window(FocusDirection::Right),
			AppAction::Window(WindowAction::CloseActive) => state.close_active_window(),
			AppAction::Buffer(BufferAction::SwitchPrev) => {
				state.switch_active_window_buffer(BufferSwitchDirection::Prev);
			}
			AppAction::Buffer(BufferAction::SwitchNext) => {
				state.switch_active_window_buffer(BufferSwitchDirection::Next);
			}
			AppAction::Tab(TabAction::New) => {
				state.open_new_tab();
			}
			AppAction::Tab(TabAction::CloseCurrent) => {
				state.close_current_tab();
			}
			AppAction::Tab(TabAction::SwitchPrev) => {
				state.switch_to_prev_tab();
			}
			AppAction::Tab(TabAction::SwitchNext) => {
				state.switch_to_next_tab();
			}
			AppAction::File(file_action) => return handle_file_action(ports, state, file_action),
			AppAction::Plugin(plugin_action) => {
				return handle_plugin_runtime_action(ports, state, plugin_action);
			}
			AppAction::System(system_action) => match system_action {
				SystemAction::Quit => {
					for (buffer_id, path, history) in state.all_file_backed_persisted_history_snapshots() {
						enqueue_history_save(ports, buffer_id, path, history);
					}
					let snapshot = state.workspace_session_snapshot();
					if let Err(source) = ports.enqueue_save_workspace_session(snapshot) {
						let err = ActionHandlerError::SaveAll { source };
						tracing::error!("workspace session save enqueue failed: {}", err);
					}
					return ControlFlow::Break(());
				}
				SystemAction::ReloadConfig => {}
				SystemAction::Tick => {
					let _ = state.tick_notifications(std::time::Instant::now());
				}
			},
		}
		if state.workbench.status_bar.message != status_before
			&& is_error_status_message(state.workbench.status_bar.message.as_str())
		{
			let error_message = state.workbench.status_bar.message.clone();
			state.push_notification(NotificationLevel::Error, error_message);
			state.workbench.status_bar.message = status_before;
		}
		ControlFlow::Continue(())
	}

	fn handle_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
	where P: ActionPorts {
		mode_flow::handle_key(ports, state, key)
	}

	fn to_normal_key(state: &RimState, key: KeyEvent) -> Option<NormalSequenceKey> {
		mode_flow::to_normal_key(state, key)
	}
}

fn is_error_status_message(message: &str) -> bool {
	let lower = message.to_ascii_lowercase();
	lower.contains("failed")
		|| lower.contains("error")
		|| lower.contains("blocked")
		|| lower.contains("invalid")
		|| lower.contains("unknown")
		|| lower.contains("unavailable")
}

#[cfg(test)]
mod tests;
