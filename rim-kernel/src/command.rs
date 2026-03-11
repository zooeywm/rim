use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{action::{AppAction, BufferAction, EditorAction, LayoutAction, TabAction, WindowAction}, state::NormalSequenceKey};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuiltinCommand {
	SplitVertical,
	SplitHorizontal,
	TabNew,
	TabCloseCurrent,
	TabSwitchPrev,
	TabSwitchNext,
	BufferCloseActive,
	BufferNewEmpty,
	DeleteCurrentLineToSlot,
	EnterInsert,
	AppendInsert,
	OpenLineBelowInsert,
	OpenLineAboveInsert,
	EnterCommandMode,
	EnterVisualMode,
	EnterVisualLineMode,
	EnterVisualBlockMode,
	Undo,
	Redo,
	MoveLeft,
	MoveLineStart,
	MoveLineEnd,
	MoveDown,
	MoveUp,
	MoveRight,
	MoveFileStart,
	MoveFileEnd,
	JoinLineBelow,
	CutCharToSlot,
	PasteSlotAfterCursor,
	BufferSwitchPrev,
	BufferSwitchNext,
	WindowFocusLeft,
	WindowFocusDown,
	WindowFocusUp,
	WindowFocusRight,
	ScrollViewDown,
	ScrollViewUp,
	ScrollViewHalfPageDown,
	ScrollViewHalfPageUp,
	Quit,
	QuitForce,
	QuitAll,
	QuitAllForce,
	Save,
	SaveForce,
	SaveAll,
	SaveAndQuit,
	SaveAndQuitForce,
	SaveAllAndQuit,
	SaveAllAndQuitForce,
	Reload,
	ReloadForce,
	OpenPickerYazi,
	VisualExit,
	VisualDelete,
	VisualYank,
	VisualPaste,
	VisualChange,
	VisualBlockInsertBefore,
	VisualBlockInsertAfter,
	VisualMoveLeft,
	VisualMoveRight,
}

impl BuiltinCommand {
	pub fn normal_mode_action(&self) -> Option<AppAction> {
		match self {
			Self::SplitVertical => Some(AppAction::Layout(LayoutAction::SplitVertical)),
			Self::SplitHorizontal => Some(AppAction::Layout(LayoutAction::SplitHorizontal)),
			Self::TabNew => Some(AppAction::Tab(TabAction::New)),
			Self::TabCloseCurrent => Some(AppAction::Tab(TabAction::CloseCurrent)),
			Self::TabSwitchPrev => Some(AppAction::Tab(TabAction::SwitchPrev)),
			Self::TabSwitchNext => Some(AppAction::Tab(TabAction::SwitchNext)),
			Self::BufferCloseActive => Some(AppAction::Editor(EditorAction::CloseActiveBuffer)),
			Self::BufferNewEmpty => Some(AppAction::Editor(EditorAction::NewEmptyBuffer)),
			Self::DeleteCurrentLineToSlot => Some(AppAction::Editor(EditorAction::DeleteCurrentLineToSlot)),
			Self::EnterInsert => Some(AppAction::Editor(EditorAction::EnterInsert)),
			Self::AppendInsert => Some(AppAction::Editor(EditorAction::AppendInsert)),
			Self::OpenLineBelowInsert => Some(AppAction::Editor(EditorAction::OpenLineBelowInsert)),
			Self::OpenLineAboveInsert => Some(AppAction::Editor(EditorAction::OpenLineAboveInsert)),
			Self::EnterCommandMode => Some(AppAction::Editor(EditorAction::EnterCommandMode)),
			Self::EnterVisualMode => Some(AppAction::Editor(EditorAction::EnterVisualMode)),
			Self::EnterVisualLineMode => Some(AppAction::Editor(EditorAction::EnterVisualLineMode)),
			Self::EnterVisualBlockMode => Some(AppAction::Editor(EditorAction::EnterVisualBlockMode)),
			Self::Undo => Some(AppAction::Editor(EditorAction::Undo)),
			Self::Redo => Some(AppAction::Editor(EditorAction::Redo)),
			Self::MoveLeft => Some(AppAction::Editor(EditorAction::MoveLeft)),
			Self::MoveLineStart => Some(AppAction::Editor(EditorAction::MoveLineStart)),
			Self::MoveLineEnd => Some(AppAction::Editor(EditorAction::MoveLineEnd)),
			Self::MoveDown => Some(AppAction::Editor(EditorAction::MoveDown)),
			Self::MoveUp => Some(AppAction::Editor(EditorAction::MoveUp)),
			Self::MoveRight => Some(AppAction::Editor(EditorAction::MoveRight)),
			Self::MoveFileStart => Some(AppAction::Editor(EditorAction::MoveFileStart)),
			Self::MoveFileEnd => Some(AppAction::Editor(EditorAction::MoveFileEnd)),
			Self::JoinLineBelow => Some(AppAction::Editor(EditorAction::JoinLineBelow)),
			Self::CutCharToSlot => Some(AppAction::Editor(EditorAction::CutCharToSlot)),
			Self::PasteSlotAfterCursor => Some(AppAction::Editor(EditorAction::PasteSlotAfterCursor)),
			Self::BufferSwitchPrev => Some(AppAction::Buffer(BufferAction::SwitchPrev)),
			Self::BufferSwitchNext => Some(AppAction::Buffer(BufferAction::SwitchNext)),
			Self::WindowFocusLeft => Some(AppAction::Window(WindowAction::FocusLeft)),
			Self::WindowFocusDown => Some(AppAction::Window(WindowAction::FocusDown)),
			Self::WindowFocusUp => Some(AppAction::Window(WindowAction::FocusUp)),
			Self::WindowFocusRight => Some(AppAction::Window(WindowAction::FocusRight)),
			Self::ScrollViewDown => Some(AppAction::Editor(EditorAction::ScrollViewDown)),
			Self::ScrollViewUp => Some(AppAction::Editor(EditorAction::ScrollViewUp)),
			Self::ScrollViewHalfPageDown => Some(AppAction::Editor(EditorAction::ScrollViewHalfPageDown)),
			Self::ScrollViewHalfPageUp => Some(AppAction::Editor(EditorAction::ScrollViewHalfPageUp)),
			_ => None,
		}
	}

