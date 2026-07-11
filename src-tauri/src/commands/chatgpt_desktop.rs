use crate::core::chatgpt_desktop::{
    self, ChatGptDesktopInstallRequest, ChatGptDesktopOperationResult, ChatGptDesktopProgress,
    ChatGptDesktopSettings, ChatGptDesktopStageReport, ChatGptDesktopState,
    ChatGptDesktopStateCache, ChatGptDesktopUninstallRequest, PlanChatGptDesktopUpdateRequest,
    StageChatGptDesktopUpdateRequest, UpdateChatGptDesktopSettingsRequest,
    CHATGPT_DESKTOP_PROGRESS_EVENT,
};
use tauri::Emitter;

#[tauri::command]
pub async fn inspect_chatgpt_desktop() -> Result<ChatGptDesktopState, String> {
    tauri::async_runtime::spawn_blocking(|| chatgpt_desktop::inspect_state(false))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn load_cached_chatgpt_desktop_state() -> Result<Option<ChatGptDesktopState>, String> {
    Ok(
        tauri::async_runtime::spawn_blocking(|| chatgpt_desktop::load_cached_state())
            .await
            .map_err(|err| err.to_string())?,
    )
}

#[tauri::command]
pub async fn load_cached_chatgpt_desktop_states() -> Result<ChatGptDesktopStateCache, String> {
    Ok(
        tauri::async_runtime::spawn_blocking(|| chatgpt_desktop::load_cached_states())
            .await
            .map_err(|err| err.to_string())?,
    )
}

#[tauri::command]
pub async fn plan_chatgpt_desktop_update(
    request: PlanChatGptDesktopUpdateRequest,
) -> Result<ChatGptDesktopState, String> {
    tauri::async_runtime::spawn_blocking(move || chatgpt_desktop::plan_update(request))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn stage_chatgpt_desktop_update(
    app: tauri::AppHandle,
    request: StageChatGptDesktopUpdateRequest,
) -> Result<ChatGptDesktopStageReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        chatgpt_desktop::stage_update_with_progress(request, |progress| {
            emit_progress(&app, progress)
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn install_chatgpt_desktop(
    app: tauri::AppHandle,
    request: ChatGptDesktopInstallRequest,
) -> Result<ChatGptDesktopOperationResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        chatgpt_desktop::install_or_update_with_progress(request, |progress| {
            emit_progress(&app, progress)
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn uninstall_chatgpt_desktop(
    request: ChatGptDesktopUninstallRequest,
) -> Result<ChatGptDesktopOperationResult, String> {
    tauri::async_runtime::spawn_blocking(move || chatgpt_desktop::uninstall(request))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn launch_chatgpt_desktop() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| chatgpt_desktop::launch())
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn update_chatgpt_desktop_settings(
    request: UpdateChatGptDesktopSettingsRequest,
) -> Result<ChatGptDesktopSettings, String> {
    chatgpt_desktop::update_settings(request)
}

#[tauri::command]
pub fn open_chatgpt_desktop_path(kind: String) -> Result<(), String> {
    chatgpt_desktop::open_path(kind)
}

fn emit_progress(app: &tauri::AppHandle, progress: ChatGptDesktopProgress) {
    let _ = app.emit(CHATGPT_DESKTOP_PROGRESS_EVENT, progress);
}
