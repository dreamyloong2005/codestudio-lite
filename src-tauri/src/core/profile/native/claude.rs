use super::super::*;
use super::NativeProfileAdapter;

pub(in crate::core::profile) static CLAUDE_ADAPTER: ClaudeAdapter = ClaudeAdapter;
pub(in crate::core::profile) struct ClaudeAdapter;

impl NativeProfileAdapter for ClaudeAdapter {
    fn target(&self, paths: &crate::core::app_paths::AppPaths) -> PathBuf {
        paths.home_dir.join(".claude").join("settings.json")
    }
    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                claude_official_config_content(current)
            }
            ProviderApplyMode::Config => claude_config_content(current, profile),
            ProviderApplyMode::Gateway => claude_gateway_config_content(current, profile),
        }
    }
    fn render_preview(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                claude_official_config_content(current)
            }
            ProviderApplyMode::Config => {
                claude_config_content_with_api_key(current, profile, secret_preview(profile))
            }
            ProviderApplyMode::Gateway => claude_gateway_config_content(current, profile),
        }
    }
    fn cleanup_gateway(&self, current: &str) -> Result<String, String> {
        claude_gateway_cleanup_config_content(current, "claude")
    }
    fn inspect(&self, current: &str) -> Result<Option<DetectedNativeProfile>, String> {
        Ok(detect_claude_native_profile(&parse_json5_or_empty(
            current,
            "Claude settings",
        )?))
    }
    fn matches(
        &self,
        current: &str,
        profile: &ProfileDraft,
        secret_match: SecretMatchMode,
    ) -> Result<bool, String> {
        Ok(claude_config_matches_profile_with_secret_match(
            &parse_json5_or_empty(current, "Claude settings")?,
            profile,
            secret_match,
        ))
    }
    fn verify(
        &self,
        path: &Path,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<bool, String> {
        match mode {
            ProviderApplyMode::Config => verify_claude_config(path, profile),
            ProviderApplyMode::Gateway => verify_claude_gateway_config(path, profile),
        }
    }
    fn preview(
        &self,
        profile: &ProfileDraft,
        path: PathBuf,
        display_path: String,
        mode: ProviderApplyMode,
    ) -> Result<NativeConfigPreview, String> {
        let official = provider_is_official(&profile.provider);
        let mut warnings = match (mode, official) {
            (ProviderApplyMode::Config, true) => vec!["Official provider restores Claude Code to its own login.".to_string(), "CodeStudio Lite removes managed API or Gateway fields from Claude settings.".to_string()],
            (ProviderApplyMode::Config, false) => vec!["Config profiles write Claude Code user settings under the env section.".to_string(), "The selected endpoint must be Anthropic/Claude-compatible; generic OpenAI-only endpoints need a translator.".to_string(), "Restart Claude Code or open a new session after applying so settings reload.".to_string()],
            (ProviderApplyMode::Gateway, _) => vec!["Gateway profiles write Claude Code settings to the tool-scoped local gateway URL.".to_string(), "Restart Claude Code or open a new session after applying so settings reload.".to_string(), "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.".to_string(), "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.".to_string()],
        };
        let (json, status) = read_json_preview(&path, "Claude settings", &mut warnings)?;
        let changes = if official {
            vec![
                json_diff_remove_line(
                    &json,
                    &["env", "ANTHROPIC_BASE_URL"],
                    "Restores Claude Code to the client's own official endpoint.",
                ),
                json_diff_remove_line(
                    &json,
                    &["env", "ANTHROPIC_AUTH_TOKEN"],
                    "Removes the CodeStudio Lite managed API token from Claude settings.",
                ),
                json_diff_remove_line(
                    &json,
                    &["model"],
                    "Removes the CodeStudio Lite managed model override.",
                ),
                json_diff_remove_line(
                    &json,
                    &["env", "ANTHROPIC_MODEL"],
                    "Removes the CodeStudio Lite managed model environment override.",
                ),
            ]
        } else if mode == ProviderApplyMode::Gateway {
            let client = gateway::client_config_for_tool("claude")?;
            let model = gateway_config_model_for_profile(profile);
            vec![
                json_diff_line(&json, &["env", "ANTHROPIC_BASE_URL"], &client.base_url, "Points Claude Code at the tool-scoped CodeStudio Lite Local Gateway."),
                json_diff_line(&json, &["env", "ANTHROPIC_AUTH_TOKEN"], &client.token_preview, "Stores only the local CodeStudio token, not the real upstream Provider API key."),
                json_diff_line(&json, &["model"], model, "Sets Claude Code to the virtual model name resolved by the Local Gateway."),
                json_diff_line(&json, &["env", "ANTHROPIC_MODEL"], model, "Keeps the local gateway virtual model available to Claude Code environment consumers."),
            ]
        } else {
            let mut changes = vec![
                json_diff_line(
                    &json,
                    &["env", "ANTHROPIC_BASE_URL"],
                    &profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url),
                    "Points Claude Code at the selected upstream Provider Base URL.",
                ),
                json_diff_line(
                    &json,
                    &["env", "ANTHROPIC_AUTH_TOKEN"],
                    secret_preview(profile),
                    "Stores the selected Provider API key as Claude Code's bearer token.",
                ),
            ];
            if let Some(model) = profile_model(profile) {
                changes.push(json_diff_line(
                    &json,
                    &["model"],
                    model,
                    "Sets Claude Code to the selected upstream model.",
                ));
                changes.push(json_diff_line(
                    &json,
                    &["env", "ANTHROPIC_MODEL"],
                    model,
                    "Keeps the model override available to Claude Code environment consumers.",
                ));
            } else {
                changes.push(json_diff_remove_line(
                    &json,
                    &["model"],
                    "Model is optional; no Claude model override will be written.",
                ));
                changes.push(json_diff_remove_line(
                    &json,
                    &["env", "ANTHROPIC_MODEL"],
                    "Model is optional; no Claude model environment override will be written.",
                ));
            }
            changes
        };
        Ok(NativeConfigPreview {
            tool: "claude".to_string(),
            path: display_path,
            status,
            write_enabled: true,
            changes,
            warnings,
            content: None,
        })
    }
}

