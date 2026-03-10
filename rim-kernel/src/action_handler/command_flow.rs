use std::{ops::ControlFlow, path::PathBuf};

use tracing::error;

use super::ActionHandlerError;
use crate::{action::{AppAction, FileAction, KeyCode, KeyEvent, WindowAction}, command::{BuiltinCommand, CommandTarget, ResolvedCommand}, ports::{FilePicker, FileWatcher, StorageIo}, state::RimState};

pub(super) fn handle_command_mode_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	if key.modifiers.contains(crate::action::KeyModifiers::CONTROL) {
		return ControlFlow::Continue(());
	}

	match key.code {
		KeyCode::Esc => state.exit_command_mode(),
		KeyCode::Enter => {
			let command = state.take_command_line();
			if command.is_empty() {
				return ControlFlow::Continue(());
			}
			let Some(resolved) = state.command_registry.resolve_command_input(command.as_str()) else {
				state.status_bar.message = format!("unknown command: {}", command);
				return ControlFlow::Continue(());
			};
			return execute_resolved_command(ports, state, resolved);
		}
		KeyCode::Backspace => state.pop_command_char(),
		KeyCode::Char(ch) => state.push_command_char(ch),
		_ => {}
	}
	ControlFlow::Continue(())
}

pub(super) fn execute_command_target<P>(
	ports: &P,
	state: &mut RimState,
	target: CommandTarget,
	argument: Option<String>,
) -> ControlFlow<()>
where
	P: StorageIo + FileWatcher + FilePicker,
{
	match target {
		CommandTarget::Builtin(builtin) => execute_builtin_command(ports, state, builtin, argument),
		CommandTarget::Plugin { command_id, .. } => {
			state.status_bar.message = format!("plugin command unavailable: {}", command_id);
			ControlFlow::Continue(())
		}
	}
}

pub(super) fn execute_resolved_command<P>(
	ports: &P,
	state: &mut RimState,
	resolved: ResolvedCommand,
) -> ControlFlow<()>
where
	P: StorageIo + FileWatcher + FilePicker,
{
	execute_command_target(ports, state, resolved.spec.target, resolved.argument)
}

fn execute_builtin_command<P>(
	ports: &P,
	state: &mut RimState,
	command: BuiltinCommand,
	argument: Option<String>,
) -> ControlFlow<()>
where
	P: StorageIo + FileWatcher + FilePicker,
{
	match command {
		command if command.normal_mode_action().is_some() => {
			let action = command.normal_mode_action().expect("checked above");
			RimState::dispatch_internal(ports, state, action)
		}
		BuiltinCommand::Quit => quit_current_scope(ports, state, false),
		BuiltinCommand::QuitForce => quit_current_scope(ports, state, true),
		BuiltinCommand::QuitAll => quit_application(ports, state, false),
		BuiltinCommand::QuitAllForce => quit_application(ports, state, true),
		BuiltinCommand::Save => {
			enqueue_save_active_buffer(ports, state, false, false, argument.map(PathBuf::from));
			ControlFlow::Continue(())
		}
		BuiltinCommand::SaveForce => {
			enqueue_save_active_buffer(ports, state, false, true, argument.map(PathBuf::from));
			ControlFlow::Continue(())
		}
		BuiltinCommand::SaveAll => {
			enqueue_save_all_buffers(ports, state, false, false);
			ControlFlow::Continue(())
		}
		BuiltinCommand::SaveAndQuit => {
			enqueue_save_active_buffer(ports, state, true, false, argument.map(PathBuf::from));
			ControlFlow::Continue(())
		}
		BuiltinCommand::SaveAndQuitForce => {
			enqueue_save_active_buffer(ports, state, true, true, argument.map(PathBuf::from));
			ControlFlow::Continue(())
		}
		BuiltinCommand::SaveAllAndQuit => {
			enqueue_save_all_buffers(ports, state, true, false);
			ControlFlow::Continue(())
		}
		BuiltinCommand::SaveAllAndQuitForce => {
			enqueue_save_all_buffers(ports, state, true, true);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Reload => {
			if let Some(path) = argument {
				RimState::dispatch_internal(
					ports,
					state,
					AppAction::File(FileAction::OpenRequested { path: PathBuf::from(path) }),
				)
			} else {
				enqueue_reload_active_buffer(ports, state, false);
				ControlFlow::Continue(())
			}
		}
		BuiltinCommand::ReloadForce => {
			if let Some(path) = argument {
				RimState::dispatch_internal(
					ports,
					state,
					AppAction::File(FileAction::OpenRequested { path: PathBuf::from(path) }),
				)
			} else {
				enqueue_reload_active_buffer(ports, state, true);
				ControlFlow::Continue(())
			}
		}
		BuiltinCommand::OpenPickerYazi => {
			enqueue_open_with_picker(ports, state);
			ControlFlow::Continue(())
		}
		_ => {
			let action =
				command.normal_mode_action().expect("normal-mode builtin command should map to app action");
			RimState::dispatch_internal(ports, state, action)
		}
	}
}

fn quit_application<P>(ports: &P, state: &mut RimState, force: bool) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	if !force && state.has_dirty_buffers() {
		state.status_bar.message = "quit all blocked: unsaved changes".to_string();
		return ControlFlow::Continue(());
	}
	RimState::dispatch_internal(ports, state, AppAction::System(crate::action::SystemAction::Quit))
}

