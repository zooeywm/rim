use tracing::{error, trace};

use super::{AppState, FocusDirection, SplitAxis, WindowId, WindowState};

impl AppState {
	pub fn focus_window(&mut self, direction: FocusDirection) {
		let tab = self.tabs.get_mut(&self.active_tab).expect("invariant: active tab must exist");
		let active_id = tab.active_window;
		let active = self.windows.get(active_id).expect("invariant: active window id must exist in windows");
		let active_left = i32::from(active.x);
		let active_right = i32::from(active.x.saturating_add(active.width));
		let active_top = i32::from(active.y);
		let active_bottom = i32::from(active.y.saturating_add(active.height));
		let active_cx = active_left + (active_right - active_left) / 2;
		let active_cy = active_top + (active_bottom - active_top) / 2;

		let best = tab
			.windows
			.iter()
			.copied()
			.filter(|id| *id != active_id)
			.filter_map(|id| self.windows.get(id).map(|w| (id, w)))
			.filter_map(|(id, w)| {
				let left = i32::from(w.x);
				let right = i32::from(w.x.saturating_add(w.width));
				let top = i32::from(w.y);
				let bottom = i32::from(w.y.saturating_add(w.height));
				let cx = left + (right - left) / 2;
				let cy = top + (bottom - top) / 2;

				let score = match direction {
					FocusDirection::Left if right <= active_left => Some((active_left - right, (cy - active_cy).abs())),
					FocusDirection::Right if left >= active_right => {
						Some((left - active_right, (cy - active_cy).abs()))
					}
					FocusDirection::Up if bottom <= active_top => Some((active_top - bottom, (cx - active_cx).abs())),
					FocusDirection::Down if top >= active_bottom => Some((top - active_bottom, (cx - active_cx).abs())),
					_ => None,
				}?;

				Some((id, score))
			})
			.min_by_key(|(_, score)| *score)
			.map(|(id, _)| id);

		if let Some(target) = best {
			tab.active_window = target;
		}
	}

	pub fn close_active_window(&mut self) {
		let tab_snapshot = self.tabs.get(&self.active_tab).expect("invariant: active tab must exist");
		let active_window = tab_snapshot.active_window;
		if tab_snapshot.windows.len() <= 1 {
			return;
		}
		let current_idx = tab_snapshot.windows.iter().position(|id| *id == active_window).unwrap_or(0);
		let remaining_windows =
			tab_snapshot.windows.iter().copied().filter(|id| *id != active_window).collect::<Vec<_>>();
		let closed_layout =
			self.windows.get(active_window).copied().expect("invariant: active window id must exist in windows");
		let absorbed_target = self.absorb_closed_window(&remaining_windows, &closed_layout);
		let absorbed_group =
			absorbed_target.is_none() && self.absorb_closed_window_by_group(&remaining_windows, &closed_layout);

		let _ = self.windows.remove(active_window);
		let tab = self.tabs.get_mut(&self.active_tab).expect("invariant: active tab must exist");
		tab.windows.retain(|id| *id != active_window);
		tab.active_window = if let Some(id) = absorbed_target {
			id
		} else if absorbed_group {
			*tab.windows.first().expect("tab must keep at least one window")
		} else {
			let next_idx = current_idx.min(tab.windows.len().saturating_sub(1));
			*tab.windows.get(next_idx).expect("tab must keep at least one window")
		};
		self.status_bar.message = "window closed".to_string();
	}

	pub fn split_active_window(&mut self, axis: SplitAxis) {
		let tab_id = self.active_tab;
		let active_window_id =
			self.tabs.get(&tab_id).map(|t| t.active_window).expect("invariant: active tab must exist");
		let active_window =
			*self.windows.get(active_window_id).expect("invariant: active window id must exist in windows");
		let (updated_active, new_window_layout) = split_window_layout(&active_window, axis);

		let Some(new_window_id) = self.create_window(active_window.buffer_id) else {
			error!(
				"split_active_window failed: unable to create new window for buffer {:?}",
				active_window.buffer_id
			);
			return;
		};
		if let Some(window) = self.windows.get_mut(active_window_id) {
			*window = updated_active;
		}
		if let Some(window) = self.windows.get_mut(new_window_id) {
			*window = new_window_layout;
		}

		let tab = self.tabs.get_mut(&tab_id).expect("invariant: active tab must exist");
		tab.windows.push(new_window_id);
		tab.active_window = new_window_id;
		self.status_bar.message = match axis {
			SplitAxis::Horizontal => "split horizontal".to_string(),
			SplitAxis::Vertical => "split vertical".to_string(),
		};
	}

