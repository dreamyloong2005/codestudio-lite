use super::super::*;
use super::NativeProfileAdapter;

pub(in crate::core::profile) static HERMES_ADAPTER: HermesAdapter = HermesAdapter;
pub(in crate::core::profile) struct HermesAdapter;

impl NativeProfileAdapter for HermesAdapter {
    fn target(&self, paths: &crate::core::app_paths::AppPaths) -> PathBuf {
        paths.home_dir.join(".hermes").join("config.yaml")
    }
    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                hermes_official_config_content(current)
            }
            ProviderApplyMode::Config => hermes_config_content(current, profile),
            ProviderApplyMode::Gateway => hermes_gateway_config_content(current, profile),
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
                hermes_official_config_content(current)
            }
            ProviderApplyMode::Config => {
                hermes_config_content_with_api_key(current, profile, secret_preview(profile))
            }
            ProviderApplyMode::Gateway => hermes_gateway_config_content(current, profile),
        }
    }
    fn cleanup_gateway(&self, current: &str) -> Result<String, String> {
        hermes_gateway_cleanup_config_content(current, "hermes")
    }
    fn inspect(&self, current: &str) -> Result<Option<DetectedNativeProfile>, String> {
        Ok(detect_hermes_native_profile(&parse_yaml_or_empty(
            current,
            "Hermes config",
        )?))
    }
    fn matches(
        &self,
        current: &str,
        profile: &ProfileDraft,
        secret_match: SecretMatchMode,
    ) -> Result<bool, String> {
        Ok(hermes_config_matches_profile_with_secret_match(
            &parse_yaml_or_empty(current, "Hermes config")?,
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
            ProviderApplyMode::Config => verify_hermes_config(path, profile),
            ProviderApplyMode::Gateway => verify_hermes_gateway_config(path, profile),
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
            (ProviderApplyMode::Config, true) => vec!["Official provider removes CodeStudio Lite managed Hermes custom endpoint fields.".to_string()],
            (ProviderApplyMode::Config, false) => vec![
                "Hermes custom providers are written to ~/.hermes/config.yaml under the model section.".to_string(),
                "Existing YAML comments are not preserved when CodeStudio Lite writes the file.".to_string(),
                "Hermes config profiles currently target OpenAI Chat Completions endpoints.".to_string(),
            ],
            (ProviderApplyMode::Gateway, _) => vec![
                "Gateway profiles write Hermes custom provider settings to the tool-scoped local gateway URL.".to_string(),
                "Existing YAML comments are not preserved when CodeStudio Lite writes the file.".to_string(),
                "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.".to_string(),
                "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.".to_string(),
            ],
        };
        let (yaml, status) = read_yaml_preview(&path, "Hermes config", &mut warnings)?;
        let changes = if official {
            vec![
                yaml_diff_remove_line(
                    &yaml,
                    &["model", "provider"],
                    "Removes CodeStudio Lite custom provider mode.",
                ),
                yaml_diff_remove_line(
                    &yaml,
                    &["model", "base_url"],
                    "Removes the managed endpoint.",
                ),
                yaml_diff_remove_line(&yaml, &["model", "api_key"], "Removes the managed API key."),
                yaml_diff_remove_line(
                    &yaml,
                    &["model", "api_mode"],
                    "Removes the managed API mode.",
                ),
                yaml_diff_remove_line(
                    &yaml,
                    &["model", "default"],
                    "Removes the managed model override.",
                ),
            ]
        } else {
            let (base_url, api_key, model) = if mode == ProviderApplyMode::Gateway {
                let client = gateway::client_config_for_tool("hermes")?;
                (
                    client.base_url,
                    client.token_preview,
                    Some(gateway_config_model_for_profile(profile).to_string()),
                )
            } else {
                (
                    profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url),
                    secret_preview(profile).to_string(),
                    profile_model(profile).map(ToString::to_string),
                )
            };
            let mut changes = vec![
                yaml_diff_line(
                    &yaml,
                    &["model", "provider"],
                    "custom",
                    "Selects Hermes custom provider mode.",
                ),
                yaml_diff_line(
                    &yaml,
                    &["model", "base_url"],
                    &base_url,
                    "Sets the selected endpoint.",
                ),
                yaml_diff_line(
                    &yaml,
                    &["model", "api_key"],
                    &api_key,
                    "Stores the appropriate redacted credential.",
                ),
                yaml_diff_line(
                    &yaml,
                    &["model", "api_mode"],
                    "chat_completions",
                    "Uses OpenAI Chat Completions custom endpoint mode.",
                ),
            ];
            if let Some(model) = model {
                changes.push(yaml_diff_line(
                    &yaml,
                    &["model", "default"],
                    &model,
                    "Sets the selected model.",
                ));
            } else {
                changes.push(yaml_diff_remove_line(
                    &yaml,
                    &["model", "default"],
                    "Model is optional; no override will be written.",
                ));
            }
            changes
        };
        Ok(NativeConfigPreview {
            tool: "hermes".to_string(),
            path: display_path,
            status,
            write_enabled: true,
            changes,
            warnings,
            content: None,
        })
    }
}

