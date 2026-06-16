use crate::core::activity_log;
use crate::core::detector;
use crate::core::env_health;
use crate::core::platform::{hidden_command_with_args, resolve_command};
use crate::core::process_control;
use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};
use crate::core::types::{
    InstallState, RepairToolPathRequest, RepairToolPathResult, Severity, ToolInstallCommand,
    ToolInstallPlan, ToolInstallPrerequisite, ToolInstallRequest, ToolInstallResult,
    ToolInstallStageResult, ToolInstallStep, ToolStatus,
};
use std::env;

#[derive(Debug, Clone)]
enum InstallAction {
    NpmGlobal(&'static str),
    Winget(&'static str),
    HomebrewFormula(&'static str),
    HomebrewCask(&'static str),
    PowerShellScript(&'static str, &'static str),
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

pub fn plan_tool_install(tool_id: &str) -> Result<ToolInstallPlan, String> {
    let definition = install_definition(tool_id)
        .ok_or_else(|| format!("工具 '{tool_id}' 不在安装白名单中。"))?;
    let current_status = current_status(tool_id).ok();
    Ok(build_plan(&definition, current_status.as_ref()))
}

pub fn install_tool(request: ToolInstallRequest) -> Result<ToolInstallResult, String> {
    if !request.confirm {
        return Err("拒绝执行：安装软件前必须显式确认。".to_string());
    }

    let definition = install_definition(&request.tool_id)
        .ok_or_else(|| format!("工具 '{}' 不在安装白名单中。", request.tool_id))?;
    let before = current_status(&request.tool_id).ok();
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
                .unwrap_or_else(|| "安装计划不可执行。".to_string()),
            command: plan.command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: plan.warnings,
        });
    }

    if plan.requires_prerequisites && !request.install_prerequisites {
        return Ok(ToolInstallResult {
            success: false,
            tool_id: plan.tool_id,
            tool_name: plan.tool_name,
            action: "prerequisites-required".to_string(),
            message: "安装此工具前需要安装前置依赖，请勾选允许安装前置后再继续。".to_string(),
            command: plan.command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: plan.warnings,
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
    let mut notes = plan.warnings.clone();

    for prerequisite in &plan.prerequisites {
        if prerequisite.installed {
            continue;
        }

        let prerequisite_definition = install_definition(&prerequisite.tool_id)
            .ok_or_else(|| format!("前置依赖 '{}' 不在安装白名单中。", prerequisite.tool_id))?;
        let output = run_install_action(&prerequisite_definition.action)?;
        let missing_command = output.missing_command.clone();
        if output.success {
            refresh_process_environment_after_install(&mut notes);
        }
        let verified = dependency_satisfied(&definition.action);
        let success = output.success && verified;
        let message = if success {
            format!("{} 前置依赖安装完成。", prerequisite.tool_name)
        } else if output.success {
            format!(
                "{} 安装命令已结束，但尚未检测到目标工具所需前置命令。请检查 PATH 或安装日志后刷新。",
                prerequisite.tool_name
            )
        } else {
            format!("{} 前置依赖安装失败。", prerequisite.tool_name)
        };
        stage_results.push(ToolInstallStageResult {
            tool_id: prerequisite.tool_id.clone(),
            tool_name: prerequisite.tool_name.clone(),
            stage: "prerequisite".to_string(),
            command: command_preview(&prerequisite_definition.action),
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
                tool_id: request.tool_id,
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

    let output = run_install_action(&definition.action)?;
    if output.success {
        refresh_process_environment_after_install(&mut notes);
    }

    detector::invalidate_update_cache();
    let after =
        current_status_for_missing_command(output.missing_command.as_deref(), &request.tool_id);
    let verified = after
        .as_ref()
        .map(|status| status.install_state == InstallState::Installed)
        .unwrap_or(false);
    let process_success = output.success;
    let success = process_success && verified;
    let exit_code = output.exit_code;
    let message = if success {
        format!("{} 安装完成并通过复检。", definition.tool.name)
    } else if process_success {
        format!(
            "{} 安装命令已结束，但复检仍未确认可用。请检查 PATH 或安装日志后刷新。",
            definition.tool.name
        )
    } else {
        format!("{} 安装失败。", definition.tool.name)
    };
    let level = if success {
        Severity::Ok
    } else {
        Severity::Warning
    };
    let _ = activity_log::append(level, message.clone());

    stage_results.push(ToolInstallStageResult {
        tool_id: request.tool_id.clone(),
        tool_name: definition.tool.name.to_string(),
        stage: "target".to_string(),
        command: command_preview(&definition.action),
        success,
        exit_code,
        stdout_tail: output.stdout_tail.clone(),
        stderr_tail: output.stderr_tail.clone(),
        message: message.clone(),
    });

    Ok(ToolInstallResult {
        success,
        tool_id: request.tool_id,
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
    if !request.confirm {
        return Err("拒绝执行：更新软件前必须显式确认。".to_string());
    }

    let definition = install_definition(&request.tool_id)
        .ok_or_else(|| format!("工具 '{}' 不在更新白名单中。", request.tool_id))?;
    let before = current_status(&request.tool_id).ok();
    let command = update_command_preview_for_tool(&request.tool_id, &definition.action);

    if before
        .as_ref()
        .map(|status| status.install_state != InstallState::Installed)
        .unwrap_or(true)
    {
        return Ok(ToolInstallResult {
            success: false,
            tool_id: request.tool_id,
            tool_name: definition.tool.name.to_string(),
            action: "blocked".to_string(),
            message: format!("{} 未安装，无法更新。", definition.tool.name),
            command,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            current_status: before,
            stage_results: Vec::new(),
            notes: Vec::new(),
        });
    }

    if !update_supported_for_tool(&request.tool_id, &definition.action) {
        return Ok(ToolInstallResult {
            success: false,
            tool_id: request.tool_id,
            tool_name: definition.tool.name.to_string(),
            action: "blocked".to_string(),
            message: format!("{} 暂无内置更新动作。", definition.tool.name),
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
    let termination = close_processes_before_update(&request.tool_id, definition.tool.name)?;
    if let Some(note) = termination.note(definition.tool.name) {
        let _ = activity_log::append(Severity::Info, note.clone());
        notes.push(note);
    }

    let output = run_update_action_for_tool(&request.tool_id, &definition.action)?;
    if output.success {
        refresh_process_environment_after_install(&mut notes);
    }
    detector::invalidate_update_cache();
    let after =
        current_status_for_missing_command(output.missing_command.as_deref(), &request.tool_id);
    let verified = after
        .as_ref()
        .map(|status| status.install_state == InstallState::Installed)
        .unwrap_or(false);
    let success = output.success && verified;
    let exit_code = output.exit_code;
    let message = if success {
        format!("{} 更新命令已完成并通过复检。", definition.tool.name)
    } else if output.success {
        format!(
            "{} 更新命令已结束，但复检仍未确认可用。请检查 PATH 或安装日志后刷新。",
            definition.tool.name
        )
    } else {
        format!("{} 更新失败。", definition.tool.name)
    };
    let level = if success {
        Severity::Ok
    } else {
        Severity::Warning
    };
    let _ = activity_log::append(level, message.clone());

    let stage_results = vec![ToolInstallStageResult {
        tool_id: request.tool_id.clone(),
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
        tool_id: request.tool_id,
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
            } else {
                InstallAction::Winget("Anthropic.Claude")
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
            } else {
                InstallAction::PowerShellScript(
                    "Hermes 官方安装脚本",
                    "iex (irm https://hermes-agent.nousresearch.com/install.ps1)",
                )
            }
        }
        "node" => {
            if cfg!(target_os = "macos") {
                InstallAction::HomebrewFormula("node")
            } else {
                InstallAction::Winget("OpenJS.NodeJS.LTS")
            }
        }
        "git" => {
            if cfg!(target_os = "macos") {
                InstallAction::HomebrewFormula("git")
            } else {
                InstallAction::Winget("Git.Git")
            }
        }
        "pnpm" => InstallAction::NpmGlobal("pnpm"),
        "bun" => {
            if cfg!(target_os = "macos") {
                InstallAction::HomebrewFormula("bun")
            } else {
                InstallAction::Winget("Oven-sh.Bun")
            }
        }
        "npm" => InstallAction::ProvidedBy("Node.js LTS"),
        _ => InstallAction::CustomUnsupported("暂无内置安装器。"),
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
    let mut warnings = Vec::new();
    let mut blocker = None;
    let mut can_install = !already_installed;

    if already_installed {
        blocker = Some(format!("{} 已安装，无需重复安装。", definition.tool.name));
        can_install = false;
    }

    match &definition.action {
        InstallAction::NpmGlobal(package) => {
            steps.push(ToolInstallStep {
                label: "检查 npm".to_string(),
                detail: "需要本机 npm 可用；npm 通常随 Node.js LTS 一起安装。".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "安装全局包".to_string(),
                detail: format!("执行 npm install -g {package}。"),
            });
            steps.push(ToolInstallStep {
                label: "复检命令".to_string(),
                detail: format!(
                    "安装后运行 {} --version 并刷新仪表盘。",
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
                    .unwrap_or_else(|| "安装 Node.js LTS".to_string());
                prerequisites.push(ToolInstallPrerequisite {
                    tool_id: "node".to_string(),
                    tool_name: "Node.js LTS".to_string(),
                    manager: node_manager.to_string(),
                    command: node_command,
                    installed: node_installed,
                    can_install: node_can_install,
                    reason: "目标工具需要 npm；npm 随 Node.js LTS 提供。".to_string(),
                });
                steps.insert(
                    0,
                    ToolInstallStep {
                        label: "安装前置依赖".to_string(),
                        detail: format!(
                            "检测到 npm 不可用；允许后会先通过 {} 安装 Node.js LTS。",
                            node_manager
                        )
                        .to_string(),
                    },
                );
                warnings.push(
                    "此计划包含前置依赖安装：Node.js LTS 会先安装，随后再安装目标 npm 包。"
                        .to_string(),
                );
                if !node_can_install {
                    blocker = Some(
                        "npm 不可用，且当前平台的 Node.js 自动安装器不可用；无法自动安装前置依赖。"
                            .to_string(),
                    );
                    can_install = false;
                }
            }
        }
        InstallAction::Winget(package_id) => {
            steps.push(ToolInstallStep {
                label: "检查 winget".to_string(),
                detail: "需要 Windows App Installer / winget 可用。".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "安装软件包".to_string(),
                detail: format!("通过 winget 安装 {package_id}。"),
            });
            steps.push(ToolInstallStep {
                label: "复检命令".to_string(),
                detail: format!(
                    "安装后运行 {} --version 并刷新仪表盘。",
                    definition.tool.command
                ),
            });
            if !cfg!(target_os = "windows") {
                blocker = Some("winget 安装器仅支持 Windows。".to_string());
                can_install = false;
            } else if !command_available("winget") {
                blocker = Some("winget 不可用。请先安装或修复 Windows App Installer。".to_string());
                can_install = false;
            }
        }
        InstallAction::HomebrewFormula(formula) => {
            steps.push(ToolInstallStep {
                label: "检查 Homebrew".to_string(),
                detail: "需要本机 brew 命令可用。".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "安装公式".to_string(),
                detail: format!("通过 Homebrew 安装 {formula}。"),
            });
            steps.push(ToolInstallStep {
                label: "复检命令".to_string(),
                detail: format!(
                    "安装后运行 {} --version 并刷新仪表盘。",
                    definition.tool.command
                ),
            });
            if !cfg!(target_os = "macos") {
                blocker = Some("Homebrew 安装器当前仅在 macOS 上启用。".to_string());
                can_install = false;
            } else if !command_available("brew") {
                blocker = Some("brew 不可用。请先安装 Homebrew，或使用手动安装。".to_string());
                can_install = false;
            }
        }
        InstallAction::HomebrewCask(cask) => {
            steps.push(ToolInstallStep {
                label: "检查 Homebrew".to_string(),
                detail: "需要本机 brew 命令可用。".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "安装应用".to_string(),
                detail: format!("通过 Homebrew Cask 安装 {cask}。"),
            });
            steps.push(ToolInstallStep {
                label: "复检应用".to_string(),
                detail: format!("安装后检测 {} 是否可用。", definition.tool.name),
            });
            if !cfg!(target_os = "macos") {
                blocker = Some("Homebrew Cask 安装器当前仅在 macOS 上启用。".to_string());
                can_install = false;
            } else if !command_available("brew") {
                blocker = Some("brew 不可用。请先安装 Homebrew，或使用手动安装。".to_string());
                can_install = false;
            }
        }
        InstallAction::PowerShellScript(label, script) => {
            steps.push(ToolInstallStep {
                label: "检查 PowerShell".to_string(),
                detail: "需要本机 PowerShell 可用。".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "运行官方安装脚本".to_string(),
                detail: format!("执行 {label}：{script}。"),
            });
            steps.push(ToolInstallStep {
                label: "复检命令".to_string(),
                detail: format!(
                    "安装后运行 {} --version 并刷新仪表盘。",
                    definition.tool.command
                ),
            });
            if !cfg!(target_os = "windows") {
                blocker = Some("此 PowerShell 安装脚本目前仅在 Windows 上启用。".to_string());
                can_install = false;
            } else if !powershell_available() {
                blocker = Some("PowerShell 不可用，无法运行官方安装脚本。".to_string());
                can_install = false;
            }
        }
        InstallAction::VsCodeExtension(extension_id) => {
            steps.push(ToolInstallStep {
                label: "检查 VS Code CLI".to_string(),
                detail: "需要本机 code 命令可用。".to_string(),
            });
            steps.push(ToolInstallStep {
                label: "安装 VS Code 扩展".to_string(),
                detail: format!("执行 code --install-extension {extension_id}。"),
            });
            steps.push(ToolInstallStep {
                label: "复检扩展".to_string(),
                detail: "安装后运行 code --list-extensions --show-versions 并刷新仪表盘。"
                    .to_string(),
            });
            if !command_available("code") {
                blocker = Some(
                    "VS Code CLI 不可用。请先安装 VS Code，或在 VS Code 中启用 code 命令。"
                        .to_string(),
                );
                can_install = false;
            }
        }
        InstallAction::ProvidedBy(provider) => {
            blocker = Some(format!(
                "{} 由 {provider} 提供，请安装 {provider}。",
                definition.tool.name
            ));
            can_install = false;
            steps.push(ToolInstallStep {
                label: "安装上游依赖".to_string(),
                detail: format!("{} 没有独立安装包。", definition.tool.name),
            });
        }
        InstallAction::CustomUnsupported(reason) => {
            blocker = Some(reason.to_string());
            can_install = false;
        }
    }

    if matches!(definition.action, InstallAction::Winget(_)) {
        warnings.push(
            "部分 winget 包可能触发系统安装权限提示；CodeStudio Lite 不会绕过系统确认。"
                .to_string(),
        );
    }
    if matches!(
        definition.action,
        InstallAction::HomebrewFormula(_) | InstallAction::HomebrewCask(_)
    ) {
        warnings.push(
            "Homebrew 安装会写入当前用户的 Homebrew 前缀，完成后可能需要重新打开终端。".to_string(),
        );
    }
    if matches!(definition.action, InstallAction::NpmGlobal(_)) {
        warnings.push(
            "全局 npm 安装会写入当前用户或当前 npm 前缀目录，完成后可能需要重新打开终端。"
                .to_string(),
        );
    }
    if matches!(definition.action, InstallAction::PowerShellScript(_, _)) {
        warnings.push(
            "此计划会运行目标工具官方发布的 PowerShell 安装脚本；请只在信任该工具来源时确认。"
                .to_string(),
        );
    }
    if matches!(definition.action, InstallAction::VsCodeExtension(_)) {
        warnings.push(
            "VS Code 扩展会安装到当前用户的 VS Code 配置中，完成后可能需要重启 VS Code。"
                .to_string(),
        );
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
    let requires_admin = matches!(definition.action, InstallAction::Winget(_))
        || prerequisites
            .iter()
            .any(|prerequisite| !prerequisite.installed && prerequisite.manager == "winget");

    ToolInstallPlan {
        tool_id: definition.tool.id.to_string(),
        tool_name: definition.tool.name.to_string(),
        manager,
        command: commands
            .iter()
            .map(|command| command.command.clone())
            .collect::<Vec<_>>()
            .join(" && "),
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

fn command_entry(definition: &InstallDefinition, stage: &str) -> ToolInstallCommand {
    ToolInstallCommand {
        tool_id: definition.tool.id.to_string(),
        tool_name: definition.tool.name.to_string(),
        stage: stage.to_string(),
        manager: manager_label(&definition.action).to_string(),
        command: command_preview(&definition.action),
        requires_admin: matches!(definition.action, InstallAction::Winget(_)),
    }
}

fn dependency_satisfied(action: &InstallAction) -> bool {
    match action {
        InstallAction::NpmGlobal(_) => command_available("npm"),
        InstallAction::HomebrewFormula(_) | InstallAction::HomebrewCask(_) => {
            command_available("brew")
        }
        InstallAction::PowerShellScript(_, _) => powershell_available(),
        InstallAction::VsCodeExtension(_) => command_available("code"),
        _ => true,
    }
}

fn run_install_action(action: &InstallAction) -> Result<InstallCommandOutput, String> {
    match action {
        InstallAction::NpmGlobal(package) => run_action_command("npm", &["install", "-g", package]),
        InstallAction::Winget(package_id) => run_action_command(
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
        ),
        InstallAction::HomebrewFormula(formula) => {
            run_action_command("brew", &["install", formula])
        }
        InstallAction::HomebrewCask(cask) => {
            run_action_command("brew", &["install", "--cask", cask])
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
        ),
        InstallAction::VsCodeExtension(extension_id) => {
            run_action_command("code", &["--install-extension", extension_id])
        }
        InstallAction::ProvidedBy(_) | InstallAction::CustomUnsupported(_) => {
            Err("此工具没有可执行的独立安装动作。".to_string())
        }
    }
}

fn run_update_action(action: &InstallAction) -> Result<InstallCommandOutput, String> {
    match action {
        InstallAction::NpmGlobal(package) => {
            let package = format!("{package}@latest");
            run_action_command_owned(
                "npm",
                vec!["install".to_string(), "-g".to_string(), package],
            )
        }
        InstallAction::Winget(package_id) => run_action_command(
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
        ),
        InstallAction::HomebrewFormula(formula) => {
            run_action_command("brew", &["upgrade", formula])
        }
        InstallAction::HomebrewCask(cask) => {
            run_action_command("brew", &["upgrade", "--cask", cask])
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
        ),
        InstallAction::VsCodeExtension(extension_id) => {
            run_action_command("code", &["--install-extension", extension_id, "--force"])
        }
        InstallAction::ProvidedBy(_) | InstallAction::CustomUnsupported(_) => {
            Err("此工具没有可执行的独立更新动作。".to_string())
        }
    }
}

fn run_update_action_for_tool(
    tool_id: &str,
    action: &InstallAction,
) -> Result<InstallCommandOutput, String> {
    if tool_id == "npm" {
        return run_update_action(&InstallAction::NpmGlobal("npm"));
    }
    run_update_action(action)
}

fn current_status(tool_id: &str) -> Result<ToolStatus, String> {
    let snapshot = detector::detect_environment()?;
    snapshot
        .tools
        .into_iter()
        .chain(snapshot.system)
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| format!("没有找到工具 '{tool_id}' 的检测状态。"))
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

fn run_action_command(program: &str, args: &[&str]) -> Result<InstallCommandOutput, String> {
    let Some(resolved) = resolve_command(program) else {
        return Ok(missing_command_output(program));
    };
    match hidden_command_with_args(&resolved, args).output() {
        Ok(output) => Ok(output_to_install_command_output(output, None)),
        Err(err) => Ok(start_failed_output(program, err)),
    }
}

fn run_action_command_owned(
    program: &str,
    args: Vec<String>,
) -> Result<InstallCommandOutput, String> {
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_action_command(program, &args)
}

fn output_to_install_command_output(
    output: std::process::Output,
    missing_command: Option<String>,
) -> InstallCommandOutput {
    let stdout = decode(&output.stdout);
    let stderr = decode(&output.stderr);
    InstallCommandOutput {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout_tail: tail(&stdout),
        stderr_tail: tail(&stderr),
        missing_command,
    }
}

fn missing_command_output(command: &str) -> InstallCommandOutput {
    InstallCommandOutput {
        success: false,
        exit_code: None,
        stdout_tail: String::new(),
        stderr_tail: format!("命令不可用：{command}。它可能已被移动或卸载。"),
        missing_command: Some(command.to_string()),
    }
}

fn start_failed_output(command: &str, err: std::io::Error) -> InstallCommandOutput {
    InstallCommandOutput {
        success: false,
        exit_code: None,
        stdout_tail: String::new(),
        stderr_tail: format!("启动命令失败：{err}"),
        missing_command: Some(command.to_string()),
    }
}

fn refresh_process_environment_after_install(notes: &mut Vec<String>) {
    match refresh_process_path_from_registry() {
        Ok(true) => push_note_once(notes, "已刷新当前应用进程 PATH，后续检测无需重启应用。"),
        Ok(false) => {}
        Err(err) => push_note_once(notes, &format!("当前应用进程 PATH 刷新失败：{err}")),
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
    .map_err(|err| format!("启动 PowerShell 刷新 PATH 失败：{err}"))?;
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
        InstallAction::VsCodeExtension(extension_id) => {
            format!("code --install-extension {extension_id}")
        }
        InstallAction::ProvidedBy(provider) => format!("由 {provider} 提供"),
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
        InstallAction::VsCodeExtension(extension_id) => {
            format!("code --install-extension {extension_id} --force")
        }
        InstallAction::ProvidedBy(provider) => format!("由 {provider} 提供"),
        InstallAction::CustomUnsupported(reason) => reason.to_string(),
    }
}

fn update_supported_for_tool(tool_id: &str, action: &InstallAction) -> bool {
    tool_id == "npm" || update_supported(action)
}

fn update_command_preview_for_tool(tool_id: &str, action: &InstallAction) -> String {
    if tool_id == "npm" {
        return "npm install -g npm@latest".to_string();
    }
    update_command_preview(action)
}

fn close_processes_before_update(
    tool_id: &str,
    tool_name: &str,
) -> Result<process_control::ProcessTerminationReport, String> {
    let targets = update_process_targets(tool_id);
    if targets.process_names.is_empty() && targets.command_line_markers.is_empty() {
        return Ok(process_control::ProcessTerminationReport::default());
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
            process_names: vec!["codex"],
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
            process_names: vec!["claude"],
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
}
