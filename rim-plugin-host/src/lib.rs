use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
	sync::Mutex,
	thread,
};

use rim_application::action::{AppAction, PluginRuntimeAction};
use rim_plugin_api::{PluginCapability, PluginCommandMetadata, PluginCommandRequest, PluginMetadata};
use rim_ports::{PluginDiscoveryResult, PluginLoadFailure, PluginRegistration, PluginRuntime, PluginRuntimeError, PluginRuntimeFailure};
use serde::Deserialize;
use tracing::error;

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
where
	Deps: AsRef<PluginHostState>,
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
	Discover {
		workspace_root: String,
	},
	InvokeCommand {
		request: PluginCommandRequest,
	},
}

#[derive(Debug, Clone)]
struct LoadedPlugin {
	registration: PluginRegistration,
	wasm_path:    PathBuf,
}

fn run_worker(request_rx: flume::Receiver<PluginHostRequest>, app_event_tx: flume::Sender<AppAction>) {
	let mut loaded_plugins = HashMap::<String, LoadedPlugin>::new();

	while let Ok(request) = request_rx.recv() {
		match request {
			PluginHostRequest::Discover { workspace_root } => {
				let result = discover_plugins(workspace_root.as_str()).map(|discovery| {
					let action_result = PluginDiscoveryResult {
						plugins:  discovery.plugins.iter().map(|plugin| plugin.registration.clone()).collect(),
						failures: discovery.failures.clone(),
					};
					(discovery.plugins, action_result)
				});
				if let Ok((plugins, _)) = &result {
					loaded_plugins.clear();
					for plugin in plugins {
						loaded_plugins.insert(plugin.registration.metadata.id.clone(), plugin.clone());
					}
				}
				let action_result = result.map(|(_, result)| result);
				if app_event_tx
					.send(AppAction::Plugin(PluginRuntimeAction::DiscoveryCompleted { result: action_result }))
					.is_err()
				{
					break;
				}
			}
			PluginHostRequest::InvokeCommand { request } => {
				let result = invoke_command(&loaded_plugins, &request);
				if app_event_tx
					.send(AppAction::Plugin(PluginRuntimeAction::CommandCompleted {
						context: request.context,
						result,
					}))
					.is_err()
				{
					break;
				}
			}
		}
	}
}

#[derive(Debug)]
struct DiscoverySnapshot {
	plugins:  Vec<LoadedPlugin>,
	failures: Vec<PluginLoadFailure>,
}

fn discover_plugins(workspace_root: &str) -> Result<DiscoverySnapshot, PluginRuntimeFailure> {
	let plugins_root = Path::new(workspace_root).join("plugins");
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
		if !path.is_dir() {
			continue;
		}
		match load_manifest(path.as_path()) {
			Ok(plugin) => plugins.push(plugin),
			Err(failure) => failures.push(failure),
		}
	}

	Ok(DiscoverySnapshot { plugins, failures })
}

fn load_manifest(plugin_dir: &Path) -> Result<LoadedPlugin, PluginLoadFailure> {
	let manifest_path = plugin_dir.join("plugin.toml");
	let manifest_text = fs::read_to_string(&manifest_path).map_err(|err| PluginLoadFailure {
		plugin_id: None,
		location:  manifest_path.display().to_string(),
		message:   format!("read plugin manifest failed: {}", err),
	})?;
	let manifest = toml::from_str::<PluginManifest>(manifest_text.as_str()).map_err(|err| PluginLoadFailure {
		plugin_id: None,
		location:  manifest_path.display().to_string(),
		message:   format!("parse plugin manifest failed: {}", err),
	})?;

	let entry_path = plugin_dir.join(&manifest.entry);
	if entry_path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
		return Err(PluginLoadFailure {
			plugin_id: Some(manifest.id.clone()),
			location:  entry_path.display().to_string(),
			message:   "plugin entry must be a .wasm file".to_string(),
		});
	}
	if !entry_path.is_file() {
		return Err(PluginLoadFailure {
			plugin_id: Some(manifest.id.clone()),
			location:  entry_path.display().to_string(),
			message:   "plugin wasm file is missing".to_string(),
		});
	}

	Ok(LoadedPlugin {
		registration: PluginRegistration {
			metadata: PluginMetadata {
				id:                    manifest.id,
				name:                  manifest.name,
				version:               manifest.version,
				abi_version:           manifest.abi_version,
				declared_capabilities: manifest.declared_capabilities,
			},
			commands: manifest.commands,
		},
		wasm_path: entry_path,
	})
}

fn invoke_command(
	loaded_plugins: &HashMap<String, LoadedPlugin>,
	request: &PluginCommandRequest,
) -> Result<rim_plugin_api::PluginCommandResponse, PluginRuntimeFailure> {
	let Some(plugin) = loaded_plugins.get(&request.context.plugin_id) else {
		return Err(PluginRuntimeFailure::Invocation {
			plugin_id:  request.context.plugin_id.clone(),
			command_id: request.command.id.clone(),
			message:    "plugin is not loaded".to_string(),
		});
	};
	let declared = plugin.registration.commands.iter().any(|command| command.id == request.command.id);
	if !declared {
		return Err(PluginRuntimeFailure::Invocation {
			plugin_id:  request.context.plugin_id.clone(),
			command_id: request.command.id.clone(),
			message:    "plugin command is not declared by the manifest".to_string(),
		});
	}

	Err(PluginRuntimeFailure::Invocation {
		plugin_id:  request.context.plugin_id.clone(),
		command_id: request.command.id.clone(),
		message:    format!(
			"wasm execution skeleton is not implemented yet for {}",
			plugin.wasm_path.display()
		),
	})
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginManifest {
	id:                    String,
	name:                  String,
	version:               String,
	abi_version:           u32,
	entry:                 String,
	#[serde(default)]
	declared_capabilities: Vec<PluginCapability>,
	#[serde(default)]
	commands:              Vec<PluginCommandMetadata>,
}
