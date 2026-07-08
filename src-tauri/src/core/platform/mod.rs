use crate::core::app_paths::app_paths;
use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub mod package;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn resolve_command(command: &str) -> Option<String> {
    if powershell_alias(command) {
        return resolve_powershell_command();
    }

    if let Some(found) = find_on_path(command) {
        return Some(found);
    }

    for dir in extra_command_dirs() {
        for candidate in command_candidates(command) {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    None
}

pub fn resolve_command_on_path(command: &str) -> Option<String> {
    find_on_path(command)
}

pub fn resolve_powershell_command() -> Option<String> {
    let path = powershell_exe();
    if path.is_file() {
        return Some(path.to_string_lossy().to_string());
    }
    find_on_path("powershell")
}

pub fn repair_candidate_for_command(command: &str) -> Option<(PathBuf, PathBuf)> {
    if find_on_path(command).is_some() {
        return None;
    }

    for dir in extra_command_dirs() {
        for candidate in command_candidates(command) {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some((path, dir));
            }
        }
    }

    None
}

pub fn hidden_command_with_args(program: &str, args: &[&str]) -> Command {
    let mut command = command_for_program(program, args);
    configure_hidden_command(&mut command);
    command
}

pub fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    configure_hidden_command(&mut command);
    command
}

pub fn run_powershell(script: &str) -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("PowerShell is only available on Windows.".to_string());
    }

    let script = format!(
        r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
{script}
"#
    );
    let output = hidden_command(powershell_exe())
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|err| format!("Failed to start PowerShell: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "PowerShell execution failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn configure_hidden_command(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
}

fn find_on_path(command: &str) -> Option<String> {
    let path_value = env::var_os("PATH")?;
    for dir in env::split_paths(&path_value) {
        for candidate in command_candidates(command) {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }
    None
}

pub fn command_candidates(command: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![
            format!("{command}.exe"),
            format!("{command}.cmd"),
            format!("{command}.bat"),
            format!("{command}.ps1"),
            command.to_string(),
        ]
    } else {
        vec![command.to_string()]
    }
}

pub fn extra_command_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(windows) {
        if let Some(app_data) = env::var_os("APPDATA") {
            dirs.push(PathBuf::from(app_data).join("npm"));
        }
        if let Ok(paths) = app_paths() {
            dirs.push(paths.home_dir.join("AppData").join("Roaming").join("npm"));
        }
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            let local_app_data = PathBuf::from(local_app_data);
            dirs.push(local_app_data.join("Programs").join("Claude"));
            dirs.push(local_app_data.join("Programs").join("nodejs"));
            dirs.push(
                local_app_data
                    .join("Programs")
                    .join("Microsoft VS Code")
                    .join("bin"),
            );
            dirs.push(
                local_app_data
                    .join("Programs")
                    .join("Microsoft VS Code Insiders")
                    .join("bin"),
            );
            dirs.push(local_app_data.join("Microsoft").join("WindowsApps"));
        }
        if let Some(program_files) = env::var_os("ProgramFiles") {
            let program_files = PathBuf::from(program_files);
            dirs.push(program_files.join("Claude"));
            dirs.push(program_files.join("Anthropic").join("Claude"));
            dirs.push(program_files.join("nodejs"));
            dirs.push(program_files.join("Microsoft VS Code").join("bin"));
            dirs.push(program_files.join("Microsoft VS Code Insiders").join("bin"));
        }
        if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
            dirs.push(PathBuf::from(program_files_x86).join("nodejs"));
        }
    } else if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/opt/homebrew/sbin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/local/sbin"));
        dirs.push(
            PathBuf::from("/Applications/Visual Studio Code.app")
                .join("Contents")
                .join("Resources")
                .join("app")
                .join("bin"),
        );
        dirs.push(
            PathBuf::from("/Applications/Visual Studio Code - Insiders.app")
                .join("Contents")
                .join("Resources")
                .join("app")
                .join("bin"),
        );
        if let Ok(paths) = app_paths() {
            dirs.push(paths.home_dir.join(".npm-global").join("bin"));
            dirs.push(paths.home_dir.join(".local").join("bin"));
            dirs.push(
                paths
                    .home_dir
                    .join("Library")
                    .join("Application Support")
                    .join("fnm")
                    .join("aliases")
                    .join("default")
                    .join("bin"),
            );
        }
    }
    dirs
}

fn command_for_program(program: &str, args: &[&str]) -> Command {
    #[cfg(windows)]
    {
        let lower = program.to_ascii_lowercase();
        if lower.ends_with(".cmd") || lower.ends_with(".bat") {
            let mut command = Command::new("cmd.exe");
            let script = cmd_script(program, args);
            command.raw_arg(format!("/d /c \"{script}\""));
            return command;
        }
        if lower.ends_with(".ps1") {
            let mut command = Command::new(powershell_exe());
            command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", program]);
            command.args(args);
            return command;
        }
    }

    let mut command = Command::new(program);
    command.args(args);
    command
}

pub fn powershell_exe() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Some(path) = windows_system_powershell_exe() {
            return path;
        }
        if let Some(found) = find_on_path("powershell") {
            return PathBuf::from(found);
        }
        return PathBuf::from("powershell.exe");
    }

    PathBuf::from("powershell")
}

fn powershell_alias(command: &str) -> bool {
    let normalized = command.trim().to_ascii_lowercase();
    normalized == "powershell" || normalized == "powershell.exe"
}

#[cfg(windows)]
fn windows_system_powershell_exe() -> Option<PathBuf> {
    ["WINDIR", "SystemRoot"]
        .iter()
        .filter_map(|key| env::var_os(key))
        .map(PathBuf::from)
        .chain(std::iter::once(PathBuf::from(r"C:\Windows")))
        .map(|root| {
            root.join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .find(|path| path.is_file())
}

#[cfg(not(windows))]
fn windows_system_powershell_exe() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn cmd_script(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .map(cmd_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn cmd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_candidates_match_platform_shell_conventions() {
        let candidates = command_candidates("tool");
        if cfg!(windows) {
            assert_eq!(
                candidates,
                vec!["tool.exe", "tool.cmd", "tool.bat", "tool.ps1", "tool"]
            );
        } else {
            assert_eq!(candidates, vec!["tool"]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn cmd_script_quotes_program_and_arguments() {
        assert_eq!(
            cmd_script("C:\\Program Files\\tool.cmd", &["--flag", "hello world"]),
            "\"C:\\Program Files\\tool.cmd\" \"--flag\" \"hello world\""
        );
    }

    #[test]
    fn powershell_runner_is_explicitly_windows_only() {
        if !cfg!(target_os = "windows") {
            assert_eq!(
                run_powershell("$PSVersionTable.PSVersion").unwrap_err(),
                "PowerShell is only available on Windows."
            );
        }
    }

    #[test]
    fn powershell_command_resolution_handles_windows_system_location() {
        if cfg!(windows) {
            let resolved = resolve_command("powershell").expect("PowerShell should resolve");
            assert!(
                resolved.to_ascii_lowercase().ends_with("powershell.exe"),
                "resolved PowerShell path should point at powershell.exe: {resolved}"
            );
        }
    }
}
