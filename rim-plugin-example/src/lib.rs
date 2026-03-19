use rim_plugin_api::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PluginCommandSet)]
pub enum ExampleCommand {
	/// Show a panel with the current plugin request context
	Inspect { message: Option<Text> },
	/// Request that Rim opens a new tab
	NewTab,
}

struct ExamplePlugin;

impl Plugin for ExamplePlugin {
	type Commands = ExampleCommand;

	const ID: &'static str = "example";
	const NAME: &'static str = "Example Plugin";
	const VERSION: &'static str = env!("CARGO_PKG_VERSION");

	fn run_command(request: PluginCommandRequest) -> PluginCommandOutcome {
		match ExampleCommand::decode(&request) {
			Ok(ExampleCommandDecoded::Inspect { message }) => Ok(inspect_command(request, message)),
			Ok(ExampleCommandDecoded::NewTab) => Ok(new_tab_command()),
			Err(err) => Err(err),
		}
	}
}

fn inspect_command(request: PluginCommandRequest, message: Option<String>) -> PluginCommandResponse {
	let mut lines =
		vec![format!("command: {}", request.command_id), format!("workspace: {}", request.workspace_root)];
	if let Some(argument) = request.argument.as_deref() {
		lines.push(format!("argument: {}", argument));
	}
	if let Some(message) = message.as_deref() {
		lines.push(format!("message: {}", message));
	}

	PluginCommandResponse {
		effects: vec![
			PluginEffect::Notify(PluginNotification {
				level:   PluginNotificationLevel::Info,
				message: "example.inspect completed".to_string(),
			}),
			PluginEffect::ShowPanel(PluginPanel {
				title: "Example Plugin".to_string(),
				lines,
				footer: Some("CommandProvider v1".to_string()),
			}),
		],
	}
}

fn new_tab_command() -> PluginCommandResponse {
	PluginCommandResponse {
		effects: vec![
			PluginEffect::Notify(PluginNotification {
				level:   PluginNotificationLevel::Info,
				message: "example.new-tab requested core.tab.new".to_string(),
			}),
			PluginEffect::RequestAction(PluginAction::RunCommand {
				command_id: "core.tab.new".to_string(),
				argument:   None,
			}),
		],
	}
}

export_plugin!(ExamplePlugin);

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn descriptor_should_be_generated_from_plugin_command_enum() {
		let descriptor = <ExamplePlugin as Plugin>::descriptor();

		assert_eq!(descriptor.metadata.id, "example");
		assert_eq!(descriptor.commands.len(), 2);
		assert_eq!(descriptor.commands[0].id, "inspect");
		assert_eq!(descriptor.commands[0].name, "Inspect");
		assert_eq!(descriptor.commands[0].params.len(), 1);
		assert_eq!(descriptor.commands[0].params[0].name, "message");
		assert_eq!(descriptor.commands[0].params[0].kind, PluginCommandParamKind::Text);
		assert!(descriptor.commands[0].params[0].optional);
		assert_eq!(descriptor.commands[1].id, "new-tab");
		assert_eq!(descriptor.commands[1].name, "NewTab");
		assert!(descriptor.commands[1].params.is_empty());
	}

	#[test]
	fn command_enum_should_decode_request_params() {
		let request = PluginCommandRequest {
			command_id:     "inspect".to_string(),
			argument:       None,
			params:         vec![PluginResolvedParam {
				name:  "message".to_string(),
				kind:  PluginCommandParamKind::Text,
				value: "hello".to_string(),
			}],
			workspace_root: "/workspace".to_string(),
		};

		let command = ExampleCommand::get(&request).expect("command should decode");
		let params = ExampleCommand::params(&request).expect("params should decode");
		let decoded = ExampleCommand::decode(&request).expect("command should decode with values");

		assert!(matches!(command, ExampleCommand::Inspect { .. }));
		assert_eq!(params.get_text("message"), Some("hello"));
		assert_eq!(decoded, ExampleCommandDecoded::Inspect { message: Some("hello".to_string()) });
	}
}
