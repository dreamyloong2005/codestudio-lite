use crate::core::{app_paths, download_http, gateway};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;
use url::Url;

pub const APP_UPDATE_PROGRESS_EVENT: &str = "app-update-progress";
const UPDATE_HOST: &str = "download.codestudio.build";
const UPDATE_DIRECTORY: &str = "application-update";
const UPDATE_CLEANUP_ATTEMPTS: usize = 15;
const UPDATE_CLEANUP_RETRY_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallApplicationUpdateRequest {
    pub version: String,
    pub url: String,
    pub signature: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgress {
    pub phase: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

pub fn application_update_target() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("windows-x86_64"),
        ("macos", "aarch64") => Ok("darwin-aarch64"),
        ("macos", "x86_64") => Ok("darwin-x86_64"),
        ("linux", "x86_64") => Ok("linux-x86_64"),
        ("linux", "aarch64") => Ok("linux-aarch64"),
        ("linux", "arm") => Ok("linux-armv7"),
        (os, arch) => Err(format!(
            "Automatic application updates are unsupported on {os}-{arch}."
        )),
    }
}

pub fn install_application_update<F>(
    app: &AppHandle,
    request: InstallApplicationUpdateRequest,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(AppUpdateProgress),
{
    validate_request(&request)?;
    let update_dir = app_paths::app_paths()
        .map_err(|err| format!("Failed to resolve application paths: {err}"))?
        .downloads_dir
        .join(UPDATE_DIRECTORY);
    fs::create_dir_all(&update_dir)
        .map_err(|err| format!("Failed to create application update directory: {err}"))?;
    let destination = update_dir.join(&request.filename);
    let temporary = update_dir.join(format!("{}.part", request.filename));

    on_progress(progress("downloading", 0, None));
    let downloaded = download_http::download_to_file(
        &request.url,
        &destination,
        &temporary,
        None,
        Duration::from_secs(120),
        download_http::DOWNLOAD_HTTP_MAX_ATTEMPTS,
        |downloaded_bytes, total_bytes| {
            on_progress(progress("downloading", downloaded_bytes, total_bytes));
        },
    )?;

    on_progress(progress("verifying", downloaded, Some(downloaded)));
    if let Err(err) = verify_installer_signature(&destination, &request.signature) {
        let _ = fs::remove_file(&destination);
        return Err(err);
    }

    on_progress(progress("installing", downloaded, Some(downloaded)));
    launch_platform_installer(app, &destination, &request.version)?;
    gateway::shutdown_for_app_exit();
    app.exit(0);
    Ok(())
}

pub fn schedule_stale_update_cleanup() {
    let Ok(paths) = app_paths::app_paths() else {
        return;
    };
    let update_dir = paths.downloads_dir.join(UPDATE_DIRECTORY);
    let mut artifacts = stale_update_artifacts(&update_dir);
    if artifacts.is_empty() {
        let _ = fs::remove_dir(&update_dir);
        return;
    }

    thread::spawn(move || {
        for attempt in 0..UPDATE_CLEANUP_ATTEMPTS {
            remove_stale_update_artifacts_once(&mut artifacts);
            if artifacts.is_empty() {
                let _ = fs::remove_dir(&update_dir);
                return;
            }
            if attempt + 1 < UPDATE_CLEANUP_ATTEMPTS {
                thread::sleep(UPDATE_CLEANUP_RETRY_DELAY);
            }
        }
    });
}

fn stale_update_artifacts(update_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(update_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .filter(|entry| is_stale_update_artifact(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect()
}

fn is_stale_update_artifact(filename: &str) -> bool {
    let filename = filename.to_ascii_lowercase();
    filename.ends_with(".exe")
        || filename.ends_with(".dmg")
        || filename.ends_with(".part")
        || (filename.starts_with("install-") && filename.ends_with(".sh"))
}

fn remove_stale_update_artifacts_once(artifacts: &mut Vec<PathBuf>) {
    artifacts.retain(|path| match fs::remove_file(path) {
        Ok(()) => false,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
    });
}

fn progress(phase: &str, downloaded_bytes: u64, total_bytes: Option<u64>) -> AppUpdateProgress {
    AppUpdateProgress {
        phase: phase.to_string(),
        downloaded_bytes,
        total_bytes,
    }
}

fn validate_request(request: &InstallApplicationUpdateRequest) -> Result<(), String> {
    if request.version.trim().is_empty()
        || !request.version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
    {
        return Err("The application update version is invalid.".to_string());
    }
    if request.filename.is_empty()
        || request.filename.contains(['/', '\\'])
        || request.filename.contains([' ', '_'])
    {
        return Err("The application update filename is invalid.".to_string());
    }

    let url = Url::parse(&request.url).map_err(|_| "The application update URL is invalid.")?;
    if url.scheme() != "https"
        || url.host_str() != Some(UPDATE_HOST)
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "Application updates must use https://{UPDATE_HOST}."
        ));
    }
    let url_filename = url
        .path_segments()
        .and_then(|segments| segments.last())
        .ok_or("The application update URL has no filename.")?;
    if url_filename != request.filename {
        return Err("The application update filename does not match its URL.".to_string());
    }

    #[cfg(target_os = "windows")]
    if !request.filename.ends_with(".exe") {
        return Err("Windows application updates must use a Burn EXE.".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        if !request.filename.ends_with(".dmg") {
            return Err("macOS application updates must use a DMG.".to_string());
        }
        let architecture = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x64"
        };
        if !request
            .filename
            .ends_with(&format!("-macOS-{architecture}.dmg"))
        {
            return Err(format!(
                "macOS application updates must match the running {architecture} architecture."
            ));
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return Err("Installer handoff is only supported on Windows and macOS.".to_string());

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    Ok(())
}

fn updater_public_key() -> Result<PublicKey, String> {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../../updater.config.json"))
            .map_err(|err| format!("Failed to read the updater public key configuration: {err}"))?;
    let encoded = config
        .get("pubkey")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or("The updater public key is not configured.")?;
    let decoded = decode_tauri_signer_text(encoded, "updater public key")?;
    PublicKey::decode(&decoded).map_err(|err| format!("The updater public key is invalid: {err}"))
}

fn verify_installer_signature(path: &Path, encoded_signature: &str) -> Result<(), String> {
    let public_key = updater_public_key()?;
    let decoded_signature =
        decode_tauri_signer_text(encoded_signature, "application update signature")?;
    let signature = Signature::decode(&decoded_signature)
        .map_err(|err| format!("The application update signature is invalid: {err}"))?;
    let mut verifier = public_key
        .verify_stream(&signature)
        .map_err(|err| format!("Failed to initialize application update verification: {err}"))?;
    let mut file = File::open(path)
        .map_err(|err| format!("Failed to open the downloaded application update: {err}"))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("Failed to verify the downloaded application update: {err}"))?;
        if read == 0 {
            break;
        }
        verifier.update(&buffer[..read]);
    }
    verifier
        .finalize()
        .map_err(|err| format!("Application update signature verification failed: {err}"))
}

fn decode_tauri_signer_text(encoded: &str, label: &str) -> Result<String, String> {
    let bytes = BASE64_STANDARD
        .decode(encoded.trim())
        .map_err(|err| format!("The {label} is not valid Base64: {err}"))?;
    String::from_utf8(bytes).map_err(|err| format!("The {label} is not UTF-8 text: {err}"))
}

#[cfg(target_os = "windows")]
fn launch_platform_installer(_: &AppHandle, installer: &Path, _: &str) -> Result<(), String> {
    launch_windows_burn(installer)
}

#[cfg(target_os = "windows")]
fn launch_windows_burn(installer: &Path) -> Result<(), String> {
    Command::new(installer)
        .args(["-quiet", "-norestart", "-LaunchAfterInstall=1"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Failed to start the CodeStudio Lite installer: {err}"))
}

#[cfg(target_os = "macos")]
fn launch_platform_installer(
    app: &AppHandle,
    installer: &Path,
    version: &str,
) -> Result<(), String> {
    launch_macos_dmg_helper(app, installer, version)
}

#[cfg(target_os = "macos")]
fn launch_macos_dmg_helper(_: &AppHandle, installer: &Path, version: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let current_exe = std::env::current_exe()
        .map_err(|err| format!("Failed to locate the running application: {err}"))?;
    let app_bundle = current_exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("app"))
        .ok_or("CodeStudio Lite is not running from a macOS application bundle.")?;
    let helper = installer
        .parent()
        .ok_or("The application update directory is invalid.")?
        .join(format!("install-{version}.sh"));
    fs::write(&helper, MACOS_DMG_HELPER)
        .map_err(|err| format!("Failed to create the macOS update helper: {err}"))?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))
        .map_err(|err| format!("Failed to secure the macOS update helper: {err}"))?;
    Command::new("/bin/sh")
        .arg(&helper)
        .arg(std::process::id().to_string())
        .arg(installer)
        .arg(app_bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Failed to start the macOS update helper: {err}"))
}

#[cfg(target_os = "macos")]
const MACOS_DMG_HELPER: &str = r#"#!/bin/sh
set -eu
parent_pid="$1"
dmg="$2"
target_app="$3"
mount_dir="$(mktemp -d /tmp/codestudio-update.XXXXXX)"
backup_app="${target_app}.update-backup"
attached=0
cleanup() {
  if [ "$attached" -eq 1 ]; then hdiutil detach "$mount_dir" -quiet || true; fi
  rm -rf "$mount_dir"
}
trap cleanup EXIT
while kill -0 "$parent_pid" 2>/dev/null; do sleep 1; done
hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "$dmg" >/dev/null
attached=1
source_app="$(find "$mount_dir" -maxdepth 2 -type d -name 'CodeStudio Lite.app' -print -quit)"
if [ -z "$source_app" ]; then exit 1; fi
rm -rf "$backup_app"
mv "$target_app" "$backup_app"
if ! ditto "$source_app" "$target_app"; then
  rm -rf "$target_app"
  mv "$backup_app" "$target_app"
  exit 1
fi
if ! open "$target_app"; then
  rm -rf "$target_app"
  mv "$backup_app" "$target_app"
  open "$target_app" || true
  exit 1
fi
rm -rf "$backup_app"
rm -f "$dmg" "$0"
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_non_r2_update_urls() {
        let request = InstallApplicationUpdateRequest {
            version: "1.4.2".to_string(),
            url: "https://example.com/CodeStudio-Lite-1.4.2-Windows-x64-setup.exe".to_string(),
            signature: "signature".to_string(),
            filename: "CodeStudio-Lite-1.4.2-Windows-x64-setup.exe".to_string(),
        };
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn reads_the_tauri_signer_wrapped_public_key() {
        assert!(updater_public_key().is_ok());
    }

    #[test]
    fn resolves_the_native_tauri_update_target() {
        let target = application_update_target().unwrap();
        assert!(matches!(
            target,
            "windows-x86_64"
                | "darwin-aarch64"
                | "darwin-x86_64"
                | "linux-x86_64"
                | "linux-aarch64"
                | "linux-armv7"
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rejects_a_macos_installer_for_the_other_architecture() {
        let other_architecture = if cfg!(target_arch = "aarch64") {
            "x64"
        } else {
            "arm64"
        };
        let filename = format!("CodeStudio-Lite-1.5.0-macOS-{other_architecture}.dmg");
        let request = InstallApplicationUpdateRequest {
            version: "1.5.0".to_string(),
            url: format!("https://{UPDATE_HOST}/releases/1.5.0/{filename}"),
            signature: "signature".to_string(),
            filename,
        };

        assert!(validate_request(&request)
            .unwrap_err()
            .contains("must match the running"));
    }

    #[test]
    fn startup_cleanup_removes_only_known_update_artifacts() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codestudio-update-cleanup-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("keep-directory")).unwrap();
        for filename in [
            "CodeStudio-Lite-1.5.0-Windows-x64-setup.exe",
            "CodeStudio-Lite-1.5.0-macOS-universal.dmg",
            "CodeStudio-Lite-1.5.0-Windows-x64-setup.exe.part",
            "install-1.5.0.sh",
            "keep.txt",
        ] {
            fs::write(root.join(filename), filename).unwrap();
        }

        let mut artifacts = stale_update_artifacts(&root);
        assert_eq!(artifacts.len(), 4);
        remove_stale_update_artifacts_once(&mut artifacts);

        assert!(artifacts.is_empty());
        assert!(root.join("keep.txt").is_file());
        assert!(root.join("keep-directory").is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
