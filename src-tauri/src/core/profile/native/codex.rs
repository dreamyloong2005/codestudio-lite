use super::super::*;
use super::NativeProfileAdapter;

pub(in crate::core::profile) const CODEX_ACTOR_AUTHORIZATION_HEADER: &str =
    "x-openai-actor-authorization";
pub(in crate::core::profile) const CODEX_ACTOR_AUTHORIZATION_VALUE: &str = "codestudio-lite";
pub(in crate::core::profile) const CODEX_ACTOR_AUTHORIZATION_INLINE_TOML: &str =
    r#"{ "x-openai-actor-authorization" = "codestudio-lite" }"#;

pub(in crate::core::profile) fn auth_json_has_chatgpt_markers(value: &serde_json::Value) -> bool {
    let mut keys = Vec::new();
    collect_json_key_paths(value, String::new(), &mut keys);
    keys.iter().any(|key| {
        key.contains("chatgpt")
            || key.contains("refresh_token")
            || key.contains("id_token")
            || key.contains("account_id")
            || key.contains("expires_at")
    })
}

fn auth_api_key_from_value(value: &serde_json::Value) -> Option<String> {
    [
        "experimental_bearer_token",
        "OPENAI_API_KEY",
        "openai_api_key",
        "api_key",
    ]
    .into_iter()
    .find_map(|key| {
        value
            .get(key)
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
    })
}

fn wire_api_for_protocol(protocol: &str) -> Result<&'static str, String> {
    match normalize_protocol(Some(protocol))?.as_str() {
        PROTOCOL_OPENAI_RESPONSES => Ok("responses"),
        PROTOCOL_OPENAI_CHAT_COMPLETIONS => Ok("chat"),
        PROTOCOL_ANTHROPIC_MESSAGES => {
            Err("Codex native config does not support Claude Messages API directly.".to_string())
        }
        PROTOCOL_GOOGLE_GEMINI => {
            Err("Codex native config does not support Gemini API directly.".to_string())
        }
        _ => Err("Unsupported Codex wire API protocol.".to_string()),
    }
}

pub(in crate::core::profile) fn provider_id_for_profile(_profile: &ProfileDraft) -> String {
    "custom".to_string()
}

pub(in crate::core::profile) static CODEX_ADAPTER: CodexAdapter = CodexAdapter;
pub(in crate::core::profile) struct CodexAdapter;

impl NativeProfileAdapter for CodexAdapter {
    fn target(&self, paths: &crate::core::app_paths::AppPaths) -> PathBuf {
        paths.home_dir.join(".codex").join("config.toml")
    }
    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config => codex_direct_config_content(current, profile),
            ProviderApplyMode::Gateway => codex_gateway_config_content(current, profile),
        }
    }
    fn render_preview(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        self.render(current, profile, mode)
    }
    fn cleanup_gateway(&self, current: &str) -> Result<String, String> {
        Ok(current.to_string())
    }
    fn inspect(&self, _current: &str) -> Result<Option<DetectedNativeProfile>, String> {
        Err("Codex inspection requires config.toml and auth.json.".to_string())
    }
    fn matches(
        &self,
        _current: &str,
        _profile: &ProfileDraft,
        _secret_match: SecretMatchMode,
    ) -> Result<bool, String> {
        Err("Codex matching requires config.toml and auth.json.".to_string())
    }
    fn verify(
        &self,
        path: &Path,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<bool, String> {
        verify_config(path, profile, mode)
    }
    fn preview(
        &self,
        profile: &ProfileDraft,
        path: PathBuf,
        display_path: String,
        mode: ProviderApplyMode,
    ) -> Result<NativeConfigPreview, String> {
        preview_config(profile, path, display_path, mode)
    }
}

