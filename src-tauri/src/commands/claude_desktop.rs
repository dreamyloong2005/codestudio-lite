use crate::core::{claude_desktop_patch, tool_installer};

#[tauri::command]
pub fn launch_claude_desktop(app: tauri::AppHandle, localize: Option<bool>) -> Result<(), String> {
    let localize = localize.unwrap_or(false);
    claude_desktop_patch::launch_with_app(localize, Some(app))
}

#[tauri::command]
pub fn open_claude_desktop_path(kind: String) -> Result<(), String> {
    tool_installer::open_claude_desktop_path(kind)
}
