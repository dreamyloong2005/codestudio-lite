use crate::core::detector;
use crate::core::types::DetectionSnapshot;

#[tauri::command]
pub async fn detect_environment() -> Result<DetectionSnapshot, String> {
    tauri::async_runtime::spawn_blocking(|| {
        detector::detect_environment().map_err(|err| err.to_string())
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
