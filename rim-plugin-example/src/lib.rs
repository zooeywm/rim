use rim_plugin_api::prelude::*;

struct ExamplePlugin;

impl CommandProviderPlugin for ExamplePlugin {
	fn descriptor() -> PluginDescriptor {
		PluginDescriptor {
			metadata: PluginMetadata {
				id:                    "example".to_string(),
				name:                  "Example Plugin".to_string(),
				version:               env!("CARGO_PKG_VERSION").to_string(),
				declared_capabilities: vec![PluginCapability::CommandProvider],
			},
			commands: vec![
				PluginCommandMetadata {
					id:          "inspect".to_string(),
					name:        "Inspect".to_string(),
					description: "Show a panel with the current plugin request context".to_string(),
					params:      vec![PluginCommandParamSpec {
						name:     "message".to_string(),
						kind:     PluginCommandParamKind::String,
						optional: true,
					}],
				},
				PluginCommandMetadata {
					id:          "new-tab".to_string(),
					name:        "NewTab".to_string(),
					description: "Request that Rim opens a new tab".to_string(),
					params:      Vec::new(),
				},
			],
		}
	}

	fn run_command(request: PluginCommandRequest) -> PluginCommandOutcome {
		match request.command_id.as_str() {
			"inspect" => Ok(inspect_command(request)),
			"new-tab" => Ok(new_tab_command()),
			other => Err(PluginCommandError::CommandUnavailable { command_id: other.to_string() }),
		}
	}
}

fn inspect_command(request: PluginCommandRequest) -> PluginCommandResponse {
	let mut lines =
		vec![format!("command: {}", request.command_id), format!("workspace: {}", request.workspace_root)];
	if let Some(argument) = request.argument.as_deref() {
		lines.push(format!("argument: {}", argument));
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
