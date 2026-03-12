use std::{fs, io::ErrorKind, path::{Path, PathBuf}};

use anyhow::{Context, Result};
use rim_kernel::command::{CommandAliasConfig, CommandAliasSection, CommandConfigFile, CommandKeymapSection, KeymapBindingConfig, ModeKeymapSections, OverlayKeymapSections};
use rim_paths::user_config_root;
use serde::{Deserialize, Serialize};

pub(crate) fn load_keymap_config() -> Result<Option<CommandConfigFile>> {
	load_keymap_config_from_path(keymaps_config_path().as_path())
}

pub(crate) fn load_command_alias_config() -> Result<Option<CommandConfigFile>> {
	load_command_alias_config_from_path(commands_config_path().as_path())
}

pub(crate) fn load_app_config() -> Result<Option<AppConfigFile>> {
	load_app_config_from_path(app_config_path().as_path())
}

pub(crate) fn keymaps_config_path() -> PathBuf { user_config_root().join("keymaps.toml") }

pub(crate) fn commands_config_path() -> PathBuf { user_config_root().join("commands.toml") }

pub(crate) fn app_config_path() -> PathBuf { user_config_root().join("config.toml") }

pub(crate) fn initialize_config_files() -> Result<()> {
	let config_root = user_config_root();
	fs::create_dir_all(config_root.as_path())
		.with_context(|| format!("create config directory failed: {}", config_root.display()))?;
	migrate_legacy_command_config_if_needed(config_root.as_path())?;
	ensure_default_keymaps_config_file(keymaps_config_path().as_path())?;
	ensure_default_commands_config_file(commands_config_path().as_path())?;
	ensure_default_app_config_file(app_config_path().as_path())?;
	Ok(())
}

fn migrate_legacy_command_config_if_needed(config_root: &Path) -> Result<()> {
	let legacy_path = config_root.join("config.toml");
	let keymaps_path = keymaps_config_path();
	let commands_path = commands_config_path();
	if keymaps_path.exists() || commands_path.exists() || !legacy_path.exists() {
		return Ok(());
	}

	let legacy_text = match fs::read_to_string(legacy_path.as_path()) {
		Ok(text) => text,
		Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
		Err(err) => {
			return Err(err).with_context(|| format!("read legacy config failed: {}", legacy_path.display()));
		}
	};
	let Ok(legacy_config) = toml::from_str::<CommandConfigFile>(legacy_text.as_str()) else {
		return Ok(());
	};

	fs::write(keymaps_path.as_path(), render_keymaps_config_toml(&legacy_config.mode, &legacy_config.overlay))
		.with_context(|| format!("write migrated keymaps failed: {}", keymaps_path.display()))?;
	fs::write(
		commands_path.as_path(),
		render_commands_config_toml(&CommandAliasSection { commands: legacy_config.command.commands }),
	)
	.with_context(|| format!("write migrated commands failed: {}", commands_path.display()))?;
	fs::write(app_config_path().as_path(), render_app_config_toml(&AppConfigFile::default()))
		.with_context(|| format!("write migrated app config failed: {}", app_config_path().display()))?;
	Ok(())
}

fn ensure_default_keymaps_config_file(config_path: &Path) -> Result<()> {
	let defaults = CommandConfigFile::with_defaults();
	ensure_default_file(config_path, render_keymaps_config_toml(&defaults.mode, &defaults.overlay))
}

fn ensure_default_commands_config_file(config_path: &Path) -> Result<()> {
	ensure_default_file(config_path, render_commands_config_toml(&CommandConfigFile::with_defaults().command))
}

fn ensure_default_app_config_file(config_path: &Path) -> Result<()> {
	ensure_default_file(config_path, render_app_config_toml(&AppConfigFile::default()))
}

fn ensure_default_file(config_path: &Path, default_text: String) -> Result<()> {
	match fs::metadata(config_path) {
		Ok(metadata) if metadata.is_file() => return Ok(()),
		Ok(_) => return Ok(()),
		Err(err) if err.kind() == ErrorKind::NotFound => {}
		Err(err) => {
			return Err(err).with_context(|| format!("inspect config failed: {}", config_path.display()));
		}
	}

	fs::write(config_path, default_text)
		.with_context(|| format!("write default config failed: {}", config_path.display()))?;
	Ok(())
}

fn load_keymap_config_from_path(config_path: &Path) -> Result<Option<CommandConfigFile>> {
	let config_text = read_optional_config_text(config_path)?;
	let Some(config_text) = config_text else {
		return Ok(None);
	};
	let keymap_config = toml::from_str::<KeymapsConfigFile>(config_text.as_str())
		.with_context(|| format!("parse keymaps config failed: {}", config_path.display()))?;
	let (mode, overlay) = keymap_config.into_runtime_sections();
	Ok(Some(CommandConfigFile { mode, overlay, command: CommandAliasSection::default() }))
}

