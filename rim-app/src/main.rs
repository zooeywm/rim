use std::{error::Error, path::PathBuf};

use rim_app::{app::App, logging};

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
	// CLI args are treated as startup files to be opened by the runtime.
	let file_paths = std::env::args().skip(1).map(PathBuf::from).collect::<Vec<_>>();
	let app = App::new()?;
	// Hand over control to the app-owned runtime loop.
	app.run(file_paths)?;
	Ok(())
}
