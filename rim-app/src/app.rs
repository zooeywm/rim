use std::{cell::RefCell, fs, ops::ControlFlow, path::{Path, PathBuf}, process::Command};

use anyhow::{Context, Result};
use rim_application::{action::{AppAction, FileAction, SystemAction}, config as application_config, state::RimState};
use rim_infra_file_watcher::FileWatcherState;
use rim_infra_input::InputPumpService;
use rim_infra_storage::StorageIoState;
use rim_infra_ui::{Renderer, TerminalSession};
use rim_ports::{FilePicker, FilePickerError, StorageIo};
use tracing::trace;

#[derive(derive_more::AsRef, derive_more::AsMut)]
pub struct App {
	// Runtime state is mutable because action dispatch mutates application and domain state.
	#[as_mut]
	state:              RimState,
	// Concrete infrastructure states are kept in the single app container.
	#[as_ref]
	storage_io:         StorageIoState,
	#[as_ref]
	file_watcher:       FileWatcherState,
	terminal_session:   RefCell<Option<TerminalSession>>,
	input_pump_service: RefCell<InputPumpService>,
	event_tx:           flume::Sender<AppAction>,
	// Event bus is the glue between runtime producers and application consumers.
	event_rx:           flume::Receiver<AppAction>,
}

pub(crate) struct AppPorts<'a> {
	pub(crate) storage_io:       &'a StorageIoState,
	pub(crate) file_watcher:     &'a FileWatcherState,
	pub(crate) terminal_session: &'a RefCell<Option<TerminalSession>>,
	pub(crate) input_pump:       &'a RefCell<InputPumpService>,
}

impl<'a> AppPorts<'a> {
	fn new(
		storage_io: &'a StorageIoState,
		file_watcher: &'a FileWatcherState,
		terminal_session: &'a RefCell<Option<TerminalSession>>,
		input_pump: &'a RefCell<InputPumpService>,
	) -> Self {
		Self { storage_io, file_watcher, terminal_session, input_pump }
	}
}

impl App {
	pub fn new(workspace_root: PathBuf) -> Result<Self> {
		// One bounded queue coordinates input, IO callbacks, and application actions.
		let (event_tx, event_rx) = flume::bounded(1024);
		let mut state = RimState::new();
		state.set_workspace_root(workspace_root);
		let config_errors = application_config::apply_all_configs(&mut state);
		application_config::apply_config_errors_to_status(&mut state, config_errors);
		Ok(Self {
			state,
			storage_io: StorageIoState::new(event_tx.clone()),
			file_watcher: FileWatcherState::new(event_tx.clone()),
			terminal_session: RefCell::new(None),
			input_pump_service: RefCell::new(InputPumpService::new(event_tx.clone())),
			event_tx,
			event_rx,
		})
	}

	pub fn start_services(&self) {
		// Infrastructure workers run independently and push completion events to the
		// bus.
		self.storage_io.start();
		self.file_watcher.start();
		for config_path in [
			application_config::keymaps_config_path(),
			application_config::commands_config_path(),
			application_config::editor_config_path(),
		] {
			if !config_path.is_file() {
				continue;
			}
			if let Err(err) = self.file_watcher.enqueue_watch_config(config_path.clone()) {
				tracing::error!("watch config failed: path={} error={}", config_path.display(), err);
			}
		}
		if let Err(err) =
			self.file_watcher.enqueue_watch_workspace_root(self.state.workspace_root().to_path_buf())
		{
			tracing::error!(
				"watch workspace root failed: path={} error={}",
				self.state.workspace_root().display(),
				err
			);
		}
		let tick_tx = self.event_tx.clone();
		std::thread::spawn(move || {
			loop {
				std::thread::sleep(std::time::Duration::from_millis(200));
				if tick_tx.send(AppAction::System(SystemAction::Tick)).is_err() {
					break;
				}
			}
		});
	}

