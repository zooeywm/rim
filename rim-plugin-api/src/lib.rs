use core::marker::PhantomData;

pub mod bindings {
	wit_bindgen::generate!({
		path: "wit",
		world: "command-plugin",
		pub_export_macro: true,
	});
}

mod wit_types {
	pub use super::bindings::exports::rim::plugin::command_provider::{CommandUnavailableError, ExecutionFailedError, Guest, InsertTextAction, InvalidRequestError, OpenFileAction, PluginAction as WitPluginAction, PluginCapability as WitPluginCapability, PluginCommandError as WitPluginCommandError, PluginCommandMetadata as WitPluginCommandMetadata, PluginCommandParamKind as WitPluginCommandParamKind, PluginCommandParamSpec as WitPluginCommandParamSpec, PluginCommandRequest as WitPluginCommandRequest, PluginCommandResponse as WitPluginCommandResponse, PluginDescriptor as WitPluginDescriptor, PluginEffect as WitPluginEffect, PluginMetadata as WitPluginMetadata, PluginNotification as WitPluginNotification, PluginNotificationLevel as WitPluginNotificationLevel, PluginPanel as WitPluginPanel, RunCommandAction};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginCapability {
	CommandProvider,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMetadata {
	pub id:                    String,
	pub name:                  String,
	pub version:               String,
	pub declared_capabilities: Vec<PluginCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommandMetadata {
	pub id:          String,
	pub name:        String,
	pub description: String,
	pub params:      Vec<PluginCommandParamSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginCommandParamKind {
	String,
	File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommandParamSpec {
	pub name:     String,
	pub kind:     PluginCommandParamKind,
	pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommandRequest {
	pub command_id:     String,
	pub argument:       Option<String>,
	pub workspace_root: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginNotificationLevel {
	Info,
	Warn,
	Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginNotification {
	pub level:   PluginNotificationLevel,
	pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPanel {
	pub title:  String,
	pub lines:  Vec<String>,
	pub footer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginAction {
	OpenFile { path: String },
	InsertText { text: String },
	RunCommand { command_id: String, argument: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginEffect {
	Notify(PluginNotification),
	ShowPanel(PluginPanel),
	RequestAction(PluginAction),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PluginCommandResponse {
	pub effects: Vec<PluginEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginCommandError {
	InvalidRequest { message: String },
	CommandUnavailable { command_id: String },
	ExecutionFailed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDescriptor {
	pub metadata: PluginMetadata,
	pub commands: Vec<PluginCommandMetadata>,
}

pub type PluginCommandOutcome = Result<PluginCommandResponse, PluginCommandError>;

pub trait CommandProviderPlugin {
	fn descriptor() -> PluginDescriptor;
	fn run_command(request: PluginCommandRequest) -> PluginCommandOutcome;
}

#[doc(hidden)]
pub struct ExportedCommandProvider<T>(PhantomData<T>);

impl<T> wit_types::Guest for ExportedCommandProvider<T>
where T: CommandProviderPlugin
{
	fn describe() -> wit_types::WitPluginDescriptor { into_wit_descriptor(T::descriptor()) }

	fn run_command(
		request: wit_types::WitPluginCommandRequest,
	) -> Result<wit_types::WitPluginCommandResponse, wit_types::WitPluginCommandError> {
		match T::run_command(from_wit_command_request(request)) {
			Ok(response) => Ok(into_wit_command_response(response)),
			Err(err) => Err(into_wit_command_error(err)),
		}
	}
}

fn into_wit_descriptor(descriptor: PluginDescriptor) -> wit_types::WitPluginDescriptor {
	wit_types::WitPluginDescriptor {
		metadata: into_wit_plugin_metadata(descriptor.metadata),
		commands: descriptor.commands.into_iter().map(into_wit_command_metadata).collect(),
	}
}

fn into_wit_plugin_metadata(metadata: PluginMetadata) -> wit_types::WitPluginMetadata {
	wit_types::WitPluginMetadata {
		id:                    metadata.id,
		name:                  metadata.name,
		version:               metadata.version,
		declared_capabilities: metadata
			.declared_capabilities
			.into_iter()
			.map(into_wit_plugin_capability)
			.collect(),
	}
}

fn into_wit_plugin_capability(capability: PluginCapability) -> wit_types::WitPluginCapability {
	match capability {
		PluginCapability::CommandProvider => wit_types::WitPluginCapability::CommandProvider,
	}
}

fn into_wit_command_metadata(metadata: PluginCommandMetadata) -> wit_types::WitPluginCommandMetadata {
	wit_types::WitPluginCommandMetadata {
		id:          metadata.id,
		name:        metadata.name,
		description: metadata.description,
		params:      metadata.params.into_iter().map(into_wit_command_param_spec).collect(),
	}
}

fn into_wit_command_param_spec(param: PluginCommandParamSpec) -> wit_types::WitPluginCommandParamSpec {
	wit_types::WitPluginCommandParamSpec {
		name:     param.name,
		kind:     into_wit_command_param_kind(param.kind),
		optional: param.optional,
	}
}

fn into_wit_command_param_kind(kind: PluginCommandParamKind) -> wit_types::WitPluginCommandParamKind {
	match kind {
		PluginCommandParamKind::String => wit_types::WitPluginCommandParamKind::Text,
		PluginCommandParamKind::File => wit_types::WitPluginCommandParamKind::File,
	}
}

fn into_wit_command_response(response: PluginCommandResponse) -> wit_types::WitPluginCommandResponse {
	wit_types::WitPluginCommandResponse {
		effects: response.effects.into_iter().map(into_wit_plugin_effect).collect(),
	}
}

fn into_wit_plugin_effect(effect: PluginEffect) -> wit_types::WitPluginEffect {
	match effect {
		PluginEffect::Notify(notification) => {
			wit_types::WitPluginEffect::Notify(into_wit_notification(notification))
		}
		PluginEffect::ShowPanel(panel) => wit_types::WitPluginEffect::ShowPanel(into_wit_panel(panel)),
		PluginEffect::RequestAction(action) => wit_types::WitPluginEffect::RequestAction(into_wit_action(action)),
	}
}

fn into_wit_notification(notification: PluginNotification) -> wit_types::WitPluginNotification {
	wit_types::WitPluginNotification {
		level:   into_wit_notification_level(notification.level),
		message: notification.message,
	}
}

fn into_wit_notification_level(level: PluginNotificationLevel) -> wit_types::WitPluginNotificationLevel {
	match level {
		PluginNotificationLevel::Info => wit_types::WitPluginNotificationLevel::Info,
		PluginNotificationLevel::Warn => wit_types::WitPluginNotificationLevel::Warn,
		PluginNotificationLevel::Error => wit_types::WitPluginNotificationLevel::Error,
	}
}

fn into_wit_panel(panel: PluginPanel) -> wit_types::WitPluginPanel {
	wit_types::WitPluginPanel { title: panel.title, lines: panel.lines, footer: panel.footer }
}

fn into_wit_action(action: PluginAction) -> wit_types::WitPluginAction {
	match action {
		PluginAction::OpenFile { path } => {
			wit_types::WitPluginAction::OpenFile(wit_types::OpenFileAction { path })
		}
		PluginAction::InsertText { text } => {
			wit_types::WitPluginAction::InsertText(wit_types::InsertTextAction { text })
		}
		PluginAction::RunCommand { command_id, argument } => {
			wit_types::WitPluginAction::RunCommand(wit_types::RunCommandAction { command_id, argument })
		}
	}
}

fn into_wit_command_error(error: PluginCommandError) -> wit_types::WitPluginCommandError {
	match error {
		PluginCommandError::InvalidRequest { message } => {
			wit_types::WitPluginCommandError::InvalidRequest(wit_types::InvalidRequestError { message })
		}
		PluginCommandError::CommandUnavailable { command_id } => {
			wit_types::WitPluginCommandError::CommandUnavailable(wit_types::CommandUnavailableError { command_id })
		}
		PluginCommandError::ExecutionFailed { message } => {
			wit_types::WitPluginCommandError::ExecutionFailed(wit_types::ExecutionFailedError { message })
		}
	}
}

fn from_wit_command_request(request: wit_types::WitPluginCommandRequest) -> PluginCommandRequest {
	PluginCommandRequest {
		command_id:     request.command_id,
		argument:       request.argument,
		workspace_root: request.workspace_root,
	}
}

#[macro_export]
macro_rules! export_plugin {
	($plugin:ty) => {
		type __RimExportedPlugin = $crate::ExportedCommandProvider<$plugin>;
		$crate::bindings::export!(__RimExportedPlugin with_types_in $crate::bindings);
	};
}

pub mod prelude {
	pub use crate::{CommandProviderPlugin, PluginAction, PluginCapability, PluginCommandError, PluginCommandMetadata, PluginCommandOutcome, PluginCommandParamKind, PluginCommandParamSpec, PluginCommandRequest, PluginCommandResponse, PluginDescriptor, PluginEffect, PluginMetadata, PluginNotification, PluginNotificationLevel, PluginPanel, export_plugin};
}
