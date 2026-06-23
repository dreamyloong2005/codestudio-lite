use crate::core::activity_log;
use crate::core::app_paths::app_paths;
use crate::core::detector;
use crate::core::env_health;
use crate::core::platform::{hidden_command, hidden_command_with_args, package, resolve_command};
use crate::core::process_control;
use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};
use crate::core::types::{
    InstallState, RepairToolPathRequest, RepairToolPathResult, Severity, ToolInstallCommand,
    ToolInstallPlan, ToolInstallPrerequisite, ToolInstallProgress, ToolInstallRequest,
    ToolInstallResult, ToolInstallStageResult, ToolInstallStep, ToolStatus, ToolUninstallRequest,
};
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
        progress,
    };
    // Prefer the install kind the caller selected (per the page tab) over the
    // detected one, so uninstalling targets the version the user is viewing.
    let install_kind = request
        .install_kind
        .as_deref()
        .or_else(|| before.as_ref().and_then(|s| s.install_kind.as_deref()));
    let output = if tool_id == "claude-desktop"
        && cfg!(target_os = "windows")
        && install_kind == Some("exe")
    {
        run_claude_desktop_exe_uninstall(Some(&context))?
    } else {
        run_uninstall_action_for_tool(&tool_id, &definition.action, Some(&context))?
    };
    if output.success {
        refresh_process_environment_after_install(&mut notes);
    }

    detector::invalidate_update_cache();
    let after = current_status_for_missing_command(output.missing_command.as_deref(), &tool_id);
    let uninstalled = after
        .as_ref()
        .map(|status| status.install_state != InstallState::Installed)
        .unwrap_or(true);
    let success = output.success && uninstalled;
    let message = if success {
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
                InstallAction::Winget("Anthropic.Claude")
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
        return run_action_command(
            "powershell",
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                CLAUDE_DESKTOP_WINDOWS_UPDATE_SCRIPT,
            ],
            progress,
        );
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
        | InstallAction::ShellScript(_, _)
        | InstallAction::InteractiveShellScript(_, _)
        | InstallAction::ProvidedByTool(_)
        | InstallAction::CustomUnsupported(_) => {
            Err("This tool has no executable standalone uninstall action.".to_string())
        }
    }
}

const CLAUDE_DESKTOP_WINDOWS_UPDATE_COMMAND: &str =
    "Download and run the latest Claude Desktop installer from downloads.claude.ai";

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

const CLAUDE_DESKTOP_WINDOWS_UPDATE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$latest = Invoke-RestMethod -Uri 'https://downloads.claude.ai/releases/win32/x64/.latest' -Headers @{ 'User-Agent' = 'CodeStudio Lite' }
$version = [string]$latest.version
$hash = [string]$latest.hash
if ([string]::IsNullOrWhiteSpace($version) -or [string]::IsNullOrWhiteSpace($hash)) {
  throw 'Claude Desktop latest metadata is incomplete.'
}
$url = "https://downloads.claude.ai/releases/win32/x64/$version/Claude-$hash.exe"
$target = Join-Path $env:TEMP "Claude-$version.exe"
Write-Output "Downloading Claude Desktop $version"
Invoke-WebRequest -Uri $url -OutFile $target -Headers @{ 'User-Agent' = 'CodeStudio Lite' }
if (-not (Test-Path -LiteralPath $target)) {
  throw "Claude Desktop installer was not downloaded."
}
$item = Get-Item -LiteralPath $target
if ($item.Length -le 0) {
  throw "Claude Desktop installer is empty."
}
Write-Output "Starting Claude Desktop installer"
$process = Start-Process -FilePath $target -WorkingDirectory ([System.IO.Path]::GetDirectoryName($target)) -Wait -PassThru
$exitCode = $process.ExitCode
Remove-Item -LiteralPath $target -Force -ErrorAction SilentlyContinue
if ($null -ne $exitCode -and $exitCode -ne 0) {
  exit $exitCode
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
foreach ($root in $roots) {
  $props = Get-ItemProperty $root -ErrorAction SilentlyContinue
  foreach ($prop in $props) {
    if ($prop.DisplayName -and $prop.DisplayName -like '*Claude*') {
      $entry = $prop
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
} else {
  Write-Output "No registry uninstall entry found, attempting direct removal"
}
$claudeDir = Join-Path $env:LOCALAPPDATA 'AnthropicClaude'
if (Test-Path -LiteralPath $claudeDir) {
  Write-Output "Removing $claudeDir"
  Remove-Item -LiteralPath $claudeDir -Recurse -Force -ErrorAction SilentlyContinue
}
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Claude.lnk'
if (Test-Path -LiteralPath $startMenu) {
  Remove-Item -LiteralPath $startMenu -Force -ErrorAction SilentlyContinue
}
Write-Output "Done"
"#;

fn run_claude_desktop_exe_uninstall(
    progress: Option<&InstallProgressContext>,
) -> Result<InstallCommandOutput, String> {
    run_action_command(
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            CLAUDE_DESKTOP_WINDOWS_EXE_UNINSTALL_SCRIPT,
        ],
        progress,
    )
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

    emit_install_progress(
        progress,
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
        .map_err(|err| format!("Failed to download Claude Desktop DMG: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download Claude Desktop DMG: HTTP {}",
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
            .map_err(|err| format!("Failed while downloading Claude Desktop DMG: {err}"))?;
        if size == 0 {
            break;
        }
        file.write_all(&buffer[..size])
            .map_err(|err| format!("Failed to write Claude Desktop DMG: {err}"))?;
        downloaded += size as u64;
        if last_emit.elapsed() >= Duration::from_millis(750) {
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
        .map_err(|err| format!("Failed to finish Claude Desktop DMG download: {err}"))?;
    fs::rename(&temp, path).map_err(|err| format!("Failed to save Claude Desktop DMG: {err}"))?;
    emit_install_progress(
        progress,
        "stdout",
        format_download_progress(downloaded, total),
        None,
        false,
    );
    Ok(())
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

#[cfg(windows)]
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
    emit_install_progress(progress, "status", String::new(), status.code(), true);
    Ok(InstallCommandOutput {
        success: status.success(),
        exit_code: status.code(),
        stdout_tail: tail(&stdout),
        stderr_tail: tail(&stderr),
        missing_command: None,
    })
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

fn uninstall_command_preview_for_tool(_tool_id: &str, action: &InstallAction) -> String {
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
mod tests {
    use super::*;
    use crate::core::types::{ConfigState, ToolCategory};

    #[test]
    fn npm_tool_plan_is_whitelisted() {
        let definition = install_definition("claude").expect("definition");
        let plan = build_plan(&definition, None);
        assert_eq!(plan.manager, "npm");
        assert!(plan.command.contains("@anthropic-ai/claude-code"));
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
            assert!(command.contains("downloads.claude.ai"));
            assert!(!command.contains("winget"));
        } else {
            assert_eq!(command, update_command_preview(&definition.action));
        }
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
}