fn load_command_alias_config_from_path(config_path: &Path) -> Result<Option<CommandConfigFile>> {
	let config_text = read_optional_config_text(config_path)?;
	let Some(config_text) = config_text else {
		return Ok(None);
	};
	let command_config = toml::from_str::<CommandsConfigFile>(config_text.as_str())
		.with_context(|| format!("parse commands config failed: {}", config_path.display()))?;
	Ok(Some(CommandConfigFile {
		mode:    ModeKeymapSections::default(),
		overlay: OverlayKeymapSections::default(),
		command: command_config.command,
	}))
}

fn load_app_config_from_path(config_path: &Path) -> Result<Option<AppConfigFile>> {
	let config_text = read_optional_config_text(config_path)?;
	let Some(config_text) = config_text else {
		return Ok(None);
	};
	let config = toml::from_str::<AppConfigFile>(config_text.as_str())
		.with_context(|| format!("parse app config failed: {}", config_path.display()))?;
	Ok(Some(config))
}

fn read_optional_config_text(config_path: &Path) -> Result<Option<String>> {
	match fs::read_to_string(config_path) {
		Ok(config_text) => Ok(Some(config_text)),
		Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
		Err(err) => Err(err).with_context(|| format!("read config failed: {}", config_path.display())),
	}
}

