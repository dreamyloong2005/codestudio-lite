use super::*;
use crate::core::types::{ConfigState, DesktopInstallKindInfo, ToolCategory};

#[test]
fn npm_tool_plan_is_whitelisted() {
    let definition = install_definition("claude").expect("definition");
    let plan = build_plan(&definition, None);
    assert_eq!(plan.manager, "npm");
    assert!(plan.command.contains("@anthropic-ai/claude-code"));
    assert!(plan.command.contains("npm install -g"));
}

#[test]
fn npm_global_commands_keep_global_flag() {
    let action = InstallAction::NpmGlobal("@anthropic-ai/claude-code");

    let install = command_preview(&action);
    let update = update_command_preview(&action);
    let uninstall = uninstall_command_preview_for_tool("claude", &action);

    assert!(install.contains("npm install -g @anthropic-ai/claude-code"));
    assert!(update.contains("npm install -g @anthropic-ai/claude-code@latest"));
    assert!(uninstall.contains("npm uninstall -g @anthropic-ai/claude-code"));
}

#[test]
fn npm_global_actions_repair_node_and_npm_path_first() {
    let repairs =
        path_repair_tool_ids_for_action(&InstallAction::NpmGlobal("@anthropic-ai/claude-code"));

    assert_eq!(repairs, vec!["node", "npm"]);
}

#[test]
fn pi_install_update_and_uninstall_keep_the_upstream_npm_contract() {
    let definition = install_definition("pi").expect("Pi install definition");

    assert_eq!(manager_label(&definition.action), "npm");
    assert_eq!(
        command_preview(&definition.action),
        "npm install -g --ignore-scripts @earendil-works/pi-coding-agent"
    );
    assert_eq!(
        update_command_preview_for_tool("pi", &definition.action),
        "npm install -g --ignore-scripts @earendil-works/pi-coding-agent@latest"
    );
    assert_eq!(
        uninstall_command_preview_for_tool("pi", &definition.action),
        "npm uninstall -g @earendil-works/pi-coding-agent"
    );
    assert_eq!(
        path_repair_tool_ids_for_action(&definition.action),
        vec!["node", "npm"]
    );
}

#[test]
fn path_repair_stage_result_is_prerequisite_stage() {
    let repair = RepairToolPathResult {
        success: true,
        tool_id: "npm".to_string(),
        tool_name: "npm".to_string(),
        added_path: Some("/Users/test/.npm-global/bin".to_string()),
        message: "Added /Users/test/.npm-global/bin to the user PATH.".to_string(),
        current_status: None,
        notes: vec!["Refreshed current process PATH.".to_string()],
    };

    let stage = path_repair_stage_result(
        "npm",
        "npm",
        "Repair PATH for npm".to_string(),
        &repair,
        path_repair_stdout(&repair),
        String::new(),
    );

    assert!(stage.success);
    assert_eq!(stage.stage, "prerequisite");
    assert_eq!(stage.exit_code, Some(0));
    assert!(stage
        .stdout_tail
        .contains("Added /Users/test/.npm-global/bin"));
    assert!(stage
        .stdout_tail
        .contains("Refreshed current process PATH."));
}

#[test]
fn npm_is_provided_by_node() {
    let definition = install_definition("npm").expect("definition");
    let plan = build_plan(&definition, None);
    assert!(plan.can_install);
    assert!(plan.command.contains("OpenJS.NodeJS.LTS") || plan.command.contains("nodejs.org"));
    assert_eq!(plan.commands[0].tool_id, "npm");
    assert_eq!(plan.commands[0].tool_name, "npm");
    if cfg!(target_os = "macos") {
        assert!(plan.interactive);
        assert!(plan.command.contains("https://nodejs.org/dist/index.json"));
    }
}

#[test]
fn npm_tool_can_include_node_prerequisite() {
    let definition = install_definition("opencode").expect("definition");
    let plan = build_plan(&definition, None);
    if command_available("npm") {
        assert!(!plan.requires_prerequisites);
        assert_eq!(plan.commands.len(), 1);
    } else if cfg!(target_os = "windows") && command_available("winget") {
        assert!(plan.can_install);
        assert!(plan.requires_prerequisites);
        assert_eq!(plan.prerequisites[0].tool_id, "node");
        assert_eq!(plan.commands[0].stage, "prerequisite");
    }
}

