use unicode_width::UnicodeWidthChar;

use super::{AppState, BufferState, CursorState};

impl AppState {
    pub fn move_cursor_left(&mut self) {
        tracing::info!("move left");
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.col > 1
        {
            cursor.col = cursor.col.saturating_sub(1);
        }
        self.preferred_col = None;
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
    }

    pub fn move_cursor_right(&mut self) {
        tracing::info!("move right");
        let row = self.active_cursor().row;
        let max_col = self.max_navigable_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.col < max_col
        {
            cursor.col = cursor.col.saturating_add(1);
        }
        self.preferred_col = None;
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
    }

    pub fn move_cursor_left_for_visual_char(&mut self) {
        if let Some(cursor_mut) = self.active_buffer_cursor_mut()
            && cursor_mut.col > 1
        {
            cursor_mut.col = cursor_mut.col.saturating_sub(1);
        }
        self.preferred_col = None;
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
    }

    pub fn move_cursor_right_for_visual_char(&mut self) {
        let row = self.active_cursor().row;
        let max_col = self.max_visual_char_col_for_row(row);
        if let Some(cursor_mut) = self.active_buffer_cursor_mut()
            && cursor_mut.col < max_col
        {
            cursor_mut.col = cursor_mut.col.saturating_add(1);
        }
        self.preferred_col = None;
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
    }

