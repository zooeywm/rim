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
	pub abi_version:           u32,
	pub declared_capabilities: Vec<PluginCapability>,
}

/// Declared command surfaced by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginCommandMetadata {
	pub id:          String,
	pub title:       String,
	pub description: String,
}

/// Minimal invocation context supplied by the runtime for tracing and budget
/// enforcement.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginContext {
	pub plugin_id:         String,
	pub invocation_id:     u64,
	pub time_budget_ms:    u64,
	pub issued_at_unix_ms: u64,
}

/// Read-only snapshot of the active buffer exposed to plugin commands.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginBufferSnapshot {
	pub path:              Option<String>,
	pub is_dirty:          bool,
	pub cursor_row:        u16,
	pub cursor_col:        u16,
	pub current_line_text: String,
}

/// Read-only selection snapshot for future visual-aware commands.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginSelectionSnapshot {
	pub text:         String,
	pub is_multiline: bool,
}

/// Request contract for the v1 CommandProvider capability.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginCommandRequest {
	pub context:        PluginContext,
	pub command:        PluginCommandMetadata,
	pub argument_tail:  Option<String>,
	pub workspace_root: String,
	pub active_buffer:  Option<PluginBufferSnapshot>,
	pub selection:      Option<PluginSelectionSnapshot>,
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
