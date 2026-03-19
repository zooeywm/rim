#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::process::Command;
use std::{collections::{HashMap, HashSet}, fs, path::Path, sync::Mutex, thread};

use rim_application::action::{AppAction, PluginRuntimeAction};
use rim_paths::user_config_root;
use rim_ports::{PluginAction, PluginCapability, PluginCommandError, PluginCommandMetadata, PluginCommandParamKind, PluginCommandParamSpec, PluginCommandRequest, PluginCommandResponse, PluginDiscoveryResult, PluginEffect, PluginInvocationError, PluginLoadFailure, PluginMetadata, PluginNotification, PluginNotificationLevel, PluginPanel, PluginRegistration, PluginResolvedParam, PluginRuntime, PluginRuntimeError, PluginRuntimeFailure};
use tracing::error;
use wasmtime::{Config, Engine, Store, component::{Component, Linker}};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

mod component_bindings {
	wasmtime::component::bindgen!({
		path: "../rim-plugin-api/wit",
		world: "command-plugin",
	});
}

use component_bindings::exports::rim::plugin::command_provider as wit;

#[derive(dep_inj::DepInj)]
#[target(PluginRuntimeImpl)]
pub struct PluginHostState {
	request_tx:   flume::Sender<PluginHostRequest>,
	request_rx:   flume::Receiver<PluginHostRequest>,
	app_event_tx: flume::Sender<AppAction>,
	worker_join:  Mutex<Option<thread::JoinHandle<()>>>,
}

impl AsRef<PluginHostState> for PluginHostState {
	fn as_ref(&self) -> &PluginHostState { self }
}

impl PluginHostState {
	pub fn new(app_event_tx: flume::Sender<AppAction>) -> Self {
		let (request_tx, request_rx) = flume::unbounded();
		Self { request_tx, request_rx, app_event_tx, worker_join: Mutex::new(None) }
	}

	pub fn start(&self) {
		let mut worker_guard = self.worker_join.lock().expect("plugin worker lock should not be poisoned");
		if worker_guard.is_some() {
			return;
		}
		let request_rx = self.request_rx.clone();
		let app_event_tx = self.app_event_tx.clone();
		*worker_guard = Some(thread::spawn(move || run_worker(request_rx, app_event_tx)));
	}
}

impl<Deps> PluginRuntime for PluginRuntimeImpl<Deps>
where Deps: AsRef<PluginHostState>
{
	fn enqueue_discover_plugins(&self, workspace_root: String) -> Result<(), PluginRuntimeError> {
		send_request(
			&self.request_tx,
			PluginHostRequest::Discover { workspace_root },
			"enqueue_discover_plugins",
			"discover_plugins",
		)
	}

	fn enqueue_invoke_command(&self, request: PluginCommandRequest) -> Result<(), PluginRuntimeError> {
		send_request(
			&self.request_tx,
			PluginHostRequest::InvokeCommand { request },
			"enqueue_invoke_command",
			"invoke_command",
		)
	}
}

fn send_request(
	request_tx: &flume::Sender<PluginHostRequest>,
	request: PluginHostRequest,
	log_name: &'static str,
	operation: &'static str,
) -> Result<(), PluginRuntimeError> {
	request_tx.send(request).map_err(|err| {
		error!("{} failed: plugin worker channel is disconnected: {}", log_name, err);
		PluginRuntimeError::RequestChannelDisconnected { operation }
	})
}

#[derive(Debug)]
enum PluginHostRequest {
	Discover { workspace_root: String },
	InvokeCommand { request: PluginCommandRequest },
}

struct LoadedPlugin {
	registration: PluginRegistration,
	component:    Component,
}

struct DiscoverySnapshot {
	plugins:  Vec<LoadedPlugin>,
	failures: Vec<PluginLoadFailure>,
}

struct PluginStoreState {
	table: ResourceTable,
	wasi:  WasiCtx,
}

impl PluginStoreState {
	fn new() -> Self { Self { table: ResourceTable::new(), wasi: WasiCtxBuilder::new().build() } }
}

impl WasiView for PluginStoreState {
	fn ctx(&mut self) -> WasiCtxView<'_> { WasiCtxView { ctx: &mut self.wasi, table: &mut self.table } }
}

