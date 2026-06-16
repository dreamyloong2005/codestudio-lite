use crate::core::profile;
use crate::core::types::{AppSettings, ProfileSummary, UpdateAppSettingsRequest};

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
pub fn update_app_settings(request: UpdateAppSettingsRequest) -> Result<AppSettings, String> {
    profile::update_app_settings(request)
}
