use std::{ops::ControlFlow, path::PathBuf};

use anyhow::{Context, Result};
use rim_infra_file_io::{FileIoImpl, FileIoState};
use rim_infra_file_watcher::{FileWatcherImpl, FileWatcherState};
use rim_infra_input::InputPumpService;
use rim_infra_swap::{SwapIoImpl, SwapIoState};
use rim_infra_ui::{Renderer, TerminalSession};
use rim_kernel::{action::{AppAction, FileAction}, ports::{FileIo, FileIoError, FileWatcher, FileWatcherError, SwapEditOp, SwapIo, SwapIoError}, state::{BufferId, RimState}};
use tracing::trace;

#[derive(derive_more::AsRef, derive_more::AsMut)]
pub struct App {
	#[as_mut]
	// Kernel state is mutable because action dispatch mutates domain state.
	state: RimState,
	#[as_ref]
	// Concrete infrastructure states are kept in the single app container.
	file_io: FileIoState,
	#[as_ref]
	file_watcher: FileWatcherState,
	#[as_ref]
	swap_io:      SwapIoState,
	// Event bus is the glue between runtime producers and kernel consumers.
	event_tx:     flume::Sender<AppAction>,
	event_rx:     flume::Receiver<AppAction>,
}

struct AppPorts<'a> {
	file_io:      &'a FileIoState,
	file_watcher: &'a FileWatcherState,
	swap_io:      &'a SwapIoState,
}

impl FileIo for AppPorts<'_> {
	fn enqueue_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self.file_io).enqueue_load(buffer_id, path)
	}

	fn enqueue_save(&self, buffer_id: BufferId, path: PathBuf, text: String) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self.file_io).enqueue_save(buffer_id, path, text)
	}

	fn enqueue_external_load(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileIoError> {
		FileIoImpl::inj_ref(self.file_io).enqueue_external_load(buffer_id, path)
	}
}

impl FileWatcher for AppPorts<'_> {
	fn enqueue_watch(&self, buffer_id: BufferId, path: PathBuf) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self.file_watcher).enqueue_watch(buffer_id, path)
	}

	fn enqueue_unwatch(&self, buffer_id: BufferId) -> Result<(), FileWatcherError> {
		FileWatcherImpl::inj_ref(self.file_watcher).enqueue_unwatch(buffer_id)
	}
}

impl SwapIo for AppPorts<'_> {
	fn enqueue_open(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError> {
		SwapIoImpl::inj_ref(self.swap_io).enqueue_open(buffer_id, source_path)
	}

	fn enqueue_detect_conflict(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError> {
		SwapIoImpl::inj_ref(self.swap_io).enqueue_detect_conflict(buffer_id, source_path)
	}

	fn enqueue_edit(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		op: SwapEditOp,
	) -> Result<(), SwapIoError> {
		SwapIoImpl::inj_ref(self.swap_io).enqueue_edit(buffer_id, source_path, op)
	}

	fn enqueue_mark_clean(&self, buffer_id: BufferId, source_path: PathBuf) -> Result<(), SwapIoError> {
		SwapIoImpl::inj_ref(self.swap_io).enqueue_mark_clean(buffer_id, source_path)
	}

	fn enqueue_initialize_base(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
		delete_existing: bool,
	) -> Result<(), SwapIoError> {
		SwapIoImpl::inj_ref(self.swap_io).enqueue_initialize_base(
			buffer_id,
			source_path,
			base_text,
			delete_existing,
		)
	}

	fn enqueue_recover(
		&self,
		buffer_id: BufferId,
		source_path: PathBuf,
		base_text: String,
	) -> Result<(), SwapIoError> {
		SwapIoImpl::inj_ref(self.swap_io).enqueue_recover(buffer_id, source_path, base_text)
	}

	fn enqueue_close(&self, buffer_id: BufferId) -> Result<(), SwapIoError> {
		SwapIoImpl::inj_ref(self.swap_io).enqueue_close(buffer_id)
	}
}

impl App {
	pub fn new() -> Result<Self> {
		// One bounded queue coordinates input, IO callbacks, and kernel actions.
		let (event_tx, event_rx) = flume::bounded(1024);

		Ok(Self {
			state: RimState::new(),
			file_io: FileIoState::new(event_tx.clone()),
			file_watcher: FileWatcherState::new(event_tx.clone()),
			swap_io: SwapIoState::new(event_tx.clone()),
			event_tx,
			event_rx,
		})
	}

	pub fn start_services(&self) {
		// Infrastructure workers run independently and push completion events to the
		// bus.
		self.file_io.start();
		self.file_watcher.start();
		self.swap_io.start();
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
		let ports =
			AppPorts { file_io: &self.file_io, file_watcher: &self.file_watcher, swap_io: &self.swap_io };
		self.state.apply_action(&ports, action)
	}

	pub fn action_affects_layout(action: &AppAction) -> bool {
		matches!(action, AppAction::Editor(_) | AppAction::Layout(_))
	}
}
