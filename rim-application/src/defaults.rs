use std::sync::OnceLock;

use serde::Deserialize;

use crate::{command::CommandConfigFile, config::{EditorConfigFile, EditorConfigSection}};

const DEFAULT_KEYMAPS_TOML: &str = include_str!("../presets/keymaps.toml");
const DEFAULT_COMMANDS_TOML: &str = include_str!("../presets/commands.toml");
const DEFAULT_EDITOR_TOML: &str = include_str!("../presets/editor.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditorPresetFile {
	editor: EditorPresetSection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditorPresetSection {
	leader_key:              char,
	cursor_scroll_threshold: u16,
	key_hints_width:         u16,
	key_hints_max_height:    u16,
}

pub(crate) fn default_command_config() -> &'static CommandConfigFile {
	static DEFAULT_COMMAND_CONFIG: OnceLock<CommandConfigFile> = OnceLock::new();
	DEFAULT_COMMAND_CONFIG.get_or_init(|| {
		let keymaps = toml::from_str::<CommandConfigFile>(DEFAULT_KEYMAPS_TOML)
			.expect("embedded default keymaps preset should be valid");
		let commands = toml::from_str::<CommandConfigFile>(DEFAULT_COMMANDS_TOML)
			.expect("embedded default commands preset should be valid");
		CommandConfigFile { mode: keymaps.mode, overlay: keymaps.overlay, command: commands.command }
	})
}

pub(crate) fn default_editor_config() -> &'static EditorConfigFile {
	static DEFAULT_EDITOR_CONFIG: OnceLock<EditorConfigFile> = OnceLock::new();
	DEFAULT_EDITOR_CONFIG.get_or_init(|| {
		let preset = toml::from_str::<EditorPresetFile>(DEFAULT_EDITOR_TOML)
			.expect("embedded default editor preset should be valid");
		EditorConfigFile {
			editor: EditorConfigSection {
				leader_key:              preset.editor.leader_key,
				cursor_scroll_threshold: preset.editor.cursor_scroll_threshold,
				key_hints_width:         preset.editor.key_hints_width,
				key_hints_max_height:    preset.editor.key_hints_max_height,
			},
		}
	})
}
