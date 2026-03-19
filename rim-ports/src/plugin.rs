use thiserror::Error;

/// Declared plugin capability supported by the host boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
	CommandProvider,
}

/// Plugin identity and compatibility metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginMetadata {
	pub id:                    String,
	pub name:                  String,
	pub version:               String,
	pub declared_capabilities: Vec<PluginCapability>,
}

/// Declared command surfaced by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginCommandMetadata {
	pub id:          String,
	pub name:        String,
	pub description: String,
	pub params:      Vec<PluginCommandParamSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCommandParamKind {
	Text,
	File,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginCommandParamSpec {
	pub name:     String,
	pub kind:     PluginCommandParamKind,
	pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginResolvedParam {
	pub name:  String,
	pub kind:  PluginCommandParamKind,
	pub value: String,
}

/// Request contract for the v1 CommandProvider capability.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginCommandRequest {
	pub command_id:     String,
	pub argument:       Option<String>,
	pub params:         Vec<PluginResolvedParam>,
	pub workspace_root: String,
}

/// Notification effect emitted by a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginNotificationLevel {
	Info,
	Warn,
	Error,
}

/// Notification payload emitted by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginNotification {
	pub level:   PluginNotificationLevel,
	pub message: String,
}

/// Simple panel payload emitted by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginPanel {
	pub title:  String,
	pub lines:  Vec<String>,
	pub footer: Option<String>,
}

/// Explicit application-side action requested by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PluginAction {
	OpenFile { path: String },
	PickFile,
	InsertText { text: String },
	RunCommand { command_id: String, argument: Option<String> },
}

/// Bounded effect surface returned by a plugin invocation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PluginEffect {
	Notify(PluginNotification),
	ShowPanel(PluginPanel),
	RequestAction(PluginAction),
}

/// Response contract for the v1 CommandProvider capability.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct PluginCommandResponse {
	pub effects: Vec<PluginEffect>,
}

/// Plugin-authored command error returned from the guest boundary.
#[derive(Debug, Clone, PartialEq, Eq, Error, serde::Serialize, serde::Deserialize)]
pub enum PluginCommandError {
	#[error("plugin command rejected the request: {message}")]
	InvalidRequest { message: String },
	#[error("plugin command is unavailable: {command_id}")]
	CommandUnavailable { command_id: String },
	#[error("plugin command failed: {message}")]
	ExecutionFailed { message: String },
}

/// Discovered plugin registration used by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRegistration {
	pub metadata: PluginMetadata,
	pub commands: Vec<PluginCommandMetadata>,
}

/// Partial discovery failure reported by the host runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLoadFailure {
	pub plugin_id: Option<String>,
	pub location:  String,
	pub message:   String,
}

/// Batched discovery result returned by the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PluginDiscoveryResult {
	pub plugins:  Vec<PluginRegistration>,
	pub failures: Vec<PluginLoadFailure>,
}

/// Invocation failure isolated to the host/runtime layer.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PluginRuntimeFailure {
	#[error("plugin discovery failed: {message}")]
	Discovery { message: String },
	#[error("plugin invocation failed for {plugin_id}:{command_id}: {message}")]
	Invocation { plugin_id: String, command_id: String, message: String },
}

/// Typed failure returned for a command invocation callback.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PluginInvocationError {
	#[error("{0}")]
	Guest(PluginCommandError),
	#[error("{0}")]
	Runtime(PluginRuntimeFailure),
}

/// Runtime-facing error contract for queueing plugin work.
#[derive(Debug, Error)]
pub enum PluginRuntimeError {
	#[error("plugin runtime request channel disconnected while enqueueing {operation}")]
	RequestChannelDisconnected { operation: &'static str },
}

/// Outbound runtime port for discovery and command execution.
pub trait PluginRuntime {
	fn enqueue_discover_plugins(&self, workspace_root: String) -> Result<(), PluginRuntimeError>;
	fn enqueue_invoke_command(&self, request: PluginCommandRequest) -> Result<(), PluginRuntimeError>;
}
