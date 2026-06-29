use crate::core::app_paths::app_paths;
use crate::core::platform::hidden_command_with_args;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const NPM_CONFIG_PREFIX: &str = "NPM_CONFIG_PREFIX";
const NPM_ROOT_TIMEOUT: Duration = Duration::from_millis(1200);

pub fn user_prefix() -> Option<PathBuf> {
    app_paths()
        .ok()
        .map(|paths| paths.home_dir.join(".npm-global"))
}

pub fn user_prefix_override_for(npm: &str) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") || env::var_os(NPM_CONFIG_PREFIX).is_some() {
        return None;
    }

    let root = npm_global_root_from_command(npm)?;
    if accepts_child_creation(&root) {
        None
    } else {
        user_prefix()
    }
}

pub fn configure_command_for_global_packages(
    command: &mut Command,
    npm: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(prefix) = user_prefix_override_for(npm) else {
        return Ok(None);
    };

    ensure_user_prefix(&prefix)?;
    command.env(NPM_CONFIG_PREFIX, &prefix);
    command.env("PATH", path_with_prefix_bin(&prefix)?);
    Ok(Some(prefix))
}

pub fn shell_prefix_assignment(prefix: &Path) -> String {
    format!(
        "NPM_CONFIG_PREFIX={}",
        shell_single_quote(&prefix.to_string_lossy())
    )
}

fn npm_global_root_from_command(npm: &str) -> Option<PathBuf> {
    let output = command_output_with_timeout(hidden_command_with_args(npm, &["root", "-g"]))?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(PathBuf::from(stdout))
    }
}

fn ensure_user_prefix(prefix: &Path) -> Result<(), String> {
    fs::create_dir_all(prefix.join("bin")).map_err(|err| {
        format!(
            "Failed to create npm user bin directory {}: {err}",
            prefix.join("bin").display()
        )
    })?;
    fs::create_dir_all(prefix.join("lib").join("node_modules")).map_err(|err| {
        format!(
            "Failed to create npm user package directory {}: {err}",
            prefix.join("lib").join("node_modules").display()
        )
    })
}

fn path_with_prefix_bin(prefix: &Path) -> Result<OsString, String> {
    let prefix_bin = prefix.join("bin");
    let current = env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![prefix_bin.clone()];
    dirs.extend(
        env::split_paths(&current)
            .filter(|dir| path_key(dir) != path_key(&prefix_bin))
            .collect::<Vec<_>>(),
    );
    env::join_paths(dirs).map_err(|err| format!("Failed to prepare npm PATH: {err}"))
}

fn accepts_child_creation(path: &Path) -> bool {
    let Some(parent) = writable_probe_directory(path) else {
        return false;
    };
    let probe = parent.join(format!(
        ".codestudio-lite-write-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn writable_probe_directory(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return path.is_dir().then(|| path.to_path_buf());
    }

    let mut current = path.parent();
    while let Some(parent) = current {
        if parent.exists() {
            return parent.is_dir().then(|| parent.to_path_buf());
        }
        current = parent.parent();
    }
    None
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn command_output_with_timeout(mut command: Command) -> Option<std::process::Output> {
    let mut child = command.spawn().ok()?;
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if started_at.elapsed() >= NPM_ROOT_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_child_creation_for_existing_temp_dir() {
        let root =
            env::temp_dir().join(format!("codestudio-npm-global-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp dir");

        assert!(accepts_child_creation(&root));

        let entries = fs::read_dir(&root)
            .expect("read temp dir")
            .collect::<Result<Vec<_>, _>>()
            .expect("entries");
        assert!(entries.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn accepts_child_creation_for_missing_child_under_writable_parent() {
        let root = env::temp_dir().join(format!(
            "codestudio-npm-global-missing-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        assert!(accepts_child_creation(
            &root.join("lib").join("node_modules")
        ));
    }

    #[test]
    fn shell_prefix_assignment_quotes_paths() {
        assert_eq!(
            shell_prefix_assignment(Path::new("/Users/alice's mac/.npm-global")),
            "NPM_CONFIG_PREFIX='/Users/alice'\\''s mac/.npm-global'"
        );
    }
}
