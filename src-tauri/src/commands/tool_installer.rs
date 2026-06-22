use crate::core::tool_installer::{self, TOOL_INSTALL_PROGRESS_EVENT};
use crate::core::tool_launch;
use crate::core::types::{
    RepairToolPathRequest, RepairToolPathResult, ToolInstallPlan, ToolInstallProgress,
    ToolInstallRequest, ToolInstallResult, ToolLaunchPlan, ToolUninstallRequest,
};
use tauri::Emitter;

#[tauri::command]
pub async fn plan_tool_install(tool_id: String) -> Result<ToolInstallPlan, String> {
    tauri::async_runtime::spawn_blocking(move || tool_installer::plan_tool_install(&tool_id))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn plan_tool_update(tool_id: String) -> Result<ToolInstallPlan, String> {
    tauri::async_runtime::spawn_blocking(move || tool_installer::plan_tool_update(&tool_id))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn plan_tool_launch(tool_id: String) -> Result<ToolLaunchPlan, String> {
    tauri::async_runtime::spawn_blocking(move || tool_launch::plan_tool_launch(&tool_id))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn install_tool(
    app: tauri::AppHandle,
    request: ToolInstallRequest,
) -> Result<ToolInstallResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let progress = move |progress| emit_progress(&app, progress);
        tool_installer::install_tool_with_progress(request, Some(&progress))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn update_tool(
    app: tauri::AppHandle,
    request: ToolInstallRequest,
) -> Result<ToolInstallResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let progress = move |progress| emit_progress(&app, progress);
        tool_installer::update_tool_with_progress(request, Some(&progress))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn uninstall_tool(
    app: tauri::AppHandle,
    request: ToolUninstallRequest,
) -> Result<ToolInstallResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let progress = move |progress| emit_progress(&app, progress);
        tool_installer::uninstall_tool_with_progress(request, Some(&progress))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn repair_tool_path(
    request: RepairToolPathRequest,
) -> Result<RepairToolPathResult, String> {
    tauri::async_runtime::spawn_blocking(move || tool_installer::repair_tool_path(request))
        .await
        .map_err(|err| err.to_string())?
}

fn emit_progress(app: &tauri::AppHandle, progress: ToolInstallProgress) {
    let _ = app.emit(TOOL_INSTALL_PROGRESS_EVENT, progress);
}
