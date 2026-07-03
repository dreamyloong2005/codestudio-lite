use crate::core::platform::package;
use crate::core::types::{ClaudeDesktopInstallKinds, CodexClientInstallKinds, DetectionSnapshot};
use crate::core::{codex_client, detector};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectEnvironmentRequest {
    wait_for_updates: Option<bool>,
}

#[tauri::command]
pub async fn detect_environment(
    request: Option<DetectEnvironmentRequest>,
) -> Result<DetectionSnapshot, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let request = request.unwrap_or_default();
        detector::detect_environment_with_options(detector::DetectionOptions {
            wait_for_updates: request.wait_for_updates.unwrap_or(false),
        })
        .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn load_cached_detection() -> Result<Option<DetectionSnapshot>, String> {
    tauri::async_runtime::spawn_blocking(detector::load_cached_detection)
        .await
        .map_err(|err| err.to_string())
}

/// Force a fresh detection for a per-tool page refresh: re-resolve install
/// state from scratch (invalidating the in-process install cache so a
/// just-completed install is found instead of serving a stale `not found`)
/// while preserving the network-fetched latest-version cache, and block briefly
/// for the Claude Desktop latest version when it isn't cached yet so the page
/// shows the latest version instead of "unknown".
#[tauri::command]
pub async fn detect_environment_fresh() -> Result<DetectionSnapshot, String> {
    tauri::async_runtime::spawn_blocking(|| {
        detector::detect_environment_fresh().map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

/// Per-kind install detection for the Claude Desktop page tabs: resolves the
/// MSIX (Windows App) and native .exe installs independently so the UI can
/// show a tab per install kind. A user may have both installed at once.
#[tauri::command]
pub async fn detect_claude_install_kinds() -> Result<ClaudeDesktopInstallKinds, String> {
    tauri::async_runtime::spawn_blocking(detector::claude_desktop_install_kinds)
        .await
        .map_err(|err| err.to_string())
}

/// Per-kind install detection for the Codex desktop client page tabs:
/// resolves the MSIX (Windows App) and portable installs independently so
/// the UI can show a tab per install kind.
#[tauri::command]
pub async fn detect_codex_install_kinds() -> Result<CodexClientInstallKinds, String> {
    tauri::async_runtime::spawn_blocking(codex_client::codex_client_install_kinds)
        .await
        .map_err(|err| err.to_string())
}

/// Local MSIX-runtime capability check for the Claude Desktop page (mirrors
/// the Codex client capability panel): probes Add-AppxPackage, AppXSvc and
/// the MSIX runtime so the user can see whether the Windows App install path
/// is available on this machine.
#[tauri::command]
pub async fn detect_claude_capabilities() -> Result<Vec<codex_client::CodexClientCapability>, String>
{
    tauri::async_runtime::spawn_blocking(|| {
        package::probe_msix_capabilities()
            .into_iter()
            .map(|cap| codex_client::CodexClientCapability {
                id: cap.id,
                label: cap.label,
                status: cap.status,
                detail: cap.detail,
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|err| err.to_string())
}
