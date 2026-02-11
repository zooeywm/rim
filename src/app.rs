use std::io;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::thread;

use crossterm::cursor::SetCursorStyle;
use crossterm::event;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tracing::error;
use tracing::trace;

use crate::action::{AppAction, FileAction, TabAction, WindowAction};
use crate::action_handler::ActionHandler;
use crate::input::InputHandler;
use crate::io_gateway::IoGateway;
use crate::state::{AppState, EditorMode};
use crate::ui::Renderer;

pub struct App {
    state: AppState,
    renderer: Renderer,
    action_handler: ActionHandler,
    io_gateway: IoGateway,
    event_tx: flume::Sender<AppAction>,
    event_rx: flume::Receiver<AppAction>,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let state = AppState::new();

        let (event_tx, event_rx) = flume::bounded(1024);
        let io_gateway = IoGateway::start(event_tx.clone());

        Ok(Self {
            state,
            renderer: Renderer::new(),
            action_handler: ActionHandler,
            io_gateway,
            event_tx,
            event_rx,
        })
    }

    pub fn open_file(&mut self, path: PathBuf) -> io::Result<()> {
        self.event_tx
            .send(AppAction::File(FileAction::OpenRequested { path }))
            .map_err(|err| {
                error!("failed to enqueue OpenRequested action: {}", err);
                io::Error::new(ErrorKind::BrokenPipe, "event bus disconnected")
            })
    }

    pub fn create_untitled_buffer(&mut self) {
        let buffer_id = self.state.create_buffer(None, String::new());
        self.state.bind_buffer_to_active_window(buffer_id);
        self.state.status_bar.message = "new file".to_string();
    }

    pub fn run(mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            SetTitle(self.state.title.as_str())
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        self.sync_cursor_style(&mut terminal)?;
        self.start_input_pump();

        loop {
            terminal.draw(|frame| self.renderer.render(frame, &mut self.state))?;
            trace!("redraw");

            let action = self.event_rx.recv().map_err(|err| {
                error!(
                    "event bus disconnected while waiting for next action: {}",
                    err
                );
                io::Error::new(ErrorKind::BrokenPipe, "event bus disconnected")
            })?;
            if self.action_affects_layout(&action) {
                self.renderer.mark_layout_dirty();
            }
            if self
                .action_handler
                .apply(&mut self.state, &self.io_gateway, action)
                .is_break()
            {
                break;
            }
            self.sync_cursor_style(&mut terminal)?;
        }

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            SetCursorStyle::DefaultUserShape,
            LeaveAlternateScreen
        )?;
        Ok(())
    }

    fn action_affects_layout(&self, action: &AppAction) -> bool {
        match action {
            AppAction::Editor(_) => true,
            AppAction::Layout(_) => true,
            AppAction::Window(WindowAction::CloseActive) => true,
            AppAction::Tab(TabAction::SwitchPrev | TabAction::SwitchNext) => {
                self.state.tabs.len() > 1
            }
            AppAction::Tab(_) => true,
            _ => false,
        }
    }

    fn start_input_pump(&self) {
        let event_tx = self.event_tx.clone();
        let input_handler = InputHandler::new();
        thread::spawn(move || {
            loop {
                let evt = match event::read() {
                    Ok(evt) => evt,
                    Err(err) => {
                        error!("input pump stopped: failed to read terminal event: {}", err);
                        break;
                    }
                };
                let Some(action) = input_handler.action(&evt) else {
                    continue;
                };
                if let Err(err) = event_tx.send(action) {
                    error!(
                        "input pump stopped: failed to send action to event bus: {}",
                        err
                    );
                    break;
                }
            }
        });
    }

    fn sync_cursor_style(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<()> {
        let style = match self.state.mode {
            EditorMode::Insert => SetCursorStyle::SteadyBar,
            EditorMode::Normal
            | EditorMode::Command
            | EditorMode::VisualChar
            | EditorMode::VisualLine => {
                SetCursorStyle::SteadyBlock
            }
        };
        execute!(terminal.backend_mut(), style)?;
        Ok(())
    }
}
