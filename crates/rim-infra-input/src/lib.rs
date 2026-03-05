use std::thread;

use crossterm::{event, event::{Event, KeyCode as CrosstermKeyCode, KeyEvent as CrosstermKeyEvent, KeyModifiers as CrosstermKeyModifiers}};
use rim_kernel::action::{AppAction, EditorAction, KeyCode, KeyEvent, KeyModifiers, LayoutAction};
use tracing::error;

pub struct InputHandler;

impl InputHandler {
	pub fn action(&self, event: &Event) -> Option<AppAction> {
		match event {
			Event::Resize(width, height) => {
				Some(AppAction::Layout(LayoutAction::ViewportResized { width: *width, height: *height }))
			}
			Event::Key(key) => {
				let key = Self::map_key(*key)?;
				Some(AppAction::Editor(EditorAction::KeyPressed(key)))
			}
			_ => None,
		}
	}

	fn map_key(event: CrosstermKeyEvent) -> Option<KeyEvent> {
		let code = match event.code {
			CrosstermKeyCode::Backspace => KeyCode::Backspace,
			CrosstermKeyCode::Enter => KeyCode::Enter,
			CrosstermKeyCode::Left => KeyCode::Left,
			CrosstermKeyCode::Right => KeyCode::Right,
			CrosstermKeyCode::Up => KeyCode::Up,
			CrosstermKeyCode::Down => KeyCode::Down,
			CrosstermKeyCode::Tab => KeyCode::Tab,
			CrosstermKeyCode::Esc => KeyCode::Esc,
			CrosstermKeyCode::Char(ch) => KeyCode::Char(ch),
			_ => return None,
		};
		let mut modifiers = KeyModifiers::NONE;
		if event.modifiers.contains(CrosstermKeyModifiers::SHIFT) {
			modifiers |= KeyModifiers::SHIFT;
		}
		if event.modifiers.contains(CrosstermKeyModifiers::CONTROL) {
			modifiers |= KeyModifiers::CONTROL;
		}
		if event.modifiers.contains(CrosstermKeyModifiers::ALT) {
			modifiers |= KeyModifiers::ALT;
		}

		Some(KeyEvent::new(code, modifiers))
	}
}

pub struct InputPumpService {
	join_handle: Option<std::thread::JoinHandle<()>>,
	event_tx:    flume::Sender<AppAction>,
}

impl InputPumpService {
	pub fn new(event_tx: flume::Sender<AppAction>) -> Self { Self { join_handle: None, event_tx } }

	pub fn start(&mut self) {
		let input_handler = InputHandler;
		let event_tx = self.event_tx.clone();
		let join_handle = thread::spawn(move || {
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
				if event_tx.send(action).is_err() {
					break;
				}
			}
		});
		self.join_handle = Some(join_handle);
	}
}

impl Drop for InputPumpService {
	fn drop(&mut self) {
		if let Some(join_handle) = self.join_handle.take()
			&& join_handle.is_finished()
		{
			let _ = join_handle.join();
		}
	}
}
