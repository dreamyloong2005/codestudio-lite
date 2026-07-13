use super::super::*;
use super::NativeProfileAdapter;

pub(in crate::core::profile) const MANAGED_MODEL_ID: &str = "codestudio";
pub(in crate::core::profile) const OFFICIAL_DEFAULT_MODEL: &str = "grok-build";
pub(in crate::core::profile) static GROK_ADAPTER: GrokAdapter = GrokAdapter;

pub(in crate::core::profile) struct GrokAdapter;

impl NativeProfileAdapter for GrokAdapter {
    fn target(&self, paths: &crate::core::app_paths::AppPaths) -> PathBuf {
        paths.home_dir.join(".grok").join("config.toml")
    }

    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                grok_official_config_content(current)
            }
            ProviderApplyMode::Config => grok_config_content(current, profile),
            ProviderApplyMode::Gateway => grok_gateway_config_content(current, profile),
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
                grok_official_config_content(current)
            }
            ProviderApplyMode::Config => {
                grok_config_content_with_api_key(current, profile, secret_preview(profile))
            }
            ProviderApplyMode::Gateway => grok_gateway_config_content(current, profile),
        }
    }

    fn cleanup_gateway(&self, current: &str) -> Result<String, String> {
        grok_gateway_cleanup_config_content(current, "grok")
    }

    fn inspect(&self, current: &str) -> Result<Option<DetectedNativeProfile>, String> {
        Ok(detect_grok_native_profile(&parse_toml_or_empty(
            current,
            "Grok config",
        )?))
    }

    fn matches(
        &self,
        current: &str,
        profile: &ProfileDraft,
        secret_match: SecretMatchMode,
    ) -> Result<bool, String> {
        Ok(grok_config_matches_profile_with_secret_match(
            &parse_toml_or_empty(current, "Grok config")?,
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
            ProviderApplyMode::Config => verify_grok_config(path, profile),
            ProviderApplyMode::Gateway => verify_grok_gateway_config(path, profile),
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
            (ProviderApplyMode::Config, true) => vec!["Official provider restores Grok to its own login and removes the CodeStudio managed model entry.".to_string()],
            (ProviderApplyMode::Config, false) => vec![
                "Grok custom models are written to ~/.grok/config.toml under [models] and [model.codestudio].".to_string(),
                "Existing TOML comments are not preserved when CodeStudio Lite writes the file.".to_string(),
                "Restart Grok or open a new session after applying so the model catalog reloads.".to_string(),
            ],
            (ProviderApplyMode::Gateway, _) => vec![
                "Gateway profiles write Grok custom model settings to the tool-scoped local gateway URL.".to_string(),
                "Existing TOML comments are not preserved when CodeStudio Lite writes the file.".to_string(),
                "Restart Grok or open a new session after applying so the model catalog reloads.".to_string(),
                "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.".to_string(),
                "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.".to_string(),
            ],
        };
        let (root, status) = read_toml_preview(&path, "Grok config", &mut warnings)?;
        let changes = if official {
            vec![
                diff_line(
                    &root,
                    "models.default",
                    OFFICIAL_DEFAULT_MODEL,
                    "Restores Grok to its built-in default model.",
                ),
                diff_remove_line(
                    &root,
                    &format!("model.{MANAGED_MODEL_ID}"),
                    "Removes the CodeStudio Lite managed Grok model entry.",
                ),
            ]
        } else {
            let (model, base_url, key, backend, name) = if mode == ProviderApplyMode::Gateway {
                let client = gateway::client_config_for_tool("grok")?;
                (
                    gateway_config_model_for_profile(profile).to_string(),
                    client.base_url,
                    client.token_preview,
                    "chat_completions".to_string(),
                    format!("CodeStudio Lite ({})", client.provider_name),
                )
            } else {
                (
                    profile_model(profile)
                        .unwrap_or(GATEWAY_FALLBACK_MODEL)
                        .to_string(),
                    profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url),
                    secret_preview(profile).to_string(),
                    grok_api_backend_for_protocol(&profile.protocol)?.to_string(),
                    if profile.name.trim().is_empty() {
                        format!("CodeStudio Lite ({})", profile.provider)
                    } else {
                        profile.name.trim().to_string()
                    },
                )
            };
            vec![
                diff_line(
                    &root,
                    "models.default",
                    MANAGED_MODEL_ID,
                    "Selects the CodeStudio managed Grok model as the session default.",
                ),
                diff_line(
                    &root,
                    &format!("model.{MANAGED_MODEL_ID}.model"),
                    &model,
                    "Sets the selected model id.",
                ),
                diff_line(
                    &root,
                    &format!("model.{MANAGED_MODEL_ID}.base_url"),
                    &base_url,
                    "Sets the selected endpoint.",
                ),
                diff_line(
                    &root,
                    &format!("model.{MANAGED_MODEL_ID}.api_key"),
                    &key,
                    "Stores the selected credential.",
                ),
                diff_line(
                    &root,
                    &format!("model.{MANAGED_MODEL_ID}.api_backend"),
                    &backend,
                    "Selects the Grok protocol adapter.",
                ),
                diff_line(
                    &root,
                    &format!("model.{MANAGED_MODEL_ID}.name"),
                    &name,
                    "Sets the display name shown in the Grok model picker.",
                ),
            ]
        };
        Ok(NativeConfigPreview {
            tool: "grok".to_string(),
            path: display_path,
            status,
            write_enabled: true,
            changes,
            warnings,
            content: None,
        })
    }
}

