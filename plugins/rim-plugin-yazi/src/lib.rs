use rim_plugin_api::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PluginCommandSet)]
pub enum YaziCommand {
	/// Open the host file picker
	Yazi,
}

struct YaziPlugin;

impl Plugin for YaziPlugin {
	type Commands = YaziCommand;

	const ID: &'static str = "yazi";
	const NAME: &'static str = "Yazi Plugin";
	const VERSION: &'static str = env!("CARGO_PKG_VERSION");

	fn run_command(request: PluginCommandRequest) -> PluginCommandOutcome {
		match YaziCommand::decode(&request) {
			Ok(YaziCommandDecoded::Yazi) => Ok(yazi_command()),
			Err(err) => Err(err),
		}
	}
}

fn yazi_command() -> PluginCommandResponse {
	PluginCommandResponse {
		effects: vec![PluginEffect::RequestAction(PluginAction::PickFile {
			command:                vec!["yazi".to_string(), "--chooser-file".to_string(), String::new()],
			chooser_file_arg_index: 2,
		})],
	}
}

export_plugin!(YaziPlugin);

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn descriptor_should_be_generated_for_yazi_plugin() {
		let descriptor = <YaziPlugin as Plugin>::descriptor();

		assert_eq!(descriptor.metadata.id, "yazi");
		assert_eq!(descriptor.commands.len(), 1);
		assert_eq!(descriptor.commands[0].id, "yazi");
		assert_eq!(descriptor.commands[0].name, "Yazi");
		assert_eq!(descriptor.commands[0].description, "Open yazi file picker");
		assert!(descriptor.commands[0].params.is_empty());
	}

	#[test]
	fn decode_should_match_unit_command() {
		let request = PluginCommandRequest {
			command_id:     "yazi".to_string(),
			argument:       None,
			params:         Vec::new(),
			workspace_root: "/workspace".to_string(),
		};

		assert_eq!(YaziCommand::decode(&request), Ok(YaziCommandDecoded::Yazi));
	}
}
