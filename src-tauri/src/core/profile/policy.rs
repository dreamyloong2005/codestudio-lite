use super::*;

pub(in crate::core::profile) fn normalize_required(
    label: &str,
    value: &str,
) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(trimmed.to_string())
    }
}

pub(in crate::core::profile) fn normalize_profile_icon(
    value: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.starts_with("data:image/") {
        if trimmed.len() > 512 * 1024 {
            return Err("Profile icon image is too large.".to_string());
        }
        return Ok(Some(trimmed.to_string()));
    }
    if trimmed.chars().count() > 4 {
        return Err("Profile icon text cannot be longer than 4 characters.".to_string());
    }
    Ok(Some(trimmed.to_string()))
}

pub(in crate::core::profile) fn normalize_token(
    label: &str,
    value: &str,
) -> Result<String, String> {
    let trimmed = normalize_required(label, value)?;
    if trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        Ok(trimmed)
    } else {
        Err(format!(
            "{label} can only contain letters, numbers, '-' and '_'"
        ))
    }
}

pub(in crate::core::profile) fn normalize_provider_token(value: &str) -> Result<String, String> {
    let trimmed = normalize_required("Provider", value)?;
    if trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        Ok(trimmed)
    } else {
        Err("Provider can only contain letters, numbers, '-', '_' and '.'".to_string())
    }
}

pub(in crate::core::profile) fn validate_base_url(value: &str) -> Result<String, String> {
    let trimmed = normalize_required("Base URL", value)?;
    if trimmed.chars().any(char::is_whitespace) {
        return Err("Base URL cannot contain whitespace".to_string());
    }
    let parsed = url::Url::parse(&trimmed)
        .map_err(|_| "Base URL must start with http:// or https://".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Base URL must start with http:// or https://".to_string());
    }
    if parsed.host_str().unwrap_or_default().is_empty() {
        return Err("Base URL must include a host".to_string());
    }
    Ok(trimmed)
}

pub(in crate::core::profile) fn validate_base_url_for_provider(
    provider: &str,
    value: &str,
) -> Result<String, String> {
    if provider_is_official(provider) && value.trim().is_empty() {
        return Ok(String::new());
    }
    validate_base_url(value)
}

pub(in crate::core::profile) fn normalize_protocol(value: Option<&str>) -> Result<String, String> {
    match value.unwrap_or("").trim() {
        PROTOCOL_OPENAI_CHAT_COMPLETIONS => Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string()),
        PROTOCOL_OPENAI_RESPONSES => Ok(PROTOCOL_OPENAI_RESPONSES.to_string()),
        PROTOCOL_ANTHROPIC_MESSAGES => Ok(PROTOCOL_ANTHROPIC_MESSAGES.to_string()),
        PROTOCOL_GOOGLE_GEMINI => Ok(PROTOCOL_GOOGLE_GEMINI.to_string()),
        _ => Err("Unsupported Provider API protocol.".to_string()),
    }
}

pub(in crate::core::profile) fn protocol_display_name(protocol: &str) -> &'static str {
    match normalize_protocol(Some(protocol)).as_deref() {
        Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS) => "OpenAI Chat Completions",
        Ok(PROTOCOL_OPENAI_RESPONSES) => "OpenAI Responses API",
        Ok(PROTOCOL_ANTHROPIC_MESSAGES) => "Claude Messages API",
        Ok(PROTOCOL_GOOGLE_GEMINI) => "Gemini API",
        _ => "Unknown protocol",
    }
}

pub(in crate::core::profile) fn credential_status(
    provider: &str,
    secret_provided: bool,
) -> Severity {
    if provider_is_official(provider) {
        Severity::Info
    } else if secret_provided {
        Severity::Ok
    } else {
        Severity::Error
    }
}

pub(in crate::core::profile) fn credential_detail(provider: &str, secret_provided: bool) -> String {
    if provider_is_official(provider) {
        "Official login flow does not require an API key in this profile draft.".to_string()
    } else if secret_provided {
        "The Provider API key will be stored in the system keychain when this profile is saved; it is not written to TOML or logs.".to_string()
    } else {
        "Provider API key is required for non-official providers.".to_string()
    }
}

pub(in crate::core::profile) fn provider_is_official(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("official")
}
pub(in crate::core::profile) fn provider_requires_api_key(provider: &str) -> bool {
    !provider_is_official(provider)
}
pub(in crate::core::profile) fn is_codex_family_app(app: &str) -> bool {
    canonical_profile_app(app) == "codex"
}

