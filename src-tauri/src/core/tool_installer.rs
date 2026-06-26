use crate::core::activity_log;
use crate::core::app_paths::app_paths;
use crate::core::detector;
use crate::core::env_health;
use crate::core::platform::{
    hidden_command, hidden_command_with_args, package, resolve_command, run_powershell,
};
use crate::core::process_control;
use crate::core::storage;
use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};
use crate::core::types::{
    ClaudeDesktopInstallKinds, ClaudeDesktopPlan, InstallState, RepairToolPathRequest,
    RepairToolPathResult, Severity, ToolInstallCommand, ToolInstallPlan, ToolInstallPrerequisite,
    ToolInstallProgress, ToolInstallRequest, ToolInstallResult, ToolInstallStageResult,
    ToolInstallStep, ToolStatus, ToolUninstallRequest,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
enum InstallAction {
    NpmGlobal(&'static str),
    Winget(&'static str),
    MacosDmgApp {
        label: &'static str,
        latest_url: &'static str,
        app_name: &'static str,
        bundle_identifier: &'static str,
        destination: &'static str,
    },
    ClaudeDesktopWindowsMsix,
    // Reserved install action for a bundled PowerShell script. The match arms
    // across plan/run/preview already handle it; no tool currently constructs
    // it, so it is intentionally dead until a tool opts in.
    #[allow(dead_code)]
    PowerShellScript(&'static str, &'static str),
    ShellScript(&'static str, &'static str),
    InteractiveShellScript(&'static str, &'static str),
    VsCodeExtension(&'static str),
    ProvidedByTool(&'static str),
    CustomUnsupported(&'static str),
}

#[derive(Debug, Clone)]
struct InstallDefinition {
    tool: ToolDefinition,
    action: InstallAction,
}

#[derive(Debug)]
struct InstallCommandOutput {
    success: bool,
    exit_code: Option<i32>,
    stdout_tail: String,
    stderr_tail: String,
    missing_command: Option<String>,
}

pub const TOOL_INSTALL_PROGRESS_EVENT: &str = "tool-install://progress";

pub type ToolInstallProgressEmitter = dyn Fn(ToolInstallProgress) + Send + Sync;

struct InstallProgressContext<'a> {
    root_tool_id: &'a str,
    tool_id: &'a str,
    tool_name: &'a str,
    stage: &'a str,
    command: &'a str,
    install_kind: Option<&'a str>,
    progress_phase: Option<&'a str>,
    progress_message: Option<&'a str>,
    progress_step: Option<u32>,
    progress_step_total: Option<u32>,
    progress: Option<&'a ToolInstallProgressEmitter>,
}

pub fn plan_tool_install(tool_id: &str) -> Result<ToolInstallPlan, String> {
    let definition = install_definition(tool_id)
        .ok_or_else(|| format!("Tool '{tool_id}' is not allowed for installation."))?;
    let current_status = current_status(tool_id).ok();
    Ok(build_plan(&definition, current_status.as_ref()))
}

pub fn plan_tool_update(tool_id: &str) -> Result<ToolInstallPlan, String> {
    let definition = install_definition(tool_id)
        .ok_or_else(|| format!("Tool '{tool_id}' is not allowed for updates."))?;
    let current_status = current_status(tool_id).ok();
    Ok(build_update_plan(&definition, current_status.as_ref()))
}

pub fn plan_claude_desktop_update() -> Result<ClaudeDesktopPlan, String> {
    let status = current_status("claude-desktop").ok();
    build_claude_desktop_plan(status.as_ref())
}

pub fn install_tool(request: ToolInstallRequest) -> Result<ToolInstallResult, String> {
    install_tool_with_progress(request, None)
}

pub fn install_tool_with_progress(
    request: ToolInstallRequest,
    progress: Option<&ToolInstallProgressEmitter>,
) -> Result<ToolInstallResult, String> {
    if !request.confirm {
        return Err("Refused: installing software requires explicit confirmation.".to_string());
    }

    let tool_id = request.tool_id.clone();
    let definition = install_definition(&tool_id)
        .ok_or_else(|| format!("Tool '{}' is not allowed for installation.", tool_id))?;
    let before = current_status(&tool_id).ok();
    let plan = build_plan(&definition, before.as_ref());
    if !plan.can_install {
        return Ok(ToolInstallResult {
            success: plan.already_installed,
            tool_id: plan.tool_id,
            tool_name: plan.tool_name,
            action: if plan.already_installed {
                "already-installed".to_string()
            } else {
                "blocked".to_string()
            },
            message: plan
                .blocker
                .unwrap_or_else(|| "The install plan cannot be executed.".to_string()),
            command: plan.command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: Vec::new(),
        });
    }

    if plan.requires_prerequisites && !request.install_prerequisites {
        return Ok(ToolInstallResult {
            success: false,
            tool_id: plan.tool_id,
            tool_name: plan.tool_name,
            action: "prerequisites-required".to_string(),
            message: "This tool requires prerequisites. Allow prerequisite installation before continuing.".to_string(),
            command: plan.command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: Vec::new(),
        });
    }

    let _ = activity_log::append(
        Severity::Info,
        format!(
            "Started install for {} using {}.",
            plan.tool_name, plan.manager
        ),
    );

    let mut stage_results = Vec::new();
    let mut notes = Vec::new();

    for prerequisite in &plan.prerequisites {
        if prerequisite.installed {
            continue;
        }

        let prerequisite_definition =
            install_definition(&prerequisite.tool_id).ok_or_else(|| {
                format!(
                    "Prerequisite '{}' is not allowed for installation.",
                    prerequisite.tool_id
                )
            })?;
        let command = command_preview(&prerequisite_definition.action);
        let context = InstallProgressContext {
            root_tool_id: &tool_id,
            tool_id: &prerequisite.tool_id,
            tool_name: &prerequisite.tool_name,
            stage: "prerequisite",
            command: &command,
            install_kind: request.install_kind.as_deref(),
            progress_phase: None,
            progress_message: None,
            progress_step: None,
            progress_step_total: None,
            progress,
        };
        let output = run_install_action(&prerequisite_definition.action, Some(&context))?;
        let missing_command = output.missing_command.clone();
        if output.success {
            refresh_process_environment_after_install(&mut notes);
        }
        let verified = dependency_satisfied(&definition.action);
        let success = output.success && verified;
        let message = if success {
            format!("{} prerequisite installed.", prerequisite.tool_name)
        } else if output.success {
            format!(
                "{} install command finished, but the prerequisite command required by the target tool was not detected. Check PATH or install logs, then refresh.",
                prerequisite.tool_name
            )
        } else {
            format!(
                "{} prerequisite installation failed.",
                prerequisite.tool_name
            )
        };
        stage_results.push(ToolInstallStageResult {
            tool_id: prerequisite.tool_id.clone(),
            tool_name: prerequisite.tool_name.clone(),
            stage: "prerequisite".to_string(),
            command,
            success,
            exit_code: output.exit_code,
            stdout_tail: output.stdout_tail,
            stderr_tail: output.stderr_tail,
            message: message.clone(),
        });

        if !success {
            let _ = activity_log::append(Severity::Warning, message.clone());
            return Ok(ToolInstallResult {
                success: false,
                tool_id,
                tool_name: definition.tool.name.to_string(),
                action: "prerequisite-failed".to_string(),
                message,
                command: plan.command,
                exit_code: stage_results.last().and_then(|stage| stage.exit_code),
                stdout_tail: stage_results
                    .last()
                    .map(|stage| stage.stdout_tail.clone())
                    .unwrap_or_default(),
                stderr_tail: stage_results
                    .last()
                    .map(|stage| stage.stderr_tail.clone())
                    .unwrap_or_default(),
                current_status: current_status_for_missing_command(
                    missing_command.as_deref(),
                    &definition.tool.id,
                ),
                stage_results,
                notes,
            });
        }
    }

    let command = command_preview(&definition.action);
    let context = InstallProgressContext {
        root_tool_id: &tool_id,
        tool_id: &tool_id,
        tool_name: definition.tool.name,
        stage: "target",
        command: &command,
        install_kind: request.install_kind.as_deref(),
        progress_phase: None,
        progress_message: None,
        progress_step: None,
        progress_step_total: None,
        progress,
    };
    let output = run_install_action(&definition.action, Some(&context))?;
    if output.success {
        refresh_process_environment_after_install(&mut notes);
    }

    detector::invalidate_update_cache();
    let after = current_status_for_missing_command(output.missing_command.as_deref(), &tool_id);
    let verified = after
        .as_ref()
        .map(|status| status.install_state == InstallState::Installed)
        .unwrap_or(false);
    let process_success = output.success;
    let success = process_success && verified;
    let exit_code = output.exit_code;
    let message = if success {
        format!("{} installed and verified.", definition.tool.name)
    } else if process_success {
        format!(
            "{} install command finished, but verification still did not confirm it is available. Check PATH or install logs, then refresh.",
            definition.tool.name
        )
    } else {
        format!("{} installation failed.", definition.tool.name)
    };
    let level = if success {
        Severity::Ok
    } else {
        Severity::Warning
    };
    let _ = activity_log::append(level, message.clone());

    stage_results.push(ToolInstallStageResult {
        tool_id: tool_id.clone(),
        tool_name: definition.tool.name.to_string(),
        stage: "target".to_string(),
        command,
        success,
        exit_code,
        stdout_tail: output.stdout_tail.clone(),
        stderr_tail: output.stderr_tail.clone(),
        message: message.clone(),
    });

    Ok(ToolInstallResult {
        success,
        tool_id,
        tool_name: definition.tool.name.to_string(),
        action: manager_label(&definition.action).to_string(),
        message,
        command: plan.command,
        exit_code,
        stdout_tail: output.stdout_tail,
        stderr_tail: output.stderr_tail,
        current_status: after,
        stage_results,
        notes,
    })
}

pub fn update_tool(request: ToolInstallRequest) -> Result<ToolInstallResult, String> {
    update_tool_with_progress(request, None)
}

pub fn uninstall_tool(request: ToolUninstallRequest) -> Result<ToolInstallResult, String> {
    uninstall_tool_with_progress(request, None)
}

pub fn update_tool_with_progress(
    request: ToolInstallRequest,
    progress: Option<&ToolInstallProgressEmitter>,
) -> Result<ToolInstallResult, String> {
    if !request.confirm {
        return Err("Refused: updating software requires explicit confirmation.".to_string());
    }

    let tool_id = request.tool_id.clone();
    let definition = install_definition(&tool_id)
        .ok_or_else(|| format!("Tool '{}' is not allowed for updates.", tool_id))?;
    let before = current_status(&tool_id).ok();
    let command = update_command_preview_for_tool(&tool_id, &definition.action);

    if before
        .as_ref()
        .map(|status| status.install_state != InstallState::Installed)
        .unwrap_or(true)
    {
        return Ok(ToolInstallResult {
            success: false,
            tool_id,
            tool_name: definition.tool.name.to_string(),
            action: "blocked".to_string(),
            message: format!(
                "{} is not installed and cannot be updated.",
                definition.tool.name
            ),
            command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: Vec::new(),
        });
    }

    if !update_supported_for_tool(&tool_id, &definition.action) {
        return Ok(ToolInstallResult {
            success: false,
            tool_id,
            tool_name: definition.tool.name.to_string(),
            action: "blocked".to_string(),
            message: format!(
                "{} does not have a built-in update action.",
                definition.tool.name
            ),
            command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: Vec::new(),
        });
    }

    let _ = activity_log::append(
        Severity::Info,
        format!("Started update for {}.", definition.tool.name),
    );

    let mut notes = Vec::new();
    let termination = close_processes_before_update(&tool_id, definition.tool.name)?;
    if let Some(note) = termination.note(definition.tool.name) {
        let _ = activity_log::append(Severity::Info, note.clone());
        notes.push(note);
    }

    let context = InstallProgressContext {
        root_tool_id: &tool_id,
        tool_id: &tool_id,
        tool_name: definition.tool.name,
        stage: "update",
        command: &command,
        install_kind: request
            .install_kind
            .as_deref()
            .or_else(|| before.as_ref().and_then(|s| s.install_kind.as_deref())),
        progress_phase: None,
        progress_message: None,
        progress_step: None,
        progress_step_total: None,
        progress,
    };
    let output = run_update_action_for_tool(&tool_id, &definition.action, Some(&context))?;
    if output.success {
        refresh_process_environment_after_install(&mut notes);
    }
    detector::invalidate_update_cache();
    let after = current_status_for_missing_command(output.missing_command.as_deref(), &tool_id);
    let verified = after
        .as_ref()
        .map(|status| status.install_state == InstallState::Installed)
        .unwrap_or(false);
    let success = output.success && verified;
    let exit_code = output.exit_code;
    let message = if success {
        format!(
            "{} update command completed and verified.",
            definition.tool.name
        )
    } else if output.success {
        format!(
            "{} update command finished, but verification still did not confirm it is available. Check PATH or install logs, then refresh.",
            definition.tool.name
        )
    } else {
        format!("{} update failed.", definition.tool.name)
    };
    let level = if success {
        Severity::Ok
    } else {
        Severity::Warning
    };
    let _ = activity_log::append(level, message.clone());

    let stage_results = vec![ToolInstallStageResult {
        tool_id: tool_id.clone(),
        tool_name: definition.tool.name.to_string(),
        stage: "update".to_string(),
        command: command.clone(),
        success,
        exit_code,
        stdout_tail: output.stdout_tail.clone(),
        stderr_tail: output.stderr_tail.clone(),
        message: message.clone(),
    }];

    Ok(ToolInstallResult {
        success,
        tool_id,
        tool_name: definition.tool.name.to_string(),
        action: "update".to_string(),
        message,
        command,
        exit_code,
        stdout_tail: output.stdout_tail,
        stderr_tail: output.stderr_tail,
        current_status: after,
        stage_results,
        notes,
    })
}

pub fn uninstall_tool_with_progress(
    request: ToolUninstallRequest,
    progress: Option<&ToolInstallProgressEmitter>,
) -> Result<ToolInstallResult, String> {
    if !request.confirm {
        return Err("Refused: uninstalling software requires explicit confirmation.".to_string());
    }

    let tool_id = request.tool_id.clone();
    let definition = install_definition(&tool_id)
        .ok_or_else(|| format!("Tool '{}' is not allowed for uninstallation.", tool_id))?;
    let before = current_status(&tool_id).ok();
    let command = uninstall_command_preview_for_tool(&tool_id, &definition.action);

    if before
        .as_ref()
        .map(|status| status.install_state != InstallState::Installed)
        .unwrap_or(true)
    {
        return Ok(ToolInstallResult {
            success: false,
            tool_id,
            tool_name: definition.tool.name.to_string(),
            action: "blocked".to_string(),
            message: format!(
                "{} is not installed and cannot be uninstalled.",
                definition.tool.name
            ),
            command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: Vec::new(),
        });
    }

    if !uninstall_supported_for_tool(&tool_id, &definition.action) {
        return Ok(ToolInstallResult {
            success: false,
            tool_id,
            tool_name: definition.tool.name.to_string(),
            action: "blocked".to_string(),
            message: format!(
                "{} does not have a built-in uninstall action.",
                definition.tool.name
            ),
            command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: Vec::new(),
        });
    }

    let _ = activity_log::append(
        Severity::Info,
        format!("Started uninstall for {}.", definition.tool.name),
    );

    let mut notes = Vec::new();
    let termination = close_processes_before_update(&tool_id, definition.tool.name)?;
    if let Some(note) = termination.note(definition.tool.name) {
        let _ = activity_log::append(Severity::Info, note.clone());
        notes.push(note);
    }

    let context = InstallProgressContext {
        root_tool_id: &tool_id,
        tool_id: &tool_id,
        tool_name: definition.tool.name,
        stage: "uninstall",
        command: &command,
        install_kind: request
            .install_kind
            .as_deref()
            .or_else(|| before.as_ref().and_then(|s| s.install_kind.as_deref())),
        progress_phase: None,
        progress_message: None,
        progress_step: None,
        progress_step_total: None,
        progress,
    };
    // Prefer the install kind the caller selected (per the page tab) over the
    // detected one, so uninstalling targets the version the user is viewing.
    let requested_install_kind = request
        .install_kind
        .as_deref()
        .or_else(|| before.as_ref().and_then(|s| s.install_kind.as_deref()));
    let claude_windows_install_kind = if tool_id == "claude-desktop" && cfg!(target_os = "windows")
    {
        let install_kinds = detector::claude_desktop_install_kinds();
        resolve_claude_desktop_windows_uninstall_kind(requested_install_kind, &install_kinds)
    } else {
        requested_install_kind.map(ToString::to_string)
    };
    let output = if tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        run_claude_desktop_windows_uninstall(
            claude_windows_install_kind.as_deref(),
            Some(&context),
        )?
    } else {
        run_uninstall_action_for_tool(&tool_id, &definition.action, Some(&context))?
    };
    if output.success {
        refresh_process_environment_after_install(&mut notes);
    }

    detector::invalidate_update_cache();
    let after = current_status_for_missing_command(output.missing_command.as_deref(), &tool_id);
    let uninstalled = if tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        let install_kinds = detector::claude_desktop_install_kinds();
        claude_desktop_windows_uninstall_verified(
            claude_windows_install_kind.as_deref(),
            &install_kinds,
            detector::claude_desktop_windows_registered_msix_installed(),
        )
    } else {
        after
            .as_ref()
            .map(|status| status.install_state != InstallState::Installed)
            .unwrap_or(true)
    };
    let success = output.success && uninstalled;
    let message = if success && tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        let install_kinds = detector::claude_desktop_install_kinds();
        claude_desktop_windows_uninstall_success_message(
            definition.tool.name,
            claude_windows_install_kind.as_deref(),
            &install_kinds,
            detector::claude_desktop_windows_registered_msix_installed(),
        )
    } else if success {
        format!("{} uninstalled.", definition.tool.name)
    } else if output.success {
        format!(
            "{} uninstall command finished, but verification still detects it. Check install state and refresh.",
            definition.tool.name
        )
    } else {
        format!("{} uninstall failed.", definition.tool.name)
    };
    let level = if success {
        Severity::Ok
    } else {
        Severity::Warning
    };
    let _ = activity_log::append(level, message.clone());

    let stage = ToolInstallStageResult {
        tool_id: tool_id.clone(),
        tool_name: definition.tool.name.to_string(),
        stage: "uninstall".to_string(),
        command: command.clone(),
        success,
        exit_code: output.exit_code,
        stdout_tail: output.stdout_tail.clone(),
        stderr_tail: output.stderr_tail.clone(),
        message: message.clone(),
    };

    Ok(ToolInstallResult {
        success,
        tool_id,
        tool_name: definition.tool.name.to_string(),
        action: "uninstall".to_string(),
        message,
        command,
        exit_code: output.exit_code,
        stdout_tail: output.stdout_tail,
        stderr_tail: output.stderr_tail,
        current_status: after,
        stage_results: vec![stage],
        notes,
    })
}

pub fn repair_tool_path(request: RepairToolPathRequest) -> Result<RepairToolPathResult, String> {
    env_health::repair_tool_path(request)
}

pub fn open_claude_desktop_path(kind: String) -> Result<(), String> {
    let target = match kind.as_str() {
        "staging" => claude_desktop_download_dir()?,
        _ => return Err("Unknown Claude Desktop path type.".to_string()),
    };
    open_folder(&target)
}

fn install_definition(tool_id: &str) -> Option<InstallDefinition> {
    let tool = ai_tools()
        .into_iter()
        .chain(system_tools())
        .find(|tool| tool.id == tool_id)?;
    let action = match tool.id {
        "codex" => InstallAction::NpmGlobal("@openai/codex"),
        "codex-vscode" => InstallAction::VsCodeExtension("openai.chatgpt"),
        "claude" => InstallAction::NpmGlobal("@anthropic-ai/claude-code"),
        "claude-desktop" => {
            if cfg!(target_os = "macos") {
                InstallAction::MacosDmgApp {
                    label: "Claude Desktop official DMG",
                    latest_url: CLAUDE_DESKTOP_LATEST_MACOS_URL,
                    app_name: CLAUDE_DESKTOP_MACOS_APP_NAME,
                    bundle_identifier: CLAUDE_DESKTOP_MACOS_BUNDLE_ID,
                    destination: CLAUDE_DESKTOP_MACOS_DESTINATION,
                }
            } else if cfg!(target_os = "windows") {
                InstallAction::ClaudeDesktopWindowsMsix
            } else {
                InstallAction::CustomUnsupported("Claude Desktop has no built-in Linux installer.")
            }
        }
        "claude-vscode" => InstallAction::VsCodeExtension("anthropic.claude-code"),
        "gemini" => InstallAction::NpmGlobal("@google/gemini-cli"),
        "gemini-code-assist" => InstallAction::VsCodeExtension("Google.geminicodeassist"),
        "opencode" => InstallAction::NpmGlobal("opencode-ai"),
        "openclaw" => InstallAction::NpmGlobal("openclaw"),
        "hermes" => {
            if cfg!(target_os = "macos") {
                InstallAction::InteractiveShellScript(
                    "Hermes official install script",
                    HERMES_UNIX_INSTALL_COMMAND,
                )
            } else if cfg!(target_os = "linux") {
                InstallAction::InteractiveShellScript(
                    "Hermes official install script",
                    HERMES_UNIX_INSTALL_COMMAND,
                )
            } else {
                InstallAction::InteractiveShellScript(
                    "Hermes official install script",
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm https://hermes-agent.nousresearch.com/install.ps1)\"",
                )
            }
        }
        "node" => {
            if cfg!(target_os = "macos") {
                InstallAction::InteractiveShellScript(
                    "Node.js official macOS pkg installer",
                    NODE_MACOS_OFFICIAL_PKG_INSTALL_COMMAND,
                )
            } else if cfg!(target_os = "linux") {
                InstallAction::ShellScript(
                    "NodeSource Node.js LTS install script",
                    "curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash - && sudo apt-get install -y nodejs",
                )
            } else {
                InstallAction::Winget("OpenJS.NodeJS.LTS")
            }
        }
        "git" => {
            if cfg!(target_os = "macos") {
                InstallAction::InteractiveShellScript(
                    "Apple Command Line Tools installer",
                    GIT_MACOS_COMMAND_LINE_TOOLS_INSTALL_COMMAND,
                )
            } else if cfg!(target_os = "linux") {
                InstallAction::ShellScript(
                    "APT Git install command",
                    "sudo apt-get update && sudo apt-get install -y git",
                )
            } else {
                InstallAction::Winget("Git.Git")
            }
        }
        "pnpm" => InstallAction::NpmGlobal("pnpm"),
        "bun" => {
            if cfg!(target_os = "macos") {
                InstallAction::InteractiveShellScript(
                    "Bun official install script",
                    BUN_UNIX_INSTALL_COMMAND,
                )
            } else if cfg!(target_os = "linux") {
                InstallAction::ShellScript("Bun official install script", BUN_UNIX_INSTALL_COMMAND)
            } else {
                InstallAction::Winget("Oven-sh.Bun")
            }
        }
        "npm" => InstallAction::ProvidedByTool("node"),
        _ => InstallAction::CustomUnsupported("No built-in installer is available."),
    };
    Some(InstallDefinition { tool, action })
}

fn build_plan(
    definition: &InstallDefinition,
    detected_status: Option<&ToolStatus>,
) -> ToolInstallPlan {
    let already_installed = detected_status
        .map(|status| status.install_state == InstallState::Installed)
        .unwrap_or(false);
    let manager = manager_label(&definition.action).to_string();
    let mut prerequisites = Vec::new();
    let mut steps = Vec::new();
    let warnings = Vec::new();
    let mut blocker = None;
    let mut can_install = !already_installed;

    if already_installed {
        blocker = Some(format!("{} is already installed.", definition.tool.name));
        can_install = false;
    }

    match &definition.action {
        InstallAction::NpmGlobal(package) => {
            steps.push(ToolInstallStep {
                label: "Check npm".to_string(),
                detail: "Local npm must be available; npm usually ships with Node.js LTS."
                    .to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Install global package".to_string(),
                detail: format!("Run npm install -g {package}."),
            });
            steps.push(ToolInstallStep {
                label: "Verify command".to_string(),
                detail: format!(
                    "After installation, run {} --version and refresh the dashboard.",
                    definition.tool.command
                ),
            });
            if !command_available("npm") {
                let node_definition = install_definition("node");
                let node_installed = current_status("node")
                    .map(|status| status.install_state == InstallState::Installed)
                    .unwrap_or(false);
                let node_can_install = node_definition
                    .as_ref()
                    .map(|definition| dependency_satisfied(&definition.action))
                    .unwrap_or(false);
                let node_manager = node_definition
                    .as_ref()
                    .map(|definition| manager_label(&definition.action))
                    .unwrap_or("manual");
                let node_command = node_definition
                    .as_ref()
                    .map(|definition| command_preview(&definition.action))
                    .unwrap_or_else(|| "Install Node.js LTS".to_string());
                prerequisites.push(ToolInstallPrerequisite {
                    tool_id: "node".to_string(),
                    tool_name: "Node.js LTS".to_string(),
                    manager: node_manager.to_string(),
                    command: node_command,
                    installed: node_installed,
                    can_install: node_can_install,
                    reason: "The target tool requires npm; npm is provided by Node.js LTS."
                        .to_string(),
                });
                steps.insert(
                    0,
                    ToolInstallStep {
                        label: "Install prerequisite".to_string(),
                        detail: format!(
                            "npm is not available; if allowed, Node.js LTS will be installed through {} first.",
                            node_manager
                        )
                        .to_string(),
                    },
                );
                if !node_can_install {
                    blocker = Some(
                        "npm is not available, and the current platform has no automatic Node.js installer; prerequisites cannot be installed automatically."
                            .to_string(),
                    );
                    can_install = false;
                }
            }
        }
        InstallAction::Winget(package_id) => {
            steps.push(ToolInstallStep {
                label: "Check winget".to_string(),
                detail: "Windows App Installer / winget must be available.".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Install package".to_string(),
                detail: format!("Install {package_id} through winget."),
            });
            steps.push(ToolInstallStep {
                label: "Verify command".to_string(),
                detail: format!(
                    "After installation, run {} --version and refresh the dashboard.",
                    definition.tool.command
                ),
            });
            if !cfg!(target_os = "windows") {
                blocker = Some("The winget installer is only supported on Windows.".to_string());
                can_install = false;
            } else if !command_available("winget") {
                blocker = Some(
                    "winget is not available. Install or repair Windows App Installer first."
                        .to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::MacosDmgApp {
            label,
            app_name,
            destination,
            ..
        } => {
            steps.push(ToolInstallStep {
                label: "Fetch official release".to_string(),
                detail: format!("Read the latest {label} metadata from downloads.claude.ai."),
            });
            steps.push(ToolInstallStep {
                label: "Install DMG".to_string(),
                detail: format!("Mount the official DMG and copy {app_name} to {destination}."),
            });
            steps.push(ToolInstallStep {
                label: "Verify app".to_string(),
                detail: format!(
                    "After installation, check whether {} is available.",
                    definition.tool.name
                ),
            });
            if !cfg!(target_os = "macos") {
                blocker = Some(
                    "The official macOS DMG installer is only supported on macOS.".to_string(),
                );
                can_install = false;
            } else if !macos_dmg_dependencies_available() {
                blocker = Some(
                    "hdiutil or ditto is unavailable, so the official DMG cannot be installed."
                        .to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::ClaudeDesktopWindowsMsix => {
            steps.push(ToolInstallStep {
                label: "Download official MSIX".to_string(),
                detail: "Download the latest Claude Desktop Windows App package from claude.ai."
                    .to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Install MSIX".to_string(),
                detail: "Install Claude.msix with Add-AppxPackage.".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Verify Windows App".to_string(),
                detail: "After installation, verify the Claude Desktop MSIX package registration."
                    .to_string(),
            });
            if !cfg!(target_os = "windows") {
                blocker = Some(
                    "The official Claude Desktop MSIX installer is only supported on Windows."
                        .to_string(),
                );
                can_install = false;
            } else if !powershell_available() {
                blocker = Some(
                    "PowerShell is not available, so Claude Desktop MSIX cannot be installed."
                        .to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::PowerShellScript(label, script) => {
            steps.push(ToolInstallStep {
                label: "Check PowerShell".to_string(),
                detail: "Local PowerShell must be available.".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Run official install script".to_string(),
                detail: format!("Run {label}: {script}."),
            });
            steps.push(ToolInstallStep {
                label: "Verify command".to_string(),
                detail: format!(
                    "After installation, run {} --version and refresh the dashboard.",
                    definition.tool.command
                ),
            });
            if !cfg!(target_os = "windows") {
                blocker = Some(
                    "This PowerShell install script is currently enabled only on Windows."
                        .to_string(),
                );
                can_install = false;
            } else if !powershell_available() {
                blocker = Some(
                    "PowerShell is not available, so the official install script cannot run."
                        .to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::ShellScript(label, script) => {
            steps.push(ToolInstallStep {
                label: "Check shell".to_string(),
                detail: "Local bash must be available.".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Run install script".to_string(),
                detail: format!("Run {label}: {script}."),
            });
            steps.push(ToolInstallStep {
                label: "Verify command".to_string(),
                detail: format!(
                    "After installation, run {} --version and refresh the dashboard.",
                    definition.tool.command
                ),
            });
            if !cfg!(target_os = "linux") {
                blocker = Some(
                    "This shell install route is currently enabled only on Linux.".to_string(),
                );
                can_install = false;
            } else if !command_available("bash") {
                blocker = Some(
                    "bash is not available, so the Linux install script cannot run.".to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::InteractiveShellScript(label, script) => {
            steps.push(ToolInstallStep {
                label: "Open interactive terminal".to_string(),
                detail: "The installer may ask for choices, credentials, or shell confirmation."
                    .to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Run official install script".to_string(),
                detail: format!("Run {label}: {script}."),
            });
            steps.push(ToolInstallStep {
                label: "Verify command".to_string(),
                detail: format!(
                    "After installation, run {} --version and refresh the dashboard.",
                    definition.tool.command
                ),
            });
            if cfg!(target_os = "linux") && !command_available("bash") {
                blocker = Some(
                    "bash is not available, so the interactive install script cannot run."
                        .to_string(),
                );
                can_install = false;
            } else if cfg!(target_os = "windows") && !powershell_available() {
                blocker = Some(
                    "PowerShell is not available, so the interactive install script cannot run."
                        .to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::VsCodeExtension(extension_id) => {
            steps.push(ToolInstallStep {
                label: "Check VS Code CLI".to_string(),
                detail: "The local code command must be available.".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Install VS Code extension".to_string(),
                detail: format!("Run code --install-extension {extension_id}."),
            });
            steps.push(ToolInstallStep {
                label: "Verify extension".to_string(),
                detail: "After installation, run code --list-extensions --show-versions and refresh the dashboard."
                    .to_string(),
            });
            if !command_available("code") {
                blocker = Some(
                    "VS Code CLI is not available. Install VS Code first, or enable the code command in VS Code."
                        .to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::ProvidedByTool(provider_tool_id) => {
            let provider_definition = install_definition(provider_tool_id);
            let provider_status = current_status(provider_tool_id).ok();
            let provider_installed = provider_status
                .as_ref()
                .map(|status| status.install_state == InstallState::Installed)
                .unwrap_or(false);
            if let Some(provider_definition) = provider_definition.as_ref() {
                let provider_plan = build_plan(provider_definition, provider_status.as_ref());
                steps.push(ToolInstallStep {
                    label: "Install upstream dependency".to_string(),
                    detail: format!(
                        "{} is provided by {}; install {} to make {} available.",
                        definition.tool.name,
                        provider_definition.tool.name,
                        provider_definition.tool.name,
                        definition.tool.name
                    ),
                });
                steps.push(ToolInstallStep {
                    label: "Verify command".to_string(),
                    detail: format!(
                        "After installation, run {} --version and refresh the dashboard.",
                        definition.tool.command
                    ),
                });
                if provider_installed {
                    steps.push(ToolInstallStep {
                        label: "Repair bundled command".to_string(),
                        detail: format!(
                            "{} appears to be installed, but {} was not found. Reinstalling {} can repair the bundled command and PATH registration.",
                            provider_definition.tool.name,
                            definition.tool.name,
                            provider_definition.tool.name
                        ),
                    });
                } else if !provider_plan.can_install {
                    blocker = Some(format!(
                        "{} is provided by {}, but {} cannot be installed automatically. {}",
                        definition.tool.name,
                        provider_definition.tool.name,
                        provider_definition.tool.name,
                        provider_plan
                            .blocker
                            .unwrap_or_else(|| "No install route is available.".to_string())
                    ));
                    can_install = false;
                }
            } else {
                blocker = Some(format!(
                    "{} is provided by {provider_tool_id}, but that installer is unavailable.",
                    definition.tool.name
                ));
                can_install = false;
            }
        }
        InstallAction::CustomUnsupported(reason) => {
            blocker = Some(reason.to_string());
            can_install = false;
        }
    }

    let mut commands = prerequisites
        .iter()
        .filter(|prerequisite| !prerequisite.installed)
        .filter_map(|prerequisite| install_definition(&prerequisite.tool_id))
        .map(|definition| command_entry(&definition, "prerequisite"))
        .collect::<Vec<_>>();
    if let InstallAction::ProvidedByTool(provider_tool_id) = &definition.action {
        if let Some(provider_definition) = install_definition(provider_tool_id) {
            commands.push(provider_command_entry(definition, &provider_definition));
        } else {
            commands.push(command_entry(definition, "target"));
        }
    } else {
        commands.push(command_entry(definition, "target"));
    }
    let requires_prerequisites = prerequisites
        .iter()
        .any(|prerequisite| !prerequisite.installed);
    let requires_admin = action_requires_admin_for_tool(definition)
        || prerequisites
            .iter()
            .filter(|prerequisite| !prerequisite.installed)
            .filter_map(|prerequisite| install_definition(&prerequisite.tool_id))
            .any(|definition| action_requires_admin(&definition.action));
    let interactive = action_interactive_for_tool(definition)
        || prerequisites
            .iter()
            .filter(|prerequisite| !prerequisite.installed)
            .filter_map(|prerequisite| install_definition(&prerequisite.tool_id))
            .any(|definition| action_interactive(&definition.action));

    ToolInstallPlan {
        tool_id: definition.tool.id.to_string(),
        tool_name: definition.tool.name.to_string(),
        manager,
        command: commands
            .iter()
            .map(|command| command.command.clone())
            .collect::<Vec<_>>()
            .join(" && "),
        interactive,
        commands,
        requires_prerequisites,
        prerequisites,
        can_install,
        already_installed,
        requires_admin,
        steps,
        warnings,
        blocker,
    }
}

fn build_update_plan(
    definition: &InstallDefinition,
    detected_status: Option<&ToolStatus>,
) -> ToolInstallPlan {
    let installed = detected_status
        .map(|status| status.install_state == InstallState::Installed)
        .unwrap_or(false);
    let update_detected = detected_status
        .map(|status| status.update_available)
        .unwrap_or(false);
    let supported = update_supported_for_tool(definition.tool.id, &definition.action);
    let command = update_command_preview_for_tool(definition.tool.id, &definition.action);
    let manager = manager_label(&definition.action).to_string();
    let mut blocker = None;

    if !installed {
        blocker = Some(format!(
            "{} is not installed and cannot be updated.",
            definition.tool.name
        ));
    } else if !supported {
        blocker = Some(format!(
            "{} does not have a built-in update action.",
            definition.tool.name
        ));
    } else if action_interactive(&definition.action) {
        blocker = Some(format!(
            "{} requires an interactive installer and cannot be updated from this dialog yet.",
            definition.tool.name
        ));
    } else if !update_detected {
        blocker = Some(format!(
            "{} has no detected update right now.",
            definition.tool.name
        ));
    }

    let can_install =
        installed && supported && !action_interactive(&definition.action) && update_detected;
    let command_entry = ToolInstallCommand {
        tool_id: definition.tool.id.to_string(),
        tool_name: definition.tool.name.to_string(),
        stage: "update".to_string(),
        manager: manager.clone(),
        command: command.clone(),
        requires_admin: action_requires_admin(&definition.action),
        interactive: false,
    };

    ToolInstallPlan {
        tool_id: definition.tool.id.to_string(),
        tool_name: definition.tool.name.to_string(),
        manager,
        command,
        interactive: false,
        commands: vec![command_entry],
        prerequisites: Vec::new(),
        requires_prerequisites: false,
        can_install,
        already_installed: installed,
        requires_admin: action_requires_admin(&definition.action),
        steps: vec![
            ToolInstallStep {
                label: "Check installed app".to_string(),
                detail: format!(
                    "Confirm {} is installed before updating.",
                    definition.tool.name
                ),
            },
            ToolInstallStep {
                label: "Run update command".to_string(),
                detail: "Close the target app if needed, then run the update command.".to_string(),
            },
            ToolInstallStep {
                label: "Verify version".to_string(),
                detail: "Refresh detection after the update command finishes.".to_string(),
            },
        ],
        warnings: Vec::new(),
        blocker,
    }
}

fn build_claude_desktop_plan(
    detected_status: Option<&ToolStatus>,
) -> Result<ClaudeDesktopPlan, String> {
    let download_url = claude_desktop_plan_download_url()?;
    let sha256 = load_cached_claude_desktop_plan()
        .filter(|plan| plan.download_url == download_url)
        .map(|plan| plan.sha256)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Pending download verification".to_string());
    Ok(ClaudeDesktopPlan {
        download_url,
        sha256,
        install_location: detected_status
            .and_then(|status| status.install_path.clone())
            .unwrap_or_else(default_claude_desktop_install_location),
    })
}

fn claude_desktop_plan_download_url() -> Result<String, String> {
    if cfg!(target_os = "macos") {
        let latest = read_claude_desktop_latest_metadata(CLAUDE_DESKTOP_LATEST_MACOS_URL)?;
        Ok(claude_desktop_macos_dmg_url(&latest.version, &latest.hash))
    } else if cfg!(target_os = "windows") {
        Ok(CLAUDE_DESKTOP_WINDOWS_MSIX_URL.to_string())
    } else {
        Err("Claude Desktop has no built-in Linux update plan.".to_string())
    }
}

fn default_claude_desktop_install_location() -> String {
    if cfg!(target_os = "macos") {
        CLAUDE_DESKTOP_MACOS_DESTINATION.to_string()
    } else if cfg!(target_os = "windows") {
        "Windows App package registration".to_string()
    } else {
        "Unsupported platform".to_string()
    }
}

fn load_cached_claude_desktop_plan() -> Option<ClaudeDesktopPlan> {
    storage::load_state_json(CLAUDE_DESKTOP_PLAN_CACHE_KEY)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str::<ClaudeDesktopPlan>(&json).ok())
}

fn store_cached_claude_desktop_plan(plan: &ClaudeDesktopPlan) {
    if let Ok(json) = serde_json::to_string(plan) {
        let _ = storage::save_state_json(CLAUDE_DESKTOP_PLAN_CACHE_KEY, &json);
    }
}

fn command_entry(definition: &InstallDefinition, stage: &str) -> ToolInstallCommand {
    ToolInstallCommand {
        tool_id: definition.tool.id.to_string(),
        tool_name: definition.tool.name.to_string(),
        stage: stage.to_string(),
        manager: manager_label(&definition.action).to_string(),
        command: command_preview(&definition.action),
        requires_admin: action_requires_admin(&definition.action),
        interactive: action_interactive(&definition.action),
    }
}

fn provider_command_entry(
    target_definition: &InstallDefinition,
    provider_definition: &InstallDefinition,
) -> ToolInstallCommand {
    ToolInstallCommand {
        tool_id: target_definition.tool.id.to_string(),
        tool_name: target_definition.tool.name.to_string(),
        stage: "target".to_string(),
        manager: manager_label(&provider_definition.action).to_string(),
        command: command_preview(&provider_definition.action),
        requires_admin: action_requires_admin(&provider_definition.action),
        interactive: action_interactive(&provider_definition.action),
    }
}

fn provider_definition_for_action(action: &InstallAction) -> Option<InstallDefinition> {
    match action {
        InstallAction::ProvidedByTool(provider_tool_id) => install_definition(provider_tool_id),
        _ => None,
    }
}

fn action_requires_admin_for_tool(definition: &InstallDefinition) -> bool {
    provider_definition_for_action(&definition.action)
        .as_ref()
        .map(|provider| action_requires_admin(&provider.action))
        .unwrap_or_else(|| action_requires_admin(&definition.action))
}

fn action_interactive_for_tool(definition: &InstallDefinition) -> bool {
    provider_definition_for_action(&definition.action)
        .as_ref()
        .map(|provider| action_interactive(&provider.action))
        .unwrap_or_else(|| action_interactive(&definition.action))
}

fn dependency_satisfied(action: &InstallAction) -> bool {
    match action {
        InstallAction::NpmGlobal(_) => command_available("npm"),
        InstallAction::MacosDmgApp { .. } => {
            cfg!(target_os = "macos") && macos_dmg_dependencies_available()
        }
        InstallAction::ClaudeDesktopWindowsMsix => {
            cfg!(target_os = "windows") && powershell_available()
        }
        InstallAction::PowerShellScript(_, _) => powershell_available(),
        InstallAction::ShellScript(_, _) | InstallAction::InteractiveShellScript(_, _) => {
            command_available("bash") || cfg!(target_os = "windows")
        }
        InstallAction::VsCodeExtension(_) => command_available("code"),
        _ => true,
    }
}

fn run_install_action(
    action: &InstallAction,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    match action {
        InstallAction::NpmGlobal(package) => {
            run_action_command("npm", &["install", "-g", package], progress)
        }
        InstallAction::Winget(package_id) => run_action_command_elevated_on_windows(
            "winget",
            &[
                "install",
                "--id",
                package_id,
                "--exact",
                "--accept-source-agreements",
                "--accept-package-agreements",
                "--disable-interactivity",
            ],
            progress,
        ),
        InstallAction::MacosDmgApp {
            latest_url,
            app_name,
            bundle_identifier,
            destination,
            ..
        } => run_macos_dmg_app_install(
            latest_url,
            app_name,
            bundle_identifier,
            Path::new(destination),
            progress,
        ),
        InstallAction::ClaudeDesktopWindowsMsix => {
            run_claude_desktop_windows_msix_install(progress)
        }
        InstallAction::PowerShellScript(_, script) => run_action_command(
            "powershell",
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ],
            progress,
        ),
        InstallAction::ShellScript(_, script) => {
            run_action_command("bash", &["-lc", script], progress)
        }
        InstallAction::InteractiveShellScript(_, _) => {
            Err("This install action requires the interactive terminal.".to_string())
        }
        InstallAction::VsCodeExtension(extension_id) => {
            run_action_command("code", &["--install-extension", extension_id], progress)
        }
        InstallAction::ProvidedByTool(provider_tool_id) => {
            let provider_definition = install_definition(provider_tool_id).ok_or_else(|| {
                format!("Provider tool '{provider_tool_id}' has no executable install action.")
            })?;
            run_install_action(&provider_definition.action, progress)
        }
        InstallAction::CustomUnsupported(_) => {
            Err("This tool has no executable standalone install action.".to_string())
        }
    }
}

fn run_update_action(
    action: &InstallAction,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    match action {
        InstallAction::NpmGlobal(package) => {
            let package = format!("{package}@latest");
            run_action_command_owned(
                "npm",
                vec!["install".to_string(), "-g".to_string(), package],
                progress,
            )
        }
        InstallAction::Winget(package_id) => run_action_command_elevated_on_windows(
            "winget",
            &[
                "upgrade",
                "--id",
                package_id,
                "--exact",
                "--accept-source-agreements",
                "--accept-package-agreements",
                "--disable-interactivity",
            ],
            progress,
        ),
        InstallAction::MacosDmgApp {
            latest_url,
            app_name,
            bundle_identifier,
            destination,
            ..
        } => run_macos_dmg_app_install(
            latest_url,
            app_name,
            bundle_identifier,
            Path::new(destination),
            progress,
        ),
        InstallAction::ClaudeDesktopWindowsMsix => {
            run_claude_desktop_windows_msix_install(progress)
        }
        InstallAction::PowerShellScript(_, script) => run_action_command(
            "powershell",
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ],
            progress,
        ),
        InstallAction::ShellScript(_, script) => {
            run_action_command("bash", &["-lc", script], progress)
        }
        InstallAction::InteractiveShellScript(_, _) => {
            Err("This update action requires the interactive terminal.".to_string())
        }
        InstallAction::VsCodeExtension(extension_id) => run_action_command(
            "code",
            &["--install-extension", extension_id, "--force"],
            progress,
        ),
        InstallAction::ProvidedByTool(provider_tool_id) => {
            let provider_definition = install_definition(provider_tool_id).ok_or_else(|| {
                format!("Provider tool '{provider_tool_id}' has no executable update action.")
            })?;
            run_update_action(&provider_definition.action, progress)
        }
        InstallAction::CustomUnsupported(_) => {
            Err("This tool has no executable standalone update action.".to_string())
        }
    }
}

fn run_update_action_for_tool(
    tool_id: &str,
    action: &InstallAction,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    if tool_id == "npm" {
        return run_update_action(&InstallAction::NpmGlobal("npm"), progress);
    }
    if tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        return run_update_action(action, progress);
    }
    run_update_action(action, progress)
}

fn run_uninstall_action_for_tool(
    _tool_id: &str,
    action: &InstallAction,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    match action {
        InstallAction::Winget(package_id) => run_action_command_elevated_on_windows(
            "winget",
            &["uninstall", "--id", package_id, "--exact"],
            progress,
        ),
        InstallAction::MacosDmgApp {
            app_name,
            bundle_identifier,
            destination,
            ..
        } => run_macos_app_uninstall(
            app_name,
            bundle_identifier,
            Path::new(destination),
            progress,
        ),
        InstallAction::VsCodeExtension(extension_id) => {
            run_action_command("code", &["--uninstall-extension", extension_id], progress)
        }
        InstallAction::NpmGlobal(package) => {
            run_action_command("npm", &["uninstall", "-g", package], progress)
        }
        InstallAction::PowerShellScript(_, _)
        | InstallAction::ClaudeDesktopWindowsMsix
        | InstallAction::ShellScript(_, _)
        | InstallAction::InteractiveShellScript(_, _)
        | InstallAction::ProvidedByTool(_)
        | InstallAction::CustomUnsupported(_) => {
            Err("This tool has no executable standalone uninstall action.".to_string())
        }
    }
}

const CLAUDE_DESKTOP_WINDOWS_UNINSTALL_COMMAND: &str =
    "Remove the detected Claude Desktop Windows package directly";
const CLAUDE_DESKTOP_WINDOWS_PACKAGE_SUFFIX: &str = "pzs8sxrjxfjjc";
const CLAUDE_DESKTOP_WINDOWS_BACKGROUND_SERVICE: &str = "CoworkVMService";
const CLAUDE_DESKTOP_WINDOWS_BACKGROUND_PROCESS: &str = "cowork-svc";
const WINGET_MULTIPLE_PACKAGES_EXIT_CODE: i32 = -1978335210;
const WINGET_MULTIPLE_PACKAGES_HEX: &str = "0x8A150016";

const CLAUDE_DESKTOP_WINDOWS_MSIX_URL: &str =
    "https://claude.ai/api/desktop/win32/x64/msix/latest/redirect";
const CLAUDE_DESKTOP_WINDOWS_UPDATE_COMMAND: &str =
    "Download and install the latest Claude Desktop MSIX from https://claude.ai/api/desktop/win32/x64/msix/latest/redirect with Add-AppxPackage -Path";
const CLAUDE_DESKTOP_PLAN_CACHE_KEY: &str = "claude_desktop.update_plan";

const CLAUDE_DESKTOP_LATEST_MACOS_URL: &str =
    "https://downloads.claude.ai/releases/darwin/universal/.latest";
const CLAUDE_DESKTOP_MACOS_APP_NAME: &str = "Claude.app";
const CLAUDE_DESKTOP_MACOS_BUNDLE_ID: &str = "com.anthropic.claudefordesktop";
const CLAUDE_DESKTOP_MACOS_DESTINATION: &str = "/Applications/Claude.app";
const HERMES_UNIX_INSTALL_COMMAND: &str =
    "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash";
const BUN_UNIX_INSTALL_COMMAND: &str = "curl -fsSL https://bun.sh/install | bash";
const GIT_MACOS_COMMAND_LINE_TOOLS_INSTALL_COMMAND: &str = "xcode-select --install";
const NODE_MACOS_OFFICIAL_PKG_INSTALL_COMMAND: &str = r#"set -e; tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT; version="$(curl -fsSL https://nodejs.org/dist/index.json | grep -m 1 '"lts":"[^"]*"' | sed -E 's/.*"version":"([^"]+)".*/\1/')"; if [ -z "$version" ]; then echo "Unable to resolve latest Node.js LTS version." >&2; exit 1; fi; pkg="$tmp/node-$version.pkg"; curl -fL "https://nodejs.org/dist/$version/node-$version.pkg" -o "$pkg"; sudo installer -pkg "$pkg" -target /"#;

const CLAUDE_DESKTOP_WINDOWS_MSIX_INSTALL_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
if (-not $env:CODESTUDIO_CLAUDE_MSIX_PATH) {
  throw "CODESTUDIO_CLAUDE_MSIX_PATH is required."
}
$target = $env:CODESTUDIO_CLAUDE_MSIX_PATH
if (-not (Test-Path -LiteralPath $target)) {
  throw "Claude Desktop MSIX was not downloaded."
}
$item = Get-Item -LiteralPath $target
if ($item.Length -le 0) {
  throw "Claude Desktop MSIX is empty."
}
Write-Output "Installing Claude Desktop MSIX with Add-AppxPackage"
Add-AppxPackage -Path $target -ForceApplicationShutdown -ErrorAction Stop
"#;

const CLAUDE_DESKTOP_WINDOWS_STALE_EXE_UNINSTALL_CLEANUP_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$roots = @(
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
function Test-ClaudeExeInstallAlive {
  param([string]$InstallLocation)
  if ([string]::IsNullOrWhiteSpace($InstallLocation)) { return $false }
  try {
    $full = [System.IO.Path]::GetFullPath($InstallLocation)
  } catch {
    return $false
  }
  if (-not (Test-Path -LiteralPath $full)) { return $false }
  $directExe = Join-Path $full 'Claude.exe'
  $directAsar = Join-Path $full 'resources\app.asar'
  if ((Test-Path -LiteralPath $directExe) -and (Test-Path -LiteralPath $directAsar)) {
    return $true
  }
  $appDir = Get-ChildItem -LiteralPath $full -Directory -Filter 'app-*' -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending |
    Select-Object -First 1
  if ($appDir) {
    $appExe = Join-Path $appDir.FullName 'Claude.exe'
    $appAsar = Join-Path $appDir.FullName 'resources\app.asar'
    if ((Test-Path -LiteralPath $appExe) -and (Test-Path -LiteralPath $appAsar)) {
      return $true
    }
  }
  return $false
}
$removed = @()
foreach ($root in $roots) {
  $props = Get-ItemProperty $root -ErrorAction SilentlyContinue
  foreach ($prop in $props) {
    $displayName = [string]$prop.DisplayName
    $publisher = [string]$prop.Publisher
    $keyName = if ($prop.PSChildName) { [string]$prop.PSChildName } else { '' }
    $isClaudeExeEntry =
      $keyName -eq 'AnthropicClaude' -or
      (($displayName -like '*Claude*') -and ($publisher -like '*Anthropic*')) -or
      (($displayName -like '*Claude*') -and ([string]$prop.UninstallString -like '*AnthropicClaude*'))
    if (-not $isClaudeExeEntry) { continue }
    $installLocation = [string]$prop.InstallLocation
    if (Test-ClaudeExeInstallAlive $installLocation) {
      Write-Output "Keeping live Claude Desktop EXE uninstall entry: $keyName"
      continue
    }
    if ($prop.PSPath) {
      try {
        Remove-Item -LiteralPath $prop.PSPath -Recurse -Force -ErrorAction Stop
        $removed += if ($installLocation) { "$keyName ($installLocation)" } else { $keyName }
      } catch {
        Write-Output "Unable to remove stale Claude Desktop EXE uninstall entry ${keyName}: $($_.Exception.Message)"
      }
    }
  }
}
if ($removed.Count -gt 0) {
  Write-Output "Removed stale Claude Desktop EXE uninstall entries: $($removed -join ', ')"
} else {
  Write-Output "No stale Claude Desktop EXE uninstall entries found."
}
"#;

const CLAUDE_DESKTOP_WINDOWS_EXE_UNINSTALL_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$roots = @(
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
$entry = $null
$installRoots = @()
function Add-ClaudeInstallRoot {
  param([string]$Path)
  if (-not $script:installRoots) { $script:installRoots = @() }
  if ([string]::IsNullOrWhiteSpace($Path)) { return }
  try {
    $full = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
  } catch {
    return
  }
  $leaf = [System.IO.Path]::GetFileName($full)
  $localAppData = [System.IO.Path]::GetFullPath($env:LOCALAPPDATA).TrimEnd('\')
  $programFiles = if ($env:ProgramFiles) { [System.IO.Path]::GetFullPath($env:ProgramFiles).TrimEnd('\') } else { '' }
  $programFilesX86 = if (${env:ProgramFiles(x86)}) { [System.IO.Path]::GetFullPath(${env:ProgramFiles(x86)}).TrimEnd('\') } else { '' }
  $underAllowedRoot =
    $full.StartsWith($localAppData + '\', [System.StringComparison]::OrdinalIgnoreCase) -or
    ($programFiles -and $full.StartsWith($programFiles + '\', [System.StringComparison]::OrdinalIgnoreCase)) -or
    ($programFilesX86 -and $full.StartsWith($programFilesX86 + '\', [System.StringComparison]::OrdinalIgnoreCase))
  $looksLikeClaudeRoot =
    $leaf -like '*Claude*' -or
    $leaf -like '*Anthropic*' -or
    (Test-Path -LiteralPath (Join-Path $full 'Claude.exe')) -or
    (Test-Path -LiteralPath (Join-Path $full 'Update.exe')) -or
    (Get-ChildItem -LiteralPath $full -Directory -Filter 'app-*' -ErrorAction SilentlyContinue | Select-Object -First 1)
  if ($underAllowedRoot -and $looksLikeClaudeRoot -and -not $installRoots.Contains($full)) {
    $script:installRoots += $full
  }
}
foreach ($root in $roots) {
  $props = Get-ItemProperty $root -ErrorAction SilentlyContinue
  foreach ($prop in $props) {
    if ($prop.DisplayName -and $prop.DisplayName -like '*Claude*') {
      $entry = $prop
      Add-ClaudeInstallRoot ([string]$prop.InstallLocation)
      break
    }
  }
  if ($entry) { break }
}
if ($entry -and $entry.UninstallString) {
  $uninstallString = [string]$entry.UninstallString
  Write-Output "Found uninstaller: $uninstallString"
  # UninstallString may be a bare path or a quoted path with args.
  # e.g. "C:\path\Update.exe" --uninstall
  $exe = $null
  $extraArgs = ''
  $trimmed = $uninstallString.Trim()
  if ($trimmed.StartsWith('"')) {
    $closeIdx = $trimmed.IndexOf('"', 1)
    if ($closeIdx -gt 0) {
      $exe = $trimmed.Substring(1, $closeIdx - 1)
      $extraArgs = $trimmed.Substring($closeIdx + 1).Trim()
    }
  } else {
    $parts = $trimmed -split ' ', 2
    $exe = $parts[0]
    if ($parts.Length -gt 1) { $extraArgs = $parts[1].Trim() }
  }
  if ($exe -and (Test-Path -LiteralPath $exe)) {
    Write-Output "Running silent uninstall: $exe $extraArgs"
    $allArgs = @('/S')
    if ($extraArgs) { $allArgs += ($extraArgs -split ' ') }
    $process = Start-Process -FilePath $exe -ArgumentList $allArgs -Wait -PassThru
    if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) {
      Write-Output "Uninstaller exited with code $($process.ExitCode)"
    }
  } else {
    Write-Output "Uninstaller not found at $exe, attempting direct removal"
  }
  if ($entry.PSPath) {
    Remove-Item -LiteralPath $entry.PSPath -Recurse -Force -ErrorAction SilentlyContinue
  }
} else {
  Write-Output "No registry uninstall entry found, attempting direct removal"
}
$claudeDir = Join-Path $env:LOCALAPPDATA 'AnthropicClaude'
Add-ClaudeInstallRoot $claudeDir
$programsClaudeDir = Join-Path $env:LOCALAPPDATA 'Programs\Claude'
Add-ClaudeInstallRoot $programsClaudeDir
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Claude.lnk'
if (Test-Path -LiteralPath $startMenu) {
  Remove-Item -LiteralPath $startMenu -Force -ErrorAction SilentlyContinue
}
$remainingRoots = @()
foreach ($root in $installRoots) {
  if (Test-Path -LiteralPath $root) {
    Write-Output "Removing $root"
    try {
      Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction Stop
    } catch {
      Write-Output ("Failed to remove {0}: {1}" -f $root, [string]$_.Exception.Message)
    }
  }
  if (Test-Path -LiteralPath $root) {
    $remainingRoots += [string]$root
  }
}
if ($remainingRoots.Count -gt 0) {
  Write-Error ('Claude Desktop remaining install roots: ' + ($remainingRoots -join '; '))
  exit 1
}
Write-Output "Done; Claude Desktop install files removed and verified."
"#;

fn run_claude_desktop_exe_uninstall(
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    stop_claude_desktop_windows_background_services(progress)?;
    let mut output = run_action_command(
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            CLAUDE_DESKTOP_WINDOWS_EXE_UNINSTALL_SCRIPT,
        ],
        progress,
    )?;
    let service_output = remove_claude_desktop_windows_background_services(progress)?;
    output.success = output.success && service_output.success;
    output.exit_code = if output.success { Some(0) } else { Some(1) };
    append_output_tail(&mut output.stdout_tail, &service_output.stdout_tail);
    append_output_tail(&mut output.stderr_tail, &service_output.stderr_tail);
    Ok(output)
}

fn run_claude_desktop_windows_uninstall(
    install_kind: Option<&str>,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    if !cfg!(target_os = "windows") {
        return Ok(failed_output(
            "Claude Desktop Windows uninstall is only supported on Windows.",
        ));
    }

    match install_kind {
        Some("exe") => run_claude_desktop_exe_uninstall(progress),
        Some("msix") => run_claude_desktop_msix_uninstall(progress),
        _ => {
            if detector::claude_desktop_install_kinds().exe.installed {
                run_claude_desktop_exe_uninstall(progress)
            } else {
                run_claude_desktop_msix_uninstall(progress)
            }
        }
    }
}

fn resolve_claude_desktop_windows_uninstall_kind(
    requested: Option<&str>,
    install_kinds: &ClaudeDesktopInstallKinds,
) -> Option<String> {
    match requested {
        Some("exe") | Some("msix") => requested.map(ToString::to_string),
        _ if install_kinds.exe.installed => Some("exe".to_string()),
        _ if install_kinds.msix.installed => Some("msix".to_string()),
        _ => requested.map(ToString::to_string),
    }
}

fn claude_desktop_windows_uninstall_verified(
    install_kind: Option<&str>,
    install_kinds: &ClaudeDesktopInstallKinds,
    registered_msix_installed: bool,
) -> bool {
    match install_kind {
        Some("exe") => !install_kinds.exe.installed,
        Some("msix") => !registered_msix_installed,
        _ => !install_kinds.exe.installed && !registered_msix_installed,
    }
}

fn claude_desktop_windows_uninstall_success_message(
    tool_name: &str,
    install_kind: Option<&str>,
    install_kinds: &ClaudeDesktopInstallKinds,
    registered_msix_installed: bool,
) -> String {
    match install_kind {
        Some("msix") if install_kinds.exe.installed => {
            format!("{tool_name} MSIX install removed. Native EXE install is still present.")
        }
        Some("exe") if registered_msix_installed => {
            format!("{tool_name} native EXE install removed. MSIX install is still present.")
        }
        Some("exe") if install_kinds.msix.installed => {
            format!("{tool_name} native EXE install removed. MSIX/AppX residue is still detected.")
        }
        Some("msix") => format!("{tool_name} MSIX install removed."),
        Some("exe") => format!("{tool_name} native EXE install removed."),
        _ => format!("{tool_name} uninstalled."),
    }
}

fn run_claude_desktop_msix_uninstall(
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    emit_install_progress(
        progress,
        "stdout",
        "Removing Claude Desktop MSIX/AppX package directly...\n".to_string(),
        None,
        false,
    );
    stop_claude_desktop_windows_background_services(progress)?;
    let report = match package::remove_first_msix_package(
        detector::claude_desktop_windows_package_identities(),
    ) {
        Ok(report) => report,
        Err(err) => return Ok(failed_output_with_progress(&err, progress)),
    };
    for note in &report.notes {
        emit_install_progress(progress, "stdout", format!("{note}\n"), None, false);
    }
    let mut notes = report.notes.clone();
    let mut message = report.message.clone();
    let mut success = report.success;
    if report.success {
        emit_install_progress(
            progress,
            "stdout",
            "Removing Claude Desktop MSIX/AppX package files...\n".to_string(),
            None,
            false,
        );
        let cleanup = match package::remove_claude_msix_payloads(
            detector::claude_desktop_windows_package_identities(),
            CLAUDE_DESKTOP_WINDOWS_PACKAGE_SUFFIX,
        ) {
            Ok(cleanup) => cleanup,
            Err(err) => return Ok(failed_output_with_progress(&err, progress)),
        };
        for note in &cleanup.notes {
            emit_install_progress(progress, "stdout", format!("{note}\n"), None, false);
        }
        message = format!("{} {}", report.message, cleanup.message);
        notes.extend(cleanup.notes);
        if !cleanup.removed_payloads.is_empty() {
            notes.push(format!(
                "Removed Claude Desktop MSIX/AppX package files: {}",
                cleanup.removed_payloads.join("; ")
            ));
        }
        if !cleanup.remaining_payloads.is_empty() {
            notes.push(format!(
                "Claude Desktop MSIX/AppX package files remain: {}",
                cleanup.remaining_payloads.join("; ")
            ));
        }
        success = cleanup.success;
    }
    let service_output = remove_claude_desktop_windows_background_services(progress)?;
    if !service_output.stdout_tail.is_empty() {
        notes.push(service_output.stdout_tail.clone());
        message = format!("{} {}", message, service_output.stdout_tail);
    }
    if !service_output.stderr_tail.is_empty() {
        notes.push(service_output.stderr_tail.clone());
    }
    success = success && service_output.success;
    let exit_code = if success { 0 } else { 1 };
    emit_install_progress(progress, "status", String::new(), Some(exit_code), true);
    Ok(InstallCommandOutput {
        success,
        exit_code: Some(exit_code),
        stdout_tail: message,
        stderr_tail: if success {
            String::new()
        } else if notes
            .iter()
            .any(|note| note.contains("MSIX/AppX package files remain"))
        {
            notes.join("\n")
        } else if notes.iter().any(|note| note.contains("CoworkVMService")) {
            notes.join("\n")
        } else {
            "Claude Desktop MSIX uninstall failed.".to_string()
        },
        missing_command: None,
    })
}

fn stop_claude_desktop_windows_background_services(
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    if !cfg!(target_os = "windows") {
        return Ok(InstallCommandOutput {
            success: true,
            exit_code: Some(0),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            missing_command: None,
        });
    }
    emit_install_progress(
        progress,
        "stdout",
        "Stopping Claude Desktop background services...\n".to_string(),
        None,
        false,
    );
    let script = r#"
$ErrorActionPreference = 'Continue'
$serviceName = __SERVICE_NAME__
$processName = __PROCESS_NAME__
$notes = @()
$service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
if ($service -and $service.Status -ne 'Stopped') {
  try {
    Stop-Service -Name $serviceName -Force -ErrorAction Stop
    $service.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(12))
    $notes += "Stopped $serviceName."
  } catch {
    $notes += ("Failed to stop {0}: {1}" -f $serviceName, [string]$_.Exception.Message)
  }
}
Get-Process -Name $processName -ErrorAction SilentlyContinue | ForEach-Object {
  try {
    Stop-Process -Id $_.Id -Force -ErrorAction Stop
    $notes += ("Stopped process {0} ({1})." -f $processName, $_.Id)
  } catch {
    $notes += ("Failed to stop process {0} ({1}): {2}" -f $processName, $_.Id, [string]$_.Exception.Message)
  }
}
Start-Sleep -Milliseconds 500
$stillService = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
$stillProcess = Get-Process -Name $processName -ErrorAction SilentlyContinue
$success = (($null -eq $stillProcess) -and (($null -eq $stillService) -or $stillService.Status -eq 'Stopped'))
[pscustomobject]@{
  success = [bool]$success
  message = if ($success) { 'Claude Desktop background services stopped.' } else { 'Claude Desktop background services are still running.' }
  notes = @($notes)
} | ConvertTo-Json -Compress -Depth 4
"#
    .replace(
        "__SERVICE_NAME__",
        &ps_single_quote(CLAUDE_DESKTOP_WINDOWS_BACKGROUND_SERVICE),
    )
    .replace(
        "__PROCESS_NAME__",
        &ps_single_quote(CLAUDE_DESKTOP_WINDOWS_BACKGROUND_PROCESS),
    );
    run_claude_desktop_windows_service_script(&script, progress)
}

fn remove_claude_desktop_windows_background_services(
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    if !cfg!(target_os = "windows") {
        return Ok(InstallCommandOutput {
            success: true,
            exit_code: Some(0),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            missing_command: None,
        });
    }
    emit_install_progress(
        progress,
        "stdout",
        "Removing Claude Desktop background service registration...\n".to_string(),
        None,
        false,
    );
    let script = r#"
$ErrorActionPreference = 'Continue'
$serviceName = __SERVICE_NAME__
$processName = __PROCESS_NAME__
$notes = @()
Get-Process -Name $processName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
if ($service) {
  if ($service.Status -ne 'Stopped') {
    Stop-Service -Name $serviceName -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
  }
  $sc = Join-Path $env:SystemRoot 'System32\sc.exe'
  if (Test-Path -LiteralPath $sc) {
    & $sc delete $serviceName | Out-String | ForEach-Object { if ($_.Trim()) { $notes += $_.Trim() } }
  } else {
    $notes += 'sc.exe was not found.'
  }
}
Start-Sleep -Seconds 1
$remainingService = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
$remainingProcess = Get-Process -Name $processName -ErrorAction SilentlyContinue
if ($remainingService) {
  $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  if (-not (Test-Path -LiteralPath $powershell)) { $powershell = 'powershell.exe' }
  $elevatedScript = @"
`$ErrorActionPreference = 'Continue'
`$serviceName = '$serviceName'
`$processName = '$processName'
Get-Process -Name `$processName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
`$svc = Get-Service -Name `$serviceName -ErrorAction SilentlyContinue
if (`$svc -and `$svc.Status -ne 'Stopped') {
  Stop-Service -Name `$serviceName -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 1
}
`$sc = Join-Path `$env:SystemRoot 'System32\sc.exe'
if (Test-Path -LiteralPath `$sc) {
  & `$sc delete `$serviceName | Out-Null
}
"@
  try {
    $process = Start-Process -FilePath $powershell -ArgumentList @(
      '-NoLogo',
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-Command',
      $elevatedScript
    ) -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) {
      $notes += ('Elevated CoworkVMService delete exited with code ' + $process.ExitCode)
    }
  } catch {
    $notes += ('Failed to run elevated CoworkVMService delete: ' + [string]$_.Exception.Message)
  }
  Start-Sleep -Seconds 1
  $remainingService = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
}
$remainingProcess = Get-Process -Name $processName -ErrorAction SilentlyContinue
$success = ($null -eq $remainingService -and $null -eq $remainingProcess)
[pscustomobject]@{
  success = [bool]$success
  message = if ($success) { 'CoworkVMService removed and verified.' } else { 'CoworkVMService or cowork-svc is still present.' }
  notes = @($notes)
} | ConvertTo-Json -Compress -Depth 4
"#
    .replace(
        "__SERVICE_NAME__",
        &ps_single_quote(CLAUDE_DESKTOP_WINDOWS_BACKGROUND_SERVICE),
    )
    .replace(
        "__PROCESS_NAME__",
        &ps_single_quote(CLAUDE_DESKTOP_WINDOWS_BACKGROUND_PROCESS),
    );
    run_claude_desktop_windows_service_script(&script, progress)
}

fn run_claude_desktop_windows_service_script(
    script: &str,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    let json = match run_powershell(script) {
        Ok(json) => json,
        Err(err) => {
            emit_install_progress(progress, "stderr", format!("{err}\n"), None, false);
            emit_install_progress(progress, "status", String::new(), Some(1), true);
            return Ok(failed_output(err));
        }
    };
    let report: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse Claude Desktop service cleanup result: {err}"))?;
    let success = report
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let message = report
        .get("message")
        .and_then(|value| value.as_str())
        .unwrap_or("Claude Desktop service cleanup completed.")
        .to_string();
    let notes = report
        .get("notes")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let mut stdout_tail = message;
    append_output_tail(&mut stdout_tail, &notes);
    emit_install_progress(progress, "stdout", format!("{stdout_tail}\n"), None, false);
    emit_install_progress(
        progress,
        "status",
        String::new(),
        Some(if success { 0 } else { 1 }),
        true,
    );
    Ok(InstallCommandOutput {
        success,
        exit_code: Some(if success { 0 } else { 1 }),
        stdout_tail,
        stderr_tail: if success { String::new() } else { notes },
        missing_command: None,
    })
}

fn append_output_tail(target: &mut String, addition: &str) {
    if addition.trim().is_empty() {
        return;
    }
    if !target.trim().is_empty() {
        target.push('\n');
    }
    target.push_str(addition.trim());
}

#[derive(Debug, Deserialize)]
struct ClaudeDesktopLatestMetadata {
    version: String,
    hash: String,
}

fn run_macos_dmg_app_install(
    latest_url: &str,
    app_name: &str,
    bundle_identifier: &str,
    destination: &Path,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    if !cfg!(target_os = "macos") {
        return Ok(failed_output(
            "The official macOS DMG installer is only supported on macOS.",
        ));
    }
    if !macos_dmg_dependencies_available() {
        return Ok(failed_output(
            "hdiutil or ditto is unavailable, so the official DMG cannot be installed.",
        ));
    }

    emit_install_progress(
        progress,
        "stdout",
        "Reading Claude Desktop official release metadata...\n".to_string(),
        None,
        false,
    );
    let latest = match read_claude_desktop_latest_metadata(latest_url) {
        Ok(latest) => latest,
        Err(err) => return Ok(failed_output_with_progress(&err, progress)),
    };
    let url = claude_desktop_macos_dmg_url(&latest.version, &latest.hash);
    let dmg_path = match claude_desktop_download_path(&latest.version) {
        Ok(path) => path,
        Err(err) => return Ok(failed_output_with_progress(&err, progress)),
    };

    emit_install_progress(
        progress,
        "stdout",
        format!(
            "Downloading Claude Desktop {} official DMG...\n",
            latest.version
        ),
        None,
        false,
    );
    if let Err(err) = download_url_to_file(&url, &dmg_path, progress) {
        return Ok(failed_output_with_progress(&err, progress));
    }

    let verify_context = progress.map(|context| InstallProgressContext {
        root_tool_id: context.root_tool_id,
        tool_id: context.tool_id,
        tool_name: context.tool_name,
        stage: context.stage,
        command: context.command,
        install_kind: context.install_kind,
        progress_phase: Some("verifying"),
        progress_message: Some("claudeDesktop.progressVerifying"),
        progress_step: Some(2),
        progress_step_total: Some(3),
        progress: context.progress,
    });
    emit_install_progress(
        verify_context.as_ref(),
        "stdout",
        "Verifying Claude Desktop DMG SHA-256...\n".to_string(),
        None,
        false,
    );
    let actual_sha256 = match sha256_file(&dmg_path) {
        Ok(value) => value,
        Err(err) => return Ok(failed_output_with_progress(&err, progress)),
    };
    if let Some(expected) = load_cached_claude_desktop_plan()
        .filter(|plan| plan.download_url == url)
        .map(|plan| plan.sha256)
        .filter(|value| value.len() == 64)
    {
        if !actual_sha256.eq_ignore_ascii_case(&expected) {
            let _ = fs::remove_file(&dmg_path);
            return Ok(failed_output_with_progress(
                &format!(
                    "SHA-256 verification failed: expected {}, got {}.",
                    expected, actual_sha256
                ),
                progress,
            ));
        }
    }
    store_cached_claude_desktop_plan(&ClaudeDesktopPlan {
        download_url: url.clone(),
        sha256: actual_sha256,
        install_location: destination.display().to_string(),
    });

    let install_context = progress.map(|context| InstallProgressContext {
        root_tool_id: context.root_tool_id,
        tool_id: context.tool_id,
        tool_name: context.tool_name,
        stage: context.stage,
        command: context.command,
        install_kind: context.install_kind,
        progress_phase: Some("installing"),
        progress_message: Some("claudeDesktop.progressInstalling"),
        progress_step: Some(3),
        progress_step_total: Some(3),
        progress: context.progress,
    });
    emit_install_progress(
        install_context.as_ref(),
        "stdout",
        format!("Installing {app_name} to {}...\n", destination.display()),
        None,
        false,
    );
    let report =
        match package::install_macos_dmg(&dmg_path, app_name, destination, Some(bundle_identifier))
        {
            Ok(report) => report,
            Err(err) => return Ok(failed_output_with_progress(&err, progress)),
        };
    for note in report.notes {
        emit_install_progress(progress, "stdout", format!("{note}\n"), None, false);
    }
    let installed = report.installed.is_some();
    let mut stdout_tail = format!(
        "Claude Desktop {} official DMG installed to {}.",
        latest.version,
        destination.display()
    );
    if installed {
        match cleanup_claude_desktop_download_cache(&dmg_path) {
            Ok(Some(note)) => {
                emit_install_progress(progress, "stdout", format!("{note}\n"), None, false);
                stdout_tail = format!("{stdout_tail} {note}");
            }
            Ok(None) => {}
            Err(err) => {
                let note = format!("Failed to clean Claude Desktop download cache: {err}");
                emit_install_progress(progress, "stderr", format!("{note}\n"), None, false);
                stdout_tail = format!("{stdout_tail} {note}");
            }
        }
    }
    emit_install_progress(
        progress,
        "status",
        String::new(),
        Some(if installed { 0 } else { 1 }),
        true,
    );
    Ok(InstallCommandOutput {
        success: installed,
        exit_code: Some(if installed { 0 } else { 1 }),
        stdout_tail,
        stderr_tail: if installed {
            String::new()
        } else {
            "Claude Desktop app was copied, but verification did not find it.".to_string()
        },
        missing_command: None,
    })
}

fn run_macos_app_uninstall(
    app_name: &str,
    bundle_identifier: &str,
    destination: &Path,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    if !cfg!(target_os = "macos") {
        return Ok(failed_output(
            "macOS app uninstall is only supported on macOS.",
        ));
    }
    let candidates = macos_app_candidates(destination, app_name);
    let app = package::detect_macos_app(&candidates, Some(bundle_identifier))
        .or_else(|| package::detect_macos_app(&candidates, None));
    let path = app
        .map(|app| PathBuf::from(app.path))
        .unwrap_or_else(|| destination.to_path_buf());
    emit_install_progress(
        progress,
        "stdout",
        format!("Removing {}...\n", path.display()),
        None,
        false,
    );
    let result = if path.exists() {
        fs::remove_dir_all(&path).map_err(|err| format!("Failed to remove macOS app bundle: {err}"))
    } else {
        Ok(())
    };
    match result {
        Ok(()) => {
            emit_install_progress(progress, "status", String::new(), Some(0), true);
            Ok(InstallCommandOutput {
                success: true,
                exit_code: Some(0),
                stdout_tail: format!("Removed {}.", path.display()),
                stderr_tail: String::new(),
                missing_command: None,
            })
        }
        Err(err) => Ok(failed_output_with_progress(&err, progress)),
    }
}

fn current_status(tool_id: &str) -> Result<ToolStatus, String> {
    let snapshot = detector::detect_environment()?;
    snapshot
        .tools
        .into_iter()
        .chain(snapshot.system)
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| format!("No detection status found for tool '{tool_id}'."))
}

fn current_status_for_missing_command(
    missing_command: Option<&str>,
    fallback_tool_id: &str,
) -> Option<ToolStatus> {
    if let Some(command) = missing_command.and_then(tool_id_for_command) {
        if let Ok(status) = current_status(command) {
            return Some(status);
        }
    }
    current_status(fallback_tool_id).ok()
}

fn tool_id_for_command(command: &str) -> Option<&'static str> {
    let command = command
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(command)
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .trim_end_matches(".bat")
        .trim_end_matches(".ps1")
        .to_ascii_lowercase();
    match command.as_str() {
        "node" => Some("node"),
        "git" => Some("git"),
        "npm" => Some("npm"),
        "pnpm" => Some("pnpm"),
        "bun" => Some("bun"),
        _ => None,
    }
}

fn command_available(command: &str) -> bool {
    let Some(resolved) = resolve_command(command) else {
        return false;
    };

    hidden_command_with_args(&resolved, &["--version"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn powershell_available() -> bool {
    let Some(resolved) = resolve_command("powershell") else {
        return false;
    };

    hidden_command_with_args(
        &resolved,
        &["-NoProfile", "-Command", "$PSVersionTable.PSVersion"],
    )
    .output()
    .map(|output| output.status.success())
    .unwrap_or(false)
}

fn command_resolves(command: &str) -> bool {
    resolve_command(command).is_some()
}

fn macos_dmg_dependencies_available() -> bool {
    command_resolves("hdiutil") && command_resolves("ditto")
}

fn failed_output(message: impl Into<String>) -> InstallCommandOutput {
    InstallCommandOutput {
        success: false,
        exit_code: Some(1),
        stdout_tail: String::new(),
        stderr_tail: message.into(),
        missing_command: None,
    }
}

fn failed_output_with_progress(
    message: &str,
    progress: Option<&InstallProgressContext>,
) -> InstallCommandOutput {
    emit_install_progress(progress, "stderr", format!("{message}\n"), None, false);
    emit_install_progress(progress, "status", String::new(), Some(1), true);
    failed_output(message)
}

fn claude_desktop_msix_temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!(
        "codestudio-claude-msix-{}-{suffix}",
        std::process::id()
    ))
}

fn run_claude_desktop_windows_msix_install(
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    if !cfg!(target_os = "windows") {
        return Ok(failed_output(
            "Claude Desktop Windows App install is only supported on Windows.",
        ));
    }
    if !powershell_available() {
        return Ok(missing_command_output("powershell"));
    }

    remove_stale_claude_desktop_windows_exe_uninstall_entries(progress)?;

    let temp_root = claude_desktop_msix_temp_dir();
    let target = temp_root.join("Claude.msix");
    let download_context = progress.map(|context| InstallProgressContext {
        root_tool_id: context.root_tool_id,
        tool_id: context.tool_id,
        tool_name: context.tool_name,
        stage: context.stage,
        command: context.command,
        install_kind: context.install_kind,
        progress_phase: Some("downloading"),
        progress_message: Some("claudeDesktop.progressDownloading"),
        progress_step: Some(2),
        progress_step_total: Some(4),
        progress: context.progress,
    });

    emit_install_progress(
        progress,
        "stdout",
        "Downloading Claude Desktop MSIX...\n".to_string(),
        None,
        false,
    );
    if let Err(err) = download_url_to_file(
        CLAUDE_DESKTOP_WINDOWS_MSIX_URL,
        &target,
        download_context.as_ref(),
    ) {
        let _ = fs::remove_dir_all(&temp_root);
        return Ok(failed_output_with_progress(&err, progress));
    }

    let verify_context = progress.map(|context| InstallProgressContext {
        root_tool_id: context.root_tool_id,
        tool_id: context.tool_id,
        tool_name: context.tool_name,
        stage: context.stage,
        command: context.command,
        install_kind: context.install_kind,
        progress_phase: Some("verifying"),
        progress_message: Some("claudeDesktop.progressVerifying"),
        progress_step: Some(3),
        progress_step_total: Some(4),
        progress: context.progress,
    });
    emit_install_progress(
        verify_context.as_ref(),
        "stdout",
        "Verifying Claude Desktop MSIX SHA-256...\n".to_string(),
        None,
        false,
    );
    let actual_sha256 = match sha256_file(&target) {
        Ok(value) => value,
        Err(err) => {
            let _ = fs::remove_dir_all(&temp_root);
            return Ok(failed_output_with_progress(&err, progress));
        }
    };
    if let Some(expected) = load_cached_claude_desktop_plan()
        .filter(|plan| plan.download_url == CLAUDE_DESKTOP_WINDOWS_MSIX_URL)
        .map(|plan| plan.sha256)
        .filter(|value| value.len() == 64)
    {
        if !actual_sha256.eq_ignore_ascii_case(&expected) {
            let _ = fs::remove_dir_all(&temp_root);
            return Ok(failed_output_with_progress(
                &format!(
                    "SHA-256 verification failed: expected {}, got {}.",
                    expected, actual_sha256
                ),
                progress,
            ));
        }
    }
    store_cached_claude_desktop_plan(&ClaudeDesktopPlan {
        download_url: CLAUDE_DESKTOP_WINDOWS_MSIX_URL.to_string(),
        sha256: actual_sha256,
        install_location: "Windows App package registration".to_string(),
    });

    let install_context = progress.map(|context| InstallProgressContext {
        root_tool_id: context.root_tool_id,
        tool_id: context.tool_id,
        tool_name: context.tool_name,
        stage: context.stage,
        command: context.command,
        install_kind: context.install_kind,
        progress_phase: Some("installing"),
        progress_message: Some("claudeDesktop.progressInstalling"),
        progress_step: Some(4),
        progress_step_total: Some(4),
        progress: context.progress,
    });

    emit_install_progress(
        install_context.as_ref(),
        "stdout",
        "Installing Claude Desktop MSIX with Add-AppxPackage...\n".to_string(),
        None,
        false,
    );
    let Some(resolved) = resolve_command("powershell") else {
        let _ = fs::remove_dir_all(&temp_root);
        return Ok(missing_command_output("powershell"));
    };
    let mut command = hidden_command_with_args(
        &resolved,
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            CLAUDE_DESKTOP_WINDOWS_MSIX_INSTALL_SCRIPT,
        ],
    );
    command.env("CODESTUDIO_CLAUDE_MSIX_PATH", &target);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = run_streaming_command(command, "powershell", install_context.as_ref());
    let _ = fs::remove_dir_all(&temp_root);
    output
}

fn remove_stale_claude_desktop_windows_exe_uninstall_entries(
    progress: Option<&InstallProgressContext>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }
    emit_install_progress(
        progress,
        "stdout",
        "Checking stale Claude Desktop EXE uninstall registry entries...\n".to_string(),
        None,
        false,
    );
    let Some(resolved) = resolve_command("powershell") else {
        return Err(
            "PowerShell is required to clean stale Claude Desktop EXE entries.".to_string(),
        );
    };
    let mut command = hidden_command_with_args(
        &resolved,
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            CLAUDE_DESKTOP_WINDOWS_STALE_EXE_UNINSTALL_CLEANUP_SCRIPT,
        ],
    );
    let output = command
        .output()
        .map_err(|err| format!("Failed to clean stale Claude Desktop EXE entries: {err}"))?;
    let stdout = decode(&output.stdout);
    if !stdout.trim().is_empty() {
        emit_install_progress(progress, "stdout", stdout.clone(), None, false);
    }
    let stderr = decode(&output.stderr);
    if !stderr.trim().is_empty() {
        emit_install_progress(progress, "stderr", stderr.clone(), None, false);
    }
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Failed to clean stale Claude Desktop EXE uninstall entries: {}",
            tail(&stderr)
        ))
    }
}

fn read_claude_desktop_latest_metadata(url: &str) -> Result<ClaudeDesktopLatestMetadata, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("CodeStudio Lite")
        .build()
        .map_err(|err| format!("Failed to create HTTP client: {err}"))?;
    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("Failed to read Claude Desktop latest metadata: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to read Claude Desktop latest metadata: HTTP {}",
            response.status()
        ));
    }
    let latest = response
        .json::<ClaudeDesktopLatestMetadata>()
        .map_err(|err| format!("Failed to parse Claude Desktop latest metadata: {err}"))?;
    if latest.version.trim().is_empty() || latest.hash.trim().is_empty() {
        return Err("Claude Desktop latest metadata is incomplete.".to_string());
    }
    Ok(latest)
}

fn claude_desktop_macos_dmg_url(version: &str, hash: &str) -> String {
    format!("https://downloads.claude.ai/releases/darwin/universal/{version}/Claude-{hash}.dmg")
}

fn claude_desktop_download_path(version: &str) -> Result<PathBuf, String> {
    Ok(claude_desktop_download_dir()?.join(format!("Claude-{version}.dmg")))
}

fn claude_desktop_download_dir() -> Result<PathBuf, String> {
    let paths = app_paths().map_err(|err| format!("Failed to resolve app paths: {err}"))?;
    let dir = paths.downloads_dir.join("claude-desktop");
    fs::create_dir_all(&dir)
        .map_err(|err| format!("Failed to create download directory: {err}"))?;
    Ok(dir)
}

fn cleanup_claude_desktop_download_cache(installed_dmg: &Path) -> Result<Option<String>, String> {
    let dir = claude_desktop_download_dir()?;
    let mut removed = false;
    if installed_dmg.exists() {
        fs::remove_file(installed_dmg).map_err(|err| {
            format!(
                "Failed to remove downloaded DMG {}: {err}",
                installed_dmg.display()
            )
        })?;
        removed = true;
    }
    let partial = installed_dmg.with_extension("download");
    if partial.exists() {
        fs::remove_file(&partial).map_err(|err| {
            format!(
                "Failed to remove partial download {}: {err}",
                partial.display()
            )
        })?;
        removed = true;
    }
    let empty = fs::read_dir(&dir)
        .map_err(|err| {
            format!(
                "Failed to inspect download directory {}: {err}",
                dir.display()
            )
        })?
        .next()
        .is_none();
    if empty {
        let _ = fs::remove_dir(&dir);
    }
    Ok(removed.then(|| "Removed Claude Desktop downloaded installer cache.".to_string()))
}

fn download_url_to_file(
    url: &str,
    path: &Path,
    progress: Option<&InstallProgressContext>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create download directory: {err}"))?;
    }
    let temp = path.with_extension("download");
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .user_agent("CodeStudio Lite")
        .build()
        .map_err(|err| format!("Failed to create HTTP client: {err}"))?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|err| format!("Failed to download installer: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download installer: HTTP {}",
            response.status()
        ));
    }
    let total = response.content_length();
    let mut file =
        fs::File::create(&temp).map_err(|err| format!("Failed to create download file: {err}"))?;
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    let mut last_emit = Instant::now() - Duration::from_secs(2);
    loop {
        let size = response
            .read(&mut buffer)
            .map_err(|err| format!("Failed while downloading installer: {err}"))?;
        if size == 0 {
            break;
        }
        file.write_all(&buffer[..size])
            .map_err(|err| format!("Failed to write installer: {err}"))?;
        downloaded += size as u64;
        if last_emit.elapsed() >= Duration::from_millis(750) {
            emit_install_download_progress(progress, downloaded, total);
            emit_install_progress(
                progress,
                "stdout",
                format_download_progress(downloaded, total),
                None,
                false,
            );
            last_emit = Instant::now();
        }
    }
    file.flush()
        .map_err(|err| format!("Failed to finish installer download: {err}"))?;
    fs::rename(&temp, path).map_err(|err| format!("Failed to save downloaded file: {err}"))?;
    emit_install_download_progress(progress, downloaded, total);
    emit_install_progress(
        progress,
        "stdout",
        format_download_progress(downloaded, total),
        None,
        false,
    );
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|err| format!("Failed to open file for SHA-256 calculation: {err}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 128];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("Failed to read file for SHA-256 calculation: {err}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn emit_install_download_progress(
    context: Option<&InstallProgressContext>,
    downloaded: u64,
    total: Option<u64>,
) {
    let Some(context) = context else {
        return;
    };
    let Some(progress) = context.progress else {
        return;
    };
    let percent = total.and_then(|total| {
        (total > 0).then(|| ((downloaded as f64 / total as f64) * 100.0).clamp(0.0, 100.0))
    });
    progress(ToolInstallProgress {
        root_tool_id: context.root_tool_id.to_string(),
        tool_id: context.tool_id.to_string(),
        tool_name: context.tool_name.to_string(),
        stage: context.stage.to_string(),
        command: context.command.to_string(),
        install_kind: context.install_kind.map(ToString::to_string),
        phase: context.progress_phase.map(ToString::to_string),
        message: context.progress_message.map(ToString::to_string),
        downloaded: Some(downloaded),
        total,
        percent,
        step: context.progress_step,
        step_total: context.progress_step_total,
        stream: "status".to_string(),
        chunk: String::new(),
        done: false,
        exit_code: None,
    });
}

fn format_download_progress(downloaded: u64, total: Option<u64>) -> String {
    match total {
        Some(total) if total > 0 => format!(
            "Downloaded {} / {} MB\n",
            downloaded / 1_000_000,
            total / 1_000_000
        ),
        _ => format!("Downloaded {} MB\n", downloaded / 1_000_000),
    }
}

fn macos_app_candidates(destination: &Path, app_name: &str) -> Vec<PathBuf> {
    let mut candidates = vec![destination.to_path_buf()];
    if let Ok(paths) = app_paths() {
        candidates.push(paths.home_dir.join("Applications").join(app_name));
    }
    candidates
}

fn open_folder(path: &Path) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        hidden_command("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to open path: {err}"))
    } else if cfg!(target_os = "macos") {
        hidden_command("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to open path: {err}"))
    } else {
        hidden_command("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to open path: {err}"))
    }
}

fn run_action_command(
    program: &str,
    args: &[&str],
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    let Some(resolved) = resolve_command(program) else {
        return Ok(missing_command_output(program));
    };
    let mut command = hidden_command_with_args(&resolved, args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    run_streaming_command(command, program, progress)
}

fn run_action_command_elevated_on_windows(
    program: &str,
    args: &[&str],
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    #[cfg(windows)]
    {
        let Some(resolved) = resolve_command(program) else {
            return Ok(missing_command_output(program));
        };
        let powershell_args = windows_elevated_powershell_args(&resolved, args);
        let powershell_args = powershell_args
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut command = hidden_command_with_args("powershell.exe", &powershell_args);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        return run_streaming_command(command, program, progress);
    }

    #[cfg(not(windows))]
    {
        run_action_command(program, args, progress)
    }
}

#[cfg(windows)]
fn windows_elevated_powershell_args(program: &str, args: &[&str]) -> Vec<String> {
    let argument_list = args
        .iter()
        .map(|arg| ps_single_quote(arg))
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        "$process = Start-Process -FilePath {} -ArgumentList @({}) -Verb RunAs -Wait -PassThru; if ($null -ne $process.ExitCode) {{ exit $process.ExitCode }}; exit 0",
        ps_single_quote(program),
        argument_list
    );
    vec![
        "-NoLogo".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-Command".to_string(),
        script,
    ]
}

fn ps_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn run_action_command_owned(
    program: &str,
    args: Vec<String>,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_action_command(program, &args, progress)
}

fn run_streaming_command(
    mut command: std::process::Command,
    missing_command_name: &str,
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => return Ok(start_failed_output(missing_command_name, err)),
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = mpsc::channel::<(&'static str, String)>();

    if let Some(stdout) = stdout {
        let tx = tx.clone();
        std::thread::spawn(move || read_stream_chunks(stdout, "stdout", tx));
    }
    if let Some(stderr) = stderr {
        let tx = tx.clone();
        std::thread::spawn(move || read_stream_chunks(stderr, "stderr", tx));
    }
    drop(tx);

    let mut stdout = String::new();
    let mut stderr = String::new();
    for (stream, chunk) in rx {
        if stream == "stdout" {
            stdout.push_str(&chunk);
        } else {
            stderr.push_str(&chunk);
        }
        emit_install_progress(progress, stream, chunk, None, false);
    }

    let status = child
        .wait()
        .map_err(|err| format!("Failed to wait for install command: {err}"))?;
    let exit_code = status.code();
    emit_install_progress(progress, "status", String::new(), exit_code, true);
    Ok(InstallCommandOutput {
        success: status.success(),
        exit_code,
        stdout_tail: tail(&stdout),
        stderr_tail: describe_command_failure(missing_command_name, exit_code, &stderr),
        missing_command: None,
    })
}

fn describe_command_failure(command_name: &str, exit_code: Option<i32>, stderr: &str) -> String {
    let stderr_tail = tail(stderr);
    if command_name.eq_ignore_ascii_case("winget")
        && exit_code == Some(WINGET_MULTIPLE_PACKAGES_EXIT_CODE)
    {
        let note = format!(
            "winget returned {WINGET_MULTIPLE_PACKAGES_HEX}: Multiple packages match the uninstall query. Refresh detection and uninstall the specific Claude Desktop install kind from CodeStudio Lite, or remove the matching Claude package from Windows Settings."
        );
        if stderr_tail.is_empty() {
            return note;
        }
        return format!("{stderr_tail}\n{note}");
    }
    stderr_tail
}

fn read_stream_chunks<R: Read + Send + 'static>(
    mut reader: R,
    stream: &'static str,
    tx: mpsc::Sender<(&'static str, String)>,
) {
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => {
                let _ = tx.send((stream, decode(&buffer[..size])));
            }
            Err(err) => {
                let _ = tx.send((stream, format!("Failed to read {stream}: {err}\n")));
                break;
            }
        }
    }
}

fn emit_install_progress(
    context: Option<&InstallProgressContext>,
    stream: &str,
    chunk: String,
    exit_code: Option<i32>,
    done: bool,
) {
    let Some(context) = context else {
        return;
    };
    let Some(progress) = context.progress else {
        return;
    };
    progress(ToolInstallProgress {
        root_tool_id: context.root_tool_id.to_string(),
        tool_id: context.tool_id.to_string(),
        tool_name: context.tool_name.to_string(),
        stage: context.stage.to_string(),
        command: context.command.to_string(),
        install_kind: context.install_kind.map(ToString::to_string),
        phase: context.progress_phase.map(ToString::to_string),
        message: context.progress_message.map(ToString::to_string),
        downloaded: None,
        total: None,
        percent: None,
        step: context.progress_step,
        step_total: context.progress_step_total,
        stream: stream.to_string(),
        chunk,
        done,
        exit_code,
    });
}

fn missing_command_output(command: &str) -> InstallCommandOutput {
    InstallCommandOutput {
        success: false,
        exit_code: None,
        stdout_tail: String::new(),
        stderr_tail: format!(
            "Command is unavailable: {command}. It may have been moved or uninstalled."
        ),
        missing_command: Some(command.to_string()),
    }
}

fn start_failed_output(command: &str, err: std::io::Error) -> InstallCommandOutput {
    InstallCommandOutput {
        success: false,
        exit_code: None,
        stdout_tail: String::new(),
        stderr_tail: format!("Failed to start command: {err}"),
        missing_command: Some(command.to_string()),
    }
}

fn refresh_process_environment_after_install(notes: &mut Vec<String>) {
    match refresh_process_path_from_registry() {
        Ok(true) => push_note_once(notes, "Refreshed the current app process PATH, so later detection does not require restarting the app."),
        Ok(false) => {}
        Err(err) => push_note_once(notes, &format!("Failed to refresh the current app process PATH: {err}")),
    }
}

fn push_note_once(notes: &mut Vec<String>, note: &str) {
    if !notes.iter().any(|item| item == note) {
        notes.push(note.to_string());
    }
}

fn refresh_process_path_from_registry() -> Result<bool, String> {
    if !cfg!(windows) {
        return Ok(false);
    }
    let script = r#"
$machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$user = [Environment]::GetEnvironmentVariable('Path', 'User')
$current = [Environment]::GetEnvironmentVariable('Path', 'Process')
$parts = @()
foreach ($value in @($machine, $user, $current)) {
  if ($value) { $parts += ($value -split ';') }
}
$seen = @{}
$merged = @()
foreach ($part in $parts) {
  $trimmed = ([string]$part).Trim()
  if (-not $trimmed) { continue }
  $key = $trimmed.ToLowerInvariant()
  if (-not $seen.ContainsKey($key)) {
    $seen[$key] = $true
    $merged += $trimmed
  }
}
[Console]::Write(($merged -join ';'))
"#;
    let output = hidden_command_with_args(
        "powershell.exe",
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
    .output()
    .map_err(|err| format!("Failed to start PowerShell to refresh PATH: {err}"))?;
    if !output.status.success() {
        return Err(decode(&output.stderr).trim().to_string());
    }
    let next_path = decode(&output.stdout).trim().to_string();
    if next_path.is_empty() {
        return Ok(false);
    }
    let current_path = env::var("PATH").unwrap_or_default();
    if current_path == next_path {
        return Ok(false);
    }
    env::set_var("PATH", next_path);
    Ok(true)
}

fn manager_label(action: &InstallAction) -> &'static str {
    match action {
        InstallAction::NpmGlobal(_) => "npm",
        InstallAction::Winget(_) => "winget",
        InstallAction::MacosDmgApp { .. } => "official-dmg",
        InstallAction::ClaudeDesktopWindowsMsix => "official-msix",
        InstallAction::PowerShellScript(_, _) => "powershell",
        InstallAction::ShellScript(_, _) => "shell",
        InstallAction::InteractiveShellScript(_, _) => "terminal",
        InstallAction::VsCodeExtension(_) => "vscode",
        InstallAction::ProvidedByTool(provider_tool_id) => install_definition(provider_tool_id)
            .map(|definition| manager_label(&definition.action))
            .unwrap_or("dependency"),
        InstallAction::CustomUnsupported(_) => "manual",
    }
}

fn command_preview(action: &InstallAction) -> String {
    match action {
        InstallAction::NpmGlobal(package) => format!("npm install -g {package}"),
        InstallAction::Winget(package_id) => {
            format!("winget install --id {package_id} --exact --accept-source-agreements --accept-package-agreements --disable-interactivity")
        }
        InstallAction::MacosDmgApp { label, .. } => {
            format!("Download and install the latest {label} from downloads.claude.ai")
        }
        InstallAction::ClaudeDesktopWindowsMsix => {
            format!(
                "Download and install the latest Claude Desktop MSIX from {CLAUDE_DESKTOP_WINDOWS_MSIX_URL} with Add-AppxPackage -Path"
            )
        }
        InstallAction::PowerShellScript(_, script) => {
            format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{script}\"")
        }
        InstallAction::ShellScript(_, script) => format!("bash -lc '{script}'"),
        InstallAction::InteractiveShellScript(_, script) => script.to_string(),
        InstallAction::VsCodeExtension(extension_id) => {
            format!("code --install-extension {extension_id}")
        }
        InstallAction::ProvidedByTool(provider_tool_id) => install_definition(provider_tool_id)
            .map(|definition| command_preview(&definition.action))
            .unwrap_or_else(|| format!("Provided by {provider_tool_id}")),
        InstallAction::CustomUnsupported(reason) => reason.to_string(),
    }
}

fn update_supported(action: &InstallAction) -> bool {
    !matches!(action, InstallAction::CustomUnsupported(_))
}

fn update_command_preview(action: &InstallAction) -> String {
    match action {
        InstallAction::NpmGlobal(package) => format!("npm install -g {package}@latest"),
        InstallAction::Winget(package_id) => {
            format!("winget upgrade --id {package_id} --exact --accept-source-agreements --accept-package-agreements --disable-interactivity")
        }
        InstallAction::MacosDmgApp { label, .. } => {
            format!("Download and install the latest {label} from downloads.claude.ai")
        }
        InstallAction::ClaudeDesktopWindowsMsix => {
            format!(
                "Download and install the latest Claude Desktop MSIX from {CLAUDE_DESKTOP_WINDOWS_MSIX_URL} with Add-AppxPackage -Path"
            )
        }
        InstallAction::PowerShellScript(_, script) => {
            format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{script}\"")
        }
        InstallAction::ShellScript(_, script) => format!("bash -lc '{script}'"),
        InstallAction::InteractiveShellScript(_, script) => script.to_string(),
        InstallAction::VsCodeExtension(extension_id) => {
            format!("code --install-extension {extension_id} --force")
        }
        InstallAction::ProvidedByTool(provider_tool_id) => install_definition(provider_tool_id)
            .map(|definition| update_command_preview(&definition.action))
            .unwrap_or_else(|| format!("Provided by {provider_tool_id}")),
        InstallAction::CustomUnsupported(reason) => reason.to_string(),
    }
}

fn update_supported_for_tool(tool_id: &str, action: &InstallAction) -> bool {
    tool_id == "npm" || update_supported(action)
}

fn uninstall_supported_for_tool(tool_id: &str, action: &InstallAction) -> bool {
    if tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        return true;
    }
    matches!(
        action,
        InstallAction::NpmGlobal(_)
            | InstallAction::Winget(_)
            | InstallAction::MacosDmgApp { .. }
            | InstallAction::ClaudeDesktopWindowsMsix
            | InstallAction::VsCodeExtension(_)
    )
}

fn update_command_preview_for_tool(tool_id: &str, action: &InstallAction) -> String {
    if tool_id == "npm" {
        return "npm install -g npm@latest".to_string();
    }
    if tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        return CLAUDE_DESKTOP_WINDOWS_UPDATE_COMMAND.to_string();
    }
    update_command_preview(action)
}

fn uninstall_command_preview_for_tool(tool_id: &str, action: &InstallAction) -> String {
    if tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        return CLAUDE_DESKTOP_WINDOWS_UNINSTALL_COMMAND.to_string();
    }
    match action {
        InstallAction::NpmGlobal(package) => format!("npm uninstall -g {package}"),
        InstallAction::Winget(package_id) => {
            format!("winget uninstall --id {package_id} --exact")
        }
        InstallAction::MacosDmgApp { destination, .. } => {
            format!("Remove macOS app bundle at {destination}")
        }
        InstallAction::VsCodeExtension(extension_id) => {
            format!("code --uninstall-extension {extension_id}")
        }
        InstallAction::PowerShellScript(_, _)
        | InstallAction::ClaudeDesktopWindowsMsix
        | InstallAction::ShellScript(_, _)
        | InstallAction::InteractiveShellScript(_, _)
        | InstallAction::ProvidedByTool(_)
        | InstallAction::CustomUnsupported(_) => {
            "No built-in uninstall action is available.".to_string()
        }
    }
}

fn action_requires_admin(action: &InstallAction) -> bool {
    match action {
        InstallAction::Winget(_) => true,
        InstallAction::MacosDmgApp { destination, .. } => destination.starts_with("/Applications/"),
        InstallAction::ClaudeDesktopWindowsMsix => false,
        InstallAction::ShellScript(_, script) => script.contains("sudo "),
        InstallAction::InteractiveShellScript(_, script) => script.contains("sudo "),
        _ => false,
    }
}

fn action_interactive(action: &InstallAction) -> bool {
    matches!(action, InstallAction::InteractiveShellScript(_, _))
}

fn close_processes_before_update(
    tool_id: &str,
    tool_name: &str,
) -> Result<process_control::ProcessTerminationReport, String> {
    let targets = update_process_targets(tool_id);
    if targets.process_names.is_empty() && targets.command_line_markers.is_empty() {
        return Ok(process_control::ProcessTerminationReport::default());
    }
    if tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        return process_control::close_appx_packages_for_update(
            tool_name,
            detector::claude_desktop_windows_package_identities(),
        );
    }
    process_control::close_processes(
        tool_name,
        &targets.process_names,
        &targets.command_line_markers,
        None,
        8,
    )
}

struct UpdateProcessTargets {
    process_names: Vec<&'static str>,
    command_line_markers: Vec<&'static str>,
}

fn update_process_targets(tool_id: &str) -> UpdateProcessTargets {
    match tool_id {
        "codex" => UpdateProcessTargets {
            process_names: if cfg!(target_os = "windows") {
                Vec::new()
            } else {
                vec!["codex"]
            },
            command_line_markers: vec!["@openai/codex"],
        },
        "codex-vscode" | "claude-vscode" | "gemini-code-assist" => UpdateProcessTargets {
            process_names: vec!["Code", "Code - Insiders"],
            command_line_markers: Vec::new(),
        },
        "claude-desktop" => UpdateProcessTargets {
            process_names: vec!["Claude"],
            command_line_markers: Vec::new(),
        },
        "claude" => UpdateProcessTargets {
            process_names: if cfg!(target_os = "windows") {
                Vec::new()
            } else {
                vec!["claude"]
            },
            command_line_markers: vec!["@anthropic-ai/claude-code"],
        },
        "gemini" => UpdateProcessTargets {
            process_names: vec!["gemini"],
            command_line_markers: vec!["@google/gemini-cli"],
        },
        "opencode" => UpdateProcessTargets {
            process_names: vec!["opencode"],
            command_line_markers: vec!["opencode-ai"],
        },
        "openclaw" => UpdateProcessTargets {
            process_names: vec!["openclaw"],
            command_line_markers: vec!["openclaw"],
        },
        "hermes" => UpdateProcessTargets {
            process_names: vec!["hermes", "Hermes"],
            command_line_markers: vec!["hermes-agent", "hermes"],
        },
        "node" => UpdateProcessTargets {
            process_names: vec!["node"],
            command_line_markers: Vec::new(),
        },
        "git" => UpdateProcessTargets {
            process_names: vec!["git", "git-gui", "gitk"],
            command_line_markers: Vec::new(),
        },
        "npm" => UpdateProcessTargets {
            process_names: vec!["npm"],
            command_line_markers: vec!["npm-cli.js"],
        },
        "pnpm" => UpdateProcessTargets {
            process_names: vec!["pnpm"],
            command_line_markers: vec!["pnpm"],
        },
        "bun" => UpdateProcessTargets {
            process_names: vec!["bun"],
            command_line_markers: Vec::new(),
        },
        _ => UpdateProcessTargets {
            process_names: Vec::new(),
            command_line_markers: Vec::new(),
        },
    }
}

fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn tail(value: &str) -> String {
    let lines = value
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let start = lines.len().saturating_sub(20);
    lines[start..].join("\n")
}

#[cfg(test)]
#[path = "tool_installer_tests.rs"]
mod tool_installer_tests;