pub(in crate::core::profile) fn grok_api_backend_for_protocol(
    protocol: &str,
) -> Result<&'static str, String> {
    match protocol {
        PROTOCOL_OPENAI_CHAT_COMPLETIONS => Ok("chat_completions"),
        PROTOCOL_OPENAI_RESPONSES => Ok("responses"),
        PROTOCOL_ANTHROPIC_MESSAGES => Ok("messages"),
        _ => Err(format!(
            "Grok does not support {} in Config profiles.",
            protocol_display_name(protocol)
        )),
    }
}

pub(in crate::core::profile) fn protocol_for_grok_api_backend(value: &str) -> Option<&'static str> {
    match value.trim() {
        "chat_completions" | "chat" => Some(PROTOCOL_OPENAI_CHAT_COMPLETIONS),
        "responses" => Some(PROTOCOL_OPENAI_RESPONSES),
        "messages" => Some(PROTOCOL_ANTHROPIC_MESSAGES),
        _ => None,
    }
}

pub(in crate::core::profile) fn grok_model_field(
    value: &toml::Value,
    model_id: &str,
    field: &str,
) -> Option<String> {
    value
        .get("model")?
        .get(model_id)?
        .get(field)?
        .as_str()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(in crate::core::profile) fn grok_active_model_id(value: &toml::Value) -> Option<String> {
    value
        .get("models")
        .and_then(|models| models.get("default"))
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("model")?
                .as_table()?
                .keys()
                .next()
                .map(|key| key.to_string())
        })
}

pub(in crate::core::profile) fn grok_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    grok_config_content_with_api_key(current, profile, &api_key)
}

pub(in crate::core::profile) fn grok_config_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(
        profile,
        &[
            PROTOCOL_OPENAI_CHAT_COMPLETIONS,
            PROTOCOL_OPENAI_RESPONSES,
            PROTOCOL_ANTHROPIC_MESSAGES,
        ],
    )?;
    let api_backend = grok_api_backend_for_protocol(&profile.protocol)?;
    let runtime_base_url =
        profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url);
    let model = profile_model(profile).unwrap_or(GATEWAY_FALLBACK_MODEL);
    let display_name = if profile.name.trim().is_empty() {
        format!("CodeStudio Lite ({})", profile.provider)
    } else {
        profile.name.trim().to_string()
    };

    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| toml_edit::DocumentMut::new());
    set_grok_managed_model_entry(
        &mut document,
        model,
        &runtime_base_url,
        &display_name,
        api_key,
        api_backend,
    );
    Ok(document.to_string())
}

