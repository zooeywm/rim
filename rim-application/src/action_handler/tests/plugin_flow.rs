use std::ops::ControlFlow;

use rim_ports::{PluginAction as RequestedPluginAction, PluginCapability, PluginCommandMetadata, PluginCommandResponse, PluginDiscoveryResult, PluginEffect, PluginMetadata, PluginNotification, PluginNotificationLevel, PluginPanel, PluginRegistration};

use super::support::RecordingPorts;
use crate::{action::{AppAction, PluginRuntimeAction}, state::RimState};

fn sample_plugin() -> PluginRegistration {
	PluginRegistration {
		metadata: PluginMetadata {
			id:                    "demo".to_string(),
			name:                  "Demo Plugin".to_string(),
			version:               "0.1.0".to_string(),
			declared_capabilities: vec![PluginCapability::CommandProvider],
		},
		commands: vec![PluginCommandMetadata {
			id:          "echo".to_string(),
			name:        "echo".to_string(),
			description: "Echo command".to_string(),
		}],
	}
}

#[test]
fn discovery_completed_should_register_plugin_commands() {
	let ports = RecordingPorts::default();
	let mut state = RimState::new();

	let flow = state.apply_action(
		&ports,
		AppAction::Plugin(PluginRuntimeAction::DiscoveryCompleted {
			result: Ok(PluginDiscoveryResult { plugins: vec![sample_plugin()], failures: Vec::new() }),
		}),
	);

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.plugin_registrations().len(), 1);
	let resolved = state
		.workbench
		.command_registry
		.resolve_command_input("echo")
		.expect("plugin default command name should resolve");
	assert!(resolved.spec.display_name.is_none());
}

#[test]
fn executing_plugin_command_should_enqueue_runtime_request() {
	let ports = RecordingPorts::default();
	let mut state = RimState::new();
	let _ = state.apply_action(
		&ports,
		AppAction::Plugin(PluginRuntimeAction::DiscoveryCompleted {
			result: Ok(PluginDiscoveryResult { plugins: vec![sample_plugin()], failures: Vec::new() }),
		}),
	);

	let resolved = state
		.workbench
		.command_registry
		.resolve_command_input("echo hello")
		.expect("registered plugin command should resolve");
	let flow = super::super::command_flow::execute_resolved_command(&ports, &mut state, resolved);

	assert!(matches!(flow, ControlFlow::Continue(())));
	let invocations = ports.plugin_invocations.borrow();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].command_id, "plugin.demo.echo");
	assert_eq!(invocations[0].argument.as_deref(), Some("hello"));
}

#[test]
fn command_completed_should_apply_effects_and_requested_actions() {
	let ports = RecordingPorts::default();
	let mut state = RimState::new();
	let initial_tab_count = state.tabs.len();

	let flow = state.apply_action(
		&ports,
		AppAction::Plugin(PluginRuntimeAction::CommandCompleted {
			command_id: "plugin.demo.echo".to_string(),
			result:     Ok(PluginCommandResponse {
				effects: vec![
					PluginEffect::Notify(PluginNotification {
						level:   PluginNotificationLevel::Info,
						message: "plugin finished".to_string(),
					}),
					PluginEffect::ShowPanel(PluginPanel {
						title:  "Plugin Panel".to_string(),
						lines:  vec!["line one".to_string(), "line two".to_string()],
						footer: Some("Close with Esc".to_string()),
					}),
					PluginEffect::RequestAction(RequestedPluginAction::RunCommand {
						command_id: "core.tab.new".to_string(),
						argument:   None,
					}),
				],
			}),
		}),
	);

	assert!(matches!(flow, ControlFlow::Continue(())));
	assert_eq!(state.tabs.len(), initial_tab_count + 1);
	assert!(state.workbench.notifications.iter().any(|entry| entry.message == "plugin finished"));
	let panel = state.floating_window().expect("plugin panel should be open");
	assert_eq!(panel.title, "Plugin Panel");
	assert_eq!(panel.footer.as_deref(), Some("Close with Esc"));
}
