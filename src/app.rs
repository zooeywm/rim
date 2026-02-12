use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::trace;

use crate::action::{AppAction, FileAction};
use crate::action_handler::{ActionHandler, ActionHandlerState};
use crate::file_io_service::FileIoState;
use crate::file_watcher_service::FileWatcherState;
use crate::input::InputPumpService;
use crate::state::AppState;
use crate::ui::{Renderer, TerminalSession};
use std::ops::ControlFlow;

#[derive(derive_more::AsRef, derive_more::AsMut)]
pub struct App {
    #[as_ref]
    #[as_mut]
    pub(super) state: crate::state::AppState,
    renderer: Renderer,
    #[as_ref]
    #[as_mut]
    pub(super) action_handler: ActionHandlerState,
    #[as_ref]
    #[as_mut]
    pub(super) file_io_service: crate::file_io_service::FileIoState,
    #[as_ref]
    #[as_mut]
    pub(super) file_watcher_service: crate::file_watcher_service::FileWatcherState,
    event_tx: flume::Sender<AppAction>,
    event_rx: flume::Receiver<AppAction>,
}

impl App {
    pub fn new() -> Result<Self> {
        let state = AppState::new();

        let (event_tx, event_rx) = flume::bounded(1024);
        let file_io_service = FileIoState::start(event_tx.clone());
        let file_watcher_service = FileWatcherState::start(event_tx.clone());

        Ok(Self {
            state,
            renderer: Renderer::new(),
            action_handler: ActionHandlerState::new(),
            file_io_service,
            file_watcher_service,
            event_tx,
            event_rx,
        })
    }

    fn bootstrap_files(&mut self, file_paths: Vec<PathBuf>) -> Result<()> {
        if file_paths.is_empty() {
            self.state.create_untitled_buffer();
            return Ok(());
        }
        for path in file_paths {
            self.event_tx
                .send(AppAction::File(FileAction::OpenRequested { path }))
                .context("event bus disconnected while enqueueing app action")?;
        }
        Ok(())
    }

    pub fn run(mut self, file_paths: Vec<PathBuf>) -> Result<()> {
        self.bootstrap_files(file_paths)
            .context("bootstrap startup files failed")?;
        let mut terminal_session = TerminalSession::enter(self.state.title.as_str())
            .context("enter terminal session failed")?;
        terminal_session
            .sync_cursor_style(self.state.mode)
            .context("sync cursor style failed")?;
        let mut input_pump_service = InputPumpService::new();
        input_pump_service.start(self.event_tx.clone());

        loop {
            terminal_session
                .draw(|frame| self.renderer.render(frame, &mut self.state))
                .context("terminal draw failed")?;
            trace!("redraw");

            let action = self
                .event_rx
                .recv()
                .context("event bus disconnected while waiting for next action")?;
            if self.action_affects_layout(&action) {
                self.renderer.mark_layout_dirty();
            }
            if self.handle_action(action).is_break() {
                break;
            }
            terminal_session
                .sync_cursor_style(self.state.mode)
                .context("sync cursor style failed")?;
        }
        Ok(())
    }

    fn action_affects_layout(&self, action: &AppAction) -> bool {
        matches!(action, AppAction::Editor(_) | AppAction::Layout(_))
    }

    fn handle_action(&mut self, action: AppAction) -> ControlFlow<()> {
        self.apply(action)
    }
}
