use std::{fs, io::ErrorKind, path::{Path, PathBuf}};

use anyhow::{Context, Result};
use rim_kernel::command::{CommandAliasConfig, CommandConfigFile, KeyBindingOn, KeymapBindingConfig};
use rim_paths::user_config_root;

pub(crate) fn load_command_config() -> Result<Option<CommandConfigFile>> {
	let config_path = command_config_path();
	ensure_default_command_config_file(config_path.as_path())?;
	load_command_config_from_path(config_path.as_path())
}

fn ensure_default_command_config_file(config_path: &Path) -> Result<()> {
	let Some(parent) = config_path.parent() else {
		return Ok(());
	};
	fs::create_dir_all(parent)
		.with_context(|| format!("create config directory failed: {}", parent.display()))?;
	match fs::metadata(config_path) {
		Ok(metadata) if metadata.is_file() => return Ok(()),
		Ok(_) => return Ok(()),
		Err(err) if err.kind() == ErrorKind::NotFound => {}
		Err(err) => {
			return Err(err).with_context(|| format!("inspect config failed: {}", config_path.display()));
		}
	}

	let default_config = CommandConfigFile::with_defaults();
	let default_text = render_command_config_toml(&default_config);
	fs::write(config_path, default_text)
		.with_context(|| format!("write default config failed: {}", config_path.display()))?;
	Ok(())
}

fn load_command_config_from_path(config_path: &Path) -> Result<Option<CommandConfigFile>> {
	let config_text = match fs::read_to_string(config_path) {
		Ok(config_text) => config_text,
		Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
		Err(err) => {
			return Err(err).with_context(|| format!("read config failed: {}", config_path.display()));
		}
	};
	let config = toml::from_str::<CommandConfigFile>(config_text.as_str())
		.with_context(|| format!("parse config failed: {}", config_path.display()))?;
	Ok(Some(config))
}

pub(crate) fn command_config_path() -> PathBuf { user_config_root().join("config.toml") }

fn render_command_config_toml(config: &CommandConfigFile) -> String {
	let mut output = String::new();
	output.push_str("[normal]\n");
	output.push_str("keymap = [\n");
	for binding in &config.normal.keymap {
		output.push_str("  ");
		output.push_str(render_keymap_binding(binding).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n\n");
	output.push_str("[command]\n");
	output.push_str("commands = [\n");
	for command in &config.command.commands {
		output.push_str("  ");
		output.push_str(render_command_alias(command).as_str());
		output.push_str(",\n");
	}
	output.push_str("]\n");
	output
}

fn render_keymap_binding(binding: &KeymapBindingConfig) -> String {
	let on = match &binding.on {
		KeyBindingOn::Single(token) => toml_string_literal(token),
		KeyBindingOn::Many(tokens) => render_string_array(tokens.as_slice()),
	};
	match &binding.desc {
		Some(desc) => format!(
			"{{ on = {}, run = {}, desc = {} }}",
			on,
			toml_string_literal(binding.run.as_str()),
			toml_string_literal(desc.as_str())
		),
		None => format!("{{ on = {}, run = {} }}", on, toml_string_literal(binding.run.as_str())),
	}
}

fn render_command_alias(command: &CommandAliasConfig) -> String {
	match &command.desc {
		Some(desc) => format!(
			"{{ name = {}, run = {}, desc = {} }}",
			toml_string_literal(command.name.as_str()),
			toml_string_literal(command.run.as_str()),
			toml_string_literal(desc.as_str())
		),
		None => format!(
			"{{ name = {}, run = {} }}",
			toml_string_literal(command.name.as_str()),
			toml_string_literal(command.run.as_str())
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

#[cfg(test)]
mod tests {
	use super::*;

	fn unique_temp_config_path(label: &str) -> PathBuf {
		let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
		std::env::temp_dir().join(format!("rim-config-test-{}-{}", label, nanos)).join("config.toml")
	}

	#[test]
	fn ensure_default_command_config_file_should_create_full_default_config_when_missing() {
		let config_path = unique_temp_config_path("create");

		ensure_default_command_config_file(config_path.as_path()).expect("default config should be created");
		let loaded = load_command_config_from_path(config_path.as_path())
			.expect("default config should load")
			.expect("config");

		assert_eq!(loaded.normal.keymap.len(), CommandConfigFile::with_defaults().normal.keymap.len());
		assert_eq!(loaded.command.commands.len(), CommandConfigFile::with_defaults().command.commands.len());
		let text = fs::read_to_string(config_path.as_path()).expect("default config should be readable");
		assert!(text.contains("[normal]\nkeymap = ["));
		assert!(!text.contains("[[normal.keymap]]"));
		let _ = fs::remove_dir_all(config_path.parent().expect("config path should have parent"));
	}

	#[test]
	fn partial_normal_config_should_keep_missing_sections_at_code_defaults_after_overlay() {
		let config_path = unique_temp_config_path("overlay");
		fs::create_dir_all(config_path.parent().expect("config path should have parent"))
			.expect("config directory should be created");
		fs::write(
			config_path.as_path(),
			r#"
[normal]
keymap = [
  { on = ["H"], run = "core.buffer.next", desc = "Switch buffer" },
]
"#,
		)
		.expect("partial config should be written");

		let loaded = load_command_config_from_path(config_path.as_path())
			.expect("partial config should load")
			.expect("config");

		assert_eq!(loaded.normal.keymap.len(), 1);
		assert!(loaded.command.commands.is_empty());
		let _ = fs::remove_dir_all(config_path.parent().expect("config path should have parent"));
	}
}