fn render_keymaps_config_toml(mode: &ModeKeymapSections, overlay: &OverlayKeymapSections) -> String {
	let mut output = String::new();
	output.push_str("[mode.normal]\n");
	output.push_str("keymap = [\n");
	for binding in &mode.normal.keymap {
		output.push_str("  ");
		output.push_str(render_keymap_binding(binding).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n\n[mode.visual]\n");
	output.push_str("keymap = [\n");
	for binding in &mode.visual.keymap {
		output.push_str("  ");
		output.push_str(render_keymap_binding(binding).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n\n[mode.command]\n");
	output.push_str("keymap = [\n");
	for binding in &mode.command.keymap {
		output.push_str("  ");
		output.push_str(render_keymap_binding(binding).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n\n[mode.insert]\n");
	output.push_str("keymap = [\n");
	for binding in &mode.insert.keymap {
		output.push_str("  ");
		output.push_str(render_keymap_binding(binding).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n\n[overlay.whichkey]\n");
	output.push_str("keymap = [\n");
	for binding in &overlay.whichkey.keymap {
		output.push_str("  ");
		output.push_str(render_keymap_binding(binding).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n\n[overlay.command_palette]\n");
	output.push_str("keymap = [\n");
	for binding in &overlay.command_palette.keymap {
		output.push_str("  ");
		output.push_str(render_keymap_binding(binding).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n\n[overlay.picker]\n");
	output.push_str("keymap = [\n");
	for binding in &overlay.picker.keymap {
		output.push_str("  ");
		output.push_str(render_keymap_binding(binding).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n");
	output
}

fn render_commands_config_toml(config: &CommandAliasSection) -> String {
	let mut output = String::new();
	output.push_str("[command]\n");
	output.push_str("commands = [\n");
	for command in &config.commands {
		output.push_str("  ");
		output.push_str(render_command_alias(command).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n");
	output
}

fn render_app_config_toml(config: &AppConfigFile) -> String {
	let mut output = String::new();
	output.push_str("[editor]\n");
	output.push_str(
		format!("leader_key = {}\n", toml_string_literal(config.editor.leader_key.to_string().as_str())).as_str(),
	);
	output.push_str(format!("cursor_scroll_threshold = {}\n", config.editor.cursor_scroll_threshold).as_str());
	output.push_str(format!("key_hints_width = {}\n", config.editor.key_hints_width).as_str());
	output.push_str(format!("key_hints_max_height = {}\n", config.editor.key_hints_max_height).as_str());
	output
}

fn render_keymap_binding(binding: &KeymapBindingConfig) -> String {
	let on = match binding.on.entries() {
		[single] => toml_string_literal(single),
		many => render_string_array(many),
	};
	match &binding.desc {
		Some(desc) => format!(
			"{{ on = {}, run = {}, desc = {} }}",
			on,
			toml_string_literal(binding.run.render().as_str()),
			toml_string_literal(desc.as_str())
		),
		None => format!("{{ on = {}, run = {} }}", on, toml_string_literal(binding.run.render().as_str())),
	}
}

fn render_command_alias(command: &CommandAliasConfig) -> String {
	match &command.desc {
		Some(desc) => format!(
			"{{ name = {}, run = {}, desc = {} }}",
			toml_string_literal(command.name.as_str()),
			toml_string_literal(command.run.render().as_str()),
			toml_string_literal(desc.as_str())
		),
		None => format!(
			"{{ name = {}, run = {} }}",
			toml_string_literal(command.name.as_str()),
			toml_string_literal(command.run.render().as_str())
		),
	}
}

fn render_string_array(values: &[String]) -> String {
	format!(
		"[{}]",
		values.iter().map(|value| toml_string_literal(value.as_str())).collect::<Vec<_>>().join(", ")
	)
}

fn toml_string_literal(text: &str) -> String { toml::Value::String(text.to_string()).to_string() }

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct KeymapsConfigFile {
	#[serde(default)]
	mode:          ModeKeymapSections,
	#[serde(default)]
	overlay:       OverlayKeymapSections,
	#[serde(default, rename = "normal", alias = "mgr")]
	legacy_normal: CommandKeymapSection,
	#[serde(default, rename = "visual")]
	legacy_visual: CommandKeymapSection,
}

impl KeymapsConfigFile {
	fn into_runtime_sections(mut self) -> (ModeKeymapSections, OverlayKeymapSections) {
		if self.mode.normal.keymap.is_empty() && !self.legacy_normal.keymap.is_empty() {
			self.mode.normal = self.legacy_normal;
		}
		if self.mode.visual.keymap.is_empty() && !self.legacy_visual.keymap.is_empty() {
			self.mode.visual = self.legacy_visual;
		}
		(self.mode, self.overlay)
	}
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct CommandsConfigFile {
	#[serde(default)]
	command: CommandAliasSection,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub(crate) struct AppConfigFile {
	#[serde(default)]
	pub editor: EditorConfigSection,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct EditorConfigSection {
	#[serde(default = "default_leader_key")]
	pub leader_key:              char,
	#[serde(default)]
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
			cursor_scroll_threshold: 0,
			key_hints_width:         default_key_hints_width(),
			key_hints_max_height:    default_key_hints_max_height(),
		}
	}
}

fn default_leader_key() -> char { ' ' }

fn default_key_hints_width() -> u16 { 42 }

fn default_key_hints_max_height() -> u16 { 36 }

#[cfg(test)]
mod tests {
	use super::*;

	fn unique_temp_config_dir(label: &str) -> PathBuf {
		let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
		std::env::temp_dir().join(format!("rim-config-test-{}-{}", label, nanos))
	}

	#[test]
	fn ensure_default_config_files_should_create_split_config_files_when_missing() {
		let config_dir = unique_temp_config_dir("create");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");

		ensure_default_keymaps_config_file(config_dir.join("keymaps.toml").as_path())
			.expect("default keymaps should be created");
		ensure_default_commands_config_file(config_dir.join("commands.toml").as_path())
			.expect("default commands should be created");
		ensure_default_app_config_file(config_dir.join("config.toml").as_path())
			.expect("default app config should be created");

		let keymaps_text =
			fs::read_to_string(config_dir.join("keymaps.toml")).expect("default keymaps should be readable");
		let commands_text =
			fs::read_to_string(config_dir.join("commands.toml")).expect("default commands should be readable");
		let app_text =
			fs::read_to_string(config_dir.join("config.toml")).expect("default app config should be readable");

		assert!(keymaps_text.contains("[mode.normal]\nkeymap = ["));
		assert!(keymaps_text.contains("[mode.visual]\nkeymap = ["));
		assert!(keymaps_text.contains("[overlay.whichkey]\nkeymap = ["));
		assert!(keymaps_text.contains(r#"{ on = "<F1>", run = "core.help.keymap""#));
		assert!(keymaps_text.contains(r#"{ on = "<Up>", run = "core.help.keymap_scroll_up""#));
		assert!(keymaps_text.contains(r#"{ on = "<Down>", run = "core.help.keymap_scroll_down""#));
		assert!(keymaps_text.contains(r#"{ on = "<C-p>", run = "core.help.keymap_scroll_up""#));
		assert!(keymaps_text.contains(r#"{ on = "<C-n>", run = "core.help.keymap_scroll_down""#));
		assert!(commands_text.contains("[command]\ncommands = ["));
		assert!(app_text.contains("[editor]\nleader_key = "));
		assert!(app_text.contains("key_hints_width = 42"));
		assert!(app_text.contains("key_hints_max_height = 36"));
		let _ = fs::remove_dir_all(config_dir);
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
		assert_eq!(loaded.mode.normal.keymap.len(), 1);
		assert!(loaded.mode.visual.keymap.is_empty());
		assert!(loaded.command.commands.is_empty());
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
		assert_eq!(loaded.command.commands.len(), 1);
		assert!(loaded.mode.normal.keymap.is_empty());
		let _ = fs::remove_dir_all(config_dir);
	}

	#[test]
	fn app_config_should_parse_scroll_threshold() {
		let config_dir = unique_temp_config_dir("app");
		let app_path = config_dir.join("config.toml");
		fs::create_dir_all(config_dir.as_path()).expect("config directory should be created");
		fs::write(
			app_path.as_path(),
			r#"
[editor]
leader_key = ","
cursor_scroll_threshold = 3
key_hints_width = 64
key_hints_max_height = 28
"#,
		)
		.expect("app config should be written");

		let loaded =
			load_app_config_from_path(app_path.as_path()).expect("app config should load").expect("config");
		assert_eq!(loaded.editor.leader_key, ',');
		assert_eq!(loaded.editor.cursor_scroll_threshold, 3);
		assert_eq!(loaded.editor.key_hints_width, 64);
		assert_eq!(loaded.editor.key_hints_max_height, 28);
		let _ = fs::remove_dir_all(config_dir);
	}
}
