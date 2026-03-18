use super::{RimState, TabId, WindowId};

impl RimState {
	pub fn open_new_tab(&mut self) -> TabId {
		let tab_id = self.editor.insert_tab_after_active();
		self.editor.switch_tab(tab_id);
		self.workbench.status_bar.message = "new tab".to_string();
		tab_id
	}

	pub fn remove_tab(&mut self, tab_id: TabId) { self.editor.remove_tab(tab_id); }

	pub fn switch_tab(&mut self, tab_id: TabId) { self.editor.switch_tab(tab_id); }

	pub fn active_tab_window_ids(&self) -> Vec<WindowId> { self.editor.active_tab_window_ids() }

	pub fn active_tab_buffer_ids(&self) -> Vec<super::BufferId> { self.editor.active_tab_buffer_ids() }

	pub fn active_window_id(&self) -> WindowId { self.editor.active_window_id() }

	pub fn close_current_tab(&mut self) {
		if self.tabs.len() <= 1 {
			return;
		}
		let current_tab = self.active_tab;
		self.editor.remove_tab(current_tab);
		self.workbench.status_bar.message = "tab closed".to_string();
	}

	pub fn switch_to_prev_tab(&mut self) { self.editor.switch_to_prev_tab(); }

	pub fn switch_to_next_tab(&mut self) { self.editor.switch_to_next_tab(); }
}