fn preview_config(
    profile: &ProfileDraft,
    path: PathBuf,
    display_path: String,
    mode: ProviderApplyMode,
) -> Result<NativeConfigPreview, String> {
    let primary_model = if mode == ProviderApplyMode::Gateway {
        gateway_config_model_for_profile(profile)
    } else {
        profile.model.trim()
    };
    let mut warnings = match mode {
        ProviderApplyMode::Config if provider_is_official(&profile.provider) => vec!["Official provider uses the target client's own login.".to_string(), "No Provider API key or model override is required.".to_string(), "Changing Codex config usually requires restarting Codex or opening a new Codex session.".to_string()],
        ProviderApplyMode::Config => vec!["Config profiles write Codex's provider entry directly to the selected upstream Provider.".to_string(), "The preview masks the Provider API key. Apply writes the actual key from the system keychain to Codex auth.json.".to_string(), "Changing Codex config usually requires restarting Codex or opening a new Codex session.".to_string()],
        ProviderApplyMode::Gateway => vec!["Gateway profiles are a one-time relay injection target, not a direct Provider switch.".to_string(), "Switching profiles later changes only the Gateway active profile for this tool.".to_string(), "The preview masks the local CodeStudio token. Apply writes only this local token to Codex auth.json; upstream Provider keys stay in the system keychain.".to_string(), "Codex official login is still required for the desktop app; the Local Gateway only takes over model requests.".to_string(), "If Codex is already running, restart Codex or open a new Codex session after bootstrap so it reloads config.toml.".to_string()],
    };
    let (value, status) = if path.exists() {
        let content = fs::read_to_string(&path).map_err(|err| err.to_string())?;
        match toml::from_str::<toml::Value>(&content) {
            Ok(value) => (value, "parsed".to_string()),
            Err(err) => {
                warnings.push(format!("Existing Codex config could not be parsed, so only create-style preview is available: {err}"));
                (
                    toml::Value::Table(toml::map::Map::new()),
                    "parse_error".to_string(),
                )
            }
        }
    } else {
        warnings.push(
            "Codex config does not exist yet; adapter would create it after confirmation."
                .to_string(),
        );
        (
            toml::Value::Table(toml::map::Map::new()),
            "missing".to_string(),
        )
    };
    let (provider_id, provider_name, wire_api, base_url) = if mode == ProviderApplyMode::Gateway {
        let client = gateway::client_config_for_tool(&profile.app)?;
        (
            client.provider_id,
            client.provider_name,
            "responses".to_string(),
            client.base_url,
        )
    } else if provider_is_official(&profile.provider) {
        (
            "openai".to_string(),
            "OpenAI".to_string(),
            "responses".to_string(),
            String::new(),
        )
    } else {
        (
            provider_id_for_profile(profile),
            profile.provider.trim().to_string(),
            wire_api_for_protocol(&profile.protocol)?.to_string(),
            profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url),
        )
    };
    let mut changes = vec![
        diff_line(
            &value,
            "model_provider",
            &provider_id,
            if mode == ProviderApplyMode::Gateway {
                "Selects the CodeStudio Lite localhost provider."
            } else if provider_is_official(&profile.provider) {
                "Selects Codex's official OpenAI provider."
            } else {
                "Selects the direct provider entry managed by CodeStudio Lite."
            },
        ),
        diff_line(
            &value,
            "cli_auth_credentials_store",
            "file",
            "Uses file-backed Codex authentication so managed credentials are read from auth.json.",
        ),
        diff_line(
            &value,
            &format!("model_providers.{provider_id}.requires_openai_auth"),
            "false",
            "Disables Codex's built-in OpenAI auth requirement for this managed provider.",
        ),
        diff_line(
            &value,
            &format!("model_providers.{provider_id}.http_headers"),
            CODEX_ACTOR_AUTHORIZATION_INLINE_TOML,
            "Adds the CodeStudio Lite actor-authorization header to this managed provider.",
        ),
    ];
    if provider_is_official(&profile.provider) && mode == ProviderApplyMode::Config {
        changes.push(diff_remove_line(
            &value,
            "model_providers.openai.base_url",
            "Removes any custom OpenAI base URL override for the official provider.",
        ));
    } else {
        changes.push(diff_line(
            &value,
            &format!("model_providers.{provider_id}.name"),
            &provider_name,
            "Adds a readable provider label for this provider.",
        ));
        changes.push(diff_line(
            &value,
            &format!("model_providers.{provider_id}.wire_api"),
            &wire_api,
            "Uses Codex's supported provider wire API.",
        ));
        changes.push(diff_line(
            &value,
            &format!("model_providers.{provider_id}.base_url"),
            &base_url,
            "Sets the managed provider endpoint.",
        ));
    }
    if primary_model.is_empty() {
        changes.push(diff_remove_line(
            &value,
            "model",
            "No model override is required when the profile has no selected model.",
        ));
    } else {
        changes.push(diff_line(
            &value,
            "model",
            primary_model,
            "Sets Codex to the selected model.",
        ));
    }
    changes.extend(codex_preserved_auth_repair_diff_lines(&value));
    let review_model = effective_profile_review_model(
        &profile.app,
        profile.review_model.as_deref(),
        primary_model,
    );
    changes.push(match review_model {
        Some(model) => diff_line(
            &value,
            "review_model",
            &model,
            "Sets the Codex model used for code review.",
        ),
        None => diff_remove_line(
            &value,
            "review_model",
            "Removes the Codex review model because the profile has no primary model to follow.",
        ),
    });
    Ok(NativeConfigPreview {
        tool: "codex".to_string(),
        path: display_path,
        status,
        write_enabled: true,
        changes,
        warnings,
        content: None,
    })
}

