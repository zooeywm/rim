use std::path::PathBuf;

use thiserror::Error;
use time::macros::format_description;
use tracing_subscriber::fmt::time::UtcTime;

#[derive(Debug, Error)]
pub enum LoggingError {
	#[error("create log directory failed")]
	CreateLogDir {
		#[source]
		source: std::io::Error,
	},
	#[error("initialize tracing subscriber failed")]
	InitSubscriber {
		#[source]
		source: Box<dyn std::error::Error + Send + Sync>,
	},
}

pub fn init_logging() -> Result<(), LoggingError> {
	let log_dir = user_log_dir();
	std::fs::create_dir_all(&log_dir).map_err(|source| LoggingError::CreateLogDir { source })?;

	let timer =
		UtcTime::new(format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]"));
	let file_appender = tracing_appender::rolling::never(&log_dir, "rim.log");
	tracing_subscriber::fmt()
		.with_timer(timer)
		.with_env_filter(
			tracing_subscriber::EnvFilter::try_from_default_env()
				.unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
		)
		.with_writer(file_appender)
		.with_ansi(false)
		.try_init()
		.map_err(|source| LoggingError::InitSubscriber { source })?;

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
