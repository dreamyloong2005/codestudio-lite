use crate::core::app_paths;
use crate::core::detector;
use crate::core::gateway;
use crate::core::macos_app_scope::{
    cleanup_managed_user_bundles_for_roots, spawn_codestudio_self_cleanup_helper, status_for_roots,
    take_codestudio_self_cleanup_failure as take_self_cleanup_failure,
    CodestudioSelfCleanupFailure, MacosApplicationCleanupResult, MacosApplicationScopeStatus,
    MacosManagedApp,
};
use std::path::Path;

fn home_dir() -> Result<std::path::PathBuf, String> {
    app_paths::app_paths()
        .map(|paths| paths.home_dir)
        .or_else(|_| {
            dirs::home_dir().ok_or_else(|| "Unable to resolve the home directory.".to_string())
        })
}

#[tauri::command]
pub fn load_macos_application_scope_status(
    app_id: MacosManagedApp,
) -> Result<MacosApplicationScopeStatus, String> {
    let home = home_dir()?;
    let current_executable = (app_id == MacosManagedApp::CodeStudioLite)
        .then(std::env::current_exe)
        .transpose()
        .map_err(|error| format!("Failed to locate the running application: {error}"))?;
    Ok(status_for_roots(
        &home,
        Path::new("/Applications"),
        app_id,
        current_executable.as_deref(),
    ))
}

#[tauri::command]
pub fn take_codestudio_self_cleanup_failure() -> Result<Option<CodestudioSelfCleanupFailure>, String>
{
    take_self_cleanup_failure(&home_dir()?)
}

#[tauri::command]
pub async fn cleanup_macos_user_application(
    app: tauri::AppHandle,
    app_id: MacosManagedApp,
) -> Result<MacosApplicationCleanupResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = home_dir()?;
        let system_applications = Path::new("/Applications");
        let result = if app_id == MacosManagedApp::CodeStudioLite {
            let current_executable = std::env::current_exe()
                .map_err(|error| format!("Failed to locate the running application: {error}"))?;
            spawn_codestudio_self_cleanup_helper(&home, system_applications, &current_executable)?
        } else {
            let result =
                cleanup_managed_user_bundles_for_roots(&home, system_applications, app_id)?;
            detector::invalidate_update_cache();
            let _ = detector::detect_environment();
            result
        };
        if result.restart_scheduled {
            gateway::shutdown_for_app_exit();
            app.exit(0);
        }
        Ok(result)
    })
    .await
    .map_err(|error| error.to_string())?
}
