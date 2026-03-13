use std::{collections::{BTreeMap, HashMap, HashSet, VecDeque}, fmt, path::{Path, PathBuf}, time::{Duration, Instant, SystemTime}};

use frizbee::{Config as FrizbeeConfig, match_list_indices};
use ropey::Rope;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};
use time::{OffsetDateTime, format_description::FormatItem, macros::format_description};
use tracing::error;

use crate::{command::{BuiltinCommand, CommandArgKind, CommandCommand, CommandConfigError, CommandConfigFile, CommandPaletteFileMatch, CommandPaletteItem, CommandPaletteMatch, CommandRegistry, PluginCommandRegistration}, preview::preview_max_scroll_with_mode};

mod buffer;
mod edit;
mod mode;
mod session;
mod tab;
mod window;

new_key_type! { pub struct BufferId; }
new_key_type! { pub struct WindowId; }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferState {
	pub name:                String,
	pub path:                Option<PathBuf>,
	pub text:                Rope,
	// This is the last clean snapshot loaded from or saved to disk.
	pub clean_text:          Rope,
	pub dirty:               bool,
	pub externally_modified: bool,
	pub undo_stack:          Vec<BufferHistoryEntry>,
	pub redo_stack:          Vec<BufferHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferHistoryEntry {
	pub edits:         Vec<BufferEditSnapshot>,
	pub before_cursor: CursorState,
	pub after_cursor:  CursorState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedBufferHistory {
	pub current_text: String,
	pub cursor:       CursorState,
	pub undo_stack:   Vec<BufferHistoryEntry>,
	pub redo_stack:   Vec<BufferHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferEditSnapshot {
	pub start_byte:    usize,
	pub deleted_text:  String,
	pub inserted_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RopeTextDiff {
	pub start_char:    usize,
	pub start_byte:    usize,
	pub deleted_text:  String,
	pub inserted_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowState {
	pub buffer_id: Option<BufferId>,
	pub cursor:    CursorState,
	pub scroll_x:  u16,
	pub scroll_y:  u16,
	pub x:         u16,
	pub y:         u16,
	pub width:     u16,
	pub height:    u16,
	pub layout_x:  u32,
	pub layout_y:  u32,
	pub layout_w:  u32,
	pub layout_h:  u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct WindowBufferViewState {
	pub cursor:   CursorState,
	pub scroll_x: u16,
	pub scroll_y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabState {
	pub windows:       Vec<WindowId>,
	pub active_window: WindowId,
	pub buffer_order:  Vec<BufferId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
	pub mode:         StatusBarMode,
	pub message:      String,
	pub key_sequence: String,
}

impl Default for StatusBarState {
	fn default() -> Self {
		Self {
			mode:         StatusBarMode::Normal,
			message:      "new file".to_string(),
			key_sequence: String::new(),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBarMode {
	Normal,
	Insert,
	InsertBlock,
	Command,
	Visual,
	VisualLine,
	VisualBlock,
}

impl fmt::Display for StatusBarMode {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let label = match self {
			Self::Normal => "NORMAL",
			Self::Insert => "INSERT",
			Self::InsertBlock => "INSERT BLOCK",
			Self::Command => "COMMAND",
			Self::Visual => "VISUAL",
			Self::VisualLine => "VISUAL LINE",
			Self::VisualBlock => "VISUAL BLOCK",
		};
		f.write_str(label)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorState {
	pub row: u16,
	pub col: u16,
}

impl Default for CursorState {
	fn default() -> Self { Self { row: 1, col: 1 } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
	Normal,
	Insert,
	Command,
	VisualChar,
	VisualLine,
	VisualBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitAxis {
	Horizontal,
	Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NormalSequenceKey {
	Leader,
	Tab,
	Esc,
	Enter,
	Backspace,
	F1,
	Left,
	Right,
	Up,
	Down,
	Char(char),
	Ctrl(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeymapScope {
	ModeNormal,
	ModeVisual,
	ModeCommand,
	ModeInsert,
	OverlayWhichKey,
	OverlayCommandPalette,
	OverlayPicker,
	OverlayNotificationCenter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatingWindowPlacement {
	Centered { width: u16, height: u16 },
	BottomRight { width: u16, height: u16, margin_right: u16, margin_bottom: u16 },
	Absolute { x: u16, y: u16, width: u16, height: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatingWindowLine {
	pub key:       String,
	pub summary:   String,
	pub is_prefix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatingWindowState {
	pub title:     String,
	pub subtitle:  Option<String>,
	pub footer:    Option<String>,
	pub placement: FloatingWindowPlacement,
	pub lines:     Vec<FloatingWindowLine>,
	pub scroll:    usize,
}

impl FloatingWindowState {
	pub fn visible_body_rows(&self) -> usize {
		let outer_height = match self.placement {
			FloatingWindowPlacement::Centered { height, .. }
			| FloatingWindowPlacement::BottomRight { height, .. }
			| FloatingWindowPlacement::Absolute { height, .. } => height as usize,
		};
		let border_rows = 2usize;
		let footer_rows = if self.footer.is_some() { 2usize } else { 0 };
		outer_height.saturating_sub(border_rows + footer_rows).max(1)
	}

	pub fn max_scroll(&self) -> usize { self.lines.len().saturating_sub(self.visible_body_rows()) }

	pub fn total_pages(&self) -> usize {
		let body_rows = self.visible_body_rows().max(1);
		self.lines.len().max(1).div_ceil(body_rows)
	}

	pub fn current_page(&self) -> usize {
		let body_rows = self.visible_body_rows().max(1);
		let visible_end = self.scroll.saturating_add(body_rows).min(self.lines.len().max(1));
		visible_end.div_ceil(body_rows).max(1).min(self.total_pages())
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteState {
	pub query:          String,
	pub items:          Vec<CommandPaletteItem>,
	pub selected:       usize,
	pub loading:        bool,
	pub showing_files:  bool,
	pub preview_title:  String,
	pub preview_lines:  Vec<String>,
	pub preview_scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFileEntry {
	pub absolute_path: PathBuf,
	pub relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFileMatch {
	pub absolute_path: PathBuf,
	pub relative_path: String,
	pub match_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFilePickerState {
	pub query:          String,
	pub entries:        Vec<WorkspaceFileEntry>,
	pub items:          Vec<WorkspaceFileMatch>,
	pub selected:       usize,
	pub preferred:      Option<(PathBuf, String)>,
	pub total_files:    usize,
	pub total_matches:  usize,
	pub preview_title:  String,
	pub preview_lines:  Vec<String>,
	pub preview_scroll: usize,
	pub loading:        bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyHintsOverlayState {
	pub scope:    KeymapScope,
	pub prefix:   Vec<NormalSequenceKey>,
	pub overview: bool,
	pub window:   FloatingWindowState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayState {
	KeyHints(KeyHintsOverlayState),
	FloatingWindow(FloatingWindowState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
	Info,
	Warn,
	Error,
}

impl NotificationLevel {
	pub fn label(self) -> &'static str {
		match self {
			Self::Info => "INFO",
			Self::Warn => "WARN",
			Self::Error => "ERROR",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationEntry {
	pub id:               u64,
	pub level:            NotificationLevel,
	pub message:          String,
	pub created_at:       SystemTime,
	pub created_at_local: String,
	pub read:             bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveNotification {
	id:         u64,
	expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotificationCenterState {
	pub selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPreviewItem {
	pub id:               u64,
	pub level:            NotificationLevel,
	pub message:          String,
	pub created_at_local: String,
	pub read:             bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPreviewState {
	pub items:            Vec<NotificationPreviewItem>,
	pub unread_total:     usize,
	pub open_center_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCenterItem {
	pub id:               u64,
	pub level:            NotificationLevel,
	pub message:          String,
	pub created_at_local: String,
	pub read:             bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCenterView {
	pub selected:     usize,
	pub items:        Vec<NotificationCenterItem>,
	pub unread_total: usize,
}

#[derive(Debug)]
pub struct PendingInsertUndoGroup {
	pub buffer_id:     BufferId,
	pub before_cursor: CursorState,
	pub edits:         Vec<BufferEditSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingBlockInsert {
	pub start_row:          u16,
	pub end_row:            u16,
	pub base_display_col:   u16,
	pub cursor_display_col: u16,
}

#[derive(Debug, Clone)]
pub struct PendingSwapDecision {
	pub buffer_id:      BufferId,
	pub source_path:    PathBuf,
	pub base_text:      String,
	pub owner_pid:      u32,
	pub owner_username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSessionSnapshot {
	pub version:          u32,
	pub buffers:          Vec<WorkspaceBufferSnapshot>,
	pub buffer_order:     Vec<usize>,
	pub tabs:             Vec<WorkspaceTabSnapshot>,
	pub active_tab_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceBufferSnapshot {
	pub path:       Option<PathBuf>,
	pub text:       String,
	pub clean_text: String,
	#[serde(default)]
	pub history:    Option<WorkspaceBufferHistorySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBufferHistorySnapshot {
	pub undo_stack: Vec<BufferHistoryEntry>,
	pub redo_stack: Vec<BufferHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTabSnapshot {
	pub windows:             Vec<WorkspaceWindowSnapshot>,
	pub active_window_index: usize,
	#[serde(default)]
	pub buffer_order:        Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceWindowSnapshot {
	pub buffer_index: Option<usize>,
	pub x:            u16,
	pub y:            u16,
	pub width:        u16,
	pub height:       u16,
	pub views:        Vec<WorkspaceWindowBufferViewSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceWindowBufferViewSnapshot {
	pub buffer_index: usize,
	pub cursor:       CursorState,
	pub scroll_x:     u16,
	pub scroll_y:     u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFilePickerBodyLayout {
	pub horizontal_split: bool,
	pub list_width:       u16,
	pub divider_width:    u16,
	pub preview_width:    u16,
}

pub fn compute_workspace_file_picker_body_layout(content_width: u16) -> WorkspaceFilePickerBodyLayout {
	let picker_width = content_width.saturating_sub(4).clamp(56, 140).min(content_width);
	let body_width = picker_width.saturating_sub(2);
	if body_width >= 96 {
		let divider_width = 1u16.min(body_width);
		let remaining = body_width.saturating_sub(divider_width);
		let mut list_width = ((u32::from(remaining) * 54) / 100) as u16;
		let mut preview_width = remaining.saturating_sub(list_width);
		if preview_width == 0 {
			preview_width = 1;
			list_width = remaining.saturating_sub(preview_width);
		}
		return WorkspaceFilePickerBodyLayout {
			horizontal_split: true,
			list_width,
			divider_width,
			preview_width,
		};
	}
	WorkspaceFilePickerBodyLayout {
		horizontal_split: false,
		list_width:       body_width,
		divider_width:    0,
		preview_width:    body_width,
	}
}

pub fn compute_command_palette_preview_width(content_width: u16) -> u16 {
	let palette_width = content_width.saturating_sub(6).clamp(48, 104);
	palette_width.saturating_sub(2)
}

fn format_local_timestamp(time: SystemTime) -> String {
	static FORMAT: &[FormatItem<'static>] =
		format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
	let Ok(duration) = time.duration_since(SystemTime::UNIX_EPOCH) else {
		return "1970-01-01 00:00:00".to_string();
	};
	let Ok(offset_time) = OffsetDateTime::from_unix_timestamp(duration.as_secs() as i64) else {
		return "1970-01-01 00:00:00".to_string();
	};
	let local = OffsetDateTime::now_local()
		.map(|now| now.offset())
		.map(|offset| offset_time.to_offset(offset))
		.unwrap_or(offset_time);
	local.format(FORMAT).unwrap_or_else(|_| "1970-01-01 00:00:00".to_string())
}

#[derive(Debug)]
pub struct RimState {
	pub title:                                 String,
	pub workspace_root:                        PathBuf,
	pub active_tab:                            TabId,
	pub leader_key:                            char,
	pub mode:                                  EditorMode,
	pub visual_anchor:                         Option<CursorState>,
	pub visual_block_anchor_display_col:       Option<u16>,
	pub visual_block_cursor_display_col:       Option<u16>,
	pub command_line:                          String,
	pub quit_after_save:                       bool,
	pub force_quit_trim_file_dirty_in_session: bool,
	pub pending_save_path:                     Option<(BufferId, PathBuf)>,
	pub preferred_col:                         Option<u16>,
	pub line_slot:                             Option<String>,
	pub line_slot_line_wise:                   bool,
	pub line_slot_block_wise:                  bool,
	pub cursor_scroll_threshold:               u16,
	pub key_hints_width:                       u16,
	pub key_hints_max_height:                  u16,
	pub word_wrap:                             bool,
	pub picker_preview_word_wrap:              bool,
	pub normal_sequence:                       Vec<NormalSequenceKey>,
	pub visual_g_pending:                      bool,
	pub pending_insert_group:                  Option<PendingInsertUndoGroup>,
	pub pending_block_insert:                  Option<PendingBlockInsert>,
	pub pending_swap_decision:                 Option<PendingSwapDecision>,
	pub in_flight_internal_saves:              HashSet<BufferId>,
	pub ignore_external_change_until:          HashMap<BufferId, Instant>,
	pub command_registry:                      CommandRegistry,
	pub overlay:                               Option<OverlayState>,
	pub command_palette:                       Option<CommandPaletteState>,
	pub workspace_file_picker:                 Option<WorkspaceFilePickerState>,
	pub notification_center:                   Option<NotificationCenterState>,
	pub notifications:                         Vec<NotificationEntry>,
	notification_preview_active:               Vec<ActiveNotification>,
	notification_preview_queue:                VecDeque<u64>,
	next_notification_id:                      u64,
	pub workspace_file_cache:                  Vec<WorkspaceFileEntry>,
	pub workspace_file_cache_loading:          bool,
	pub(crate) window_buffer_views:            HashMap<(WindowId, BufferId), WindowBufferViewState>,
	pub buffers:                               SlotMap<BufferId, BufferState>,
	pub buffer_order:                          Vec<BufferId>,
	pub windows:                               SlotMap<WindowId, WindowState>,
	pub tabs:                                  BTreeMap<TabId, TabState>,
	pub status_bar:                            StatusBarState,
}

impl RimState {
	const INTERNAL_SAVE_WATCHER_IGNORE_WINDOW: Duration = Duration::from_millis(750);
	const MAX_HISTORY_ENTRIES: usize = 256;
	const NOTIFICATION_PREVIEW_CAPACITY: usize = 5;
	const NOTIFICATION_PREVIEW_DURATION: Duration = Duration::from_secs(3);

	pub fn new() -> Self {
		let buffers = SlotMap::with_key();
		let mut windows = SlotMap::with_key();
		let mut tabs = BTreeMap::new();

		let tab_id = TabId(1);
		let window_id = windows.insert(WindowState::default());

		tabs.insert(tab_id, TabState {
			windows:       vec![window_id],
			active_window: window_id,
			buffer_order:  Vec::new(),
		});

		Self {
			title: "Rim".to_string(),
			workspace_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
			active_tab: tab_id,
			leader_key: ' ',
			mode: EditorMode::Normal,
			visual_anchor: None,
			visual_block_anchor_display_col: None,
			visual_block_cursor_display_col: None,
			command_line: String::new(),
			quit_after_save: false,
			force_quit_trim_file_dirty_in_session: false,
			pending_save_path: None,
			preferred_col: None,
			line_slot: None,
			line_slot_line_wise: false,
			line_slot_block_wise: false,
			cursor_scroll_threshold: 0,
			key_hints_width: 42,
			key_hints_max_height: 36,
			word_wrap: false,
			picker_preview_word_wrap: true,
			normal_sequence: Vec::new(),
			visual_g_pending: false,
			pending_insert_group: None,
			pending_block_insert: None,
			pending_swap_decision: None,
			in_flight_internal_saves: HashSet::new(),
			ignore_external_change_until: HashMap::new(),
			command_registry: CommandRegistry::with_defaults(),
			overlay: None,
			command_palette: None,
			workspace_file_picker: None,
			notification_center: None,
			notifications: Vec::new(),
			notification_preview_active: Vec::new(),
			notification_preview_queue: VecDeque::new(),
			next_notification_id: 1,
			workspace_file_cache: Vec::new(),
			workspace_file_cache_loading: false,
			window_buffer_views: HashMap::new(),
			buffers,
			buffer_order: Vec::new(),
			windows,
			tabs,
			status_bar: StatusBarState::default(),
		}
	}

	pub fn apply_command_config(&mut self, config: &CommandConfigFile) -> Vec<CommandConfigError> {
		self.command_registry.apply_config(config)
	}

	pub fn push_notification(&mut self, level: NotificationLevel, message: impl Into<String>) {
		let message = message.into();
		let now = Instant::now();
		let id = self.next_notification_id;
		self.next_notification_id = self.next_notification_id.saturating_add(1);
		let created_at = SystemTime::now();
		let created_at_local = format_local_timestamp(created_at);
		self.notifications.push(NotificationEntry {
			id,
			level,
			message,
			created_at,
			created_at_local,
			read: false,
		});
		self.notification_preview_queue.push_back(id);
		self.tick_notifications(now);
	}

	pub fn tick_notifications(&mut self, now: Instant) -> bool {
		let before = self.notification_preview_active.len();
		self.notification_preview_active.retain(|item| item.expires_at > now);
		while self.notification_preview_active.len() < Self::NOTIFICATION_PREVIEW_CAPACITY {
			let Some(next_id) = self.notification_preview_queue.pop_front() else {
				break;
			};
			if self.notifications.iter().any(|entry| entry.id == next_id) {
				self.notification_preview_active.push(ActiveNotification {
					id:         next_id,
					expires_at: now + Self::NOTIFICATION_PREVIEW_DURATION,
				});
			}
		}
		before != self.notification_preview_active.len()
			|| !self.notification_preview_active.is_empty()
			|| !self.notification_preview_queue.is_empty()
	}

	pub fn notification_preview(&self) -> Option<NotificationPreviewState> {
		if self.notification_preview_active.is_empty() && self.notification_preview_queue.is_empty() {
			return None;
		}
		let items = self
			.notification_preview_active
			.iter()
			.filter_map(|active| self.notifications.iter().find(|entry| entry.id == active.id))
			.map(|entry| NotificationPreviewItem {
				id:               entry.id,
				level:            entry.level,
				message:          entry.message.clone(),
				created_at_local: entry.created_at_local.clone(),
				read:             entry.read,
			})
			.collect::<Vec<_>>();
		Some(NotificationPreviewState {
			items,
			unread_total: self.unread_notification_count(),
			open_center_hint: self.notification_center_open_hint(),
		})
	}

	fn notification_center_open_hint(&self) -> String {
		let mut bindings = self.command_registry.binding_sequences_for_builtin(
			KeymapScope::ModeNormal,
			BuiltinCommand::Command(CommandCommand::Notifications),
		);
		if bindings.is_empty() {
			return ":notifications".to_string();
		}
		if bindings.len() == 1 {
			return bindings.remove(0);
		}
		format!("{} (+{})", bindings.remove(0), bindings.len())
	}

	pub fn unread_notification_count(&self) -> usize {
		self.notifications.iter().filter(|entry| !entry.read).count()
	}

	pub fn open_notification_center(&mut self) {
		self.close_key_hints();
		self.close_workspace_file_picker();
		self.close_command_palette();
		if self.notification_center.is_none() {
			self.notification_center = Some(NotificationCenterState::default());
		}
		self.mark_selected_notification_read();
	}

	pub fn close_notification_center(&mut self) { self.notification_center = None; }

	pub fn notification_center_open(&self) -> bool { self.notification_center.is_some() }

	pub fn notification_center_view(&self) -> Option<NotificationCenterView> {
		let center = self.notification_center.as_ref()?;
		let mut items = self
			.notifications
			.iter()
			.rev()
			.map(|entry| NotificationCenterItem {
				id:               entry.id,
				level:            entry.level,
				message:          entry.message.clone(),
				created_at_local: entry.created_at_local.clone(),
				read:             entry.read,
			})
			.collect::<Vec<_>>();
		if items.is_empty() {
			items.push(NotificationCenterItem {
				id:               0,
				level:            NotificationLevel::Info,
				message:          "No notifications".to_string(),
				created_at_local: String::new(),
				read:             true,
			});
		}
		let selected = center.selected.min(items.len().saturating_sub(1));
		Some(NotificationCenterView { selected, items, unread_total: self.unread_notification_count() })
	}

	pub fn move_notification_center_selection(&mut self, delta: isize) -> bool {
		let Some(center) = self.notification_center.as_mut() else {
			return false;
		};
		if self.notifications.is_empty() {
			center.selected = 0;
			return false;
		}
		let max_index = self.notifications.len().saturating_sub(1);
		let next = center.selected.saturating_add_signed(delta).min(max_index);
		let changed = next != center.selected;
		center.selected = next;
		self.mark_selected_notification_read();
		changed
	}

	pub fn delete_selected_notification(&mut self) -> bool {
		let Some(center) = self.notification_center.as_mut() else {
			return false;
		};
		if self.notifications.is_empty() {
			center.selected = 0;
			return false;
		}
		let selected_in_reverse = center.selected.min(self.notifications.len().saturating_sub(1));
		let index = self.notifications.len().saturating_sub(1).saturating_sub(selected_in_reverse);
		let removed = self.notifications.remove(index);
		self.notification_preview_queue.retain(|id| *id != removed.id);
		self.notification_preview_active.retain(|item| item.id != removed.id);
		if self.notifications.is_empty() {
			center.selected = 0;
		} else {
			center.selected = center.selected.min(self.notifications.len().saturating_sub(1));
		}
		self.mark_selected_notification_read();
		true
	}

	fn mark_selected_notification_read(&mut self) {
		let Some(center) = self.notification_center.as_ref() else {
			return;
		};
		if self.notifications.is_empty() {
			return;
		}
		let selected_in_reverse = center.selected.min(self.notifications.len().saturating_sub(1));
		let index = self.notifications.len().saturating_sub(1).saturating_sub(selected_in_reverse);
		if let Some(entry) = self.notifications.get_mut(index) {
			entry.read = true;
		}
	}

	pub fn register_plugin_command(&mut self, registration: PluginCommandRegistration) -> Result<(), String> {
		self.command_registry.register_plugin_command(registration)
	}

	pub fn command_palette(&self) -> Option<&CommandPaletteState> { self.command_palette.as_ref() }

	pub fn workspace_file_picker(&self) -> Option<&WorkspaceFilePickerState> {
		self.workspace_file_picker.as_ref()
	}

	pub fn workspace_root(&self) -> &Path { self.workspace_root.as_path() }

	pub fn set_workspace_root(&mut self, workspace_root: PathBuf) { self.workspace_root = workspace_root; }

	pub fn refresh_command_palette(&mut self) {
		if !self.is_command_mode() {
			self.command_palette = None;
			return;
		}
		let previous_palette_preview = self
			.command_palette
			.as_ref()
			.map(|palette| (palette.preview_title.clone(), palette.preview_lines.clone(), palette.preview_scroll))
			.unwrap_or_else(|| (String::new(), Vec::new(), 0));
		let previous_selected_file = self
			.command_palette
			.as_ref()
			.and_then(|palette| palette.items.get(palette.selected))
			.and_then(CommandPaletteItem::as_file)
			.map(|item| (item.absolute_path.clone(), item.relative_path.clone()));
		let (items, loading, showing_files) = if let Some((_, file_query)) = self
			.command_palette_path_argument_context()
			.map(|(command, query)| (command.to_string(), query.to_string()))
		{
			if self.workspace_file_cache_loading {
				(Vec::new(), true, true)
			} else {
				(self.command_palette_file_matches(file_query.as_str(), 512), false, true)
			}
		} else {
			let command_query = self.command_line.split_whitespace().next().unwrap_or_default();
			let items = self
				.command_registry
				.command_palette_matches(command_query, 512)
				.into_iter()
				.map(CommandPaletteItem::Command)
				.collect();
			(items, false, false)
		};
		let selected = if let Some((selected_path, selected_relative_path)) = previous_selected_file.as_ref() {
			items
				.iter()
				.position(|item| {
					item.as_file().is_some_and(|file| {
						&file.absolute_path == selected_path || &file.relative_path == selected_relative_path
					})
				})
				.unwrap_or_else(|| {
					self
						.command_palette
						.as_ref()
						.map(|palette| palette.selected.min(items.len().saturating_sub(1)))
						.unwrap_or_default()
				})
		} else {
			self
				.command_palette
				.as_ref()
				.map(|palette| palette.selected.min(items.len().saturating_sub(1)))
				.unwrap_or_default()
		};
		let selected_file_path =
			items.get(selected).and_then(CommandPaletteItem::as_file).map(|file| file.absolute_path.as_path());
		let (preview_title, preview_lines, preview_scroll) = if showing_files
			&& selected_file_path
				.zip(previous_selected_file.as_ref().map(|(path, _)| path.as_path()))
				.is_some_and(|(selected_path, previous_path)| selected_path == previous_path)
		{
			previous_palette_preview
		} else {
			(String::new(), Vec::new(), 0)
		};
		self.command_palette = Some(CommandPaletteState {
			query: self.command_line.clone(),
			items,
			selected,
			loading,
			showing_files,
			preview_title,
			preview_lines,
			preview_scroll,
		});
	}

	pub fn close_command_palette(&mut self) {
		self.command_palette = None;
		if matches!(
			self.overlay,
			Some(OverlayState::KeyHints(KeyHintsOverlayState { scope: KeymapScope::OverlayCommandPalette, .. }))
		) {
			self.overlay = None;
		}
	}

	pub fn has_workspace_file_cache(&self) -> bool { !self.workspace_file_cache.is_empty() }

	pub fn workspace_file_cache_is_loading(&self) -> bool { self.workspace_file_cache_loading }

	pub fn workspace_file_cache_entries(&self) -> &[WorkspaceFileEntry] { self.workspace_file_cache.as_slice() }

	pub fn command_palette_needs_workspace_files(&self) -> bool {
		self.command_palette_path_argument_context().is_some()
			&& self.workspace_file_cache.is_empty()
			&& !self.workspace_file_cache_loading
	}

	pub fn begin_workspace_file_cache_loading(&mut self) {
		self.workspace_file_cache_loading = true;
		self.refresh_command_palette();
	}

	pub fn set_workspace_file_cache(&mut self, entries: Vec<WorkspaceFileEntry>) {
		self.workspace_file_cache = entries;
		self.workspace_file_cache_loading = false;
		self.refresh_command_palette();
	}

	pub fn fail_workspace_file_cache_loading(&mut self) {
		self.workspace_file_cache_loading = false;
		self.refresh_command_palette();
	}

	pub fn open_workspace_file_picker(&mut self, entries: Vec<WorkspaceFileEntry>) {
		let total_files = entries.len();
		self.close_key_hints();
		self.close_command_palette();
		self.workspace_file_picker = Some(WorkspaceFilePickerState {
			query: String::new(),
			entries,
			items: Vec::new(),
			selected: 0,
			preferred: None,
			total_files,
			total_matches: 0,
			preview_title: String::new(),
			preview_lines: Vec::new(),
			preview_scroll: 0,
			loading: false,
		});
		self.refresh_workspace_file_picker_matches();
	}

	pub fn open_workspace_file_picker_loading(&mut self) {
		self.close_key_hints();
		self.close_command_palette();
		if let Some(picker) = self.workspace_file_picker.as_mut() {
			picker.loading = true;
			return;
		}
		self.workspace_file_picker = Some(WorkspaceFilePickerState {
			query:          String::new(),
			entries:        Vec::new(),
			items:          Vec::new(),
			selected:       0,
			preferred:      None,
			total_files:    0,
			total_matches:  0,
			preview_title:  String::new(),
			preview_lines:  vec!["Loading workspace files...".to_string()],
			preview_scroll: 0,
			loading:        true,
		});
	}

	pub fn close_workspace_file_picker(&mut self) {
		self.workspace_file_picker = None;
		if matches!(
			self.overlay,
			Some(OverlayState::KeyHints(KeyHintsOverlayState { scope: KeymapScope::OverlayPicker, .. }))
		) {
			self.overlay = None;
		}
	}

	pub fn workspace_file_picker_open(&self) -> bool { self.workspace_file_picker.is_some() }

	pub fn workspace_file_picker_loading(&self) -> bool {
		self.workspace_file_picker.as_ref().is_some_and(|picker| picker.loading)
	}

	pub fn refresh_workspace_file_picker_matches(&mut self) {
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			return;
		};
		let previous_selected_path = picker.preferred.clone().or_else(|| {
			picker.items.get(picker.selected).map(|item| (item.absolute_path.clone(), item.relative_path.clone()))
		});
		let query = picker.query.trim();
		picker.loading = false;
		let mut matches = picker
			.entries
			.iter()
			.filter_map(|entry| {
				if query.is_empty() {
					return Some((0u16, WorkspaceFileMatch {
						absolute_path: entry.absolute_path.clone(),
						relative_path: entry.relative_path.clone(),
						match_indices: Vec::new(),
					}));
				}
				let (score, mut indices) = workspace_file_match(query, entry.relative_path.as_str())?;
				indices.sort_unstable();
				Some((score, WorkspaceFileMatch {
					absolute_path: entry.absolute_path.clone(),
					relative_path: entry.relative_path.clone(),
					match_indices: indices,
				}))
			})
			.collect::<Vec<_>>();
		matches.sort_by(|left, right| {
			right.0.cmp(&left.0).then_with(|| left.1.relative_path.cmp(&right.1.relative_path))
		});
		picker.total_matches = matches.len();
		picker.items = matches.into_iter().take(512).map(|(_, item)| item).collect();
		let restored_selected =
			previous_selected_path.clone().and_then(|(selected_path, selected_relative_path)| {
				picker.items.iter().position(|item| {
					item.absolute_path == selected_path || item.relative_path == selected_relative_path
				})
			});
		picker.selected =
			restored_selected.unwrap_or_else(|| picker.selected.min(picker.items.len().saturating_sub(1)));
		picker.preferred = if restored_selected.is_some() {
			picker
				.items
				.get(picker.selected)
				.map(|item| (item.absolute_path.clone(), item.relative_path.clone()))
				.or(previous_selected_path)
		} else {
			previous_selected_path
		};
		if picker.items.is_empty() {
			picker.preview_title.clear();
			picker.preview_lines.clear();
			picker.preview_scroll = 0;
		}
	}

	pub fn replace_workspace_file_picker_entries(&mut self, entries: Vec<WorkspaceFileEntry>) {
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			self.open_workspace_file_picker(entries);
			return;
		};
		picker.entries = entries;
		picker.total_files = picker.entries.len();
		self.refresh_workspace_file_picker_matches();
	}

	pub fn move_workspace_file_picker_selection(&mut self, delta: isize) -> bool {
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			return false;
		};
		if picker.items.is_empty() {
			return false;
		}
		let next = picker.selected.saturating_add_signed(delta).min(picker.items.len().saturating_sub(1));
		let changed = next != picker.selected;
		picker.selected = next;
		picker.preferred =
			picker.items.get(picker.selected).map(|item| (item.absolute_path.clone(), item.relative_path.clone()));
		if changed {
			picker.preview_scroll = 0;
		}
		changed
	}

	pub fn push_workspace_file_picker_char(&mut self, ch: char) {
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			return;
		};
		picker.query.push(ch);
		picker.selected = 0;
		picker.preferred = None;
		picker.preview_scroll = 0;
		self.refresh_workspace_file_picker_matches();
	}

	pub fn pop_workspace_file_picker_char(&mut self) {
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			return;
		};
		let _ = picker.query.pop();
		picker.selected = 0;
		picker.preferred = None;
		picker.preview_scroll = 0;
		self.refresh_workspace_file_picker_matches();
	}

	pub fn selected_workspace_file_picker_path(&self) -> Option<&Path> {
		let picker = self.workspace_file_picker.as_ref()?;
		let selected = picker.items.get(picker.selected)?;
		Some(selected.absolute_path.as_path())
	}

	pub fn set_workspace_file_picker_preview(&mut self, path: &Path, preview: String) {
		let content_width = self.active_tab_content_size().0;
		let preview_width = compute_workspace_file_picker_body_layout(content_width).preview_width as usize;
		let word_wrap = self.picker_preview_word_wrap;
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			return;
		};
		let selected = picker.items.get(picker.selected).map(|item| item.absolute_path.as_path());
		if selected != Some(path) {
			return;
		}
		let previous_scroll = picker.preview_scroll;
		let next_preview_lines = if preview.is_empty() {
			vec!["<empty>".to_string()]
		} else {
			preview.lines().map(ToString::to_string).collect::<Vec<_>>()
		};
		let next_max_scroll =
			preview_max_scroll_with_mode(next_preview_lines.as_slice(), preview_width, word_wrap);
		picker.preview_title = path
			.file_name()
			.map(|name| name.to_string_lossy().to_string())
			.unwrap_or_else(|| path.display().to_string());
		picker.preview_lines = next_preview_lines;
		picker.preview_scroll = previous_scroll.min(next_max_scroll);
	}

	pub fn set_workspace_file_picker_preview_loading(&mut self, path: &Path) {
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			return;
		};
		let selected = picker.items.get(picker.selected).map(|item| item.absolute_path.as_path());
		if selected != Some(path) {
			return;
		}
		picker.preview_title = path
			.file_name()
			.map(|name| name.to_string_lossy().to_string())
			.unwrap_or_else(|| path.display().to_string());
	}

	pub fn clear_workspace_file_picker_preview(&mut self) {
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			return;
		};
		picker.preview_title.clear();
		picker.preview_lines.clear();
		picker.preview_scroll = 0;
	}

	pub fn scroll_workspace_file_picker_preview(&mut self, delta: isize) -> bool {
		let content_width = self.active_tab_content_size().0;
		let preview_width = compute_workspace_file_picker_body_layout(content_width).preview_width as usize;
		let word_wrap = self.picker_preview_word_wrap;
		let Some(picker) = self.workspace_file_picker.as_mut() else {
			return false;
		};
		let max_scroll = preview_max_scroll_with_mode(picker.preview_lines.as_slice(), preview_width, word_wrap);
		let next = picker.preview_scroll.saturating_add_signed(delta).min(max_scroll);
		let changed = next != picker.preview_scroll;
		picker.preview_scroll = next;
		changed
	}

	pub fn move_command_palette_selection(&mut self, delta: isize) -> bool {
		let Some(palette) = self.command_palette.as_mut() else {
			return false;
		};
		if palette.items.is_empty() {
			return false;
		}
		let next = palette.selected.saturating_add_signed(delta).min(palette.items.len().saturating_sub(1));
		let changed = next != palette.selected;
		palette.selected = next;
		changed
	}

	pub fn scroll_command_palette_preview(&mut self, delta: isize) -> bool {
		let content_width = self.active_tab_content_size().0;
		let preview_width = compute_command_palette_preview_width(content_width) as usize;
		let word_wrap = self.picker_preview_word_wrap;
		let Some(palette) = self.command_palette.as_mut() else {
			return false;
		};
		if !palette.showing_files {
			return false;
		}
		let max_scroll = preview_max_scroll_with_mode(palette.preview_lines.as_slice(), preview_width, word_wrap);
		let next = palette.preview_scroll.saturating_add_signed(delta).min(max_scroll);
		let changed = next != palette.preview_scroll;
		palette.preview_scroll = next;
		changed
	}

	fn active_tab_content_size(&self) -> (u16, u16) {
		let Some(tab) = self.tabs.get(&self.active_tab) else {
			return (1, 1);
		};
		let max_right = tab
			.windows
			.iter()
			.filter_map(|id| self.windows.get(*id))
			.map(|window| window.x.saturating_add(window.width))
			.max()
			.unwrap_or(1)
			.max(1);
		let max_bottom = tab
			.windows
			.iter()
			.filter_map(|id| self.windows.get(*id))
			.map(|window| window.y.saturating_add(window.height))
			.max()
			.unwrap_or(1)
			.max(1);
		(max_right, max_bottom)
	}

	pub fn page_command_palette_selection(&mut self, direction: isize) -> bool {
		let step = 10isize;
		if direction.is_negative() {
			self.move_command_palette_selection(-step)
		} else {
			self.move_command_palette_selection(step)
		}
	}

	pub fn word_wrap_enabled(&self) -> bool { self.word_wrap }

	pub fn toggle_word_wrap(&mut self) {
		self.word_wrap = !self.word_wrap;
		for window_id in self.active_tab_window_ids() {
			if let Some(window) = self.windows.get_mut(window_id) {
				window.scroll_x = 0;
			}
		}
		self.align_active_window_scroll_to_cursor();
		self.status_bar.message =
			if self.word_wrap { "word wrap enabled".to_string() } else { "word wrap disabled".to_string() };
	}

	pub fn picker_preview_word_wrap_enabled(&self) -> bool { self.picker_preview_word_wrap }

	pub fn toggle_picker_preview_word_wrap(&mut self) {
		self.picker_preview_word_wrap = !self.picker_preview_word_wrap;
		let content_width = self.active_tab_content_size().0;
		let picker_preview_width =
			compute_workspace_file_picker_body_layout(content_width).preview_width as usize;
		let command_preview_width = compute_command_palette_preview_width(content_width) as usize;
		if let Some(picker) = self.workspace_file_picker.as_mut() {
			let max_scroll = preview_max_scroll_with_mode(
				picker.preview_lines.as_slice(),
				picker_preview_width,
				self.picker_preview_word_wrap,
			);
			picker.preview_scroll = picker.preview_scroll.min(max_scroll);
		}
		if let Some(palette) = self.command_palette.as_mut()
			&& palette.showing_files
		{
			let max_scroll = preview_max_scroll_with_mode(
				palette.preview_lines.as_slice(),
				command_preview_width,
				self.picker_preview_word_wrap,
			);
			palette.preview_scroll = palette.preview_scroll.min(max_scroll);
		}
		self.status_bar.message = if self.picker_preview_word_wrap {
			"picker preview wrap enabled".to_string()
		} else {
			"picker preview wrap disabled".to_string()
		};
	}

	pub fn selected_command_palette_match(&self) -> Option<&CommandPaletteMatch> {
		let palette = self.command_palette.as_ref()?;
		palette.items.get(palette.selected)?.as_command()
	}

	pub fn selected_command_palette_file_match(&self) -> Option<&CommandPaletteFileMatch> {
		let palette = self.command_palette.as_ref()?;
		palette.items.get(palette.selected)?.as_file()
	}

	pub fn selected_command_palette_file_path(&self) -> Option<&Path> {
		self.selected_command_palette_file_match().map(|item| item.absolute_path.as_path())
	}

	pub fn command_palette_showing_files(&self) -> bool {
		self.command_palette.as_ref().is_some_and(|palette| palette.showing_files)
	}

	pub fn set_command_palette_preview(&mut self, path: &Path, preview: String) {
		let content_width = self.active_tab_content_size().0;
		let preview_width = compute_command_palette_preview_width(content_width) as usize;
		let word_wrap = self.picker_preview_word_wrap;
		let Some(palette) = self.command_palette.as_mut() else {
			return;
		};
		if !palette.showing_files {
			return;
		}
		let selected = palette.items.get(palette.selected).and_then(CommandPaletteItem::as_file);
		if selected.map(|item| item.absolute_path.as_path()) != Some(path) {
			return;
		}
		let previous_scroll = palette.preview_scroll;
		let next_preview_lines = if preview.is_empty() {
			vec!["<empty>".to_string()]
		} else {
			preview.lines().map(ToString::to_string).collect::<Vec<_>>()
		};
		let next_max_scroll =
			preview_max_scroll_with_mode(next_preview_lines.as_slice(), preview_width, word_wrap);
		palette.preview_title = path
			.file_name()
			.map(|name| name.to_string_lossy().to_string())
			.unwrap_or_else(|| path.display().to_string());
		palette.preview_lines = next_preview_lines;
		palette.preview_scroll = previous_scroll.min(next_max_scroll);
	}

	pub fn set_command_palette_preview_loading(&mut self, path: &Path) {
		let Some(palette) = self.command_palette.as_mut() else {
			return;
		};
		if !palette.showing_files {
			return;
		}
		let selected = palette.items.get(palette.selected).and_then(CommandPaletteItem::as_file);
		if selected.map(|item| item.absolute_path.as_path()) != Some(path) {
			return;
		}
		palette.preview_title = path
			.file_name()
			.map(|name| name.to_string_lossy().to_string())
			.unwrap_or_else(|| path.display().to_string());
	}

	pub fn complete_command_palette_selection(&mut self) -> bool {
		let Some(palette) = self.command_palette.as_ref() else {
			return false;
		};
		let Some(item) = palette.items.get(palette.selected) else {
			return false;
		};
		match item {
			CommandPaletteItem::Command(item) => {
				let completion = if item.name.is_empty() { item.command_id_label.clone() } else { item.name.clone() };
				self.command_line = completion;
				self.refresh_command_palette();
				true
			}
			CommandPaletteItem::File(item) => {
				let Some((command, _)) = self.command_line.split_once(' ') else {
					return false;
				};
				self.command_line = format!("{} {}", command, item.relative_path);
				self.refresh_command_palette();
				true
			}
		}
	}

	pub fn active_keymap_scope(&self) -> KeymapScope {
		if let Some(OverlayState::KeyHints(overlay)) = self.overlay.as_ref() {
			overlay.scope
		} else if self.workspace_file_picker_open() {
			KeymapScope::OverlayPicker
		} else if self.notification_center_open() {
			KeymapScope::OverlayNotificationCenter
		} else if self.command_palette().is_some() {
			KeymapScope::OverlayCommandPalette
		} else if self.is_visual_mode() {
			KeymapScope::ModeVisual
		} else if self.is_command_mode() {
			KeymapScope::ModeCommand
		} else if self.is_insert_mode() {
			KeymapScope::ModeInsert
		} else {
			KeymapScope::ModeNormal
		}
	}

	pub fn floating_window(&self) -> Option<&FloatingWindowState> {
		match self.overlay.as_ref() {
			Some(OverlayState::KeyHints(overlay)) => Some(&overlay.window),
			Some(OverlayState::FloatingWindow(window)) => Some(window),
			None => None,
		}
	}

	pub fn close_key_hints(&mut self) {
		if matches!(self.overlay, Some(OverlayState::KeyHints(_))) {
			self.overlay = None;
		}
	}

	pub fn key_hints_open(&self) -> bool { matches!(self.overlay, Some(OverlayState::KeyHints(_))) }

	pub fn open_key_hints_overview(&mut self) { self.open_key_hints(Vec::new(), true); }

	pub fn scroll_key_hints_up(&mut self) -> bool { self.scroll_overlay_lines(-1) }

	pub fn scroll_key_hints_down(&mut self) -> bool { self.scroll_overlay_lines(1) }

	pub fn scroll_key_hints_half_page_up(&mut self) -> bool { self.scroll_overlay_half_page(-1) }

	pub fn scroll_key_hints_half_page_down(&mut self) -> bool { self.scroll_overlay_half_page(1) }

	pub fn refresh_key_hints_overlay_after_config_reload(&mut self) {
		let Some(OverlayState::KeyHints(overlay)) = self.overlay.as_ref() else {
			return;
		};
		self.open_key_hints(overlay.prefix.clone(), overlay.overview);
	}

	pub fn refresh_pending_key_hints(&mut self) {
		if self.normal_sequence.is_empty() {
			self.close_key_hints();
			return;
		}
		self.open_key_hints(self.normal_sequence.clone(), false);
	}

	fn open_key_hints(&mut self, prefix: Vec<NormalSequenceKey>, overview: bool) {
		let scope = self.active_keymap_scope();
		let lines = self.command_registry.key_hints(scope, prefix.as_slice());
		if lines.is_empty() {
			self.close_key_hints();
			return;
		}
		let title = if overview {
			format!("{} keymap", self.active_keymap_scope_label(scope))
		} else {
			format!("{} {}", self.active_keymap_scope_label(scope), self.render_sequence(prefix.as_slice()))
		};
		let subtitle = Some("Scrollable".to_string());
		let height = lines.len().saturating_add(4).min(self.key_hints_max_height as usize) as u16;
		let window = FloatingWindowState {
			title,
			subtitle,
			footer: Some("Esc close  Backspace back".to_string()),
			placement: FloatingWindowPlacement::BottomRight {
				width: self.key_hints_width,
				height,
				margin_right: 1,
				margin_bottom: 1,
			},
			lines,
			scroll: 0,
		};
		self.overlay = Some(OverlayState::KeyHints(KeyHintsOverlayState { scope, prefix, overview, window }));
	}

	fn active_keymap_scope_label(&self, scope: KeymapScope) -> &'static str {
		match scope {
			KeymapScope::ModeNormal => "NORMAL",
			KeymapScope::ModeVisual => "VISUAL",
			KeymapScope::ModeCommand => "COMMAND",
			KeymapScope::ModeInsert => "INSERT",
			KeymapScope::OverlayWhichKey => "WHICHKEY",
			KeymapScope::OverlayCommandPalette => "COMMAND",
			KeymapScope::OverlayPicker => "PICKER",
			KeymapScope::OverlayNotificationCenter => "NOTIFICATIONS",
		}
	}

	fn render_sequence(&self, keys: &[NormalSequenceKey]) -> String {
		keys
			.iter()
			.map(|key| match key {
				NormalSequenceKey::Leader => "<leader>".to_string(),
				NormalSequenceKey::Tab => "<Tab>".to_string(),
				NormalSequenceKey::Esc => "<Esc>".to_string(),
				NormalSequenceKey::Enter => "<Enter>".to_string(),
				NormalSequenceKey::Backspace => "<Backspace>".to_string(),
				NormalSequenceKey::F1 => "<F1>".to_string(),
				NormalSequenceKey::Left => "<Left>".to_string(),
				NormalSequenceKey::Right => "<Right>".to_string(),
				NormalSequenceKey::Up => "<Up>".to_string(),
				NormalSequenceKey::Down => "<Down>".to_string(),
				NormalSequenceKey::Char(ch) => ch.to_string(),
				NormalSequenceKey::Ctrl(ch) => format!("<C-{}>", ch),
			})
			.collect::<Vec<_>>()
			.join("")
	}

	fn command_palette_path_argument_context(&self) -> Option<(&str, &str)> {
		let (command, tail) = self.command_line.split_once(' ')?;
		let resolved = self.command_registry.resolve_command_input(command)?;
		if resolved.spec.arg_kind != CommandArgKind::OptionalPath {
			return None;
		}
		Some((command, tail.trim_start()))
	}

	fn command_palette_file_matches(&self, query: &str, limit: usize) -> Vec<CommandPaletteItem> {
		let mut matches = self
			.workspace_file_cache
			.iter()
			.filter_map(|entry| {
				if query.is_empty() {
					return Some((0u16, CommandPaletteFileMatch {
						relative_path: entry.relative_path.clone(),
						absolute_path: entry.absolute_path.clone(),
						match_indices: Vec::new(),
					}));
				}
				let (score, mut indices) = workspace_file_match(query, entry.relative_path.as_str())?;
				indices.sort_unstable();
				Some((score, CommandPaletteFileMatch {
					relative_path: entry.relative_path.clone(),
					absolute_path: entry.absolute_path.clone(),
					match_indices: indices,
				}))
			})
			.collect::<Vec<_>>();

		matches.sort_by(|left, right| {
			right.0.cmp(&left.0).then_with(|| left.1.relative_path.cmp(&right.1.relative_path))
		});
		matches.into_iter().take(limit).map(|(_, item)| CommandPaletteItem::File(item)).collect()
	}

	pub fn step_back_key_hint_prefix(&mut self) -> bool {
		if self.normal_sequence.pop().is_none() {
			self.close_key_hints();
			return false;
		}
		self.status_bar.key_sequence = self.render_sequence(self.normal_sequence.as_slice());
		self.refresh_pending_key_hints();
		true
	}

	fn scroll_overlay_lines(&mut self, delta: isize) -> bool {
		let Some(window) = self.floating_window_mut() else {
			return false;
		};
		let body_rows = window.visible_body_rows();
		if body_rows == 0 {
			return false;
		}
		let max_scroll = window.max_scroll();
		let next_scroll = if delta.is_negative() {
			window.scroll.saturating_sub(delta.unsigned_abs())
		} else {
			window.scroll.saturating_add(delta as usize).min(max_scroll)
		};
		if next_scroll == window.scroll {
			return false;
		}
		window.scroll = next_scroll;
		true
	}

	fn scroll_overlay_half_page(&mut self, direction: isize) -> bool {
		let Some(window) = self.floating_window_mut() else {
			return false;
		};
		let step = (window.visible_body_rows() / 2).max(1) as isize;
		if direction.is_negative() { self.scroll_overlay_lines(-step) } else { self.scroll_overlay_lines(step) }
	}

	fn floating_window_mut(&mut self) -> Option<&mut FloatingWindowState> {
		match self.overlay.as_mut() {
			Some(OverlayState::KeyHints(overlay)) => Some(&mut overlay.window),
			Some(OverlayState::FloatingWindow(window)) => Some(window),
			None => None,
		}
	}

	pub fn create_window(&mut self, buffer_id: Option<BufferId>) -> Option<WindowId> {
		if let Some(buffer_id) = buffer_id
			&& !self.buffers.contains_key(buffer_id)
		{
			error!("create_window failed: buffer {:?} not found", buffer_id);
			return None;
		}

		let id = self.windows.insert(WindowState { buffer_id, ..WindowState::default() });
		if let Some(buffer_id) = buffer_id {
			self.window_buffer_views.insert((id, buffer_id), WindowBufferViewState::default());
		}
		Some(id)
	}

	pub fn active_buffer_id(&self) -> Option<BufferId> {
		self.windows.get(self.active_window_id()).and_then(|window| window.buffer_id)
	}

	pub fn bind_buffer_to_active_window(&mut self, buffer_id: BufferId) {
		let active_window_id = self.active_window_id();
		self.bind_buffer_to_window(active_window_id, buffer_id, true);
	}

	pub(crate) fn bind_buffer_to_window(
		&mut self,
		window_id: WindowId,
		buffer_id: BufferId,
		persist_previous_cursor: bool,
	) {
		let Some(window_snapshot) = self.windows.get(window_id).copied() else {
			return;
		};
		let previous_buffer_id = window_snapshot.buffer_id;
		if persist_previous_cursor && let Some(previous_buffer_id) = window_snapshot.buffer_id {
			self.window_buffer_views.insert((window_id, previous_buffer_id), WindowBufferViewState {
				cursor:   window_snapshot.cursor,
				scroll_x: window_snapshot.scroll_x,
				scroll_y: window_snapshot.scroll_y,
			});
		}

		let restored_view = self.window_buffer_views.get(&(window_id, buffer_id)).copied().unwrap_or_default();
		let next_cursor = self
			.buffers
			.get(buffer_id)
			.map(|buffer| clamp_cursor_for_rope(&buffer.text, restored_view.cursor))
			.unwrap_or(restored_view.cursor);
		if let Some(window) = self.windows.get_mut(window_id) {
			window.buffer_id = Some(buffer_id);
			window.cursor = next_cursor;
			window.scroll_x = restored_view.scroll_x;
			window.scroll_y = restored_view.scroll_y;
		}
		self.window_buffer_views.insert((window_id, buffer_id), WindowBufferViewState {
			cursor:   next_cursor,
			scroll_x: restored_view.scroll_x,
			scroll_y: restored_view.scroll_y,
		});
		if let Some(tab_id) = self.tab_id_for_window(window_id) {
			self.register_buffer_in_tab_order(tab_id, buffer_id, previous_buffer_id);
		}
	}

	pub(crate) fn sync_window_view_binding(&mut self, window_id: WindowId) {
		let Some(window) = self.windows.get(window_id) else {
			return;
		};
		let Some(buffer_id) = window.buffer_id else {
			return;
		};
		self.window_buffer_views.insert((window_id, buffer_id), WindowBufferViewState {
			cursor:   window.cursor,
			scroll_x: window.scroll_x,
			scroll_y: window.scroll_y,
		});
	}

	pub(crate) fn remove_window_view_bindings(&mut self, window_id: WindowId) {
		self.window_buffer_views.retain(|(candidate_window_id, _), _| *candidate_window_id != window_id);
	}
}

impl Default for RimState {
	fn default() -> Self { Self::new() }
}

fn buffer_name_from_path(path: &Path) -> Option<String> {
	path.file_name().map(|name| name.to_string_lossy().to_string())
}

pub(crate) fn rope_line_count(text: &Rope) -> usize {
	let line_count = text.len_lines();
	if line_count == 0 {
		return 1;
	}
	if rope_ends_with_newline(text) { line_count.saturating_sub(1).max(1) } else { line_count.max(1) }
}

pub(crate) fn rope_is_empty(text: &Rope) -> bool { text.len_chars() == 0 }

pub(crate) fn rope_line_without_newline(text: &Rope, row_index: usize) -> Option<String> {
	if row_index >= rope_line_count(text) {
		return None;
	}
	let mut line = text.line(row_index).to_string();
	if line.ends_with('\n') {
		line.pop();
		if line.ends_with('\r') {
			line.pop();
		}
	}
	Some(line)
}

pub(crate) fn rope_line_len_chars(text: &Rope, row_index: usize) -> usize {
	rope_line_without_newline(text, row_index).map(|line| line.chars().count()).unwrap_or(0)
}

pub(crate) fn rope_ends_with_newline(text: &Rope) -> bool {
	text.len_chars() > 0 && text.char(text.len_chars().saturating_sub(1)) == '\n'
}

pub(crate) fn clamp_cursor_for_rope(text: &Rope, cursor: CursorState) -> CursorState {
	let max_row = rope_line_count(text) as u16;
	let row = cursor.row.min(max_row).max(1);
	let row_index = row.saturating_sub(1) as usize;
	let max_col = rope_line_len_chars(text, row_index).max(1) as u16;
	let col = cursor.col.min(max_col).max(1);
	CursorState { row, col }
}

fn apply_text_delta_undo(text: &mut Rope, delta: &BufferEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let inserted_end_byte = delta.start_byte.saturating_add(delta.inserted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(inserted_end_byte);
	text.remove(start_char..end_char);
	text.insert(start_char, delta.deleted_text.as_str());
}

fn apply_text_delta_redo(text: &mut Rope, delta: &BufferEditSnapshot) {
	let start_char = text.byte_to_char(delta.start_byte.min(text.len_bytes()));
	let deleted_end_byte = delta.start_byte.saturating_add(delta.deleted_text.len()).min(text.len_bytes());
	let end_char = text.byte_to_char(deleted_end_byte);
	text.remove(start_char..end_char);
	text.insert(start_char, delta.inserted_text.as_str());
}

fn merge_adjacent_insert_history_edits(
	last_edit: &mut BufferEditSnapshot,
	next_edit: &BufferEditSnapshot,
) -> bool {
	if !last_edit.deleted_text.is_empty() || !next_edit.deleted_text.is_empty() {
		return false;
	}
	let expected_start = last_edit.start_byte.saturating_add(last_edit.inserted_text.len());
	if next_edit.start_byte != expected_start {
		return false;
	}
	last_edit.inserted_text.push_str(next_edit.inserted_text.as_str());
	true
}

pub(crate) fn compute_rope_text_diff(before: &Rope, after: &Rope) -> Option<RopeTextDiff> {
	if before == after {
		return None;
	}

	let mut common_prefix_chars = 0usize;
	let mut common_prefix_bytes = 0usize;
	for (before_ch, after_ch) in before.chars().zip(after.chars()) {
		if before_ch != after_ch {
			break;
		}
		common_prefix_chars = common_prefix_chars.saturating_add(1);
		common_prefix_bytes = common_prefix_bytes.saturating_add(before_ch.len_utf8());
	}

	let before_len_chars = before.len_chars();
	let after_len_chars = after.len_chars();
	let mut common_suffix_chars = 0usize;
	let mut before_mid_end = before_len_chars;
	let mut after_mid_end = after_len_chars;
	while before_mid_end > common_prefix_chars && after_mid_end > common_prefix_chars {
		if before.char(before_mid_end.saturating_sub(1)) != after.char(after_mid_end.saturating_sub(1)) {
			break;
		}
		common_suffix_chars = common_suffix_chars.saturating_add(1);
		before_mid_end = before_mid_end.saturating_sub(1);
		after_mid_end = after_mid_end.saturating_sub(1);
	}

	Some(RopeTextDiff {
		start_char:    common_prefix_chars,
		start_byte:    common_prefix_bytes,
		deleted_text:  before.slice(common_prefix_chars..before_mid_end).to_string(),
		inserted_text: after.slice(common_prefix_chars..after_mid_end).to_string(),
	})
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDirection {
	Left,
	Down,
	Up,
	Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSwitchDirection {
	Prev,
	Next,
}

fn workspace_file_match(query: &str, haystack: &str) -> Option<(u16, Vec<usize>)> {
	let config = FrizbeeConfig::default();
	let matched = match_list_indices(query, &[haystack], &config).into_iter().next()?;
	let mut indices = matched.indices;
	indices.sort_unstable();
	Some((matched.score, indices))
}

#[cfg(test)]
mod tests;
