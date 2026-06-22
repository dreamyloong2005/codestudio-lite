use crate::core::types::{
    ApplyProfileRequest, ApplyProfileResult, ClearEnvironmentVariablesRequest,
    ClearEnvironmentVariablesResult, DeleteProfileDraftRequest, DuplicateProfileDraftRequest,
    PreviewProfileApplyRequest, PreviewProfileApplyResult, PreviewProfileWriteRequest,
    PreviewProfileWriteResult, ProfileDraft, ProfileSummary, ReorderProfileDraftsRequest,
    SaveProfileDraftRequest, StartCodexOAuthLoginResult, SwitchActiveProfileRequest,
    TestProfileConnectionRequest, TestProfileConnectionResult, UpdateProfileDraftRequest,
};
use crate::core::{env_health, profile};

#[tauri::command]
pub fn load_profile_summary() -> Result<ProfileSummary, String> {
    profile::load_profile_summary().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn start_codex_oauth_login() -> Result<StartCodexOAuthLoginResult, String> {
    profile::start_codex_oauth_login().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_profile_draft(request: SaveProfileDraftRequest) -> Result<ProfileDraft, String> {
    profile::save_profile_draft(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn update_profile_draft(request: UpdateProfileDraftRequest) -> Result<ProfileDraft, String> {
    profile::update_profile_draft(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn duplicate_profile_draft(
    request: DuplicateProfileDraftRequest,
) -> Result<ProfileDraft, String> {
    profile::duplicate_profile_draft(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn delete_profile_draft(request: DeleteProfileDraftRequest) -> Result<ProfileSummary, String> {
    profile::delete_profile_draft(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn reorder_profile_drafts(
    request: ReorderProfileDraftsRequest,
) -> Result<ProfileSummary, String> {
    profile::reorder_profile_drafts(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn preview_profile_write(
    request: PreviewProfileWriteRequest,
) -> Result<PreviewProfileWriteResult, String> {
    profile::preview_profile_write(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn preview_profile_apply(
    request: PreviewProfileApplyRequest,
) -> Result<PreviewProfileApplyResult, String> {
    profile::preview_profile_apply(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn apply_profile(request: ApplyProfileRequest) -> Result<ApplyProfileResult, String> {
    profile::apply_profile(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn test_profile_connection(
    request: TestProfileConnectionRequest,
) -> Result<TestProfileConnectionResult, String> {
    profile::test_profile_connection(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn switch_active_profile(
    request: SwitchActiveProfileRequest,
) -> Result<ProfileSummary, String> {
    profile::switch_active_profile(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn clear_claude_environment_variables(
    request: ClearEnvironmentVariablesRequest,
) -> Result<ClearEnvironmentVariablesResult, String> {
    env_health::clear_environment_variables(request).map_err(|err| err.to_string())
}
