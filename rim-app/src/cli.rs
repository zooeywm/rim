use std::{env, fs, path::{Path, PathBuf}, process::Command, time::{SystemTime, UNIX_EPOCH}};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rim_paths::user_config_root;

#[derive(Debug, Parser)]
#[command(
	name = "rim",
	about = "A terminal-first editor with Wasm plugin support.",
	args_conflicts_with_subcommands = true
)]
pub(crate) struct Cli {
	#[command(subcommand)]
	pub(crate) command: Option<CommandGroup>,

	/// Files to open on startup.
	#[arg(value_name = "FILE")]
	pub(crate) files: Vec<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CommandGroup {
	/// Install and manage plugins.
	Plugin(PluginCommand),
}

#[derive(Debug, Args)]
pub(crate) struct PluginCommand {
	#[command(subcommand)]
	pub(crate) command: PluginSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PluginSubcommand {
	/// Install a plugin from GitHub by cloning and compiling its Wasm artifact.
	Install(PluginInstallCommand),
}

#[derive(Debug, Args)]
pub(crate) struct PluginInstallCommand {
	#[command(subcommand)]
	pub(crate) source: PluginInstallSource,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PluginInstallSource {
	/// Clone a GitHub repository, build the selected crate to wasm32-wasip2, and
	/// install it.
	Github(GithubInstallArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GithubInstallArgs {
	/// GitHub repository in owner/repo form or a full https://github.com/... URL.
	pub(crate) repository: String,

	/// Cargo package name to build.
	#[arg(long)]
	pub(crate) package: String,

	/// Cargo build profile used for the Wasm artifact.
	#[arg(long, value_enum, default_value_t = BuildProfile::Release)]
	pub(crate) profile: BuildProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum BuildProfile {
	Debug,
	Release,
}

impl BuildProfile {
	fn cargo_flag(self) -> Option<&'static str> {
		match self {
			Self::Debug => None,
			Self::Release => Some("--release"),
		}
	}

	fn target_dir(self) -> &'static str {
		match self {
			Self::Debug => "debug",
			Self::Release => "release",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubRepo {
	display:   String,
	clone_url: String,
}

#[derive(Debug)]
struct TempDirGuard {
	path: PathBuf,
}

impl TempDirGuard {
	fn new(prefix: &str) -> Result<Self> {
		let nonce =
			SystemTime::now().duration_since(UNIX_EPOCH).context("system clock is before unix epoch")?.as_nanos();
		let path = env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nonce));
		fs::create_dir_all(&path).with_context(|| format!("create temp directory failed: {}", path.display()))?;
		Ok(Self { path })
	}

	fn path(&self) -> &Path { self.path.as_path() }
}

impl Drop for TempDirGuard {
	fn drop(&mut self) { let _ = fs::remove_dir_all(&self.path); }
}

pub(crate) fn parse() -> Cli { Cli::parse() }

pub(crate) fn run(command: CommandGroup) -> Result<()> {
	match command {
		CommandGroup::Plugin(plugin) => run_plugin_command(plugin),
	}
}

fn run_plugin_command(plugin: PluginCommand) -> Result<()> {
	match plugin.command {
		PluginSubcommand::Install(install) => match install.source {
			PluginInstallSource::Github(args) => install_plugin_from_github(args),
		},
	}
}

fn install_plugin_from_github(args: GithubInstallArgs) -> Result<()> {
	let repository = normalize_github_repository(args.repository.as_str())?;
	ensure_install_environment()?;

	let checkout = TempDirGuard::new("rim-plugin-install")?;
	run_status_command(
		Command::new("git")
			.arg("clone")
			.arg("--depth")
			.arg("1")
			.arg(repository.clone_url.as_str())
			.arg(checkout.path()),
		&format!("clone plugin repository {}", repository.display),
	)?;

	let mut build = Command::new("cargo");
	build
		.arg("build")
		.arg("-p")
		.arg(args.package.as_str())
		.arg("--target")
		.arg("wasm32-wasip2")
		.current_dir(checkout.path());
	if let Some(flag) = args.profile.cargo_flag() {
		build.arg(flag);
	}
	run_status_command(&mut build, &format!("build package {}", args.package))?;

	let artifact = expected_wasm_artifact_path(checkout.path(), args.package.as_str(), args.profile);
	if !artifact.is_file() {
		bail!(
			"built package '{}' but no Wasm artifact was found at {}. Confirm the package name and that it \
			 produces a component for target wasm32-wasip2.",
			args.package,
			artifact.display()
		);
	}

	let plugins_root = user_config_root().join("plugins");
	fs::create_dir_all(&plugins_root)
		.with_context(|| format!("create plugin directory failed: {}", plugins_root.display()))?;
	let destination = plugins_root.join(
		artifact.file_name().ok_or_else(|| anyhow!("artifact path has no file name: {}", artifact.display()))?,
	);
	fs::copy(&artifact, &destination).with_context(|| {
		format!("copy plugin artifact failed: {} -> {}", artifact.display(), destination.display())
	})?;

	println!("Installed {}", destination.display());
	println!("Restart rim to discover the new plugin.");
	Ok(())
}

fn ensure_install_environment() -> Result<()> {
	for requirement in ["git", "cargo", "rustup"] {
		ensure_command_available(requirement)?;
	}
	ensure_wasm_target_installed()
}

fn ensure_command_available(command: &str) -> Result<()> {
	let status =
		Command::new(command).arg("--version").status().with_context(|| missing_command_message(command))?;
	if !status.success() {
		bail!("{}", missing_command_message(command));
	}
	Ok(())
}

fn ensure_wasm_target_installed() -> Result<()> {
	let output = Command::new("rustup")
		.args(["target", "list", "--installed"])
		.output()
		.context("run `rustup target list --installed` failed")?;
	if !output.status.success() {
		bail!(
			"failed to inspect installed Rust targets. Run `rustup target list --installed` manually, then \
			 install `wasm32-wasip2` with `rustup target add wasm32-wasip2`."
		);
	}
	let installed = String::from_utf8(output.stdout).context("rustup target list output is not valid utf-8")?;
	if installed.lines().any(|line| line.trim() == "wasm32-wasip2") {
		return Ok(());
	}
	bail!("missing Rust target `wasm32-wasip2`. Install it first with `rustup target add wasm32-wasip2`.");
}

fn missing_command_message(command: &str) -> String {
	match command {
		"git" => {
			"missing required tool `git`. Install Git first, then rerun `rim plugin install ...`.".to_string()
		}
		"cargo" => "missing required tool `cargo`. Install Rust with rustup first, then rerun `rim plugin \
		            install ...`."
			.to_string(),
		"rustup" => "missing required tool `rustup`. Install rustup first so rim can verify and guide the Wasm \
		             target setup."
			.to_string(),
		other => format!("missing required tool `{other}`."),
	}
}

fn run_status_command(command: &mut Command, description: &str) -> Result<()> {
	let status = command.status().with_context(|| format!("failed to {}", description))?;
	if status.success() {
		return Ok(());
	}
	match status.code() {
		Some(code) => bail!("failed to {}: command exited with status {}", description, code),
		None => bail!("failed to {}: command terminated by signal", description),
	}
}

fn normalize_github_repository(input: &str) -> Result<GithubRepo> {
	let trimmed = input.trim().trim_end_matches('/');
	if trimmed.is_empty() {
		bail!("GitHub repository must not be empty");
	}
	if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
		return normalize_owner_repo(rest);
	}
	if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
		return normalize_owner_repo(rest);
	}
	if trimmed.matches('/').count() == 1 && !trimmed.contains("://") {
		return normalize_owner_repo(trimmed);
	}
	bail!("unsupported GitHub repository '{}'. Use `owner/repo` or `https://github.com/owner/repo`.", input);
}

fn normalize_owner_repo(path: &str) -> Result<GithubRepo> {
	let cleaned = path.trim().trim_end_matches(".git").trim_matches('/');
	let mut parts = cleaned.split('/');
	let owner = parts.next().unwrap_or_default().trim();
	let repo = parts.next().unwrap_or_default().trim();
	if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
		bail!("unsupported GitHub repository '{}'. Use `owner/repo` or `https://github.com/owner/repo`.", path);
	}
	Ok(GithubRepo {
		display:   format!("{owner}/{repo}"),
		clone_url: format!("https://github.com/{owner}/{repo}.git"),
	})
}

fn expected_wasm_artifact_path(worktree: &Path, package: &str, profile: BuildProfile) -> PathBuf {
	let file_name = format!("{}.wasm", package.replace('-', "_"));
	worktree.join("target").join("wasm32-wasip2").join(profile.target_dir()).join(file_name)
}

#[cfg(test)]
mod tests {
	use clap::Parser;

