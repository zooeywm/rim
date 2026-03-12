use std::{error::Error, path::PathBuf};

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
	let launch_dir = std::env::current_dir()?;
	let workspace_root = detect_workspace_root(launch_dir.as_path());
	std::env::set_current_dir(workspace_root.as_path())?;
	// CLI args are treated as startup files to be opened by the runtime.
	let file_paths = std::env::args().skip(1).map(PathBuf::from).collect::<Vec<_>>();
	let app = App::new(workspace_root)?;
	// Hand over control to the app-owned runtime loop.
	app.run(file_paths)?;
	Ok(())
}
