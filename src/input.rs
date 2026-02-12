use std::thread;

use crossterm::event;
use crossterm::event::Event;
use tracing::error;

use crate::action::{AppAction, EditorAction, LayoutAction};

pub struct InputHandler;

impl InputHandler {
    pub fn action(&self, event: &Event) -> Option<AppAction> {
        match event {
            Event::Resize(width, height) => {
                Some(AppAction::Layout(LayoutAction::ViewportResized {
                    width: *width,
                    height: *height,
                }))
            }
            Event::Key(key) => Some(AppAction::Editor(EditorAction::KeyPressed(*key))),
            _ => None,
        }
    }
}

pub struct InputPumpService {
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl InputPumpService {
    pub fn new() -> Self {
        Self { join_handle: None }
    }

    pub fn start(&mut self, event_tx: flume::Sender<AppAction>) {
        let input_handler = InputHandler;
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

impl Default for InputPumpService {
    fn default() -> Self {
        Self::new()
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
