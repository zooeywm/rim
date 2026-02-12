use std::collections::BTreeMap;

use super::{AppState, TabId, TabState, WindowId};

impl AppState {
    pub fn open_new_tab(&mut self) -> TabId {
        let tab_id = self.insert_tab_after_active();
        self.switch_tab(tab_id);
        self.status_bar.message = "new tab".to_string();
        tab_id
    }

    pub fn remove_tab(&mut self, tab_id: TabId) {
        if self.tabs.len() <= 1 {
            return;
        }
        if !self.tabs.contains_key(&tab_id) {
            return;
        }

        if self.active_tab == tab_id {
            let next_active = self
                .tabs
                .keys()
                .copied()
                .filter(|id| *id != tab_id && id.0 < tab_id.0)
                .max_by_key(|id| id.0)
                .or_else(|| {
                    self.tabs
                        .keys()
                        .copied()
                        .filter(|id| *id != tab_id && id.0 > tab_id.0)
                        .min_by_key(|id| id.0)
                })
                .expect("invariant: there must be another tab when removing active tab");
            self.active_tab = next_active;
        }

        self.tabs.remove(&tab_id);
        self.compact_tab_ids_after(tab_id);
    }

    pub fn switch_tab(&mut self, tab_id: TabId) {
        if self.tabs.contains_key(&tab_id) {
            self.active_tab = tab_id;
        }
    }

    pub fn active_tab_window_ids(&self) -> Vec<WindowId> {
        self.tabs
            .get(&self.active_tab)
            .map(|tab| tab.windows.clone())
            .unwrap_or_default()
    }

    pub fn active_window_id(&self) -> WindowId {
        self.tabs
            .get(&self.active_tab)
            .map(|tab| tab.active_window)
            .expect("active tab must exist")
    }

    pub fn close_current_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let current_tab = self.active_tab;
        self.remove_tab(current_tab);
        self.status_bar.message = "tab closed".to_string();
    }

    pub fn switch_to_prev_tab(&mut self) {
        let current_tab = self.active_tab;
        if let Some(prev_tab) = self
            .tabs
            .keys()
            .copied()
            .filter(|id| id.0 < current_tab.0)
            .max_by_key(|id| id.0)
        {
            self.switch_tab(prev_tab);
        }
    }

    pub fn switch_to_next_tab(&mut self) {
        let current_tab = self.active_tab;
        if let Some(next_tab) = self
            .tabs
            .keys()
            .copied()
            .filter(|id| id.0 > current_tab.0)
            .min_by_key(|id| id.0)
        {
            self.switch_tab(next_tab);
        }
    }

    fn insert_tab_after_active(&mut self) -> TabId {
        let current = self.active_tab.0;
        let new_id = TabId(current.saturating_add(1));
        let buffer_id = self.create_buffer(None, String::new());
        let window_id = self
            .create_window(Some(buffer_id))
            .expect("create default tab window should never fail");
        let old_tabs = std::mem::take(&mut self.tabs);
        let mut rebuilt_tabs = BTreeMap::new();

        for (id, tab) in old_tabs {
            let target_id = if id.0 > current {
                TabId(id.0.saturating_add(1))
            } else {
                id
            };
            rebuilt_tabs.insert(target_id, tab);
        }

        rebuilt_tabs.insert(
            new_id,
            TabState {
                windows: vec![window_id],
                active_window: window_id,
            },
        );
        self.tabs = rebuilt_tabs;
        new_id
    }

    fn compact_tab_ids_after(&mut self, removed: TabId) {
        let old_tabs = std::mem::take(&mut self.tabs);
        let mut rebuilt_tabs = BTreeMap::new();
        for (id, tab) in old_tabs {
            let target_id = if id.0 > removed.0 {
                TabId(id.0.saturating_sub(1))
            } else {
                id
            };
            rebuilt_tabs.insert(target_id, tab);
        }
        if self.active_tab.0 > removed.0 {
            self.active_tab = TabId(self.active_tab.0.saturating_sub(1));
        }
        self.tabs = rebuilt_tabs;
    }
}
