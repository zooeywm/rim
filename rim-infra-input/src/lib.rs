use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, thread, time::Duration};

use crossterm::{event, event::{Event, KeyCode as CrosstermKeyCode, KeyEvent as CrosstermKeyEvent, KeyEventKind as CrosstermKeyEventKind, KeyModifiers as CrosstermKeyModifiers}};
use rim_application::action::{AppAction, EditorAction, KeyCode, KeyEvent, KeyModifiers, LayoutAction};
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
		// Ignore key releases so one physical keypress does not get dispatched twice on
		// terminals that emit both Press and Release events (notably Windows consoles).
		match event.kind {
			CrosstermKeyEventKind::Press | CrosstermKeyEventKind::Repeat => {}
			CrosstermKeyEventKind::Release => return None,
		}

		let code = match event.code {
			CrosstermKeyCode::Backspace => KeyCode::Backspace,
			CrosstermKeyCode::Enter => KeyCode::Enter,
			CrosstermKeyCode::Left => KeyCode::Left,
			CrosstermKeyCode::Right => KeyCode::Right,
			CrosstermKeyCode::Up => KeyCode::Up,
			CrosstermKeyCode::Down => KeyCode::Down,
			CrosstermKeyCode::Tab => KeyCode::Tab,
			CrosstermKeyCode::Esc => KeyCode::Esc,
			CrosstermKeyCode::F(1) => KeyCode::F1,
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
	running:     Arc<AtomicBool>,
	event_tx:    flume::Sender<AppAction>,
}

impl InputPumpService {
	pub fn new(event_tx: flume::Sender<AppAction>) -> Self {
		Self { join_handle: None, running: Arc::new(AtomicBool::new(false)), event_tx }
	}

	pub fn start(&mut self) {
		if self.join_handle.is_some() {
			return;
		}
		let input_handler = InputHandler;
		let running = self.running.clone();
		running.store(true, Ordering::SeqCst);
		let event_tx = self.event_tx.clone();
		let join_handle = thread::spawn(move || {
			while running.load(Ordering::SeqCst) {
				let poll_ready = match event::poll(Duration::from_millis(50)) {
					Ok(ready) => ready,
					Err(err) => {
						error!("input pump stopped: failed to poll terminal event: {}", err);
						break;
					}
				};
				if !poll_ready {
					continue;
				}
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

	pub fn stop(&mut self) {
		self.running.store(false, Ordering::SeqCst);
		if let Some(join_handle) = self.join_handle.take() {
			let _ = join_handle.join();
		}
	}
}

impl Drop for InputPumpService {
	fn drop(&mut self) { self.stop(); }
}

#[cfg(test)]
mod tests {
	use crossterm::event::{Event, KeyCode as CrosstermKeyCode, KeyEvent as CrosstermKeyEvent, KeyEventKind as CrosstermKeyEventKind, KeyEventState as CrosstermKeyEventState, KeyModifiers as CrosstermKeyModifiers};
	use rim_application::action::{AppAction, EditorAction, KeyCode, KeyModifiers, LayoutAction};

	use super::InputHandler;

	fn make_key_event(kind: CrosstermKeyEventKind) -> CrosstermKeyEvent {
		CrosstermKeyEvent {
			code: CrosstermKeyCode::Char('a'),
			modifiers: CrosstermKeyModifiers::NONE,
			kind,
			state: CrosstermKeyEventState::NONE,
		}
	}

	#[test]
	fn should_map_key_press_event() {
		let input_handler = InputHandler;

		let action = input_handler.action(&Event::Key(make_key_event(CrosstermKeyEventKind::Press)));

		match action {
			Some(AppAction::Editor(EditorAction::KeyPressed(key))) => {
				assert!(matches!(key.code, KeyCode::Char('a')));
				assert_eq!(key.modifiers, KeyModifiers::NONE);
			}
			_ => panic!("expected mapped key press action"),
		}
	}

	#[test]
	fn should_ignore_key_release_event() {
		let input_handler = InputHandler;

		let action = input_handler.action(&Event::Key(make_key_event(CrosstermKeyEventKind::Release)));

		assert!(action.is_none());
	}

	#[test]
	fn should_map_resize_event() {
		let input_handler = InputHandler;

		let action = input_handler.action(&Event::Resize(120, 40));

		match action {
			Some(AppAction::Layout(LayoutAction::ViewportResized { width, height })) => {
				assert_eq!(width, 120);
				assert_eq!(height, 40);
			}
			_ => panic!("expected resize action"),
		}
	}

	#[test]
	fn should_map_f1_key_event() {
		let input_handler = InputHandler;
		let action = input_handler.action(&Event::Key(CrosstermKeyEvent {
			code:      CrosstermKeyCode::F(1),
			modifiers: CrosstermKeyModifiers::NONE,
			kind:      CrosstermKeyEventKind::Press,
			state:     CrosstermKeyEventState::NONE,
		}));

		match action {
			Some(AppAction::Editor(EditorAction::KeyPressed(key))) => {
				assert_eq!(key.code, KeyCode::F1);
				assert_eq!(key.modifiers, KeyModifiers::NONE);
			}
			_ => panic!("expected mapped F1 action"),
		}
	}
}
