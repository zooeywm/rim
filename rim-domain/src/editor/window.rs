use crate::{editor::EditorState, model::FocusDirection};

impl EditorState {
	pub fn focus_window(&mut self, direction: FocusDirection) {
		let active_tab = self.active_tab;
		let (active_id, window_ids) = {
			let tab = self.tabs.get(&active_tab).expect("invariant: active tab must exist");
			(tab.active_window, tab.windows.clone())
		};
		let active = self.windows.get(active_id).expect("invariant: active window id must exist in windows");
		let active_left = i32::from(active.x);
		let active_right = i32::from(active.x.saturating_add(active.width));
		let active_top = i32::from(active.y);
		let active_bottom = i32::from(active.y.saturating_add(active.height));
		let active_cx = active_left + (active_right - active_left) / 2;
		let active_cy = active_top + (active_bottom - active_top) / 2;

		let best = window_ids
			.into_iter()
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

		if let Some(target) = best
			&& let Some(tab) = self.tabs.get_mut(&active_tab)
		{
			tab.active_window = target;
		}
	}
}
