use crate::core::types::ToolCategory;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: ToolCategory,
    pub command: &'static str,
    pub version_args: &'static [&'static str],
    pub version_output_contains: Option<&'static str>,
    pub config_relative_path: Option<&'static str>,
    pub install_command: Option<&'static str>,
}

pub fn ai_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            id: "codex",
            name: "Codex CLI",
            category: ToolCategory::AiTool,
            command: "codex",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: Some(".codex/config.toml"),
            install_command: Some("npm install -g @openai/codex"),
        },
        ToolDefinition {
            id: "codex-vscode",
            name: "Codex VS Code",
            category: ToolCategory::AiTool,
            command: "code",
            version_args: &["--list-extensions", "--show-versions"],
            version_output_contains: Some("openai.chatgpt"),
            config_relative_path: Some(".codex/config.toml"),
            install_command: Some("code --install-extension openai.chatgpt"),
        },
        ToolDefinition {
            id: "claude-desktop",
            name: "Claude Desktop",
            category: ToolCategory::AiTool,
            command: "Claude",
            version_args: &[],
            version_output_contains: None,
            config_relative_path: claude_desktop_config_relative_path(),
            install_command: claude_desktop_install_command(),
        },
        ToolDefinition {
            id: "claude",
            name: "Claude Code",
            category: ToolCategory::AiTool,
            command: "claude",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: Some(".claude"),
            install_command: Some("npm install -g @anthropic-ai/claude-code"),
        },
        ToolDefinition {
            id: "claude-vscode",
            name: "Claude VS Code",
            category: ToolCategory::AiTool,
            command: "code",
            version_args: &["--list-extensions", "--show-versions"],
            version_output_contains: Some("anthropic.claude-code"),
            config_relative_path: Some(".claude/config.json"),
            install_command: Some("code --install-extension anthropic.claude-code"),
        },
        ToolDefinition {
            id: "gemini",
            name: "Gemini CLI",
            category: ToolCategory::AiTool,
            command: "gemini",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: Some(".gemini"),
            install_command: Some("npm install -g @google/gemini-cli"),
        },
        ToolDefinition {
            id: "gemini-code-assist",
            name: "Gemini Code Assist",
            category: ToolCategory::AiTool,
            command: "code",
            version_args: &["--list-extensions", "--show-versions"],
            version_output_contains: Some("google.geminicodeassist"),
            config_relative_path: vscode_user_settings_relative_path(),
            install_command: Some("code --install-extension Google.geminicodeassist"),
        },
        ToolDefinition {
            id: "opencode",
            name: "OpenCode",
            category: ToolCategory::AiTool,
            command: "opencode",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: Some(".config/opencode"),
            install_command: Some("npm install -g opencode-ai"),
        },
        ToolDefinition {
            id: "openclaw",
            name: "OpenClaw",
            category: ToolCategory::AiTool,
            command: "openclaw",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: Some(".openclaw"),
            install_command: Some("npm install -g openclaw"),
        },
        ToolDefinition {
            id: "hermes",
            name: "Hermes",
            category: ToolCategory::AiTool,
            command: "hermes",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: Some(".hermes/config.yaml"),
            install_command: hermes_install_command(),
        },
    ]
}

pub fn system_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            id: "node",
            name: "Node.js",
            category: ToolCategory::System,
            command: "node",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: None,
            install_command: node_install_command(),
        },
        ToolDefinition {
            id: "git",
            name: "Git",
            category: ToolCategory::System,
            command: "git",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: None,
            install_command: git_install_command(),
        },
        ToolDefinition {
            id: "npm",
            name: "npm",
            category: ToolCategory::System,
            command: "npm",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: None,
            install_command: None,
        },
        ToolDefinition {
            id: "pnpm",
            name: "pnpm",
            category: ToolCategory::System,
            command: "pnpm",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: None,
            install_command: Some("npm install -g pnpm"),
        },
        ToolDefinition {
            id: "bun",
            name: "Bun",
            category: ToolCategory::System,
            command: "bun",
            version_args: &["--version"],
            version_output_contains: None,
            config_relative_path: None,
            install_command: bun_install_command(),
        },
    ]
}

fn claude_desktop_config_relative_path() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("AppData/Local/Claude")
    } else if cfg!(target_os = "macos") {
        Some("Library/Application Support/Claude")
    } else {
        None
    }
}

fn vscode_user_settings_relative_path() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("AppData/Roaming/Code/User/settings.json")
    } else if cfg!(target_os = "macos") {
        Some("Library/Application Support/Code/User/settings.json")
    } else {
        Some(".config/Code/User/settings.json")
    }
}

fn claude_desktop_install_command() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("winget install --id Anthropic.Claude --exact")
    } else if cfg!(target_os = "macos") {
        Some("brew install --cask claude")
    } else {
        None
    }
}

fn hermes_install_command() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm https://hermes-agent.nousresearch.com/install.ps1)\"",
        )
    } else if cfg!(target_os = "macos") {
        Some("brew install hermes-agent")
    } else {
        Some("curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash")
    }
}

fn node_install_command() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("winget install OpenJS.NodeJS.LTS")
    } else if cfg!(target_os = "macos") {
        Some("brew install node")
    } else {
        Some("curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash - && sudo apt-get install -y nodejs")
    }
}

fn git_install_command() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("winget install Git.Git")
    } else if cfg!(target_os = "macos") {
        Some("brew install git")
    } else {
        Some("sudo apt-get update && sudo apt-get install -y git")
    }
}

fn bun_install_command() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("winget install Oven-sh.Bun")
    } else if cfg!(target_os = "macos") {
        Some("brew install bun")
    } else {
        Some("curl -fsSL https://bun.sh/install | bash")
    }
}