pub(in crate::core::profile) fn detect_claude_native_profile(
    value: &serde_json::Value,
) -> Option<DetectedNativeProfile> {
    let base_url = json_string_lookup(value, &["env", "ANTHROPIC_BASE_URL"])?
        .trim()
        .to_string();
    if base_url.is_empty() {
        return None;
    }
    let api_key = json_string_lookup(value, &["env", "ANTHROPIC_AUTH_TOKEN"])?
        .trim()
        .to_string();
    if api_key.is_empty() || looks_like_local_gateway_token(&api_key) {
        return None;
    }
    let model = json_string_lookup(value, &["model"])
        .or_else(|| json_string_lookup(value, &["env", "ANTHROPIC_MODEL"]))
        .and_then(|model| native_optional_model(&model))
        .unwrap_or_default();
    Some(DetectedNativeProfile {
        app: "claude".to_string(),
        provider: provider_slug_from_base_url(&base_url).unwrap_or_else(|| "anthropic".to_string()),
        protocol: PROTOCOL_ANTHROPIC_MESSAGES.to_string(),
        model,
        review_model: None,
        base_url,
        api_key,
    })
}

pub(in crate::core::profile) fn claude_config_matches_profile(
    value: &serde_json::Value,
    profile: &ProfileDraft,
) -> bool {
    claude_config_matches_profile_with_secret_match(value, profile, SecretMatchMode::ExactKeychain)
}

#[cfg(test)]
pub(in crate::core::profile) fn claude_config_matches_profile_without_keychain(
    value: &serde_json::Value,
    profile: &ProfileDraft,
) -> bool {
    claude_config_matches_profile_with_secret_match(
        value,
        profile,
        SecretMatchMode::KeychainReference,
    )
}

pub(in crate::core::profile) fn claude_config_matches_profile_with_secret_match(
    value: &serde_json::Value,
    profile: &ProfileDraft,
    secret_match: SecretMatchMode,
) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "claude"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_ANTHROPIC_MESSAGES)
            && !claude_settings_have_managed_endpoint(value);
    }
    if canonical_profile_app(&profile.app) != "claude"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref() != Ok(PROTOCOL_ANTHROPIC_MESSAGES)
    {
        return false;
    }
    let model_matches = match profile_model(profile) {
        Some(model) => {
            json_string_lookup(value, &["model"]).as_deref() == Some(model)
                || json_string_lookup(value, &["env", "ANTHROPIC_MODEL"]).as_deref() == Some(model)
        }
        None => {
            json_string_lookup(value, &["model"]).is_none()
                && json_string_lookup(value, &["env", "ANTHROPIC_MODEL"]).is_none()
        }
    };
    let token_matches = json_string_lookup(value, &["env", "ANTHROPIC_AUTH_TOKEN"])
        .map(|token| profile_api_key_matches_config(profile, &token, secret_match))
        .unwrap_or(false);
    json_string_lookup(value, &["env", "ANTHROPIC_BASE_URL"])
        .map(|base_url| {
            profile_runtime_base_url_matches(&profile.protocol, &base_url, &profile.base_url)
        })
        .unwrap_or(false)
        && token_matches
        && model_matches
}