#[test]
fn macos_install_routes_do_not_use_brew_commands() {
    let source = include_str!("tool_installer.rs");
    let registry = include_str!("tool_registry.rs");
    let detector = include_str!("detector.rs");
    let brew = ["br", "ew"].concat();
    let formula_variant = ["Home", "brew", "Formula"].concat();

    assert!(!source.contains(&formula_variant));
    assert!(!source.contains(&format!("{brew} install")));
    assert!(!source.contains(&format!("{brew} upgrade")));
    assert!(!source.contains(&format!("{brew} uninstall")));
    assert!(!registry.contains(&format!("{brew} install")));
    assert!(!detector.contains(&format!("{brew} upgrade")));
    assert!(source.contains("https://nodejs.org/dist/index.json"));
    assert!(source.contains("https://bun.sh/install"));
    assert!(source.contains("https://hermes-agent.nousresearch.com/install.sh"));
    assert!(source.contains("xcode-select --install"));
}

#[test]
fn official_script_actions_render_expected_commands() {
    assert_eq!(
        command_preview(&InstallAction::InteractiveShellScript(
            "Bun official install script",
            BUN_UNIX_INSTALL_COMMAND,
        )),
        BUN_UNIX_INSTALL_COMMAND
    );
    assert_eq!(
        command_preview(&InstallAction::InteractiveShellScript(
            "Apple Command Line Tools installer",
            GIT_MACOS_COMMAND_LINE_TOOLS_INSTALL_COMMAND,
        )),
        GIT_MACOS_COMMAND_LINE_TOOLS_INSTALL_COMMAND
    );
}

#[test]
fn hermes_update_uses_cli_update_command_not_installer_script() {
    let definition = install_definition("hermes").expect("definition");

    assert_eq!(
        update_command_preview_for_tool("hermes", &definition.action),
        "hermes update"
    );
}

#[test]
fn grok_install_uses_official_platform_script() {
    let definition = install_definition("grok").expect("Grok install definition");

    assert_eq!(manager_label(&definition.action), "terminal");
    if cfg!(target_os = "windows") {
        assert_eq!(
            command_preview(&definition.action),
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://x.ai/cli/install.ps1 | iex\""
        );
    } else {
        assert_eq!(
            command_preview(&definition.action),
            "curl -fsSL https://x.ai/cli/install.sh | bash"
        );
    }
}

#[test]
fn claude_desktop_macos_uses_official_dmg_not_homebrew_cask() {
    let source = include_str!("tool_installer.rs");
    let registry = include_str!("tool_registry.rs");
    let detector = include_str!("detector.rs");
    let cask_variant = ["Home", "brew", "Cask(\"", "cla", "ude\")"].concat();
    let cask_install = ["br", "ew install --", "cask ", "cla", "ude"].concat();
    let cask_upgrade = ["br", "ew upgrade --", "cask ", "cla", "ude"].concat();
    let cask_uninstall = ["br", "ew uninstall --", "cask ", "cla", "ude"].concat();
    let cask_detection = ["\"claude-desktop\" => Some(\"", "cla", "ude\")"].concat();

    assert!(source.contains("InstallAction::MacosDmgApp"));
    assert!(source.contains("CLAUDE_DESKTOP_LATEST_MACOS_URL"));
    assert!(source.contains("downloads.claude.ai/releases/darwin/universal"));
    assert!(!source.contains(&cask_variant));
    assert!(!source.contains(&cask_install));
    assert!(!source.contains(&cask_upgrade));
    assert!(!source.contains(&cask_uninstall));
    assert!(!registry.contains(&cask_install));
    assert!(!detector.contains(&cask_detection));
    assert!(!detector.contains(&cask_upgrade));
    assert_eq!(
        command_preview(&InstallAction::MacosDmgApp {
            label: "Claude Desktop official DMG",
            latest_url: CLAUDE_DESKTOP_LATEST_MACOS_URL,
            app_name: CLAUDE_DESKTOP_MACOS_APP_NAME,
            bundle_identifier: CLAUDE_DESKTOP_MACOS_BUNDLE_ID,
            destination: CLAUDE_DESKTOP_MACOS_DESTINATION,
        }),
        "Download and install the latest Claude Desktop official DMG from downloads.claude.ai"
    );
}

