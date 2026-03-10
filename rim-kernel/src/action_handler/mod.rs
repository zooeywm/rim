use std::ops::ControlFlow;

mod command_flow;
mod editor_flow;
mod errors;
mod file_flow;
mod mode_flow;
mod post_edit_flow;

use errors::ActionHandlerError;
use file_flow::{enqueue_history_save, enqueue_history_save_for_buffer, handle_file_action, handle_pending_swap_decision_key};
use mode_flow::SequenceMatch;

use crate::{action::{AppAction, BufferAction, EditorAction, KeyEvent, LayoutAction, SystemAction, TabAction, WindowAction}, ports::{FilePicker, FileWatcher, StorageIo}, state::{BufferSwitchDirection, FocusDirection, NormalSequenceKey, RimState, SplitAxis}};

impl RimState {
	pub fn apply_action<P>(&mut self, ports: &P, action: AppAction) -> ControlFlow<()>
	where P: StorageIo + FileWatcher + FilePicker {
		Self::dispatch_internal(ports, self, action)
	}
}

impl RimState {
	fn predicted_normal_mode_editor_action_for_key(state: &RimState, key: KeyEvent) -> Option<EditorAction> {
		let normal_key = Self::to_normal_key(state, key)?;
		let mut keys = state.normal_sequence.clone();
		keys.push(normal_key);
		match mode_flow::resolve_normal_sequence_with_registry(&state.command_registry, &keys) {
			SequenceMatch::Action(AppAction::Editor(editor_action)) => Some(editor_action),
			_ => None,
		}
	}

	fn dispatch_internal<P>(ports: &P, state: &mut RimState, action: AppAction) -> ControlFlow<()>
	where P: StorageIo + FileWatcher + FilePicker {
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
				SystemAction::ReloadCommandConfig => {}
			},
		}
		ControlFlow::Continue(())
	}

	fn handle_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
	where P: StorageIo + FileWatcher + FilePicker {
		mode_flow::handle_key(ports, state, key)
	}

	fn to_normal_key(state: &RimState, key: KeyEvent) -> Option<NormalSequenceKey> {
		mode_flow::to_normal_key(state, key)
	}
}

#[cfg(test)]
mod tests;
