use rim_ports::{PluginCommandParamKind, PluginCommandRequest, PluginPanel, PluginRegistration, PluginResolvedParam};

use super::{FloatingWindowLine, FloatingWindowPlacement, FloatingWindowState, OverlayState, RimState};
use crate::command::{CommandArgKind, CommandParamSpec, CommandValue, PluginCommandRegistration, ResolvedParams};

impl RimState {
	pub fn set_plugin_registrations(&mut self, plugins: Vec<PluginRegistration>) {
		self.workbench.plugins = plugins;
	}

	pub fn plugin_registrations(&self) -> &[PluginRegistration] { self.workbench.plugins.as_slice() }

	pub fn plugin_command_registrations(&self) -> Vec<PluginCommandRegistration> {
		self
			.workbench
			.plugins
			.iter()
			.flat_map(|plugin| {
				plugin.commands.iter().map(|command| PluginCommandRegistration {
					id:           format!("plugin.{}.{}", plugin.metadata.id, command.id),
					default_name: command.name.clone(),
					plugin_id:    plugin.metadata.id.clone(),
					command_id:   command.id.clone(),
					category:     plugin.metadata.name.clone(),
					description:  command.description.clone(),
					params:       command.params.iter().map(plugin_param_to_command_param).collect(),
				})
			})
			.collect()
	}

	pub fn rebuild_plugin_command_registry_entries(&mut self) {
		let registrations = self.plugin_command_registrations();
		for registration in registrations {
			let _ = self.register_plugin_command(registration);
		}
	}

	pub fn build_plugin_command_request(
		&mut self,
		plugin_id: String,
		command_id: String,
		params: &ResolvedParams,
	) -> PluginCommandRequest {
		PluginCommandRequest {
			command_id:     format!("plugin.{}.{}", plugin_id, command_id),
			argument:       params.as_slice().first().map(|param| param.value.as_str().to_string()),
			params:         params.as_slice().iter().map(command_param_to_plugin_param).collect(),
			workspace_root: self.workbench.workspace_root.display().to_string(),
		}
	}

	pub fn show_plugin_panel(&mut self, panel: PluginPanel) {
		let lines = panel
			.lines
			.into_iter()
			.map(|line| FloatingWindowLine { key: String::new(), summary: line, is_prefix: false })
			.collect::<Vec<_>>();
		let height = lines.len().saturating_add(4).min(self.workbench.key_hints_max_height as usize) as u16;
		self.workbench.overlay = Some(OverlayState::FloatingWindow(FloatingWindowState {
			title: panel.title,
			subtitle: Some("Plugin output".to_string()),
			footer: panel.footer.or_else(|| Some("Esc close".to_string())),
			placement: FloatingWindowPlacement::BottomRight {
				width:         self.workbench.key_hints_width,
				height:        height.max(4),
				margin_right:  1,
				margin_bottom: 1,
			},
			lines,
			scroll: 0,
		}));
	}
}

fn command_param_to_plugin_param(param: &crate::command::ResolvedParam) -> PluginResolvedParam {
	PluginResolvedParam {
		name:  param.name.clone(),
		kind:  match param.kind {
			CommandArgKind::Text => PluginCommandParamKind::Text,
			CommandArgKind::File => PluginCommandParamKind::File,
		},
		value: match &param.value {
			CommandValue::Text(value) | CommandValue::File(value) => value.clone(),
		},
	}
}

fn plugin_param_to_command_param(param: &rim_ports::PluginCommandParamSpec) -> CommandParamSpec {
	CommandParamSpec {
		name:     param.name.clone(),
		kind:     match param.kind {
			PluginCommandParamKind::Text => CommandArgKind::Text,
			PluginCommandParamKind::File => CommandArgKind::File,
		},
		optional: param.optional,
	}
}