#[cfg(windows)]
#[test]
fn winget_commands_are_wrapped_in_elevated_powershell() {
    let args = windows_elevated_powershell_args(
        "C:\\Program Files\\WindowsApps\\winget.exe",
        &[
            "install",
            "--id",
            "OpenJS.NodeJS.LTS",
            "--exact",
            "--source",
            "O'Reilly",
        ],
    );
    let script = args.last().expect("powershell script");

    assert!(script.contains("Start-Process"));
    assert!(script.contains("-Verb RunAs"));
    assert!(script.contains("-Wait"));
    assert!(script.contains("-PassThru"));
    assert!(script.contains("'C:\\Program Files\\WindowsApps\\winget.exe'"));
    assert!(script.contains("'OpenJS.NodeJS.LTS'"));
    assert!(script.contains("'O''Reilly'"));
    assert!(script.contains("exit $process.ExitCode"));
}

#[test]
fn update_plan_allows_installed_tools_with_detected_updates() {
    let definition = install_definition("claude").expect("definition");
    let status = ToolStatus {
        id: "claude".to_string(),
        name: "Claude Code".to_string(),
        category: ToolCategory::AiTool,
        command: "claude".to_string(),
        path_repair: None,
        version: Some("2.1.126".to_string()),
        latest_version: Some("2.1.130".to_string()),
        update_available: true,
        update_command: Some("npm install -g @anthropic-ai/claude-code@latest".to_string()),
        install_state: InstallState::Installed,
        config_state: ConfigState::Configured,
        config_path: Some("~/.claude".to_string()),
        install_path: None,
        install_command: Some("npm install -g @anthropic-ai/claude-code".to_string()),
        details: Some("Ready".to_string()),
        install_kind: None,
        running: false,
    };

    let plan = build_update_plan(&definition, Some(&status));

    assert!(plan.can_install);
    assert!(plan.already_installed);
    assert_eq!(plan.commands.len(), 1);
    assert_eq!(plan.commands[0].stage, "update");
    assert!(plan.command.contains("@anthropic-ai/claude-code@latest"));
    assert!(plan.blocker.is_none());
}

#[test]
fn claude_desktop_windows_update_does_not_use_stale_winget_source() {
    let definition = install_definition("claude-desktop").expect("definition");
    let command = update_command_preview_for_tool("claude-desktop", &definition.action);

    if cfg!(target_os = "windows") {
        let architecture = crate::core::claude_desktop_release::windows_release_architecture()
            .expect("Windows release architecture");
        assert!(command.contains(&format!(
            "claude.ai/api/desktop/win32/{architecture}/msix/latest/redirect"
        )));
        assert!(command.contains("Add-AppxPackage -Path"));
        assert!(!command.contains("winget"));
        assert!(!command.contains("win32/x64/.latest"));
        assert!(!command.contains("Claude-$hash.exe"));
    } else {
        assert_eq!(command, update_command_preview(&definition.action));
    }
}

#[test]
fn claude_desktop_windows_download_urls_follow_native_architecture() {
    assert_eq!(
        crate::core::claude_desktop_release::claude_desktop_windows_latest_url_for_arch("arm64")
            .unwrap(),
        "https://downloads.claude.ai/releases/win32/arm64/.latest"
    );
    assert_eq!(
        crate::core::claude_desktop_release::claude_desktop_windows_latest_url_for_arch("x64")
            .unwrap(),
        "https://downloads.claude.ai/releases/win32/x64/.latest"
    );
    assert_eq!(
        crate::core::claude_desktop_release::claude_desktop_windows_msix_url_for_arch("arm64")
            .unwrap(),
        "https://claude.ai/api/desktop/win32/arm64/msix/latest/redirect"
    );
    assert_eq!(
        crate::core::claude_desktop_release::claude_desktop_windows_msix_url_for_arch("x64")
            .unwrap(),
        "https://claude.ai/api/desktop/win32/x64/msix/latest/redirect"
    );
    assert!(
        crate::core::claude_desktop_release::claude_desktop_windows_msix_url_for_arch("powerpc")
            .is_err()
    );
}