fn claude_settings_have_managed_endpoint(value: &serde_json::Value) -> bool {
    json_string_lookup(value, &["env", "ANTHROPIC_BASE_URL"]).is_some()
        || json_string_lookup(value, &["env", "ANTHROPIC_AUTH_TOKEN"]).is_some()
}

fn verify_claude_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let value = parse_json5_or_empty(
        &fs::read_to_string(path).map_err(|err| err.to_string())?,
        "Claude settings",
    )?;
    if provider_is_official(&profile.provider) {
        return Ok(claude_config_matches_profile(&value, profile));
    }
    let model_matches = match profile_model(profile) {
        Some(model) => {
            json_string_lookup(&value, &["model"]).as_deref() == Some(model)
                || json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).as_deref() == Some(model)
        }
        None => {
            json_string_lookup(&value, &["model"]).is_none()
                && json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).is_none()
        }
    };
    let token_matches = json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"])
        .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, &token))
        .unwrap_or(false);
    Ok(json_string_lookup(&value, &["env", "ANTHROPIC_BASE_URL"])
        .map(|base_url| {
            profile_runtime_base_url_matches(&profile.protocol, &base_url, &profile.base_url)
        })
        .unwrap_or(false)
        && token_matches
        && model_matches)
}

fn verify_claude_gateway_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let value = parse_json5_or_empty(
        &fs::read_to_string(path).map_err(|err| err.to_string())?,
        "Claude settings",
    )?;
    let model = gateway_config_model_for_profile(profile);
    Ok(
        json_string_lookup(&value, &["env", "ANTHROPIC_BASE_URL"]).as_deref()
            == Some(client.base_url.as_str())
            && json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"]).as_deref()
                == Some(client.token.as_str())
            && (json_string_lookup(&value, &["model"]).as_deref() == Some(model)
                || json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).as_deref()
                    == Some(model)),
    )
}

pub(in crate::core::profile) fn claude_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    claude_config_content_with_api_key(current, profile, &api_key)
}

pub(in crate::core::profile) fn claude_config_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_ANTHROPIC_MESSAGES])?;
    let mut value = parse_json5_or_empty(current, "Claude settings")?;
    let runtime_base_url =
        profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url);
    set_json_string_path(
        &mut value,
        &["env", "ANTHROPIC_BASE_URL"],
        &runtime_base_url,
    );
    set_json_string_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"], api_key);
    if let Some(model) = profile_model(profile) {
        set_json_string_path(&mut value, &["model"], model);
        set_json_string_path(&mut value, &["env", "ANTHROPIC_MODEL"], model);
    } else {
        remove_json_path(&mut value, &["model"]);
        remove_json_path(&mut value, &["env", "ANTHROPIC_MODEL"]);
    }
    render_json_config(value, "Claude settings")
}

pub(in crate::core::profile) fn claude_official_config_content(
    current: &str,
) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude settings")?;
    remove_json_path(&mut value, &["env", "ANTHROPIC_BASE_URL"]);
    remove_json_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"]);
    remove_json_path(&mut value, &["model"]);
    remove_json_path(&mut value, &["env", "ANTHROPIC_MODEL"]);
    render_json_config(value, "Claude settings")
}

pub(in crate::core::profile) fn claude_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "Claude settings")?;
    let model = gateway_config_model_for_profile(profile);
    set_json_string_path(&mut value, &["env", "ANTHROPIC_BASE_URL"], &client.base_url);
    set_json_string_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"], &client.token);
    set_json_string_path(&mut value, &["model"], model);
    set_json_string_path(&mut value, &["env", "ANTHROPIC_MODEL"], model);
    render_json_config(value, "Claude settings")
}

pub(in crate::core::profile) fn claude_gateway_cleanup_config_content(
    current: &str,
    tool_id: &str,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_json5_or_empty(current, "Claude settings")?;
    remove_json_string_path_if(&mut value, &["env", "ANTHROPIC_BASE_URL"], &client.base_url);
    if json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"])
        .as_deref()
        .map(looks_like_local_gateway_token)
        .unwrap_or(false)
    {
        remove_json_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"]);
    }
    remove_json_string_path_if(&mut value, &["model"], &client.model);
    remove_json_string_path_if(&mut value, &["model"], GATEWAY_FALLBACK_MODEL);
    remove_json_string_path_if(&mut value, &["env", "ANTHROPIC_MODEL"], &client.model);
    remove_json_string_path_if(
        &mut value,
        &["env", "ANTHROPIC_MODEL"],
        GATEWAY_FALLBACK_MODEL,
    );
    render_json_config(value, "Claude settings")
}
