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
