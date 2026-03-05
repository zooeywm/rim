use std::path::PathBuf;

use anyhow::{Context, Result};
use rim_app::app::App;
use rim_cli::logging;

fn main() {
	// Keep process-level failure handling centralized in one place.
	if let Err(err) = run() {
		eprintln!("{:#}", err);
		std::process::exit(1);
	}
}

fn run() -> Result<()> {
	// Bootstrap cross-cutting infrastructure before constructing the app container.
	logging::init_logging().context("initialize logging failed")?;
	// CLI args are treated as startup files to be opened by the runtime.
	let file_paths = std::env::args().skip(1).map(PathBuf::from).collect::<Vec<_>>();
	let app = App::new().context("initialize app failed")?;
	// Hand over control to the app-owned runtime loop.
	app.run(file_paths).context("run app failed")
}
