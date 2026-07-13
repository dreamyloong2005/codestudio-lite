use super::super::*;
use super::NativeProfileAdapter;

pub(in crate::core::profile) const MANAGED_PROVIDER_ID: &str = "codestudio";
pub(in crate::core::profile) static PI_ADAPTER: PiAdapter = PiAdapter;

pub(in crate::core::profile) struct PiAdapter;

impl NativeProfileAdapter for PiAdapter {
    fn target(&self, paths: &crate::core::app_paths::AppPaths) -> PathBuf {
        paths.home_dir.join(".pi").join("agent").join("models.json")
    }

    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                pi_official_config_content(current)
            }
            ProviderApplyMode::Config => pi_config_content(current, profile),
            ProviderApplyMode::Gateway => pi_gateway_config_content(current, profile),
        }
    }

    fn cleanup_gateway(&self, current: &str) -> Result<String, String> {
        pi_gateway_cleanup_config_content(current, "pi")
    }

    fn render_preview(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                pi_official_config_content(current)
            }
            ProviderApplyMode::Config => {
                pi_config_content_with_api_key(current, profile, secret_preview(profile))
            }
            ProviderApplyMode::Gateway => pi_gateway_config_content(current, profile),
        }
    }

    fn inspect(&self, current: &str) -> Result<Option<DetectedNativeProfile>, String> {
        let value = parse_json5_or_empty(current, "Pi Agent models")?;
        Ok(detect_pi_native_profile(&value))
    }

    fn matches(
        &self,
        current: &str,
        profile: &ProfileDraft,
        secret_match: SecretMatchMode,
    ) -> Result<bool, String> {
        let value = parse_json5_or_empty(current, "Pi Agent models")?;
        Ok(pi_config_matches_profile_with_secret_match(
            &value,
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
            ProviderApplyMode::Config => verify_pi_config(path, profile),
            ProviderApplyMode::Gateway => verify_pi_gateway_config(path, profile),
        }
    }

    fn preview(
        &self,
        profile: &ProfileDraft,
        path: PathBuf,
        display_path: String,
        mode: ProviderApplyMode,
    ) -> Result<NativeConfigPreview, String> {
        let is_official = provider_is_official(&profile.provider);
        let mut warnings = match (mode, is_official) {
            (ProviderApplyMode::Config, true) => vec![
                "Official provider removes CodeStudio Lite managed Pi Agent provider entries."
                    .to_string(),
            ],
            (ProviderApplyMode::Config, false) => vec![
                "Pi Agent custom providers are written to ~/.pi/agent/models.json.".to_string(),
                "Existing JSON comments are not preserved when CodeStudio Lite writes the file."
                    .to_string(),
                "Open /model in Pi after applying to select the managed provider model.".to_string(),
            ],
            (ProviderApplyMode::Gateway, _) => vec![
                "Gateway profiles write Pi Agent provider settings to the tool-scoped local gateway URL."
                    .to_string(),
                "Existing JSON comments are not preserved when CodeStudio Lite writes the file."
                    .to_string(),
                "Open /model in Pi after applying to select the managed provider model.".to_string(),
                "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running."
                    .to_string(),
                "Real upstream Provider API keys stay in the system keychain and are used by the local gateway."
                    .to_string(),
            ],
        };
        let (json, status) = read_json_preview(&path, "Pi Agent models", &mut warnings)?;
        let provider_id = MANAGED_PROVIDER_ID;
        let changes = if is_official {
            vec![
                diff_value_line(
                    "providers.codestudio".to_string(),
                    Some("managed provider entries".to_string()),
                    None,
                    "Removes CodeStudio Lite managed Pi Agent provider entries.",
                ),
                json_diff_remove_line(
                    &json,
                    &["providers", provider_id],
                    "Deletes the managed CodeStudio Lite Pi provider.",
                ),
            ]
        } else if mode == ProviderApplyMode::Gateway {
            let client = gateway::client_config_for_tool("pi")?;
            let model = gateway_config_model_for_profile(profile);
            vec![
                json_diff_line(
                    &json,
                    &["providers", provider_id, "baseUrl"],
                    &client.base_url,
                    "Points Pi Agent at the tool-scoped CodeStudio Lite Local Gateway.",
                ),
                json_diff_line(
                    &json,
                    &["providers", provider_id, "api"],
                    "openai-completions",
                    "Uses OpenAI Chat Completions against the Local Gateway.",
                ),
                json_diff_line(
                    &json,
                    &["providers", provider_id, "apiKey"],
                    &client.token_preview,
                    "Stores only the local CodeStudio token, not the real upstream Provider API key.",
                ),
                json_diff_line(
                    &json,
                    &["providers", provider_id, "models"],
                    &format!("[{model}]"),
                    "Registers the virtual gateway model under the managed Pi provider.",
                ),
            ]
        } else {
            let runtime_base_url =
                profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url);
            let api = pi_api_for_protocol(&profile.protocol)?;
            let model = profile_model(profile).unwrap_or(GATEWAY_FALLBACK_MODEL);
            let mut changes = vec![
                json_diff_line(
                    &json,
                    &["providers", provider_id, "baseUrl"],
                    &runtime_base_url,
                    "Points Pi Agent at the selected upstream Provider Base URL.",
                ),
                json_diff_line(
                    &json,
                    &["providers", provider_id, "api"],
                    api,
                    "Selects the Pi Agent API adapter for this provider.",
                ),
                json_diff_line(
                    &json,
                    &["providers", provider_id, "apiKey"],
                    secret_preview(profile),
                    "Stores the selected Provider API key for Pi Agent.",
                ),
                json_diff_line(
                    &json,
                    &["providers", provider_id, "models"],
                    &format!("[{model}]"),
                    "Registers the selected model under the managed Pi provider.",
                ),
            ];
            if matches!(api, "openai-completions" | "openai-responses") {
                changes.push(json_diff_line(
                    &json,
                    &["providers", provider_id, "compat", "supportsDeveloperRole"],
                    "false",
                    "Uses system-role prompts for broader OpenAI-compatible endpoint support.",
                ));
                changes.push(json_diff_line(
                    &json,
                    &[
                        "providers",
                        provider_id,
                        "compat",
                        "supportsReasoningEffort",
                    ],
                    "false",
                    "Disables unsupported reasoning-effort parameters for compatible endpoints.",
                ));
            }
            changes
        };

        Ok(NativeConfigPreview {
            tool: "pi".to_string(),
            path: display_path,
            status,
            write_enabled: true,
            changes,
            warnings,
            content: None,
        })
    }
}

