use super::*;
use reqwest::blocking::Client;

pub fn test_profile_connection(
    request: TestProfileConnectionRequest,
) -> Result<TestProfileConnectionResult, String> {
    ensure_app_dirs()?;

    let app = canonical_profile_app(&normalize_token("Tool", &request.app)?);
    let provider = normalize_provider_token(&request.provider)?;
    let protocol = normalize_protocol(request.protocol.as_deref())?;
    let model = request.model.trim().to_string();
    let base_url = validate_base_url_for_provider(&provider, &request.base_url)?;
    let snapshot = detector::detect_environment()?;
    let mut checks = Vec::new();

    if let Some(tool) = snapshot
        .tools
        .iter()
        .find(|tool| canonical_profile_app(&tool.id) == app)
    {
        checks.push(ProfileConnectionCheck {
            id: "tool-install".to_string(),
            label: "Target tool".to_string(),
            status: if tool.install_state == InstallState::Installed {
                Severity::Ok
            } else {
                Severity::Warning
            },
            detail: tool
                .version
                .as_ref()
                .map(|version| format!("{} is installed: {version}", tool.name))
                .unwrap_or_else(|| {
                    tool.install_command
                        .as_ref()
                        .map(|command| {
                            format!("{} is missing. Suggested command: {command}", tool.name)
                        })
                        .unwrap_or_else(|| format!("{} is missing.", tool.name))
                }),
        });

        checks.push(ProfileConnectionCheck {
            id: "tool-config".to_string(),
            label: "Existing tool config".to_string(),
            status: if tool.config_state == ConfigState::Configured {
                Severity::Ok
            } else {
                Severity::Info
            },
            detail: tool
                .config_path
                .as_ref()
                .map(|path| format!("{} at {path}", format_config_state(&tool.config_state)))
                .unwrap_or_else(|| "No config path is known for this tool.".to_string()),
        });
    } else {
        checks.push(ProfileConnectionCheck {
            id: "tool-install".to_string(),
            label: "Target tool".to_string(),
            status: Severity::Error,
            detail: format!("Tool '{app}' is not in the registry."),
        });
    }

    checks.push(ProfileConnectionCheck {
        id: "base-url".to_string(),
        label: "Provider base URL".to_string(),
        status: if provider_is_official(&provider) {
            Severity::Info
        } else {
            Severity::Ok
        },
        detail: if provider_is_official(&provider) {
            "Official provider uses the target client's own login and default endpoint.".to_string()
        } else {
            base_url
        },
    });
    checks.push(ProfileConnectionCheck {
        id: "protocol".to_string(),
        label: "Protocol".to_string(),
        status: Severity::Ok,
        detail: format!(
            "Selected upstream API protocol: {}.",
            protocol_display_name(&protocol)
        ),
    });
    checks.push(ProfileConnectionCheck {
        id: "model".to_string(),
        label: "Model".to_string(),
        status: if model.is_empty() {
            Severity::Info
        } else {
            Severity::Ok
        },
        detail: if model.is_empty() {
            "Model is not specified.".to_string()
        } else {
            model
        },
    });
    checks.push(ProfileConnectionCheck {
        id: "credential".to_string(),
        label: "Credential".to_string(),
        status: credential_status(&provider, request.secret_provided),
        detail: if request
            .api_key
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        {
            "Provider API key is ready to be stored in the system keychain when this profile is saved.".to_string()
        } else {
            credential_detail(&provider, request.secret_provided)
        },
    });
    checks.push(ProfileConnectionCheck {
        id: "network".to_string(),
        label: "Provider ping".to_string(),
        status: Severity::Info,
        detail: "Network provider checks are not sent yet.".to_string(),
    });

    let status = aggregate_check_status(&checks);
    activity_log::append(
        status.clone(),
        format!("Ran profile connection checks for {app}/{provider}."),
    )?;

    Ok(TestProfileConnectionResult {
        generated_at: Utc::now().to_rfc3339(),
        status,
        checks,
    })
}