	pub fn open_startup_files(&mut self, file_paths: Vec<PathBuf>) {
		// Startup file opening is expressed as regular actions to reuse the same
		// application flow.
		if file_paths.is_empty() {
			let ports =
				AppPorts::new(&self.storage_io, &self.file_watcher, &self.terminal_session, &self.input_pump_service);
			if let Err(err) = ports.enqueue_load_workspace_session() {
				self.state.create_untitled_buffer();
				self.state.workbench.status_bar.message = format!("session load failed: {}", err);
			}
			return;
		}
		for path in file_paths {
			let _ = self.process_action(AppAction::File(FileAction::OpenRequested { path }));
		}
	}

	pub fn run(mut self, file_paths: Vec<PathBuf>) -> Result<()> {
		// Start external workers first, then seed startup actions into the application.
		self.start_services();
		self.open_startup_files(file_paths);

		// Terminal session and input pump are pure runtime concerns.
		let title = self.state.workbench.title.clone();
		let terminal_session = TerminalSession::enter(title.as_str()).context("enter terminal session failed")?;
		self.terminal_session.replace(Some(terminal_session));
		{
			let mut terminal_session = self.terminal_session.borrow_mut();
			terminal_session
				.as_mut()
				.expect("terminal session should exist while app is running")
				.sync_cursor_style(self.state.mode)
				.context("sync cursor style failed")?;
		}
		self.input_pump_service.borrow_mut().start();
		let mut renderer = Renderer::new();

		loop {
			// Render from the current state snapshot.
			{
				let mut terminal_session = self.terminal_session.borrow_mut();
				terminal_session
					.as_mut()
					.expect("terminal session should exist while app is running")
					.draw(|frame| renderer.render(frame, &mut self.state))
					.context("terminal draw failed")?;
			}
			trace!("redraw");

			// Pull one action from the event bus and dispatch it through the
			// application handler.
			let action = self.event_rx.recv().context("event bus disconnected while waiting for next action")?;
			if Self::action_affects_layout(&action) {
				renderer.mark_layout_dirty();
			}
			if self.process_action(action).is_break() {
				break;
			}
			// Cursor shape is synchronized after each state transition.
			let mut terminal_session = self.terminal_session.borrow_mut();
			terminal_session
				.as_mut()
				.expect("terminal session should exist while app is running")
				.sync_cursor_style(self.state.mode)
				.context("sync cursor style failed")?;
		}
		Ok(())
	}

	pub fn process_action(&mut self, action: AppAction) -> ControlFlow<()> {
		if matches!(action, AppAction::System(SystemAction::ReloadConfig)) {
			return self.reload_all_configs();
		}
		// All state transitions must go through one handler entrypoint.
		let state = &mut self.state;
		let ports =
			AppPorts::new(&self.storage_io, &self.file_watcher, &self.terminal_session, &self.input_pump_service);
		state.apply_action(&ports, action)
	}

	pub fn action_affects_layout(action: &AppAction) -> bool {
		matches!(
			action,
			AppAction::Editor(_)
				| AppAction::Layout(_)
				| AppAction::File(FileAction::WorkspaceSessionLoaded { .. })
		)
	}

	fn reload_all_configs(&mut self) -> ControlFlow<()> {
		let config_errors = application_config::apply_all_configs(&mut self.state);
		self.state.refresh_key_hints_overlay_after_config_reload();
		self.state.refresh_command_palette();
		if config_errors.is_empty() {
			self.state.workbench.status_bar.message = "config reloaded".to_string();
		} else {
			application_config::apply_config_errors_to_status(&mut self.state, config_errors);
		}
		ControlFlow::Continue(())
	}
}

pub fn detect_workspace_root(start_dir: &Path) -> PathBuf {
	let git_root = Command::new("git")
		.arg("rev-parse")
		.arg("--show-toplevel")
		.current_dir(start_dir)
		.output()
		.ok()
		.filter(|output| output.status.success())
		.and_then(|output| String::from_utf8(output.stdout).ok())
		.map(|stdout| stdout.trim().to_string())
		.filter(|stdout| !stdout.is_empty())
		.map(PathBuf::from);
	let root = git_root.unwrap_or_else(|| start_dir.to_path_buf());
	fs::canonicalize(&root).unwrap_or(root)
}