pub(in crate::core::profile) fn pi_api_for_protocol(
    protocol: &str,
) -> Result<&'static str, String> {
    match protocol {
        PROTOCOL_OPENAI_CHAT_COMPLETIONS => Ok("openai-completions"),
        PROTOCOL_OPENAI_RESPONSES => Ok("openai-responses"),
        PROTOCOL_ANTHROPIC_MESSAGES => Ok("anthropic-messages"),
        PROTOCOL_GOOGLE_GEMINI => Ok("google-generative-ai"),
        _ => Err(format!(
            "Pi Agent does not support {} in Config profiles.",
            protocol_display_name(protocol)
        )),
    }
}

pub(in crate::core::profile) fn protocol_for_pi_api(value: &str) -> Option<&'static str> {
    match value.trim() {
        "openai-completions" | "openai-chat-completions" => Some(PROTOCOL_OPENAI_CHAT_COMPLETIONS),
        "openai-responses" => Some(PROTOCOL_OPENAI_RESPONSES),
        "anthropic-messages" => Some(PROTOCOL_ANTHROPIC_MESSAGES),
        "google-generative-ai" | "google-gemini" => Some(PROTOCOL_GOOGLE_GEMINI),
        _ => None,
    }
}

pub(in crate::core::profile) fn pi_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    pi_config_content_with_api_key(current, profile, &api_key)
}