#[test]
fn claude_desktop_windows_msix_install_cleans_stale_exe_registry_entries_first() {
    let source = include_str!("tool_installer.rs");
    let production = source
        .split("#[cfg(test)]")
        .next()
        .expect("production section");
    let install_function = production
        .split("fn run_claude_desktop_windows_msix_install")
        .nth(1)
        .and_then(|body| {
            body.split("fn remove_stale_claude_desktop_windows_exe_uninstall_entries")
                .next()
        })
        .expect("MSIX install function");
    let cleanup_script = CLAUDE_DESKTOP_WINDOWS_STALE_EXE_UNINSTALL_CLEANUP_SCRIPT;

    let cleanup_index = install_function
        .find("remove_stale_claude_desktop_windows_exe_uninstall_entries")
        .expect("MSIX install should clean stale EXE uninstall entries");
    let download_index = install_function
        .find("download_url_to_file")
        .expect("MSIX install should download the official package");
    assert!(
        cleanup_index < download_index,
        "stale EXE registry cleanup must happen before downloading/installing the MSIX"
    );
    assert!(cleanup_script.contains("AnthropicClaude"));
    assert!(cleanup_script.contains("InstallLocation"));
    assert!(cleanup_script.contains("Test-ClaudeExeInstallAlive"));
    assert!(cleanup_script.contains("resources\\app.asar"));
    assert!(cleanup_script.contains("Keeping live Claude Desktop EXE uninstall entry"));
    assert!(cleanup_script.contains("Remove-Item -LiteralPath $prop.PSPath"));
    assert!(cleanup_script.contains("${keyName}: $($_.Exception.Message)"));
    assert!(!cleanup_script.contains("$keyName:"));
}

#[test]
fn claude_desktop_windows_uninstall_uses_specific_package_removal() {
    let definition = install_definition("claude-desktop").expect("definition");
    let command = uninstall_command_preview_for_tool("claude-desktop", &definition.action);

    if cfg!(target_os = "windows") {
        assert_eq!(command, CLAUDE_DESKTOP_WINDOWS_UNINSTALL_COMMAND);
        assert!(!command.contains("winget"));
    } else {
        assert!(!command.is_empty());
    }
}

#[test]
fn claude_desktop_windows_uninstall_scripts_delete_and_verify_install_files() {
    let exe_script = CLAUDE_DESKTOP_WINDOWS_EXE_UNINSTALL_SCRIPT;

    assert!(exe_script.contains("InstallLocation"));
    assert!(exe_script.contains("$installRoots"));
    assert!(exe_script.contains("Remove-Item -LiteralPath $root -Recurse -Force"));
    assert!(exe_script.contains("Test-Path -LiteralPath $root"));
    assert!(exe_script.contains("remaining install roots"));

    let source = include_str!("tool_installer.rs");
    assert!(source.contains("remove_claude_msix_payloads"));
    assert!(source.contains("remaining_payloads"));
    assert!(source.contains("MSIX/AppX package files remain"));
}

#[test]
fn claude_desktop_windows_uninstall_stops_cowork_service_before_file_cleanup() {
    let source = include_str!("tool_installer.rs");
    let production = source
        .split("#[cfg(test)]")
        .next()
        .expect("production section");
    let remove_service_function = production
        .split("fn remove_claude_desktop_windows_background_services")
        .nth(1)
        .and_then(|body| {
            body.split("fn run_claude_desktop_windows_service_script")
                .next()
        })
        .expect("service removal function");

    assert!(production.contains("CoworkVMService"));
    assert!(production.contains("cowork-svc"));
    assert!(production.contains("stop_claude_desktop_windows_background_services"));
    assert!(production.contains("remove_claude_desktop_windows_background_services"));
    assert!(remove_service_function.contains("sc.exe"));
    assert!(remove_service_function.contains("delete"));
    assert!(remove_service_function.contains("Start-Process"));
    assert!(remove_service_function.contains("-Verb RunAs"));
}

