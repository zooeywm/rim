use std::io;
use std::path::PathBuf;

use time::macros::format_description;
use tracing_subscriber::fmt::time::UtcTime;

pub fn init_logging() -> io::Result<()> {
    let log_dir = user_log_dir();
    std::fs::create_dir_all(&log_dir)?;

    let timer = UtcTime::new(format_description!(
        "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]"
    ));
    let file_appender = tracing_appender::rolling::never(&log_dir, "rim.log");
    tracing_subscriber::fmt()
        .with_timer(timer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(file_appender)
        .with_ansi(false)
        .init();

    Ok(())
}

fn user_log_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("rim")
            .join("logs")
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Library")
            .join("Logs")
            .join("rim")
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(state_home) = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from) {
            return state_home.join("rim").join("logs");
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(".local")
            .join("state")
            .join("rim")
            .join("logs")
    }
}