pub(in crate::core::profile) fn pi_config_content_with_api_key(
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
            PROTOCOL_GOOGLE_GEMINI,
        ],
    )?;
    let api = pi_api_for_protocol(&profile.protocol)?;
    let mut value = parse_json5_or_empty(current, "Pi Agent models")?;
    let runtime_base_url =
        profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url);
    let provider_id = MANAGED_PROVIDER_ID;
    remove_json_managed_provider_entries(&mut value, &["providers"]);

    set_json_string_path(
        &mut value,
        &["providers", provider_id, "baseUrl"],
        &runtime_base_url,
    );
    set_json_string_path(&mut value, &["providers", provider_id, "api"], api);
    set_json_string_path(&mut value, &["providers", provider_id, "apiKey"], api_key);
    // Many OpenAI-compatible proxies reject developer-role prompts.
    if api == "openai-completions" || api == "openai-responses" {
        set_json_value_path(
            &mut value,
            &["providers", provider_id, "compat", "supportsDeveloperRole"],
            serde_json::Value::Bool(false),
        );
        set_json_value_path(
            &mut value,
            &[
                "providers",
                provider_id,
                "compat",
                "supportsReasoningEffort",
            ],
            serde_json::Value::Bool(false),
        );
    }

    let model_id = profile_model(profile).unwrap_or(GATEWAY_FALLBACK_MODEL);
    let model_name = if profile.name.trim().is_empty() {
        model_id.to_string()
    } else {
        profile.name.trim().to_string()
    };
    // Replace models array with a single managed model entry.
    let model_entry = serde_json::json!({
        "id": model_id,
        "name": model_name
    });
    set_json_value_path(
        &mut value,
        &["providers", provider_id, "models"],
        serde_json::Value::Array(vec![model_entry]),
    );

    render_json_config(value, "Pi Agent models")
}

pub(in crate::core::profile) fn pi_official_config_content(
    current: &str,
) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Pi Agent models")?;
    remove_json_managed_provider_entries(&mut value, &["providers"]);
    render_json_config(value, "Pi Agent models")
}

pub(in crate::core::profile) fn pi_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "Pi Agent models")?;
    let provider_id = MANAGED_PROVIDER_ID;
    let model = gateway_config_model_for_profile(profile);
    remove_json_managed_provider_entries(&mut value, &["providers"]);

    set_json_string_path(
        &mut value,
        &["providers", provider_id, "baseUrl"],
        &client.base_url,
    );
    set_json_string_path(
        &mut value,
        &["providers", provider_id, "api"],
        "openai-completions",
    );
    set_json_string_path(
        &mut value,
        &["providers", provider_id, "apiKey"],
        &client.token,
    );
    set_json_value_path(
        &mut value,
        &["providers", provider_id, "compat", "supportsDeveloperRole"],
        serde_json::Value::Bool(false),
    );
    set_json_value_path(
        &mut value,
        &["providers", provider_id, "models"],
        serde_json::json!([{
            "id": model,
            "name": format!("CodeStudio Lite ({})", client.provider_name)
        }]),
    );

    render_json_config(value, "Pi Agent models")
}

pub(in crate::core::profile) fn pi_gateway_cleanup_config_content(
    current: &str,
    tool_id: &str,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_json5_or_empty(current, "Pi Agent models")?;
    let provider_id = MANAGED_PROVIDER_ID;
    let base_url =
        json_string_lookup(&value, &["providers", provider_id, "baseUrl"]).unwrap_or_default();
    let api_key =
        json_string_lookup(&value, &["providers", provider_id, "apiKey"]).unwrap_or_default();
    let is_gateway = base_url.trim() == client.base_url.trim()
        || looks_like_local_gateway_url(&base_url)
        || looks_like_local_gateway_token(&api_key);
    if is_gateway {
        remove_json_path(&mut value, &["providers", provider_id]);
    }
    render_json_config(value, "Pi Agent models")
}