fn run_worker(request_rx: flume::Receiver<PluginHostRequest>, app_event_tx: flume::Sender<AppAction>) {
	let engine = build_engine();
	let mut loaded_plugins = HashMap::<String, LoadedPlugin>::new();

	while let Ok(request) = request_rx.recv() {
		match request {
			PluginHostRequest::Discover { workspace_root } => {
				let result = discover_plugins(&engine, workspace_root.as_str());
				match result {
					Ok(discovery) => {
						let action_result = PluginDiscoveryResult {
							plugins:  discovery.plugins.iter().map(|plugin| plugin.registration.clone()).collect(),
							failures: discovery.failures.clone(),
						};
						loaded_plugins.clear();
						for plugin in discovery.plugins {
							loaded_plugins.insert(plugin.registration.metadata.id.clone(), plugin);
						}
						if app_event_tx
							.send(AppAction::Plugin(PluginRuntimeAction::DiscoveryCompleted { result: Ok(action_result) }))
							.is_err()
						{
							break;
						}
					}
					Err(failure) => {
						if app_event_tx
							.send(AppAction::Plugin(PluginRuntimeAction::DiscoveryCompleted { result: Err(failure) }))
							.is_err()
						{
							break;
						}
					}
				}
			}
			PluginHostRequest::InvokeCommand { request } => {
				let command_id = request.command_id.clone();
				let result = invoke_command(&loaded_plugins, &request);
				if app_event_tx
					.send(AppAction::Plugin(PluginRuntimeAction::CommandCompleted { command_id, result }))
					.is_err()
				{
					break;
				}
			}
		}
	}
}

fn build_engine() -> Engine {
	let mut config = Config::new();
	config.wasm_component_model(true);
	Engine::new(&config).expect("wasmtime engine should enable the component model")
}

fn build_linker(engine: &Engine) -> Result<Linker<PluginStoreState>, String> {
	let mut linker = Linker::new(engine);
	wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
		.map_err(|err| format!("link preview2 WASI imports failed: {}", err))?;
	Ok(linker)
}

fn build_store(engine: &Engine) -> Store<PluginStoreState> { Store::new(engine, PluginStoreState::new()) }

fn discover_plugins(
	engine: &Engine,
	_workspace_root: &str,
) -> Result<DiscoverySnapshot, PluginRuntimeFailure> {
	let plugins_root = user_config_root().join("plugins");
	if !plugins_root.exists() {
		return Ok(DiscoverySnapshot { plugins: Vec::new(), failures: Vec::new() });
	}

	let mut plugins = Vec::new();
	let mut failures = Vec::new();
	let entries = fs::read_dir(&plugins_root).map_err(|err| PluginRuntimeFailure::Discovery {
		message: format!("read plugin directory failed: {}", err),
	})?;

	for entry in entries {
		let entry = match entry {
			Ok(entry) => entry,
			Err(err) => {
				failures.push(PluginLoadFailure {
					plugin_id: None,
					location:  plugins_root.display().to_string(),
					message:   format!("enumerate plugin entry failed: {}", err),
				});
				continue;
			}
		};
		let path = entry.path();
		if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
			continue;
		}
		match load_plugin(engine, path.as_path()) {
			Ok(plugin) => {
				if plugins
					.iter()
					.any(|loaded: &LoadedPlugin| loaded.registration.metadata.id == plugin.registration.metadata.id)
				{
					failures.push(PluginLoadFailure {
						plugin_id: Some(plugin.registration.metadata.id.clone()),
						location:  path.display().to_string(),
						message:   "duplicate plugin id is already loaded from another wasm file".to_string(),
					});
					continue;
				}
				plugins.push(plugin);
			}
			Err(failure) => failures.push(failure),
		}
	}

	Ok(DiscoverySnapshot { plugins, failures })
}

fn load_plugin(engine: &Engine, entry_path: &Path) -> Result<LoadedPlugin, PluginLoadFailure> {
	if entry_path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
		return Err(PluginLoadFailure {
			plugin_id: None,
			location:  entry_path.display().to_string(),
			message:   "plugin entry must be a .wasm file".to_string(),
		});
	}
	if !entry_path.is_file() {
		return Err(PluginLoadFailure {
			plugin_id: None,
			location:  entry_path.display().to_string(),
			message:   "plugin wasm file is missing".to_string(),
		});
	}

	let component = Component::from_file(engine, entry_path).map_err(|err| PluginLoadFailure {
		plugin_id: None,
		location:  entry_path.display().to_string(),
		message:   format!("compile plugin component failed: {}", err),
	})?;
	let descriptor = read_descriptor(&component).map_err(|message| PluginLoadFailure {
		plugin_id: None,
		location: entry_path.display().to_string(),
		message,
	})?;

	validate_descriptor(&descriptor, entry_path)?;
	let registration = registration_from_descriptor(descriptor);

	Ok(LoadedPlugin { registration, component })
}