	pub fn update_active_tab_layout(&mut self, width: u16, height: u16) {
		trace!("update_active_tab_layout");
		let tab = self.tabs.get(&self.active_tab).expect("invariant: active tab must exist");
		let window_ids = tab.windows.clone();

		if window_ids.is_empty() {
			return;
		}

		let max_right = window_ids
			.iter()
			.filter_map(|id| self.windows.get(*id))
			.map(|w| w.x.saturating_add(w.width))
			.max()
			.unwrap_or(0);
		let max_bottom = window_ids
			.iter()
			.filter_map(|id| self.windows.get(*id))
			.map(|w| w.y.saturating_add(w.height))
			.max()
			.unwrap_or(0);

		if max_right == 0 || max_bottom == 0 {
			if let Some(first_id) = window_ids.first().copied() {
				if let Some(window) = self.windows.get_mut(first_id) {
					window.x = 0;
					window.y = 0;
					window.width = width.max(1);
					window.height = height.max(1);
				}
				for id in window_ids.iter().skip(1) {
					if let Some(window) = self.windows.get_mut(*id) {
						window.x = 0;
						window.y = 0;
						window.width = width.max(1);
						window.height = height.max(1);
					}
				}
			}
			return;
		}

		for id in &window_ids {
			if let Some(window) = self.windows.get_mut(*id) {
				let old_right = window.x.saturating_add(window.width);
				let old_bottom = window.y.saturating_add(window.height);
				let new_x = (u32::from(window.x) * u32::from(width) / u32::from(max_right)) as u16;
				let new_y = (u32::from(window.y) * u32::from(height) / u32::from(max_bottom)) as u16;
				let new_right = (u32::from(old_right) * u32::from(width) / u32::from(max_right)) as u16;
				let new_bottom = (u32::from(old_bottom) * u32::from(height) / u32::from(max_bottom)) as u16;
				window.x = new_x.min(width.saturating_sub(1));
				window.y = new_y.min(height.saturating_sub(1));
				window.width = new_right.saturating_sub(new_x).max(1).min(width.saturating_sub(window.x).max(1));
				window.height = new_bottom.saturating_sub(new_y).max(1).min(height.saturating_sub(window.y).max(1));
			}
		}
	}

	fn absorb_closed_window(&mut self, candidates: &[WindowId], closed: &WindowState) -> Option<WindowId> {
		for id in candidates {
			let Some(w) = self.windows.get(*id).cloned() else {
				continue;
			};
			if w.y == closed.y && w.height == closed.height && w.x.saturating_add(w.width) == closed.x {
				if let Some(target) = self.windows.get_mut(*id) {
					target.width = target.width.saturating_add(closed.width);
				}
				return Some(*id);
			}
			if w.y == closed.y && w.height == closed.height && closed.x.saturating_add(closed.width) == w.x {
				if let Some(target) = self.windows.get_mut(*id) {
					target.x = closed.x;
					target.width = target.width.saturating_add(closed.width);
				}
				return Some(*id);
			}
			if w.x == closed.x && w.width == closed.width && w.y.saturating_add(w.height) == closed.y {
				if let Some(target) = self.windows.get_mut(*id) {
					target.height = target.height.saturating_add(closed.height);
				}
				return Some(*id);
			}
			if w.x == closed.x && w.width == closed.width && closed.y.saturating_add(closed.height) == w.y {
				if let Some(target) = self.windows.get_mut(*id) {
					target.y = closed.y;
					target.height = target.height.saturating_add(closed.height);
				}
				return Some(*id);
			}
		}
		None
	}

	fn absorb_closed_window_by_group(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
		self.absorb_group_from_right(candidates, closed)
			|| self.absorb_group_from_left(candidates, closed)
			|| self.absorb_group_from_bottom(candidates, closed)
			|| self.absorb_group_from_top(candidates, closed)
	}

	fn absorb_group_from_right(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
		let mut group = candidates
			.iter()
			.copied()
			.filter_map(|id| self.windows.get(id).map(|w| (id, *w)))
			.filter(|(_, w)| {
				closed.x.saturating_add(closed.width) == w.x
					&& w.width > 0
					&& overlap_len(w.y, w.height, closed.y, closed.height) > 0
			})
			.collect::<Vec<_>>();
		if group.is_empty() {
			return false;
		}
		group.sort_by_key(|(_, w)| w.y);
		if !is_full_vertical_cover(&group, closed) {
			return false;
		}
		for (id, _) in group {
			if let Some(target) = self.windows.get_mut(id) {
				target.x = closed.x;
				target.width = target.width.saturating_add(closed.width);
			}
		}
		true
	}