fn codex_preserved_auth_repair_diff_lines(root: &toml::Value) -> Vec<NativeConfigDiffLine> {
    vec![
        diff_remove_line(
            root,
            "auth.OPENAI_API_KEY",
            "Removes a legacy API-key mirror from Codex config.toml without touching auth.json.",
        ),
        diff_remove_line(
            root,
            "auth.api_key",
            "Removes a legacy API-key mirror from Codex config.toml without touching auth.json.",
        ),
        diff_remove_line(
            root,
            "env.OPENAI_API_KEY",
            "Removes a legacy environment-style API key from Codex config.toml.",
        ),
    ]
}

pub(in crate::core::profile) fn auth_json_path(
    paths: &crate::core::app_paths::AppPaths,
) -> PathBuf {
    paths.home_dir.join(".codex").join("auth.json")
}

pub(in crate::core::profile) fn read_auth_json(
    paths: &crate::core::app_paths::AppPaths,
) -> Result<serde_json::Value, String> {
    let path = auth_json_path(paths);
    let content = fs::read_to_string(&path).map_err(|err| {
        format!(
            "Codex auth.json could not be read at {}: {err}",
            display_path(&path)
        )
    })?;
    serde_json::from_str(&content)
        .map_err(|err| format!("Codex auth.json is not valid JSON: {err}"))
}

fn parse_auth_json(content: &str) -> Result<serde_json::Value, String> {
    let value = if content.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(content)
            .map_err(|err| format!("Existing Codex auth.json could not be parsed: {err}"))?
    };
    if !value.is_object() {
        return Err("Existing Codex auth.json must contain a JSON object.".to_string());
    }
    Ok(value)
}

fn render_auth_json(value: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|content| format!("{content}\n"))
        .map_err(|err| format!("Generated Codex auth.json is invalid: {err}"))
}

pub(in crate::core::profile) fn auth_json_content_with_api_key(
    current: &str,
    api_key: &str,
) -> Result<String, String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err("Codex auth.json requires a non-empty API key.".to_string());
    }
    let mut value = parse_auth_json(current)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Existing Codex auth.json must contain a JSON object.".to_string())?;
    object.insert(
        "auth_mode".to_string(),
        serde_json::Value::String("apikey".to_string()),
    );
    object.insert(
        "experimental_bearer_token".to_string(),
        serde_json::Value::String(api_key.to_string()),
    );
    object.remove("OPENAI_API_KEY");
    object.remove("openai_api_key");
    object.remove("api_key");
    render_auth_json(&value)
}