impl FilePicker for AppPorts<'_> {
	fn pick_open_path(&self) -> Result<Option<PathBuf>, FilePickerError> {
		let chooser_file = std::env::temp_dir().join(format!(
			"rim-yazi-chooser-{}-{}.txt",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map(|duration| duration.as_nanos())
				.unwrap_or_default()
		));
		let mut terminal_session = self.terminal_session.borrow_mut();
		let Some(terminal_session) = terminal_session.as_mut() else {
			return Err(FilePickerError::Unavailable { message: "terminal session is not active" });
		};
		self.input_pump.borrow_mut().stop();
		if let Err(err) = terminal_session.suspend() {
			self.input_pump.borrow_mut().start();
			return Err(FilePickerError::Failed { message: format!("suspend terminal failed: {}", err) });
		}

		let command_result = Command::new("yazi").arg("--chooser-file").arg(&chooser_file).status();
		let resume_result = terminal_session.resume();
		self.input_pump.borrow_mut().start();

		if let Err(err) = resume_result {
			let _ = fs::remove_file(&chooser_file);
			return Err(FilePickerError::Failed { message: format!("resume terminal failed: {}", err) });
		}

		let status = command_result
			.map_err(|err| FilePickerError::Failed { message: format!("spawn yazi failed: {}", err) })?;
		if !status.success() {
			let _ = fs::remove_file(&chooser_file);
			let message = match status.code() {
				Some(code) => format!("yazi exited with status {}", code),
				None => "yazi terminated by signal".to_string(),
			};
			return Err(FilePickerError::Failed { message });
		}

		let selected_path = match fs::read_to_string(&chooser_file) {
			Ok(content) => content.lines().find(|line| !line.trim().is_empty()).map(PathBuf::from),
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
			Err(err) => {
				let _ = fs::remove_file(&chooser_file);
				return Err(FilePickerError::Failed { message: format!("read chooser output failed: {}", err) });
			}
		};
		let _ = fs::remove_file(&chooser_file);
		Ok(selected_path)
	}
}

#[cfg(test)]
mod tests {
	use rim_application::{command::{BindingMatch, BuiltinCommand, CommandConfigFile, CommandKeymapSection, CommandTarget, CursorCommand, KeyBindingOn, KeymapBindingConfig, ModeKeymapSections}, config as application_config, state::{KeymapScope, NormalSequenceKey, RimState}};

	#[test]
	fn reset_config_state_to_defaults_should_restore_removed_user_keymap_override() {
		let mut state = RimState::new();
		let errors = state.apply_command_config(&CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::single("go"),
						run:  "core.cursor.file_start".into(),
						desc: Some("custom".to_string()),
					}],
				},
				..ModeKeymapSections::default()
			},
			..CommandConfigFile::default()
		});
		assert!(errors.is_empty());
		assert_eq!(
			state.workbench.command_registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[
				NormalSequenceKey::Char('g'),
				NormalSequenceKey::Char('g')
			]),
			BindingMatch::NoMatch
		);

		application_config::reset_config_state_to_defaults(&mut state);

		assert_eq!(
			state.workbench.command_registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[
				NormalSequenceKey::Char('g'),
				NormalSequenceKey::Char('g')
			]),
			BindingMatch::Exact(CommandTarget::Builtin(BuiltinCommand::Cursor(CursorCommand::FileStart)))
		);
		assert_eq!(
			state.workbench.command_registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[
				NormalSequenceKey::Char('g'),
				NormalSequenceKey::Char('o')
			]),
			BindingMatch::NoMatch
		);
	}

	#[test]
	fn reset_config_state_to_defaults_should_restore_default_key_hint_description() {
		let mut state = RimState::new();
		let errors = state.apply_command_config(&CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::single("gg"),
						run:  "core.cursor.file_start".into(),
						desc: Some("Jump to beginning".to_string()),
					}],
				},
				..ModeKeymapSections::default()
			},
			..CommandConfigFile::default()
		});
		assert!(errors.is_empty());
		assert_eq!(
			state.workbench.command_registry.key_hints(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('g')])[0]
				.summary,
			"Jump to beginning"
		);

		application_config::reset_config_state_to_defaults(&mut state);

		assert_eq!(
			state.workbench.command_registry.key_hints(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('g')])[0]
				.summary,
			"Move to file start"
		);
	}
}
