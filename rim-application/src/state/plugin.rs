use std::time::{SystemTime, UNIX_EPOCH};

use rim_ports::{PluginBufferSnapshot, PluginCommandMetadata, PluginCommandRequest, PluginContext, PluginPanel, PluginRegistration};

use super::{FloatingWindowLine, FloatingWindowPlacement, FloatingWindowState, OverlayState, RimState, rope_line_without_newline};

const DEFAULT_PLUGIN_TIME_BUDGET_MS: u64 = 250;

impl RimState {
	pub fn set_plugin_registrations(&mut self, plugins: Vec<PluginRegistration>) {
		self.workbench.plugins = plugins;
	}

	pub fn plugin_registrations(&self) -> &[PluginRegistration] { self.workbench.plugins.as_slice() }

	pub fn build_plugin_command_request(
		&mut self,
		plugin_id: String,
		command_id: String,
		argument_tail: Option<String>,
	) -> PluginCommandRequest {
		let command = self
			.workbench
			.plugins
			.iter()
			.find(|plugin| plugin.metadata.id == plugin_id)
			.and_then(|plugin| plugin.commands.iter().find(|command| command.id == command_id))
			.cloned()
			.unwrap_or_else(|| PluginCommandMetadata {
				id:          command_id.clone(),
				title:       command_id.clone(),
				description: format!("Plugin command '{}'", command_id),
			});

		let invocation_id = self.workbench.next_plugin_invocation_id;
		self.workbench.next_plugin_invocation_id = self.workbench.next_plugin_invocation_id.saturating_add(1);
		let issued_at_unix_ms = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map(|duration| duration.as_millis() as u64)
			.unwrap_or_default();

		PluginCommandRequest {
			context: PluginContext {
				plugin_id,
				invocation_id,
				time_budget_ms: DEFAULT_PLUGIN_TIME_BUDGET_MS,
				issued_at_unix_ms,
			},
			command,
			argument_tail,
			workspace_root: self.workbench.workspace_root.display().to_string(),
			active_buffer: self.active_plugin_buffer_snapshot(),
			selection: None,
		}
	}

	fn active_plugin_buffer_snapshot(&self) -> Option<PluginBufferSnapshot> {
		let buffer_id = self.active_buffer_id()?;
		let buffer = self.buffers.get(buffer_id)?;
		let cursor = self.active_cursor();
		let current_line_text =
			rope_line_without_newline(&buffer.text, cursor.row.saturating_sub(1) as usize).unwrap_or_default();
		Some(PluginBufferSnapshot {
			path: buffer.path.as_ref().map(|path| path.display().to_string()),
			is_dirty: buffer.dirty,
			cursor_row: cursor.row,
			cursor_col: cursor.col,
			current_line_text,
		})
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