fn quit_current_scope<P>(ports: &P, state: &mut RimState, force: bool) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	if !force && state.has_dirty_buffers() {
		state.status_bar.message = "quit blocked: unsaved changes (use :q!)".to_string();
		return ControlFlow::Continue(());
	}
	if state.active_tab_window_ids().len() > 1 {
		return RimState::dispatch_internal(ports, state, AppAction::Window(WindowAction::CloseActive));
	}
	if state.tabs.len() > 1 {
		return RimState::dispatch_internal(ports, state, AppAction::Tab(crate::action::TabAction::CloseCurrent));
	}
	RimState::dispatch_internal(ports, state, AppAction::System(crate::action::SystemAction::Quit))
}

fn enqueue_save_active_buffer<P>(
	ports: &P,
	state: &mut RimState,
	quit_after_save: bool,
	force_overwrite: bool,
	path_override: Option<PathBuf>,
) where
	P: StorageIo + FileWatcher,
{
	if !force_overwrite
		&& path_override.is_none()
		&& matches!(state.active_buffer_is_externally_modified(), Some(true))
	{
		state.status_bar.message = "save blocked: file changed externally (use :w! to overwrite)".to_string();
		state.quit_after_save = false;
		return;
	}

	let bind_override_path =
		matches!((path_override.as_ref(), state.active_buffer_has_path()), (Some(_), Some(false)));
	let (buffer_id, path, text) = match state.active_buffer_save_snapshot(path_override.clone()) {
		Ok(snapshot) => snapshot,
		Err(reason) => {
			state.status_bar.message = format!("save failed: {}", reason);
			state.quit_after_save = false;
			return;
		}
	};

	state.in_flight_internal_saves.insert(buffer_id);
	if let Err(source) = ports.enqueue_save(buffer_id, path, text) {
		let err = ActionHandlerError::Save { source };
		error!("io worker unavailable while enqueueing file save: {}", err);
		state.status_bar.message = "save failed: io worker unavailable".to_string();
		state.in_flight_internal_saves.remove(&buffer_id);
		state.clear_recent_internal_save(buffer_id);
		state.quit_after_save = false;
		return;
	}

	if bind_override_path {
		state.set_pending_save_path(buffer_id, path_override);
	} else {
		state.set_pending_save_path(buffer_id, None);
	}
	state.quit_after_save = quit_after_save;
	state.status_bar.message = "saving...".to_string();
}

fn enqueue_reload_active_buffer<P>(ports: &P, state: &mut RimState, force_reload: bool)
where P: StorageIo + FileWatcher {
	let active_is_dirty =
		state.active_buffer_id().and_then(|id| state.buffers.get(id)).map(|buffer| buffer.dirty).unwrap_or(false);
	if !force_reload && active_is_dirty {
		state.status_bar.message = "reload blocked: buffer is dirty (use :e! to force reload)".to_string();
		return;
	}

	let (buffer_id, path) = match state.active_buffer_load_target() {
		Ok(target) => target,
		Err(reason) => {
			state.status_bar.message = format!("reload failed: {}", reason);
			return;
		}
	};

	if let Err(source) = ports.enqueue_load(buffer_id, path.clone()) {
		let err = ActionHandlerError::Reload { source };
		error!("io worker unavailable while enqueueing file load: {}", err);
		state.status_bar.message = "reload failed: io worker unavailable".to_string();
		return;
	}
	state.status_bar.message = format!("loading {}", path.display());
}

fn enqueue_open_with_picker<P>(ports: &P, state: &mut RimState)
where P: FilePicker + StorageIo + FileWatcher {
	match ports.pick_open_path() {
		Ok(Some(path)) => {
			let _ = RimState::dispatch_internal(ports, state, AppAction::File(FileAction::OpenRequested { path }));
		}
		Ok(None) => {
			state.status_bar.message = "open cancelled".to_string();
		}
		Err(err) => {
			error!("file picker failed: {}", err);
			state.status_bar.message = format!("open failed: {}", err);
		}
	}
}

fn enqueue_save_all_buffers<P>(
	ports: &P,
	state: &mut RimState,
	quit_after_save: bool,
	force_overwrite: bool,
) where
	P: StorageIo + FileWatcher,
{
	let (snapshots, missing_path) = state.all_buffer_save_snapshots();
	if missing_path > 0 {
		state.status_bar.message = format!("save all failed: {} buffer(s) have no file path", missing_path);
		state.quit_after_save = false;
		return;
	}
	if !force_overwrite
		&& snapshots.iter().any(|(buffer_id, ..)| {
			state.buffers.get(*buffer_id).map(|buffer| buffer.externally_modified).unwrap_or(false)
		}) {
		state.status_bar.message =
			"save all blocked: file changed externally (use :wqa! to overwrite)".to_string();
		state.quit_after_save = false;
		return;
	}
	if snapshots.is_empty() {
		if missing_path > 0 {
			state.status_bar.message = "save failed: no buffer has file path".to_string();
		} else {
			state.status_bar.message = "nothing to save".to_string();
		}
		state.quit_after_save = false;
		return;
	}

	let mut enqueued = 0usize;
	for (buffer_id, path, text) in snapshots {
		state.in_flight_internal_saves.insert(buffer_id);
		if let Err(source) = ports.enqueue_save(buffer_id, path, text) {
			let err = ActionHandlerError::SaveAll { source };
			error!("io worker unavailable while enqueueing file save: {}", err);
			state.status_bar.message = "save failed: io worker unavailable".to_string();
			state.in_flight_internal_saves.remove(&buffer_id);
			state.clear_recent_internal_save(buffer_id);
			state.quit_after_save = false;
			return;
		}
		enqueued = enqueued.saturating_add(1);
	}

	state.quit_after_save = quit_after_save;
	state.status_bar.message = format!("saving {} buffers...", enqueued);
}
