pub mod commands;
pub mod core;

pub fn run() {
    tauri::Builder::default()
        // Single-instance guard: must be registered before any other plugin.
        // If a second instance is launched, the callback fires in the first
        // (already-running) instance — we show and focus the main window so
        // the user is brought back to the existing app instead of spawning a
        // duplicate.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
                #[cfg(target_os = "windows")]
                {
                    let _ = window.set_skip_taskbar(false);
                }
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::activity_log::load_activity_log,
            commands::app_updater::application_update_target,
            commands::app_updater::install_application_update,
            commands::backup::list_backups,
            commands::backup::restore_backup,
            commands::claude_desktop::inspect_claude_desktop_page,
            commands::claude_desktop::launch_claude_desktop,
            commands::claude_desktop::open_claude_desktop_path,
            commands::claude_desktop::plan_claude_desktop_update,
            commands::claude_desktop::restart_claude_desktop_after_accessibility_grant,
            commands::claude_desktop::take_pending_claude_desktop_launch_after_restart,
            commands::chatgpt_desktop::inspect_chatgpt_desktop,
            commands::chatgpt_desktop::load_chatgpt_history_sync_targets,
            commands::chatgpt_desktop::sync_chatgpt_history_now,
            commands::chatgpt_desktop::preview_chatgpt_session_index_cleanup,
            commands::chatgpt_desktop::apply_chatgpt_session_index_cleanup,
            commands::chatgpt_desktop::install_chatgpt_desktop,
            commands::chatgpt_desktop::load_cached_chatgpt_desktop_state,
            commands::chatgpt_desktop::load_cached_chatgpt_desktop_states,
            commands::chatgpt_desktop::launch_chatgpt_desktop,
            commands::chatgpt_desktop::open_chatgpt_desktop_path,
            commands::chatgpt_desktop::plan_chatgpt_desktop_update,
            commands::chatgpt_desktop::stage_chatgpt_desktop_update,
            commands::chatgpt_desktop::uninstall_chatgpt_desktop,
            commands::chatgpt_desktop::update_chatgpt_desktop_settings,
            commands::detect::detect_environment,
            commands::detect::detect_environment_fresh,
            commands::detect::detect_claude_install_kinds,
            commands::detect::detect_claude_capabilities,
            commands::detect::detect_chatgpt_desktop_install_kinds,
            commands::detect::load_cached_detection,
            commands::doctor::run_doctor,
            commands::gateway::load_gateway_status,
            commands::gateway::restart_gateway,
            commands::gateway::start_gateway,
            commands::gateway::stop_gateway,
            commands::gateway::update_gateway_settings,
            commands::gateway_request_log::load_gateway_request_log,
            commands::install_terminal::resize_install_terminal,
            commands::install_terminal::start_install_terminal,
            commands::install_terminal::launch_tool_external,
            commands::install_terminal::stop_install_terminal,
            commands::install_terminal::write_install_terminal,
            commands::profiles::apply_profile,
            commands::profiles::clear_claude_environment_variables,
            commands::profiles::delete_profile_draft,
            commands::profiles::duplicate_profile_draft,
            commands::profiles::list_profile_models,
            commands::profiles::load_profile_summary,
            commands::profiles::preview_profile_apply,
            commands::profiles::preview_profile_write,
            commands::profiles::reorder_profile_drafts,
            commands::profiles::save_profile_draft,
            commands::profiles::start_codex_oauth_login,
            commands::profiles::switch_active_profile,
            commands::profiles::test_profile_connection,
            commands::profiles::update_profile_draft,
            commands::settings::ensure_app_dirs,
            commands::settings::load_app_settings,
            commands::settings::update_app_settings,
            commands::tool_installer::install_tool,
            commands::tool_installer::plan_tool_install,
            commands::tool_installer::plan_tool_launch,
            commands::tool_installer::plan_tool_update,
            commands::tool_installer::repair_tool_path,
            commands::tool_installer::uninstall_tool,
            commands::tool_installer::update_tool,
            commands::usage_query::delete_usage_script,
            commands::usage_query::load_usage_script_state,
            commands::usage_query::query_profile_usage,
            commands::usage_query::save_usage_script,
            commands::usage_query::test_usage_script,
        ])
        .setup(|app| {
            // A successful Windows update relaunches before Burn has fully
            // exited, so remove captured updater artifacts with lock retries.
            crate::core::app_updater::schedule_stale_update_cleanup();
            // GUI launches on macOS do not source shell profiles, so restore
            // PATH entries that CodeStudio Lite repaired in earlier sessions.
            let _ = crate::core::env_health::restore_persisted_path_repairs();
            // Register the system tray icon + menu so closing the main window
            // hides it to the tray instead of quitting the app. The tray's
            // "Quit" entry performs the real shutdown (including the gateway).
            crate::core::tray::setup(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Intercept the main window close: hide to the tray instead of
            // quitting. The app keeps running with its tray icon; the gateway
            // is only shut down on an explicit Quit from the tray menu.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(target_os = "windows")]
                {
                    let _ = window.set_skip_taskbar(true);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run CodeStudio Lite");
}
