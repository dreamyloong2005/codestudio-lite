use crate::core::app_updater::{self, InstallApplicationUpdateRequest};
use tauri::Emitter;

#[tauri::command]
pub fn application_update_target() -> Result<String, String> {
    app_updater::application_update_target().map(str::to_string)
}

#[tauri::command]
pub async fn install_application_update(
    app: tauri::AppHandle,
    request: InstallApplicationUpdateRequest,
) -> Result<(), String> {
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app_updater::install_application_update(&app, request, |progress| {
            let _ = progress_app.emit(app_updater::APP_UPDATE_PROGRESS_EVENT, progress);
        })
    })
    .await
    .map_err(|err| err.to_string())?
}
