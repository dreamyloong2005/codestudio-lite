use crate::core::claude_desktop_patch;
use crate::core::credentials;
use crate::core::platform::resolve_command;
use crate::core::profile;
use crate::core::tool_registry::{ai_tools, system_tools};
use crate::core::types::{
    ProfileDraft, ProviderApplyMode, ToolLaunchPlan, ToolLaunchProfileOption, ToolLaunchShellOption,
};
use std::collections::HashMap;

pub fn plan_tool_launch(tool_id: &str) -> Result<ToolLaunchPlan, String> {
    let canonical_tool_id = canonical_profile_app(tool_id);
    let tools = ai_tools()
        .into_iter()
        .chain(system_tools())
        .collect::<Vec<_>>();
    let tool = tools
        .iter()
        .cloned()
        .find(|tool| tool.id == tool_id)
        .or_else(|| {
            tools
                .iter()
                .cloned()
                .find(|tool| canonical_profile_app(tool.id) == canonical_tool_id)
        })
        .ok_or_else(|| format!("Tool '{tool_id}' is not supported for launch."))?;
    let command = launch_command_for_tool(tool.id, tool.command);
    let shells = detect_shells();
    let profiles = profile::load_profile_summary()?
        .drafts
        .into_iter()
        .filter(|draft| canonical_profile_app(&draft.app) == canonical_tool_id)
        .filter(|draft| draft.mode == ProviderApplyMode::Config)
        .map(|draft| ToolLaunchProfileOption {
            id: draft.id,
            name: draft.name,
            mode: draft.mode,
            provider: draft.provider,
            base_url: draft.base_url,
            is_builtin: draft.is_builtin,
        })
        .collect::<Vec<_>>();
    let can_launch = resolve_command_from_launch_command(&command).is_some();
    Ok(ToolLaunchPlan {
        tool_id: canonical_tool_id,
        tool_name: tool.name.to_string(),
        command,
        can_launch,
        blocker: if can_launch {
            None
        } else {
            Some(format!("Tool '{}' cannot be found.", tool.name))
        },
        shells,
        profiles,
    })
}

pub fn launch_environment_for_profile(
    profile_id: Option<&str>,
) -> Result<Vec<(String, String)>, String> {
    let Some(profile_id) = profile_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    let profile = profile::load_profile_by_id(profile_id)?;
    if !profile_uses_temporary_environment(&profile) {
        return Ok(Vec::new());
    }
    profile_env(&profile)
}

fn profile_uses_temporary_environment(profile: &ProfileDraft) -> bool {
    !provider_is_official(&profile.provider)
}

pub fn detect_shells() -> Vec<ToolLaunchShellOption> {
    shell_candidates()
        .into_iter()
        .map(|candidate| {
            let command =
                resolve_command(candidate.command).unwrap_or_else(|| candidate.command.to_string());
            ToolLaunchShellOption {
                id: candidate.id.to_string(),
                label: candidate.label.to_string(),
                command,
                available: candidate.available(),
                default: candidate.default,
            }
        })
        .collect()
}

fn profile_env(profile: &ProfileDraft) -> Result<Vec<(String, String)>, String> {
    let app = canonical_profile_app(&profile.app);
    let secret = profile
        .auth_ref
        .as_deref()
        .map(credentials::load_keychain_secret)
        .transpose()?
        .unwrap_or_default();
    let mut env = Vec::new();
    match app.as_str() {
        "codex" => {
            push_non_empty(&mut env, "OPENAI_API_KEY", &secret);
            push_non_empty(&mut env, "OPENAI_BASE_URL", &profile.base_url);
            push_non_empty(&mut env, "OPENAI_MODEL", &profile.model);
        }
        "claude" | "claude-desktop" => {
            push_non_empty(&mut env, "ANTHROPIC_API_KEY", &secret);
            push_non_empty(&mut env, "ANTHROPIC_BASE_URL", &profile.base_url);
            push_non_empty(&mut env, "ANTHROPIC_MODEL", &profile.model);
        }
        "gemini" | "gemini-code-assist" => {
            push_non_empty(&mut env, "GEMINI_API_KEY", &secret);
            push_non_empty(&mut env, "GOOGLE_GEMINI_BASE_URL", &profile.base_url);
            push_non_empty(&mut env, "GEMINI_MODEL", &profile.model);
        }
        "opencode" | "openclaw" | "hermes" => {
            push_non_empty(&mut env, "OPENAI_API_KEY", &secret);
            push_non_empty(&mut env, "OPENAI_BASE_URL", &profile.base_url);
            push_non_empty(&mut env, "OPENAI_MODEL", &profile.model);
        }
        _ => {}
    }
    push_non_empty(&mut env, "CODESTUDIO_PROFILE_ID", &profile.id);
    push_non_empty(&mut env, "CODESTUDIO_PROFILE_NAME", &profile.name);
    push_non_empty(&mut env, "CODESTUDIO_PROFILE_PROVIDER", &profile.provider);
    push_non_empty(&mut env, "CODESTUDIO_PROFILE_PROTOCOL", &profile.protocol);
    Ok(env)
}

