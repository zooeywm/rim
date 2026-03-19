use std::{ops::ControlFlow, path::PathBuf};

use rim_ports::{PluginAction as RequestedPluginAction, PluginCommandResponse, PluginEffect, PluginNotificationLevel};
use tracing::error;

use super::{ActionHandlerError, ActionPorts, PluginPorts, RimState};
use crate::{action::{AppAction, FileAction, PluginRuntimeAction}, command::{CommandArgKind, PluginCommandRegistration}, state::NotificationLevel};

pub(super) fn enqueue_plugin_discovery<P>(ports: &P, state: &RimState)
where P: PluginPorts {
	if let Err(source) = ports.enqueue_discover_plugins(state.workspace_root().display().to_string()) {
		let err = ActionHandlerError::PluginDiscover { source };
		error!("plugin discovery enqueue failed: {}", err);
	}
}

pub(super) fn enqueue_plugin_command<P>(
	ports: &P,
	state: &mut RimState,
	plugin_id: String,
	command_id: String,
	argument: Option<String>,
) -> ControlFlow<()>
where
	P: ActionPorts,
{
	let request = state.build_plugin_command_request(plugin_id.clone(), command_id.clone(), argument);
	let title = request.command.title.clone();
	if let Err(source) = ports.enqueue_invoke_command(request) {
		let err = ActionHandlerError::PluginInvoke { source };
		error!("plugin command enqueue failed: {}", err);
		state
			.push_notification(NotificationLevel::Error, format!("plugin command enqueue failed: {}", command_id));
		state.workbench.status_bar.message = format!("plugin command failed: {}", plugin_id);
		return ControlFlow::Continue(());
	}
	state.workbench.status_bar.message = format!("plugin command running: {}", title);
	ControlFlow::Continue(())
}

pub(super) fn handle_plugin_runtime_action<P>(
	ports: &P,
	state: &mut RimState,
	action: PluginRuntimeAction,
) -> ControlFlow<()>
where
	P: ActionPorts,
{
	match action {
		PluginRuntimeAction::DiscoverRequested => {
			enqueue_plugin_discovery(ports, state);
		}
		PluginRuntimeAction::DiscoveryCompleted { result } => match result {
			Ok(discovery) => {
				state.set_plugin_registrations(discovery.plugins.clone());
				let mut registered_commands = 0usize;
				for plugin in &discovery.plugins {
					for command in &plugin.commands {
						let registration = PluginCommandRegistration {
							id:          format!("plugin.{}.{}", plugin.metadata.id, command.id),
							plugin_id:   plugin.metadata.id.clone(),
							command_id:  command.id.clone(),
							title:       command.title.clone(),
							category:    plugin.metadata.name.clone(),
							description: command.description.clone(),
							arg_kind:    CommandArgKind::RawTail,
						};
						match state.register_plugin_command(registration) {
							Ok(()) => registered_commands = registered_commands.saturating_add(1),
							Err(err) => {
								error!(
									"plugin command registration failed: plugin={} command={} error={}",
									plugin.metadata.id, command.id, err
								);
								state.push_notification(
									NotificationLevel::Warn,
									format!(
										"plugin command registration skipped: {}:{} ({})",
										plugin.metadata.id, command.id, err
									),
								);
							}
						}
					}
				}
				for failure in discovery.failures {
					state
						.push_notification(NotificationLevel::Warn, format!("plugin load failed: {}", failure.message));
				}
				state.workbench.status_bar.message = format!(
					"plugins ready: {} plugin(s), {} command(s)",
					state.plugin_registrations().len(),
					registered_commands
				);
			}
			Err(failure) => {
				error!("plugin discovery failed: {}", failure);
				state.push_notification(NotificationLevel::Error, failure.to_string());
				state.workbench.status_bar.message = "plugin discovery failed".to_string();
			}
		},
		PluginRuntimeAction::CommandCompleted { context, result } => match result {
			Ok(response) => {
				apply_plugin_response(ports, state, response)?;
				if state.workbench.status_bar.message.is_empty() {
					state.workbench.status_bar.message = format!("plugin command completed: {}", context.plugin_id);
				}
			}
			Err(failure) => {
				error!("plugin command failed: {}", failure);
				state.push_notification(NotificationLevel::Error, failure.to_string());
				state.workbench.status_bar.message = format!("plugin command failed: {}", context.plugin_id);
			}
		},
	}

	ControlFlow::Continue(())
}

fn apply_plugin_response<P>(
	ports: &P,
	state: &mut RimState,
	response: PluginCommandResponse,
) -> ControlFlow<()>
where
	P: ActionPorts,
{
	for effect in response.effects {
		match effect {
			PluginEffect::Notify(notification) => {
				state.push_notification(map_notification_level(notification.level), notification.message);
			}
			PluginEffect::ShowPanel(panel) => {
				state.show_plugin_panel(panel);
			}
			PluginEffect::RequestAction(action) => {
				apply_requested_action(ports, state, action)?;
			}
		}
	}
	ControlFlow::Continue(())
}

fn apply_requested_action<P>(
	ports: &P,
	state: &mut RimState,
	action: RequestedPluginAction,
) -> ControlFlow<()>
where
	P: ActionPorts,
{
	match action {
		RequestedPluginAction::OpenFile { path } => RimState::dispatch_internal(
			ports,
			state,
			AppAction::File(FileAction::OpenRequested { path: PathBuf::from(path) }),
		),
		RequestedPluginAction::InsertText { text } => {
			state.push_notification(
				NotificationLevel::Warn,
				format!("plugin insert_text is not implemented yet: {}", text),
			);
			ControlFlow::Continue(())
		}
		RequestedPluginAction::RunCommand { command_id, argument } => {
			let raw = match argument {
				Some(argument) if !argument.is_empty() => format!("{} {}", command_id, argument),
				_ => command_id,
			};
			let Some(resolved) = state.workbench.command_registry.resolve_command_input(raw.as_str()) else {
				state
					.push_notification(NotificationLevel::Error, format!("plugin requested unknown command: {}", raw));
				return ControlFlow::Continue(());
			};
			super::command_flow::execute_resolved_command(ports, state, resolved)
		}
	}
}

fn map_notification_level(level: PluginNotificationLevel) -> NotificationLevel {
	match level {
		PluginNotificationLevel::Info => NotificationLevel::Info,
		PluginNotificationLevel::Warn => NotificationLevel::Warn,
		PluginNotificationLevel::Error => NotificationLevel::Error,
	}
}
