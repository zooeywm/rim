use std::path::PathBuf;

use crate::state::RimState;

pub(super) fn test_state() -> RimState {
	let mut state = RimState::new();
	let buffer_id = state.create_buffer(Some(PathBuf::from("test.rs")), "fn main() {}");
	state.bind_buffer_to_active_window(buffer_id);
	state
}

pub(super) fn set_active_buffer_text(state: &mut RimState, text: &str) {
	let buffer_id = state.active_buffer_id().expect("buffer id exists");
	let buffer = state.buffers.get_mut(buffer_id).expect("active buffer exists");
	buffer.text = text.to_string().into();
	buffer.clean_text = buffer.text.clone();
	buffer.dirty = false;
	buffer.externally_modified = false;
}
