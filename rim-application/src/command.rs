use std::{collections::{BTreeMap, HashMap, HashSet}, path::PathBuf};

use frizbee::{Config as FrizbeeConfig, match_list_indices};
use rim_command_macros::{BuiltinCommandGroup, BuiltinCommandRoot};
use serde::{Deserialize, Serialize};

use crate::{action::{AppAction, BufferAction, EditorAction, LayoutAction, TabAction, WindowAction}, defaults, state::{FloatingWindowLine, KeymapScope, NormalSequenceKey}};

pub trait BuiltinCommandGroupMeta: Copy {
	fn command_segment(self) -> &'static str;
	fn description(self) -> &'static str;
	fn params(self) -> &'static [BuiltinCommandParamSpec];
	fn all_commands() -> &'static [Self];
}

pub trait BuiltinCommandRootMeta: Copy {
	fn id(self) -> String;
	fn category(self) -> BuiltinCommandCategory;
	fn description(self) -> &'static str;
	fn params(self) -> &'static [BuiltinCommandParamSpec];
	fn all_commands() -> Vec<Self>;
	fn from_id(id: &str) -> Option<Self>;
}

/// Zero-sized marker used only by builtin command declaration parsing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct File;

/// Zero-sized marker used only by builtin command declaration parsing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinCommandParamSpec {
	pub name:     &'static str,
	pub kind:     CommandArgKind,
	pub optional: bool,
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
	Save { path: Option<File> },
	/// Force save current buffer
	SaveForce { path: Option<File> },
	/// Save all file-backed buffers
	SaveAll,
	/// Save current buffer and quit
	SaveAndQuit { path: Option<File> },
	/// Force save current buffer and quit
	SaveAndQuitForce { path: Option<File> },
	/// Save all file-backed buffers and quit
	SaveAllAndQuit,
	/// Force save all file-backed buffers and quit
	SaveAllAndQuitForce,
	/// Reload current buffer
	Reload { path: Option<File> },
	/// Force reload current buffer
	ReloadForce { path: Option<File> },
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
	PluginInvocation { plugin_name: String },
	Unresolved(String),
}

impl RunDirective {
	pub fn parse(text: &str) -> Self {
		let trimmed = text.trim();
		if let Some(command) = BuiltinCommand::from_id(trimmed) {
			return Self::Builtin(command);
		}
		if let Some(plugin_name) = trimmed.strip_prefix("plugin.")
			&& parse_plugin_command_name(plugin_name).is_some()
		{
			return Self::PluginInvocation { plugin_name: plugin_name.to_string() };
		}
		if let Some(payload) = trimmed.strip_prefix("plugin ") {
			let payload = payload.trim();
			if payload.is_empty() {
				return Self::Unresolved(trimmed.to_string());
			}
			let mut segments = payload.split_whitespace();
			let plugin_name = segments.next().expect("plugin command is non-empty").to_string();
			if segments.next().is_some() || parse_plugin_command_name(plugin_name.as_str()).is_none() {
				return Self::Unresolved(trimmed.to_string());
			}
			return Self::PluginInvocation { plugin_name };
		}
		Self::Unresolved(trimmed.to_string())
	}