fn push_non_empty(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        env.push((key.to_string(), value.to_string()));
    }
}

fn launch_command_for_tool(tool_id: &str, command: &str) -> String {
    match tool_id {
        "codex-vscode" | "claude-vscode" | "gemini-code-assist" => "code".to_string(),
        "claude-desktop" if cfg!(target_os = "macos") => "open -a Claude".to_string(),
        "claude-desktop" => claude_desktop_patch::base_launch_command(tool_id, "Claude"),
        _ => command.to_string(),
    }
}

fn resolve_command_from_launch_command(command: &str) -> Option<String> {
    if command.contains("launch-claude.ps1") || command.contains("launch-claude-zh.ps1") {
        return resolve_command("powershell");
    }
    let first = command.split_whitespace().next().unwrap_or(command);
    resolve_command(first)
}

fn provider_is_official(provider: &str) -> bool {
    provider.trim().eq_ignore_ascii_case("official")
}

fn canonical_profile_app(app: &str) -> String {
    match app.trim().to_ascii_lowercase().as_str() {
        "codex" | "codex-cli" | "codex-app" | "codex-client" | "codex-desktop" | "codex-vscode"
        | "codex-code-vscode" | "codex-vs-code" => "codex".to_string(),
        "claude-desktop" | "claude-app" | "claude-client" => "claude-desktop".to_string(),
        "claude-vscode" | "claude-code-vscode" | "claude-vs-code" => "claude".to_string(),
        "gemini-code-assist" | "gemini-vscode" | "gemini-code-vscode" | "gemini-vs-code" => {
            "gemini-code-assist".to_string()
        }
        "hermes" | "hermes-agent" => "hermes".to_string(),
        other => other.to_string(),
    }
}

#[derive(Clone, Copy)]
struct ShellCandidate {
    id: &'static str,
    label: &'static str,
    command: &'static str,
    default: bool,
}

impl ShellCandidate {
    fn available(self) -> bool {
        resolve_command(self.command).is_some()
    }
}

fn shell_candidates() -> Vec<ShellCandidate> {
    if cfg!(target_os = "windows") {
        vec![
            ShellCandidate {
                id: "cmd",
                label: "Command Prompt",
                command: "cmd",
                default: true,
            },
            ShellCandidate {
                id: "powershell",
                label: "Windows PowerShell 5",
                command: "powershell",
                default: false,
            },
            ShellCandidate {
                id: "pwsh",
                label: "PowerShell 7",
                command: "pwsh",
                default: false,
            },
        ]
    } else {
        vec![
            ShellCandidate {
                id: "sh",
                label: "sh",
                command: "sh",
                default: true,
            },
            ShellCandidate {
                id: "bash",
                label: "bash",
                command: "bash",
                default: false,
            },
            ShellCandidate {
                id: "zsh",
                label: "zsh",
                command: "zsh",
                default: false,
            },
        ]
    }
}

pub fn shell_command_builder(
    shell_id: Option<&str>,
    command: &str,
    keep_open: bool,
) -> portable_pty::CommandBuilder {
    let shell = shell_by_id(shell_id).unwrap_or_else(default_shell);
    let resolved = resolve_command(shell.command).unwrap_or_else(|| shell.command.to_string());
    let mut builder = portable_pty::CommandBuilder::new(resolved);
    builder.args(shell_arguments(shell.id, command, keep_open));
    builder
}

