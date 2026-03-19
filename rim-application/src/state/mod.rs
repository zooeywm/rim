use std::{collections::{HashMap, HashSet, VecDeque}, fmt, ops::{Deref, DerefMut}, path::{Path, PathBuf}, time::{Duration, Instant, SystemTime}};

use frizbee::{Config as FrizbeeConfig, match_list_indices};
use rim_domain::preview::preview_max_scroll_with_mode;
pub use rim_domain::{editor::{EditorOperationError, EditorState}, model::{BufferEditSnapshot, BufferHistoryEntry, BufferId, BufferState, BufferSwitchDirection, CursorState, EditorMode, FocusDirection, PendingBlockInsert, PendingInsertUndoGroup, PersistedBufferHistory, RopeTextDiff, SplitAxis, TabId, TabState, WindowBufferViewState, WindowId, WindowState, WorkspaceBufferHistorySnapshot, WorkspaceBufferSnapshot, WorkspaceSessionSnapshot, WorkspaceTabSnapshot, WorkspaceWindowBufferViewSnapshot, WorkspaceWindowSnapshot}};
use rim_ports::PluginRegistration;
use time::{OffsetDateTime, format_description::FormatItem, macros::format_description};

use crate::{command::{BuiltinCommand, CommandArgKind, CommandCommand, CommandConfigError, CommandConfigFile, CommandPaletteFileMatch, CommandPaletteItem, CommandPaletteMatch, CommandRegistry, PluginCommandRegistration}, defaults};

mod buffer;
mod edit;
mod mode;
mod plugin;
mod session;
mod tab;
mod window;

pub(crate) use rim_domain::text::{buffer_name_from_path, compute_rope_text_diff, rope_line_count, rope_line_without_newline};

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

#[derive(Debug, Clone)]
pub struct PendingSwapDecision {
	pub buffer_id:      BufferId,
	pub source_path:    PathBuf,
	pub base_text:      String,
	pub owner_pid:      u32,
	pub owner_username: String,
}

#[derive(Debug)]
pub struct WorkbenchState {
	pub title:                                 String,
	pub workspace_root:                        PathBuf,
	pub plugins:                               Vec<PluginRegistration>,
	pub leader_key:                            char,
	pub command_line:                          String,
	pub quit_after_save:                       bool,
	pub force_quit_trim_file_dirty_in_session: bool,
	pub pending_save_path:                     Option<(BufferId, PathBuf)>,
	pub cursor_scroll_threshold:               u16,
	pub key_hints_width:                       u16,
	pub key_hints_max_height:                  u16,
	pub word_wrap:                             bool,
	pub picker_preview_word_wrap:              bool,
	pub normal_sequence:                       Vec<NormalSequenceKey>,
	pub visual_g_pending:                      bool,
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
	pub status_bar:                            StatusBarState,
}

impl WorkbenchState {
	pub fn new() -> Self {
		let default_editor = defaults::default_editor_config();
		Self {
			title:                                 "Rim".to_string(),
			workspace_root:                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
			plugins:                               Vec::new(),
			leader_key:                            default_editor.editor.leader_key,
			command_line:                          String::new(),
			quit_after_save:                       false,
			force_quit_trim_file_dirty_in_session: false,
			pending_save_path:                     None,
			cursor_scroll_threshold:               default_editor.editor.cursor_scroll_threshold,
			key_hints_width:                       default_editor.editor.key_hints_width,
			key_hints_max_height:                  default_editor.editor.key_hints_max_height,
			word_wrap:                             false,
			picker_preview_word_wrap:              true,
			normal_sequence:                       Vec::new(),
			visual_g_pending:                      false,
			pending_swap_decision:                 None,
			in_flight_internal_saves:              HashSet::new(),
			ignore_external_change_until:          HashMap::new(),
			command_registry:                      CommandRegistry::with_defaults(),
			overlay:                               None,
			command_palette:                       None,
			workspace_file_picker:                 None,
			notification_center:                   None,
			notifications:                         Vec::new(),
			notification_preview_active:           Vec::new(),
			notification_preview_queue:            VecDeque::new(),
			next_notification_id:                  1,
			workspace_file_cache:                  Vec::new(),
			workspace_file_cache_loading:          false,
			status_bar:                            StatusBarState::default(),
		}
	}
}

