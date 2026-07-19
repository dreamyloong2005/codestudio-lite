use crate::core::chatgpt_desktop::{
    self, ChatGptDesktopInstallRequest, ChatGptDesktopOperationResult, ChatGptDesktopProgress,
    ChatGptDesktopSettings, ChatGptDesktopStageReport, ChatGptDesktopState,
    ChatGptDesktopStateCache, ChatGptDesktopUninstallRequest, PlanChatGptDesktopUpdateRequest,
    StageChatGptDesktopUpdateRequest, UpdateChatGptDesktopSettingsRequest,
    CHATGPT_DESKTOP_PROGRESS_EVENT,
};
use crate::core::codex_provider_sync::{
    self, ProviderSyncReport, ProviderSyncStatus, ProviderSyncTargetList, ProviderSyncTargetOption,
    ProviderSyncTargetSource, SessionIndexCleanupPreview, SessionIndexCleanupResult,
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
pub async fn load_chatgpt_history_sync_targets() -> Result<ProviderSyncTargetList, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut result = codex_provider_sync::load_provider_sync_targets(None);
        let (selected, saved) = chatgpt_desktop::history_sync_preferences()?;
        for provider in saved.into_iter().chain(std::iter::once(selected)) {
            let provider = provider.trim();
            if provider.is_empty() {
                continue;
            }
            if let Some(target) = result.targets.iter_mut().find(|item| item.id == provider) {
                if !target.sources.contains(&ProviderSyncTargetSource::Manual) {
                    target.sources.push(ProviderSyncTargetSource::Manual);
                    target.sources.sort();
                }
            } else {
                result.targets.push(ProviderSyncTargetOption {
                    id: provider.to_string(),
                    sources: vec![ProviderSyncTargetSource::Manual],
                    is_current_provider: provider == result.current_provider,
                });
            }
        }
        result.targets.sort_by(|left, right| {
            right
                .is_current_provider
                .cmp(&left.is_current_provider)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(result)
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn sync_chatgpt_history_now(
    target_provider: Option<String>,
) -> Result<ProviderSyncReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = target_provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let report = codex_provider_sync::run_provider_sync_with_target(None, target);
        if report.status == ProviderSyncStatus::Synced {
            chatgpt_desktop::remember_history_sync_provider(&report.target_provider)?;
        }
        Ok(report)
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn preview_chatgpt_session_index_cleanup() -> Result<SessionIndexCleanupPreview, String> {
    tauri::async_runtime::spawn_blocking(|| {
        codex_provider_sync::preview_session_index_cleanup(None)
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn apply_chatgpt_session_index_cleanup(
    snapshot_sha256: String,
    thread_ids: Vec<String>,
) -> Result<SessionIndexCleanupResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        codex_provider_sync::apply_session_index_cleanup(None, &snapshot_sha256, &thread_ids)
            .map_err(|error| {
                if let Some(backup) = error.backup_dir {
                    format!("{} Backup: {}", error.message, backup.display())
                } else {
                    error.message
                }
            })
    })
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
