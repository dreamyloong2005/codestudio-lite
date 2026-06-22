use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub home_dir: PathBuf,
    pub config_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub database_file: PathBuf,
}

pub fn app_paths() -> io::Result<AppPaths> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;
    let config_dir = home_dir.join(".codestudio-lite");
    let downloads_dir = config_dir.join("downloads");
    let database_file = config_dir.join("app_state.sqlite");

    Ok(AppPaths {
        home_dir,
        config_dir,
        downloads_dir,
        database_file,
    })
}

pub fn ensure_dirs(paths: &AppPaths) -> io::Result<()> {
    std::fs::create_dir_all(&paths.config_dir)?;
    std::fs::create_dir_all(&paths.downloads_dir)?;
    Ok(())
}

pub fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