fn shell_arguments(shell_id: &str, command: &str, keep_open: bool) -> Vec<String> {
    if cfg!(target_os = "windows") {
        match shell_id {
            "powershell" | "pwsh" if keep_open => vec![
                "-NoLogo".to_string(),
                "-NoExit".to_string(),
                "-Command".to_string(),
                command.to_string(),
            ],
            "powershell" | "pwsh" => vec![
                "-NoLogo".to_string(),
                "-Command".to_string(),
                command.to_string(),
            ],
            _ if keep_open => vec!["/S".to_string(), "/K".to_string(), command.to_string()],
            _ => vec!["/S".to_string(), "/C".to_string(), command.to_string()],
        }
    } else if keep_open {
        vec![
            "-lc".to_string(),
            format!("{command}; printf '\\n'; exec $SHELL"),
        ]
    } else {
        vec!["-lc".to_string(), command.to_string()]
    }
}

fn shell_by_id(shell_id: Option<&str>) -> Option<ShellCandidate> {
    let id = shell_id?.trim();
    shell_candidates()
        .into_iter()
        .find(|candidate| candidate.id == id && candidate.available())
}

fn default_shell() -> ShellCandidate {
    shell_candidates()
        .into_iter()
        .find(|candidate| candidate.default && candidate.available())
        .or_else(|| {
            shell_candidates()
                .into_iter()
                .find(|candidate| candidate.available())
        })
        .unwrap_or(ShellCandidate {
            id: "system",
            label: "System Shell",
            command: if cfg!(target_os = "windows") {
                "cmd"
            } else {
                "sh"
            },
            default: true,
        })
}

pub fn env_map(values: Vec<(String, String)>) -> HashMap<String, String> {
    values.into_iter().collect()
}

/// Spawn `command` in a brand-new, visible console window that is fully
/// detached from this process. Used by the "external" launch mode so the
/// CLI runs in an independent terminal the user can move, resize and close
/// freely, without the app owning a PTY or capturing its output. On Windows
/// a fresh console is allocated for the child; on macOS Terminal.app is
/// told to run a generated launcher script; on Linux a best-effort terminal
/// emulator is invoked.
pub fn spawn_external_terminal(
    shell_id: Option<&str>,
    command: &str,
    env: &[(String, String)],
    working_directory: Option<&std::path::Path>,
) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let shell = shell_by_id(shell_id).unwrap_or_else(default_shell);
        let resolved = resolve_command(shell.command).unwrap_or_else(|| shell.command.to_string());
        let mut cmd = std::process::Command::new(&resolved);
        cmd.args(shell_arguments(shell.id, command, true));
        for (key, value) in env {
            cmd.env(key, value);
        }
        if let Some(directory) = working_directory {
            cmd.current_dir(directory);
        }
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        cmd.creation_flags(CREATE_NEW_CONSOLE);
        cmd.spawn()?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = shell_id;
        let mut script = String::from("#!/bin/sh\n\n");
        if let Some(directory) = working_directory {
            script.push_str("cd ");
            script.push_str(&sh_single_quote(&directory.to_string_lossy()));
            script.push('\n');
        }
        for (key, value) in env {
            script.push_str("export ");
            script.push_str(key);
            script.push('=');
            script.push_str(&sh_single_quote(value));
            script.push('\n');
        }
        script.push_str(command);
        script.push_str(
            "
exec \"${SHELL:-/bin/sh}\"
",
        );
        let dir = std::env::temp_dir();
        let script_path = dir.join(format!("csl-external-{}.sh", std::process::id()));
        write_external_launcher_script(&script_path, &script)?;
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .args(["-a", "Terminal", &script_path.to_string_lossy()])
                .spawn()?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            let opened = ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"]
                .iter()
                .find_map(|emulator| {
                    std::process::Command::new(emulator)
                        .arg(&script_path)
                        .spawn()
                        .ok()
                });
            if opened.is_none() {
                std::process::Command::new("sh").arg(&script_path).spawn()?;
            }
        }
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn write_external_launcher_script(
    script_path: &std::path::Path,
    script: &str,
) -> std::io::Result<()> {
    std::fs::write(script_path, script)?;
    set_external_launcher_permissions(script_path)
}