	pub fn visual_mode_action(&self) -> Option<AppAction> {
		match self {
			Self::EnterVisualMode => Some(AppAction::Editor(EditorAction::EnterVisualMode)),
			Self::EnterVisualLineMode => Some(AppAction::Editor(EditorAction::EnterVisualLineMode)),
			Self::EnterVisualBlockMode => Some(AppAction::Editor(EditorAction::EnterVisualBlockMode)),
			Self::MoveDown => Some(AppAction::Editor(EditorAction::MoveDown)),
			Self::MoveUp => Some(AppAction::Editor(EditorAction::MoveUp)),
			Self::MoveLineStart => Some(AppAction::Editor(EditorAction::MoveLineStart)),
			Self::MoveLineEnd => Some(AppAction::Editor(EditorAction::MoveLineEnd)),
			Self::MoveFileStart => Some(AppAction::Editor(EditorAction::MoveFileStart)),
			Self::MoveFileEnd => Some(AppAction::Editor(EditorAction::MoveFileEnd)),
			Self::ScrollViewDown => Some(AppAction::Editor(EditorAction::ScrollViewDown)),
			Self::ScrollViewUp => Some(AppAction::Editor(EditorAction::ScrollViewUp)),
			Self::ScrollViewHalfPageDown => Some(AppAction::Editor(EditorAction::ScrollViewHalfPageDown)),
			Self::ScrollViewHalfPageUp => Some(AppAction::Editor(EditorAction::ScrollViewHalfPageUp)),
			Self::VisualExit => Some(AppAction::Editor(EditorAction::ExitVisualMode)),
			Self::VisualDelete => Some(AppAction::Editor(EditorAction::DeleteVisualSelectionToSlot)),
			Self::VisualYank => Some(AppAction::Editor(EditorAction::YankVisualSelectionToSlot)),
			Self::VisualPaste => Some(AppAction::Editor(EditorAction::ReplaceVisualSelectionWithSlot)),
			Self::VisualChange => Some(AppAction::Editor(EditorAction::ChangeVisualSelectionToInsertMode)),
			Self::VisualBlockInsertBefore => Some(AppAction::Editor(EditorAction::BeginVisualBlockInsertBefore)),
			Self::VisualBlockInsertAfter => Some(AppAction::Editor(EditorAction::BeginVisualBlockInsertAfter)),
			Self::VisualMoveLeft => Some(AppAction::Editor(EditorAction::MoveLeftInVisual)),
			Self::VisualMoveRight => Some(AppAction::Editor(EditorAction::MoveRightInVisual)),
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
	pub id:          String,
	pub category:    String,
	pub description: String,
	pub arg_kind:    CommandArgKind,
	pub target:      CommandTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCommand {
	pub spec:     CommandSpec,
	pub argument: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingMatch<T> {
	Exact(T),
	Pending,
	NoMatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalKeyBinding {
	keys:       Vec<NormalSequenceKey>,
	command_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisualKeyBinding {
	keys:       Vec<NormalSequenceKey>,
	command_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandAlias {
	name:       String,
	command_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct CommandRegistry {
	commands:        HashMap<String, CommandSpec>,
	normal_bindings: Vec<NormalKeyBinding>,
	visual_bindings: Vec<VisualKeyBinding>,
	command_aliases: Vec<CommandAlias>,
}

impl CommandRegistry {
	pub fn with_defaults() -> Self {
		let mut registry = Self::default();
		registry.register_default_builtins();
		registry
	}

	pub fn apply_config(&mut self, config: &CommandConfigFile) -> Vec<String> {
		let mut errors = Vec::new();

		if !config.normal.keymap.is_empty() {
			// Treat configured key bindings as an authoritative replacement set.
			self.normal_bindings.clear();
		}
		for binding in &config.normal.keymap {
			let keys = match binding.on.parse_keys() {
				Ok(keys) => keys,
				Err(err) => {
					errors.push(format!("invalid normal key binding '{}': {}", binding.on.display_for_error(), err));
					continue;
				}
			};
			let Some(resolved) = self.resolve_or_register_run_directive(binding.run.as_str()) else {
				errors.push(format!("unknown run directive in normal keymap: {}", binding.run));
				continue;
			};
			self.normal_bindings.retain(|candidate| candidate.keys != keys);
			self.normal_bindings.push(NormalKeyBinding { keys, command_id: resolved.spec.id.clone() });
		}

		if !config.visual.keymap.is_empty() {
			self.visual_bindings.clear();
		}
		for binding in &config.visual.keymap {
			let keys = match binding.on.parse_keys() {
				Ok(keys) => keys,
				Err(err) => {
					errors.push(format!("invalid visual key binding '{}': {}", binding.on.display_for_error(), err));
					continue;
				}
			};
			let Some(resolved) = self.resolve_or_register_run_directive(binding.run.as_str()) else {
				errors.push(format!("unknown run directive in visual keymap: {}", binding.run));
				continue;
			};
			self.visual_bindings.retain(|candidate| candidate.keys != keys);
			self.visual_bindings.push(VisualKeyBinding { keys, command_id: resolved.spec.id.clone() });
		}

		if !config.command.commands.is_empty() {
			// Treat configured command aliases as an authoritative replacement set.
			self.command_aliases.clear();
		}
		for alias in &config.command.commands {
			let Some(resolved) = self.resolve_or_register_run_directive(alias.run.as_str()) else {
				errors.push(format!("unknown run directive in command alias: {}", alias.run));
				continue;
			};
			self
				.command_aliases
				.push(CommandAlias { name: alias.name.clone(), command_id: resolved.spec.id });
		}

		errors
	}

	pub fn register_plugin_command(&mut self, registration: PluginCommandRegistration) -> Result<(), String> {
		if self.commands.contains_key(registration.id.as_str()) {
			return Err(format!("duplicate command id: {}", registration.id));
		}
		self.commands.insert(registration.id.clone(), CommandSpec {
			id:          registration.id.clone(),
			category:    registration.category,
			description: registration.description,
			arg_kind:    registration.arg_kind,
			target:      CommandTarget::Plugin { plugin_id: registration.plugin_id, command_id: registration.id },
		});
		Ok(())
	}

	pub fn resolve_normal_sequence(&self, keys: &[NormalSequenceKey]) -> BindingMatch<CommandTarget> {
		resolve_key_binding_set(&self.commands, &self.normal_bindings, keys)
	}

	pub fn resolve_visual_sequence(&self, keys: &[NormalSequenceKey]) -> BindingMatch<CommandTarget> {
		resolve_key_binding_set(&self.commands, &self.visual_bindings, keys)
	}

	pub fn resolve_command_input(&self, input: &str) -> Option<ResolvedCommand> {
		let trimmed = input.trim();
		let (name, argument) = match trimmed.split_once(' ') {
			Some((name, argument)) => (name, Some(argument.trim().to_string())),
			None => (trimmed, None),
		};
		let alias = self.command_aliases.iter().find(|alias| alias.name == name)?;
		let spec = self.commands.get(alias.command_id.as_str())?.clone();

		match spec.arg_kind {
			CommandArgKind::None if argument.as_deref().is_some_and(|arg| !arg.is_empty()) => None,
			CommandArgKind::None => Some(ResolvedCommand { spec, argument: None }),
			CommandArgKind::OptionalPath | CommandArgKind::RawTail => {
				Some(ResolvedCommand { spec, argument: argument.filter(|arg| !arg.is_empty()) })
			}
		}
	}

	fn resolve_run_directive(&self, run: &str) -> Option<ResolvedCommand> {
		let trimmed = run.trim();
		if trimmed.is_empty() {
			return None;
		}
		if let Some(spec) = self.commands.get(trimmed).cloned() {
			return Some(ResolvedCommand { spec, argument: None });
		}
		self.resolve_command_input(trimmed)
	}

	fn resolve_or_register_run_directive(&mut self, run: &str) -> Option<ResolvedCommand> {
		if let Some(resolved) = self.resolve_run_directive(run) {
			return Some(resolved);
		}
		self.resolve_plugin_run_directive(run)
	}

	fn resolve_plugin_run_directive(&mut self, run: &str) -> Option<ResolvedCommand> {
		let trimmed = run.trim();
		let payload = trimmed.strip_prefix("plugin ")?;
		let payload = payload.trim();
		if payload.is_empty() {
			return None;
		}

		let mut segments = payload.split_whitespace();
		let command_name = segments.next()?;
		let argument = {
			let remaining = segments.collect::<Vec<_>>().join(" ");
			if remaining.is_empty() { None } else { Some(remaining) }
		};
		let spec_id = format!("plugin.{}", command_name);
		if !self.commands.contains_key(spec_id.as_str()) {
			self.commands.insert(spec_id.clone(), CommandSpec {
				id:          spec_id.clone(),
				category:    "plugin".to_string(),
				description: format!("Plugin command '{}'", command_name),
				arg_kind:    CommandArgKind::RawTail,
				target:      CommandTarget::Plugin {
					plugin_id:  command_name.to_string(),
					command_id: command_name.to_string(),
				},
			});
		}

		let spec = self.commands.get(spec_id.as_str())?.clone();
		Some(ResolvedCommand { spec, argument })
	}

	pub fn export_config(&self) -> CommandConfigFile {
		let normal = export_keymap_bindings(&self.commands, &self.normal_bindings);
		let visual = export_keymap_bindings(&self.commands, &self.visual_bindings);

		let mut command = Vec::with_capacity(self.command_aliases.len());
		for alias in &self.command_aliases {
			let Some(spec) = self.commands.get(alias.command_id.as_str()) else {
				continue;
			};
			command.push(CommandAliasConfig {
				name: alias.name.clone(),
				run:  alias.command_id.clone(),
				desc: Some(spec.description.clone()),
			});
		}

		CommandConfigFile {
			normal:  CommandKeymapSection { keymap: normal },
			visual:  CommandKeymapSection { keymap: visual },
			command: CommandAliasSection { commands: command },
		}
	}

	fn register_default_builtins(&mut self) {
		self.register_builtin(
			"core.window.split_vertical",
			"window",
			"Split vertically",
			CommandArgKind::None,
			BuiltinCommand::SplitVertical,
		);
		self.register_builtin(
			"core.window.split_horizontal",
			"window",
			"Split horizontally",
			CommandArgKind::None,
			BuiltinCommand::SplitHorizontal,
		);
		self.register_builtin(
			"core.tab.new",
			"tab",
			"Open a new tab",
			CommandArgKind::None,
			BuiltinCommand::TabNew,
		);
		self.register_builtin(
			"core.tab.close_current",
			"tab",
			"Close current tab",
			CommandArgKind::None,
			BuiltinCommand::TabCloseCurrent,
		);
		self.register_builtin(
			"core.tab.prev",
			"tab",
			"Previous tab",
			CommandArgKind::None,
			BuiltinCommand::TabSwitchPrev,
		);
		self.register_builtin(
			"core.tab.next",
			"tab",
			"Next tab",
			CommandArgKind::None,
			BuiltinCommand::TabSwitchNext,
		);
		self.register_builtin(
			"core.buffer.close",
			"buffer",
			"Close active buffer",
			CommandArgKind::None,
			BuiltinCommand::BufferCloseActive,
		);
		self.register_builtin(
			"core.buffer.new_empty",
			"buffer",
			"Create an empty buffer",
			CommandArgKind::None,
			BuiltinCommand::BufferNewEmpty,
		);
		self.register_builtin(
			"core.buffer.delete_line",
			"buffer",
			"Delete current line",
			CommandArgKind::None,
			BuiltinCommand::DeleteCurrentLineToSlot,
		);
		self.register_builtin(
			"core.mode.insert",
			"mode",
			"Enter insert mode",
			CommandArgKind::None,
			BuiltinCommand::EnterInsert,
		);
		self.register_builtin(
			"core.mode.append",
			"mode",
			"Append and enter insert mode",
			CommandArgKind::None,
			BuiltinCommand::AppendInsert,
		);
		self.register_builtin(
			"core.mode.open_below",
			"mode",
			"Open line below and enter insert mode",
			CommandArgKind::None,
			BuiltinCommand::OpenLineBelowInsert,
		);
		self.register_builtin(
			"core.mode.open_above",
			"mode",
			"Open line above and enter insert mode",
			CommandArgKind::None,
			BuiltinCommand::OpenLineAboveInsert,
		);
		self.register_builtin(
			"core.mode.command",
			"mode",
			"Enter command mode",
			CommandArgKind::None,
			BuiltinCommand::EnterCommandMode,
		);
		self.register_builtin(
			"core.mode.visual",
			"mode",
			"Enter visual mode",
			CommandArgKind::None,
			BuiltinCommand::EnterVisualMode,
		);
		self.register_builtin(
			"core.mode.visual_line",
			"mode",
			"Enter visual line mode",
			CommandArgKind::None,
			BuiltinCommand::EnterVisualLineMode,
		);
		self.register_builtin(
			"core.mode.visual_block",
			"mode",
			"Enter visual block mode",
			CommandArgKind::None,
			BuiltinCommand::EnterVisualBlockMode,
		);
		self.register_builtin("core.edit.undo", "edit", "Undo", CommandArgKind::None, BuiltinCommand::Undo);
		self.register_builtin("core.edit.redo", "edit", "Redo", CommandArgKind::None, BuiltinCommand::Redo);
		self.register_builtin(
			"core.cursor.left",
			"cursor",
			"Move left",
			CommandArgKind::None,
			BuiltinCommand::MoveLeft,
		);
		self.register_builtin(
			"core.cursor.line_start",
			"cursor",
			"Move to line start",
			CommandArgKind::None,
			BuiltinCommand::MoveLineStart,
		);
		self.register_builtin(
			"core.cursor.line_end",
			"cursor",
			"Move to line end",
			CommandArgKind::None,
			BuiltinCommand::MoveLineEnd,
		);
		self.register_builtin(
			"core.cursor.down",
			"cursor",
			"Move down",
			CommandArgKind::None,
			BuiltinCommand::MoveDown,
		);
		self.register_builtin(
			"core.cursor.up",
			"cursor",
			"Move up",
			CommandArgKind::None,
			BuiltinCommand::MoveUp,
		);
		self.register_builtin(
			"core.cursor.right",
			"cursor",
			"Move right",
			CommandArgKind::None,
			BuiltinCommand::MoveRight,
		);
		self.register_builtin(
			"core.cursor.file_start",
			"cursor",
			"Move to file start",
			CommandArgKind::None,
			BuiltinCommand::MoveFileStart,
		);
		self.register_builtin(
			"core.cursor.file_end",
			"cursor",
			"Move to file end",
			CommandArgKind::None,
			BuiltinCommand::MoveFileEnd,
		);
		self.register_builtin(
			"core.edit.join_line_below",
			"edit",
			"Join line below",
			CommandArgKind::None,
			BuiltinCommand::JoinLineBelow,
		);
		self.register_builtin(
			"core.edit.cut_char",
			"edit",
			"Cut current char",
			CommandArgKind::None,
			BuiltinCommand::CutCharToSlot,
		);
		self.register_builtin(
			"core.edit.paste",
			"edit",
			"Paste slot after cursor",
			CommandArgKind::None,
			BuiltinCommand::PasteSlotAfterCursor,
		);
		self.register_builtin(
			"core.buffer.prev",
			"buffer",
			"Previous buffer",
			CommandArgKind::None,
			BuiltinCommand::BufferSwitchPrev,
		);
		self.register_builtin(
			"core.buffer.next",
			"buffer",
			"Next buffer",
			CommandArgKind::None,
			BuiltinCommand::BufferSwitchNext,
		);
		self.register_builtin(
			"core.window.focus_left",
			"window",
			"Focus left window",
			CommandArgKind::None,
			BuiltinCommand::WindowFocusLeft,
		);
		self.register_builtin(
			"core.window.focus_down",
			"window",
			"Focus down window",
			CommandArgKind::None,
			BuiltinCommand::WindowFocusDown,
		);
		self.register_builtin(
			"core.window.focus_up",
			"window",
			"Focus up window",
			CommandArgKind::None,
			BuiltinCommand::WindowFocusUp,
		);
		self.register_builtin(
			"core.window.focus_right",
			"window",
			"Focus right window",
			CommandArgKind::None,
			BuiltinCommand::WindowFocusRight,
		);
		self.register_builtin(
			"core.view.scroll_down",
			"view",
			"Scroll down",
			CommandArgKind::None,
			BuiltinCommand::ScrollViewDown,
		);
		self.register_builtin(
			"core.view.scroll_up",
			"view",
			"Scroll up",
			CommandArgKind::None,
			BuiltinCommand::ScrollViewUp,
		);
		self.register_builtin(
			"core.view.scroll_half_page_down",
			"view",
			"Scroll down half page",
			CommandArgKind::None,
			BuiltinCommand::ScrollViewHalfPageDown,
		);
		self.register_builtin(
			"core.view.scroll_half_page_up",
			"view",
			"Scroll up half page",
			CommandArgKind::None,
			BuiltinCommand::ScrollViewHalfPageUp,
		);
		self.register_builtin(
			"core.quit",
			"command",
			"Quit current scope",
			CommandArgKind::None,
			BuiltinCommand::Quit,
		);
		self.register_builtin(
			"core.quit_force",
			"command",
			"Force quit current scope",
			CommandArgKind::None,
			BuiltinCommand::QuitForce,
		);
		self.register_builtin(
			"core.quit_all",
			"command",
			"Quit application",
			CommandArgKind::None,
			BuiltinCommand::QuitAll,
		);
		self.register_builtin(
			"core.quit_all_force",
			"command",
			"Force quit application",
			CommandArgKind::None,
			BuiltinCommand::QuitAllForce,
		);
		self.register_builtin(
			"core.save",
			"command",
			"Save current buffer",
			CommandArgKind::OptionalPath,
			BuiltinCommand::Save,
		);
		self.register_builtin(
			"core.save_force",
			"command",
			"Force save current buffer",
			CommandArgKind::OptionalPath,
			BuiltinCommand::SaveForce,
		);
		self.register_builtin(
			"core.save_all",
			"command",
			"Save all file-backed buffers",
			CommandArgKind::None,
			BuiltinCommand::SaveAll,
		);
		self.register_builtin(
			"core.save_and_quit",
			"command",
			"Save current buffer and quit",
			CommandArgKind::OptionalPath,
			BuiltinCommand::SaveAndQuit,
		);
		self.register_builtin(
			"core.save_and_quit_force",
			"command",
			"Force save current buffer and quit",
			CommandArgKind::OptionalPath,
			BuiltinCommand::SaveAndQuitForce,
		);
		self.register_builtin(
			"core.save_all_and_quit",
			"command",
			"Save all file-backed buffers and quit",
			CommandArgKind::None,
			BuiltinCommand::SaveAllAndQuit,
		);
		self.register_builtin(
			"core.save_all_and_quit_force",
			"command",
			"Force save all file-backed buffers and quit",
			CommandArgKind::None,
			BuiltinCommand::SaveAllAndQuitForce,
		);
		self.register_builtin(
			"core.reload",
			"command",
			"Reload current buffer",
			CommandArgKind::OptionalPath,
			BuiltinCommand::Reload,
		);
		self.register_builtin(
			"core.reload_force",
			"command",
			"Force reload current buffer",
			CommandArgKind::OptionalPath,
			BuiltinCommand::ReloadForce,
		);
		self.register_builtin(
			"core.picker.yazi",
			"command",
			"Open the yazi picker",
			CommandArgKind::None,
			BuiltinCommand::OpenPickerYazi,
		);
		self.register_builtin(
			"core.visual.exit",
			"visual",
			"Exit visual mode",
			CommandArgKind::None,
			BuiltinCommand::VisualExit,
		);
		self.register_builtin(
			"core.visual.delete",
			"visual",
			"Delete visual selection",
			CommandArgKind::None,
			BuiltinCommand::VisualDelete,
		);
		self.register_builtin(
			"core.visual.yank",
			"visual",
			"Yank visual selection",
			CommandArgKind::None,
			BuiltinCommand::VisualYank,
		);
		self.register_builtin(
			"core.visual.paste",
			"visual",
			"Replace visual selection with slot",
			CommandArgKind::None,
			BuiltinCommand::VisualPaste,
		);
		self.register_builtin(
			"core.visual.change",
			"visual",
			"Change visual selection",
			CommandArgKind::None,
			BuiltinCommand::VisualChange,
		);
		self.register_builtin(
			"core.visual.block_insert_before",
			"visual",
			"Insert before visual block",
			CommandArgKind::None,
			BuiltinCommand::VisualBlockInsertBefore,
		);
		self.register_builtin(
			"core.visual.block_insert_after",
			"visual",
			"Append after visual block",
			CommandArgKind::None,
			BuiltinCommand::VisualBlockInsertAfter,
		);
		self.register_builtin(
			"core.visual.left",
			"visual",
			"Move left in visual mode",
			CommandArgKind::None,
			BuiltinCommand::VisualMoveLeft,
		);
		self.register_builtin(
			"core.visual.right",
			"visual",
			"Move right in visual mode",
			CommandArgKind::None,
			BuiltinCommand::VisualMoveRight,
		);

		self.bind_default_normal("h", "core.cursor.left");
		self.bind_default_normal("0", "core.cursor.line_start");
		self.bind_default_normal("$", "core.cursor.line_end");
		self.bind_default_normal("j", "core.cursor.down");
		self.bind_default_normal("k", "core.cursor.up");
		self.bind_default_normal("l", "core.cursor.right");
		self.bind_default_normal("gg", "core.cursor.file_start");
		self.bind_default_normal("G", "core.cursor.file_end");
		self.bind_default_normal("J", "core.edit.join_line_below");
		self.bind_default_normal("x", "core.edit.cut_char");
		self.bind_default_normal("p", "core.edit.paste");
		self.bind_default_normal("i", "core.mode.insert");
		self.bind_default_normal("a", "core.mode.append");
		self.bind_default_normal("o", "core.mode.open_below");
		self.bind_default_normal("O", "core.mode.open_above");
		self.bind_default_normal(":", "core.mode.command");
		self.bind_default_normal("v", "core.mode.visual");
		self.bind_default_normal("V", "core.mode.visual_line");
		self.bind_default_normal("<C-v>", "core.mode.visual_block");
		self.bind_default_normal("u", "core.edit.undo");
		self.bind_default_normal("dd", "core.buffer.delete_line");
		self.bind_default_normal("H", "core.buffer.prev");
		self.bind_default_normal("L", "core.buffer.next");
		self.bind_default_normal("{", "core.buffer.prev");
		self.bind_default_normal("}", "core.buffer.next");
		self.bind_default_normal("<C-h>", "core.window.focus_left");
		self.bind_default_normal("<C-j>", "core.window.focus_down");
		self.bind_default_normal("<C-k>", "core.window.focus_up");
		self.bind_default_normal("<C-l>", "core.window.focus_right");
		self.bind_default_normal("<C-e>", "core.view.scroll_down");
		self.bind_default_normal("<C-y>", "core.view.scroll_up");
		self.bind_default_normal("<C-d>", "core.view.scroll_half_page_down");
		self.bind_default_normal("<C-u>", "core.view.scroll_half_page_up");
		self.bind_default_normal("<C-r>", "core.edit.redo");
		self.bind_default_normal("<leader>wv", "core.window.split_vertical");
		self.bind_default_normal("<leader>wh", "core.window.split_horizontal");
		self.bind_default_normal("<leader><Tab>n", "core.tab.new");
		self.bind_default_normal("<leader><Tab>d", "core.tab.close_current");
		self.bind_default_normal("<leader><Tab>[", "core.tab.prev");
		self.bind_default_normal("<leader><Tab>]", "core.tab.next");
		self.bind_default_normal("<leader>bd", "core.buffer.close");
		self.bind_default_normal("<leader>bn", "core.buffer.new_empty");
		self.bind_default_visual("<Esc>", "core.visual.exit");
		self.bind_default_visual("v", "core.mode.visual");
		self.bind_default_visual("V", "core.mode.visual_line");
		self.bind_default_visual("<C-v>", "core.mode.visual_block");
		self.bind_default_visual("c", "core.visual.change");
		self.bind_default_visual("d", "core.visual.delete");
		self.bind_default_visual("x", "core.visual.delete");
		self.bind_default_visual("y", "core.visual.yank");
		self.bind_default_visual("p", "core.visual.paste");
		self.bind_default_visual("I", "core.visual.block_insert_before");
		self.bind_default_visual("A", "core.visual.block_insert_after");
		self.bind_default_visual("h", "core.visual.left");
		self.bind_default_visual("j", "core.cursor.down");
		self.bind_default_visual("k", "core.cursor.up");
		self.bind_default_visual("l", "core.visual.right");
		self.bind_default_visual("0", "core.cursor.line_start");
		self.bind_default_visual("$", "core.cursor.line_end");
		self.bind_default_visual("gg", "core.cursor.file_start");
		self.bind_default_visual("G", "core.cursor.file_end");
		self.bind_default_visual("<C-e>", "core.view.scroll_down");
		self.bind_default_visual("<C-y>", "core.view.scroll_up");
		self.bind_default_visual("<C-d>", "core.view.scroll_half_page_down");
		self.bind_default_visual("<C-u>", "core.view.scroll_half_page_up");

		self.bind_default_command("q", "core.quit");
		self.bind_default_command("quit", "core.quit");
		self.bind_default_command("q!", "core.quit_force");
		self.bind_default_command("quit!", "core.quit_force");
		self.bind_default_command("qa", "core.quit_all");
		self.bind_default_command("qa!", "core.quit_all_force");
		self.bind_default_command("w", "core.save");
		self.bind_default_command("w!", "core.save_force");
		self.bind_default_command("wa", "core.save_all");
		self.bind_default_command("wq", "core.save_and_quit");
		self.bind_default_command("wq!", "core.save_and_quit_force");
		self.bind_default_command("wqa", "core.save_all_and_quit");
		self.bind_default_command("wqa!", "core.save_all_and_quit_force");
		self.bind_default_command("e", "core.reload");
		self.bind_default_command("e!", "core.reload_force");
		self.bind_default_command("yazi", "core.picker.yazi");
		self.bind_default_command("files", "core.picker.yazi");
	}

	fn register_builtin(
		&mut self,
		id: &str,
		category: &str,
		description: &str,
		arg_kind: CommandArgKind,
		builtin: BuiltinCommand,
	) {
		self.commands.insert(id.to_string(), CommandSpec {
			id: id.to_string(),
			category: category.to_string(),
			description: description.to_string(),
			arg_kind,
			target: CommandTarget::Builtin(builtin),
		});
	}

	fn bind_default_normal(&mut self, on: &str, command_id: &str) {
		let keys = parse_normal_sequence(on).expect("default key binding should be valid");
		self.normal_bindings.push(NormalKeyBinding { keys, command_id: command_id.to_string() });
	}

	fn bind_default_visual(&mut self, on: &str, command_id: &str) {
		let keys = parse_normal_sequence(on).expect("default key binding should be valid");
		self.visual_bindings.push(VisualKeyBinding { keys, command_id: command_id.to_string() });
	}

	fn bind_default_command(&mut self, name: &str, command_id: &str) {
		self
			.command_aliases
			.push(CommandAlias { name: name.to_string(), command_id: command_id.to_string() });
	}
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CommandConfigFile {
	#[serde(default, alias = "mgr")]
	pub normal:  CommandKeymapSection,
	#[serde(default)]
	pub visual:  CommandKeymapSection,
	#[serde(default)]
	pub command: CommandAliasSection,
}

impl CommandConfigFile {
	pub fn with_defaults() -> Self { CommandRegistry::with_defaults().export_config() }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CommandKeymapSection {
	#[serde(default)]
	pub keymap: Vec<KeymapBindingConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CommandAliasSection {
	#[serde(default)]
	pub commands: Vec<CommandAliasConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeymapBindingConfig {
	pub on:   KeyBindingOn,
	pub run:  String,
	pub desc: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum KeyBindingOn {
	Single(String),
	Many(Vec<String>),
}

impl KeyBindingOn {
	fn parse_keys(&self) -> Result<Vec<NormalSequenceKey>, String> {
		match self {
			Self::Single(token) => parse_normal_sequence(token),
			Self::Many(tokens) => parse_key_token_array(tokens),
		}
	}

	fn display_for_error(&self) -> String {
		match self {
			Self::Single(token) => token.clone(),
			Self::Many(tokens) => format!("[{}]", tokens.join(",")),
		}
	}
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandAliasConfig {
	pub name: String,
	pub run:  String,
	pub desc: Option<String>,
}

trait KeyBindingView {
	fn keys(&self) -> &[NormalSequenceKey];
	fn command_id(&self) -> &str;
}

impl KeyBindingView for NormalKeyBinding {
	fn keys(&self) -> &[NormalSequenceKey] { self.keys.as_slice() }

	fn command_id(&self) -> &str { self.command_id.as_str() }
}

impl KeyBindingView for VisualKeyBinding {
	fn keys(&self) -> &[NormalSequenceKey] { self.keys.as_slice() }

	fn command_id(&self) -> &str { self.command_id.as_str() }
}

#[derive(Debug, Clone)]
pub struct PluginCommandRegistration {
	pub id:          String,
	pub plugin_id:   String,
	pub category:    String,
	pub description: String,
	pub arg_kind:    CommandArgKind,
}

fn resolve_key_binding_set<T>(
	commands: &HashMap<String, CommandSpec>,
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
	commands: &HashMap<String, CommandSpec>,
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
			on:   KeyBindingOn::Single(render_normal_sequence(binding.keys())),
			run:  binding.command_id().to_string(),
			desc: Some(spec.description.clone()),
		});
	}
	exported
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

fn parse_key_token_array(tokens: &[String]) -> Result<Vec<NormalSequenceKey>, String> {
	if tokens.is_empty() {
		return Err("empty key token list".to_string());
	}
	let mut keys = Vec::new();
	for token in tokens {
		let parsed = parse_normal_sequence(token)?;
		keys.extend(parsed);
	}
	Ok(keys)
}

fn render_normal_sequence(keys: &[NormalSequenceKey]) -> String {
	keys
		.iter()
		.map(|key| match key {
			NormalSequenceKey::Leader => "<leader>".to_string(),
			NormalSequenceKey::Tab => "<Tab>".to_string(),
			NormalSequenceKey::Esc => "<Esc>".to_string(),
			NormalSequenceKey::Char(ch) => ch.to_string(),
			NormalSequenceKey::Ctrl(ch) => format!("<C-{}>", ch),
		})
		.collect::<Vec<_>>()
		.join("")
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
	fn config_should_override_existing_normal_binding() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			normal: CommandKeymapSection {
				keymap: vec![KeymapBindingConfig {
					on:   KeyBindingOn::Single("H".to_string()),
					run:  "core.buffer.next".to_string(),
					desc: Some("custom".to_string()),
				}],
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);

		assert!(errors.is_empty());
		assert_eq!(
			registry.resolve_normal_sequence(&[NormalSequenceKey::Char('H')]),
			BindingMatch::Exact(CommandTarget::Builtin(BuiltinCommand::BufferSwitchNext))
		);
		assert_eq!(registry.resolve_normal_sequence(&[NormalSequenceKey::Char('L')]), BindingMatch::NoMatch);
	}

	#[test]
	fn config_should_register_command_alias() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "qq".to_string(),
					run:  "core.quit_all".to_string(),
					desc: Some("custom".to_string()),
				}],
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);
		let resolved = registry.resolve_command_input("qq").expect("command alias should resolve");

		assert!(errors.is_empty());
		assert_eq!(resolved.spec.target, CommandTarget::Builtin(BuiltinCommand::QuitAll));
	}

	#[test]
	fn configured_command_aliases_should_replace_default_alias_table() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			command: CommandAliasSection {
				commands: vec![CommandAliasConfig {
					name: "haha".to_string(),
					run:  "core.quit_all".to_string(),
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
	fn manager_keymap_should_accept_array_form() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			normal: CommandKeymapSection {
				keymap: vec![KeymapBindingConfig {
					on:   KeyBindingOn::Many(vec!["g".to_string(), "g".to_string()]),
					run:  "core.cursor.file_end".to_string(),
					desc: Some("custom".to_string()),
				}],
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);

		assert!(errors.is_empty());
		assert_eq!(
			registry.resolve_normal_sequence(&[NormalSequenceKey::Char('g'), NormalSequenceKey::Char('g')]),
			BindingMatch::Exact(CommandTarget::Builtin(BuiltinCommand::MoveFileEnd))
		);
	}

	#[test]
	fn normal_keymap_should_accept_plugin_run_directive() {
		let mut registry = CommandRegistry::with_defaults();
		let config = CommandConfigFile {
			normal: CommandKeymapSection {
				keymap: vec![KeymapBindingConfig {
					on:   KeyBindingOn::Many(vec!["c".to_string(), "m".to_string()]),
					run:  "plugin chmod".to_string(),
					desc: Some("plugin".to_string()),
				}],
			},
			..CommandConfigFile::default()
		};

		let errors = registry.apply_config(&config);
		let resolved =
			registry.resolve_normal_sequence(&[NormalSequenceKey::Char('c'), NormalSequenceKey::Char('m')]);

		assert!(errors.is_empty());
		assert!(matches!(resolved, BindingMatch::Exact(CommandTarget::Plugin { .. })));
	}
}