impl Default for WorkbenchState {
	fn default() -> Self { Self::new() }
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
	pub editor:    EditorState,
	pub workbench: WorkbenchState,
}

impl Deref for RimState {
	type Target = EditorState;

	fn deref(&self) -> &Self::Target { &self.editor }
}

impl DerefMut for RimState {
	fn deref_mut(&mut self) -> &mut Self::Target { &mut self.editor }
}

impl RimState {
	const INTERNAL_SAVE_WATCHER_IGNORE_WINDOW: Duration = Duration::from_millis(750);
	const NOTIFICATION_PREVIEW_CAPACITY: usize = 5;
	const NOTIFICATION_PREVIEW_DURATION: Duration = Duration::from_secs(3);

	pub fn new() -> Self { Self { editor: EditorState::new(), workbench: WorkbenchState::new() } }

	pub fn apply_command_config(&mut self, config: &CommandConfigFile) -> Vec<CommandConfigError> {
		let errors = self.workbench.command_registry.apply_config(config);
		self.rebuild_plugin_command_registry_entries();
		errors
	}

	pub fn push_notification(&mut self, level: NotificationLevel, message: impl Into<String>) {
		let message = message.into();
		let now = Instant::now();
		let id = self.workbench.next_notification_id;
		self.workbench.next_notification_id = self.workbench.next_notification_id.saturating_add(1);
		let created_at = SystemTime::now();
		let created_at_local = format_local_timestamp(created_at);
		self.workbench.notifications.push(NotificationEntry {
			id,
			level,
			message,
			created_at,
			created_at_local,
			read: false,
		});
		self.workbench.notification_preview_queue.push_back(id);
		self.tick_notifications(now);
	}

	pub fn tick_notifications(&mut self, now: Instant) -> bool {
		let before = self.workbench.notification_preview_active.len();
		self.workbench.notification_preview_active.retain(|item| item.expires_at > now);
		while self.workbench.notification_preview_active.len() < Self::NOTIFICATION_PREVIEW_CAPACITY {
			let Some(next_id) = self.workbench.notification_preview_queue.pop_front() else {
				break;
			};
			if self.workbench.notifications.iter().any(|entry| entry.id == next_id) {
				self.workbench.notification_preview_active.push(ActiveNotification {
					id:         next_id,
					expires_at: now + Self::NOTIFICATION_PREVIEW_DURATION,
				});
			}
		}
		before != self.workbench.notification_preview_active.len()
			|| !self.workbench.notification_preview_active.is_empty()
			|| !self.workbench.notification_preview_queue.is_empty()
	}

