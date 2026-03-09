use std::{ops::ControlFlow, path::PathBuf};

use anyhow::{Context, Result};
use rim_infra_file_watcher::FileWatcherState;
use rim_infra_input::InputPumpService;
use rim_infra_storage::StorageIoState;
use rim_infra_ui::{Renderer, TerminalSession};
use rim_kernel::{action::{AppAction, FileAction}, state::RimState};
use tracing::trace;

#[derive(derive_more::AsRef, derive_more::AsMut)]
pub struct App {
	// Kernel state is mutable because action dispatch mutates domain state.
	#[as_mut]
	state:        RimState,
	// Concrete infrastructure states are kept in the single app container.
	#[as_ref]
	storage_io:   StorageIoState,
	#[as_ref]
	file_watcher: FileWatcherState,
	// Event bus is the glue between runtime producers and kernel consumers.
	event_tx:     flume::Sender<AppAction>,
	event_rx:     flume::Receiver<AppAction>,
}

pub(crate) struct AppPorts<'a> {
	pub(crate) storage_io:   &'a StorageIoState,
	pub(crate) file_watcher: &'a FileWatcherState,
}

impl<'a> AppPorts<'a> {
	fn new(storage_io: &'a StorageIoState, file_watcher: &'a FileWatcherState) -> Self {
		Self { storage_io, file_watcher }
	}
}

impl App {
	pub fn new() -> Result<Self> {
		// One bounded queue coordinates input, IO callbacks, and kernel actions.
		let (event_tx, event_rx) = flume::bounded(1024);

		Ok(Self {
			state: RimState::new(),
			storage_io: StorageIoState::new(event_tx.clone()),
			file_watcher: FileWatcherState::new(event_tx.clone()),
			event_tx,
			event_rx,
		})
	}

	pub fn start_services(&self) {
		// Infrastructure workers run independently and push completion events to the
		// bus.
		self.storage_io.start();
		self.file_watcher.start();
	}

	pub fn open_startup_files(&mut self, file_paths: Vec<PathBuf>) {
		// Startup file opening is expressed as regular actions to reuse the same kernel
		// flow.
		if file_paths.is_empty() {
			self.state.create_untitled_buffer();
			return;
		}
		for path in file_paths {
			let _ = self.process_action(AppAction::File(FileAction::OpenRequested { path }));
		}
	}

	pub fn run(mut self, file_paths: Vec<PathBuf>) -> Result<()> {
		// Start external workers first, then seed startup actions into the kernel.
		self.start_services();
		self.open_startup_files(file_paths);

		// Terminal session and input pump are pure runtime concerns.
		let title = self.state.title.clone();
		let mut terminal_session =
			TerminalSession::enter(title.as_str()).context("enter terminal session failed")?;
		terminal_session.sync_cursor_style(self.state.mode).context("sync cursor style failed")?;
		let mut input_pump_service = InputPumpService::new(self.event_tx.clone());
		input_pump_service.start();
		let mut renderer = Renderer::new();

		loop {
			// Render from current kernel state snapshot.
			terminal_session
				.draw(|frame| renderer.render(frame, &mut self.state))
				.context("terminal draw failed")?;
			trace!("redraw");

			// Pull one action from the event bus and dispatch it through the kernel
			// handler.
			let action = self.event_rx.recv().context("event bus disconnected while waiting for next action")?;
			if Self::action_affects_layout(&action) {
				renderer.mark_layout_dirty();
			}
			if self.process_action(action).is_break() {
				break;
			}
			// Cursor shape is synchronized after each state transition.
			terminal_session.sync_cursor_style(self.state.mode).context("sync cursor style failed")?;
		}
		Ok(())
	}

	pub fn process_action(&mut self, action: AppAction) -> ControlFlow<()> {
		// All domain transitions must go through one handler entrypoint.
		let state = &mut self.state;
		let ports = AppPorts::new(&self.storage_io, &self.file_watcher);
		state.apply_action(&ports, action)
	}

	pub fn action_affects_layout(action: &AppAction) -> bool {
		matches!(action, AppAction::Editor(_) | AppAction::Layout(_))
	}
}