#[test]
fn claude_desktop_windows_uninstall_verifies_selected_install_kind_only() {
    let after_msix_uninstall = ClaudeDesktopInstallKinds {
        msix: DesktopInstallKindInfo {
            installed: false,
            version: None,
            path: None,
        },
        exe: DesktopInstallKindInfo {
            installed: true,
            version: Some("1.14271.0".to_string()),
            path: Some(r"C:\Users\test\AppData\Local\AnthropicClaude\Claude.exe".to_string()),
        },
    };
    assert!(claude_desktop_windows_uninstall_verified(
        Some("msix"),
        &after_msix_uninstall,
        false
    ));

    let after_exe_uninstall = ClaudeDesktopInstallKinds {
        msix: DesktopInstallKindInfo {
            installed: true,
            version: Some("1.14271.0".to_string()),
            path: Some(
                r"C:\Program Files\WindowsApps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc".to_string(),
            ),
        },
        exe: DesktopInstallKindInfo {
            installed: false,
            version: None,
            path: None,
        },
    };
    assert!(claude_desktop_windows_uninstall_verified(
        Some("exe"),
        &after_exe_uninstall,
        true
    ));
}

#[test]
fn claude_desktop_windows_uninstall_message_mentions_remaining_other_kind() {
    let after_msix_uninstall = ClaudeDesktopInstallKinds {
        msix: DesktopInstallKindInfo {
            installed: false,
            version: None,
            path: None,
        },
        exe: DesktopInstallKindInfo {
            installed: true,
            version: Some("1.14271.0".to_string()),
            path: Some(r"C:\Users\test\AppData\Local\AnthropicClaude\Claude.exe".to_string()),
        },
    };

    assert_eq!(
        claude_desktop_windows_uninstall_success_message(
            "Claude Desktop",
            Some("msix"),
            &after_msix_uninstall,
            false,
        ),
        "Claude Desktop MSIX install removed. Native EXE install is still present."
    );
}

#[test]
fn claude_desktop_windows_uninstall_message_separates_stale_msix_residue() {
    let after_exe_uninstall = ClaudeDesktopInstallKinds {
        msix: DesktopInstallKindInfo {
            installed: true,
            version: Some("1.14271.0".to_string()),
            path: Some(
                r"C:\Program Files\WindowsApps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc".to_string(),
            ),
        },
        exe: DesktopInstallKindInfo {
            installed: false,
            version: None,
            path: None,
        },
    };

    assert_eq!(
        claude_desktop_windows_uninstall_success_message(
            "Claude Desktop",
            Some("exe"),
            &after_exe_uninstall,
            false,
        ),
        "Claude Desktop native EXE install removed. MSIX/AppX residue is still detected."
    );
}

#[test]
fn winget_multiple_package_exit_code_gets_explained() {
    let stderr = describe_command_failure(
        "winget",
        Some(WINGET_MULTIPLE_PACKAGES_EXIT_CODE),
        "No installed package found matching input criteria.",
    );

    assert!(stderr.contains(WINGET_MULTIPLE_PACKAGES_HEX));
    assert!(stderr.contains("Multiple packages match"));
}

#[test]
fn install_progress_payload_keeps_root_tool_scope() {
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured_for_progress = captured.clone();
    let progress = move |progress| {
        captured_for_progress
            .lock()
            .expect("captured")
            .push(progress);
    };
    let context = InstallProgressContext {
        root_tool_id: "opencode",
        tool_id: "node",
        tool_name: "Node.js LTS",
        stage: "prerequisite",
        command: "winget install OpenJS.NodeJS.LTS",
        install_kind: None,
        progress_phase: None,
        progress_message: None,
        progress_step: None,
        progress_step_total: None,
        progress: Some(&progress),
    };

    emit_install_progress(
        Some(&context),
        "stdout",
        "installing\n".to_string(),
        None,
        false,
    );
    emit_install_progress(Some(&context), "status", String::new(), Some(0), true);

    let captured = captured.lock().expect("captured");
    assert_eq!(TOOL_INSTALL_PROGRESS_EVENT, "tool-install://progress");
    assert_eq!(captured[0].root_tool_id, "opencode");
    assert_eq!(captured[0].tool_id, "node");
    assert_eq!(captured[0].chunk, "installing\n");
    assert!(captured[1].done);
    assert_eq!(captured[1].exit_code, Some(0));
}
