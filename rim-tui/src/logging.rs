use rim_paths::user_log_dir;
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