pub(in crate::core::profile) fn hermes_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    hermes_config_content_with_api_key(current, profile, &api_key)
}

pub(in crate::core::profile) fn hermes_config_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_OPENAI_CHAT_COMPLETIONS])?;
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;
    let runtime_base_url =
        profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url);

    set_yaml_string_path(&mut value, &["model", "provider"], "custom");
    set_yaml_string_path(&mut value, &["model", "base_url"], &runtime_base_url);
    set_yaml_string_path(&mut value, &["model", "api_key"], api_key);
    set_yaml_string_path(&mut value, &["model", "api_mode"], "chat_completions");
    if let Some(model) = profile_model(profile) {
        set_yaml_string_path(&mut value, &["model", "default"], model);
    } else {
        remove_yaml_path(&mut value, &["model", "default"]);
    }

    render_yaml_config(value, "Hermes config")
}

pub(in crate::core::profile) fn hermes_official_config_content(
    current: &str,
) -> Result<String, String> {
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;
    remove_yaml_string_path_if(&mut value, &["model", "provider"], "custom");
    remove_yaml_path(&mut value, &["model", "base_url"]);
    remove_yaml_path(&mut value, &["model", "api_key"]);
    remove_yaml_path(&mut value, &["model", "api_mode"]);
    remove_yaml_path(&mut value, &["model", "default"]);
    render_yaml_config(value, "Hermes config")
}

pub(in crate::core::profile) fn hermes_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;
    let model = gateway_config_model_for_profile(profile);

    set_yaml_string_path(&mut value, &["model", "provider"], "custom");
    set_yaml_string_path(&mut value, &["model", "base_url"], &client.base_url);
    set_yaml_string_path(&mut value, &["model", "api_key"], &client.token);
    set_yaml_string_path(&mut value, &["model", "api_mode"], "chat_completions");
    set_yaml_string_path(&mut value, &["model", "default"], model);

    render_yaml_config(value, "Hermes config")
}