pub(in crate::core::profile) fn is_custom_codex_official_profile(
    app: &str,
    provider: &str,
    mode: ProviderApplyMode,
) -> bool {
    is_codex_family_app(app) && provider_is_official(provider) && mode == ProviderApplyMode::Config
}

pub(in crate::core::profile) fn ensure_custom_official_profile_allowed(
    app: &str,
    provider: &str,
    mode: ProviderApplyMode,
) -> Result<(), String> {
    if !provider_is_official(provider) || is_custom_codex_official_profile(app, provider, mode) {
        Ok(())
    } else {
        Err("Only Codex OAuth profiles can be saved as custom official profiles.".to_string())
    }
}

pub(in crate::core::profile) fn default_profile_mode(provider: &str) -> ProviderApplyMode {
    if provider_is_official(provider) {
        ProviderApplyMode::Config
    } else {
        ProviderApplyMode::Gateway
    }
}

pub(in crate::core::profile) fn normalize_profile_mode(
    provider: &str,
    requested: Option<&ProviderApplyMode>,
) -> Result<ProviderApplyMode, String> {
    let mode = requested
        .cloned()
        .unwrap_or_else(|| default_profile_mode(provider));
    if provider_is_official(provider) && mode == ProviderApplyMode::Gateway {
        Err(
            "Official provider uses the client login directly and cannot use Gateway profiles."
                .to_string(),
        )
    } else {
        Ok(mode)
    }
}

pub(in crate::core::profile) fn normalize_stored_profile_mode(
    provider: &str,
    value: Option<String>,
) -> ProviderApplyMode {
    let mode = match value.as_deref().map(str::trim) {
        Some("config") => ProviderApplyMode::Config,
        Some("gateway") => ProviderApplyMode::Gateway,
        _ => default_profile_mode(provider),
    };
    if provider_is_official(provider) && mode == ProviderApplyMode::Gateway {
        ProviderApplyMode::Config
    } else {
        mode
    }
}

pub(in crate::core::profile) fn secret_preview(profile: &ProfileDraft) -> &'static str {
    if profile.auth_ref.is_some() {
        "keychain:****"
    } else if !provider_requires_api_key(&profile.provider) {
        "(no api key required)"
    } else {
        "(missing keychain secret)"
    }
}

pub(in crate::core::profile) fn require_profile_protocol(
    profile: &ProfileDraft,
    supported: &[&str],
) -> Result<(), String> {
    let protocol = normalize_protocol(Some(&profile.protocol))?;
    if supported.iter().any(|candidate| *candidate == protocol) {
        Ok(())
    } else {
        Err(format!(
            "{} does not support {} in Config profiles.",
            profile.app,
            protocol_display_name(&protocol)
        ))
    }
}

pub(in crate::core::profile) fn config_file_protocol_supported_fields(
    app: &str,
    provider: &str,
    protocol: &str,
) -> bool {
    if provider_is_official(provider) {
        return true;
    }
    normalize_protocol(Some(protocol))
        .map(|protocol| supports_config_protocol(app, &protocol))
        .unwrap_or(false)
}

pub(in crate::core::profile) fn profile_protocol_supported_for_mode(
    app: &str,
    mode: ProviderApplyMode,
    provider: &str,
    protocol: &str,
) -> bool {
    provider_is_official(provider)
        || mode == ProviderApplyMode::Gateway
        || config_file_protocol_supported_fields(app, provider, protocol)
}

pub(in crate::core::profile) fn ensure_profile_protocol_supported_for_mode(
    app: &str,
    mode: ProviderApplyMode,
    provider: &str,
    protocol: &str,
) -> Result<(), String> {
    if profile_protocol_supported_for_mode(app, mode, provider, protocol) {
        Ok(())
    } else {
        Err(format!(
            "Config profiles do not support {} for '{}'.",
            protocol_display_name(protocol),
            canonical_profile_app(app)
        ))
    }
}

pub(in crate::core::profile) fn config_file_protocol_supported(profile: &ProfileDraft) -> bool {
    config_file_protocol_supported_fields(&profile.app, &profile.provider, &profile.protocol)
}

pub(in crate::core::profile) fn profile_model(profile: &ProfileDraft) -> Option<&str> {
    let model = profile.model.trim();
    (!model.is_empty()).then_some(model)
}