fn validate_descriptor(
	descriptor: &wit::PluginDescriptor,
	entry_path: &Path,
) -> Result<(), PluginLoadFailure> {
	if descriptor.metadata.id.trim().is_empty() {
		return Err(PluginLoadFailure {
			plugin_id: None,
			location:  entry_path.display().to_string(),
			message:   "plugin descriptor id must not be empty".to_string(),
		});
	}
	if !descriptor
		.metadata
		.declared_capabilities
		.iter()
		.any(|capability| matches!(capability, wit::PluginCapability::CommandProvider))
	{
		return Err(PluginLoadFailure {
			plugin_id: Some(descriptor.metadata.id.clone()),
			location:  entry_path.display().to_string(),
			message:   "plugin does not declare the command-provider capability".to_string(),
		});
	}

	let mut seen_commands = HashSet::new();
	for command in &descriptor.commands {
		if !seen_commands.insert(command.id.clone()) {
			return Err(PluginLoadFailure {
				plugin_id: Some(descriptor.metadata.id.clone()),
				location:  entry_path.display().to_string(),
				message:   format!("duplicate plugin command id '{}'", command.id),
			});
		}
	}
	Ok(())
}

fn read_descriptor(component: &Component) -> Result<wit::PluginDescriptor, String> {
	let linker = build_linker(component.engine())?;
	let mut store = build_store(component.engine());
	let bindings = component_bindings::CommandPlugin::instantiate(&mut store, component, &linker)
		.map_err(|err| format!("instantiate plugin component failed: {}", err))?;
	bindings
		.rim_plugin_command_provider()
		.call_describe(&mut store)
		.map_err(|err| format!("call command-provider.describe failed: {}", err))
}

fn registration_from_descriptor(descriptor: wit::PluginDescriptor) -> PluginRegistration {
	PluginRegistration {
		metadata: plugin_metadata_from_wit(descriptor.metadata),
		commands: descriptor.commands.into_iter().map(plugin_command_metadata_from_wit).collect(),
	}
}

fn invoke_command(
	loaded_plugins: &HashMap<String, LoadedPlugin>,
	request: &PluginCommandRequest,
) -> Result<PluginCommandResponse, PluginInvocationError> {
	let (plugin_id, local_command_id) = parse_plugin_command_id(&request.command_id).ok_or_else(|| {
		PluginInvocationError::Runtime(PluginRuntimeFailure::Invocation {
			plugin_id:  "<unknown>".to_string(),
			command_id: request.command_id.clone(),
			message:    "plugin command id must be formatted as plugin.<plugin-id>.<command-id>".to_string(),
		})
	})?;
	let Some(plugin) = loaded_plugins.get(plugin_id) else {
		return Err(PluginInvocationError::Runtime(PluginRuntimeFailure::Invocation {
			plugin_id:  plugin_id.to_string(),
			command_id: local_command_id.to_string(),
			message:    "plugin is not loaded".to_string(),
		}));
	};
	if !plugin.registration.commands.iter().any(|command| command.id == local_command_id) {
		return Err(PluginInvocationError::Runtime(PluginRuntimeFailure::Invocation {
			plugin_id:  plugin_id.to_string(),
			command_id: local_command_id.to_string(),
			message:    "plugin command is not declared by the descriptor".to_string(),
		}));
	}

	match call_run_command(&plugin.component, plugin_id, local_command_id, request)
		.map_err(PluginInvocationError::Runtime)?
	{
		Ok(response) => Ok(plugin_command_response_from_wit(response)),
		Err(err) => Err(PluginInvocationError::Guest(plugin_command_error_from_wit(err))),
	}
}

