use super::common::test_state;
use crate::state::TabId;

#[test]
fn switch_and_remove_tab_flow() {
    let mut state = test_state();
    let tab2 = state.open_new_tab();

    state.switch_tab(tab2);
    assert_eq!(state.active_tab, tab2);

    state.remove_tab(TabId(1));
}

#[test]
fn remove_active_tab_should_switch_to_another_when_not_last() {
    let mut state = test_state();
    let tab2 = state.open_new_tab();
    state.switch_tab(tab2);
    state.remove_tab(state.active_tab);
    assert_eq!(state.active_tab, TabId(1));
    assert_eq!(state.tabs.len(), 1);
}

#[test]
fn open_new_tab_should_insert_after_active_tab() {
    let mut state = test_state();
    let old_tab2 = state.open_new_tab();
    let old_tab3 = state.open_new_tab();
    let old_tab4 = state.open_new_tab();
    assert_eq!(old_tab2.0, 2);
    assert_eq!(old_tab3.0, 3);
    assert_eq!(old_tab4.0, 4);
    state.switch_tab(TabId(1));

    let created = state.open_new_tab();
    assert_eq!(created.0, 2);
    assert_eq!(state.active_tab, TabId(2));
    assert!(state.tabs.contains_key(&TabId(3)));
    assert!(state.tabs.contains_key(&TabId(4)));
    assert!(state.tabs.contains_key(&TabId(5)));
}

#[test]
fn open_new_tab_should_create_default_window_with_untitled_buffer() {
    let mut state = test_state();
    let tab_id = state.open_new_tab();
    let tab = state.tabs.get(&tab_id).expect("new tab should exist");
    assert_eq!(tab.windows.len(), 1);
    let window_id = tab.windows[0];
    let window = state.windows.get(window_id).expect("window should exist");
    let buffer_id = window
        .buffer_id
        .expect("new tab window should bind a buffer");
    let buffer = state.buffers.get(buffer_id).expect("buffer should exist");
    assert_eq!(buffer.name, "untitled");
    assert_eq!(buffer.path, None);
    assert_eq!(buffer.text, "");
    assert_eq!(tab.active_window, window_id);
}

#[test]
fn switch_prev_next_tab_should_change_active_tab_by_one_step() {
    let mut state = test_state();
    let tab2 = state.open_new_tab();
    let tab3 = state.open_new_tab();
    state.switch_tab(tab2);

    state.switch_to_next_tab();
    assert_eq!(state.active_tab, tab3);

    state.switch_to_prev_tab();
    assert_eq!(state.active_tab, tab2);
}

#[test]
fn switch_prev_next_tab_should_noop_at_edges() {
    let mut state = test_state();
    let tab2 = state.open_new_tab();
    state.switch_tab(TabId(1));

    state.switch_to_prev_tab();
    assert_eq!(state.active_tab, TabId(1));

    state.switch_tab(tab2);
    state.switch_to_next_tab();
    assert!(state.tabs.contains_key(&tab2));
    assert_eq!(state.active_tab, tab2);
}

#[test]
fn close_current_tab_should_switch_to_another_tab() {
    let mut state = test_state();
    let tab1 = state.active_tab;
    let tab2 = state.open_new_tab();
    state.switch_tab(tab2);
    state.close_current_tab();

    assert_eq!(state.active_tab, tab1);
    assert!(!state.tabs.contains_key(&tab2));
}

#[test]
fn close_current_tab_should_prefer_lower_tab_id() {
    let mut state = test_state();
    let tab2 = state.open_new_tab();
    let tab3 = state.open_new_tab();
    state.switch_tab(tab3);
    state.close_current_tab();

    assert_eq!(state.active_tab, tab2);
    assert!(!state.tabs.contains_key(&tab3));
}

#[test]
fn close_middle_tab_should_compact_following_tab_ids() {
    let mut state = test_state();
    let tab2 = state.open_new_tab();
    let tab3 = state.open_new_tab();

    state.switch_tab(tab2);
    state.close_current_tab();

    assert!(!state.tabs.contains_key(&tab3));
    assert!(state.tabs.contains_key(&TabId(2)));
    assert_eq!(state.tabs.len(), 2);
}

#[test]
fn close_current_tab_should_noop_when_only_one_tab() {
    let mut state = test_state();
    let active_before = state.active_tab;
    let tab_count_before = state.tabs.len();
    state.close_current_tab();

    assert_eq!(state.active_tab, active_before);
    assert_eq!(state.tabs.len(), tab_count_before);
}

#[test]
fn open_new_tab_should_reuse_deleted_tab_id() {
    let mut state = test_state();
    let tab2 = state.open_new_tab();
    state.switch_tab(tab2);
    state.close_current_tab();

    let recreated = state.open_new_tab();
    assert_eq!(recreated, tab2);
}
