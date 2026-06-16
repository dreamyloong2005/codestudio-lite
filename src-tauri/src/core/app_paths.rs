use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub home_dir: PathBuf,
    pub config_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub applied_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub config_file: PathBuf,
    pub activity_log_file: PathBuf,
    pub gateway_request_log_file: PathBuf,
}

pub fn app_paths() -> io::Result<AppPaths> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;
    let config_dir = home_dir.join(".codestudio-lite");
    let profiles_dir = config_dir.join("profiles");
    let applied_dir = config_dir.join("applied");
    let backups_dir = config_dir.join("backups");
    let logs_dir = config_dir.join("logs");
    let config_file = config_dir.join("config.toml");
    let activity_log_file = logs_dir.join("activity.jsonl");
    let gateway_request_log_file = logs_dir.join("gateway-requests.jsonl");

    Ok(AppPaths {
        home_dir,
        config_dir,
        profiles_dir,
        applied_dir,
        backups_dir,
        logs_dir,
        config_file,
        activity_log_file,
        gateway_request_log_file,
    })
}

pub fn ensure_dirs(paths: &AppPaths) -> io::Result<()> {
    std::fs::create_dir_all(&paths.profiles_dir)?;
    std::fs::create_dir_all(&paths.applied_dir)?;
    std::fs::create_dir_all(&paths.backups_dir)?;
    std::fs::create_dir_all(&paths.logs_dir)?;
    Ok(())
}

pub fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