fn call_run_command(
	component: &Component,
	plugin_id: &str,
	local_command_id: &str,
	request: &PluginCommandRequest,
) -> Result<Result<wit::PluginCommandResponse, wit::PluginCommandError>, PluginRuntimeFailure> {
	let linker = build_linker(component.engine()).map_err(|message| PluginRuntimeFailure::Invocation {
		plugin_id: plugin_id.to_string(),
		command_id: local_command_id.to_string(),
		message,
	})?;
	let mut store = build_store(component.engine());
	let bindings =
		component_bindings::CommandPlugin::instantiate(&mut store, component, &linker).map_err(|err| {
			PluginRuntimeFailure::Invocation {
				plugin_id:  plugin_id.to_string(),
				command_id: local_command_id.to_string(),
				message:    format!("instantiate plugin component failed: {}", err),
			}
		})?;
	let request = plugin_command_request_to_wit(local_command_id, request);
	bindings.rim_plugin_command_provider().call_run_command(&mut store, &request).map_err(|err| {
		PluginRuntimeFailure::Invocation {
			plugin_id:  plugin_id.to_string(),
			command_id: local_command_id.to_string(),
			message:    format!("call command-provider.run-command failed: {}", err),
		}
	})
}

fn plugin_command_request_to_wit(
	local_command_id: &str,
	request: &PluginCommandRequest,
) -> wit::PluginCommandRequest {
	wit::PluginCommandRequest {
		command_id:     local_command_id.to_string(),
		argument:       request.argument.clone(),
		params:         request.params.iter().cloned().map(plugin_resolved_param_to_wit).collect(),
		workspace_root: request.workspace_root.clone(),
	}
}

fn plugin_resolved_param_to_wit(param: PluginResolvedParam) -> wit::PluginResolvedParam {
	wit::PluginResolvedParam {
		name:  param.name,
		kind:  plugin_command_param_kind_to_wit(param.kind),
		value: param.value,
	}
}

fn plugin_command_param_kind_to_wit(kind: PluginCommandParamKind) -> wit::PluginCommandParamKind {
	match kind {
		PluginCommandParamKind::Text => wit::PluginCommandParamKind::Text,
		PluginCommandParamKind::File => wit::PluginCommandParamKind::File,
	}
}

fn parse_plugin_command_id(command_id: &str) -> Option<(&str, &str)> {
	let rest = command_id.strip_prefix("plugin.")?;
	let (plugin_id, local_command_id) = rest.split_once('.')?;
	(!plugin_id.is_empty() && !local_command_id.is_empty()).then_some((plugin_id, local_command_id))
}

fn plugin_metadata_from_wit(metadata: wit::PluginMetadata) -> PluginMetadata {
	PluginMetadata {
		id:                    metadata.id,
		name:                  metadata.name,
		version:               metadata.version,
		declared_capabilities: metadata
			.declared_capabilities
			.into_iter()
			.map(plugin_capability_from_wit)
			.collect(),
	}
}

fn plugin_capability_from_wit(capability: wit::PluginCapability) -> PluginCapability {
	match capability {
		wit::PluginCapability::CommandProvider => PluginCapability::CommandProvider,
	}
}

fn plugin_command_metadata_from_wit(metadata: wit::PluginCommandMetadata) -> PluginCommandMetadata {
	PluginCommandMetadata {
		id:          metadata.id,
		name:        metadata.name,
		description: metadata.description,
		params:      metadata.params.into_iter().map(plugin_command_param_spec_from_wit).collect(),
	}
}

fn plugin_command_param_spec_from_wit(param: wit::PluginCommandParamSpec) -> PluginCommandParamSpec {
	PluginCommandParamSpec {
		name:     param.name,
		kind:     plugin_command_param_kind_from_wit(param.kind),
		optional: param.optional,
	}
}

fn plugin_command_param_kind_from_wit(kind: wit::PluginCommandParamKind) -> PluginCommandParamKind {
	match kind {
		wit::PluginCommandParamKind::Text => PluginCommandParamKind::Text,
		wit::PluginCommandParamKind::File => PluginCommandParamKind::File,
	}
}

fn plugin_command_response_from_wit(response: wit::PluginCommandResponse) -> PluginCommandResponse {
	PluginCommandResponse { effects: response.effects.into_iter().map(plugin_effect_from_wit).collect() }
}

fn plugin_effect_from_wit(effect: wit::PluginEffect) -> PluginEffect {
	match effect {
		wit::PluginEffect::Notify(notification) => {
			PluginEffect::Notify(plugin_notification_from_wit(notification))
		}
		wit::PluginEffect::ShowPanel(panel) => PluginEffect::ShowPanel(plugin_panel_from_wit(panel)),
		wit::PluginEffect::RequestAction(action) => PluginEffect::RequestAction(plugin_action_from_wit(action)),
	}
}

fn plugin_notification_from_wit(notification: wit::PluginNotification) -> PluginNotification {
	PluginNotification {
		level:   plugin_notification_level_from_wit(notification.level),
		message: notification.message,
	}
}

