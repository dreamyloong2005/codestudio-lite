use crate::core::codex_client::{
    self, CodexClientInstallRequest, CodexClientOperationResult, CodexClientProgress,
    CodexClientSettings, CodexClientStageReport, CodexClientState, CodexClientStateCache,
    CodexClientUninstallRequest, PlanCodexClientUpdateRequest, StageCodexClientUpdateRequest,
    UpdateCodexClientSettingsRequest, CODEX_CLIENT_PROGRESS_EVENT,
};
use tauri::Emitter;

#[tauri::command]
pub async fn inspect_codex_client() -> Result<CodexClientState, String> {
    tauri::async_runtime::spawn_blocking(|| codex_client::inspect_state(false))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn load_cached_codex_client_state() -> Result<Option<CodexClientState>, String> {
    Ok(
        tauri::async_runtime::spawn_blocking(|| codex_client::load_cached_state())
            .await
            .map_err(|err| err.to_string())?,
    )
}

#[tauri::command]
pub async fn load_cached_codex_client_states() -> Result<CodexClientStateCache, String> {
    Ok(
        tauri::async_runtime::spawn_blocking(|| codex_client::load_cached_states())
            .await
            .map_err(|err| err.to_string())?,
    )
}

#[tauri::command]
pub async fn plan_codex_client_update(
    request: PlanCodexClientUpdateRequest,
) -> Result<CodexClientState, String> {
    tauri::async_runtime::spawn_blocking(move || codex_client::plan_update(request))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn stage_codex_client_update(
    app: tauri::AppHandle,
    request: StageCodexClientUpdateRequest,
) -> Result<CodexClientStageReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        codex_client::stage_update_with_progress(request, |progress| emit_progress(&app, progress))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn install_codex_client(
    app: tauri::AppHandle,
    request: CodexClientInstallRequest,
) -> Result<CodexClientOperationResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        codex_client::install_or_update_with_progress(request, |progress| {
            emit_progress(&app, progress)
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn uninstall_codex_client(
    request: CodexClientUninstallRequest,
) -> Result<CodexClientOperationResult, String> {
    tauri::async_runtime::spawn_blocking(move || codex_client::uninstall(request))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn launch_codex_client() -> Result<(), String> {
    codex_client::launch()
}

#[tauri::command]
pub fn update_codex_client_settings(
    request: UpdateCodexClientSettingsRequest,
) -> Result<CodexClientSettings, String> {
    codex_client::update_settings(request)
}

#[tauri::command]
pub fn open_codex_client_path(kind: String) -> Result<(), String> {
    codex_client::open_path(kind)
}

fn emit_progress(app: &tauri::AppHandle, progress: CodexClientProgress) {
    let _ = app.emit(CODEX_CLIENT_PROGRESS_EVENT, progress);
}