pub(in crate::core::profile) fn official_auth_json_content(
    current: &str,
) -> Result<Option<String>, String> {
    if current.trim().is_empty() {
        return Ok(None);
    }
    let mut value = parse_auth_json(current)?;
    if !auth_json_has_chatgpt_markers(&value) {
        return Ok(None);
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Existing Codex auth.json must contain a JSON object.".to_string())?;
    object.insert(
        "auth_mode".to_string(),
        serde_json::Value::String("chatgpt".to_string()),
    );
    object.remove("OPENAI_API_KEY");
    object.remove("openai_api_key");
    object.remove("api_key");
    object.remove("experimental_bearer_token");
    render_auth_json(&value).map(Some)
}

pub(in crate::core::profile) fn verify_auth_json_write(
    path: &Path,
    expected: &str,
) -> Result<bool, String> {
    Ok(fs::read_to_string(path).map_err(|err| err.to_string())? == expected)
}

pub(in crate::core::profile) fn detect_native_profile_with_auth(
    value: &toml::Value,
    auth: Option<&serde_json::Value>,
) -> Option<DetectedNativeProfile> {
    let provider_id = read_toml_string(value, "model_provider")?;
    if provider_id == "codestudio-local" {
        return None;
    }
    let base_url = toml_lookup(value, &format!("model_providers.{provider_id}.base_url"))?
        .as_str()?
        .trim();
    if base_url.is_empty() || looks_like_local_gateway_url(base_url) {
        return None;
    }
    let api_key = auth.and_then(auth_api_key_from_value)?;
    let wire_api = toml_lookup(value, &format!("model_providers.{provider_id}.wire_api"))
        .and_then(|item| item.as_str())
        .unwrap_or("responses");
    let protocol = protocol_for_wire_api(wire_api)?;
    let provider = toml_lookup(value, &format!("model_providers.{provider_id}.name"))
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or(provider_id.as_str());
    Some(DetectedNativeProfile {
        app: "codex".to_string(),
        provider: provider.to_string(),
        protocol: protocol.to_string(),
        model: read_toml_string(value, "model")
            .and_then(|model| native_optional_model(&model))
            .unwrap_or_default(),
        review_model: normalize_profile_review_model(
            "codex",
            read_toml_string(value, "review_model").as_deref(),
        ),
        base_url: base_url.to_string(),
        api_key,
    })
}

fn protocol_for_wire_api(value: &str) -> Option<&'static str> {
    match value.trim() {
        "responses" => Some(PROTOCOL_OPENAI_RESPONSES),
        "chat" => Some(PROTOCOL_OPENAI_CHAT_COMPLETIONS),
        _ => None,
    }
}

pub(in crate::core::profile) fn config_matches_profile_with_auth(
    value: &toml::Value,
    auth: Option<&serde_json::Value>,
    profile: &ProfileDraft,
    secret_match: SecretMatchMode,
) -> bool {
    if !is_codex_family_app(&profile.app) || profile.mode != ProviderApplyMode::Config {
        return false;
    }
    if provider_is_official(&profile.provider) {
        return official_config_matches_profile(value, profile);
    }
    let Some(provider_id) = active_provider_id_for_profile(value, profile) else {
        return false;
    };
    let model_matches = if profile.model.trim().is_empty() {
        read_toml_string(value, "model").is_none()
    } else {
        read_toml_string(value, "model").as_deref() == Some(profile.model.trim())
    };
    let Ok(wire_api) = wire_api_for_protocol(&profile.protocol) else {
        return false;
    };
    let token_matches = auth
        .and_then(auth_api_key_from_value)
        .map(|token| profile_api_key_matches_config(profile, &token, secret_match))
        .unwrap_or(false);
    read_toml_string(value, "model_provider").as_deref() == Some(provider_id.as_str())
        && model_matches
        && review_model_matches_profile(value, profile, false)
        && token_matches
        && toml_lookup(value, &format!("model_providers.{provider_id}.base_url"))
            .and_then(|item| item.as_str())
            .map(|base_url| {
                profile_runtime_base_url_matches(&profile.protocol, base_url, &profile.base_url)
            })
            .unwrap_or(false)
        && toml_lookup(value, &format!("model_providers.{provider_id}.wire_api"))
            .and_then(|item| item.as_str())
            == Some(wire_api)
        && managed_provider_auth_matches_legacy_or_current(value, &provider_id)
}