	use super::*;

	#[test]
	fn cli_should_keep_positional_files() {
		let cli = Cli::parse_from(["rim", "src/main.rs", "Cargo.toml"]);
		assert!(cli.command.is_none());
		assert_eq!(cli.files, vec![PathBuf::from("src/main.rs"), PathBuf::from("Cargo.toml")]);
	}

	#[test]
	fn cli_should_allow_path_named_plugin_when_disambiguated_by_position() {
		let cli = Cli::parse_from(["rim", "plugin.txt"]);
		assert!(cli.command.is_none());
		assert_eq!(cli.files, vec![PathBuf::from("plugin.txt")]);
	}

	#[test]
	fn cli_should_parse_plugin_install_from_github() {
		let cli =
			Cli::parse_from(["rim", "plugin", "install", "github", "zooeywm/rim", "--package", "rim-plugin-yazi"]);
		let Some(CommandGroup::Plugin(plugin)) = cli.command else {
			panic!("expected plugin command");
		};
		let PluginSubcommand::Install(install) = plugin.command;
		let PluginInstallSource::Github(args) = install.source;
		assert_eq!(args.repository, "zooeywm/rim");
		assert_eq!(args.package, "rim-plugin-yazi");
		assert_eq!(args.profile, BuildProfile::Release);
	}

	#[test]
	fn normalize_github_repository_should_accept_shorthand() {
		let repo = normalize_github_repository("zooeywm/rim").expect("repo should normalize");
		assert_eq!(repo.display, "zooeywm/rim");
		assert_eq!(repo.clone_url, "https://github.com/zooeywm/rim.git");
	}

	#[test]
	fn normalize_github_repository_should_accept_full_url() {
		let repo =
			normalize_github_repository("https://github.com/zooeywm/rim.git").expect("repo should normalize");
		assert_eq!(repo.display, "zooeywm/rim");
		assert_eq!(repo.clone_url, "https://github.com/zooeywm/rim.git");
	}

	#[test]
	fn expected_wasm_artifact_path_should_follow_cargo_layout() {
		let path =
			expected_wasm_artifact_path(Path::new("/tmp/worktree"), "rim-plugin-yazi", BuildProfile::Debug);
		assert_eq!(path, PathBuf::from("/tmp/worktree/target/wasm32-wasip2/debug/rim_plugin_yazi.wasm"));
	}
}