pub(in crate::core::profile) fn set_grok_managed_model_entry(
    document: &mut toml_edit::DocumentMut,
    model: &str,
    base_url: &str,
    display_name: &str,
    api_key: &str,
    api_backend: &str,
) {
    if document.get("models").is_none() {
        document["models"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    document["models"]["default"] = toml_edit::value(MANAGED_MODEL_ID);

    let mut model_entry = toml_edit::Table::new();
    model_entry["model"] = toml_edit::value(model);
    model_entry["base_url"] = toml_edit::value(base_url);
    model_entry["name"] = toml_edit::value(display_name);
    model_entry["api_key"] = toml_edit::value(api_key);
    model_entry["api_backend"] = toml_edit::value(api_backend);
    if document.get("model").is_none() {
        let mut root = toml_edit::Table::new();
        root.set_implicit(true);
        document["model"] = toml_edit::Item::Table(root);
    }
    document["model"][MANAGED_MODEL_ID] = toml_edit::Item::Table(model_entry);
}

pub(in crate::core::profile) fn grok_official_config_content(
    current: &str,
) -> Result<String, String> {
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| toml_edit::DocumentMut::new());
    if document
        .get("models")
        .and_then(|models| models.get("default"))
        .and_then(|item| item.as_str())
        == Some(MANAGED_MODEL_ID)
    {
        document["models"]["default"] = toml_edit::value(OFFICIAL_DEFAULT_MODEL);
    }
    if let Some(model_table) = document
        .get_mut("model")
        .and_then(|item| item.as_table_like_mut())
    {
        model_table.remove(MANAGED_MODEL_ID);
    }
    Ok(document.to_string())
}

pub(in crate::core::profile) fn grok_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let model = gateway_config_model_for_profile(profile);
    // Local gateway exposes OpenAI-compatible endpoints; chat completions is the
    // broadest default for third-party tools pointed at CodeStudio Lite.
    let api_backend = "chat_completions";

    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| toml_edit::DocumentMut::new());
    set_grok_managed_model_entry(
        &mut document,
        model,
        &client.base_url,
        &format!("CodeStudio Lite ({})", client.provider_name),
        &client.token,
        api_backend,
    );
    Ok(document.to_string())
}

pub(in crate::core::profile) fn grok_gateway_cleanup_config_content(
    current: &str,
    tool_id: &str,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| toml_edit::DocumentMut::new());
    let managed = document
        .get("model")
        .and_then(|item| item.get(MANAGED_MODEL_ID))
        .cloned();
    let should_remove = managed
        .as_ref()
        .and_then(|item| item.get("base_url"))
        .and_then(|item| item.as_str())
        .map(|base_url| {
            base_url.trim() == client.base_url.trim() || looks_like_local_gateway_url(base_url)
        })
        .unwrap_or(false)
        || managed
            .as_ref()
            .and_then(|item| item.get("api_key"))
            .and_then(|item| item.as_str())
            .map(looks_like_local_gateway_token)
            .unwrap_or(false);
    if should_remove {
        if let Some(model_table) = document
            .get_mut("model")
            .and_then(|item| item.as_table_like_mut())
        {
            model_table.remove(MANAGED_MODEL_ID);
        }
        if document
            .get("models")
            .and_then(|models| models.get("default"))
            .and_then(|item| item.as_str())
            == Some(MANAGED_MODEL_ID)
        {
            document["models"]["default"] = toml_edit::value(OFFICIAL_DEFAULT_MODEL);
        }
    }
    Ok(document.to_string())
}

pub(in crate::core::profile) fn detect_grok_native_profile(
    value: &toml::Value,
) -> Option<DetectedNativeProfile> {
    let model_id = grok_active_model_id(value)?;
    let base_url = grok_model_field(value, &model_id, "base_url")?;
    if looks_like_local_gateway_url(&base_url) {
        return None;
    }
    let api_key = grok_model_field(value, &model_id, "api_key")
        .filter(|item| !looks_like_local_gateway_token(item))?;
    let api_backend = grok_model_field(value, &model_id, "api_backend")
        .unwrap_or_else(|| "chat_completions".into());
    let protocol = protocol_for_grok_api_backend(&api_backend)?;
    let model = grok_model_field(value, &model_id, "model")
        .or_else(|| native_optional_model(&model_id))
        .unwrap_or_default();

    Some(DetectedNativeProfile {
        app: "grok".to_string(),
        provider: provider_slug_from_base_url(&base_url).unwrap_or_else(|| "custom".to_string()),
        protocol: protocol.to_string(),
        model,
        review_model: None,
        base_url,
        api_key,
    })
}