pub(in crate::core::profile) fn official_config_matches_profile(
    value: &toml::Value,
    profile: &ProfileDraft,
) -> bool {
    let provider_matches = read_toml_string(value, "model_provider")
        .map(|provider| provider == "openai")
        .unwrap_or(true);
    let model_matches = profile.model.trim().is_empty()
        || read_toml_string(value, "model").as_deref() == Some(profile.model.trim());
    let base_url_is_absent = toml_lookup(value, "model_providers.openai.base_url")
        .and_then(|item| item.as_str())
        .map(|base_url| base_url.trim().is_empty())
        .unwrap_or(true);
    let auth_matches = toml_lookup(value, "model_providers.openai.requires_openai_auth").is_none()
        || managed_provider_auth_matches_legacy_or_current(value, "openai");
    provider_matches
        && model_matches
        && review_model_matches_profile(value, profile, true)
        && base_url_is_absent
        && auth_matches
}

fn review_model_matches_profile(
    value: &toml::Value,
    profile: &ProfileDraft,
    unspecified_matches_any: bool,
) -> bool {
    let expected_model = if profile.mode == ProviderApplyMode::Gateway {
        gateway_config_model_for_profile(profile)
    } else {
        profile.model.trim()
    };
    let expected = effective_profile_review_model(
        &profile.app,
        profile.review_model.as_deref(),
        expected_model,
    );
    if expected.is_none() && unspecified_matches_any {
        return true;
    }
    effective_profile_review_model(
        "codex",
        read_toml_string(value, "review_model").as_deref(),
        read_toml_string(value, "model")
            .as_deref()
            .unwrap_or_default(),
    ) == expected
}

pub(in crate::core::profile) fn managed_provider_auth_contract_matches(
    value: &toml::Value,
    provider_id: &str,
) -> bool {
    toml_lookup(
        value,
        &format!("model_providers.{provider_id}.requires_openai_auth"),
    )
    .and_then(|item| item.as_bool())
        == Some(false)
        && toml_lookup(
            value,
            &format!(
                "model_providers.{provider_id}.http_headers.{CODEX_ACTOR_AUTHORIZATION_HEADER}"
            ),
        )
        .and_then(|item| item.as_str())
            == Some(CODEX_ACTOR_AUTHORIZATION_VALUE)
}

fn managed_provider_auth_matches_legacy_or_current(value: &toml::Value, provider_id: &str) -> bool {
    managed_provider_auth_contract_matches(value, provider_id)
        || toml_lookup(
            value,
            &format!("model_providers.{provider_id}.requires_openai_auth"),
        )
        .and_then(|item| item.as_bool())
            == Some(true)
}

fn active_provider_id_for_profile(value: &toml::Value, profile: &ProfileDraft) -> Option<String> {
    let active_provider = read_toml_string(value, "model_provider")?;
    let managed_provider = provider_id_for_profile(profile);
    if active_provider == managed_provider {
        return Some(active_provider);
    }
    let base_url_matches = toml_lookup(
        value,
        &format!("model_providers.{active_provider}.base_url"),
    )
    .and_then(|item| item.as_str())
    .map(str::trim)
    .map(|base_url| {
        profile_runtime_base_url_matches(&profile.protocol, base_url, &profile.base_url)
    })
    .unwrap_or(false);
    let wire_api = wire_api_for_protocol(&profile.protocol).ok()?;
    let wire_api_matches = toml_lookup(
        value,
        &format!("model_providers.{active_provider}.wire_api"),
    )
    .and_then(|item| item.as_str())
        == Some(wire_api);
    (base_url_matches && wire_api_matches).then_some(active_provider)
}

