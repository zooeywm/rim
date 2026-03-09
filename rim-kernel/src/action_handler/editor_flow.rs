use tracing::error;

use super::{ActionHandlerError, RimState, enqueue_history_save_for_buffer};
use crate::{action::EditorAction, ports::{FileWatcher, StorageIo}};

pub(super) fn apply_editor_action<P>(ports: &P, state: &mut RimState, action: EditorAction)
where P: StorageIo + FileWatcher {
	match action {
		EditorAction::KeyPressed(_) => {}
		EditorAction::EnterInsert => {
			state.begin_insert_history_group();
			state.enter_insert_mode();
		}
		EditorAction::AppendInsert => {
			state.begin_insert_history_group();
			state.move_cursor_right_for_insert();
			state.enter_insert_mode();
		}
		EditorAction::OpenLineBelowInsert => {
			state.begin_insert_history_group();
			state.open_line_below_at_cursor();
			state.enter_insert_mode();
		}
		EditorAction::OpenLineAboveInsert => {
			state.begin_insert_history_group();
			state.open_line_above_at_cursor();
			state.enter_insert_mode();
		}
		EditorAction::EnterCommandMode => state.enter_command_mode(),
		EditorAction::EnterVisualMode => state.enter_visual_mode(),
		EditorAction::EnterVisualLineMode => state.enter_visual_line_mode(),
		EditorAction::EnterVisualBlockMode => state.enter_visual_block_mode(),
		EditorAction::MoveLeft => state.move_cursor_left(),
		EditorAction::MoveLineStart => state.move_cursor_line_start(),
		EditorAction::MoveLineEnd => state.move_cursor_line_end(),
		EditorAction::MoveDown => state.move_cursor_down(),
		EditorAction::MoveUp => state.move_cursor_up(),
		EditorAction::MoveRight => state.move_cursor_right(),
		EditorAction::MoveFileStart => state.move_cursor_file_start(),
		EditorAction::MoveFileEnd => state.move_cursor_file_end(),
		EditorAction::ScrollViewDown => state.scroll_view_down_one_line(),
		EditorAction::ScrollViewUp => state.scroll_view_up_one_line(),
		EditorAction::ScrollViewHalfPageDown => state.scroll_view_down_half_page(),
		EditorAction::ScrollViewHalfPageUp => state.scroll_view_up_half_page(),
		EditorAction::Undo => state.undo_active_buffer_edit(),
		EditorAction::Redo => state.redo_active_buffer_edit(),
		EditorAction::JoinLineBelow => state.join_line_below_at_cursor(),
		EditorAction::CutCharToSlot => state.cut_current_char_to_slot(),
		EditorAction::PasteSlotAfterCursor => state.paste_slot_at_cursor(),
		EditorAction::DeleteCurrentLineToSlot => state.delete_current_line_to_slot(),
		EditorAction::CloseActiveBuffer => {
			let closed_buffer_id = state.active_buffer_id();
			if let Some(buffer_id) = closed_buffer_id {
				enqueue_history_save_for_buffer(ports, state, buffer_id);
			}
			state.close_active_buffer();
			if let Some(buffer_id) = closed_buffer_id
				&& let Err(source) = ports.enqueue_unwatch(buffer_id)
			{
				let err = ActionHandlerError::CloseBufferUnwatch { source };
				error!("watch worker unavailable while enqueueing file unwatch: {}", err);
			}
			if let Some(buffer_id) = closed_buffer_id
				&& let Err(source) = ports.enqueue_close(buffer_id)
			{
				let err = ActionHandlerError::PersistenceSwapClose { source };
				error!("persistence worker unavailable while enqueueing swap close: {}", err);
			}
		}
		EditorAction::NewEmptyBuffer => {
			state.create_untitled_buffer();
		}
	}
}