pub(in crate::core::profile) fn grok_config_matches_profile(
    value: &toml::Value,
    profile: &ProfileDraft,
) -> bool {
    grok_config_matches_profile_with_secret_match(value, profile, SecretMatchMode::ExactKeychain)
}

pub(in crate::core::profile) fn grok_config_matches_profile_with_secret_match(
    value: &toml::Value,
    profile: &ProfileDraft,
    secret_match: SecretMatchMode,
) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "grok"
            && profile.mode == ProviderApplyMode::Config
            && !grok_config_has_managed_endpoint(value);
    }

    if canonical_profile_app(&profile.app) != "grok" || profile.mode != ProviderApplyMode::Config {
        return false;
    }
    let Ok(api_backend) = grok_api_backend_for_protocol(&profile.protocol) else {
        return false;
    };
    let model_id = grok_active_model_id(value).unwrap_or_default();
    if model_id != MANAGED_MODEL_ID {
        // Also accept a non-managed default that still matches identity (imported custom).
        let Some(base_url) = grok_model_field(value, &model_id, "base_url") else {
            return false;
        };
        let token_matches = grok_model_field(value, &model_id, "api_key")
            .map(|token| profile_api_key_matches_config(profile, &token, secret_match))
            .unwrap_or(false);
        let model_matches = match profile_model(profile) {
            Some(model) => {
                grok_model_field(value, &model_id, "model").as_deref() == Some(model)
                    || model_id == model
            }
            None => true,
        };
        let backend_matches = grok_model_field(value, &model_id, "api_backend")
            .unwrap_or_else(|| "chat_completions".into())
            == api_backend;
        return token_matches
            && model_matches
            && backend_matches
            && profile_runtime_base_url_matches(&profile.protocol, &base_url, &profile.base_url);
    }

    let base_url = grok_model_field(value, MANAGED_MODEL_ID, "base_url");
    let token_matches = grok_model_field(value, MANAGED_MODEL_ID, "api_key")
        .map(|token| profile_api_key_matches_config(profile, &token, secret_match))
        .unwrap_or(false);
    let model_matches = match profile_model(profile) {
        Some(model) => grok_model_field(value, MANAGED_MODEL_ID, "model").as_deref() == Some(model),
        None => true,
    };
    let backend_matches =
        grok_model_field(value, MANAGED_MODEL_ID, "api_backend").as_deref() == Some(api_backend);

    base_url
        .as_deref()
        .map(|base_url| {
            profile_runtime_base_url_matches(&profile.protocol, base_url, &profile.base_url)
        })
        .unwrap_or(false)
        && token_matches
        && model_matches
        && backend_matches
}

pub(in crate::core::profile) fn grok_config_has_managed_endpoint(value: &toml::Value) -> bool {
    if grok_model_field(value, MANAGED_MODEL_ID, "base_url").is_some()
        || grok_model_field(value, MANAGED_MODEL_ID, "api_key").is_some()
    {
        return true;
    }
    let Some(model_id) = grok_active_model_id(value) else {
        return false;
    };
    grok_model_field(value, &model_id, "base_url").is_some()
        || grok_model_field(value, &model_id, "api_key").is_some()
}

pub(in crate::core::profile) fn verify_grok_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_toml_or_empty(&content, "Grok config")?;
    Ok(grok_config_matches_profile(&value, profile))
}

pub(in crate::core::profile) fn verify_grok_gateway_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_toml_or_empty(&content, "Grok config")?;
    let model = gateway_config_model_for_profile(profile);
    Ok(
        grok_active_model_id(&value).as_deref() == Some(MANAGED_MODEL_ID)
            && grok_model_field(&value, MANAGED_MODEL_ID, "base_url").as_deref()
                == Some(client.base_url.as_str())
            && grok_model_field(&value, MANAGED_MODEL_ID, "api_key").as_deref()
                == Some(client.token.as_str())
            && grok_model_field(&value, MANAGED_MODEL_ID, "model").as_deref() == Some(model)
            && grok_model_field(&value, MANAGED_MODEL_ID, "api_backend").as_deref()
                == Some("chat_completions"),
    )
}
