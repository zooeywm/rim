use std::io;

use crossterm::{cursor::SetCursorStyle, execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode}};
use ratatui::{Terminal, backend::CrosstermBackend};
use thiserror::Error;

use crate::state::EditorMode;

#[derive(Debug, Error)]
pub enum TerminalSessionError {
	#[error("enable raw mode failed")]
	EnableRawMode {
		#[source]
		source: io::Error,
	},
	#[error("enter alternate screen failed")]
	EnterAlternateScreen {
		#[source]
		source: io::Error,
	},
	#[error("create terminal backend failed")]
	CreateTerminal {
		#[source]
		source: io::Error,
	},
	#[error("terminal draw failed")]
	Draw {
		#[source]
		source: io::Error,
	},
	#[error("set cursor style failed")]
	SetCursorStyle {
		#[source]
		source: io::Error,
	},
}

struct TerminalModeGuard;

impl Drop for TerminalModeGuard {
	fn drop(&mut self) {
		let _ = disable_raw_mode();
		let mut stdout = io::stdout();
		let _ = execute!(stdout, SetCursorStyle::DefaultUserShape, LeaveAlternateScreen);
	}
}

pub(crate) struct TerminalSession {
	terminal:    Terminal<CrosstermBackend<io::Stdout>>,
	_mode_guard: TerminalModeGuard,
}

impl TerminalSession {
	pub(crate) fn enter(title: &str) -> Result<Self, TerminalSessionError> {
		enable_raw_mode().map_err(|source| TerminalSessionError::EnableRawMode { source })?;
		let mode_guard = TerminalModeGuard;
		let mut stdout = io::stdout();
		execute!(stdout, EnterAlternateScreen, SetTitle(title))
			.map_err(|source| TerminalSessionError::EnterAlternateScreen { source })?;
		let backend = CrosstermBackend::new(stdout);
		let terminal =
			Terminal::new(backend).map_err(|source| TerminalSessionError::CreateTerminal { source })?;
		Ok(Self { terminal, _mode_guard: mode_guard })
	}

	pub(crate) fn draw(
		&mut self,
		render: impl FnOnce(&mut ratatui::Frame<'_>),
	) -> Result<(), TerminalSessionError> {
		self.terminal.draw(render).map_err(|source| TerminalSessionError::Draw { source })?;
		Ok(())
	}

	pub(crate) fn sync_cursor_style(&mut self, mode: EditorMode) -> Result<(), TerminalSessionError> {
		let style = match mode {
			EditorMode::Insert => SetCursorStyle::SteadyBar,
			EditorMode::Normal | EditorMode::Command | EditorMode::VisualChar | EditorMode::VisualLine => {
				SetCursorStyle::SteadyBlock
			}
		};
		execute!(self.terminal.backend_mut(), style)
			.map_err(|source| TerminalSessionError::SetCursorStyle { source })?;
		Ok(())
	}
}