pub(in crate::core::profile) fn hermes_gateway_cleanup_config_content(
    current: &str,
    tool_id: &str,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;

    remove_yaml_string_path_if(&mut value, &["model", "base_url"], &client.base_url);
    if yaml_string_lookup(&value, &["model", "api_key"])
        .as_deref()
        .map(looks_like_local_gateway_token)
        .unwrap_or(false)
    {
        remove_yaml_path(&mut value, &["model", "api_key"]);
    }
    remove_yaml_string_path_if(&mut value, &["model", "api_mode"], "chat_completions");
    remove_yaml_string_path_if(&mut value, &["model", "default"], &client.model);
    remove_yaml_string_path_if(&mut value, &["model", "default"], GATEWAY_FALLBACK_MODEL);
    if yaml_string_lookup(&value, &["model", "base_url"]).is_none()
        && yaml_string_lookup(&value, &["model", "api_key"]).is_none()
    {
        remove_yaml_string_path_if(&mut value, &["model", "provider"], "custom");
    }

    render_yaml_config(value, "Hermes config")
}
pub(in crate::core::profile) fn detect_hermes_native_profile(
    value: &serde_norway::Value,
) -> Option<DetectedNativeProfile> {
    if yaml_string_lookup(value, &["model", "provider"]).as_deref() != Some("custom") {
        return None;
    }
    let base_url = yaml_string_lookup(value, &["model", "base_url"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())?;
    let api_key = yaml_string_lookup(value, &["model", "api_key"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;
    if yaml_string_lookup(value, &["model", "api_mode"])
        .as_deref()
        .map(|mode| mode != "chat_completions")
        .unwrap_or(false)
    {
        return None;
    }

    Some(DetectedNativeProfile {
        app: "hermes".to_string(),
        provider: provider_slug_from_base_url(&base_url).unwrap_or_else(|| "openai".to_string()),
        protocol: PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string(),
        model: yaml_string_lookup(value, &["model", "default"])
            .and_then(|model| native_optional_model(&model))
            .unwrap_or_default(),
        review_model: None,
        base_url,
        api_key,
    })
}
pub(in crate::core::profile) fn hermes_config_matches_profile(
    value: &serde_norway::Value,
    profile: &ProfileDraft,
) -> bool {
    hermes_config_matches_profile_with_secret_match(value, profile, SecretMatchMode::ExactKeychain)
}

pub(in crate::core::profile) fn hermes_config_matches_profile_with_secret_match(
    value: &serde_norway::Value,
    profile: &ProfileDraft,
    secret_match: SecretMatchMode,
) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "hermes"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
            && !hermes_config_has_managed_endpoint(value);
    }

    if canonical_profile_app(&profile.app) != "hermes"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref()
            != Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
    {
        return false;
    }

    let model_matches = match profile_model(profile) {
        Some(model) => yaml_string_lookup(value, &["model", "default"]).as_deref() == Some(model),
        None => yaml_string_lookup(value, &["model", "default"]).is_none(),
    };
    let token_matches = yaml_string_lookup(value, &["model", "api_key"])
        .map(|token| profile_api_key_matches_config(profile, &token, secret_match))
        .unwrap_or(false);

    yaml_string_lookup(value, &["model", "provider"]).as_deref() == Some("custom")
        && yaml_string_lookup(value, &["model", "base_url"])
            .map(|base_url| {
                profile_runtime_base_url_matches(&profile.protocol, &base_url, &profile.base_url)
            })
            .unwrap_or(false)
        && yaml_string_lookup(value, &["model", "api_mode"]).as_deref() == Some("chat_completions")
        && token_matches
        && model_matches
}
pub(in crate::core::profile) fn hermes_config_has_managed_endpoint(
    value: &serde_norway::Value,
) -> bool {
    yaml_string_lookup(value, &["model", "base_url"]).is_some()
        || yaml_string_lookup(value, &["model", "api_key"]).is_some()
}
pub(in crate::core::profile) fn verify_hermes_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_yaml_or_empty(&content, "Hermes config")?;
    if provider_is_official(&profile.provider) {
        return Ok(hermes_config_matches_profile(&value, profile));
    }
    let model_matches = match profile_model(profile) {
        Some(model) => yaml_string_lookup(&value, &["model", "default"]).as_deref() == Some(model),
        None => yaml_string_lookup(&value, &["model", "default"]).is_none(),
    };
    let token_matches = yaml_string_lookup(&value, &["model", "api_key"])
        .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, &token))
        .unwrap_or(false);

    Ok(
        yaml_string_lookup(&value, &["model", "provider"]).as_deref() == Some("custom")
            && yaml_string_lookup(&value, &["model", "base_url"]).as_deref()
                == Some(
                    profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url)
                        .as_str(),
                )
            && yaml_string_lookup(&value, &["model", "api_mode"]).as_deref()
                == Some("chat_completions")
            && token_matches
            && model_matches,
    )
}

pub(in crate::core::profile) fn verify_hermes_gateway_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_yaml_or_empty(&content, "Hermes config")?;
    let model = gateway_config_model_for_profile(profile);

    Ok(
        yaml_string_lookup(&value, &["model", "provider"]).as_deref() == Some("custom")
            && yaml_string_lookup(&value, &["model", "base_url"]).as_deref()
                == Some(client.base_url.as_str())
            && yaml_string_lookup(&value, &["model", "api_key"]).as_deref()
                == Some(client.token.as_str())
            && yaml_string_lookup(&value, &["model", "api_mode"]).as_deref()
                == Some("chat_completions")
            && yaml_string_lookup(&value, &["model", "default"]).as_deref() == Some(model),
    )
}