	pub fn notification_preview(&self) -> Option<NotificationPreviewState> {
		if self.workbench.notification_preview_active.is_empty()
			&& self.workbench.notification_preview_queue.is_empty()
		{
			return None;
		}
		let items = self
			.workbench
			.notification_preview_active
			.iter()
			.filter_map(|active| self.workbench.notifications.iter().find(|entry| entry.id == active.id))
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
		let mut bindings = self.workbench.command_registry.binding_sequences_for_builtin(
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
		self.workbench.notifications.iter().filter(|entry| !entry.read).count()
	}

	pub fn open_notification_center(&mut self) {
		self.close_key_hints();
		self.close_workspace_file_picker();
		self.close_command_palette();
		if self.workbench.notification_center.is_none() {
			self.workbench.notification_center = Some(NotificationCenterState::default());
		}
		self.mark_selected_notification_read();
	}

	pub fn close_notification_center(&mut self) { self.workbench.notification_center = None; }

	pub fn notification_center_open(&self) -> bool { self.workbench.notification_center.is_some() }

	pub fn notification_center_view(&self) -> Option<NotificationCenterView> {
		let center = self.workbench.notification_center.as_ref()?;
		let mut items = self
			.workbench
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
		let Some(center) = self.workbench.notification_center.as_mut() else {
			return false;
		};
		if self.workbench.notifications.is_empty() {
			center.selected = 0;
			return false;
		}
		let max_index = self.workbench.notifications.len().saturating_sub(1);
		let next = center.selected.saturating_add_signed(delta).min(max_index);
		let changed = next != center.selected;
		center.selected = next;
		self.mark_selected_notification_read();
		changed
	}

	pub fn delete_selected_notification(&mut self) -> bool {
		let Some(center) = self.workbench.notification_center.as_mut() else {
			return false;
		};
		if self.workbench.notifications.is_empty() {
			center.selected = 0;
			return false;
		}
		let selected_in_reverse = center.selected.min(self.workbench.notifications.len().saturating_sub(1));
		let index = self.workbench.notifications.len().saturating_sub(1).saturating_sub(selected_in_reverse);
		let removed = self.workbench.notifications.remove(index);
		self.workbench.notification_preview_queue.retain(|id| *id != removed.id);
		self.workbench.notification_preview_active.retain(|item| item.id != removed.id);
		if self.workbench.notifications.is_empty() {
			center.selected = 0;
		} else {
			center.selected = center.selected.min(self.workbench.notifications.len().saturating_sub(1));
		}
		self.mark_selected_notification_read();
		true
	}

	fn mark_selected_notification_read(&mut self) {
		let Some(center) = self.workbench.notification_center.as_ref() else {
			return;
		};
		if self.workbench.notifications.is_empty() {
			return;
		}
		let selected_in_reverse = center.selected.min(self.workbench.notifications.len().saturating_sub(1));
		let index = self.workbench.notifications.len().saturating_sub(1).saturating_sub(selected_in_reverse);
		if let Some(entry) = self.workbench.notifications.get_mut(index) {
			entry.read = true;
		}
	}

	pub fn register_plugin_command(&mut self, registration: PluginCommandRegistration) -> Result<(), String> {
		self.workbench.command_registry.register_plugin_command(registration)
	}

	pub fn command_palette(&self) -> Option<&CommandPaletteState> { self.workbench.command_palette.as_ref() }

	pub fn workspace_file_picker(&self) -> Option<&WorkspaceFilePickerState> {
		self.workbench.workspace_file_picker.as_ref()
	}

	pub fn workspace_root(&self) -> &Path { self.workbench.workspace_root.as_path() }

	pub fn set_workspace_root(&mut self, workspace_root: PathBuf) {
		self.workbench.workspace_root = workspace_root;
	}

	pub fn refresh_command_palette(&mut self) {
		if !self.is_command_mode() {
			self.workbench.command_palette = None;
			return;
		}
		let previous_palette_preview = self
			.workbench
			.command_palette
			.as_ref()
			.map(|palette| (palette.preview_title.clone(), palette.preview_lines.clone(), palette.preview_scroll))
			.unwrap_or_else(|| (String::new(), Vec::new(), 0));
		let previous_selected_file = self
			.workbench
			.command_palette
			.as_ref()
			.and_then(|palette| palette.items.get(palette.selected))
			.and_then(CommandPaletteItem::as_file)
			.map(|item| (item.absolute_path.clone(), item.relative_path.clone()));
		let (items, loading, showing_files) = if let Some((_, file_query)) = self
			.command_palette_path_argument_context()
			.map(|(command, query)| (command.to_string(), query.to_string()))
		{
			if self.workbench.workspace_file_cache_loading {
				(Vec::new(), true, true)
			} else {
				(self.command_palette_file_matches(file_query.as_str(), 512), false, true)
			}
		} else {
			let command_query = self.workbench.command_line.split_whitespace().next().unwrap_or_default();
			let items = self
				.workbench
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
						.workbench
						.command_palette
						.as_ref()
						.map(|palette| palette.selected.min(items.len().saturating_sub(1)))
						.unwrap_or_default()
				})
		} else {
			self
				.workbench
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
		self.workbench.command_palette = Some(CommandPaletteState {
			query: self.workbench.command_line.clone(),
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
		self.workbench.command_palette = None;
		if matches!(
			self.workbench.overlay,
			Some(OverlayState::KeyHints(KeyHintsOverlayState { scope: KeymapScope::OverlayCommandPalette, .. }))
		) {
			self.workbench.overlay = None;
		}
	}

	pub fn has_workspace_file_cache(&self) -> bool { !self.workbench.workspace_file_cache.is_empty() }

	pub fn workspace_file_cache_is_loading(&self) -> bool { self.workbench.workspace_file_cache_loading }

	pub fn workspace_file_cache_entries(&self) -> &[WorkspaceFileEntry] {
		self.workbench.workspace_file_cache.as_slice()
	}

	pub fn command_palette_needs_workspace_files(&self) -> bool {
		self.command_palette_path_argument_context().is_some()
			&& self.workbench.workspace_file_cache.is_empty()
			&& !self.workbench.workspace_file_cache_loading
	}

	pub fn begin_workspace_file_cache_loading(&mut self) {
		self.workbench.workspace_file_cache_loading = true;
		self.refresh_command_palette();
	}

	pub fn set_workspace_file_cache(&mut self, entries: Vec<WorkspaceFileEntry>) {
		self.workbench.workspace_file_cache = entries;
		self.workbench.workspace_file_cache_loading = false;
		self.refresh_command_palette();
	}

	pub fn fail_workspace_file_cache_loading(&mut self) {
		self.workbench.workspace_file_cache_loading = false;
		self.refresh_command_palette();
	}

	pub fn open_workspace_file_picker(&mut self, entries: Vec<WorkspaceFileEntry>) {
		let total_files = entries.len();
		self.close_key_hints();
		self.close_command_palette();
		self.workbench.workspace_file_picker = Some(WorkspaceFilePickerState {
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
		if let Some(picker) = self.workbench.workspace_file_picker.as_mut() {
			picker.loading = true;
			return;
		}
		self.workbench.workspace_file_picker = Some(WorkspaceFilePickerState {
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
		self.workbench.workspace_file_picker = None;
		if matches!(
			self.workbench.overlay,
			Some(OverlayState::KeyHints(KeyHintsOverlayState { scope: KeymapScope::OverlayPicker, .. }))
		) {
			self.workbench.overlay = None;
		}
	}

	pub fn workspace_file_picker_open(&self) -> bool { self.workbench.workspace_file_picker.is_some() }

	pub fn workspace_file_picker_loading(&self) -> bool {
		self.workbench.workspace_file_picker.as_ref().is_some_and(|picker| picker.loading)
	}

	pub fn refresh_workspace_file_picker_matches(&mut self) {
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
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
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
			self.open_workspace_file_picker(entries);
			return;
		};
		picker.entries = entries;
		picker.total_files = picker.entries.len();
		self.refresh_workspace_file_picker_matches();
	}

	pub fn move_workspace_file_picker_selection(&mut self, delta: isize) -> bool {
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
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
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
			return;
		};
		picker.query.push(ch);
		picker.selected = 0;
		picker.preferred = None;
		picker.preview_scroll = 0;
		self.refresh_workspace_file_picker_matches();
	}

	pub fn pop_workspace_file_picker_char(&mut self) {
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
			return;
		};
		let _ = picker.query.pop();
		picker.selected = 0;
		picker.preferred = None;
		picker.preview_scroll = 0;
		self.refresh_workspace_file_picker_matches();
	}

	pub fn selected_workspace_file_picker_path(&self) -> Option<&Path> {
		let picker = self.workbench.workspace_file_picker.as_ref()?;
		let selected = picker.items.get(picker.selected)?;
		Some(selected.absolute_path.as_path())
	}

	pub fn set_workspace_file_picker_preview(&mut self, path: &Path, preview: String) {
		let content_width = self.active_tab_content_size().0;
		let preview_width = compute_workspace_file_picker_body_layout(content_width).preview_width as usize;
		let word_wrap = self.workbench.picker_preview_word_wrap;
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
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
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
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
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
			return;
		};
		picker.preview_title.clear();
		picker.preview_lines.clear();
		picker.preview_scroll = 0;
	}

	pub fn scroll_workspace_file_picker_preview(&mut self, delta: isize) -> bool {
		let content_width = self.active_tab_content_size().0;
		let preview_width = compute_workspace_file_picker_body_layout(content_width).preview_width as usize;
		let word_wrap = self.workbench.picker_preview_word_wrap;
		let Some(picker) = self.workbench.workspace_file_picker.as_mut() else {
			return false;
		};
		let max_scroll = preview_max_scroll_with_mode(picker.preview_lines.as_slice(), preview_width, word_wrap);
		let next = picker.preview_scroll.saturating_add_signed(delta).min(max_scroll);
		let changed = next != picker.preview_scroll;
		picker.preview_scroll = next;
		changed
	}

	pub fn move_command_palette_selection(&mut self, delta: isize) -> bool {
		let Some(palette) = self.workbench.command_palette.as_mut() else {
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
		let word_wrap = self.workbench.picker_preview_word_wrap;
		let Some(palette) = self.workbench.command_palette.as_mut() else {
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

	pub fn word_wrap_enabled(&self) -> bool { self.workbench.word_wrap }

	pub fn toggle_word_wrap(&mut self) {
		self.workbench.word_wrap = !self.workbench.word_wrap;
		for window_id in self.active_tab_window_ids() {
			if let Some(window) = self.windows.get_mut(window_id) {
				window.scroll_x = 0;
			}
		}
		self.align_active_window_scroll_to_cursor();
		self.workbench.status_bar.message = if self.workbench.word_wrap {
			"word wrap enabled".to_string()
		} else {
			"word wrap disabled".to_string()
		};
	}

	pub fn picker_preview_word_wrap_enabled(&self) -> bool { self.workbench.picker_preview_word_wrap }

	pub fn toggle_picker_preview_word_wrap(&mut self) {
		self.workbench.picker_preview_word_wrap = !self.workbench.picker_preview_word_wrap;
		let content_width = self.active_tab_content_size().0;
		let picker_preview_width =
			compute_workspace_file_picker_body_layout(content_width).preview_width as usize;
		let command_preview_width = compute_command_palette_preview_width(content_width) as usize;
		if let Some(picker) = self.workbench.workspace_file_picker.as_mut() {
			let max_scroll = preview_max_scroll_with_mode(
				picker.preview_lines.as_slice(),
				picker_preview_width,
				self.workbench.picker_preview_word_wrap,
			);
			picker.preview_scroll = picker.preview_scroll.min(max_scroll);
		}
		if let Some(palette) = self.workbench.command_palette.as_mut()
			&& palette.showing_files
		{
			let max_scroll = preview_max_scroll_with_mode(
				palette.preview_lines.as_slice(),
				command_preview_width,
				self.workbench.picker_preview_word_wrap,
			);
			palette.preview_scroll = palette.preview_scroll.min(max_scroll);
		}
		self.workbench.status_bar.message = if self.workbench.picker_preview_word_wrap {
			"picker preview wrap enabled".to_string()
		} else {
			"picker preview wrap disabled".to_string()
		};
	}

	pub fn selected_command_palette_match(&self) -> Option<&CommandPaletteMatch> {
		let palette = self.workbench.command_palette.as_ref()?;
		palette.items.get(palette.selected)?.as_command()
	}

	pub fn selected_command_palette_file_match(&self) -> Option<&CommandPaletteFileMatch> {
		let palette = self.workbench.command_palette.as_ref()?;
		palette.items.get(palette.selected)?.as_file()
	}

	pub fn selected_command_palette_file_path(&self) -> Option<&Path> {
		self.selected_command_palette_file_match().map(|item| item.absolute_path.as_path())
	}

	pub fn command_palette_showing_files(&self) -> bool {
		self.workbench.command_palette.as_ref().is_some_and(|palette| palette.showing_files)
	}

	pub fn set_command_palette_preview(&mut self, path: &Path, preview: String) {
		let content_width = self.active_tab_content_size().0;
		let preview_width = compute_command_palette_preview_width(content_width) as usize;
		let word_wrap = self.workbench.picker_preview_word_wrap;
		let Some(palette) = self.workbench.command_palette.as_mut() else {
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
		let Some(palette) = self.workbench.command_palette.as_mut() else {
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
		let Some(palette) = self.workbench.command_palette.as_ref() else {
			return false;
		};
		let Some(item) = palette.items.get(palette.selected) else {
			return false;
		};
		match item {
			CommandPaletteItem::Command(item) => {
				self.workbench.command_line = item.completion.clone();
				self.refresh_command_palette();
				true
			}
			CommandPaletteItem::File(item) => {
				let Some((command, _)) = self.workbench.command_line.split_once(' ') else {
					return false;
				};
				self.workbench.command_line = format!("{} {}", command, item.relative_path);
				self.refresh_command_palette();
				true
			}
		}
	}

	pub fn active_keymap_scope(&self) -> KeymapScope {
		if let Some(OverlayState::KeyHints(overlay)) = self.workbench.overlay.as_ref() {
			overlay.scope
		} else if self.workspace_file_picker_open() {
			KeymapScope::OverlayPicker
		} else if self.notification_center_open() {
			KeymapScope::OverlayNotificationCenter
		} else if self.workbench.command_palette.is_some() {
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
		match self.workbench.overlay.as_ref() {
			Some(OverlayState::KeyHints(overlay)) => Some(&overlay.window),
			Some(OverlayState::FloatingWindow(window)) => Some(window),
			None => None,
		}
	}

	pub fn close_key_hints(&mut self) {
		if matches!(self.workbench.overlay, Some(OverlayState::KeyHints(_))) {
			self.workbench.overlay = None;
		}
	}

	pub fn key_hints_open(&self) -> bool { matches!(self.workbench.overlay, Some(OverlayState::KeyHints(_))) }

	pub fn open_key_hints_overview(&mut self) { self.open_key_hints(Vec::new(), true); }

	pub fn scroll_key_hints_up(&mut self) -> bool { self.scroll_overlay_lines(-1) }

	pub fn scroll_key_hints_down(&mut self) -> bool { self.scroll_overlay_lines(1) }

	pub fn scroll_key_hints_half_page_up(&mut self) -> bool { self.scroll_overlay_half_page(-1) }

	pub fn scroll_key_hints_half_page_down(&mut self) -> bool { self.scroll_overlay_half_page(1) }

	pub fn refresh_key_hints_overlay_after_config_reload(&mut self) {
		let Some(OverlayState::KeyHints(overlay)) = self.workbench.overlay.as_ref() else {
			return;
		};
		self.open_key_hints(overlay.prefix.clone(), overlay.overview);
	}

	pub fn refresh_pending_key_hints(&mut self) {
		if self.workbench.normal_sequence.is_empty() {
			self.close_key_hints();
			return;
		}
		self.open_key_hints(self.workbench.normal_sequence.clone(), false);
	}

	fn open_key_hints(&mut self, prefix: Vec<NormalSequenceKey>, overview: bool) {
		let scope = self.active_keymap_scope();
		let lines = self.workbench.command_registry.key_hints(scope, prefix.as_slice());
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
		let height = lines.len().saturating_add(4).min(self.workbench.key_hints_max_height as usize) as u16;
		let window = FloatingWindowState {
			title,
			subtitle,
			footer: Some("Esc close  Backspace back".to_string()),
			placement: FloatingWindowPlacement::BottomRight {
				width: self.workbench.key_hints_width,
				height,
				margin_right: 1,
				margin_bottom: 1,
			},
			lines,
			scroll: 0,
		};
		self.workbench.overlay =
			Some(OverlayState::KeyHints(KeyHintsOverlayState { scope, prefix, overview, window }));
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
		let (command, tail) = self.workbench.command_line.split_once(' ')?;
		let resolved = self.workbench.command_registry.resolve_command_input(command)?;
		if resolved.spec.arg_kind != CommandArgKind::OptionalPath {
			return None;
		}
		Some((command, tail.trim_start()))
	}

	fn command_palette_file_matches(&self, query: &str, limit: usize) -> Vec<CommandPaletteItem> {
		let mut matches = self
			.workbench
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
		if self.workbench.normal_sequence.pop().is_none() {
			self.close_key_hints();
			return false;
		}
		self.workbench.status_bar.key_sequence = self.render_sequence(self.workbench.normal_sequence.as_slice());
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
		match self.workbench.overlay.as_mut() {
			Some(OverlayState::KeyHints(overlay)) => Some(&mut overlay.window),
			Some(OverlayState::FloatingWindow(window)) => Some(window),
			None => None,
		}
	}

	pub fn create_window(&mut self, buffer_id: Option<BufferId>) -> Option<WindowId> {
		self.editor.create_window(buffer_id)
	}

	pub fn active_buffer_id(&self) -> Option<BufferId> { self.editor.active_buffer_id() }

	pub fn bind_buffer_to_active_window(&mut self, buffer_id: BufferId) {
		self.editor.bind_buffer_to_active_window(buffer_id);
	}

	pub(crate) fn bind_buffer_to_window(
		&mut self,
		window_id: WindowId,
		buffer_id: BufferId,
		persist_previous_cursor: bool,
	) {
		self.editor.bind_buffer_to_window(window_id, buffer_id, persist_previous_cursor);
	}

	pub(crate) fn sync_window_view_binding(&mut self, window_id: WindowId) {
		self.editor.sync_window_view_binding(window_id);
	}
}

impl Default for RimState {
	fn default() -> Self { Self::new() }
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
