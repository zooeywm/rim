use std::{ops::ControlFlow, path::PathBuf};

use tracing::error;

use super::ActionHandlerError;
use crate::{action::{AppAction, FileAction, KeyCode, KeyEvent, WindowAction}, command::{BindingMatch, BuiltinCommand, CommandCommand, CommandPaletteCommand, CommandTarget, InsertCommand, ModeCommand, OverlayCommand, PickerCommand, ResolvedCommand}, ports::{FilePicker, FileWatcher, StorageIo}, state::{KeymapScope, NotificationLevel, RimState}};

pub(super) fn handle_command_mode_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	if let Some(flow) = dispatch_scope_key(ports, state, key, KeymapScope::OverlayCommandPalette) {
		return flow;
	}
	if let Some(flow) = dispatch_scope_key(ports, state, key, KeymapScope::ModeCommand) {
		return flow;
	}
	if key.modifiers.contains(crate::action::KeyModifiers::CONTROL) {
		return ControlFlow::Continue(());
	}
	match key.code {
		KeyCode::Char(ch) => {
			state.push_command_char(ch);
			ensure_command_palette_workspace_files(ports, state);
			enqueue_command_palette_preview(ports, state, true);
		}
		KeyCode::Tab => {
			let _ = state.complete_command_palette_selection();
			ensure_command_palette_workspace_files(ports, state);
			enqueue_command_palette_preview(ports, state, true);
		}
		_ => {}
	}
	ControlFlow::Continue(())
}

pub(super) fn handle_workspace_file_picker_key<P>(
	ports: &P,
	state: &mut RimState,
	key: KeyEvent,
) -> ControlFlow<()>
where
	P: StorageIo + FileWatcher + FilePicker,
{
	if let Some(flow) = dispatch_scope_key(ports, state, key, KeymapScope::OverlayPicker) {
		return flow;
	}
	if key.modifiers.contains(crate::action::KeyModifiers::CONTROL) || state.workspace_file_picker_loading() {
		return ControlFlow::Continue(());
	}

	match key.code {
		KeyCode::Backspace => {
			state.pop_workspace_file_picker_char();
			enqueue_workspace_file_picker_preview(ports, state, true);
		}
		KeyCode::Char(ch) => {
			state.push_workspace_file_picker_char(ch);
			enqueue_workspace_file_picker_preview(ports, state, true);
		}
		_ => {}
	}

	ControlFlow::Continue(())
}

fn dispatch_scope_key<P>(
	ports: &P,
	state: &mut RimState,
	key: KeyEvent,
	scope: KeymapScope,
) -> Option<ControlFlow<()>>
where
	P: StorageIo + FileWatcher + FilePicker,
{
	let normal_key = super::mode_flow::to_normal_key(state, key)?;
	match state.command_registry.resolve_scope_sequence(scope, &[normal_key]) {
		BindingMatch::Exact(CommandTarget::Builtin(builtin)) => {
			Some(execute_builtin_command(ports, state, builtin, None))
		}
		BindingMatch::Exact(target) => Some(execute_command_target(ports, state, target, None)),
		BindingMatch::Pending | BindingMatch::NoMatch => None,
	}
}

