use crate::core::chatgpt_desktop::DesktopClientCapability;
use crate::core::platform::package;
use crate::core::types::{ClaudeDesktopPageState, ClaudeDesktopPendingLaunch, ClaudeDesktopPlan};
use crate::core::{claude_desktop_patch, detector, tool_installer};

#[tauri::command]
pub async fn launch_claude_desktop(
    app: tauri::AppHandle,
    localize: Option<bool>,
) -> Result<(), String> {
    let localize = localize.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        claude_desktop_patch::launch_with_app(localize, Some(app))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn take_pending_claude_desktop_launch_after_restart(
) -> Result<Option<ClaudeDesktopPendingLaunch>, String> {
    claude_desktop_patch::take_pending_claude_desktop_launch_after_restart()
}

#[tauri::command]
pub fn restart_claude_desktop_after_accessibility_grant(
    app: tauri::AppHandle,
    localize: Option<bool>,
) -> Result<(), String> {
    claude_desktop_patch::restart_claude_desktop_after_accessibility_grant(
        app,
        localize.unwrap_or(false),
    )
}

#[tauri::command]
pub fn open_claude_desktop_path(kind: String) -> Result<(), String> {
    tool_installer::open_claude_desktop_path(kind)
}

#[tauri::command]
pub fn plan_claude_desktop_update() -> Result<ClaudeDesktopPlan, String> {
    tool_installer::plan_claude_desktop_update()
}

#[tauri::command]
pub async fn inspect_claude_desktop_page(
    force: Option<bool>,
) -> Result<ClaudeDesktopPageState, String> {
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = if force {
            detector::detect_environment_fresh()?
        } else {
            detector::detect_environment()?
        };
        let status = snapshot
            .tools
            .iter()
            .find(|tool| tool.id == "claude-desktop");
        let install_plan =
            tool_installer::plan_tool_install_for_status("claude-desktop", status).ok();
        let update_plan =
            tool_installer::plan_tool_update_for_status("claude-desktop", status).ok();
        let plan = tool_installer::plan_claude_desktop_update_for_status(status).ok();
        let capabilities = package::probe_msix_capabilities()
            .into_iter()
            .map(|cap| DesktopClientCapability {
                id: cap.id,
                label: cap.label,
                status: cap.status,
                detail: cap.detail,
            })
            .collect::<Vec<_>>();

        Ok(ClaudeDesktopPageState {
            snapshot,
            install_plan,
            update_plan,
            plan,
            capabilities,
        })
    })
    .await
    .map_err(|err| err.to_string())?
}
