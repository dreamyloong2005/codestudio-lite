use crate::core::activity_log;
use crate::core::detector;
use crate::core::env_health;
use crate::core::platform::{hidden_command_with_args, resolve_command};
use crate::core::process_control;
use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};
use crate::core::types::{
    InstallState, RepairToolPathRequest, RepairToolPathResult, Severity, ToolInstallCommand,
    ToolInstallPlan, ToolInstallPrerequisite, ToolInstallProgress, ToolInstallRequest,
    ToolInstallResult, ToolInstallStageResult, ToolInstallStep, ToolStatus, ToolUninstallRequest,
};
use std::env;
use std::io::Read;
use std::process::Stdio;
use std::sync::mpsc;

#[derive(Debug, Clone)]
enum InstallAction {
    NpmGlobal(&'static str),
    Winget(&'static str),
    HomebrewFormula(&'static str),
    HomebrewCask(&'static str),
    // Reserved install action for a bundled PowerShell script. The match arms
    // across plan/run/preview already handle it; no tool currently constructs
    // it, so it is intentionally dead until a tool opts in.
    #[allow(dead_code)]
    PowerShellScript(&'static str, &'static str),
    ShellScript(&'static str, &'static str),
    InteractiveShellScript(&'static str, &'static str),
    VsCodeExtension(&'static str),
    ProvidedBy(&'static str),
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
                InstallAction::HomebrewCask("claude")
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
                InstallAction::HomebrewFormula("hermes-agent")
            } else if cfg!(target_os = "linux") {
                InstallAction::InteractiveShellScript(
                    "Hermes official install script",
                    "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash",
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
                InstallAction::HomebrewFormula("node")
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
                InstallAction::HomebrewFormula("git")
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
                InstallAction::HomebrewFormula("bun")
            } else if cfg!(target_os = "linux") {
                InstallAction::ShellScript(
                    "Bun official install script",
                    "curl -fsSL https://bun.sh/install | bash",
                )
            } else {
                InstallAction::Winget("Oven-sh.Bun")
            }
        }
        "npm" => InstallAction::ProvidedBy("Node.js LTS"),
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
        InstallAction::HomebrewFormula(formula) => {
            steps.push(ToolInstallStep {
                label: "Check Homebrew".to_string(),
                detail: "The local brew command must be available.".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Install formula".to_string(),
                detail: format!("Install {formula} through Homebrew."),
            });
            steps.push(ToolInstallStep {
                label: "Verify command".to_string(),
                detail: format!(
                    "After installation, run {} --version and refresh the dashboard.",
                    definition.tool.command
                ),
            });
            if !cfg!(target_os = "macos") {
                blocker =
                    Some("The Homebrew installer is currently enabled only on macOS.".to_string());
                can_install = false;
            } else if !command_available("brew") {
                blocker = Some(
                    "brew is not available. Install Homebrew first, or install manually."
                        .to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::HomebrewCask(cask) => {
            steps.push(ToolInstallStep {
                label: "Check Homebrew".to_string(),
                detail: "The local brew command must be available.".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "Install app".to_string(),
                detail: format!("Install {cask} through Homebrew Cask."),
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
                    "The Homebrew Cask installer is currently enabled only on macOS.".to_string(),
                );
                can_install = false;
            } else if !command_available("brew") {
                blocker = Some(
                    "brew is not available. Install Homebrew first, or install manually."
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
        InstallAction::ProvidedBy(provider) => {
            blocker = Some(format!(
                "{} is provided by {provider}; install {provider}.",
                definition.tool.name
            ));
            can_install = false;
            steps.push(ToolInstallStep {
                label: "Install upstream dependency".to_string(),
                detail: format!(
                    "{} does not have a standalone installer.",
                    definition.tool.name
                ),
            });
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
    commands.push(command_entry(definition, "target"));
    let requires_prerequisites = prerequisites
        .iter()
        .any(|prerequisite| !prerequisite.installed);
    let requires_admin = action_requires_admin(&definition.action)
        || prerequisites
            .iter()
            .filter(|prerequisite| !prerequisite.installed)
            .filter_map(|prerequisite| install_definition(&prerequisite.tool_id))
            .any(|definition| action_requires_admin(&definition.action));
    let interactive = action_interactive(&definition.action);

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

fn dependency_satisfied(action: &InstallAction) -> bool {
    match action {
        InstallAction::NpmGlobal(_) => command_available("npm"),
        InstallAction::HomebrewFormula(_) | InstallAction::HomebrewCask(_) => {
            command_available("brew")
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
        InstallAction::HomebrewFormula(formula) => {
            run_action_command("brew", &["install", formula], progress)
        }
        InstallAction::HomebrewCask(cask) => {
            run_action_command("brew", &["install", "--cask", cask], progress)
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
        InstallAction::ProvidedBy(_) | InstallAction::CustomUnsupported(_) => {
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
        InstallAction::HomebrewFormula(formula) => {
            run_action_command("brew", &["upgrade", formula], progress)
        }
        InstallAction::HomebrewCask(cask) => {
            run_action_command("brew", &["upgrade", "--cask", cask], progress)
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
        InstallAction::ProvidedBy(_) | InstallAction::CustomUnsupported(_) => {
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
        InstallAction::HomebrewCask(cask) => {
            run_action_command("brew", &["uninstall", "--cask", cask], progress)
        }
        InstallAction::HomebrewFormula(formula) => {
            run_action_command("brew", &["uninstall", formula], progress)
        }
        InstallAction::VsCodeExtension(extension_id) => {
            run_action_command("code", &["--uninstall-extension", extension_id], progress)
        }
        InstallAction::NpmGlobal(package) => {
            run_action_command("npm", &["uninstall", "-g", package], progress)
        }
        InstallAction::PowerShellScript(_, _)
        | InstallAction::ShellScript(_, _)
        | InstallAction::InteractiveShellScript(_, _)
        | InstallAction::ProvidedBy(_)
        | InstallAction::CustomUnsupported(_) => {
            Err("This tool has no executable standalone uninstall action.".to_string())
        }
    }
}

const CLAUDE_DESKTOP_WINDOWS_UPDATE_COMMAND: &str =
    "Download and run the latest Claude Desktop installer from downloads.claude.ai";

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
if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) {
  exit $process.ExitCode
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
        InstallAction::HomebrewFormula(_) | InstallAction::HomebrewCask(_) => "homebrew",
        InstallAction::PowerShellScript(_, _) => "powershell",
        InstallAction::ShellScript(_, _) => "shell",
        InstallAction::InteractiveShellScript(_, _) => "terminal",
        InstallAction::VsCodeExtension(_) => "vscode",
        InstallAction::ProvidedBy(_) => "dependency",
        InstallAction::CustomUnsupported(_) => "manual",
    }
}

fn command_preview(action: &InstallAction) -> String {
    match action {
        InstallAction::NpmGlobal(package) => format!("npm install -g {package}"),
        InstallAction::Winget(package_id) => {
            format!("winget install --id {package_id} --exact --accept-source-agreements --accept-package-agreements --disable-interactivity")
        }
        InstallAction::HomebrewFormula(formula) => format!("brew install {formula}"),
        InstallAction::HomebrewCask(cask) => format!("brew install --cask {cask}"),
        InstallAction::PowerShellScript(_, script) => {
            format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{script}\"")
        }
        InstallAction::ShellScript(_, script) => format!("bash -lc '{script}'"),
        InstallAction::InteractiveShellScript(_, script) => script.to_string(),
        InstallAction::VsCodeExtension(extension_id) => {
            format!("code --install-extension {extension_id}")
        }
        InstallAction::ProvidedBy(provider) => format!("Provided by {provider}"),
        InstallAction::CustomUnsupported(reason) => reason.to_string(),
    }
}

fn update_supported(action: &InstallAction) -> bool {
    !matches!(
        action,
        InstallAction::ProvidedBy(_) | InstallAction::CustomUnsupported(_)
    )
}

fn update_command_preview(action: &InstallAction) -> String {
    match action {
        InstallAction::NpmGlobal(package) => format!("npm install -g {package}@latest"),
        InstallAction::Winget(package_id) => {
            format!("winget upgrade --id {package_id} --exact --accept-source-agreements --accept-package-agreements --disable-interactivity")
        }
        InstallAction::HomebrewFormula(formula) => format!("brew upgrade {formula}"),
        InstallAction::HomebrewCask(cask) => format!("brew upgrade --cask {cask}"),
        InstallAction::PowerShellScript(_, script) => {
            format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{script}\"")
        }
        InstallAction::ShellScript(_, script) => format!("bash -lc '{script}'"),
        InstallAction::InteractiveShellScript(_, script) => script.to_string(),
        InstallAction::VsCodeExtension(extension_id) => {
            format!("code --install-extension {extension_id} --force")
        }
        InstallAction::ProvidedBy(provider) => format!("Provided by {provider}"),
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
            | InstallAction::HomebrewFormula(_)
            | InstallAction::HomebrewCask(_)
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
        InstallAction::HomebrewFormula(formula) => format!("brew uninstall {formula}"),
        InstallAction::HomebrewCask(cask) => format!("brew uninstall --cask {cask}"),
        InstallAction::VsCodeExtension(extension_id) => {
            format!("code --uninstall-extension {extension_id}")
        }
        InstallAction::PowerShellScript(_, _)
        | InstallAction::ShellScript(_, _)
        | InstallAction::InteractiveShellScript(_, _)
        | InstallAction::ProvidedBy(_)
        | InstallAction::CustomUnsupported(_) => {
            "No built-in uninstall action is available.".to_string()
        }
    }
}

fn action_requires_admin(action: &InstallAction) -> bool {
    match action {
        InstallAction::Winget(_) => true,
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
            process_names: if cfg!(target_os = "windows") { Vec::new() } else { vec!["codex"] },
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
            process_names: if cfg!(target_os = "windows") { Vec::new() } else { vec!["claude"] },
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
        assert!(!plan.can_install);
        assert!(plan.blocker.unwrap().contains("Node.js"));
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
    fn homebrew_actions_render_expected_commands() {
        assert_eq!(
            command_preview(&InstallAction::HomebrewFormula("node")),
            "brew install node"
        );
        assert_eq!(
            command_preview(&InstallAction::HomebrewCask("claude")),
            "brew install --cask claude"
        );
        assert_eq!(
            update_command_preview(&InstallAction::HomebrewFormula("bun")),
            "brew upgrade bun"
        );
        assert_eq!(
            update_command_preview(&InstallAction::HomebrewCask("claude")),
            "brew upgrade --cask claude"
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