fn execute_current_command_input<P>(ports: &P, state: &mut RimState) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	let raw_command = state.command_line.clone();
	let command = raw_command.trim().to_string();
	if command.is_empty() {
		state.exit_command_mode();
		return ControlFlow::Continue(());
	}
	let resolved = state.command_registry.resolve_command_input(command.as_str());
	let Some(resolved) = resolved else {
		state.push_notification(NotificationLevel::Error, format!("unknown command: {}", command));
		return ControlFlow::Continue(());
	};
	state.exit_command_mode();
	execute_resolved_command(ports, state, resolved)
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
			state.push_notification(NotificationLevel::Warn, format!("plugin command unavailable: {}", command_id));
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
		BuiltinCommand::Command(CommandCommand::Quit) => quit_current_scope(ports, state, false),
		BuiltinCommand::Command(CommandCommand::QuitForce) => quit_current_scope(ports, state, true),
		BuiltinCommand::Command(CommandCommand::QuitAll) => quit_application(ports, state, false),
		BuiltinCommand::Command(CommandCommand::QuitAllForce) => quit_application(ports, state, true),
		BuiltinCommand::Command(CommandCommand::Save) => {
			enqueue_save_active_buffer(ports, state, false, false, argument.map(PathBuf::from));
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::SaveForce) => {
			enqueue_save_active_buffer(ports, state, false, true, argument.map(PathBuf::from));
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::SaveAll) => {
			enqueue_save_all_buffers(ports, state, false, false);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::SaveAndQuit) => {
			enqueue_save_active_buffer(ports, state, true, false, argument.map(PathBuf::from));
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::SaveAndQuitForce) => {
			enqueue_save_active_buffer(ports, state, true, true, argument.map(PathBuf::from));
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::SaveAllAndQuit) => {
			enqueue_save_all_buffers(ports, state, true, false);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::SaveAllAndQuitForce) => {
			enqueue_save_all_buffers(ports, state, true, true);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::Reload) => {
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
		BuiltinCommand::Command(CommandCommand::ReloadForce) => {
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
		BuiltinCommand::Picker(PickerCommand::Files) => {
			open_workspace_file_picker(ports, state);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Picker(PickerCommand::Yazi) => {
			enqueue_open_with_picker(ports, state);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Mode(ModeCommand::Normal) => {
			if state.is_command_mode() {
				state.exit_command_mode();
			} else if state.is_insert_mode() {
				state.exit_insert_mode();
			} else if state.is_visual_mode() {
				state.exit_visual_mode();
			} else if state.key_hints_open() {
				state.close_key_hints();
			} else if state.workspace_file_picker_open() {
				state.close_workspace_file_picker();
			} else if state.notification_center_open() {
				state.close_notification_center();
			}
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::Submit) => execute_current_command_input(ports, state),
		BuiltinCommand::Command(CommandCommand::Backspace) => {
			state.pop_command_char();
			ensure_command_palette_workspace_files(ports, state);
			enqueue_command_palette_preview(ports, state, true);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Command(CommandCommand::Notifications) => {
			state.open_notification_center();
			ControlFlow::Continue(())
		}
		BuiltinCommand::CommandPalette(CommandPaletteCommand::PageUp) => {
			let moved = state.page_command_palette_selection(-1);
			enqueue_command_palette_preview(ports, state, moved);
			ControlFlow::Continue(())
		}
		BuiltinCommand::CommandPalette(CommandPaletteCommand::PageDown) => {
			let moved = state.page_command_palette_selection(1);
			enqueue_command_palette_preview(ports, state, moved);
			ControlFlow::Continue(())
		}
		BuiltinCommand::CommandPalette(CommandPaletteCommand::PreviewScrollDown) => {
			let _ = state.scroll_command_palette_preview(1);
			ControlFlow::Continue(())
		}
		BuiltinCommand::CommandPalette(CommandPaletteCommand::PreviewScrollUp) => {
			let _ = state.scroll_command_palette_preview(-1);
			ControlFlow::Continue(())
		}
		BuiltinCommand::CommandPalette(CommandPaletteCommand::Prev) => {
			let moved = state.move_command_palette_selection(-1);
			enqueue_command_palette_preview(ports, state, moved);
			ControlFlow::Continue(())
		}
		BuiltinCommand::CommandPalette(CommandPaletteCommand::Next) => {
			let moved = state.move_command_palette_selection(1);
			enqueue_command_palette_preview(ports, state, moved);
			ControlFlow::Continue(())
		}
		BuiltinCommand::View(crate::command::ViewCommand::ToggleWordWrap) => {
			state.toggle_word_wrap();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Picker(PickerCommand::Prev) => {
			if !state.workspace_file_picker_loading() {
				let moved = state.move_workspace_file_picker_selection(-1);
				enqueue_workspace_file_picker_preview(ports, state, moved);
			}
			ControlFlow::Continue(())
		}
		BuiltinCommand::Picker(PickerCommand::Next) => {
			if !state.workspace_file_picker_loading() {
				let moved = state.move_workspace_file_picker_selection(1);
				enqueue_workspace_file_picker_preview(ports, state, moved);
			}
			ControlFlow::Continue(())
		}
		BuiltinCommand::Picker(PickerCommand::PreviewScrollDown) => {
			let _ = state.scroll_workspace_file_picker_preview(1);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Picker(PickerCommand::PreviewScrollUp) => {
			let _ = state.scroll_workspace_file_picker_preview(-1);
			ControlFlow::Continue(())
		}
		BuiltinCommand::Picker(PickerCommand::TogglePreviewWordWrap) => {
			state.toggle_picker_preview_word_wrap();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Picker(PickerCommand::Confirm) => {
			let Some(path) = state.selected_workspace_file_picker_path().map(PathBuf::from) else {
				state.push_notification(NotificationLevel::Warn, "open failed: no file selected");
				return ControlFlow::Continue(());
			};
			state.close_workspace_file_picker();
			RimState::dispatch_internal(ports, state, AppAction::File(FileAction::OpenRequested { path }))
		}
		BuiltinCommand::Overlay(OverlayCommand::Close) => {
			if state.key_hints_open() {
				state.close_key_hints();
			} else if state.workspace_file_picker_open() {
				state.close_workspace_file_picker();
			}
			ControlFlow::Continue(())
		}
		BuiltinCommand::Overlay(OverlayCommand::Back) => {
			let _ = state.step_back_key_hint_prefix();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Insert(InsertCommand::Newline) => {
			state.insert_newline_at_cursor();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Insert(InsertCommand::Backspace) => {
			state.backspace_at_cursor();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Insert(InsertCommand::Left) => {
			state.move_cursor_left();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Insert(InsertCommand::Down) => {
			state.move_cursor_down();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Insert(InsertCommand::Up) => {
			state.move_cursor_up();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Insert(InsertCommand::Right) => {
			state.move_cursor_right_for_insert();
			ControlFlow::Continue(())
		}
		BuiltinCommand::Insert(InsertCommand::Tab) => {
			state.insert_char_at_cursor('\t');
			ControlFlow::Continue(())
		}
		_ => {
			let action =
				command.normal_mode_action().expect("normal-mode builtin command should map to app action");
			RimState::dispatch_internal(ports, state, action)
		}
	}
}

fn ensure_command_palette_workspace_files<P>(ports: &P, state: &mut RimState)
where P: StorageIo {
	if !state.command_palette_needs_workspace_files() {
		return;
	}
	state.begin_workspace_file_cache_loading();
	if let Err(source) = ports.enqueue_list_workspace_files(state.workspace_root().to_path_buf()) {
		let err = ActionHandlerError::Reload { source };
		error!("workspace file list enqueue failed for command palette: {}", err);
		state.fail_workspace_file_cache_loading();
		state.status_bar.message = format!("workspace file list failed: {}", err);
	}
}

fn enqueue_command_palette_preview<P>(ports: &P, state: &mut RimState, force: bool)
where P: StorageIo {
	if !force {
		return;
	}
	let Some(path) = state.selected_command_palette_file_path().map(PathBuf::from) else {
		return;
	};
	state.set_command_palette_preview_loading(path.as_path());
	if let Err(source) = ports.enqueue_load_workspace_file_preview(path.clone()) {
		let err = ActionHandlerError::Reload { source };
		error!("command palette preview enqueue failed: path={} error={}", path.display(), err);
		state.set_command_palette_preview(path.as_path(), format!("<preview error: {}>", err));
	}
}

fn open_workspace_file_picker<P>(ports: &P, state: &mut RimState)
where P: FilePicker + StorageIo + FileWatcher {
	if state.has_workspace_file_cache() {
		state.open_workspace_file_picker(state.workspace_file_cache_entries().to_vec());
		enqueue_workspace_file_picker_preview(ports, state, true);
		return;
	}
	if state.workspace_file_cache_is_loading() {
		state.open_workspace_file_picker_loading();
		return;
	}
	state.open_workspace_file_picker_loading();
	state.begin_workspace_file_cache_loading();
	if let Err(source) = ports.enqueue_list_workspace_files(state.workspace_root().to_path_buf()) {
		let err = ActionHandlerError::Reload { source };
		error!("workspace file picker enqueue failed: {}", err);
		state.fail_workspace_file_cache_loading();
		state.close_workspace_file_picker();
		state.status_bar.message = format!("workspace file picker failed: {}", err);
	}
}

fn enqueue_workspace_file_picker_preview<P>(ports: &P, state: &mut RimState, force: bool)
where P: StorageIo {
	if !force {
		return;
	}
	let Some(path) = state.selected_workspace_file_picker_path().map(PathBuf::from) else {
		state.clear_workspace_file_picker_preview();
		return;
	};
	state.set_workspace_file_picker_preview_loading(path.as_path());
	if let Err(source) = ports.enqueue_load_workspace_file_preview(path.clone()) {
		let err = ActionHandlerError::Reload { source };
		error!("workspace preview enqueue failed: path={} error={}", path.display(), err);
		state.set_workspace_file_picker_preview(path.as_path(), format!("<preview error: {}>", err));
	}
}

fn quit_application<P>(ports: &P, state: &mut RimState, force: bool) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	if !force && state.has_dirty_buffers() {
		state.status_bar.message = "quit all blocked: unsaved changes".to_string();
		return ControlFlow::Continue(());
	}
	state.force_quit_trim_file_dirty_in_session = force;
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
	state.force_quit_trim_file_dirty_in_session = force;
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
