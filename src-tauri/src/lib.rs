pub mod commands;
pub mod core;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::activity_log::load_activity_log,
            commands::backup::list_backups,
            commands::backup::restore_backup,
            commands::codex_client::inspect_codex_client,
            commands::codex_client::install_codex_client,
            commands::codex_client::launch_codex_client,
            commands::codex_client::open_codex_client_path,
            commands::codex_client::plan_codex_client_update,
            commands::codex_client::stage_codex_client_update,
            commands::codex_client::uninstall_codex_client,
            commands::codex_client::update_codex_client_settings,
            commands::detect::detect_environment,
            commands::detect::load_cached_detection,
            commands::doctor::run_doctor,
            commands::gateway::load_gateway_status,
            commands::gateway::restart_gateway,
            commands::gateway::start_gateway,
            commands::gateway::stop_gateway,
            commands::gateway_request_log::load_gateway_request_log,
            commands::profiles::apply_profile,
            commands::profiles::clear_claude_environment_variables,
            commands::profiles::duplicate_profile_draft,
            commands::profiles::export_profiles,
            commands::profiles::import_profiles,
            commands::profiles::load_profile_summary,
            commands::profiles::preview_profile_apply,
            commands::profiles::preview_profile_write,
            commands::profiles::save_profile_draft,
            commands::profiles::switch_active_profile,
            commands::profiles::test_profile_connection,
            commands::profiles::update_profile_draft,
            commands::settings::ensure_app_dirs,
            commands::settings::load_app_settings,
            commands::settings::update_app_settings,
            commands::tool_installer::install_tool,
            commands::tool_installer::plan_tool_install,
            commands::tool_installer::repair_tool_path,
            commands::tool_installer::update_tool,
        ])
        .on_window_event(|_, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                crate::core::gateway::shutdown_for_app_exit();
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run CodeStudio Lite");
}
