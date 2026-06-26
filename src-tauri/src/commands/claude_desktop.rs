use crate::core::types::{ClaudeDesktopPendingLaunch, ClaudeDesktopPlan};
use crate::core::{claude_desktop_patch, tool_installer};

#[tauri::command]
pub fn launch_claude_desktop(app: tauri::AppHandle, localize: Option<bool>) -> Result<(), String> {
    let localize = localize.unwrap_or(false);
    claude_desktop_patch::launch_with_app(localize, Some(app))
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
