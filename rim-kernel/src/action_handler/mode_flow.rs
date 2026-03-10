use std::ops::ControlFlow;

use super::{command_flow, enqueue_history_save_for_buffer, handle_pending_swap_decision_key, post_edit_flow};
use crate::{action::{AppAction, EditorAction, KeyCode, KeyEvent, KeyModifiers}, command::{BindingMatch, CommandRegistry, CommandTarget}, ports::{FilePicker, FileWatcher, StorageIo}, state::{EditorMode, NormalSequenceKey, RimState}};

#[derive(Debug)]
pub(super) enum SequenceMatch {
	Action(AppAction),
	Command(CommandTarget),
	Pending,
	NoMatch,
}

pub(super) fn handle_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	if state.pending_swap_decision.is_some() {
		return handle_pending_swap_decision_key(ports, state, key);
	}

	if !state.is_visual_mode() {
		state.visual_g_pending = false;
	}

	if key.modifiers.contains(KeyModifiers::ALT) {
		state.normal_sequence.clear();
		state.status_bar.key_sequence.clear();
		return ControlFlow::Continue(());
	}

	let mode_before = state.mode;
	let pre_text_snapshot = post_edit_flow::capture_active_buffer_text_snapshot(state);
	let predicted_editor_action =
		if !state.is_command_mode() && !state.is_insert_mode() && !state.is_visual_mode() {
			RimState::predicted_normal_mode_editor_action_for_key(state, key)
		} else {
			None
		};
	let skip_history = matches!(predicted_editor_action, Some(EditorAction::Undo | EditorAction::Redo));

	let flow = if state.is_command_mode() {
		state.normal_sequence.clear();
		state.status_bar.key_sequence.clear();
		command_flow::handle_command_mode_key(ports, state, key)
	} else if state.is_visual_mode() {
		state.normal_sequence.clear();
		state.status_bar.key_sequence.clear();
		handle_visual_mode_key(state, key)
	} else if state.is_insert_mode() {
		state.normal_sequence.clear();
		state.status_bar.key_sequence.clear();
		handle_insert_mode_key(state, key)
	} else {
		handle_normal_mode_key(ports, state, key)
	};

	if let Some(snapshot) = pre_text_snapshot.as_ref() {
		state.record_history_from_text_diff(
			snapshot.buffer_id,
			&snapshot.text,
			snapshot.cursor,
			mode_before,
			skip_history,
		);
	}
	if mode_before == EditorMode::Insert && state.mode != EditorMode::Insert {
		state.commit_insert_history_group();
	}
	post_edit_flow::enqueue_swap_ops_from_text_diff(ports, state, pre_text_snapshot.clone());
	if let Some(snapshot) = pre_text_snapshot
		&& state.buffers.get(snapshot.buffer_id).is_some_and(|buffer| buffer.text != snapshot.text)
	{
		enqueue_history_save_for_buffer(ports, state, snapshot.buffer_id);
	}

	flow
}

pub(super) fn handle_normal_mode_key<P>(ports: &P, state: &mut RimState, key: KeyEvent) -> ControlFlow<()>
where P: StorageIo + FileWatcher + FilePicker {
	let Some(normal_key) = to_normal_key(state, key) else {
		state.normal_sequence.clear();
		state.status_bar.key_sequence.clear();
		return ControlFlow::Continue(());
	};

	state.normal_sequence.push(normal_key);

	loop {
		match resolve_normal_sequence_with_registry(&state.command_registry, &state.normal_sequence) {
			SequenceMatch::Action(action) => {
				state.normal_sequence.clear();
				state.status_bar.key_sequence.clear();
				return RimState::dispatch_internal(ports, state, action);
			}
			SequenceMatch::Command(target) => {
				state.normal_sequence.clear();
				state.status_bar.key_sequence.clear();
				return command_flow::execute_command_target(ports, state, target, None);
			}
			SequenceMatch::Pending => {
				state.status_bar.key_sequence = render_normal_sequence(&state.normal_sequence);
				return ControlFlow::Continue(());
			}
			SequenceMatch::NoMatch => {
				if state.normal_sequence.len() <= 1 {
					state.normal_sequence.clear();
					state.status_bar.key_sequence.clear();
					return ControlFlow::Continue(());
				}
				let last = *state.normal_sequence.last().expect("normal sequence has at least one key");
				state.normal_sequence.clear();
				state.normal_sequence.push(last);
				state.status_bar.key_sequence = render_normal_sequence(&state.normal_sequence);
			}
		}
	}
}

pub(super) fn to_normal_key(state: &RimState, key: KeyEvent) -> Option<NormalSequenceKey> {
	if key.modifiers.contains(KeyModifiers::ALT) {
		return None;
	}

	if key.modifiers.contains(KeyModifiers::CONTROL) {
		if let KeyCode::Char(ch) = key.code {
			return Some(NormalSequenceKey::Ctrl(ch.to_ascii_lowercase()));
		}
		return None;
	}

	if let KeyCode::Char(ch) = key.code {
		if ch == state.leader_key {
			return Some(NormalSequenceKey::Leader);
		}
		let normalized = if key.modifiers.contains(KeyModifiers::SHIFT) && ch.is_ascii_lowercase() {
			ch.to_ascii_uppercase()
		} else {
			ch
		};
		return Some(NormalSequenceKey::Char(normalized));
	}

	if key.code == KeyCode::Tab {
		return Some(NormalSequenceKey::Tab);
	}

	None
}

