use super::super::*;
use super::NativeProfileAdapter;

pub(in crate::core::profile) static OPENCLAW_ADAPTER: OpenClawAdapter = OpenClawAdapter;
pub(in crate::core::profile) struct OpenClawAdapter;

impl NativeProfileAdapter for OpenClawAdapter {
    fn target(&self, paths: &crate::core::app_paths::AppPaths) -> PathBuf {
        paths.home_dir.join(".openclaw").join("openclaw.json")
    }
    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                openclaw_official_config_content(current)
            }
            ProviderApplyMode::Config => openclaw_config_content(current, profile),
            ProviderApplyMode::Gateway => openclaw_gateway_config_content(current, profile),
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
                openclaw_official_config_content(current)
            }
            ProviderApplyMode::Config => {
                openclaw_config_content_with_api_key(current, profile, secret_preview(profile))
            }
            ProviderApplyMode::Gateway => openclaw_gateway_config_content(current, profile),
        }
    }
    fn cleanup_gateway(&self, current: &str) -> Result<String, String> {
        openclaw_gateway_cleanup_config_content(current, "openclaw")
    }
    fn inspect(&self, current: &str) -> Result<Option<DetectedNativeProfile>, String> {
        Ok(detect_openclaw_native_profile(&parse_json5_or_empty(
            current,
            "OpenClaw config",
        )?))
    }
    fn matches(
        &self,
        current: &str,
        profile: &ProfileDraft,
        secret_match: SecretMatchMode,
    ) -> Result<bool, String> {
        Ok(openclaw_config_matches_profile_with_secret_match(
            &parse_json5_or_empty(current, "OpenClaw config")?,
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
            ProviderApplyMode::Config => verify_openclaw_config(path, profile),
            ProviderApplyMode::Gateway => verify_openclaw_gateway_config(path, profile),
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
            (ProviderApplyMode::Config, true) => vec!["Official provider removes CodeStudio Lite managed OpenClaw provider entries.".to_string()],
            (ProviderApplyMode::Config, false) => vec!["OpenClaw providers are written in models.mode=merge so existing provider definitions can stay available.".to_string(), "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file.".to_string()],
            (ProviderApplyMode::Gateway, _) => vec!["Gateway profiles write OpenClaw's provider entry to the tool-scoped local gateway URL.".to_string(), "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file.".to_string(), "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.".to_string(), "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.".to_string()],
        };
        let (json, status) = read_json_preview(&path, "OpenClaw config", &mut warnings)?;
        let provider_id = custom_provider_id_for_profile(profile);
        let changes = if official {
            vec![
                json_diff_remove_line(
                    &json,
                    &["models", "providers", &provider_id],
                    "Deletes the managed OpenClaw provider.",
                ),
                json_diff_remove_line(
                    &json,
                    &["agents", "defaults", "model", "primary"],
                    "Removes the managed model reference.",
                ),
            ]
        } else {
            let (base_url, key, model) = if mode == ProviderApplyMode::Gateway {
                let c = gateway::client_config_for_tool("openclaw")?;
                (
                    c.base_url,
                    c.token_preview,
                    gateway_config_model_for_profile(profile).to_string(),
                )
            } else {
                (
                    profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url),
                    secret_preview(profile).to_string(),
                    profile_model(profile)
                        .unwrap_or(GATEWAY_FALLBACK_MODEL)
                        .to_string(),
                )
            };
            vec![
                json_diff_line(
                    &json,
                    &["models", "mode"],
                    "merge",
                    "Keeps existing OpenClaw providers available.",
                ),
                json_diff_line(
                    &json,
                    &["models", "providers", &provider_id, "baseUrl"],
                    &base_url,
                    "Sets the selected endpoint.",
                ),
                json_diff_line(
                    &json,
                    &["models", "providers", &provider_id, "apiKey"],
                    &key,
                    "Stores the appropriate redacted credential.",
                ),
                json_diff_line(
                    &json,
                    &["agents", "defaults", "model", "primary"],
                    &format!("{provider_id}/{model}"),
                    "Selects the managed OpenClaw model.",
                ),
            ]
        };
        Ok(NativeConfigPreview {
            tool: "openclaw".to_string(),
            path: display_path,
            status,
            write_enabled: true,
            changes,
            warnings,
            content: None,
        })
    }
}