pub fn list_profile_models(
    request: ListProfileModelsRequest,
) -> Result<ListProfileModelsResult, String> {
    ensure_app_dirs()?;

    let app = canonical_profile_app(&normalize_token("Tool", &request.app)?);
    let provider = normalize_provider_token(&request.provider)?;
    if provider_is_official(&provider) {
        return Err(
            "Official provider uses the target client's own login; model listing is only available for API profiles."
                .to_string(),
        );
    }
    let mode = normalize_profile_mode(&provider, request.mode.as_ref())?;
    let protocol = normalize_protocol(request.protocol.as_deref())?;
    ensure_profile_protocol_supported_for_mode(&app, mode, &provider, &protocol)?;
    let base_url = validate_base_url_for_provider(&provider, &request.base_url)?;
    let api_key = resolve_model_list_api_key(&request, &provider)?;
    let url = profile_model_list_url(&protocol, &base_url);
    let payload = fetch_profile_model_list_payload(&protocol, &url, &api_key)?;
    let models = profile_model_options_from_payload(&protocol, &payload);

    activity_log::append(
        Severity::Ok,
        format!(
            "Fetched {} model option(s) for {app}/{provider}.",
            models.len()
        ),
    )?;

    Ok(ListProfileModelsResult {
        generated_at: Utc::now().to_rfc3339(),
        provider,
        protocol,
        base_url,
        models,
    })
}

fn resolve_model_list_api_key(
    request: &ListProfileModelsRequest,
    provider: &str,
) -> Result<String, String> {
    if !provider_requires_api_key(provider) {
        return Ok(String::new());
    }

    if let Some(api_key) = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(api_key.to_string());
    }

    let Some(profile_id) = request
        .profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err("Provider API key is required to fetch models.".to_string());
    };
    let profile = load_profile_by_id(profile_id)?;
    if !profile.provider.eq_ignore_ascii_case(provider) {
        return Err("Provider API key is required after changing Provider.".to_string());
    }
    let Some(auth_ref) = profile.auth_ref.as_deref() else {
        return Err("Stored Provider API key is missing for this profile.".to_string());
    };
    let api_key = credentials::load_keychain_secret(auth_ref)?;
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err("Stored Provider API key is empty.".to_string());
    }
    Ok(trimmed.to_string())
}

fn fetch_profile_model_list_payload(
    protocol: &str,
    url: &str,
    api_key: &str,
) -> Result<serde_json::Value, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| format!("Could not create provider model client: {err}"))?;
    let request = client.get(url).header("Accept", "application/json");
    let request = match protocol {
        PROTOCOL_ANTHROPIC_MESSAGES => request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
        PROTOCOL_GOOGLE_GEMINI => request.header("x-goog-api-key", api_key),
        _ => request.bearer_auth(api_key),
    };
    let response = request
        .send()
        .map_err(|err| format!("Could not fetch provider models: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|err| format!("Could not read provider model response: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "Provider model request failed with HTTP {}: {}",
            status.as_u16(),
            provider_error_summary(&text)
        ));
    }
    serde_json::from_str(&text)
        .map_err(|err| format!("Provider model response is not valid JSON: {err}"))
}

pub(in crate::core::profile) fn profile_model_list_url(protocol: &str, base_url: &str) -> String {
    let runtime_base_url = profile_runtime_base_url_for_protocol(protocol, base_url);
    let path = match protocol {
        PROTOCOL_OPENAI_CHAT_COMPLETIONS | PROTOCOL_OPENAI_RESPONSES => "/models",
        PROTOCOL_ANTHROPIC_MESSAGES => "/models",
        PROTOCOL_GOOGLE_GEMINI => "/models",
        _ => "/models",
    };
    append_profile_api_path(&runtime_base_url, path)
}