#[cfg(unix)]
fn set_external_launcher_permissions(script_path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(script_path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(all(not(unix), not(target_os = "windows")))]
fn set_external_launcher_permissions(_script_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROTOCOL_OPENAI_RESPONSES: &str = "openai-responses";
    const PROTOCOL_ANTHROPIC_MESSAGES: &str = "anthropic-messages";
    const PROTOCOL_GOOGLE_GEMINI: &str = "google-gemini";

    fn test_profile(app: &str, protocol: &str) -> ProfileDraft {
        ProfileDraft {
            id: "profile-test".to_string(),
            name: "Example".to_string(),
            icon: None,
            remark: None,
            app: app.to_string(),
            is_builtin: false,
            mode: ProviderApplyMode::Config,
            provider: "apikey.fun".to_string(),
            protocol: protocol.to_string(),
            model: "model-a".to_string(),
            model_mappings: Vec::new(),
            base_url: "https://api.example.test/v1".to_string(),
            auth_ref: None,
            created_at: None,
            updated_at: None,
            last_test_status: None,
            usage_enabled: false,
            sort_order: 0,
        }
    }

    #[test]
    fn profile_env_maps_claude_to_anthropic_variables() {
        let env =
            env_map(profile_env(&test_profile("claude", PROTOCOL_ANTHROPIC_MESSAGES)).unwrap());

        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://api.example.test/v1")
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("model-a")
        );
        assert_eq!(
            env.get("CODESTUDIO_PROFILE_PROVIDER").map(String::as_str),
            Some("apikey.fun")
        );
    }

    #[test]
    fn profile_env_maps_gemini_to_gemini_variables() {
        let env = env_map(profile_env(&test_profile("gemini", PROTOCOL_GOOGLE_GEMINI)).unwrap());

        assert_eq!(
            env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str),
            Some("https://api.example.test/v1")
        );
        assert_eq!(env.get("GEMINI_MODEL").map(String::as_str), Some("model-a"));
    }

    #[test]
    fn official_profile_does_not_create_temporary_api_environment() {
        let mut profile = test_profile("codex", PROTOCOL_OPENAI_RESPONSES);
        profile.provider = "official".to_string();

        assert!(!profile_uses_temporary_environment(&profile));
    }

    #[test]
    fn launch_command_uses_vscode_host_for_plugin_tools() {
        assert_eq!(launch_command_for_tool("claude-vscode", "code"), "code");
        assert_eq!(
            launch_command_for_tool("gemini-code-assist", "code"),
            "code"
        );
    }

    #[test]
    fn launch_plan_prefers_the_requested_plugin_tool_over_shared_profile_family() {
        let plan = plan_tool_launch("codex-vscode").unwrap();

        assert_eq!(plan.tool_name, "Codex VS Code");
        assert_eq!(plan.command, "code");
    }

    #[test]
    fn shell_arguments_keep_install_commands_short_lived_and_launch_commands_open() {
        if cfg!(target_os = "windows") {
            assert_eq!(
                shell_arguments("cmd", "codex", false),
                vec!["/S".to_string(), "/C".to_string(), "codex".to_string()]
            );
            assert_eq!(
                shell_arguments("cmd", "codex", true),
                vec!["/S".to_string(), "/K".to_string(), "codex".to_string()]
            );
        } else {
            assert_eq!(
                shell_arguments("zsh", "codex", false),
                vec!["-lc".to_string(), "codex".to_string()]
            );
            assert_eq!(
                shell_arguments("zsh", "codex", true),
                vec![
                    "-lc".to_string(),
                    "codex; printf '\\n'; exec $SHELL".to_string()
                ]
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_single_quote_escapes_embedded_quotes() {
        assert_eq!(sh_single_quote("a'b"), "'a'\\''b'");
    }

    #[cfg(unix)]
    #[test]
    fn external_launcher_script_is_owner_executable() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!(
            "csl-external-permission-test-{}.sh",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        write_external_launcher_script(&path, "#!/bin/sh\nexit 0\n").expect("write launcher");
        let mode = std::fs::metadata(&path)
            .expect("launcher metadata")
            .permissions()
            .mode()
            & 0o777;
        let _ = std::fs::remove_file(&path);

        assert_eq!(mode, 0o700);
    }
}