pub(in crate::core::profile) fn openclaw_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    openclaw_config_content_with_api_key(current, profile, &api_key)
}

pub(in crate::core::profile) fn openclaw_config_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_OPENAI_CHAT_COMPLETIONS])?;
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    let provider_id = custom_provider_id_for_profile(profile);
    let provider_name = profile.provider.trim();
    remove_json_managed_provider_entries(&mut value, &["models", "providers"]);

    set_json_string_path(&mut value, &["models", "mode"], "merge");
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "name"],
        &provider_name,
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "api"],
        "openai-completions",
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "baseUrl"],
        &profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url),
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "apiKey"],
        api_key,
    );

    if let Some(model) = profile_model(profile) {
        set_json_string_path(
            &mut value,
            &["agents", "defaults", "model", "primary"],
            &format!("{provider_id}/{model}"),
        );
        set_json_value_path(
            &mut value,
            &["models", "providers", &provider_id, "models"],
            serde_json::json!([
                {
                    "id": model,
                    "name": model,
                    "input": ["text"],
                    "output": ["text"]
                }
            ]),
        );
    }

    render_json_config(value, "OpenClaw config")
}

pub(in crate::core::profile) fn openclaw_official_config_content(
    current: &str,
) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    for provider_id in json_object_keys(&value, &["models", "providers"])
        .into_iter()
        .filter(|provider_id| managed_json_provider_key(provider_id))
        .collect::<Vec<_>>()
    {
        let model_prefix = format!("{provider_id}/");
        if json_string_lookup(&value, &["agents", "defaults", "model", "primary"])
            .as_deref()
            .map(|model| model.starts_with(&model_prefix))
            .unwrap_or(false)
        {
            remove_json_path(&mut value, &["agents", "defaults", "model", "primary"]);
        }
        remove_json_path(&mut value, &["models", "providers", &provider_id]);
    }
    render_json_config(value, "OpenClaw config")
}

pub(in crate::core::profile) fn openclaw_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    let provider_id = client.provider_id;
    let model = gateway_config_model_for_profile(profile);

    set_json_string_path(&mut value, &["models", "mode"], "merge");
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "name"],
        &client.provider_name,
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "api"],
        "openai-completions",
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "baseUrl"],
        &client.base_url,
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "apiKey"],
        &client.token,
    );
    set_json_string_path(
        &mut value,
        &["agents", "defaults", "model", "primary"],
        &format!("{provider_id}/{model}"),
    );
    set_json_value_path(
        &mut value,
        &["models", "providers", &provider_id, "models"],
        serde_json::json!([
            {
                "id": model,
                "name": model,
                "input": ["text"],
                "output": ["text"]
            }
        ]),
    );

    render_json_config(value, "OpenClaw config")
}