	fn absorb_group_from_left(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
		let mut group = candidates
			.iter()
			.copied()
			.filter_map(|id| self.windows.get(id).map(|w| (id, *w)))
			.filter(|(_, w)| {
				w.x.saturating_add(w.width) == closed.x
					&& w.width > 0
					&& overlap_len(w.y, w.height, closed.y, closed.height) > 0
			})
			.collect::<Vec<_>>();
		if group.is_empty() {
			return false;
		}
		group.sort_by_key(|(_, w)| w.y);
		if !is_full_vertical_cover(&group, closed) {
			return false;
		}
		for (id, _) in group {
			if let Some(target) = self.windows.get_mut(id) {
				target.width = target.width.saturating_add(closed.width);
			}
		}
		true
	}

	fn absorb_group_from_bottom(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
		let mut group = candidates
			.iter()
			.copied()
			.filter_map(|id| self.windows.get(id).map(|w| (id, *w)))
			.filter(|(_, w)| {
				closed.y.saturating_add(closed.height) == w.y
					&& w.height > 0
					&& overlap_len(w.x, w.width, closed.x, closed.width) > 0
			})
			.collect::<Vec<_>>();
		if group.is_empty() {
			return false;
		}
		group.sort_by_key(|(_, w)| w.x);
		if !is_full_horizontal_cover(&group, closed) {
			return false;
		}
		for (id, _) in group {
			if let Some(target) = self.windows.get_mut(id) {
				target.y = closed.y;
				target.height = target.height.saturating_add(closed.height);
			}
		}
		true
	}

	fn absorb_group_from_top(&mut self, candidates: &[WindowId], closed: &WindowState) -> bool {
		let mut group = candidates
			.iter()
			.copied()
			.filter_map(|id| self.windows.get(id).map(|w| (id, *w)))
			.filter(|(_, w)| {
				w.y.saturating_add(w.height) == closed.y
					&& w.height > 0
					&& overlap_len(w.x, w.width, closed.x, closed.width) > 0
			})
			.collect::<Vec<_>>();
		if group.is_empty() {
			return false;
		}
		group.sort_by_key(|(_, w)| w.x);
		if !is_full_horizontal_cover(&group, closed) {
			return false;
		}
		for (id, _) in group {
			if let Some(target) = self.windows.get_mut(id) {
				target.height = target.height.saturating_add(closed.height);
			}
		}
		true
	}
}

fn overlap_len(a_start: u16, a_len: u16, b_start: u16, b_len: u16) -> u16 {
	let a_end = a_start.saturating_add(a_len);
	let b_end = b_start.saturating_add(b_len);
	let start = a_start.max(b_start);
	let end = a_end.min(b_end);
	end.saturating_sub(start)
}

fn is_full_vertical_cover(group: &[(WindowId, WindowState)], closed: &WindowState) -> bool {
	let mut cursor = closed.y;
	let end = closed.y.saturating_add(closed.height);
	for (_, w) in group {
		if w.y > cursor {
			return false;
		}
		let w_end = w.y.saturating_add(w.height);
		if w_end > cursor {
			cursor = w_end;
		}
		if cursor >= end {
			return true;
		}
	}
	cursor >= end
}

fn is_full_horizontal_cover(group: &[(WindowId, WindowState)], closed: &WindowState) -> bool {
	let mut cursor = closed.x;
	let end = closed.x.saturating_add(closed.width);
	for (_, w) in group {
		if w.x > cursor {
			return false;
		}
		let w_end = w.x.saturating_add(w.width);
		if w_end > cursor {
			cursor = w_end;
		}
		if cursor >= end {
			return true;
		}
	}
	cursor >= end
}

fn split_window_layout(window: &WindowState, axis: SplitAxis) -> (WindowState, WindowState) {
	let mut first = *window;
	let mut second = *window;
	let base_width = window.width.max(1);
	let base_height = window.height.max(1);
	match axis {
		SplitAxis::Horizontal => {
			let left_w = (base_width / 2).max(1);
			let right_w = base_width.saturating_sub(left_w).max(1);
			first.width = left_w;
			first.height = base_height;
			second.x = window.x.saturating_add(left_w);
			second.width = right_w;
			second.height = base_height;
		}
		SplitAxis::Vertical => {
			let top_h = (base_height / 2).max(1);
			let bottom_h = base_height.saturating_sub(top_h).max(1);
			first.width = base_width;
			first.height = top_h;
			second.width = base_width;
			second.y = window.y.saturating_add(top_h);
			second.height = bottom_h;
		}
	}
	(first, second)
}
