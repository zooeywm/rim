mod action;
mod action_handler;
mod app;
mod input;
mod io_gateway;
mod logging;
mod state;
mod ui;

use std::io;
use std::path::PathBuf;

use app::App;

fn main() -> io::Result<()> {
    logging::init_logging()?;
    let file_paths = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let mut app = App::new()?;
    if file_paths.is_empty() {
        app.create_untitled_buffer();
    } else {
        for path in file_paths {
            app.open_file(path)?;
        }
    }
    app.run()
}
