use crate::core::tool_catalog::{canonical_profile_tool_id, profile_display_name};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) struct GatewayRouteTarget {
    pub(super) original_path: String,
    pub(super) route_path: String,
    pub(super) tool_id: Option<String>,
    pub(super) strict_tool: bool,
}

impl GatewayRouteTarget {
    pub(super) fn resolve(path: &str, headers: &HashMap<String, String>) -> Self {
        let original_path = path.split('?').next().unwrap_or(path).to_string();
        if let Some((raw_tool_id, route_path)) = original_path
            .strip_prefix("/tools/")
            .and_then(|rest| rest.split_once('/'))
        {
            if let Some(tool_id) = canonical_profile_tool_id(raw_tool_id) {
                let route_path = format!("/{route_path}");
                return Self {
                    original_path,
                    route_path,
                    tool_id: Some(tool_id),
                    strict_tool: true,
                };
            }
        }
        let explicit = explicit_tool(headers);
        let tool_id = explicit.clone().or_else(|| inferred_tool(headers));
        Self {
            route_path: original_path.clone(),
            original_path,
            tool_id,
            strict_tool: explicit.is_some(),
        }
    }
}

pub(super) fn detect_client(headers: &HashMap<String, String>, tool_id: Option<&str>) -> String {
    if let Some(tool_id) = tool_id {
        return profile_display_name(tool_id)
            .unwrap_or("Unknown client")
            .to_string();
    }
    let value = client_header(headers);
    if value.contains("codex") {
        "Codex".into()
    } else if value.contains("opencode") {
        "OpenCode".into()
    } else if value.contains("openclaw") {
        "OpenClaw".into()
    } else if value.contains("curl") {
        "curl".into()
    } else if value.is_empty() {
        "Unknown client".into()
    } else {
        value.chars().take(48).collect()
    }
}

fn explicit_tool(headers: &HashMap<String, String>) -> Option<String> {
    headers
        .get("x-codestudio-tool")
        .or_else(|| headers.get("x-codestudio-client-tool"))
        .and_then(|value| canonical_profile_tool_id(value))
        .or_else(|| {
            headers
                .get("x-codestudio-client")
                .and_then(|value| canonical_profile_tool_id(value))
        })
}

fn inferred_tool(headers: &HashMap<String, String>) -> Option<String> {
    let value = client_header(headers);
    let tool = if value.contains("chatgpt-desktop")
        || value.contains("chatgpt desktop")
        || value.contains("codex desktop")
        || value.contains("codex client")
        || value.contains("codex")
    {
        "codex"
    } else if value.contains("claude desktop") {
        "claude-desktop"
    } else if value.contains("claude") {
        "claude"
    } else if value.contains("gemini-code-assist") || value.contains("geminicodeassist") {
        "gemini-code-assist"
    } else if value.contains("gemini") {
        "gemini"
    } else if value.contains("opencode") {
        "opencode"
    } else if value.contains("openclaw") {
        "openclaw"
    } else if value.contains("hermes") {
        "hermes"
    } else if value.contains("grok") {
        "grok"
    } else if value.contains("pi-coding-agent")
        || value.contains("pi agent")
        || value == "pi"
        || value.starts_with("pi/")
    {
        "pi"
    } else {
        return None;
    };
    Some(tool.to_string())
}

fn client_header(headers: &HashMap<String, String>) -> String {
    headers
        .get("x-codestudio-client")
        .or_else(|| headers.get("user-agent"))
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default()
}