pub(super) fn resolve_normal_sequence_with_registry(
	registry: &CommandRegistry,
	keys: &[NormalSequenceKey],
) -> SequenceMatch {
	match registry.resolve_normal_sequence(keys) {
		BindingMatch::Pending => SequenceMatch::Pending,
		BindingMatch::NoMatch => SequenceMatch::NoMatch,
		BindingMatch::Exact(CommandTarget::Builtin(builtin)) => {
			if let Some(action) = builtin.normal_mode_action() {
				SequenceMatch::Action(action)
			} else {
				SequenceMatch::Command(CommandTarget::Builtin(builtin))
			}
		}
		BindingMatch::Exact(target) => SequenceMatch::Command(target),
	}
}

pub(super) fn render_normal_sequence(keys: &[NormalSequenceKey]) -> String {
	keys
		.iter()
		.map(|key| match key {
			NormalSequenceKey::Leader => "<leader>".to_string(),
			NormalSequenceKey::Tab => "<tab>".to_string(),
			NormalSequenceKey::Char(ch) => ch.to_string(),
			NormalSequenceKey::Ctrl(ch) => format!("<C-{}>", ch),
		})
		.collect::<Vec<_>>()
		.join("")
}

pub(super) fn handle_insert_mode_key(state: &mut RimState, key: KeyEvent) -> ControlFlow<()> {
	if state.is_block_insert_mode() {
		return handle_block_insert_mode_key(state, key);
	}

	if key.modifiers.contains(KeyModifiers::CONTROL) {
		return ControlFlow::Continue(());
	}

	match key.code {
		KeyCode::Esc => {
			state.exit_insert_mode();
		}
		KeyCode::Enter => {
			state.insert_newline_at_cursor();
		}
		KeyCode::Backspace => {
			state.backspace_at_cursor();
		}
		KeyCode::Left => state.move_cursor_left(),
		KeyCode::Down => state.move_cursor_down(),
		KeyCode::Up => state.move_cursor_up(),
		KeyCode::Right => state.move_cursor_right_for_insert(),
		KeyCode::Tab => state.insert_char_at_cursor('\t'),
		KeyCode::Char(ch) => {
			state.insert_char_at_cursor(ch);
		}
	}

	ControlFlow::Continue(())
}

fn handle_block_insert_mode_key(state: &mut RimState, key: KeyEvent) -> ControlFlow<()> {
	if key.modifiers.contains(KeyModifiers::CONTROL) {
		return ControlFlow::Continue(());
	}

	match key.code {
		KeyCode::Esc => state.exit_insert_mode(),
		KeyCode::Backspace => state.backspace_at_block_cursor(),
		KeyCode::Tab => state.insert_char_at_block_cursor('\t'),
		KeyCode::Char(ch) => state.insert_char_at_block_cursor(ch),
		KeyCode::Enter | KeyCode::Left | KeyCode::Down | KeyCode::Up | KeyCode::Right => {
			state.status_bar.message = "block insert supports text, tab, backspace, esc only".to_string();
		}
	}

	ControlFlow::Continue(())
}

pub(super) fn handle_visual_mode_key(state: &mut RimState, key: KeyEvent) -> ControlFlow<()> {
	if key.modifiers.contains(KeyModifiers::CONTROL) {
		state.visual_g_pending = false;
		match key.code {
			KeyCode::Char('e') => state.scroll_view_down_one_line(),
			KeyCode::Char('y') => state.scroll_view_up_one_line(),
			KeyCode::Char('d') => state.scroll_view_down_half_page(),
			KeyCode::Char('u') => state.scroll_view_up_half_page(),
			KeyCode::Char('v') => state.enter_visual_block_mode(),
			_ => {}
		}
		return ControlFlow::Continue(());
	}

	match key.code {
		KeyCode::Esc => {
			state.visual_g_pending = false;
			state.exit_visual_mode();
		}
		KeyCode::Char('v') => state.enter_visual_mode(),
		KeyCode::Char('V') => state.enter_visual_line_mode(),
		KeyCode::Char('c') => state.change_visual_selection_to_insert_mode(),
		KeyCode::Char('d') => {
			let _ = state.delete_visual_selection_to_slot();
		}
		KeyCode::Char('x') => {
			let _ = state.delete_visual_selection_to_slot();
		}
		KeyCode::Char('y') => state.yank_visual_selection_to_slot(),
		KeyCode::Char('p') => state.replace_visual_selection_with_slot(),
		KeyCode::Char('I') if state.is_visual_block_mode() => {
			state.begin_insert_history_group();
			state.begin_visual_block_insert(false);
		}
		KeyCode::Char('A') if state.is_visual_block_mode() => {
			state.begin_insert_history_group();
			state.begin_visual_block_insert(true);
		}
		KeyCode::Char('h') => {
			if state.is_visual_line_mode() {
				state.move_cursor_left();
			} else {
				state.move_cursor_left_for_visual_char();
			}
		}
		KeyCode::Char('j') => state.move_cursor_down(),
		KeyCode::Char('k') => state.move_cursor_up(),
		KeyCode::Char('l') => {
			if state.is_visual_line_mode() {
				state.move_cursor_right();
			} else {
				state.move_cursor_right_for_visual_char();
			}
		}
		KeyCode::Char('0') => state.move_cursor_line_start(),
		KeyCode::Char('$') => state.move_cursor_line_end(),
		KeyCode::Char('g') => {
			if state.visual_g_pending {
				state.visual_g_pending = false;
				state.move_cursor_file_start();
			} else {
				state.visual_g_pending = true;
			}
			return ControlFlow::Continue(());
		}
		KeyCode::Char('G') => state.move_cursor_file_end(),
		_ => {}
	}
	state.visual_g_pending = false;
	ControlFlow::Continue(())
}
