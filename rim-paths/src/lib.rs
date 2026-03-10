use std::path::PathBuf;

pub fn user_config_root() -> PathBuf {
	#[cfg(target_os = "windows")]
	{
		windows_config_root_from_env(
			std::env::var_os("APPDATA").map(PathBuf::from),
			std::env::var_os("USERPROFILE").map(PathBuf::from),
			std::env::temp_dir(),
		)
	}

	#[cfg(target_os = "macos")]
	{
		std::env::var_os("HOME")
			.map(PathBuf::from)
			.unwrap_or_else(std::env::temp_dir)
			.join("Library")
			.join("Application Support")
			.join("rim")
	}

	#[cfg(all(unix, not(target_os = "macos")))]
	{
		if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
			return config_home.join("rim");
		}
		std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(".config").join("rim")
	}
}

pub fn user_state_root() -> PathBuf {
	#[cfg(target_os = "windows")]
	{
		windows_state_root_from_env(
			std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
			std::env::var_os("USERPROFILE").map(PathBuf::from),
			std::env::temp_dir(),
		)
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
			return state_home.join("rim");
		}
		std::env::var_os("HOME")
			.map(PathBuf::from)
			.unwrap_or_else(std::env::temp_dir)
			.join(".local")
			.join("state")
			.join("rim")
	}
}

pub fn user_log_dir() -> PathBuf {
	#[cfg(target_os = "macos")]
	{
		user_state_root()
	}

	#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
	{
		user_state_root().join("logs")
	}
}

#[cfg(any(test, target_os = "windows"))]
fn windows_state_root_from_env(
	local_app_data: Option<PathBuf>,
	user_profile: Option<PathBuf>,
	temp_dir: PathBuf,
) -> PathBuf {
	if let Some(local_app_data) = local_app_data.filter(|path| path.is_absolute()) {
		return local_app_data.join("rim");
	}
	if let Some(user_profile) = user_profile.filter(|path| path.is_absolute()) {
		return user_profile.join("AppData").join("Local").join("rim");
	}
	temp_dir.join("rim")
}

#[cfg(any(test, target_os = "windows"))]
fn windows_config_root_from_env(
	app_data: Option<PathBuf>,
	user_profile: Option<PathBuf>,
	temp_dir: PathBuf,
) -> PathBuf {
	if let Some(app_data) = app_data.filter(|path| path.is_absolute()) {
		return app_data.join("rim");
	}
	if let Some(user_profile) = user_profile.filter(|path| path.is_absolute()) {
		return user_profile.join("AppData").join("Roaming").join("rim");
	}
	temp_dir.join("rim")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn windows_state_root_should_ignore_relative_localappdata() {
		let root = windows_state_root_from_env(
			Some(PathBuf::from("relative-localappdata")),
			Some(PathBuf::from("/Users/tester")),
			PathBuf::from("/tmp"),
		);

		assert_eq!(root, PathBuf::from("/Users/tester/AppData/Local/rim"));
	}

	#[test]
	fn windows_config_root_should_ignore_relative_appdata() {
		let root = windows_config_root_from_env(
			Some(PathBuf::from("relative-appdata")),
			Some(PathBuf::from("/Users/tester")),
			PathBuf::from("/tmp"),
		);

		assert_eq!(root, PathBuf::from("/Users/tester/AppData/Roaming/rim"));
	}
}