pub(in crate::core::profile) fn detect_pi_native_profile(
    value: &serde_json::Value,
) -> Option<DetectedNativeProfile> {
    let providers = value.get("providers")?.as_object()?;
    for (provider_id, provider) in providers {
        let Some(base_url) = provider
            .get("baseUrl")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
        else {
            continue;
        };
        if looks_like_local_gateway_url(base_url) {
            continue;
        }
        let Some(api_key) = provider
            .get("apiKey")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .filter(|item| !looks_like_local_gateway_token(item))
        else {
            continue;
        };
        let api = provider
            .get("api")
            .and_then(|item| item.as_str())
            .unwrap_or("openai-completions");
        let Some(protocol) = protocol_for_pi_api(api) else {
            continue;
        };
        let model = provider
            .get("models")
            .and_then(|item| item.as_array())
            .and_then(|models| models.first())
            .and_then(|model| model.get("id"))
            .and_then(|item| item.as_str())
            .and_then(|model| native_optional_model(model))
            .unwrap_or_default();
        return Some(DetectedNativeProfile {
            app: "pi".to_string(),
            provider: if managed_json_provider_key(provider_id) {
                provider_slug_from_base_url(base_url).unwrap_or_else(|| "custom".to_string())
            } else {
                provider_id.to_string()
            },
            protocol: protocol.to_string(),
            model,
            review_model: None,
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
        });
    }
    None
}

pub(in crate::core::profile) fn pi_config_matches_profile(
    value: &serde_json::Value,
    profile: &ProfileDraft,
) -> bool {
    pi_config_matches_profile_with_secret_match(value, profile, SecretMatchMode::ExactKeychain)
}

pub(in crate::core::profile) fn pi_config_matches_profile_with_secret_match(
    value: &serde_json::Value,
    profile: &ProfileDraft,
    secret_match: SecretMatchMode,
) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "pi"
            && profile.mode == ProviderApplyMode::Config
            && !pi_config_has_managed_provider(value);
    }
    if canonical_profile_app(&profile.app) != "pi" || profile.mode != ProviderApplyMode::Config {
        return false;
    }
    let Ok(api) = pi_api_for_protocol(&profile.protocol) else {
        return false;
    };
    let provider_id = MANAGED_PROVIDER_ID;
    let base_url = json_string_lookup(value, &["providers", provider_id, "baseUrl"]);
    let token_matches = json_string_lookup(value, &["providers", provider_id, "apiKey"])
        .map(|token| profile_api_key_matches_config(profile, &token, secret_match))
        .unwrap_or(false);
    let api_matches =
        json_string_lookup(value, &["providers", provider_id, "api"]).as_deref() == Some(api);
    let model_matches = match profile_model(profile) {
        Some(model) => value
            .get("providers")
            .and_then(|item| item.get(provider_id))
            .and_then(|item| item.get("models"))
            .and_then(|item| item.as_array())
            .map(|models| {
                models.iter().any(|entry| {
                    entry
                        .get("id")
                        .and_then(|item| item.as_str())
                        .map(|id| id == model)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false),
        None => true,
    };

    base_url
        .as_deref()
        .map(|base_url| {
            profile_runtime_base_url_matches(&profile.protocol, base_url, &profile.base_url)
        })
        .unwrap_or(false)
        && token_matches
        && api_matches
        && model_matches
}

pub(in crate::core::profile) fn pi_config_has_managed_provider(value: &serde_json::Value) -> bool {
    json_object_keys(value, &["providers"])
        .into_iter()
        .any(|key| managed_json_provider_key(&key))
}

pub(in crate::core::profile) fn verify_pi_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Pi Agent models")?;
    Ok(pi_config_matches_profile(&value, profile))
}

pub(in crate::core::profile) fn verify_pi_gateway_config(
    path: &Path,
    profile: &ProfileDraft,
) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Pi Agent models")?;
    let provider_id = MANAGED_PROVIDER_ID;
    let model = gateway_config_model_for_profile(profile);
    let model_matches = value
        .get("providers")
        .and_then(|item| item.get(provider_id))
        .and_then(|item| item.get("models"))
        .and_then(|item| item.as_array())
        .map(|models| {
            models.iter().any(|entry| {
                entry
                    .get("id")
                    .and_then(|item| item.as_str())
                    .map(|id| id == model)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    Ok(
        json_string_lookup(&value, &["providers", provider_id, "baseUrl"]).as_deref()
            == Some(client.base_url.as_str())
            && json_string_lookup(&value, &["providers", provider_id, "apiKey"]).as_deref()
                == Some(client.token.as_str())
            && json_string_lookup(&value, &["providers", provider_id, "api"]).as_deref()
                == Some("openai-completions")
            && model_matches,
    )
}
