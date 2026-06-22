use crate::core::profile;
use crate::core::tray;
use crate::core::types::{AppSettings, ProfileSummary, UpdateAppSettingsRequest};
use tauri::AppHandle;

#[tauri::command]
pub fn ensure_app_dirs() -> Result<ProfileSummary, String> {
    profile::ensure_app_dirs()?;
    profile::load_profile_summary().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn load_app_settings() -> Result<AppSettings, String> {
    profile::load_app_settings()
}

#[tauri::command]
pub fn update_app_settings(
    app: AppHandle,
    request: UpdateAppSettingsRequest,
) -> Result<AppSettings, String> {
    let previous_language = profile::load_app_settings()
        .map(|settings| settings.language)
        .unwrap_or_default();
    let updated = profile::update_app_settings(request)?;
    // Keep the tray menu labels in sync with the UI language.
    if updated.language != previous_language {
        tray::refresh(&app);
    }
    Ok(updated)
}
