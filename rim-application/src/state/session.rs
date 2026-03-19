use super::{RimState, StatusBarState, WorkspaceSessionSnapshot};

impl RimState {
	pub fn workspace_session_snapshot(&self) -> WorkspaceSessionSnapshot {
		self.editor.workspace_session_snapshot(self.workbench.force_quit_trim_file_dirty_in_session)
	}

	pub fn restore_workspace_session(&mut self, snapshot: WorkspaceSessionSnapshot) -> bool {
		self.workbench.command_line.clear();
		self.workbench.quit_after_save = false;
		self.workbench.force_quit_trim_file_dirty_in_session = false;
		self.workbench.pending_save_path = None;
		self.workbench.normal_sequence.clear();
		self.workbench.visual_g_pending = false;
		self.workbench.pending_swap_decision = None;
		self.workbench.in_flight_internal_saves.clear();
		self.workbench.ignore_external_change_until.clear();
		self.workbench.status_bar = StatusBarState::default();
		if !self.editor.restore_workspace_session(snapshot) {
			return false;
		}
		self.workbench.status_bar.message = "session restored".to_string();
		true
	}
}
