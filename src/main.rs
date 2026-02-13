use std::path::PathBuf;

use anyhow::{Context, Result};
use rim::{app::App, logging};

fn main() {
	if let Err(err) = run() {
		eprintln!("{:#}", err);
		std::process::exit(1);
	}
}

fn run() -> Result<()> {
	logging::init_logging().context("initialize logging failed")?;
	let file_paths = std::env::args().skip(1).map(PathBuf::from).collect::<Vec<_>>();
	let app = App::new().context("initialize app failed")?;
	app.run(file_paths).context("run app failed")
}