pub(in crate::core::profile) fn verify_config(
    path: &Path,
    profile: &ProfileDraft,
    mode: ProviderApplyMode,
) -> Result<bool, String> {
    let value: toml::Value =
        toml::from_str(&fs::read_to_string(path).map_err(|err| err.to_string())?)
            .map_err(|err| err.to_string())?;
    if mode == ProviderApplyMode::Gateway {
        let client = gateway::client_config_for_tool(&profile.app)?;
        let provider_id = client.provider_id;
        let model = gateway_config_model_for_profile(profile);
        return Ok(
            read_toml_string(&value, "cli_auth_credentials_store").as_deref() == Some("file")
                && read_toml_string(&value, "model_provider").as_deref()
                    == Some(provider_id.as_str())
                && read_toml_string(&value, "model").as_deref() == Some(model)
                && review_model_matches_profile(&value, profile, false)
                && toml_lookup(&value, &format!("model_providers.{provider_id}.base_url"))
                    .and_then(|item| item.as_str())
                    == Some(client.base_url.as_str())
                && managed_provider_auth_contract_matches(&value, &provider_id),
        );
    }
    if provider_is_official(&profile.provider) {
        let model_matches = profile
            .model
            .trim()
            .is_empty()
            .then(|| read_toml_string(&value, "model").is_none())
            .unwrap_or_else(|| {
                read_toml_string(&value, "model").as_deref() == Some(profile.model.trim())
            });
        return Ok(
            read_toml_string(&value, "cli_auth_credentials_store").as_deref() == Some("file")
                && read_toml_string(&value, "model_provider").as_deref() == Some("openai")
                && model_matches
                && review_model_matches_profile(&value, profile, false)
                && toml_lookup(&value, "model_providers.openai.base_url").is_none()
                && managed_provider_auth_contract_matches(&value, "openai"),
        );
    }
    let provider_id = provider_id_for_profile(profile);
    let wire_api = wire_api_for_protocol(&profile.protocol)?;
    let model_matches = if profile.model.trim().is_empty() {
        read_toml_string(&value, "model").is_none()
    } else {
        read_toml_string(&value, "model").as_deref() == Some(profile.model.trim())
    };
    Ok(
        read_toml_string(&value, "cli_auth_credentials_store").as_deref() == Some("file")
            && read_toml_string(&value, "model_provider").as_deref() == Some(provider_id.as_str())
            && model_matches
            && review_model_matches_profile(&value, profile, false)
            && toml_lookup(&value, &format!("model_providers.{provider_id}.wire_api"))
                .and_then(|item| item.as_str())
                == Some(wire_api)
            && toml_lookup(&value, &format!("model_providers.{provider_id}.base_url"))
                .and_then(|item| item.as_str())
                == Some(
                    profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url)
                        .as_str(),
                )
            && managed_provider_auth_contract_matches(&value, &provider_id),
    )
}

pub(in crate::core::profile) fn codex_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| format!("Existing Codex config could not be parsed: {err}"))?;
    normalize_model_providers_table(&mut document);
    remove_legacy_managed_direct_providers(&mut document);
    let provider_id = client.provider_id;
    let model = gateway_config_model_for_profile(profile);
    document["cli_auth_credentials_store"] = toml_edit::value("file");
    document["model_provider"] = toml_edit::value(provider_id.clone());
    document["model"] = toml_edit::value(model);
    set_review_model(&mut document, profile, model);
    remove_provider_entry(&mut document, &provider_id);
    document["model_providers"][&provider_id] = toml_edit::Item::Table(toml_edit::Table::new());
    document["model_providers"][&provider_id]["name"] = toml_edit::value(client.provider_name);
    document["model_providers"][&provider_id]["wire_api"] = toml_edit::value("responses");
    document["model_providers"][&provider_id]["base_url"] = toml_edit::value(client.base_url);
    set_managed_provider_auth(&mut document, &provider_id);
    repair_codex_preserved_auth_config(&mut document);
    render_valid_document(document)
}