fn plugin_notification_level_from_wit(level: wit::PluginNotificationLevel) -> PluginNotificationLevel {
	match level {
		wit::PluginNotificationLevel::Info => PluginNotificationLevel::Info,
		wit::PluginNotificationLevel::Warn => PluginNotificationLevel::Warn,
		wit::PluginNotificationLevel::Error => PluginNotificationLevel::Error,
	}
}

fn plugin_panel_from_wit(panel: wit::PluginPanel) -> PluginPanel {
	PluginPanel { title: panel.title, lines: panel.lines, footer: panel.footer }
}

fn plugin_action_from_wit(action: wit::PluginAction) -> PluginAction {
	match action {
		wit::PluginAction::OpenFile(payload) => PluginAction::OpenFile { path: payload.path },
		wit::PluginAction::PickFile(action) => PluginAction::PickFile {
			command:                action.command,
			chooser_file_arg_index: action.chooser_file_arg_index,
		},
		wit::PluginAction::InsertText(payload) => PluginAction::InsertText { text: payload.text },
		wit::PluginAction::RunCommand(payload) => {
			PluginAction::RunCommand { command_id: payload.command_id, argument: payload.argument }
		}
	}
}

fn plugin_command_error_from_wit(error: wit::PluginCommandError) -> PluginCommandError {
	match error {
		wit::PluginCommandError::InvalidRequest(payload) => {
			PluginCommandError::InvalidRequest { message: payload.message }
		}
		wit::PluginCommandError::CommandUnavailable(payload) => {
			PluginCommandError::CommandUnavailable { command_id: payload.command_id }
		}
		wit::PluginCommandError::ExecutionFailed(payload) => {
			PluginCommandError::ExecutionFailed { message: payload.message }
		}
	}
}

#[cfg(test)]
mod tests {
	use std::time::{SystemTime, UNIX_EPOCH};

	use super::*;

	fn workspace_root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..") }

	fn unique_test_config_root() -> PathBuf {
		let nonce = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time should be after unix epoch")
			.as_nanos();
		std::env::temp_dir().join(format!("rim-plugin-host-test-{}-{}", std::process::id(), nonce))
	}

	#[test]
	fn yazi_plugin_can_be_discovered_and_invoked() {
		let workspace_root = workspace_root();
		let status = Command::new("cargo")
			.arg("build")
			.arg("-p")
			.arg("rim-plugin-yazi")
			.arg("--target")
			.arg("wasm32-wasip2")
			.current_dir(&workspace_root)
			.status()
			.expect("cargo build for yazi plugin should start");
		assert!(status.success(), "yazi plugin component build should succeed");

		let config_root = unique_test_config_root();
		let plugins_root = config_root.join("rim").join("plugins");
		fs::create_dir_all(&plugins_root).expect("plugin config directory should be created");
		let wasm_source = workspace_root.join("target/wasm32-wasip2/debug/rim_plugin_yazi.wasm");
		let wasm_target = plugins_root.join("rim_plugin_yazi.wasm");
		fs::copy(&wasm_source, &wasm_target).expect("yazi plugin wasm should be copied into config plugin dir");
		unsafe {
			std::env::set_var("XDG_CONFIG_HOME", &config_root);
		}

		let engine = build_engine();
		let discovery =
			discover_plugins(&engine, workspace_root.to_str().expect("workspace path should be utf-8"))
				.expect("plugin discovery should succeed");
		assert!(discovery.failures.is_empty(), "expected no plugin discovery failures: {:?}", discovery.failures);
		let plugins = discovery
			.plugins
			.into_iter()
			.map(|plugin| (plugin.registration.metadata.id.clone(), plugin))
			.collect::<HashMap<_, _>>();
		let request = PluginCommandRequest {
			command_id:     "plugin.yazi.yazi".to_string(),
			argument:       None,
			params:         Vec::new(),
			workspace_root: workspace_root.display().to_string(),
		};

		let response = invoke_command(&plugins, &request).expect("plugin invocation should succeed");
		assert!(
			response.effects.iter().any(|effect| {
				matches!(
					effect,
					PluginEffect::RequestAction(PluginAction::PickFile {
						command,
						chooser_file_arg_index,
					})
						if command
							== &vec![
								"yazi".to_string(),
								"--chooser-file".to_string(),
								String::new(),
							] && *chooser_file_arg_index == 2
				)
			}),
			"yazi plugin should request the host file picker"
		);
	}
}
