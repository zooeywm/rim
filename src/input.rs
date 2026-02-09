use crossterm::event::Event;

use crate::action::{AppAction, EditorAction, LayoutAction};

pub struct InputHandler;

impl InputHandler {
    pub fn new() -> Self {
        Self
    }

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