pub(in crate::core::profile) fn codex_direct_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    if provider_is_official(&profile.provider) {
        return codex_official_config_content(current, profile);
    }
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| format!("Existing Codex config could not be parsed: {err}"))?;
    normalize_model_providers_table(&mut document);
    remove_legacy_managed_direct_providers(&mut document);
    let provider_id = provider_id_for_profile(profile);
    let model = profile.model.trim();
    document["cli_auth_credentials_store"] = toml_edit::value("file");
    document["model_provider"] = toml_edit::value(provider_id.clone());
    if model.is_empty() {
        document.as_table_mut().remove("model");
    } else {
        document["model"] = toml_edit::value(model);
    }
    set_review_model(&mut document, profile, model);
    remove_provider_entry(&mut document, &provider_id);
    document["model_providers"][&provider_id] = toml_edit::Item::Table(toml_edit::Table::new());
    document["model_providers"][&provider_id]["name"] = toml_edit::value(profile.provider.trim());
    document["model_providers"][&provider_id]["wire_api"] =
        toml_edit::value(wire_api_for_protocol(&profile.protocol)?);
    document["model_providers"][&provider_id]["base_url"] = toml_edit::value(
        profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url),
    );
    set_managed_provider_auth(&mut document, &provider_id);
    repair_codex_preserved_auth_config(&mut document);
    render_valid_document(document)
}

pub(in crate::core::profile) fn codex_official_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| format!("Existing Codex config could not be parsed: {err}"))?;
    let provider_id = "openai";
    normalize_model_providers_table(&mut document);
    document["cli_auth_credentials_store"] = toml_edit::value("file");
    document["model_provider"] = toml_edit::value(provider_id);
    if profile.model.trim().is_empty() {
        document.remove("model");
    } else {
        document["model"] = toml_edit::value(profile.model.trim());
    }
    set_review_model(&mut document, profile, profile.model.trim());
    remove_provider_entry(&mut document, provider_id);
    document["model_providers"][provider_id] = toml_edit::Item::Table(toml_edit::Table::new());
    set_managed_provider_auth(&mut document, provider_id);
    repair_codex_preserved_auth_config(&mut document);
    render_valid_document(document)
}

fn render_valid_document(document: toml_edit::DocumentMut) -> Result<String, String> {
    let updated = document.to_string();
    toml::from_str::<toml::Value>(&updated)
        .map_err(|err| format!("Generated Codex config is invalid: {err}"))?;
    Ok(updated)
}

fn normalize_model_providers_table(document: &mut toml_edit::DocumentMut) {
    let table = document
        .as_table_mut()
        .remove("model_providers")
        .map(|item| {
            item.into_table().unwrap_or_else(|item| {
                item.as_table_like()
                    .map(table_like_to_table)
                    .unwrap_or_default()
            })
        })
        .unwrap_or_default();
    document["model_providers"] = toml_edit::Item::Table(table);
}

fn table_like_to_table(table_like: &dyn toml_edit::TableLike) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    for (key, value) in table_like.iter() {
        table[key] = value.clone();
    }
    table
}

fn remove_legacy_managed_direct_providers(document: &mut toml_edit::DocumentMut) {
    let Some(table) = document
        .get_mut("model_providers")
        .and_then(|item| item.as_table_like_mut())
    else {
        return;
    };
    let keys = table
        .iter()
        .filter_map(|(key, _)| key.starts_with("codestudio-").then(|| key.to_string()))
        .collect::<Vec<_>>();
    for key in keys {
        table.remove(&key);
    }
}

