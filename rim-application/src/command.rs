use std::{collections::{BTreeMap, HashMap, HashSet}, path::PathBuf};

use frizbee::{Config as FrizbeeConfig, match_list_indices};
use rim_command_macros::{BuiltinCommandGroup, BuiltinCommandRoot};
use serde::{Deserialize, Serialize};

use crate::{action::{AppAction, BufferAction, EditorAction, LayoutAction, TabAction, WindowAction}, defaults, state::{FloatingWindowLine, KeymapScope, NormalSequenceKey}};

pub trait BuiltinCommandGroupMeta: Copy {
	fn command_segment(self) -> &'static str;
	fn description(self) -> &'static str;
	fn arg_kind(self) -> CommandArgKind;
	fn all_commands() -> &'static [Self];
}

pub trait BuiltinCommandRootMeta: Copy {
	fn id(self) -> String;
	fn category(self) -> BuiltinCommandCategory;
	fn description(self) -> &'static str;
	fn arg_kind(self) -> CommandArgKind;
	fn all_commands() -> Vec<Self>;
	fn from_id(id: &str) -> Option<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinCommandCategory {
	Window,
	Tab,
	Buffer,
	Mode,
	Edit,
	Cursor,
	View,
	Help,
	Command,
	CommandPalette,
	Picker,
	Overlay,
	Notification,
	Insert,
	Visual,
}

impl BuiltinCommandCategory {
	pub fn label(self) -> &'static str {
		match self {
			Self::Window => "window",
			Self::Tab => "tab",
			Self::Buffer => "buffer",
			Self::Mode => "mode",
			Self::Edit => "edit",
			Self::Cursor => "cursor",
			Self::View => "view",
			Self::Help => "help",
			Self::Command => "command",
			Self::CommandPalette => "command_palette",
			Self::Picker => "picker",
			Self::Overlay => "overlay",
			Self::Notification => "notification",
			Self::Insert => "insert",
			Self::Visual => "visual",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum WindowCommand {
	/// Split vertically
	SplitVertical,
	/// Split horizontally
	SplitHorizontal,
	/// Focus left window
	FocusLeft,
	/// Focus down window
	FocusDown,
	/// Focus up window
	FocusUp,
	/// Focus right window
	FocusRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum TabCommand {
	/// Open a new tab
	New,
	/// Close current tab
	CloseCurrent,
	/// Previous tab
	Prev,
	/// Next tab
	Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum BufferCommand {
	/// Close active buffer
	Close,
	/// Create an empty buffer
	NewEmpty,
	/// Delete current line
	DeleteLine,
	/// Previous buffer
	Prev,
	/// Next buffer
	Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum ModeCommand {
	/// Enter insert mode
	Insert,
	/// Append and enter insert mode
	Append,
	/// Open line below and enter insert mode
	OpenBelow,
	/// Open line above and enter insert mode
	OpenAbove,
	/// Enter command mode
	Command,
	/// Enter visual mode
	Visual,
	/// Enter visual line mode
	VisualLine,
	/// Enter visual block mode
	VisualBlock,
	/// Return to normal mode
	Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum EditCommand {
	/// Undo
	Undo,
	/// Redo
	Redo,
	/// Join line below
	JoinLineBelow,
	/// Cut current char
	CutChar,
	/// Paste slot after cursor
	Paste,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum CursorCommand {
	/// Move left
	Left,
	/// Move to line start
	LineStart,
	/// Move to line end
	LineEnd,
	/// Move down
	Down,
	/// Move up
	Up,
	/// Move right
	Right,
	/// Move to file start
	FileStart,
	/// Move to file end
	FileEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum ViewCommand {
	/// Scroll down
	ScrollDown,
	/// Scroll up
	ScrollUp,
	/// Scroll down half page
	ScrollHalfPageDown,
	/// Scroll up half page
	ScrollHalfPageUp,
	/// Toggle word wrap
	ToggleWordWrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum HelpCommand {
	/// Toggle current mode key hints
	Keymap,
	/// Scroll key hint window up
	KeymapScrollUp,
	/// Scroll key hint window down
	KeymapScrollDown,
	/// Scroll key hint window up half page
	KeymapHalfPageUp,
	/// Scroll key hint window down half page
	KeymapHalfPageDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum CommandPaletteCommand {
	/// Select previous command candidate
	Prev,
	/// Select next command candidate
	Next,
	/// Select previous command candidate page
	PageUp,
	/// Select next command candidate page
	PageDown,
	/// Scroll command palette preview down
	PreviewScrollDown,
	/// Scroll command palette preview up
	PreviewScrollUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum PickerCommand {
	/// Select previous picker item
	Prev,
	/// Select next picker item
	Next,
	/// Scroll picker preview down
	PreviewScrollDown,
	/// Scroll picker preview up
	PreviewScrollUp,
	/// Toggle picker preview word wrap
	TogglePreviewWordWrap,
	/// Confirm picker selection
	Confirm,
	/// Open the workspace file picker
	Files,
	/// Open the yazi picker
	Yazi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum OverlayCommand {
	/// Close active overlay
	Close,
	/// Go back inside active overlay
	Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum NotificationCommand {
	/// Select previous notification item
	Prev,
	/// Select next notification item
	Next,
	/// Delete selected notification item
	Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum InsertCommand {
	/// Insert newline
	Newline,
	/// Delete previous character in insert mode
	Backspace,
	/// Move left in insert mode
	Left,
	/// Move down in insert mode
	Down,
	/// Move up in insert mode
	Up,
	/// Move right in insert mode
	Right,
	/// Insert tab
	Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum VisualCommand {
	/// Exit visual mode
	Exit,
	/// Delete visual selection
	Delete,
	/// Yank visual selection
	Yank,
	/// Replace visual selection with slot
	Paste,
	/// Change visual selection
	Change,
	/// Insert before visual block
	BlockInsertBefore,
	/// Append after visual block
	BlockInsertAfter,
	/// Move left in visual mode
	Left,
	/// Move right in visual mode
	Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
pub enum CommandCommand {
	/// Quit current scope
	Quit,
	/// Force quit current scope
	QuitForce,
	/// Quit application
	QuitAll,
	/// Force quit application
	QuitAllForce,
	/// Save current buffer
	#[command(arg = OptionalPath)]
	Save,
	/// Force save current buffer
	#[command(arg = OptionalPath)]
	SaveForce,
	/// Save all file-backed buffers
	SaveAll,
	/// Save current buffer and quit
	#[command(arg = OptionalPath)]
	SaveAndQuit,
	/// Force save current buffer and quit
	#[command(arg = OptionalPath)]
	SaveAndQuitForce,
	/// Save all file-backed buffers and quit
	SaveAllAndQuit,
	/// Force save all file-backed buffers and quit
	SaveAllAndQuitForce,
	/// Reload current buffer
	#[command(arg = OptionalPath)]
	Reload,
	/// Force reload current buffer
	#[command(arg = OptionalPath)]
	ReloadForce,
	/// Execute current command input
	Submit,
	/// Delete previous command character
	Backspace,
	/// Open notification center
	Notifications,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandRoot)]
pub enum BuiltinCommand {
	Window(WindowCommand),
	Tab(TabCommand),
	Buffer(BufferCommand),
	Mode(ModeCommand),
	Edit(EditCommand),
	Cursor(CursorCommand),
	View(ViewCommand),
	Help(HelpCommand),
	#[command(namespace = "")]
	Command(CommandCommand),
	CommandPalette(CommandPaletteCommand),
	Picker(PickerCommand),
	Overlay(OverlayCommand),
	Notification(NotificationCommand),
	Insert(InsertCommand),
	Visual(VisualCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CommandId {
	Builtin(BuiltinCommand),
	Plugin(String),
}

impl CommandId {
	pub fn display_text(&self) -> String {
		match self {
			Self::Builtin(command) => command.id(),
			Self::Plugin(id) => id.clone(),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CommandCategory {
	Builtin(BuiltinCommandCategory),
	Plugin,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandCategoryInfo {
	pub kind:  CommandCategory,
	pub label: String,
}

impl CommandCategoryInfo {
	pub fn builtin(category: BuiltinCommandCategory) -> Self {
		Self { kind: CommandCategory::Builtin(category), label: category.label().to_string() }
	}

	pub fn plugin(label: impl Into<String>) -> Self {
		Self { kind: CommandCategory::Plugin, label: label.into() }
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunDirective {
	Builtin(BuiltinCommand),
	PluginInvocation { plugin_name: String, argument: Option<String> },
	Unresolved(String),
}

impl RunDirective {
	pub fn parse(text: &str) -> Self {
		let trimmed = text.trim();
		if let Some(command) = BuiltinCommand::from_id(trimmed) {
			return Self::Builtin(command);
		}
		if let Some(payload) = trimmed.strip_prefix("plugin ") {
			let payload = payload.trim();
			if payload.is_empty() {
				return Self::Unresolved(trimmed.to_string());
			}
			let mut segments = payload.split_whitespace();
			let plugin_name = segments.next().expect("plugin command is non-empty").to_string();
			let tail = segments.collect::<Vec<_>>().join(" ");
			let argument = if tail.is_empty() { None } else { Some(tail) };
			return Self::PluginInvocation { plugin_name, argument };
		}
		Self::Unresolved(trimmed.to_string())
	}

	pub fn render(&self) -> String {
		match self {
			Self::Builtin(command) => command.id(),
			Self::PluginInvocation { plugin_name, argument } => match argument {
				Some(argument) => format!("plugin {} {}", plugin_name, argument),
				None => format!("plugin {}", plugin_name),
			},
			Self::Unresolved(raw) => raw.clone(),
		}
	}
}

impl From<&str> for RunDirective {
	fn from(value: &str) -> Self { Self::parse(value) }
}

impl From<String> for RunDirective {
	fn from(value: String) -> Self { Self::parse(value.as_str()) }
}

impl Serialize for RunDirective {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where S: serde::Serializer {
		serializer.serialize_str(self.render().as_str())
	}
}

impl<'de> Deserialize<'de> for RunDirective {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where D: serde::Deserializer<'de> {
		let raw = String::deserialize(deserializer)?;
		Ok(Self::parse(raw.as_str()))
	}
}

impl BuiltinCommand {
	pub fn normal_mode_action(&self) -> Option<AppAction> {
		match self {
			Self::Window(WindowCommand::SplitVertical) => Some(AppAction::Layout(LayoutAction::SplitVertical)),
			Self::Window(WindowCommand::SplitHorizontal) => Some(AppAction::Layout(LayoutAction::SplitHorizontal)),
			Self::Tab(TabCommand::New) => Some(AppAction::Tab(TabAction::New)),
			Self::Tab(TabCommand::CloseCurrent) => Some(AppAction::Tab(TabAction::CloseCurrent)),
			Self::Tab(TabCommand::Prev) => Some(AppAction::Tab(TabAction::SwitchPrev)),
			Self::Tab(TabCommand::Next) => Some(AppAction::Tab(TabAction::SwitchNext)),
			Self::Buffer(BufferCommand::Close) => Some(AppAction::Editor(EditorAction::CloseActiveBuffer)),
			Self::Buffer(BufferCommand::NewEmpty) => Some(AppAction::Editor(EditorAction::NewEmptyBuffer)),
			Self::Buffer(BufferCommand::DeleteLine) => {
				Some(AppAction::Editor(EditorAction::DeleteCurrentLineToSlot))
			}
			Self::Mode(ModeCommand::Insert) => Some(AppAction::Editor(EditorAction::EnterInsert)),
			Self::Mode(ModeCommand::Append) => Some(AppAction::Editor(EditorAction::AppendInsert)),
			Self::Mode(ModeCommand::OpenBelow) => Some(AppAction::Editor(EditorAction::OpenLineBelowInsert)),
			Self::Mode(ModeCommand::OpenAbove) => Some(AppAction::Editor(EditorAction::OpenLineAboveInsert)),
			Self::Mode(ModeCommand::Command) => Some(AppAction::Editor(EditorAction::EnterCommandMode)),
			Self::Mode(ModeCommand::Visual) => Some(AppAction::Editor(EditorAction::EnterVisualMode)),
			Self::Mode(ModeCommand::VisualLine) => Some(AppAction::Editor(EditorAction::EnterVisualLineMode)),
			Self::Mode(ModeCommand::VisualBlock) => Some(AppAction::Editor(EditorAction::EnterVisualBlockMode)),
			Self::Edit(EditCommand::Undo) => Some(AppAction::Editor(EditorAction::Undo)),
			Self::Edit(EditCommand::Redo) => Some(AppAction::Editor(EditorAction::Redo)),
			Self::Cursor(CursorCommand::Left) => Some(AppAction::Editor(EditorAction::MoveLeft)),
			Self::Cursor(CursorCommand::LineStart) => Some(AppAction::Editor(EditorAction::MoveLineStart)),
			Self::Cursor(CursorCommand::LineEnd) => Some(AppAction::Editor(EditorAction::MoveLineEnd)),
			Self::Cursor(CursorCommand::Down) => Some(AppAction::Editor(EditorAction::MoveDown)),
			Self::Cursor(CursorCommand::Up) => Some(AppAction::Editor(EditorAction::MoveUp)),
			Self::Cursor(CursorCommand::Right) => Some(AppAction::Editor(EditorAction::MoveRight)),
			Self::Cursor(CursorCommand::FileStart) => Some(AppAction::Editor(EditorAction::MoveFileStart)),
			Self::Cursor(CursorCommand::FileEnd) => Some(AppAction::Editor(EditorAction::MoveFileEnd)),
			Self::Edit(EditCommand::JoinLineBelow) => Some(AppAction::Editor(EditorAction::JoinLineBelow)),
			Self::Edit(EditCommand::CutChar) => Some(AppAction::Editor(EditorAction::CutCharToSlot)),
			Self::Edit(EditCommand::Paste) => Some(AppAction::Editor(EditorAction::PasteSlotAfterCursor)),
			Self::Buffer(BufferCommand::Prev) => Some(AppAction::Buffer(BufferAction::SwitchPrev)),
			Self::Buffer(BufferCommand::Next) => Some(AppAction::Buffer(BufferAction::SwitchNext)),
			Self::Window(WindowCommand::FocusLeft) => Some(AppAction::Window(WindowAction::FocusLeft)),
			Self::Window(WindowCommand::FocusDown) => Some(AppAction::Window(WindowAction::FocusDown)),
			Self::Window(WindowCommand::FocusUp) => Some(AppAction::Window(WindowAction::FocusUp)),
			Self::Window(WindowCommand::FocusRight) => Some(AppAction::Window(WindowAction::FocusRight)),
			Self::View(ViewCommand::ScrollDown) => Some(AppAction::Editor(EditorAction::ScrollViewDown)),
			Self::View(ViewCommand::ScrollUp) => Some(AppAction::Editor(EditorAction::ScrollViewUp)),
			Self::View(ViewCommand::ScrollHalfPageDown) => {
				Some(AppAction::Editor(EditorAction::ScrollViewHalfPageDown))
			}
			Self::View(ViewCommand::ScrollHalfPageUp) => {
				Some(AppAction::Editor(EditorAction::ScrollViewHalfPageUp))
			}
			Self::Help(HelpCommand::Keymap) => Some(AppAction::Editor(EditorAction::ShowKeyHints)),
			Self::Help(HelpCommand::KeymapScrollUp) => Some(AppAction::Editor(EditorAction::ScrollKeyHintsUp)),
			Self::Help(HelpCommand::KeymapScrollDown) => Some(AppAction::Editor(EditorAction::ScrollKeyHintsDown)),
			Self::Help(HelpCommand::KeymapHalfPageUp) => {
				Some(AppAction::Editor(EditorAction::ScrollKeyHintsHalfPageUp))
			}
			Self::Help(HelpCommand::KeymapHalfPageDown) => {
				Some(AppAction::Editor(EditorAction::ScrollKeyHintsHalfPageDown))
			}
			_ => None,
		}
	}

	pub fn visual_mode_action(&self) -> Option<AppAction> {
		match self {
			Self::Mode(ModeCommand::Visual) => Some(AppAction::Editor(EditorAction::EnterVisualMode)),
			Self::Mode(ModeCommand::VisualLine) => Some(AppAction::Editor(EditorAction::EnterVisualLineMode)),
			Self::Mode(ModeCommand::VisualBlock) => Some(AppAction::Editor(EditorAction::EnterVisualBlockMode)),
			Self::Cursor(CursorCommand::Down) => Some(AppAction::Editor(EditorAction::MoveDown)),
			Self::Cursor(CursorCommand::Up) => Some(AppAction::Editor(EditorAction::MoveUp)),
			Self::Cursor(CursorCommand::LineStart) => Some(AppAction::Editor(EditorAction::MoveLineStart)),
			Self::Cursor(CursorCommand::LineEnd) => Some(AppAction::Editor(EditorAction::MoveLineEnd)),
			Self::Cursor(CursorCommand::FileStart) => Some(AppAction::Editor(EditorAction::MoveFileStart)),
			Self::Cursor(CursorCommand::FileEnd) => Some(AppAction::Editor(EditorAction::MoveFileEnd)),
			Self::View(ViewCommand::ScrollDown) => Some(AppAction::Editor(EditorAction::ScrollViewDown)),
			Self::View(ViewCommand::ScrollUp) => Some(AppAction::Editor(EditorAction::ScrollViewUp)),
			Self::View(ViewCommand::ScrollHalfPageDown) => {
				Some(AppAction::Editor(EditorAction::ScrollViewHalfPageDown))
			}
			Self::View(ViewCommand::ScrollHalfPageUp) => {
				Some(AppAction::Editor(EditorAction::ScrollViewHalfPageUp))
			}
			Self::Help(HelpCommand::Keymap) => Some(AppAction::Editor(EditorAction::ShowKeyHints)),
			Self::Help(HelpCommand::KeymapScrollUp) => Some(AppAction::Editor(EditorAction::ScrollKeyHintsUp)),
			Self::Help(HelpCommand::KeymapScrollDown) => Some(AppAction::Editor(EditorAction::ScrollKeyHintsDown)),
			Self::Help(HelpCommand::KeymapHalfPageUp) => {
				Some(AppAction::Editor(EditorAction::ScrollKeyHintsHalfPageUp))
			}
			Self::Help(HelpCommand::KeymapHalfPageDown) => {
				Some(AppAction::Editor(EditorAction::ScrollKeyHintsHalfPageDown))
			}
			Self::Visual(VisualCommand::Exit) => Some(AppAction::Editor(EditorAction::ExitVisualMode)),
			Self::Visual(VisualCommand::Delete) => {
				Some(AppAction::Editor(EditorAction::DeleteVisualSelectionToSlot))
			}
			Self::Visual(VisualCommand::Yank) => Some(AppAction::Editor(EditorAction::YankVisualSelectionToSlot)),
			Self::Visual(VisualCommand::Paste) => {
				Some(AppAction::Editor(EditorAction::ReplaceVisualSelectionWithSlot))
			}
			Self::Visual(VisualCommand::Change) => {
				Some(AppAction::Editor(EditorAction::ChangeVisualSelectionToInsertMode))
			}
			Self::Visual(VisualCommand::BlockInsertBefore) => {
				Some(AppAction::Editor(EditorAction::BeginVisualBlockInsertBefore))
			}
			Self::Visual(VisualCommand::BlockInsertAfter) => {
				Some(AppAction::Editor(EditorAction::BeginVisualBlockInsertAfter))
			}
			Self::Visual(VisualCommand::Left) => Some(AppAction::Editor(EditorAction::MoveLeftInVisual)),
			Self::Visual(VisualCommand::Right) => Some(AppAction::Editor(EditorAction::MoveRightInVisual)),
			_ => None,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandTarget {
	Builtin(BuiltinCommand),
	Plugin { plugin_id: String, command_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandArgKind {
	None,
	OptionalPath,
	RawTail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
	pub id:           CommandId,
	pub category:     CommandCategoryInfo,
	pub description:  String,
	pub arg_kind:     CommandArgKind,
	pub target:       CommandTarget,
	pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCommand {
	pub spec:     CommandSpec,
	pub argument: Option<String>,
}

impl CommandSpec {
	fn builtin(command: BuiltinCommand) -> Self {
		Self {
			id:           CommandId::Builtin(command),
			category:     CommandCategoryInfo::builtin(command.category()),
			description:  command.description().to_string(),
			arg_kind:     command.arg_kind(),
			target:       CommandTarget::Builtin(command),
			display_name: None,
		}
	}

	fn plugin(registration: PluginCommandRegistration) -> Self {
		Self {
			id:           CommandId::Plugin(registration.id.clone()),
			category:     CommandCategoryInfo::plugin(registration.category),
			description:  registration.description,
			arg_kind:     registration.arg_kind,
			target:       CommandTarget::Plugin {
				plugin_id:  registration.plugin_id,
				command_id: registration.command_id,
			},
			display_name: None,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteMatch {
	pub name:                      String,
	pub command_id:                CommandId,
	pub command_id_label:          String,
	pub description:               String,
	pub name_match_indices:        Vec<usize>,
	pub command_id_match_indices:  Vec<usize>,
	pub description_match_indices: Vec<usize>,
	pub is_error:                  bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteFileMatch {
	pub relative_path: String,
	pub absolute_path: PathBuf,
	pub match_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPaletteItem {
	Command(CommandPaletteMatch),
	File(CommandPaletteFileMatch),
}

impl CommandPaletteItem {
	pub fn as_command(&self) -> Option<&CommandPaletteMatch> {
		match self {
			Self::Command(item) => Some(item),
			Self::File(_) => None,
		}
	}

	pub fn as_file(&self) -> Option<&CommandPaletteFileMatch> {
		match self {
			Self::Command(_) => None,
			Self::File(item) => Some(item),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandPaletteCandidate {
	name:             String,
	command_id:       CommandId,
	command_id_label: String,
	description:      String,
	is_error:         bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingMatch<T> {
	Exact(T),
	Pending,
	NoMatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopedKeyBinding {
	keys:       Vec<NormalSequenceKey>,
	command_id: CommandId,
	desc:       Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandAlias {
	name:                String,
	resolved_command_id: Option<CommandId>,
	run:                 RunDirective,
	desc:                Option<String>,
	error:               Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CommandRegistry {
	commands:                             HashMap<CommandId, CommandSpec>,
	normal_bindings:                      Vec<ScopedKeyBinding>,
	visual_bindings:                      Vec<ScopedKeyBinding>,
	command_mode_bindings:                Vec<ScopedKeyBinding>,
	insert_mode_bindings:                 Vec<ScopedKeyBinding>,
	overlay_whichkey_bindings:            Vec<ScopedKeyBinding>,
	overlay_command_palette_bindings:     Vec<ScopedKeyBinding>,
	overlay_picker_bindings:              Vec<ScopedKeyBinding>,
	overlay_notification_center_bindings: Vec<ScopedKeyBinding>,
	command_aliases:                      Vec<CommandAlias>,
}

impl CommandRegistry {
	pub fn with_defaults() -> Self {
		let mut registry = Self::default();
		registry.register_builtin_specs();
		let errors = registry.apply_config(defaults::default_command_config());
		assert!(errors.is_empty(), "embedded default command preset contains invalid entries: {:?}", errors);
		registry
	}

	pub fn apply_config(&mut self, config: &CommandConfigFile) -> Vec<CommandConfigError> {
		let mut errors = Vec::new();
		self.apply_scope_keymap(KeymapScope::ModeNormal, "mode.normal", &config.mode.normal.keymap, &mut errors);
		self.apply_scope_keymap(KeymapScope::ModeVisual, "mode.visual", &config.mode.visual.keymap, &mut errors);
		self.apply_scope_keymap(
			KeymapScope::ModeCommand,
			"mode.command",
			&config.mode.command.keymap,
			&mut errors,
		);
		self.apply_scope_keymap(KeymapScope::ModeInsert, "mode.insert", &config.mode.insert.keymap, &mut errors);
		self.apply_scope_keymap(
			KeymapScope::OverlayWhichKey,
			"overlay.whichkey",
			&config.overlay.whichkey.keymap,
			&mut errors,
		);
		self.apply_scope_keymap(
			KeymapScope::OverlayCommandPalette,
			"overlay.command_palette",
			&config.overlay.command_palette.keymap,
			&mut errors,
		);
		self.apply_scope_keymap(
			KeymapScope::OverlayPicker,
			"overlay.picker",
			&config.overlay.picker.keymap,
			&mut errors,
		);
		self.apply_scope_keymap(
			KeymapScope::OverlayNotificationCenter,
			"overlay.notification_center",
			&config.overlay.notification_center.keymap,
			&mut errors,
		);

		if !config.command.commands.is_empty() {
			self.command_aliases.clear();
		}
		for (alias_index, alias) in config.command.commands.iter().enumerate() {
			let Some(resolved) = self.resolve_or_register_run_directive(&alias.run) else {
				let raw_run = alias.run.render();
				let error = format!("invalid run directive: {}", raw_run);
				errors.push(CommandConfigError::CommandAlias {
					alias_index,
					reason: format!("unknown run directive: {}", raw_run),
				});
				self.command_aliases.push(CommandAlias {
					name:                alias.name.clone(),
					resolved_command_id: None,
					run:                 alias.run.clone(),
					desc:                alias.desc.clone(),
					error:               Some(error),
				});
				continue;
			};
			self.command_aliases.push(CommandAlias {
				name:                alias.name.clone(),
				resolved_command_id: Some(resolved.spec.id),
				run:                 alias.run.clone(),
				desc:                alias.desc.clone(),
				error:               None,
			});
		}

		errors
	}

	fn apply_scope_keymap(
		&mut self,
		scope: KeymapScope,
		scope_label: &str,
		configured_bindings: &[KeymapBindingConfig],
		errors: &mut Vec<CommandConfigError>,
	) {
		let mut overridden_commands = Vec::new();
		for (binding_index, binding) in configured_bindings.iter().enumerate() {
			let key_sets = match binding.on.parse_bindings() {
				Ok(key_sets) => key_sets,
				Err(err) => {
					errors.push(CommandConfigError::Keymap {
						scope: scope_label.to_string(),
						binding_index,
						reason: format!("invalid key binding '{}': {}", binding.on.display_for_error(), err),
					});
					continue;
				}
			};
			let Some(resolved) = self.resolve_or_register_run_directive(&binding.run) else {
				errors.push(CommandConfigError::Keymap {
					scope: scope_label.to_string(),
					binding_index,
					reason: format!("unknown run directive: {}", binding.run.render()),
				});
				continue;
			};
			let bindings = self.bindings_mut(scope);
			let should_override = !overridden_commands.iter().any(|command_id| command_id == &resolved.spec.id);
			let mut staged = if should_override {
				bindings
					.iter()
					.filter(|candidate| candidate.command_id != resolved.spec.id)
					.cloned()
					.collect::<Vec<_>>()
			} else {
				bindings.clone()
			};
			let mut conflicted = false;
			for keys in key_sets {
				if let Some(existing) = staged.iter_mut().find(|candidate| candidate.keys == keys) {
					if existing.command_id != resolved.spec.id {
						errors.push(CommandConfigError::Keymap {
							scope: scope_label.to_string(),
							binding_index,
							reason: format!(
								"conflicting key binding '{}' already mapped to '{:?}'",
								render_normal_sequence(&keys),
								existing.command_id
							),
						});
						conflicted = true;
						break;
					}
					existing.desc = binding.desc.clone();
					continue;
				}
				let mut has_prefix_conflict = false;
				for candidate in staged.iter() {
					if is_prefix_sequence(candidate.keys.as_slice(), &keys)
						|| is_prefix_sequence(&keys, candidate.keys.as_slice())
					{
						errors.push(CommandConfigError::Keymap {
							scope: scope_label.to_string(),
							binding_index,
							reason: format!(
								"prefix conflict between '{}' and existing '{}'",
								render_normal_sequence(&keys),
								render_normal_sequence(candidate.keys.as_slice())
							),
						});
						has_prefix_conflict = true;
						break;
					}
				}
				if has_prefix_conflict {
					conflicted = true;
					break;
				}
				staged.push(ScopedKeyBinding {
					keys,
					command_id: resolved.spec.id.clone(),
					desc: binding.desc.clone(),
				});
			}
			if conflicted {
				continue;
			}
			if should_override {
				overridden_commands.push(resolved.spec.id.clone());
			}
			*bindings = staged;
		}
	}

	pub fn register_plugin_command(&mut self, registration: PluginCommandRegistration) -> Result<(), String> {
		if BuiltinCommand::from_id(registration.id.as_str()).is_some() {
			return Err(format!("duplicate command id: {}", registration.id));
		}
		let command_id = CommandId::Plugin(registration.id.clone());
		if self.commands.contains_key(&command_id) {
			return Err(format!("duplicate command id: {}", registration.id));
		}
		self.commands.insert(command_id, CommandSpec::plugin(registration));
		Ok(())
	}

	pub fn resolve_scope_sequence(
		&self,
		scope: KeymapScope,
		keys: &[NormalSequenceKey],
	) -> BindingMatch<CommandTarget> {
		resolve_key_binding_set(&self.commands, self.bindings(scope), keys)
	}

	pub fn key_hints(&self, scope: KeymapScope, prefix: &[NormalSequenceKey]) -> Vec<FloatingWindowLine> {
		collect_key_hints(&self.commands, self.bindings(scope), prefix)
	}

	pub fn binding_sequences_for_builtin(&self, scope: KeymapScope, command: BuiltinCommand) -> Vec<String> {
		let command_id = CommandId::Builtin(command);
		self
			.bindings(scope)
			.iter()
			.filter(|binding| binding.command_id == command_id)
			.map(|binding| render_normal_sequence(binding.keys.as_slice()))
			.collect()
	}

	pub fn resolve_command_input(&self, input: &str) -> Option<ResolvedCommand> {
		let trimmed = input.trim();
		let (name, argument) = match trimmed.split_once(' ') {
			Some((name, argument)) => (name, Some(argument.trim().to_string())),
			None => (trimmed, None),
		};
		let command_id = if let Some(command) = BuiltinCommand::from_id(name) {
			CommandId::Builtin(command)
		} else if self.commands.contains_key(&CommandId::Plugin(name.to_string())) {
			CommandId::Plugin(name.to_string())
		} else {
			let alias = self.command_aliases.iter().find(|alias| alias.name == name)?;
			alias.resolved_command_id.clone()?
		};
		self.resolve_command_id_with_argument(&command_id, argument)
	}

	pub fn resolve_command_id_with_argument(
		&self,
		command_id: &CommandId,
		argument: Option<String>,
	) -> Option<ResolvedCommand> {
		let spec = self.commands.get(command_id)?.clone();
		match spec.arg_kind {
			CommandArgKind::None if argument.as_deref().is_some_and(|arg| !arg.is_empty()) => None,
			CommandArgKind::None => Some(ResolvedCommand { spec, argument: None }),
			CommandArgKind::OptionalPath | CommandArgKind::RawTail => {
				Some(ResolvedCommand { spec, argument: argument.filter(|arg| !arg.is_empty()) })
			}
		}
	}

	pub fn command_palette_matches(&self, input: &str, limit: usize) -> Vec<CommandPaletteMatch> {
		let query = input.trim();
		let candidates = self.command_palette_candidates();
		let mut matches = candidates
			.into_iter()
			.filter_map(|candidate| {
				if query.is_empty() {
					return Some((0u16, CommandPaletteMatch {
						name:                      candidate.name,
						command_id:                candidate.command_id,
						command_id_label:          candidate.command_id_label,
						description:               candidate.description,
						name_match_indices:        Vec::new(),
						command_id_match_indices:  Vec::new(),
						description_match_indices: Vec::new(),
						is_error:                  candidate.is_error,
					}));
				}

				let name_match = frizbee_match(query, candidate.name.as_str());
				let command_match = frizbee_match(query, candidate.command_id_label.as_str());
				let description_match = frizbee_match(query, candidate.description.as_str());
				let score = [name_match.as_ref(), command_match.as_ref(), description_match.as_ref()]
					.into_iter()
					.flatten()
					.map(|(score, _)| *score)
					.max()?;
				let name_match_indices = name_match.map(|(_, indices)| indices).unwrap_or_default();
				let command_id_match_indices = command_match.map(|(_, indices)| indices).unwrap_or_default();
				let description_match_indices = description_match.map(|(_, indices)| indices).unwrap_or_default();

				Some((score, CommandPaletteMatch {
					name: candidate.name,
					command_id: candidate.command_id,
					command_id_label: candidate.command_id_label,
					description: candidate.description,
					name_match_indices,
					command_id_match_indices,
					description_match_indices,
					is_error: candidate.is_error,
				}))
			})
			.collect::<Vec<_>>();

		matches.sort_by(|left, right| {
			left
				.1
				.name
				.is_empty()
				.cmp(&right.1.name.is_empty())
				.then_with(|| right.0.cmp(&left.0))
				.then_with(|| left.1.name.cmp(&right.1.name))
				.then_with(|| left.1.command_id_label.cmp(&right.1.command_id_label))
		});

		matches.into_iter().take(limit).map(|(_, item)| item).collect()
	}

	fn command_palette_candidates(&self) -> Vec<CommandPaletteCandidate> {
		let mut candidates = Vec::with_capacity(self.command_aliases.len().max(self.commands.len()));
		let mut aliased_command_ids = HashSet::new();

		for alias in &self.command_aliases {
			if let Some(command_id) = alias.resolved_command_id.as_ref() {
				aliased_command_ids.insert(command_id.clone());
			}
			candidates.push(CommandPaletteCandidate {
				name:             alias.name.clone(),
				command_id:       alias
					.resolved_command_id
					.clone()
					.unwrap_or_else(|| CommandId::Plugin(alias.run.render())),
				command_id_label: alias
					.resolved_command_id
					.as_ref()
					.map(CommandId::display_text)
					.unwrap_or_else(|| alias.run.render()),
				description:      match (
					alias.error.as_deref(),
					alias.desc.as_deref(),
					alias.resolved_command_id.as_ref(),
				) {
					(Some(_), Some(_), _) => "invalid command".to_string(),
					(Some(_), None, _) => "invalid command".to_string(),
					(None, Some(desc), _) => desc.to_string(),
					(None, None, Some(command_id)) => {
						self.commands.get(command_id).map(|spec| spec.description.clone()).unwrap_or_default()
					}
					(None, None, None) => "invalid command".to_string(),
				},
				is_error:         alias.error.is_some(),
			});
		}

		let mut unaliased_specs = self
			.commands
			.values()
			.filter(|spec| !aliased_command_ids.contains(&spec.id))
			.cloned()
			.collect::<Vec<_>>();
		unaliased_specs.sort_by(|left, right| left.id.display_text().cmp(&right.id.display_text()));
		for spec in unaliased_specs {
			candidates.push(CommandPaletteCandidate {
				name:             String::new(),
				command_id_label: spec.id.display_text(),
				command_id:       spec.id,
				description:      spec.description,
				is_error:         false,
			});
		}

		candidates
	}

	fn resolve_run_directive(&self, run: &RunDirective) -> Option<ResolvedCommand> {
		match run {
			RunDirective::Builtin(command) => {
				let spec = self.commands.get(&CommandId::Builtin(*command))?.clone();
				Some(ResolvedCommand { spec, argument: None })
			}
			RunDirective::PluginInvocation { .. } | RunDirective::Unresolved(_) => None,
		}
	}

	fn resolve_or_register_run_directive(&mut self, run: &RunDirective) -> Option<ResolvedCommand> {
		if let Some(resolved) = self.resolve_run_directive(run) {
			return Some(resolved);
		}
		self.resolve_plugin_run_directive(run)
	}

	fn resolve_plugin_run_directive(&mut self, run: &RunDirective) -> Option<ResolvedCommand> {
		let RunDirective::PluginInvocation { plugin_name, argument } = run else {
			return None;
		};
		let command_id = CommandId::Plugin(format!("plugin.{}", plugin_name));
		if !self.commands.contains_key(&command_id) {
			self.commands.insert(command_id.clone(), CommandSpec {
				id:           command_id.clone(),
				category:     CommandCategoryInfo::plugin("plugin"),
				description:  format!("Plugin command '{}'", plugin_name),
				arg_kind:     CommandArgKind::RawTail,
				target:       CommandTarget::Plugin {
					plugin_id:  plugin_name.clone(),
					command_id: plugin_name.clone(),
				},
				display_name: None,
			});
		}

		let spec = self.commands.get(&command_id)?.clone();
		Some(ResolvedCommand { spec, argument: argument.clone() })
	}

	fn bindings(&self, scope: KeymapScope) -> &[ScopedKeyBinding] {
		match scope {
			KeymapScope::ModeNormal => self.normal_bindings.as_slice(),
			KeymapScope::ModeVisual => self.visual_bindings.as_slice(),
			KeymapScope::ModeCommand => self.command_mode_bindings.as_slice(),
			KeymapScope::ModeInsert => self.insert_mode_bindings.as_slice(),
			KeymapScope::OverlayWhichKey => self.overlay_whichkey_bindings.as_slice(),
			KeymapScope::OverlayCommandPalette => self.overlay_command_palette_bindings.as_slice(),
			KeymapScope::OverlayPicker => self.overlay_picker_bindings.as_slice(),
			KeymapScope::OverlayNotificationCenter => self.overlay_notification_center_bindings.as_slice(),
		}
	}

	fn bindings_mut(&mut self, scope: KeymapScope) -> &mut Vec<ScopedKeyBinding> {
		match scope {
			KeymapScope::ModeNormal => &mut self.normal_bindings,
			KeymapScope::ModeVisual => &mut self.visual_bindings,
			KeymapScope::ModeCommand => &mut self.command_mode_bindings,
			KeymapScope::ModeInsert => &mut self.insert_mode_bindings,
			KeymapScope::OverlayWhichKey => &mut self.overlay_whichkey_bindings,
			KeymapScope::OverlayCommandPalette => &mut self.overlay_command_palette_bindings,
			KeymapScope::OverlayPicker => &mut self.overlay_picker_bindings,
			KeymapScope::OverlayNotificationCenter => &mut self.overlay_notification_center_bindings,
		}
	}

	pub fn export_config(&self) -> CommandConfigFile {
		let normal = export_keymap_bindings(&self.commands, self.bindings(KeymapScope::ModeNormal));
		let visual = export_keymap_bindings(&self.commands, self.bindings(KeymapScope::ModeVisual));
		let command_mode = export_keymap_bindings(&self.commands, self.bindings(KeymapScope::ModeCommand));
		let insert_mode = export_keymap_bindings(&self.commands, self.bindings(KeymapScope::ModeInsert));
		let overlay_whichkey =
			export_keymap_bindings(&self.commands, self.bindings(KeymapScope::OverlayWhichKey));
		let overlay_command_palette =
			export_keymap_bindings(&self.commands, self.bindings(KeymapScope::OverlayCommandPalette));
		let overlay_picker = export_keymap_bindings(&self.commands, self.bindings(KeymapScope::OverlayPicker));
		let overlay_notification_center =
			export_keymap_bindings(&self.commands, self.bindings(KeymapScope::OverlayNotificationCenter));

		let mut command = Vec::with_capacity(self.command_aliases.len());
		for alias in &self.command_aliases {
			let Some(command_id) = alias.resolved_command_id.as_ref() else {
				continue;
			};
			let Some(spec) = self.commands.get(command_id) else {
				continue;
			};
			command.push(CommandAliasConfig {
				name: alias.name.clone(),
				run:  alias.run.clone(),
				desc: alias.desc.clone().or_else(|| Some(spec.description.clone())),
			});
		}

		CommandConfigFile {
			mode:    ModeKeymapSections {
				normal:  CommandKeymapSection { keymap: normal },
				visual:  CommandKeymapSection { keymap: visual },
				command: CommandKeymapSection { keymap: command_mode },
				insert:  CommandKeymapSection { keymap: insert_mode },
			},
			overlay: OverlayKeymapSections {
				whichkey:            CommandKeymapSection { keymap: overlay_whichkey },
				command_palette:     CommandKeymapSection { keymap: overlay_command_palette },
				picker:              CommandKeymapSection { keymap: overlay_picker },
				notification_center: CommandKeymapSection { keymap: overlay_notification_center },
			},
			command: CommandAliasSection { commands: command },
		}
	}

	fn register_builtin_specs(&mut self) {
		for command in BuiltinCommand::all_commands() {
			self.commands.insert(CommandId::Builtin(command), CommandSpec::builtin(command));
		}
	}
}

fn frizbee_match(query: &str, haystack: &str) -> Option<(u16, Vec<usize>)> {
	let config = FrizbeeConfig::default();
	let matched = match_list_indices(query, &[haystack], &config).into_iter().next()?;
	let mut indices = matched.indices;
	indices.sort_unstable();
	Some((matched.score, indices))
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CommandConfigFile {
	#[serde(default)]
	pub mode:    ModeKeymapSections,
	#[serde(default)]
	pub overlay: OverlayKeymapSections,
	#[serde(default)]
	pub command: CommandAliasSection,
}

impl CommandConfigFile {
	pub fn with_defaults() -> Self { defaults::default_command_config().clone() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandConfigError {
	Keymap { scope: String, binding_index: usize, reason: String },
	CommandAlias { alias_index: usize, reason: String },
}

impl std::fmt::Display for CommandConfigError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Keymap { scope, binding_index, reason } => {
				write!(f, "{}.keymap[{}]: {}", scope, binding_index, reason)
			}
			Self::CommandAlias { alias_index, reason } => {
				write!(f, "command.commands[{}]: {}", alias_index, reason)
			}
		}
	}
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CommandKeymapSection {
	#[serde(default)]
	pub keymap: Vec<KeymapBindingConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ModeKeymapSections {
	#[serde(default, alias = "mgr")]
	pub normal:  CommandKeymapSection,
	#[serde(default)]
	pub visual:  CommandKeymapSection,
	#[serde(default)]
	pub command: CommandKeymapSection,
	#[serde(default)]
	pub insert:  CommandKeymapSection,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct OverlayKeymapSections {
	#[serde(default)]
	pub whichkey:            CommandKeymapSection,
	#[serde(default)]
	pub command_palette:     CommandKeymapSection,
	#[serde(default)]
	pub picker:              CommandKeymapSection,
	#[serde(default)]
	pub notification_center: CommandKeymapSection,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CommandAliasSection {
	#[serde(default)]
	pub commands: Vec<CommandAliasConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeymapBindingConfig {
	pub on:   KeyBindingOn,
	pub run:  RunDirective,
	pub desc: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyBindingOn(Vec<String>);

impl KeyBindingOn {
	pub fn single(token: impl Into<String>) -> Self { Self(vec![token.into()]) }

	pub fn many(tokens: Vec<String>) -> Self { Self(tokens) }

	pub fn entries(&self) -> &[String] { self.0.as_slice() }

	fn parse_bindings(&self) -> Result<Vec<Vec<NormalSequenceKey>>, String> {
		if self.0.is_empty() {
			return Err("empty key binding list".to_string());
		}
		self.0.iter().map(|token| parse_normal_sequence(token)).collect()
	}

	fn display_for_error(&self) -> String {
		match self.0.as_slice() {
			[single] => single.clone(),
			many => format!("[{}]", many.join(",")),
		}
	}
}

impl<'de> Deserialize<'de> for KeyBindingOn {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where D: serde::Deserializer<'de> {
		#[derive(Deserialize)]
		#[serde(untagged)]
		enum KeyBindingOnSerde {
			Single(String),
			Many(Vec<String>),
		}

		match KeyBindingOnSerde::deserialize(deserializer)? {
			KeyBindingOnSerde::Single(token) => Ok(Self::single(token)),
			KeyBindingOnSerde::Many(tokens) => Ok(Self::many(tokens)),
		}
	}
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandAliasConfig {
	pub name: String,
	pub run:  RunDirective,
	pub desc: Option<String>,
}

trait KeyBindingView {
	fn keys(&self) -> &[NormalSequenceKey];
	fn command_id(&self) -> &CommandId;
	fn desc(&self) -> Option<&str>;
}

impl KeyBindingView for ScopedKeyBinding {
	fn keys(&self) -> &[NormalSequenceKey] { self.keys.as_slice() }

	fn command_id(&self) -> &CommandId { &self.command_id }

	fn desc(&self) -> Option<&str> { self.desc.as_deref() }
}

#[derive(Debug, Clone)]
pub struct PluginCommandRegistration {
	pub id:          String,
	pub plugin_id:   String,
	pub command_id:  String,
	pub category:    String,
	pub description: String,
	pub arg_kind:    CommandArgKind,
}

fn resolve_key_binding_set<T>(
	commands: &HashMap<CommandId, CommandSpec>,
	bindings: &[T],
	keys: &[NormalSequenceKey],
) -> BindingMatch<CommandTarget>
where
	T: KeyBindingView,
{
	let mut has_prefix = false;
	for binding in bindings {
		if binding.keys() == keys
			&& let Some(spec) = commands.get(binding.command_id())
		{
			return BindingMatch::Exact(spec.target.clone());
		}
		if binding.keys().starts_with(keys) {
			has_prefix = true;
		}
	}
	if has_prefix { BindingMatch::Pending } else { BindingMatch::NoMatch }
}

fn export_keymap_bindings<T>(
	commands: &HashMap<CommandId, CommandSpec>,
	bindings: &[T],
) -> Vec<KeymapBindingConfig>
where
	T: KeyBindingView,
{
	let mut exported = Vec::with_capacity(bindings.len());
	for binding in bindings {
		let Some(spec) = commands.get(binding.command_id()) else {
			continue;
		};
		exported.push(KeymapBindingConfig {
			on:   KeyBindingOn::single(render_normal_sequence(binding.keys())),
			run:  match binding.command_id() {
				CommandId::Builtin(command) => RunDirective::Builtin(*command),
				CommandId::Plugin(command_id) => RunDirective::Unresolved(command_id.clone()),
			},
			desc: binding.desc().map(ToString::to_string).or_else(|| Some(spec.description.clone())),
		});
	}
	exported
}

fn collect_key_hints(
	commands: &HashMap<CommandId, CommandSpec>,
	bindings: &[ScopedKeyBinding],
	prefix: &[NormalSequenceKey],
) -> Vec<FloatingWindowLine> {
	#[derive(Default)]
	struct HintAggregate {
		exact_description: Option<String>,
		exact_category:    Option<CommandCategoryInfo>,
		child_categories:  Vec<CommandCategoryInfo>,
		has_children:      bool,
	}

	let mut aggregates: BTreeMap<NormalSequenceKey, HintAggregate> = BTreeMap::new();
	for binding in bindings {
		let keys = binding.keys();
		if !keys.starts_with(prefix) || keys.len() <= prefix.len() {
			continue;
		}
		let next = keys[prefix.len()];
		let Some(spec) = commands.get(binding.command_id()) else {
			continue;
		};
		let aggregate = aggregates.entry(next).or_default();
		if keys.len() == prefix.len().saturating_add(1) {
			aggregate.exact_description = Some(binding.desc().unwrap_or(spec.description.as_str()).to_string());
			aggregate.exact_category = Some(spec.category.clone());
		} else {
			aggregate.has_children = true;
			aggregate.child_categories.push(spec.category.clone());
		}
	}

	aggregates
		.into_iter()
		.map(|(key, aggregate)| {
			let summary = if aggregate.has_children {
				let category = common_category_label(aggregate.child_categories.as_slice())
					.or(aggregate.exact_category.as_ref())
					.map(|category| category.label.as_str())
					.unwrap_or("more");
				format!("+{}", category)
			} else {
				aggregate.exact_description.unwrap_or_else(|| "+more".to_string())
			};
			FloatingWindowLine { key: render_normal_sequence(&[key]), summary, is_prefix: aggregate.has_children }
		})
		.collect()
}

fn common_category_label(categories: &[CommandCategoryInfo]) -> Option<&CommandCategoryInfo> {
	let first = categories.first()?;
	if categories.iter().all(|candidate| candidate == first) { Some(first) } else { None }
}

fn parse_normal_sequence(input: &str) -> Result<Vec<NormalSequenceKey>, String> {
	let mut result = Vec::new();
	let mut chars = input.chars().peekable();
	while let Some(ch) = chars.next() {
		if ch == '<' {
			let mut token = String::new();
			loop {
				let Some(next) = chars.next() else {
					return Err("unterminated <> token".to_string());
				};
				if next == '>' {
					break;
				}
				token.push(next);
			}
			let lowered = token.to_ascii_lowercase();
			if lowered == "leader" {
				result.push(NormalSequenceKey::Leader);
				continue;
			}
			if lowered == "tab" {
				result.push(NormalSequenceKey::Tab);
				continue;
			}
			if lowered == "esc" {
				result.push(NormalSequenceKey::Esc);
				continue;
			}
			if lowered == "enter" {
				result.push(NormalSequenceKey::Enter);
				continue;
			}
			if lowered == "backspace" {
				result.push(NormalSequenceKey::Backspace);
				continue;
			}
			if lowered == "f1" {
				result.push(NormalSequenceKey::F1);
				continue;
			}
			if lowered == "left" {
				result.push(NormalSequenceKey::Left);
				continue;
			}
			if lowered == "right" {
				result.push(NormalSequenceKey::Right);
				continue;
			}
			if lowered == "up" {
				result.push(NormalSequenceKey::Up);
				continue;
			}
			if lowered == "down" {
				result.push(NormalSequenceKey::Down);
				continue;
			}
			if let Some(rest) = lowered.strip_prefix("c-") {
				let mut token_chars = rest.chars();
				let Some(ctrl_char) = token_chars.next() else {
					return Err("control token missing key".to_string());
				};
				if token_chars.next().is_some() {
					return Err(format!("unsupported control token: <{}>", token));
				}
				result.push(NormalSequenceKey::Ctrl(ctrl_char));
				continue;
			}
			return Err(format!("unsupported token: <{}>", token));
		}
		result.push(NormalSequenceKey::Char(ch));
	}
	if result.is_empty() {
		return Err("empty key sequence".to_string());
	}
	Ok(result)
}

fn render_normal_sequence(keys: &[NormalSequenceKey]) -> String {
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

fn is_prefix_sequence(prefix: &[NormalSequenceKey], sequence: &[NormalSequenceKey]) -> bool {
	prefix.len() < sequence.len() && sequence.starts_with(prefix)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_normal_sequence_should_support_leader_and_ctrl_tokens() {
		let keys = parse_normal_sequence("<leader><Tab><C-h>").expect("sequence should parse");

		assert_eq!(keys, vec![NormalSequenceKey::Leader, NormalSequenceKey::Tab, NormalSequenceKey::Ctrl('h'),]);
	}

	#[test]
	fn config_should_reject_conflicting_normal_binding() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::single("H"),
						run:  "core.buffer.next".into(),
						desc: Some("custom".to_string()),
					}],
				},
				..ModeKeymapSections::default()
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);

		assert_eq!(errors.len(), 1);
		assert_eq!(
			registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('H')]),
			BindingMatch::Exact(CommandTarget::Builtin(BuiltinCommand::Buffer(BufferCommand::Prev)))
		);
		assert_eq!(
			registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('L')]),
			BindingMatch::Exact(CommandTarget::Builtin(BuiltinCommand::Buffer(BufferCommand::Next)))
		);
		assert_eq!(
			registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('j')]),
			BindingMatch::Exact(CommandTarget::Builtin(BuiltinCommand::Cursor(CursorCommand::Down)))
		);
	}

	#[test]
	fn config_should_register_command_alias() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "qq".to_string(),
					run:  "core.quit_all".into(),
					desc: Some("custom".to_string()),
				}],
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);
		let resolved = registry.resolve_command_input("qq").expect("command alias should resolve");

		assert!(errors.is_empty());
		assert_eq!(
			resolved.spec.target,
			CommandTarget::Builtin(BuiltinCommand::Command(CommandCommand::QuitAll))
		);
	}

	#[test]
	fn configured_command_aliases_should_replace_default_alias_table() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "haha".to_string(),
					run:  "core.quit_all".into(),
					desc: Some("custom".to_string()),
				}],
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);

		assert!(errors.is_empty());
		assert!(registry.resolve_command_input("qa").is_none());
		assert!(registry.resolve_command_input("haha").is_some());
	}

	#[test]
	fn manager_keymap_should_report_conflict_for_array_form() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::many(vec!["gg".to_string(), "G".to_string()]),
						run:  "core.cursor.file_end".into(),
						desc: Some("custom".to_string()),
					}],
				},
				..ModeKeymapSections::default()
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);

		assert_eq!(errors.len(), 1);
		assert_eq!(
			registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[
				NormalSequenceKey::Char('g'),
				NormalSequenceKey::Char('g')
			]),
			BindingMatch::Exact(CommandTarget::Builtin(BuiltinCommand::Cursor(CursorCommand::FileStart)))
		);
		assert_eq!(
			registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('G')]),
			BindingMatch::Exact(CommandTarget::Builtin(BuiltinCommand::Cursor(CursorCommand::FileEnd)))
		);
	}

	#[test]
	fn normal_keymap_should_accept_plugin_run_directive() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::many(vec!["cm".to_string(), "mc".to_string()]),
						run:  "plugin chmod".into(),
						desc: Some("plugin".to_string()),
					}],
				},
				..ModeKeymapSections::default()
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);
		let resolved = registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[
			NormalSequenceKey::Char('c'),
			NormalSequenceKey::Char('m'),
		]);
		let alternate = registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[
			NormalSequenceKey::Char('m'),
			NormalSequenceKey::Char('c'),
		]);

		assert!(errors.is_empty());
		assert!(matches!(resolved, BindingMatch::Exact(CommandTarget::Plugin { .. })));
		assert!(matches!(alternate, BindingMatch::Exact(CommandTarget::Plugin { .. })));
	}

	#[test]
	fn key_hints_should_group_multi_key_prefixes() {
		let registry = CommandRegistry::with_defaults();

		let leader_hints = registry.key_hints(KeymapScope::ModeNormal, &[NormalSequenceKey::Leader]);

		assert!(leader_hints.iter().any(|hint| hint.key == "b" && hint.summary == "+buffer" && hint.is_prefix));
		assert!(leader_hints.iter().any(|hint| hint.key == "w" && hint.summary == "+window" && hint.is_prefix));
	}

	#[test]
	fn key_hints_should_describe_non_leader_multi_key_sequences() {
		let registry = CommandRegistry::with_defaults();

		let hints = registry.key_hints(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('g')]);

		assert_eq!(hints.len(), 1);
		assert_eq!(hints[0].key, "g");
		assert_eq!(hints[0].summary, "Move to file start");
		assert!(!hints[0].is_prefix);
	}

	#[test]
	fn configured_keymap_desc_should_override_key_hint_summary() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::single("gg"),
						run:  "core.cursor.file_start".into(),
						desc: Some("Jump to beginning".to_string()),
					}],
				},
				..ModeKeymapSections::default()
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);
		let hints = registry.key_hints(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('g')]);

		assert!(errors.is_empty());
		assert_eq!(hints.len(), 1);
		assert_eq!(hints[0].summary, "Jump to beginning");
	}

	#[test]
	fn default_export_should_include_f1_key_hint_binding() {
		let config = CommandRegistry::with_defaults().export_config();

		assert!(config.mode.normal.keymap.iter().any(|binding| {
			matches!(binding.on.entries(), [token] if token == "<F1>")
				&& binding.run == RunDirective::Builtin(BuiltinCommand::Help(HelpCommand::Keymap))
		}));
		assert!(config.mode.visual.keymap.iter().any(|binding| {
			matches!(binding.on.entries(), [token] if token == "<F1>")
				&& binding.run == RunDirective::Builtin(BuiltinCommand::Help(HelpCommand::Keymap))
		}));
		assert!(config.overlay.whichkey.keymap.iter().any(|binding| {
			matches!(binding.on.entries(), [token] if token == "<F1>")
				&& binding.run == RunDirective::Builtin(BuiltinCommand::Help(HelpCommand::Keymap))
		}));
		assert!(config.overlay.command_palette.keymap.iter().any(|binding| {
			matches!(binding.on.entries(), [token] if token == "<F1>")
				&& binding.run == RunDirective::Builtin(BuiltinCommand::Help(HelpCommand::Keymap))
		}));
		assert!(config.overlay.picker.keymap.iter().any(|binding| {
			matches!(binding.on.entries(), [token] if token == "<F1>")
				&& binding.run == RunDirective::Builtin(BuiltinCommand::Help(HelpCommand::Keymap))
		}));
		assert!(config.overlay.notification_center.keymap.iter().any(|binding| {
			matches!(binding.on.entries(), [token] if token == "<F1>")
				&& binding.run == RunDirective::Builtin(BuiltinCommand::Help(HelpCommand::Keymap))
		}));
	}

	#[test]
	fn command_palette_should_match_command_ids_and_descriptions() {
		let registry = CommandRegistry::with_defaults();

		let id_matches = registry.command_palette_matches("yazi", 12);
		let desc_matches = registry.command_palette_matches("yazi picker", 12);

		assert!(
			id_matches
				.iter()
				.any(|item| item.command_id == CommandId::Builtin(BuiltinCommand::Picker(PickerCommand::Yazi)))
		);
		assert!(
			desc_matches
				.iter()
				.any(|item| item.command_id == CommandId::Builtin(BuiltinCommand::Picker(PickerCommand::Yazi)))
		);
		assert!(id_matches.iter().any(|item| {
			item.name == "yazi"
				&& item.command_id == CommandId::Builtin(BuiltinCommand::Picker(PickerCommand::Yazi))
		}));
		assert!(
			id_matches
				.iter()
				.find(|item| item.command_id == CommandId::Builtin(BuiltinCommand::Picker(PickerCommand::Yazi)))
				.is_some_and(|item| !item.name_match_indices.is_empty())
		);
	}

	#[test]
	fn empty_command_palette_should_include_unaliased_commands_with_blank_name() {
		let registry = CommandRegistry::with_defaults();

		let matches = registry.command_palette_matches("", 128);

		assert!(matches.iter().any(|item| {
			item.name == "yazi"
				&& item.command_id == CommandId::Builtin(BuiltinCommand::Picker(PickerCommand::Yazi))
		}));
		assert!(matches.iter().any(|item| {
			item.name.is_empty()
				&& item.command_id == CommandId::Builtin(BuiltinCommand::Window(WindowCommand::SplitVertical))
		}));
		let yazi_position = matches
			.iter()
			.position(|item| {
				item.name == "yazi"
					&& item.command_id == CommandId::Builtin(BuiltinCommand::Picker(PickerCommand::Yazi))
			})
			.expect("aliased command should be listed");
		let unaliased_position = matches
			.iter()
			.position(|item| {
				item.name.is_empty()
					&& item.command_id == CommandId::Builtin(BuiltinCommand::Window(WindowCommand::SplitVertical))
			})
			.expect("unaliased command should be listed");
		assert!(yazi_position < unaliased_position);
	}

	#[test]
	fn resolve_command_input_should_accept_direct_command_id() {
		let registry = CommandRegistry::with_defaults();

		let resolved = registry.resolve_command_input("core.tab.new").expect("direct command id should resolve");

		assert_eq!(resolved.spec.id, CommandId::Builtin(BuiltinCommand::Tab(TabCommand::New)));
	}

	#[test]
	fn invalid_command_alias_should_still_appear_in_command_palette_as_error() {
		let mut registry = CommandRegistry::with_defaults();
		let errors = registry.apply_config(&CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "bad".to_string(),
					run:  "core.not.exists".into(),
					desc: Some("Broken alias".to_string()),
				}],
			},
			..CommandConfigFile::default()
		});

		assert_eq!(errors.len(), 1);

		let matches = registry.command_palette_matches("bad", 16);
		let item = matches.iter().find(|item| item.name == "bad").expect("invalid alias should be visible");
		assert!(item.is_error);
		assert_eq!(item.description, "invalid command");
	}

	#[test]
	fn config_should_report_prefix_conflict_for_keymap_binding() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::single("g"),
						run:  "core.cursor.down".into(),
						desc: Some("conflict".to_string()),
					}],
				},
				..ModeKeymapSections::default()
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);

		assert_eq!(errors.len(), 1);
		assert!(matches!(
			&errors[0],
			CommandConfigError::Keymap { reason, .. } if reason.contains("prefix conflict")
		));
	}

	#[test]
	fn config_should_report_exact_key_conflict_for_different_command() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::single("p"),
						run:  "core.cursor.down".into(),
						desc: Some("conflict".to_string()),
					}],
				},
				..ModeKeymapSections::default()
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);

		assert_eq!(errors.len(), 1);
		assert!(matches!(
			&errors[0],
			CommandConfigError::Keymap { reason, .. } if reason.contains("conflicting key binding")
		));
	}
}
