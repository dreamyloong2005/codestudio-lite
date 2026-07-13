pub use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileCapabilities {
    pub id: &'static str,
    pub display_name: &'static str,
    pub official_protocol: &'static str,
    pub config_protocols: &'static [&'static str],
    pub supports_review_model: bool,
    pub supports_model_mappings: bool,
}

pub const OPENAI_CHAT_COMPLETIONS: &str = "openai-chat-completions";
pub const OPENAI_RESPONSES: &str = "openai-responses";
pub const ANTHROPIC_MESSAGES: &str = "anthropic-messages";
pub const GOOGLE_GEMINI: &str = "google-gemini";

const PROFILE_CAPABILITIES: &[ProfileCapabilities] = &[
    ProfileCapabilities {
        id: "codex",
        display_name: "Codex",
        official_protocol: OPENAI_RESPONSES,
        config_protocols: &[OPENAI_CHAT_COMPLETIONS, OPENAI_RESPONSES],
        supports_review_model: true,
        supports_model_mappings: false,
    },
    ProfileCapabilities {
        id: "claude-desktop",
        display_name: "Claude Desktop",
        official_protocol: ANTHROPIC_MESSAGES,
        config_protocols: &[ANTHROPIC_MESSAGES],
        supports_review_model: false,
        supports_model_mappings: false,
    },
    ProfileCapabilities {
        id: "claude",
        display_name: "Claude Code",
        official_protocol: ANTHROPIC_MESSAGES,
        config_protocols: &[ANTHROPIC_MESSAGES],
        supports_review_model: false,
        supports_model_mappings: true,
    },
    ProfileCapabilities {
        id: "gemini",
        display_name: "Gemini CLI",
        official_protocol: GOOGLE_GEMINI,
        config_protocols: &[GOOGLE_GEMINI],
        supports_review_model: false,
        supports_model_mappings: false,
    },
    ProfileCapabilities {
        id: "gemini-code-assist",
        display_name: "Gemini Code Assist",
        official_protocol: GOOGLE_GEMINI,
        config_protocols: &[GOOGLE_GEMINI],
        supports_review_model: false,
        supports_model_mappings: false,
    },
    ProfileCapabilities {
        id: "opencode",
        display_name: "OpenCode",
        official_protocol: OPENAI_CHAT_COMPLETIONS,
        config_protocols: &[OPENAI_CHAT_COMPLETIONS, OPENAI_RESPONSES],
        supports_review_model: false,
        supports_model_mappings: false,
    },
    ProfileCapabilities {
        id: "openclaw",
        display_name: "OpenClaw",
        official_protocol: OPENAI_CHAT_COMPLETIONS,
        config_protocols: &[OPENAI_CHAT_COMPLETIONS],
        supports_review_model: false,
        supports_model_mappings: false,
    },
    ProfileCapabilities {
        id: "hermes",
        display_name: "Hermes",
        official_protocol: OPENAI_CHAT_COMPLETIONS,
        config_protocols: &[OPENAI_CHAT_COMPLETIONS],
        supports_review_model: false,
        supports_model_mappings: false,
    },
    ProfileCapabilities {
        id: "grok",
        display_name: "Grok",
        official_protocol: OPENAI_RESPONSES,
        config_protocols: &[
            OPENAI_CHAT_COMPLETIONS,
            OPENAI_RESPONSES,
            ANTHROPIC_MESSAGES,
        ],
        supports_review_model: false,
        supports_model_mappings: false,
    },
    ProfileCapabilities {
        id: "pi",
        display_name: "Pi Agent",
        official_protocol: ANTHROPIC_MESSAGES,
        config_protocols: &[
            OPENAI_CHAT_COMPLETIONS,
            OPENAI_RESPONSES,
            ANTHROPIC_MESSAGES,
            GOOGLE_GEMINI,
        ],
        supports_review_model: false,
        supports_model_mappings: false,
    },
];

pub fn canonical_tool_id(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "codex" | "codex-cli" | "chatgpt-desktop" | "codex-app" | "codex-client"
        | "codex-desktop" | "codex-vscode" | "codex-code-vscode" | "codex-vs-code" => {
            "codex".to_string()
        }
        "claude-desktop" | "claude-app" | "claude-client" => "claude-desktop".to_string(),
        "claude" | "claude-code" | "claude-vscode" | "claude-code-vscode" | "claude-vs-code" => {
            "claude".to_string()
        }
        "gemini" | "gemini-cli" => "gemini".to_string(),
        "gemini-code-assist" | "gemini-vscode" | "gemini-code-vscode" | "gemini-vs-code" => {
            "gemini-code-assist".to_string()
        }
        "opencode" | "open-code" => "opencode".to_string(),
        "openclaw" | "open-claw" => "openclaw".to_string(),
        "hermes" | "hermes-agent" => "hermes".to_string(),
        "grok" | "grok-cli" | "grok-build" => "grok".to_string(),
        "pi" | "pi-agent" | "pi-coding-agent" => "pi".to_string(),
        other => other.to_string(),
    }
}

pub fn profile_capabilities(value: &str) -> Option<&'static ProfileCapabilities> {
    let canonical = canonical_tool_id(value);
    PROFILE_CAPABILITIES
        .iter()
        .find(|capabilities| capabilities.id == canonical)
}

pub fn canonical_profile_tool_id(value: &str) -> Option<String> {
    profile_capabilities(value).map(|capabilities| capabilities.id.to_string())
}

pub fn profile_display_name(value: &str) -> Option<&'static str> {
    profile_capabilities(value).map(|capabilities| capabilities.display_name)
}

pub fn supports_config_protocol(tool_id: &str, protocol: &str) -> bool {
    profile_capabilities(tool_id)
        .map(|capabilities| {
            capabilities
                .config_protocols
                .iter()
                .any(|candidate| *candidate == protocol)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve_to_one_profile_identity() {
        let cases = [
            ("chatgpt-desktop", "codex"),
            ("codex-vscode", "codex"),
            ("claude-app", "claude-desktop"),
            ("claude-vscode", "claude"),
            ("gemini-cli", "gemini"),
            ("gemini-vscode", "gemini-code-assist"),
            ("open-code", "opencode"),
            ("open-claw", "openclaw"),
            ("hermes-agent", "hermes"),
            ("grok-build", "grok"),
            ("pi-coding-agent", "pi"),
        ];
        for (alias, expected) in cases {
            assert_eq!(canonical_tool_id(alias), expected);
            assert_eq!(canonical_profile_tool_id(alias).as_deref(), Some(expected));
        }
    }

    #[test]
    fn profile_capabilities_hold_the_protocol_matrix() {
        assert!(supports_config_protocol("codex", OPENAI_RESPONSES));
        assert!(!supports_config_protocol("codex", ANTHROPIC_MESSAGES));
        assert!(supports_config_protocol("grok", ANTHROPIC_MESSAGES));
        assert!(supports_config_protocol("pi", GOOGLE_GEMINI));
        assert!(
            profile_capabilities("claude")
                .unwrap()
                .supports_model_mappings
        );
        assert!(profile_capabilities("codex").unwrap().supports_review_model);
    }
}