fn remove_provider_entry(document: &mut toml_edit::DocumentMut, provider_id: &str) {
    if let Some(table) = document
        .get_mut("model_providers")
        .and_then(|item| item.as_table_like_mut())
    {
        table.remove(provider_id);
    }
}

fn set_managed_provider_auth(document: &mut toml_edit::DocumentMut, provider_id: &str) {
    document["model_providers"][provider_id]["requires_openai_auth"] = toml_edit::value(false);
    let mut headers = toml_edit::InlineTable::new();
    let header_key = format!("\"{CODEX_ACTOR_AUTHORIZATION_HEADER}\"")
        .parse::<toml_edit::Key>()
        .expect("Codex actor-authorization header key should be valid TOML");
    headers.insert_formatted(
        &header_key,
        toml_edit::Value::from(CODEX_ACTOR_AUTHORIZATION_VALUE),
    );
    document["model_providers"][provider_id]["http_headers"] = toml_edit::value(headers);
}

fn set_review_model(
    document: &mut toml_edit::DocumentMut,
    profile: &ProfileDraft,
    primary_model: &str,
) {
    match effective_profile_review_model(
        &profile.app,
        profile.review_model.as_deref(),
        primary_model,
    ) {
        Some(review_model) => document["review_model"] = toml_edit::value(review_model),
        None => {
            document.as_table_mut().remove("review_model");
        }
    }
}

pub(in crate::core::profile) fn repair_codex_preserved_auth_config(
    document: &mut toml_edit::DocumentMut,
) {
    remove_legacy_key_from_table(document, "auth", &["OPENAI_API_KEY", "api_key"]);
    remove_legacy_key_from_table(document, "env", &["OPENAI_API_KEY"]);
}

fn remove_legacy_key_from_table(
    document: &mut toml_edit::DocumentMut,
    table_name: &str,
    keys: &[&str],
) {
    let should_remove = {
        let Some(table) = document
            .get_mut(table_name)
            .and_then(|item| item.as_table_like_mut())
        else {
            return;
        };
        for key in keys {
            table.remove(key);
        }
        table.is_empty()
    };
    if should_remove {
        document.as_table_mut().remove(table_name);
    }
}

#[cfg(test)]
pub(in crate::core::profile) fn detect_codex_native_profile(
    value: &toml::Value,
) -> Option<DetectedNativeProfile> {
    detect_native_profile_with_auth(value, None)
}
#[cfg(test)]
pub(in crate::core::profile) use detect_native_profile_with_auth as detect_codex_native_profile_with_auth;
#[cfg(test)]
pub(in crate::core::profile) fn codex_direct_config_matches_profile(
    value: &toml::Value,
    auth: Option<&serde_json::Value>,
    profile: &ProfileDraft,
) -> bool {
    config_matches_profile_with_auth(value, auth, profile, SecretMatchMode::ExactKeychain)
}
#[cfg(test)]
pub(in crate::core::profile) fn codex_direct_config_matches_profile_without_keychain(
    value: &toml::Value,
    auth: Option<&serde_json::Value>,
    profile: &ProfileDraft,
) -> bool {
    config_matches_profile_with_auth(value, auth, profile, SecretMatchMode::KeychainReference)
}
#[cfg(test)]
pub(in crate::core::profile) use official_config_matches_profile as codex_official_config_matches_profile;
#[cfg(test)]
pub(in crate::core::profile) fn verify_codex_direct_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    verify_config(path, profile, ProviderApplyMode::Config)
}
#[cfg(test)]
pub(in crate::core::profile) fn verify_codex_native_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    verify_config(path, profile, ProviderApplyMode::Gateway)
}
#[cfg(test)]
pub(in crate::core::profile) use auth_json_content_with_api_key as codex_auth_json_content_with_api_key;
#[cfg(test)]
pub(in crate::core::profile) use auth_json_path as codex_auth_json_path;
#[cfg(test)]
pub(in crate::core::profile) use official_auth_json_content as codex_official_auth_json_content;