    pub fn move_cursor_line_start(&mut self) {
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = 1;
        }
        self.preferred_col = None;
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Left);
    }

    pub fn move_cursor_line_end(&mut self) {
        let row = self.active_cursor().row;
        let max_col = self.max_navigable_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = max_col;
        }
        self.preferred_col = None;
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
    }

    pub fn move_cursor_up(&mut self) {
        tracing::info!("move up");
        let target_col = self.capture_preferred_col_for_vertical();
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.row > 1
        {
            cursor.row = cursor.row.saturating_sub(1);
        }
        let row = self.active_cursor().row;
        let max_col = self.max_navigable_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = target_col.min(max_col).max(1);
        }
        self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Up);
    }

    pub fn move_cursor_down(&mut self) {
        tracing::info!("move down");
        let target_col = self.capture_preferred_col_for_vertical();
        let max_row = self.max_row();
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.row < max_row
        {
            cursor.row = cursor.row.saturating_add(1);
        }
        let row = self.active_cursor().row;
        let max_col = self.max_navigable_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = target_col.min(max_col).max(1);
        }
        self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Down);
    }

    pub fn move_cursor_file_start(&mut self) {
        let target_col = self.capture_preferred_col_for_vertical();
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.row = 1;
        }
        let max_col = self.max_navigable_col_for_row(1);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = target_col.min(max_col).max(1);
        }
        self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Up);
    }

    pub fn move_cursor_file_end(&mut self) {
        let target_col = self.capture_preferred_col_for_vertical();
        let max_row = self.max_row();
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.row = max_row;
        }
        let max_col = self.max_navigable_col_for_row(max_row);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = target_col.min(max_col).max(1);
        }
        self.adjust_scroll_after_vertical_move(VerticalMoveDirection::Down);
    }

    pub fn move_cursor_right_for_insert(&mut self) {
        let row = self.active_cursor().row;
        let max_col = self.max_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut()
            && cursor.col < max_col
        {
            cursor.col = cursor.col.saturating_add(1);
        }
        self.preferred_col = None;
        self.adjust_scroll_after_horizontal_move(HorizontalMoveDirection::Right);
    }

    pub(crate) fn clamp_cursor_to_navigable_col(&mut self) {
        let row = self.active_cursor().row;
        let max_col = self.max_navigable_col_for_row(row);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.col = cursor.col.min(max_col).max(1);
        }
        self.align_active_window_scroll_to_cursor();
    }

    pub fn scroll_view_down_one_line(&mut self) {
        let target_col = self.capture_preferred_col_for_vertical();
        self.scroll_view_with_col_memory(1, target_col);
    }

    pub fn scroll_view_up_one_line(&mut self) {
        let target_col = self.capture_preferred_col_for_vertical();
        self.scroll_view_with_col_memory(-1, target_col);
    }

    pub fn scroll_view_down_half_page(&mut self) {
        let target_col = self.capture_preferred_col_for_vertical();
        let step = self.active_window_visible_rows().saturating_div(2).max(1) as i16;
        self.scroll_view_with_col_memory(step, target_col);
    }

    pub fn scroll_view_up_half_page(&mut self) {
        let target_col = self.capture_preferred_col_for_vertical();
        let step = self.active_window_visible_rows().saturating_div(2).max(1) as i16;
        self.scroll_view_with_col_memory(-step, target_col);
    }

    fn scroll_view_with_col_memory(&mut self, delta: i16, target_col: u16) {
        let active_window_id = self.active_window_id();
        let visible_rows = self.active_window_visible_rows();
        let max_scroll = self.max_row().saturating_sub(visible_rows);
        if let Some(window) = self.windows.get_mut(active_window_id) {
            if delta >= 0 {
                window.scroll_y = window.scroll_y.saturating_add(delta as u16).min(max_scroll);
            } else {
                window.scroll_y = window.scroll_y.saturating_sub((-delta) as u16);
            }
        }
        self.keep_cursor_in_view_after_scroll(target_col);
    }

    pub fn active_cursor(&self) -> CursorState {
        self.active_buffer_id()
            .and_then(|buffer_id| self.buffers.get(buffer_id))
            .map(|buffer| buffer.cursor)
            .unwrap_or_default()
    }

    pub fn insert_char_at_cursor(&mut self, ch: char) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }

        let line = lines
            .get_mut(row_idx)
            .expect("row index must exist after resize");
        let byte_idx = char_to_byte_idx(line, col_idx);
        line.insert(byte_idx, ch);
        buffer.cursor.col = buffer.cursor.col.saturating_add(1);
        buffer.text = lines.join("\n");
        self.mark_active_buffer_dirty();
        self.align_active_window_scroll_to_cursor();
    }

    pub fn insert_newline_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }

        let line = lines
            .get_mut(row_idx)
            .expect("row index must exist after resize");
        let byte_idx = char_to_byte_idx(line, col_idx);
        let tail = line.split_off(byte_idx);
        lines.insert(row_idx.saturating_add(1), tail);

        buffer.cursor.row = buffer.cursor.row.saturating_add(1);
        buffer.cursor.col = 1;
        buffer.text = lines.join("\n");
        self.mark_active_buffer_dirty();
        self.align_active_window_scroll_to_cursor();
    }

    pub fn open_line_below_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }

        lines.insert(row_idx.saturating_add(1), String::new());
        buffer.cursor.row = buffer.cursor.row.saturating_add(1);
        buffer.cursor.col = 1;
        buffer.text = lines.join("\n");
        self.mark_active_buffer_dirty();
        self.align_active_window_scroll_to_cursor();
    }

    pub fn open_line_above_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }

        lines.insert(row_idx, String::new());
        buffer.cursor.col = 1;
        buffer.text = lines.join("\n");
        self.mark_active_buffer_dirty();
        self.align_active_window_scroll_to_cursor();
    }

    pub fn join_line_below_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };

        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);
        if row_idx + 1 >= lines.len() {
            return;
        }

        let next = lines.remove(row_idx + 1);
        let current = lines
            .get_mut(row_idx)
            .expect("current row must exist while joining");

        let next_trimmed = next.trim_start();
        if !current.is_empty() && !next_trimmed.is_empty() && !current.ends_with(' ') {
            current.push(' ');
        }
        current.push_str(next_trimmed);

        let max_col = current.chars().count() as u16 + 1;
        buffer.cursor.col = buffer.cursor.col.min(max_col).max(1);
        buffer.text = lines.join("\n");
        self.mark_active_buffer_dirty();
        self.preferred_col = None;
        self.align_active_window_scroll_to_cursor();
    }

    pub fn backspace_at_cursor(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);

        if lines.is_empty() || row_idx >= lines.len() {
            return;
        }

        if col_idx > 0 {
            let line = lines.get_mut(row_idx).expect("row index must exist");
            let end = char_to_byte_idx(line, col_idx);
            col_idx = col_idx.saturating_sub(1);
            let start = char_to_byte_idx(line, col_idx);
            line.drain(start..end);
            buffer.cursor.col = buffer.cursor.col.saturating_sub(1);
        } else if row_idx > 0 {
            let current_line = lines.remove(row_idx);
            let prev_line = lines
                .get_mut(row_idx.saturating_sub(1))
                .expect("previous row must exist");
            let prev_char_len = prev_line.chars().count() as u16;
            prev_line.push_str(&current_line);
            buffer.cursor.row = buffer.cursor.row.saturating_sub(1);
            buffer.cursor.col = prev_char_len.saturating_add(1);
        } else {
            return;
        }

        buffer.text = lines.join("\n");
        self.mark_active_buffer_dirty();
        self.align_active_window_scroll_to_cursor();
    }

    pub fn cut_current_char_to_slot(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "cut failed: no active buffer".to_string();
            return;
        };
        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);
        let Some(line) = lines.get_mut(row_idx) else {
            self.status_bar.message = "cut failed: out of range".to_string();
            return;
        };
        let char_count = line.chars().count();
        if col_idx >= char_count {
            self.status_bar.message = "cut failed: no char".to_string();
            return;
        }

        let start = char_to_byte_idx(line, col_idx);
        let end = char_to_byte_idx(line, col_idx.saturating_add(1));
        let cut = line[start..end].to_string();
        line.drain(start..end);
        buffer.text = lines.join("\n");
        self.mark_active_buffer_dirty();
        self.line_slot = Some(cut);
        self.line_slot_line_wise = false;
        self.align_active_window_scroll_to_cursor();
        self.status_bar.message = "char cut".to_string();
    }

    pub fn paste_slot_at_cursor(&mut self) {
        let Some(slot_text) = self.line_slot.clone() else {
            self.status_bar.message = "paste failed: slot is empty".to_string();
            return;
        };
        let line_wise_slot = self.line_slot_line_wise;
        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "paste failed: no active buffer".to_string();
            return;
        };

        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);
        if row_idx >= lines.len() {
            lines.resize(row_idx.saturating_add(1), String::new());
        }
        if line_wise_slot {
            let insert_at = row_idx.saturating_add(1).min(lines.len());
            let inserted_lines = split_lines_owned(&slot_text);
            let inserted_count = inserted_lines.len() as u16;
            lines.splice(insert_at..insert_at, inserted_lines);
            buffer.cursor.row = insert_at as u16 + inserted_count;
            buffer.cursor.col = 1;
            buffer.text = lines.join("\n");
            self.mark_active_buffer_dirty();
            self.align_active_window_scroll_to_cursor();
            self.status_bar.message = "pasted".to_string();
            return;
        }

        let col_idx = buffer.cursor.col.saturating_sub(1) as usize;
        let line = lines
            .get_mut(row_idx)
            .expect("row index must exist after resize");
        let char_count = line.chars().count();
        let insert_char_idx = col_idx.saturating_add(1).min(char_count);
        let byte_idx = char_to_byte_idx(line, insert_char_idx);
        line.insert_str(byte_idx, &slot_text);
        buffer.cursor.col = buffer
            .cursor
            .col
            .saturating_add(slot_text.chars().count() as u16);
        buffer.text = lines.join("\n");
        self.mark_active_buffer_dirty();
        self.align_active_window_scroll_to_cursor();
        self.status_bar.message = "pasted".to_string();
    }

    pub fn delete_current_line_to_slot(&mut self) {
        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "line delete failed: no active buffer".to_string();
            return;
        };

        let row_idx = buffer.cursor.row.saturating_sub(1) as usize;
        let mut lines = split_lines_owned(&buffer.text);
        if row_idx >= lines.len() {
            self.status_bar.message = "line delete failed: out of range".to_string();
            return;
        }

        let deleted = lines.remove(row_idx);
        if lines.is_empty() {
            lines.push(String::new());
        }

        buffer.text = lines.join("\n");
        let visible_rows = if buffer.text.is_empty() {
            1
        } else {
            buffer.text.lines().count().max(1)
        };
        let new_row = row_idx
            .min(visible_rows.saturating_sub(1))
            .saturating_add(1) as u16;
        buffer.cursor.row = new_row;
        buffer.cursor.col = 1;
        self.mark_active_buffer_dirty();
        self.line_slot = Some(deleted);
        self.line_slot_line_wise = true;
        self.align_active_window_scroll_to_cursor();
        self.status_bar.message = "line deleted".to_string();
    }

    pub fn delete_visual_selection_to_slot(&mut self) {
        let line_wise = self.is_visual_line_mode();
        let Some((start, end)) = self.normalized_visual_bounds() else {
            self.status_bar.message = "visual delete failed: no anchor".to_string();
            self.exit_visual_mode();
            return;
        };

        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "visual delete failed: no active buffer".to_string();
            self.exit_visual_mode();
            return;
        };

        let mut lines = split_lines_owned(&buffer.text);
        let start_row = start.row.saturating_sub(1) as usize;
        let end_row = end.row.saturating_sub(1) as usize;
        if start_row >= lines.len() || end_row >= lines.len() {
            self.status_bar.message = "visual delete failed: out of range".to_string();
            self.exit_visual_mode();
            return;
        }

        if line_wise {
            let deleted = lines[start_row..=end_row].join("\n");
            lines.drain(start_row..=end_row);
            if lines.is_empty() {
                lines.push(String::new());
            }
            buffer.text = lines.join("\n");
            let visible_rows = if buffer.text.is_empty() {
                1
            } else {
                buffer.text.lines().count().max(1)
            };
            let new_row = start_row
                .min(visible_rows.saturating_sub(1))
                .saturating_add(1) as u16;
            buffer.cursor.row = new_row;
            buffer.cursor.col = 1;
            self.line_slot = Some(deleted);
            self.line_slot_line_wise = true;
            self.mark_active_buffer_dirty();
            self.align_active_window_scroll_to_cursor();
            self.exit_visual_mode();
            self.status_bar.message = "selection deleted".to_string();
            return;
        }

        let start_line_len = lines[start_row].chars().count() as u16;
        let end_line_len = lines[end_row].chars().count() as u16;
        if start_line_len == 0 && end_line_len == 0 {
            self.status_bar.message = "visual delete failed: empty".to_string();
            self.exit_visual_mode();
            return;
        }

        let start_col = start.col.max(1).min(start_line_len.max(1));
        let end_col = end.col.max(1).min(end_line_len.max(1));

        let deleted_text = if start_row == end_row {
            let line = &mut lines[start_row];
            let start_byte = char_to_byte_idx(line, start_col.saturating_sub(1) as usize);
            let end_byte = char_to_byte_idx(line, end_col as usize);
            let deleted = line[start_byte..end_byte].to_string();
            line.drain(start_byte..end_byte);
            deleted
        } else {
            let start_line = lines[start_row].clone();
            let end_line = lines[end_row].clone();
            let start_keep_byte =
                char_to_byte_idx(&start_line, start_col.saturating_sub(1) as usize);
            let end_del_byte = char_to_byte_idx(&end_line, end_col as usize);

            let mut deleted_parts = Vec::new();
            deleted_parts.push(start_line[start_keep_byte..].to_string());
            for line in lines.iter().take(end_row).skip(start_row.saturating_add(1)) {
                deleted_parts.push(line.clone());
            }
            deleted_parts.push(end_line[..end_del_byte].to_string());

            let merged_line = format!(
                "{}{}",
                &start_line[..start_keep_byte],
                &end_line[end_del_byte..]
            );
            lines[start_row] = merged_line;
            for _ in start_row.saturating_add(1)..=end_row {
                lines.remove(start_row.saturating_add(1));
            }

            deleted_parts.join("\n")
        };

        buffer.text = lines.join("\n");
        buffer.cursor.row = start_row.saturating_add(1) as u16;
        let line_len = lines[start_row].chars().count() as u16;
        buffer.cursor.col = start_col.min(line_len.saturating_add(1));
        self.mark_active_buffer_dirty();
        self.line_slot = Some(deleted_text);
        self.line_slot_line_wise = false;
        self.align_active_window_scroll_to_cursor();
        self.exit_visual_mode();
        self.status_bar.message = "selection deleted".to_string();
    }

    pub fn yank_visual_selection_to_slot(&mut self) {
        let line_wise = self.is_visual_line_mode();
        let Some((start, end)) = self.normalized_visual_bounds() else {
            self.status_bar.message = "visual yank failed: no anchor".to_string();
            self.exit_visual_mode();
            return;
        };

        let Some(text) = self.active_buffer_text() else {
            self.status_bar.message = "visual yank failed: no active buffer".to_string();
            self.exit_visual_mode();
            return;
        };
        let lines = split_lines_owned(text);
        let start_row = start.row.saturating_sub(1) as usize;
        let end_row = end.row.saturating_sub(1) as usize;
        if start_row >= lines.len() || end_row >= lines.len() {
            self.status_bar.message = "visual yank failed: out of range".to_string();
            self.exit_visual_mode();
            return;
        }
        if line_wise {
            self.line_slot = Some(lines[start_row..=end_row].join("\n"));
            self.line_slot_line_wise = true;
            self.exit_visual_mode();
            self.status_bar.message = "selection yanked".to_string();
            return;
        }

        let start_line = &lines[start_row];
        let end_line = &lines[end_row];
        let start_line_len = start_line.chars().count() as u16;
        let end_line_len = end_line.chars().count() as u16;
        let start_col = start.col.max(1).min(start_line_len.max(1));
        let end_col = end.col.max(1).min(end_line_len.max(1));

        let yanked = if start_row == end_row {
            let start_byte = char_to_byte_idx(start_line, start_col.saturating_sub(1) as usize);
            let end_byte = char_to_byte_idx(start_line, end_col as usize);
            start_line[start_byte..end_byte].to_string()
        } else {
            let mut parts = Vec::new();
            let start_keep_byte =
                char_to_byte_idx(start_line, start_col.saturating_sub(1) as usize);
            parts.push(start_line[start_keep_byte..].to_string());
            for line in lines.iter().take(end_row).skip(start_row.saturating_add(1)) {
                parts.push(line.clone());
            }
            let end_del_byte = char_to_byte_idx(end_line, end_col as usize);
            parts.push(end_line[..end_del_byte].to_string());
            parts.join("\n")
        };

        self.line_slot = Some(yanked);
        self.line_slot_line_wise = false;
        self.exit_visual_mode();
        self.status_bar.message = "selection yanked".to_string();
    }

    pub fn replace_visual_selection_with_slot(&mut self) {
        let line_wise = self.is_visual_line_mode();
        let Some(slot_text) = self.line_slot.clone() else {
            self.status_bar.message = "paste failed: slot is empty".to_string();
            self.exit_visual_mode();
            return;
        };
        let Some((start, end)) = self.normalized_visual_bounds() else {
            self.status_bar.message = "visual paste failed: no anchor".to_string();
            self.exit_visual_mode();
            return;
        };

        let Some(buffer) = self.active_buffer_mut() else {
            self.status_bar.message = "visual paste failed: no active buffer".to_string();
            self.exit_visual_mode();
            return;
        };

        let mut lines = split_lines_owned(&buffer.text);
        let start_row = start.row.saturating_sub(1) as usize;
        let end_row = end.row.saturating_sub(1) as usize;
        if start_row >= lines.len() || end_row >= lines.len() {
            self.status_bar.message = "visual paste failed: out of range".to_string();
            self.exit_visual_mode();
            return;
        }

        if line_wise {
            let replacement = split_lines_owned(&slot_text);
            lines.splice(start_row..=end_row, replacement);
            if lines.is_empty() {
                lines.push(String::new());
            }
            buffer.text = lines.join("\n");
            buffer.cursor.row = start_row.saturating_add(1) as u16;
            buffer.cursor.col = 1;
            self.mark_active_buffer_dirty();
            self.align_active_window_scroll_to_cursor();
            self.exit_visual_mode();
            self.status_bar.message = "selection replaced".to_string();
            return;
        }

        let start_line = lines[start_row].clone();
        let end_line = lines[end_row].clone();
        let start_line_len = start_line.chars().count() as u16;
        let end_line_len = end_line.chars().count() as u16;
        let start_col = start.col.max(1).min(start_line_len.max(1));
        let end_col = end.col.max(1).min(end_line_len.max(1));

        let prefix_end = char_to_byte_idx(&start_line, start_col.saturating_sub(1) as usize);
        let suffix_start = char_to_byte_idx(&end_line, end_col as usize);
        let prefix = start_line[..prefix_end].to_string();
        let suffix = end_line[suffix_start..].to_string();

        let slot_lines = split_lines_owned(&slot_text);
        let mut replacement = Vec::new();
        if slot_lines.len() == 1 {
            replacement.push(format!("{}{}{}", prefix, slot_lines[0], suffix));
        } else {
            replacement.push(format!("{}{}", prefix, slot_lines[0]));
            for line in slot_lines
                .iter()
                .skip(1)
                .take(slot_lines.len().saturating_sub(2))
            {
                replacement.push(line.clone());
            }
            let last = slot_lines.last().cloned().unwrap_or_default();
            replacement.push(format!("{}{}", last, suffix));
        }

        lines.splice(start_row..=end_row, replacement);
        buffer.text = lines.join("\n");
        buffer.cursor.row = start_row.saturating_add(1) as u16;
        buffer.cursor.col = start_col;
        self.mark_active_buffer_dirty();
        self.align_active_window_scroll_to_cursor();
        self.exit_visual_mode();
        self.status_bar.message = "selection replaced".to_string();
    }

    fn capture_preferred_col_for_vertical(&mut self) -> u16 {
        if let Some(col) = self.preferred_col {
            return col;
        }
        let col = self.active_cursor().col;
        self.preferred_col = Some(col);
        col
    }

    fn max_row(&self) -> u16 {
        self.active_buffer_text()
            .map(|text| {
                if text.is_empty() {
                    1
                } else {
                    text.lines().count() as u16
                }
            })
            .unwrap_or(1)
    }

    fn max_col_for_row(&self, row: u16) -> u16 {
        let row_index = row.saturating_sub(1) as usize;
        let line_len = self
            .active_buffer_text()
            .and_then(|text| text.lines().nth(row_index))
            .map(|line| line.chars().count() as u16)
            .unwrap_or(0);
        line_len.saturating_add(1)
    }

    fn max_navigable_col_for_row(&self, row: u16) -> u16 {
        self.max_col_for_row(row).saturating_sub(1).max(1)
    }

    fn max_visual_char_col_for_row(&self, row: u16) -> u16 {
        let line_len = self.max_col_for_row(row).saturating_sub(1);
        if self.row_has_newline_char(row) {
            line_len.saturating_add(1).max(1)
        } else {
            line_len.max(1)
        }
    }

    fn row_has_newline_char(&self, row: u16) -> bool {
        let Some(text) = self.active_buffer_text() else {
            return false;
        };
        if text.is_empty() {
            return false;
        }
        let total_rows = text.lines().count().max(1) as u16;
        if row < total_rows {
            return true;
        }
        row == total_rows && text.ends_with('\n')
    }

    fn active_buffer_text(&self) -> Option<&str> {
        let active_window_id = self.tabs.get(&self.active_tab)?.active_window;
        let buffer_id = self.windows.get(active_window_id)?.buffer_id?;
        self.buffers
            .get(buffer_id)
            .map(|buffer| buffer.text.as_str())
    }

    fn active_buffer_cursor_mut(&mut self) -> Option<&mut CursorState> {
        let buffer_id = self.active_buffer_id()?;
        self.buffers
            .get_mut(buffer_id)
            .map(|buffer| &mut buffer.cursor)
    }

    fn active_buffer_mut(&mut self) -> Option<&mut BufferState> {
        let buffer_id = self.active_buffer_id()?;
        self.buffers.get_mut(buffer_id)
    }

    fn normalized_visual_bounds(&self) -> Option<(CursorState, CursorState)> {
        let anchor = self.visual_anchor?;
        let cursor = self.active_cursor();
        let (mut start, mut end) = if (anchor.row, anchor.col) <= (cursor.row, cursor.col) {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };

        if self.is_visual_line_mode() {
            start.col = 1;
            end.col = self.max_col_for_row(end.row).saturating_sub(1).max(1);
        }
        Some((start, end))
    }

    fn active_window_visible_rows(&self) -> u16 {
        let window_id = self.active_window_id();
        self.windows
            .get(window_id)
            .map(|window| {
                let reserved_for_split_line = u16::from(window.y > 0);
                window.height.saturating_sub(reserved_for_split_line).max(1)
            })
            .unwrap_or(1)
    }

    fn keep_cursor_in_view_after_scroll(&mut self, target_col: u16) {
        let active_window_id = self.active_window_id();
        let Some(window) = self.windows.get(active_window_id).cloned() else {
            return;
        };
        let visible_rows = self.active_window_visible_rows();
        let top_row = window.scroll_y.saturating_add(1);
        let bottom_row = top_row.saturating_add(visible_rows.saturating_sub(1));
        let row = self.active_cursor().row;

        let target_row = if row < top_row {
            top_row
        } else if row > bottom_row {
            bottom_row
        } else {
            return;
        };

        let max_col = self.max_navigable_col_for_row(target_row);
        if let Some(cursor) = self.active_buffer_cursor_mut() {
            cursor.row = target_row;
            cursor.col = target_col.min(max_col).max(1);
        }
    }

    fn active_window_visible_text_cols(&self) -> u16 {
        let window_id = self.active_window_id();
        self.windows
            .get(window_id)
            .map(|window| {
                let reserved_for_split_line = u16::from(window.x > 0);
                let local_width = window.width.saturating_sub(reserved_for_split_line).max(1);
                let total_lines = self
                    .active_buffer_text()
                    .map(|text| text.lines().count().max(1))
                    .unwrap_or(1);
                let desired_number_col_width = total_lines.to_string().len() as u16 + 1;
                let number_col_width = if local_width <= desired_number_col_width {
                    0
                } else {
                    desired_number_col_width
                };
                local_width.saturating_sub(number_col_width).max(1)
            })
            .unwrap_or(1)
    }

    fn adjust_scroll_after_vertical_move(&mut self, direction: VerticalMoveDirection) {
        let active_window_id = self.active_window_id();
        let cursor_row = self.active_cursor().row;
        let cursor_line = cursor_row.saturating_sub(1);
        let visible_rows = self.active_window_visible_rows();
        let max_row = self.max_row();
        let max_scroll = max_row.saturating_sub(visible_rows);
        let threshold = self.cursor_scroll_threshold;
        let visible_tail = visible_rows.saturating_sub(1);

        if let Some(window) = self.windows.get_mut(active_window_id) {
            match direction {
                VerticalMoveDirection::Up => {
                    let top_trigger = window.scroll_y.saturating_add(threshold);
                    if cursor_line < top_trigger {
                        window.scroll_y = cursor_line.saturating_sub(threshold).min(max_scroll);
                    }
                }
                VerticalMoveDirection::Down => {
                    let bottom = window.scroll_y.saturating_add(visible_tail);
                    let bottom_trigger = bottom.saturating_sub(threshold);
                    if cursor_line > bottom_trigger {
                        let needed_top = cursor_line
                            .saturating_add(threshold)
                            .saturating_sub(visible_tail);
                        window.scroll_y = needed_top.min(max_scroll);
                    }
                }
            }
        }
    }

    fn adjust_scroll_after_horizontal_move(&mut self, direction: HorizontalMoveDirection) {
        let active_window_id = self.active_window_id();
        let visible_cols = self.active_window_visible_text_cols();
        let visible_tail = visible_cols.saturating_sub(1);
        let threshold = self.cursor_scroll_threshold.min(visible_tail);
        let cursor_display_col = self.active_cursor_display_col();
        let line_display_width = self.active_line_display_width();
        let max_scroll = line_display_width.saturating_sub(visible_tail);

        if let Some(window) = self.windows.get_mut(active_window_id) {
            match direction {
                HorizontalMoveDirection::Left => {
                    let left_trigger = window.scroll_x.saturating_add(threshold);
                    if cursor_display_col < left_trigger {
                        window.scroll_x =
                            cursor_display_col.saturating_sub(threshold).min(max_scroll);
                    }
                }
                HorizontalMoveDirection::Right => {
                    let right = window.scroll_x.saturating_add(visible_tail);
                    let right_trigger = right.saturating_sub(threshold);
                    if cursor_display_col > right_trigger {
                        let needed_left = cursor_display_col
                            .saturating_add(threshold)
                            .saturating_sub(visible_tail);
                        window.scroll_x = needed_left.min(max_scroll);
                    }
                }
            }
        }
    }

    pub(super) fn align_active_window_scroll_to_cursor(&mut self) {
        let active_window_id = self.active_window_id();
        let Some(window) = self.windows.get(active_window_id).cloned() else {
            return;
        };
        let cursor_line = self.active_cursor().row.saturating_sub(1);
        let visible_rows = self.active_window_visible_rows();
        let max_row = self.max_row();
        let max_scroll = max_row.saturating_sub(visible_rows);
        let threshold = self
            .cursor_scroll_threshold
            .min(visible_rows.saturating_sub(1));
        let visible_tail = visible_rows.saturating_sub(1);
        let top_trigger = window.scroll_y.saturating_add(threshold);
        let bottom = window.scroll_y.saturating_add(visible_tail);
        let bottom_trigger = bottom.saturating_sub(threshold);
        let visible_cols = self.active_window_visible_text_cols();
        let col_tail = visible_cols.saturating_sub(1);
        let col_threshold = self.cursor_scroll_threshold.min(col_tail);
        let cursor_display_col = self.active_cursor_display_col();
        let line_display_width = self.active_line_display_width();
        let max_scroll_x = line_display_width.saturating_sub(col_tail);
        let left_trigger = window.scroll_x.saturating_add(col_threshold);
        let right = window.scroll_x.saturating_add(col_tail);
        let right_trigger = right.saturating_sub(col_threshold);

        let mut next_scroll = window.scroll_y;
        if cursor_line < top_trigger {
            next_scroll = cursor_line.saturating_sub(threshold);
        } else if cursor_line > bottom_trigger {
            next_scroll = cursor_line
                .saturating_add(threshold)
                .saturating_sub(visible_tail);
        }
        next_scroll = next_scroll.min(max_scroll);
        let mut next_scroll_x = window.scroll_x;
        if cursor_display_col < left_trigger {
            next_scroll_x = cursor_display_col.saturating_sub(col_threshold);
        } else if cursor_display_col > right_trigger {
            next_scroll_x = cursor_display_col
                .saturating_add(col_threshold)
                .saturating_sub(col_tail);
        }
        next_scroll_x = next_scroll_x.min(max_scroll_x);

        if let Some(active_window) = self.windows.get_mut(active_window_id) {
            active_window.scroll_y = next_scroll;
            active_window.scroll_x = next_scroll_x;
        }
    }

    fn active_cursor_display_col(&self) -> u16 {
        let cursor = self.active_cursor();
        let row_index = cursor.row.saturating_sub(1) as usize;
        let char_index = cursor.col.saturating_sub(1) as usize;
        self.active_buffer_text()
            .and_then(|text| text.lines().nth(row_index))
            .map(|line| display_width_of_char_prefix(line, char_index) as u16)
            .unwrap_or(0)
    }

    fn active_line_display_width(&self) -> u16 {
        let row_index = self.active_cursor().row.saturating_sub(1) as usize;
        self.active_buffer_text()
            .and_then(|text| text.lines().nth(row_index))
            .map(|line| {
                line.chars()
                    .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0) as u16)
                    .sum()
            })
            .unwrap_or(0)
    }
}

fn split_lines_owned(text: &str) -> Vec<String> {
    let mut lines = text
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    s.char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

fn display_width_of_char_prefix(line: &str, char_count: usize) -> usize {
    line.chars()
        .take(char_count)
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerticalMoveDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HorizontalMoveDirection {
    Left,
    Right,
}
