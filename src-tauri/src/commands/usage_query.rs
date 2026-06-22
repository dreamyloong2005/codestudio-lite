use crate::core::types::{UsageQueryResult, UsageScriptSaveRequest, UsageScriptState};
use crate::core::usage_query;

#[tauri::command]
pub fn load_usage_script_state(profile_id: String) -> Result<UsageScriptState, String> {
    usage_query::load_usage_state(&profile_id)
}

#[tauri::command]
pub fn save_usage_script(request: UsageScriptSaveRequest) -> Result<UsageScriptState, String> {
    usage_query::save_usage_script(request)
}

#[tauri::command]
pub fn test_usage_script(request: UsageScriptSaveRequest) -> Result<UsageQueryResult, String> {
    usage_query::test_usage_script(request)
}

#[tauri::command]
pub fn query_profile_usage(profile_id: String) -> Result<UsageQueryResult, String> {
    usage_query::query_usage(&profile_id)
}

#[tauri::command]
pub fn delete_usage_script(profile_id: String) -> Result<UsageScriptState, String> {
    usage_query::delete_usage_script(&profile_id)
}
