use crate::core::claude_desktop_patch;

#[tauri::command]
pub fn launch_claude_desktop(localize: Option<bool>) -> Result<(), String> {
    let localize = localize.unwrap_or(false);
    claude_desktop_patch::launch(localize)
}
