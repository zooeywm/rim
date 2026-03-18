use std::time::{Duration, Instant};

use crate::state::{NotificationLevel, RimState};

#[test]
fn notification_preview_should_cap_at_five_and_queue_remaining_as_unread() {
	let mut state = RimState::new();
	for index in 0..7 {
		state.push_notification(NotificationLevel::Error, format!("error {}", index));
	}

	let preview = state.notification_preview().expect("preview should open");
	assert_eq!(preview.items.len(), 5);
	assert_eq!(preview.unread_total, 7);
}

#[test]
fn notification_preview_should_drain_queue_then_auto_close_after_expire() {
	let mut state = RimState::new();
	for index in 0..7 {
		state.push_notification(NotificationLevel::Warn, format!("warn {}", index));
	}

	let base = Instant::now();
	let _ = state.tick_notifications(base + Duration::from_secs(4));
	let preview = state.notification_preview().expect("preview should still show queued entries");
	assert_eq!(preview.items.len(), 2);
	assert_eq!(preview.unread_total, 7);

	let _ = state.tick_notifications(base + Duration::from_secs(8));
	assert!(state.notification_preview().is_none());
}

#[test]
fn notification_center_should_delete_selected_item() {
	let mut state = RimState::new();
	state.push_notification(NotificationLevel::Info, "info");
	state.push_notification(NotificationLevel::Error, "error");
	state.open_notification_center();

	assert!(state.delete_selected_notification());
	let view = state.notification_center_view().expect("center should stay open");
	assert_eq!(view.items.len(), 1);
}

#[test]
fn notification_should_mark_read_only_when_viewed_in_center() {
	let mut state = RimState::new();
	state.push_notification(NotificationLevel::Info, "n1");
	state.push_notification(NotificationLevel::Warn, "n2");
	assert_eq!(state.unread_notification_count(), 2);

	state.open_notification_center();
	assert_eq!(state.unread_notification_count(), 1);
	let _ = state.move_notification_center_selection(1);
	assert_eq!(state.unread_notification_count(), 0);
}