pub(in crate::core::profile) fn openclaw_gateway_cleanup_config_content(
    current: &str,
    tool_id: &str,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    let provider_id = client.provider_id;
    let model_ref = format!("{provider_id}/{}", client.model);
    let fallback_model_ref = format!("{provider_id}/{GATEWAY_FALLBACK_MODEL}");

    remove_json_string_path_if(
        &mut value,
        &["agents", "defaults", "model", "primary"],
        &model_ref,
    );
    remove_json_string_path_if(
        &mut value,
        &["agents", "defaults", "model", "primary"],
        &fallback_model_ref,
    );
    remove_json_path(&mut value, &["models", "providers", &provider_id]);

    render_json_config(value, "OpenClaw config")
}
pub(in crate::core::profile) fn detect_openclaw_native_profile(
    value: &serde_json::Value,
) -> Option<DetectedNativeProfile> {
    let provider_id = openclaw_active_provider_id(value)?;
    if provider_id == "codestudio-local" {
        return None;
    }
    let base_url = json_string_lookup(value, &["models", "providers", &provider_id, "baseUrl"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())?;
    if looks_like_local_gateway_url(&base_url) {
        return None;
    }
    let api_key = json_string_lookup(value, &["models", "providers", &provider_id, "apiKey"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;
    let provider = json_string_lookup(value, &["models", "providers", &provider_id, "name"])
        .unwrap_or_else(|| provider_id.clone());

    Some(DetectedNativeProfile {
        app: "openclaw".to_string(),
        provider,
        protocol: PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string(),
        model: super::model_from_provider_ref(
            json_string_lookup(value, &["agents", "defaults", "model", "primary"]).as_deref(),
            &provider_id,
        )
        .unwrap_or_default(),
        review_model: None,
        base_url,
        api_key,
    })
}

pub(in crate::core::profile) fn openclaw_active_provider_id(
    value: &serde_json::Value,
) -> Option<String> {
    if let Some(model) = json_string_lookup(value, &["agents", "defaults", "model", "primary"]) {
        if let Some((provider, _)) = model.split_once('/') {
            if !provider.trim().is_empty() {
                return Some(provider.trim().to_string());
            }
        }
    }
    json_object_keys(value, &["models", "providers"])
        .into_iter()
        .find(|provider_id| {
            json_string_lookup(value, &["models", "providers", provider_id, "baseUrl"]).is_some()
                && json_string_lookup(value, &["models", "providers", provider_id, "apiKey"])
                    .is_some()
        })
}
pub(in crate::core::profile) fn openclaw_config_matches_profile(
    value: &serde_json::Value,
    profile: &ProfileDraft,
) -> bool {
    openclaw_config_matches_profile_with_secret_match(
        value,
        profile,
        SecretMatchMode::ExactKeychain,
    )
}

pub(in crate::core::profile) fn openclaw_config_matches_profile_with_secret_match(
    value: &serde_json::Value,
    profile: &ProfileDraft,
    secret_match: SecretMatchMode,
) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "openclaw"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
            && !openclaw_config_has_managed_provider(value);
    }

    if canonical_profile_app(&profile.app) != "openclaw"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref()
            != Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
    {
        return false;
    }

    let provider_id = custom_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => {
            json_string_lookup(value, &["agents", "defaults", "model", "primary"]).as_deref()
                == Some(model)
        }
        None => true,
    };
    let token_matches = json_string_lookup(value, &["models", "providers", &provider_id, "apiKey"])
        .map(|token| profile_api_key_matches_config(profile, &token, secret_match))
        .unwrap_or(false);

    json_string_lookup(value, &["models", "providers", &provider_id, "baseUrl"])
        .map(|base_url| {
            profile_runtime_base_url_matches(&profile.protocol, &base_url, &profile.base_url)
        })
        .unwrap_or(false)
        && token_matches
        && model_matches
}
pub(in crate::core::profile) fn openclaw_config_has_managed_provider(
    value: &serde_json::Value,
) -> bool {
    json_object_keys(value, &["models", "providers"])
        .into_iter()
        .any(|key| managed_json_provider_key(&key))
}
pub(in crate::core::profile) fn verify_openclaw_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "OpenClaw config")?;
    if provider_is_official(&profile.provider) {
        return Ok(openclaw_config_matches_profile(&value, profile));
    }
    let provider_id = custom_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => {
            json_string_lookup(&value, &["agents", "defaults", "model", "primary"]).as_deref()
                == Some(model)
        }
        None => true,
    };
    let token_matches =
        json_string_lookup(&value, &["models", "providers", &provider_id, "apiKey"])
            .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, &token))
            .unwrap_or(false);

    Ok(
        json_string_lookup(&value, &["models", "providers", &provider_id, "baseUrl"])
            .map(|base_url| {
                profile_runtime_base_url_matches(&profile.protocol, &base_url, &profile.base_url)
            })
            .unwrap_or(false)
            && token_matches
            && model_matches,
    )
}

pub(in crate::core::profile) fn verify_openclaw_gateway_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "OpenClaw config")?;
    let provider_id = client.provider_id;
    let expected_model = format!(
        "{provider_id}/{}",
        gateway_config_model_for_profile(profile)
    );

    Ok(
        json_string_lookup(&value, &["models", "providers", &provider_id, "baseUrl"]).as_deref()
            == Some(client.base_url.as_str())
            && json_string_lookup(&value, &["models", "providers", &provider_id, "apiKey"])
                .as_deref()
                == Some(client.token.as_str())
            && json_string_lookup(&value, &["agents", "defaults", "model", "primary"]).as_deref()
                == Some(expected_model.as_str()),
    )
}