fn append_profile_api_path(base_url: &str, path: &str) -> String {
    let trimmed_base = base_url.trim_end_matches('/');
    let clean_path = format!("/{}", path.trim().trim_start_matches('/'));
    let fallback = || format!("{trimmed_base}{clean_path}");
    let Ok(mut parsed) = Url::parse(trimmed_base) else {
        return fallback();
    };
    if parsed.scheme().is_empty() || parsed.host_str().is_none() {
        return fallback();
    }
    let base_path = parsed.path().trim_end_matches('/');
    let next_path = if base_path.is_empty() || base_path == "/" {
        clean_path
    } else {
        format!("{base_path}{clean_path}")
    };
    parsed.set_path(&next_path);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

pub(in crate::core::profile) fn profile_model_options_from_payload(
    protocol: &str,
    payload: &serde_json::Value,
) -> Vec<ProfileModelOption> {
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    for array in profile_model_arrays(payload) {
        for item in array {
            if let Some(model) = profile_model_option_from_value(protocol, item) {
                if seen.insert(model.id.clone()) {
                    models.push(model);
                }
            }
        }
    }
    models
}

fn profile_model_arrays(payload: &serde_json::Value) -> Vec<&Vec<serde_json::Value>> {
    let mut arrays = Vec::new();
    if let Some(array) = payload.as_array() {
        arrays.push(array);
    }
    for key in ["data", "models", "items"] {
        if let Some(array) = payload.get(key).and_then(|value| value.as_array()) {
            arrays.push(array);
        }
    }
    arrays
}

fn profile_model_option_from_value(
    protocol: &str,
    item: &serde_json::Value,
) -> Option<ProfileModelOption> {
    if let Some(value) = item
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(ProfileModelOption {
            id: normalize_profile_model_id(protocol, value),
            name: None,
            owned_by: None,
            supports_1m: false,
        });
    }
    if protocol == PROTOCOL_GOOGLE_GEMINI && !gemini_model_supports_generation(item) {
        return None;
    }
    let raw_id = json_string_any(item, &["id", "name", "model", "value"])?;
    let id = normalize_profile_model_id(protocol, &raw_id);
    let name = json_string_any(
        item,
        &["display_name", "displayName", "label", "description"],
    )
    .filter(|value| value != &id && value != &raw_id);
    let owned_by = json_string_any(item, &["owned_by", "ownedBy", "owner"]);
    let supports_1m = json_bool_any(item, &["supports1m", "supports_1m", "supportsOneMillion"])
        .unwrap_or_else(|| {
            model_context_window(item)
                .map(|window| window >= 1_000_000)
                .unwrap_or(false)
        });
    Some(ProfileModelOption {
        id,
        name,
        owned_by,
        supports_1m,
    })
}

fn normalize_profile_model_id(protocol: &str, value: &str) -> String {
    let trimmed = value.trim();
    if protocol == PROTOCOL_GOOGLE_GEMINI {
        return trimmed
            .strip_prefix("models/")
            .unwrap_or(trimmed)
            .to_string();
    }
    trimmed.to_string()
}

fn gemini_model_supports_generation(item: &serde_json::Value) -> bool {
    let methods = item
        .get("supportedGenerationMethods")
        .or_else(|| item.get("supported_generation_methods"))
        .and_then(|value| value.as_array());
    let Some(methods) = methods else {
        return true;
    };
    methods
        .iter()
        .filter_map(|value| value.as_str())
        .any(|method| {
            method.eq_ignore_ascii_case("generateContent")
                || method.eq_ignore_ascii_case("streamGenerateContent")
        })
}

fn json_string_any(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
    })
}

fn json_bool_any(value: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|item| item.as_bool()))
}

fn model_context_window(value: &serde_json::Value) -> Option<u64> {
    [
        "context_window",
        "contextWindow",
        "max_context_tokens",
        "maxContextTokens",
        "input_token_limit",
        "inputTokenLimit",
    ]
    .iter()
    .find_map(|key| value.get(*key).and_then(|item| item.as_u64()))
}

fn provider_error_summary(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = json_string_lookup(&value, &["error", "message"])
            .or_else(|| json_string_lookup(&value, &["message"]))
        {
            return truncate_provider_error(&message);
        }
    }
    truncate_provider_error(body)
}

fn truncate_provider_error(body: &str) -> String {
    let normalized = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "No response body.".to_string();
    }
    let mut output = String::new();
    for ch in normalized.chars().take(300) {
        output.push(ch);
    }
    if normalized.chars().count() > output.chars().count() {
        output.push_str("...");
    }
    output
}
