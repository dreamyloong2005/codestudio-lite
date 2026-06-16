use crate::core::tool_installer;
use crate::core::types::{
    RepairToolPathRequest, RepairToolPathResult, ToolInstallPlan, ToolInstallRequest,
    ToolInstallResult,
};

#[tauri::command]
pub async fn plan_tool_install(tool_id: String) -> Result<ToolInstallPlan, String> {
    tauri::async_runtime::spawn_blocking(move || tool_installer::plan_tool_install(&tool_id))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn install_tool(request: ToolInstallRequest) -> Result<ToolInstallResult, String> {
    tauri::async_runtime::spawn_blocking(move || tool_installer::install_tool(request))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn update_tool(request: ToolInstallRequest) -> Result<ToolInstallResult, String> {
    tauri::async_runtime::spawn_blocking(move || tool_installer::update_tool(request))
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
