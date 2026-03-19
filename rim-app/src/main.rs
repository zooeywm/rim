mod cli;

use std::error::Error;

use rim_app::{app::{App, detect_workspace_root}, logging};

fn main() {
	// Keep process-level failure handling centralized in one place.
	if let Err(err) = run() {
		eprintln!("{:#}", err);
		std::process::exit(1);
	}
}

fn run() -> Result<(), Box<dyn Error>> {
	// Bootstrap cross-cutting infrastructure before constructing the app container.
	logging::init_logging()?;
	let cli = cli::parse();
	if let Some(command) = cli.command {
		return Ok(cli::run(command)?);
	}
	let launch_dir = std::env::current_dir()?;
	let workspace_root = detect_workspace_root(launch_dir.as_path());
	std::env::set_current_dir(workspace_root.as_path())?;
	let app = App::new(workspace_root)?;
	// CLI positional args are treated as startup files to be opened by the runtime.
	let file_paths = cli.files;
	// Hand over control to the app-owned runtime loop.
	app.run(file_paths)?;
	Ok(())
}
