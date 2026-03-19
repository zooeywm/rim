use std::{collections::HashMap, fmt, fs, io::ErrorKind, ops::Range, path::{Path, PathBuf}};

use anyhow::{Context, Result};
use rim_paths::user_config_root;
use serde::{Deserialize, Serialize};

use crate::{command::{CommandAliasConfig, CommandAliasSection, CommandConfigError, CommandConfigFile, CommandKeymapSection, CommandRegistry, KeymapBindingConfig, ModeKeymapSections, OverlayKeymapSections}, defaults, state::{NotificationLevel, RimState}};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigLoadError {
	pub file:   PathBuf,
	pub line:   Option<usize>,
	pub column: Option<usize>,
	pub reason: String,
}

impl fmt::Display for ConfigLoadError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let file = self
			.file
			.file_name()
			.and_then(|name| name.to_str())
			.map(ToOwned::to_owned)
			.unwrap_or_else(|| self.file.display().to_string());
		match (self.line, self.column) {
			(Some(line), Some(column)) => write!(f, "{}:{}:{} {}", file, line, column, self.reason),
			_ => write!(f, "{} {}", file, self.reason),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConfigEntryPosition {
	pub line:   usize,
	pub column: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CommandConfigLocations {
	keymap_positions:  HashMap<(String, usize), ConfigEntryPosition>,
	command_positions: HashMap<usize, ConfigEntryPosition>,
}

impl CommandConfigLocations {
	pub(crate) fn locate(&self, error: &CommandConfigError) -> Option<ConfigEntryPosition> {
		match error {
			CommandConfigError::Keymap { scope, binding_index, .. } => {
				self.keymap_positions.get(&(scope.clone(), *binding_index)).copied()
			}
			CommandConfigError::CommandAlias { alias_index, .. } => {
				self.command_positions.get(alias_index).copied()
			}
		}
	}
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedCommandConfig {
	pub config:      CommandConfigFile,
	pub locations:   CommandConfigLocations,
	pub config_path: PathBuf,
}

pub(crate) fn format_command_config_error(
	loaded: &LoadedCommandConfig,
	error: &CommandConfigError,
) -> ConfigLoadError {
	let reason = match error {
		CommandConfigError::Keymap { reason, .. } | CommandConfigError::CommandAlias { reason, .. } => {
			reason.clone()
		}
	};
	let position = loaded.locations.locate(error);
	ConfigLoadError {
		file: loaded.config_path.clone(),
		line: position.map(|item| item.line),
		column: position.map(|item| item.column),
		reason,
	}
}

pub(crate) fn load_keymap_config() -> std::result::Result<Option<LoadedCommandConfig>, ConfigLoadError> {
	load_keymap_config_from_path(keymaps_config_path().as_path())
}

pub(crate) fn load_command_alias_config() -> std::result::Result<Option<LoadedCommandConfig>, ConfigLoadError>
{
	load_command_alias_config_from_path(commands_config_path().as_path())
}

pub(crate) fn load_editor_config() -> std::result::Result<Option<EditorConfigFile>, ConfigLoadError> {
	load_editor_config_from_path(editor_config_path().as_path())
}

pub fn keymaps_config_path() -> PathBuf { user_config_root().join("keymaps.toml") }

pub fn commands_config_path() -> PathBuf { user_config_root().join("commands.toml") }

pub fn editor_config_path() -> PathBuf { user_config_root().join("editor.toml") }

pub fn apply_all_configs(state: &mut RimState) -> Vec<String> {
	let mut errors = Vec::new();
	reset_config_state_to_defaults(state);
	match load_editor_config() {
		Ok(Some(config)) => {
			state.workbench.leader_key = config.editor.leader_key;
			state.workbench.cursor_scroll_threshold = config.editor.cursor_scroll_threshold;
			state.workbench.key_hints_width = config.editor.key_hints_width;
			state.workbench.key_hints_max_height = config.editor.key_hints_max_height;
		}
		Ok(None) => {}
		Err(err) => {
			tracing::error!("editor config load failed: {}", err);
			errors.push(err.to_string());
		}
	}
	match load_keymap_config() {
		Ok(Some(loaded)) => {
			for error in state.apply_command_config(&loaded.config) {
				tracing::error!("keymaps config ignored entry: {}", error);
				errors.push(format_command_config_error(&loaded, &error).to_string());
			}
		}
		Ok(None) => {}
		Err(err) => {
			tracing::error!("keymaps config load failed: {}", err);
			errors.push(err.to_string());
		}
	}
	match load_command_alias_config() {
		Ok(Some(loaded)) => {
			for error in state.apply_command_config(&loaded.config) {
				tracing::error!("commands config ignored entry: {}", error);
				errors.push(format_command_config_error(&loaded, &error).to_string());
			}
		}
		Ok(None) => {}
		Err(err) => {
			tracing::error!("commands config load failed: {}", err);
			errors.push(err.to_string());
		}
	}
	errors
}

pub fn apply_config_errors_to_status(state: &mut RimState, errors: Vec<String>) {
	let Some(first) = errors.first() else {
		return;
	};
	let suffix = if errors.len() > 1 { format!(" (+{} more)", errors.len() - 1) } else { String::new() };
	state.push_notification(NotificationLevel::Error, format!("config error: {}{}", first, suffix));
}

pub fn reset_config_state_to_defaults(state: &mut RimState) {
	let default_editor = defaults::default_editor_config();
	state.workbench.leader_key = default_editor.editor.leader_key;
	state.workbench.cursor_scroll_threshold = default_editor.editor.cursor_scroll_threshold;
	state.workbench.key_hints_width = default_editor.editor.key_hints_width;
	state.workbench.key_hints_max_height = default_editor.editor.key_hints_max_height;
	state.workbench.command_registry = CommandRegistry::with_defaults();
}

fn load_keymap_config_from_path(
	config_path: &Path,
) -> std::result::Result<Option<LoadedCommandConfig>, ConfigLoadError> {
	let config_text = read_optional_config_text(config_path).map_err(|err| ConfigLoadError {
		file:   config_path.to_path_buf(),
		line:   None,
		column: None,
		reason: format!("read failed: {}", err),
	})?;
	let Some(config_text) = config_text else {
		return Ok(None);
	};
	let keymap_config = toml::from_str::<KeymapsConfigFileSpanned>(config_text.as_str())
		.map_err(|err| map_toml_parse_error(config_path, config_text.as_str(), err))?;
	let (mode, overlay, locations) = keymap_config.into_runtime_sections_with_locations(config_text.as_str());
	Ok(Some(LoadedCommandConfig {
		config: CommandConfigFile { mode, overlay, command: CommandAliasSection::default() },
		locations,
		config_path: config_path.to_path_buf(),
	}))
}

fn load_command_alias_config_from_path(
	config_path: &Path,
) -> std::result::Result<Option<LoadedCommandConfig>, ConfigLoadError> {
	let config_text = read_optional_config_text(config_path).map_err(|err| ConfigLoadError {
		file:   config_path.to_path_buf(),
		line:   None,
		column: None,
		reason: format!("read failed: {}", err),
	})?;
	let Some(config_text) = config_text else {
		return Ok(None);
	};
	let command_config = toml::from_str::<CommandsConfigFileSpanned>(config_text.as_str())
		.map_err(|err| map_toml_parse_error(config_path, config_text.as_str(), err))?;
	let (command, locations) = command_config.into_runtime_with_locations(config_text.as_str());
	Ok(Some(LoadedCommandConfig {
		config: CommandConfigFile {
			mode: ModeKeymapSections::default(),
			overlay: OverlayKeymapSections::default(),
			command,
		},
		locations,
		config_path: config_path.to_path_buf(),
	}))
}

fn load_editor_config_from_path(
	config_path: &Path,
) -> std::result::Result<Option<EditorConfigFile>, ConfigLoadError> {
	let config_text = read_optional_config_text(config_path).map_err(|err| ConfigLoadError {
		file:   config_path.to_path_buf(),
		line:   None,
		column: None,
		reason: format!("read failed: {}", err),
	})?;
	let Some(config_text) = config_text else {
		return Ok(None);
	};
	let config = toml::from_str::<EditorConfigFile>(config_text.as_str())
		.map_err(|err| map_toml_parse_error(config_path, config_text.as_str(), err))?;
	Ok(Some(config))
}

fn map_toml_parse_error(config_path: &Path, input: &str, err: toml::de::Error) -> ConfigLoadError {
	let (line, column) = span_to_line_column(input, err.span()).unwrap_or((1, 1));
	ConfigLoadError {
		file:   config_path.to_path_buf(),
		line:   Some(line),
		column: Some(column),
		reason: err.message().to_string(),
	}
}

fn span_to_line_column(input: &str, span: Option<Range<usize>>) -> Option<(usize, usize)> {
	let index = span?.start.min(input.len());
	line_column_from_index(input, index).map(|position| (position.line, position.column))
}

fn line_column_from_index(input: &str, mut index: usize) -> Option<ConfigEntryPosition> {
	index = index.min(input.len());
	while index > 0 && !input.is_char_boundary(index) {
		index -= 1;
	}
	let prefix = &input[..index];
	let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
	let column = match prefix.rsplit_once('\n') {
		Some((_, tail)) => tail.chars().count() + 1,
		None => prefix.chars().count() + 1,
	};
	Some(ConfigEntryPosition { line, column })
}

fn read_optional_config_text(config_path: &Path) -> Result<Option<String>> {
	match fs::read_to_string(config_path) {
		Ok(config_text) => Ok(Some(config_text)),
		Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
		Err(err) => Err(err).with_context(|| format!("read config failed: {}", config_path.display())),
	}
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct KeymapsConfigFileSpanned {
	#[serde(default)]
	mode:          ModeKeymapSectionsSpanned,
	#[serde(default)]
	overlay:       OverlayKeymapSectionsSpanned,
	#[serde(default, rename = "normal", alias = "mgr")]
	legacy_normal: CommandKeymapSectionSpanned,
	#[serde(default, rename = "visual")]
	legacy_visual: CommandKeymapSectionSpanned,
}

impl KeymapsConfigFileSpanned {
	fn into_runtime_sections_with_locations(
		mut self,
		input: &str,
	) -> (ModeKeymapSections, OverlayKeymapSections, CommandConfigLocations) {
		if self.mode.normal.keymap.is_empty() && !self.legacy_normal.keymap.is_empty() {
			self.mode.normal = self.legacy_normal;
		}
		if self.mode.visual.keymap.is_empty() && !self.legacy_visual.keymap.is_empty() {
			self.mode.visual = self.legacy_visual;
		}
		let mut locations = CommandConfigLocations::default();
		let normal = collect_keymap_section("mode.normal", self.mode.normal, input, &mut locations);
		let visual = collect_keymap_section("mode.visual", self.mode.visual, input, &mut locations);
		let command = collect_keymap_section("mode.command", self.mode.command, input, &mut locations);
		let insert = collect_keymap_section("mode.insert", self.mode.insert, input, &mut locations);
		let whichkey = collect_keymap_section("overlay.whichkey", self.overlay.whichkey, input, &mut locations);
		let command_palette =
			collect_keymap_section("overlay.command_palette", self.overlay.command_palette, input, &mut locations);
		let picker = collect_keymap_section("overlay.picker", self.overlay.picker, input, &mut locations);
		let notification_center = collect_keymap_section(
			"overlay.notification_center",
			self.overlay.notification_center,
			input,
			&mut locations,
		);
		(
			ModeKeymapSections { normal, visual, command, insert },
			OverlayKeymapSections { whichkey, command_palette, picker, notification_center },
			locations,
		)
	}
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct CommandsConfigFileSpanned {
	#[serde(default)]
	command: CommandAliasSectionSpanned,
}

impl CommandsConfigFileSpanned {
	fn into_runtime_with_locations(self, input: &str) -> (CommandAliasSection, CommandConfigLocations) {
		let mut locations = CommandConfigLocations::default();
		let command = collect_command_alias_section(self.command, input, &mut locations);
		(command, locations)
	}
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct CommandKeymapSectionSpanned {
	#[serde(default)]
	keymap: Vec<toml::Spanned<KeymapBindingConfig>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct ModeKeymapSectionsSpanned {
	#[serde(default, alias = "mgr")]
	normal:  CommandKeymapSectionSpanned,
	#[serde(default)]
	visual:  CommandKeymapSectionSpanned,
	#[serde(default)]
	command: CommandKeymapSectionSpanned,
	#[serde(default)]
	insert:  CommandKeymapSectionSpanned,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct OverlayKeymapSectionsSpanned {
	#[serde(default)]
	whichkey:            CommandKeymapSectionSpanned,
	#[serde(default)]
	command_palette:     CommandKeymapSectionSpanned,
	#[serde(default)]
	picker:              CommandKeymapSectionSpanned,
	#[serde(default)]
	notification_center: CommandKeymapSectionSpanned,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct CommandAliasSectionSpanned {
	#[serde(default)]
	commands: Vec<toml::Spanned<CommandAliasConfig>>,
}

fn collect_keymap_section(
	scope: &str,
	section: CommandKeymapSectionSpanned,
	input: &str,
	locations: &mut CommandConfigLocations,
) -> CommandKeymapSection {
	let mut keymap = Vec::with_capacity(section.keymap.len());
	for (index, binding) in section.keymap.into_iter().enumerate() {
		if let Some(position) = line_column_from_index(input, binding.span().start) {
			locations.keymap_positions.insert((scope.to_string(), index), position);
		}
		keymap.push(binding.into_inner());
	}
	CommandKeymapSection { keymap }
}

fn collect_command_alias_section(
	section: CommandAliasSectionSpanned,
	input: &str,
	locations: &mut CommandConfigLocations,
) -> CommandAliasSection {
	let mut commands = Vec::with_capacity(section.commands.len());
	for (index, command) in section.commands.into_iter().enumerate() {
		if let Some(position) = line_column_from_index(input, command.span().start) {
			locations.command_positions.insert(index, position);
		}
		commands.push(command.into_inner());
	}
	CommandAliasSection { commands }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditorConfigFile {
	#[serde(default)]
	pub editor: EditorConfigSection,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditorConfigSection {
	#[serde(default = "default_leader_key")]
	pub leader_key:              char,
	#[serde(default = "default_cursor_scroll_threshold")]
	pub cursor_scroll_threshold: u16,
	#[serde(default = "default_key_hints_width")]
	pub key_hints_width:         u16,
	#[serde(default = "default_key_hints_max_height")]
	pub key_hints_max_height:    u16,
}

impl Default for EditorConfigSection {
	fn default() -> Self {
		Self {
			leader_key:              default_leader_key(),
			cursor_scroll_threshold: default_cursor_scroll_threshold(),
			key_hints_width:         default_key_hints_width(),
			key_hints_max_height:    default_key_hints_max_height(),
		}
	}
}

fn default_leader_key() -> char { defaults::default_editor_config().editor.leader_key }

fn default_cursor_scroll_threshold() -> u16 {
	defaults::default_editor_config().editor.cursor_scroll_threshold
}

fn default_key_hints_width() -> u16 { defaults::default_editor_config().editor.key_hints_width }

fn default_key_hints_max_height() -> u16 { defaults::default_editor_config().editor.key_hints_max_height }

#[cfg(test)]
mod tests {
	use super::*;
	use crate::state::RimState;

	fn unique_temp_config_dir(label: &str) -> PathBuf {
		let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
		std::env::temp_dir().join(format!("rim-config-test-{}-{}", label, nanos))
	}

	#[test]
	fn partial_keymaps_config_should_only_override_keymap_entries() {
		let config_dir = unique_temp_config_dir("keymaps");
		let keymaps_path = config_dir.join("keymaps.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			keymaps_path.as_path(),
			r#"
[normal]
keymap = [
  { on = ["H"], run = "core.buffer.next", desc = "Switch buffer" },
]
"#,
		)
		.expect("partial keymaps config should be written");

		let loaded = load_keymap_config_from_path(keymaps_path.as_path())
			.expect("partial keymaps config should load")
			.expect("config");
		assert_eq!(loaded.config.mode.normal.keymap.len(), 1);
		assert!(loaded.config.mode.visual.keymap.is_empty());
		assert!(loaded.config.command.commands.is_empty());
		let _ = fs::remove_dir_all(config_dir);
	}

	#[test]
	fn partial_commands_config_should_only_override_command_alias_entries() {
		let config_dir = unique_temp_config_dir("commands");
		let commands_path = config_dir.join("commands.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			commands_path.as_path(),
			r#"
[command]
commands = [
  { name = "haha", run = "core.quit", desc = "Quit" },
]
"#,
		)
		.expect("partial commands config should be written");

		let loaded = load_command_alias_config_from_path(commands_path.as_path())
			.expect("partial commands config should load")
			.expect("config");
		assert_eq!(loaded.config.command.commands.len(), 1);
		assert!(loaded.config.mode.normal.keymap.is_empty());
		let _ = fs::remove_dir_all(config_dir);
	}

	#[test]
	fn editor_config_should_parse_scroll_threshold() {
		let config_dir = unique_temp_config_dir("editor");
		let editor_path = config_dir.join("editor.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			editor_path.as_path(),
			r#"
[editor]
leader_key = ","
cursor_scroll_threshold = 3
key_hints_width = 64
key_hints_max_height = 28
"#,
		)
		.expect("editor config should be written");

		let loaded = load_editor_config_from_path(editor_path.as_path())
			.expect("editor config should load")
			.expect("config");
		assert_eq!(loaded.editor.leader_key, ',');
		assert_eq!(loaded.editor.cursor_scroll_threshold, 3);
		assert_eq!(loaded.editor.key_hints_width, 64);
		assert_eq!(loaded.editor.key_hints_max_height, 28);
		let _ = fs::remove_dir_all(config_dir);
	}

	#[test]
	fn editor_config_should_fail_on_unknown_field() {
		let config_dir = unique_temp_config_dir("editor-unknown-field");
		let editor_path = config_dir.join("editor.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			editor_path.as_path(),
			r#"
[editor]
leader_key = ","

[command]
commands = [
  { name = "qq", run = "core.quit_all" },
]
"#,
		)
		.expect("editor config should be written");

		let err = load_editor_config_from_path(editor_path.as_path())
			.expect_err("editor config with command section should fail");
		assert_eq!(err.file, editor_path);
		assert!(err.line.is_some());
		assert!(err.column.is_some());
		assert!(err.reason.contains("unknown field"));
		let _ = fs::remove_dir_all(config_dir);
	}

	#[test]
	fn keymaps_config_should_fail_on_unknown_field() {
		let config_dir = unique_temp_config_dir("keymaps-unknown-field");
		let keymaps_path = config_dir.join("keymaps.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			keymaps_path.as_path(),
			r#"
[mode.normal]
keymap = [
  { on = "H", run = "core.buffer.next", args = ["x"] },
]
"#,
		)
		.expect("keymaps config should be written");

		let err = load_keymap_config_from_path(keymaps_path.as_path())
			.expect_err("keymaps config with unknown field should fail");
		assert_eq!(err.file, keymaps_path);
		assert!(err.line.is_some());
		assert!(err.column.is_some());
		assert!(err.reason.contains("unknown field"));
		let _ = fs::remove_dir_all(config_dir);
	}

	#[test]
	fn commands_config_should_fail_on_unknown_field() {
		let config_dir = unique_temp_config_dir("commands-unknown-field");
		let commands_path = config_dir.join("commands.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			commands_path.as_path(),
			r#"
[command]
commands = [
  { name = "qq", run = "core.quit_all", args = ["x"] },
]
"#,
		)
		.expect("commands config should be written");

		let err = load_command_alias_config_from_path(commands_path.as_path())
			.expect_err("commands config with unknown field should fail");
		assert_eq!(err.file, commands_path);
		assert!(err.line.is_some());
		assert!(err.column.is_some());
		assert!(err.reason.contains("unknown field"));
		let _ = fs::remove_dir_all(config_dir);
	}

	#[test]
	fn formatted_keymap_semantic_error_should_include_line_column_and_reason() {
		let config_dir = unique_temp_config_dir("keymaps-semantic-error");
		let keymaps_path = config_dir.join("keymaps.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			keymaps_path.as_path(),
			r#"
[mode.normal]
keymap = [
  { on = "g", run = "core.cursor.down" },
]
"#,
		)
		.expect("keymaps config should be written");

		let loaded = load_keymap_config_from_path(keymaps_path.as_path())
			.expect("keymaps config should load")
			.expect("config");
		let mut state = RimState::new();
		let errors = state.apply_command_config(&loaded.config);
		assert_eq!(errors.len(), 1);
		let rendered = format_command_config_error(&loaded, &errors[0]).to_string();
		assert!(rendered.contains("keymaps.toml:"));
		assert!(rendered.contains("prefix conflict"));
		let _ = fs::remove_dir_all(config_dir);
	}

	#[test]
	fn formatted_command_semantic_error_should_include_line_column_and_reason() {
		let config_dir = unique_temp_config_dir("commands-semantic-error");
		let commands_path = config_dir.join("commands.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			commands_path.as_path(),
			r#"
[command]
commands = [
  { name = "x", run = "core.save_any" },
]
"#,
		)
		.expect("commands config should be written");

		let loaded = load_command_alias_config_from_path(commands_path.as_path())
			.expect("commands config should load")
			.expect("config");
		let mut state = RimState::new();
		let errors = state.apply_command_config(&loaded.config);
		assert_eq!(errors.len(), 1);
		let rendered = format_command_config_error(&loaded, &errors[0]).to_string();
		assert!(rendered.contains("commands.toml:"));
		assert!(rendered.contains("unknown run directive"));
		let _ = fs::remove_dir_all(config_dir);
	}
}