	pub fn render(&self) -> String {
		match self {
			Self::Builtin(command) => command.id(),
			Self::PluginInvocation { plugin_name } => format!("plugin.{}", plugin_name),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandArgKind {
	Text,
	File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandParamSpec {
	pub name:     String,
	pub kind:     CommandArgKind,
	pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandValue {
	Text(String),
	File(String),
}

impl CommandValue {
	pub fn as_str(&self) -> &str {
		match self {
			Self::Text(value) | Self::File(value) => value.as_str(),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedParam {
	pub name:  String,
	pub kind:  CommandArgKind,
	pub value: CommandValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedParams(Vec<ResolvedParam>);

impl ResolvedParams {
	pub fn new(params: Vec<ResolvedParam>) -> Self { Self(params) }

	pub fn as_slice(&self) -> &[ResolvedParam] { self.0.as_slice() }

	pub fn iter(&self) -> impl Iterator<Item = &ResolvedParam> { self.0.iter() }

	pub fn len(&self) -> usize { self.0.len() }

	pub fn is_empty(&self) -> bool { self.0.is_empty() }

	pub fn get(&self, name: &str) -> Option<&ResolvedParam> { self.0.iter().find(|param| param.name == name) }

	pub fn get_text(&self, name: &str) -> Option<&str> {
		match &self.get(name)?.value {
			CommandValue::Text(value) => Some(value.as_str()),
			CommandValue::File(_) => None,
		}
	}

	pub fn get_file(&self, name: &str) -> Option<&str> {
		match &self.get(name)?.value {
			CommandValue::File(value) => Some(value.as_str()),
			CommandValue::Text(_) => None,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
	pub id:           CommandId,
	pub category:     CommandCategoryInfo,
	pub description:  String,
	pub params:       Vec<CommandParamSpec>,
	pub target:       CommandTarget,
	pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCommand {
	pub command_id: CommandId,
	pub target:     CommandTarget,
	pub argv:       Vec<String>,
	pub params:     ResolvedParams,
}

impl ResolvedCommand {
	pub fn get_param(&self, name: &str) -> Option<&ResolvedParam> { self.params.get(name) }

	pub fn get_text(&self, name: &str) -> Option<&str> { self.params.get_text(name) }

	pub fn get_file(&self, name: &str) -> Option<&str> { self.params.get_file(name) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResolveError {
	UnknownCommand { input: String },
	InvalidSyntax { message: String },
	MissingRequiredParams { command_id: String, missing: Vec<String> },
	TooManyArguments { command_id: String, expected: usize, actual: usize },
}

impl std::fmt::Display for CommandResolveError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::UnknownCommand { input } => write!(f, "unknown command: {}", input),
			Self::InvalidSyntax { message } => write!(f, "invalid command syntax: {}", message),
			Self::MissingRequiredParams { command_id, missing } => {
				write!(f, "missing required parameters for {}: {}", command_id, missing.join(", "))
			}
			Self::TooManyArguments { command_id, expected, actual } => {
				write!(f, "too many arguments for {}: expected at most {}, got {}", command_id, expected, actual)
			}
		}
	}
}

impl CommandSpec {
	fn builtin(command: BuiltinCommand) -> Self {
		Self {
			id:           CommandId::Builtin(command),
			category:     CommandCategoryInfo::builtin(command.category()),
			description:  command.description().to_string(),
			params:       builtin_params_to_runtime(command.params()),
			target:       CommandTarget::Builtin(command),
			display_name: Some(derived_display_name(command)),
		}
	}

	fn plugin(registration: PluginCommandRegistration) -> Self {
		Self {
			id:           CommandId::Plugin(registration.id.clone()),
			category:     CommandCategoryInfo::plugin(registration.category),
			description:  registration.description,
			params:       registration.params,
			target:       CommandTarget::Plugin {
				plugin_id:  registration.plugin_id,
				command_id: registration.command_id,
			},
			display_name: None,
		}
	}
}

fn builtin_params_to_runtime(params: &[BuiltinCommandParamSpec]) -> Vec<CommandParamSpec> {
	params
		.iter()
		.map(|param| CommandParamSpec {
			name:     param.name.to_string(),
			kind:     param.kind,
			optional: param.optional,
		})
		.collect()
}

fn derived_display_name(command: BuiltinCommand) -> String {
	normalize_pascal_case_name(command.id().rsplit('.').next().unwrap_or_default())
}

fn render_param_summary(params: &[CommandParamSpec]) -> String {
	params
		.iter()
		.map(|param| if param.optional { format!("[{}]", param.name) } else { format!("<{}>", param.name) })
		.collect::<Vec<_>>()
		.join(" ")
}

fn format_command_palette_command_label(command_id: &CommandId, params: &[CommandParamSpec]) -> String {
	let summary = render_param_summary(params);
	if summary.is_empty() {
		command_id.display_text()
	} else {
		format!("{} {}", command_id.display_text(), summary)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteMatch {
	pub name:                      String,
	pub completion:                String,
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
	alternate_name:   Option<String>,
	completion:       String,
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
	target:     CommandTarget,
	args:       Vec<String>,
	desc:       Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandAlias {
	name:                String,
	resolved_command_id: Option<CommandId>,
	target:              Option<CommandTarget>,
	args:                Vec<String>,
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
		let blocking_errors =
			errors.into_iter().filter(|error| !is_deferred_plugin_alias_error(error)).collect::<Vec<_>>();
		assert!(
			blocking_errors.is_empty(),
			"embedded default command preset contains invalid entries: {:?}",
			blocking_errors
		);
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
					target:              None,
					args:                alias.args.clone(),
					run:                 alias.run.clone(),
					desc:                alias.desc.clone(),
					error:               Some(error),
				});
				continue;
			};
			self.command_aliases.push(CommandAlias {
				name:                alias.name.clone(),
				resolved_command_id: Some(resolved.command_id.clone()),
				target:              Some(resolved.target.clone()),
				args:                alias.args.clone(),
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
			let should_override = !overridden_commands.iter().any(|command_id| command_id == &resolved.command_id);
			let mut staged = if should_override {
				bindings
					.iter()
					.filter(|candidate| candidate.command_id != resolved.command_id)
					.cloned()
					.collect::<Vec<_>>()
			} else {
				bindings.clone()
			};
			let mut conflicted = false;
			for keys in key_sets {
				if let Some(existing) = staged.iter_mut().find(|candidate| candidate.keys == keys) {
					if existing.command_id != resolved.command_id {
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
					existing.target = resolved.target.clone();
					existing.args = binding.args.clone();
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
					command_id: resolved.command_id.clone(),
					target: resolved.target.clone(),
					args: binding.args.clone(),
					desc: binding.desc.clone(),
				});
			}
			if conflicted {
				continue;
			}
			if should_override {
				overridden_commands.push(resolved.command_id.clone());
			}
			*bindings = staged;
		}
	}

	pub fn register_plugin_command(&mut self, registration: PluginCommandRegistration) -> Result<(), String> {
		if BuiltinCommand::from_id(registration.id.as_str()).is_some() {
			return Err(format!("duplicate command id: {}", registration.id));
		}
		let command_id = CommandId::Plugin(registration.id.clone());
		let default_name = registration.default_name.clone();
		let plugin_id = registration.plugin_id.clone();
		let plugin_command_id = registration.command_id.clone();
		let description = registration.description.clone();
		let next_spec = CommandSpec::plugin(registration);
		if let Some(existing) = self.commands.get(&command_id)
			&& existing.target != next_spec.target
		{
			return Err(format!("duplicate command id: {}", command_id.display_text()));
		}
		self.commands.insert(command_id.clone(), next_spec);
		self.register_plugin_default_alias(command_id, default_name, plugin_id, plugin_command_id, description);
		self.refresh_deferred_command_aliases();
		Ok(())
	}

	fn register_plugin_default_alias(
		&mut self,
		command_id: CommandId,
		default_name: String,
		plugin_id: String,
		plugin_command_id: String,
		description: String,
	) {
		let default_name = normalize_pascal_case_name(default_name.trim());
		if default_name.is_empty() {
			return;
		}
		if self.command_aliases.iter().any(|alias| alias.resolved_command_id.as_ref() == Some(&command_id)) {
			return;
		}
		if self.command_aliases.iter().any(|alias| normalize_pascal_case_name(alias.name.trim()) == default_name)
		{
			return;
		}
		self.command_aliases.push(CommandAlias {
			name:                default_name,
			resolved_command_id: Some(command_id),
			target:              Some(CommandTarget::Plugin {
				plugin_id:  plugin_id.clone(),
				command_id: plugin_command_id.clone(),
			}),
			args:                Vec::new(),
			run:                 RunDirective::PluginInvocation {
				plugin_name: format!("{}.{}", plugin_id, plugin_command_id),
			},
			desc:                Some(description),
			error:               None,
		});
	}

	fn refresh_deferred_command_aliases(&mut self) {
		let deferred = self
			.command_aliases
			.iter()
			.enumerate()
			.filter_map(|(index, alias)| {
				(alias.resolved_command_id.is_none()).then_some((index, alias.run.clone()))
			})
			.collect::<Vec<_>>();
		for (index, run) in deferred {
			let Some(resolved) = self.resolve_or_register_run_directive(&run) else {
				continue;
			};
			let Some(alias) = self.command_aliases.get_mut(index) else {
				continue;
			};
			alias.resolved_command_id = Some(resolved.command_id);
			alias.target = Some(resolved.target);
			alias.error = None;
		}
	}

	pub fn resolve_scope_sequence(
		&self,
		scope: KeymapScope,
		keys: &[NormalSequenceKey],
	) -> BindingMatch<ResolvedCommand> {
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

	pub fn command_spec(&self, command_id: &CommandId) -> Option<&CommandSpec> { self.commands.get(command_id) }

	pub fn resolve_command_input(&self, input: &str) -> Result<ResolvedCommand, CommandResolveError> {
		let trimmed = input.trim_start();
		let Some((resolution, raw_argument_input, _)) = self.resolve_command_prefix(trimmed) else {
			let unknown =
				trimmed.split_whitespace().next().map(str::to_string).unwrap_or_else(|| input.trim().to_string());
			return Err(CommandResolveError::UnknownCommand { input: unknown });
		};
		let parsed = tokenize_command_input(raw_argument_input)
			.map_err(|message| CommandResolveError::InvalidSyntax { message })?;
		let mut argv = resolution.argv_prefix;
		argv.extend(parsed.tokens);
		self.resolve_command_id_with_argv(&resolution.command_id, &resolution.target, argv)
	}

	pub fn resolve_command_id_with_argv(
		&self,
		command_id: &CommandId,
		target: &CommandTarget,
		argv: Vec<String>,
	) -> Result<ResolvedCommand, CommandResolveError> {
		let spec = self
			.commands
			.get(command_id)
			.ok_or_else(|| CommandResolveError::UnknownCommand { input: command_id.display_text() })?;
		validate_command_argv(spec, argv.as_slice())?;
		let params = resolved_params_from_argv(spec, argv.as_slice());
		Ok(ResolvedCommand { command_id: command_id.clone(), target: target.clone(), argv, params })
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
						completion:                candidate.completion,
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
				let alternate_name_match =
					candidate.alternate_name.as_deref().and_then(|text| frizbee_match(query, text));
				let command_match = frizbee_match(query, candidate.command_id_label.as_str());
				let description_match = frizbee_match(query, candidate.description.as_str());
				let mut score = name_match.as_ref().map(|(score, _)| *score).unwrap_or_default();
				score = score.max(alternate_name_match.as_ref().map(|(score, _)| *score).unwrap_or_default());
				score = score.max(command_match.as_ref().map(|(score, _)| *score).unwrap_or_default());
				score = score.max(description_match.as_ref().map(|(score, _)| *score).unwrap_or_default());
				if score == 0 {
					return None;
				}
				let name_match_indices = name_match.map(|(_, indices)| indices).unwrap_or_default();
				let command_id_match_indices = command_match.map(|(_, indices)| indices).unwrap_or_default();
				let description_match_indices = description_match.map(|(_, indices)| indices).unwrap_or_default();

				Some((score, CommandPaletteMatch {
					name: candidate.name,
					completion: candidate.completion,
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
				alternate_name:   alias
					.resolved_command_id
					.as_ref()
					.and_then(|command_id| self.commands.get(command_id))
					.and_then(|spec| spec.display_name.clone()),
				completion:       alias
					.resolved_command_id
					.as_ref()
					.map(CommandId::display_text)
					.unwrap_or_else(|| alias.run.render()),
				command_id:       alias
					.resolved_command_id
					.clone()
					.unwrap_or_else(|| CommandId::Plugin(alias.run.render())),
				command_id_label: alias
					.resolved_command_id
					.as_ref()
					.and_then(|command_id| {
						self
							.commands
							.get(command_id)
							.map(|spec| format_command_palette_command_label(command_id, spec.params.as_slice()))
					})
					.unwrap_or_else(|| alias.run.render()),
				description:      match (
					alias.error.as_deref(),
					alias.desc.as_deref(),
					alias.resolved_command_id.as_ref(),
				) {
					(Some(_), Some(_), _) => "invalid command".to_string(),
					(Some(_), None, _) => "invalid command".to_string(),
					(None, Some(desc), Some(_)) => desc.to_string(),
					(None, Some(desc), None) => desc.to_string(),
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
				name:             spec.display_name.clone().unwrap_or_default(),
				alternate_name:   None,
				completion:       spec.id.display_text(),
				command_id_label: format_command_palette_command_label(&spec.id, spec.params.as_slice()),
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
				let spec = self.commands.get(&CommandId::Builtin(*command))?;
				Some(ResolvedCommand {
					command_id: spec.id.clone(),
					target:     spec.target.clone(),
					argv:       Vec::new(),
					params:     ResolvedParams::default(),
				})
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
		let RunDirective::PluginInvocation { plugin_name } = run else {
			return None;
		};
		let command_id = CommandId::Plugin(format!("plugin.{}", plugin_name));
		let spec = self.commands.get(&command_id)?;
		Some(ResolvedCommand {
			command_id: spec.id.clone(),
			target:     spec.target.clone(),
			argv:       Vec::new(),
			params:     ResolvedParams::default(),
		})
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
				args: alias.args.clone(),
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

	pub fn active_parameter_context(&self, input: &str) -> Option<ActiveParameterContext> {
		let trimmed = input.trim_start();
		let (resolution, raw_argument_input, has_separator) = self.resolve_command_prefix(trimmed)?;
		let spec = self.commands.get(&resolution.command_id)?;
		if spec.params.is_empty() || !has_separator {
			return None;
		}
		let parsed = tokenize_command_input(raw_argument_input).ok()?;
		let arg_tokens = parsed.tokens.as_slice();
		let active_index =
			if parsed.ends_with_separator { arg_tokens.len() } else { arg_tokens.len().checked_sub(1)? };
		let param = spec.params.get(active_index)?;
		let input = if parsed.ends_with_separator {
			String::new()
		} else {
			arg_tokens.get(active_index).cloned().unwrap_or_default()
		};
		Some(ActiveParameterContext {
			command_id: resolution.command_id,
			target: resolution.target,
			index: active_index,
			param: param.clone(),
			input,
		})
	}

	fn resolve_command_prefix<'a>(&self, input: &'a str) -> Option<(CommandTokenResolution, &'a str, bool)> {
		let trimmed = input.trim_start();
		if trimmed.is_empty() {
			return None;
		}

		let command_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
		let command_token = &trimmed[..command_end];
		let raw_argument_input = &trimmed[command_end..];
		if let Some(resolution) = self.resolve_command_token(command_token) {
			return Some((resolution, raw_argument_input, !raw_argument_input.is_empty()));
		}

		let mut display_name_matches = self
			.commands
			.values()
			.filter_map(|spec| spec.display_name.as_ref().map(|display_name| (display_name.as_str(), spec)))
			.collect::<Vec<_>>();
		display_name_matches
			.sort_by(|left, right| right.0.len().cmp(&left.0.len()).then_with(|| left.0.cmp(right.0)));
		for (display_name, spec) in display_name_matches {
			if trimmed == display_name {
				return Some((
					CommandTokenResolution {
						command_id:  spec.id.clone(),
						target:      spec.target.clone(),
						argv_prefix: Vec::new(),
					},
					"",
					false,
				));
			}
			if let Some(raw_argument_input) = trimmed.strip_prefix(display_name)
				&& raw_argument_input.chars().next().is_some_and(char::is_whitespace)
			{
				return Some((
					CommandTokenResolution {
						command_id:  spec.id.clone(),
						target:      spec.target.clone(),
						argv_prefix: Vec::new(),
					},
					raw_argument_input,
					true,
				));
			}
		}

		None
	}

	fn resolve_command_token(&self, token: &str) -> Option<CommandTokenResolution> {
		if let Some(command) = BuiltinCommand::from_id(token) {
			let spec = self.commands.get(&CommandId::Builtin(command))?;
			return Some(CommandTokenResolution {
				command_id:  spec.id.clone(),
				target:      spec.target.clone(),
				argv_prefix: Vec::new(),
			});
		}
		if let Some(spec) = self.commands.get(&CommandId::Plugin(token.to_string())) {
			return Some(CommandTokenResolution {
				command_id:  spec.id.clone(),
				target:      spec.target.clone(),
				argv_prefix: Vec::new(),
			});
		}
		if let Some(alias) = self.command_aliases.iter().find(|alias| alias.name == token) {
			return Some(CommandTokenResolution {
				command_id:  alias.resolved_command_id.clone()?,
				target:      alias.target.clone()?,
				argv_prefix: alias.args.clone(),
			});
		}
		self.commands.values().find(|spec| spec.display_name.as_deref() == Some(token)).map(|spec| {
			CommandTokenResolution {
				command_id:  spec.id.clone(),
				target:      spec.target.clone(),
				argv_prefix: Vec::new(),
			}
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
	pub value:       String,
	pub label:       String,
	pub description: Option<String>,
}

pub trait Picker {
	fn suggestions(&self, input: &str) -> Vec<Suggestion>;
}

pub struct PickerRegistry<'a> {
	map: HashMap<CommandArgKind, Box<dyn Picker + 'a>>,
}

impl<'a> Default for PickerRegistry<'a> {
	fn default() -> Self { Self { map: HashMap::new() } }
}

impl<'a> PickerRegistry<'a> {
	pub fn register(&mut self, kind: CommandArgKind, picker: impl Picker + 'a) {
		self.map.insert(kind, Box::new(picker));
	}

	pub fn suggestions(&self, kind: CommandArgKind, input: &str) -> Vec<Suggestion> {
		self.map.get(&kind).map(|picker| picker.suggestions(input)).unwrap_or_default()
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveParameterContext {
	pub command_id: CommandId,
	pub target:     CommandTarget,
	pub index:      usize,
	pub param:      CommandParamSpec,
	pub input:      String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandTokenResolution {
	command_id:  CommandId,
	target:      CommandTarget,
	argv_prefix: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenizedCommandInput {
	tokens:              Vec<String>,
	ends_with_separator: bool,
}

fn validate_command_argv(spec: &CommandSpec, argv: &[String]) -> Result<(), CommandResolveError> {
	let required_count = spec.params.iter().filter(|param| !param.optional).count();
	if argv.len() < required_count {
		let missing = spec
			.params
			.iter()
			.skip(argv.len())
			.filter(|param| !param.optional)
			.map(|param| param.name.clone())
			.collect::<Vec<_>>();
		return Err(CommandResolveError::MissingRequiredParams { command_id: spec.id.display_text(), missing });
	}
	if argv.len() > spec.params.len() {
		return Err(CommandResolveError::TooManyArguments {
			command_id: spec.id.display_text(),
			expected:   spec.params.len(),
			actual:     argv.len(),
		});
	}
	Ok(())
}

fn resolved_params_from_argv(spec: &CommandSpec, argv: &[String]) -> ResolvedParams {
	ResolvedParams::new(
		spec
			.params
			.iter()
			.zip(argv.iter())
			.map(|(param, value)| ResolvedParam {
				name:  param.name.clone(),
				kind:  param.kind,
				value: match param.kind {
					CommandArgKind::Text => CommandValue::Text(value.clone()),
					CommandArgKind::File => CommandValue::File(value.clone()),
				},
			})
			.collect(),
	)
}

fn tokenize_command_input(input: &str) -> Result<TokenizedCommandInput, String> {
	let mut tokens = Vec::new();
	let mut current = String::new();
	let mut in_single = false;
	let mut in_double = false;
	let mut escaped = false;
	let mut saw_token_boundary = false;

	for ch in input.chars() {
		if escaped {
			current.push(ch);
			escaped = false;
			saw_token_boundary = false;
			continue;
		}
		match ch {
			'\\' => escaped = true,
			'\'' if !in_double => in_single = !in_single,
			'"' if !in_single => in_double = !in_double,
			ch if ch.is_whitespace() && !in_single && !in_double => {
				if !current.is_empty() {
					tokens.push(std::mem::take(&mut current));
				}
				saw_token_boundary = true;
			}
			_ => {
				current.push(ch);
				saw_token_boundary = false;
			}
		}
	}

	if escaped {
		return Err("unterminated escape sequence".to_string());
	}
	if in_single || in_double {
		return Err("unterminated quoted string".to_string());
	}
	if !current.is_empty() {
		tokens.push(current);
	}

	Ok(TokenizedCommandInput { tokens, ends_with_separator: saw_token_boundary })
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

fn is_deferred_plugin_alias_error(error: &CommandConfigError) -> bool {
	matches!(
		error,
		CommandConfigError::CommandAlias { reason, .. } if reason.starts_with("unknown run directive: plugin.")
	)
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
	#[serde(default)]
	pub args: Vec<String>,
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
	#[serde(default)]
	pub args: Vec<String>,
	pub desc: Option<String>,
}

trait KeyBindingView {
	fn keys(&self) -> &[NormalSequenceKey];
	fn command_id(&self) -> &CommandId;
	fn target(&self) -> &CommandTarget;
	fn args(&self) -> &[String];
	fn desc(&self) -> Option<&str>;
}

impl KeyBindingView for ScopedKeyBinding {
	fn keys(&self) -> &[NormalSequenceKey] { self.keys.as_slice() }

	fn command_id(&self) -> &CommandId { &self.command_id }

	fn target(&self) -> &CommandTarget { &self.target }

	fn args(&self) -> &[String] { self.args.as_slice() }

	fn desc(&self) -> Option<&str> { self.desc.as_deref() }
}

#[derive(Debug, Clone)]
pub struct PluginCommandRegistration {
	pub id:           String,
	pub default_name: String,
	pub plugin_id:    String,
	pub command_id:   String,
	pub category:     String,
	pub description:  String,
	pub params:       Vec<CommandParamSpec>,
}

fn resolve_key_binding_set<T>(
	commands: &HashMap<CommandId, CommandSpec>,
	bindings: &[T],
	keys: &[NormalSequenceKey],
) -> BindingMatch<ResolvedCommand>
where
	T: KeyBindingView,
{
	let mut has_prefix = false;
	for binding in bindings {
		if binding.keys() == keys
			&& let Some(spec) = commands.get(binding.command_id())
		{
			return BindingMatch::Exact(ResolvedCommand {
				command_id: spec.id.clone(),
				target:     binding.target().clone(),
				argv:       binding.args().to_vec(),
				params:     resolved_params_from_argv(spec, binding.args()),
			});
		}
		if binding.keys().starts_with(keys) {
			has_prefix = true;
		}
	}
	if has_prefix { BindingMatch::Pending } else { BindingMatch::NoMatch }
}

fn parse_plugin_command_name(name: &str) -> Option<(String, String)> {
	let (plugin_id, command_id) = name.split_once('.')?;
	let plugin_id = plugin_id.trim();
	let command_id = command_id.trim();
	if plugin_id.is_empty() || command_id.is_empty() {
		return None;
	}
	Some((plugin_id.to_string(), command_id.to_string()))
}

fn normalize_pascal_case_name(name: &str) -> String {
	name
		.split(|ch: char| !ch.is_alphanumeric())
		.filter(|segment| !segment.is_empty())
		.map(|segment| {
			let mut chars = segment.chars();
			let Some(first) = chars.next() else {
				return String::new();
			};
			let mut rendered = first.to_uppercase().collect::<String>();
			rendered.push_str(chars.as_str());
			rendered
		})
		.collect::<String>()
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
				CommandId::Plugin(command_id) => {
					RunDirective::PluginInvocation { plugin_name: command_id.trim_start_matches("plugin.").to_string() }
				}
			},
			args: binding.args().to_vec(),
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

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BuiltinCommandGroup)]
	enum MacroDslTestCommand {
		/// Rename file
		Rename { from: File, to: File },
		/// Search project
		Search { query: Text, path: Option<File> },
	}

	fn register_demo_pick_plugin_command(registry: &mut CommandRegistry) {
		registry
			.register_plugin_command(PluginCommandRegistration {
				id:           "plugin.demo.pick".to_string(),
				default_name: "Pick".to_string(),
				plugin_id:    "demo".to_string(),
				command_id:   "pick".to_string(),
				category:     "Demo Plugin".to_string(),
				description:  "Open the host file picker".to_string(),
				params:       Vec::new(),
			})
			.expect("demo pick plugin command should register");
	}

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
						args: Vec::new(),
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
			BindingMatch::Exact(ResolvedCommand {
				command_id: CommandId::Builtin(BuiltinCommand::Buffer(BufferCommand::Prev)),
				target:     CommandTarget::Builtin(BuiltinCommand::Buffer(BufferCommand::Prev)),
				argv:       Vec::new(),
				params:     ResolvedParams::default(),
			})
		);
		assert_eq!(
			registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('L')]),
			BindingMatch::Exact(ResolvedCommand {
				command_id: CommandId::Builtin(BuiltinCommand::Buffer(BufferCommand::Next)),
				target:     CommandTarget::Builtin(BuiltinCommand::Buffer(BufferCommand::Next)),
				argv:       Vec::new(),
				params:     ResolvedParams::default(),
			})
		);
		assert_eq!(
			registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('j')]),
			BindingMatch::Exact(ResolvedCommand {
				command_id: CommandId::Builtin(BuiltinCommand::Cursor(CursorCommand::Down)),
				target:     CommandTarget::Builtin(BuiltinCommand::Cursor(CursorCommand::Down)),
				argv:       Vec::new(),
				params:     ResolvedParams::default(),
			})
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
					args: Vec::new(),
					desc: Some("custom".to_string()),
				}],
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);
		let resolved = registry.resolve_command_input("qq").expect("command alias should resolve");

		assert!(errors.is_empty());
		assert_eq!(resolved.target, CommandTarget::Builtin(BuiltinCommand::Command(CommandCommand::QuitAll)));
		assert!(resolved.argv.is_empty());
	}

	#[test]
	fn configured_command_aliases_should_replace_default_alias_table() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "haha".to_string(),
					run:  "core.quit_all".into(),
					args: Vec::new(),
					desc: Some("custom".to_string()),
				}],
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);

		assert!(errors.is_empty());
		assert!(matches!(registry.resolve_command_input("qa"), Err(CommandResolveError::UnknownCommand { .. })));
		assert!(registry.resolve_command_input("haha").is_ok());
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
						args: Vec::new(),
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
			BindingMatch::Exact(ResolvedCommand {
				command_id: CommandId::Builtin(BuiltinCommand::Cursor(CursorCommand::FileStart)),
				target:     CommandTarget::Builtin(BuiltinCommand::Cursor(CursorCommand::FileStart)),
				argv:       Vec::new(),
				params:     ResolvedParams::default(),
			})
		);
		assert_eq!(
			registry.resolve_scope_sequence(KeymapScope::ModeNormal, &[NormalSequenceKey::Char('G')]),
			BindingMatch::Exact(ResolvedCommand {
				command_id: CommandId::Builtin(BuiltinCommand::Cursor(CursorCommand::FileEnd)),
				target:     CommandTarget::Builtin(BuiltinCommand::Cursor(CursorCommand::FileEnd)),
				argv:       Vec::new(),
				params:     ResolvedParams::default(),
			})
		);
	}

	#[test]
	fn struct_like_command_variants_should_map_fields_to_ordered_param_specs() {
		let rename = MacroDslTestCommand::Rename { from: File, to: File }.params();
		let search = MacroDslTestCommand::Search { query: Text, path: None }.params();

		assert_eq!(rename, &[
			BuiltinCommandParamSpec { name: "from", kind: CommandArgKind::File, optional: false },
			BuiltinCommandParamSpec { name: "to", kind: CommandArgKind::File, optional: false },
		]);
		assert_eq!(search, &[
			BuiltinCommandParamSpec { name: "query", kind: CommandArgKind::Text, optional: false },
			BuiltinCommandParamSpec { name: "path", kind: CommandArgKind::File, optional: true },
		]);
	}

	#[test]
	fn normal_keymap_should_accept_plugin_run_directive() {
		let mut registry = CommandRegistry::with_defaults();
		registry
			.register_plugin_command(PluginCommandRegistration {
				id:           "plugin.demo.chmod".to_string(),
				default_name: "Chmod".to_string(),
				plugin_id:    "demo".to_string(),
				command_id:   "chmod".to_string(),
				category:     "Demo Plugin".to_string(),
				description:  "Plugin chmod".to_string(),
				params:       Vec::new(),
			})
			.expect("plugin command should register");
		let config = CommandConfigFile {
			mode: ModeKeymapSections {
				normal: CommandKeymapSection {
					keymap: vec![KeymapBindingConfig {
						on:   KeyBindingOn::many(vec!["cm".to_string(), "mc".to_string()]),
						run:  "plugin.demo.chmod".into(),
						args: Vec::new(),
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
		assert!(matches!(
			resolved,
			BindingMatch::Exact(ResolvedCommand { target: CommandTarget::Plugin { .. }, .. })
		));
		assert!(matches!(
			alternate,
			BindingMatch::Exact(ResolvedCommand { target: CommandTarget::Plugin { .. }, .. })
		));
	}

	#[test]
	fn run_directive_should_accept_canonical_plugin_command_id() {
		let parsed = RunDirective::parse("plugin.example.inspect");

		assert_eq!(parsed, RunDirective::PluginInvocation { plugin_name: "example.inspect".to_string() });
		assert_eq!(parsed.render(), "plugin.example.inspect");
	}

	#[test]
	fn config_should_reject_unknown_plugin_run_directive() {
		let mut registry = CommandRegistry::with_defaults();
		let errors = registry.apply_config(&CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "bad-plugin".to_string(),
					run:  "plugin.example2.inspect".into(),
					args: Vec::new(),
					desc: Some("Broken plugin alias".to_string()),
				}],
			},
			..CommandConfigFile::default()
		});

		assert_eq!(errors.len(), 1);
		assert!(matches!(
			&errors[0],
			CommandConfigError::CommandAlias { reason, .. } if reason.contains("unknown run directive")
		));

		let matches = registry.command_palette_matches("bad-plugin", 16);
		let item = matches
			.iter()
			.find(|candidate| candidate.name == "bad-plugin")
			.expect("invalid plugin alias should still be visible");
		assert!(item.is_error);
		assert_eq!(item.description, "invalid command");
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
						args: Vec::new(),
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
		let mut registry = CommandRegistry::with_defaults();
		register_demo_pick_plugin_command(&mut registry);

		let id_matches = registry.command_palette_matches("pick", 12);
		let desc_matches = registry.command_palette_matches("host file picker", 12);

		assert!(
			id_matches.iter().any(|item| item.command_id == CommandId::Plugin("plugin.demo.pick".to_string()))
		);
		assert!(
			desc_matches.iter().any(|item| item.command_id == CommandId::Plugin("plugin.demo.pick".to_string()))
		);
		assert!(id_matches.iter().any(|item| {
			item.name == "Pick" && item.command_id == CommandId::Plugin("plugin.demo.pick".to_string())
		}));
		assert!(
			id_matches
				.iter()
				.find(|item| item.command_id == CommandId::Plugin("plugin.demo.pick".to_string()))
				.is_some_and(|item| !item.name_match_indices.is_empty())
		);
	}

	#[test]
	fn empty_command_palette_should_include_derived_display_names_for_unaliased_commands() {
		let mut registry = CommandRegistry::with_defaults();
		register_demo_pick_plugin_command(&mut registry);

		let matches = registry.command_palette_matches("", 128);

		assert!(matches.iter().any(|item| {
			item.name == "Pick" && item.command_id == CommandId::Plugin("plugin.demo.pick".to_string())
		}));
		assert!(matches.iter().any(|item| {
			item.name == "SplitVertical"
				&& item.command_id == CommandId::Builtin(BuiltinCommand::Window(WindowCommand::SplitVertical))
		}));
	}

	#[test]
	fn command_palette_should_show_param_summary_in_command_column() {
		let registry = CommandRegistry::with_defaults();

		let item = registry
			.command_palette_matches("e", 16)
			.into_iter()
			.find(|candidate| {
				candidate.command_id
					== CommandId::Builtin(BuiltinCommand::Command(CommandCommand::Reload { path: None }))
			})
			.expect("reload command should be visible");

		assert_eq!(item.command_id_label, "core.reload [path]");
		assert_eq!(item.description, "Reload current buffer");
	}

	#[test]
	fn builtin_path_params_should_register_file_picker_metadata() {
		let registry = CommandRegistry::with_defaults();
		let spec = registry
			.command_spec(&CommandId::Builtin(BuiltinCommand::Command(CommandCommand::Reload { path: None })))
			.expect("reload command should be registered");

		assert_eq!(spec.params.len(), 1);
		assert_eq!(spec.params[0].name, "path");
		assert_eq!(spec.params[0].kind, CommandArgKind::File);
	}

	#[test]
	fn resolve_command_input_should_accept_direct_command_id() {
		let registry = CommandRegistry::with_defaults();

		let resolved = registry.resolve_command_input("core.tab.new").expect("direct command id should resolve");

		assert_eq!(resolved.command_id, CommandId::Builtin(BuiltinCommand::Tab(TabCommand::New)));
	}

	#[test]
	fn resolve_command_input_should_accept_pascal_case_display_name() {
		let registry = CommandRegistry::with_defaults();

		let resolved = registry.resolve_command_input("FileStart").expect("display name should resolve");

		assert_eq!(resolved.command_id, CommandId::Builtin(BuiltinCommand::Cursor(CursorCommand::FileStart)));
		assert!(resolved.argv.is_empty());
	}

	#[test]
	fn resolve_command_input_should_parse_quoted_arguments_and_validate_params() {
		let mut registry = CommandRegistry::with_defaults();
		registry
			.register_plugin_command(PluginCommandRegistration {
				id:           "plugin.demo.echo".to_string(),
				default_name: "Echo".to_string(),
				plugin_id:    "demo".to_string(),
				command_id:   "echo".to_string(),
				category:     "Demo Plugin".to_string(),
				description:  "Echo arguments".to_string(),
				params:       vec![
					CommandParamSpec {
						name:     "message".to_string(),
						kind:     CommandArgKind::Text,
						optional: false,
					},
					CommandParamSpec { name: "path".to_string(), kind: CommandArgKind::File, optional: true },
				],
			})
			.expect("plugin command should register");

		let resolved =
			registry.resolve_command_input("Echo \"hello world\" src/main.rs").expect("quoted args should resolve");

		assert_eq!(resolved.argv, vec!["hello world".to_string(), "src/main.rs".to_string()]);
		assert_eq!(resolved.get_text("message"), Some("hello world"));
		assert_eq!(resolved.get_file("path"), Some("src/main.rs"));
		assert!(matches!(
			registry.resolve_command_input("Echo"),
			Err(CommandResolveError::MissingRequiredParams { .. })
		));
		assert!(matches!(
			registry.resolve_command_input("Echo one two three"),
			Err(CommandResolveError::TooManyArguments { .. })
		));
	}

	#[test]
	fn command_alias_args_should_prefix_user_arguments() {
		let mut registry = CommandRegistry::with_defaults();
		registry
			.register_plugin_command(PluginCommandRegistration {
				id:           "plugin.demo.echo".to_string(),
				default_name: "Echo".to_string(),
				plugin_id:    "demo".to_string(),
				command_id:   "echo".to_string(),
				category:     "Demo Plugin".to_string(),
				description:  "Echo arguments".to_string(),
				params:       vec![
					CommandParamSpec { name: "first".to_string(), kind: CommandArgKind::Text, optional: false },
					CommandParamSpec {
						name:     "second".to_string(),
						kind:     CommandArgKind::Text,
						optional: false,
					},
				],
			})
			.expect("plugin command should register");
		let errors = registry.apply_config(&CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "echop".to_string(),
					run:  "plugin demo.echo".into(),
					args: vec!["configured".to_string()],
					desc: Some("Echo with prefix".to_string()),
				}],
			},
			..CommandConfigFile::default()
		});

		assert!(errors.is_empty());
		let resolved = registry.resolve_command_input("echop typed").expect("alias args should prefix user args");
		assert_eq!(resolved.argv, vec!["configured".to_string(), "typed".to_string()]);
	}

	#[test]
	fn active_parameter_context_should_require_resolved_command_and_separator() {
		let registry = CommandRegistry::with_defaults();

		assert!(registry.active_parameter_context("save").is_none());
		assert!(registry.active_parameter_context("unknown ").is_none());

		let context = registry
			.active_parameter_context("core.save ")
			.expect("file parameter should become active after separator");
		assert_eq!(context.index, 0);
		assert_eq!(context.param.name, "path");
		assert_eq!(context.param.kind, CommandArgKind::File);
		assert_eq!(context.input, "");

		let editing_context = registry
			.active_parameter_context("core.save src/main.rs")
			.expect("editing first file parameter should stay active");
		assert_eq!(editing_context.index, 0);
		assert_eq!(editing_context.input, "src/main.rs");
	}

	#[test]
	fn invalid_command_alias_should_still_appear_in_command_palette_as_error() {
		let mut registry = CommandRegistry::with_defaults();
		let errors = registry.apply_config(&CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "bad".to_string(),
					run:  "core.not.exists".into(),
					args: Vec::new(),
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
						args: Vec::new(),
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
						args: Vec::new(),
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
