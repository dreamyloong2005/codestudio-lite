use crate::core::activity_log;
use crate::core::types::ActivityEvent;

#[tauri::command]
pub fn load_activity_log() -> Result<Vec<ActivityEvent>, String> {
    activity_log::load_recent().map_err(|err| err.to_string())
}
