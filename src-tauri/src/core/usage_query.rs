use crate::core::credentials;
use crate::core::profile;
use crate::core::storage;
use crate::core::types::{
    ProfileDraft, UsageData, UsageQueryResult, UsageScriptConfig, UsageScriptSaveRequest,
    UsageScriptState, UsageScriptTemplateType,
};
use chrono::Utc;
use reqwest::blocking::Client;
use reqwest::{Method, StatusCode};
use rquickjs::{Context, Function, Runtime};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use url::{Host, Url};

const KEYCHAIN_SERVICE: &str = "codestudio-lite";
const CODEX_CHATGPT_BASE_URL: &str = "https://chatgpt.com/backend-api";
const CODESTUDIO_USER_AGENT: &str = "codestudio-lite/1.0";
const DEFAULT_USAGE_QUERY_TIMEOUT_SECONDS: u16 = 10;

#[derive(Debug, Deserialize)]
struct RequestConfig {
    url: String,
    method: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexOAuthUsageAuth {
    access_token: String,
    account_id: Option<String>,
    plan_type: Option<String>,
}

pub fn default_usage_script(template_type: &UsageScriptTemplateType) -> String {
    match template_type {
        UsageScriptTemplateType::NewApi => NEW_API_TEMPLATE.to_string(),
        UsageScriptTemplateType::TokenPlan => TOKEN_PLAN_TEMPLATE.to_string(),
        UsageScriptTemplateType::Balance => BALANCE_TEMPLATE.to_string(),
        UsageScriptTemplateType::Custom => CUSTOM_TEMPLATE.to_string(),
        UsageScriptTemplateType::General => GENERAL_TEMPLATE.to_string(),
    }
}

pub fn load_usage_state(profile_id: &str) -> Result<UsageScriptState, String> {
    ensure_profile_exists(profile_id)?;
    let config = storage::load_usage_script(profile_id)?;
    let last_result = storage::load_usage_result(profile_id)?;
    let template = config
        .as_ref()
        .map(|config| config.template_type.clone())
        .unwrap_or(UsageScriptTemplateType::General);
    let default_code = default_usage_script(&template);
    Ok(UsageScriptState {
        profile_id: profile_id.to_string(),
        config,
        last_result,
        default_code,
    })
}

pub fn save_usage_script(request: UsageScriptSaveRequest) -> Result<UsageScriptState, String> {
    let profile = ensure_profile_exists(&request.profile_id)?;
    if is_codex_official_profile(&profile) {
        let existing = storage::load_usage_script(&profile.id)?;
        let config =
            codex_official_usage_config_from_request(&profile, request, existing.as_ref())?;
        storage::save_usage_script(&config)?;
        return load_usage_state(&profile.id);
    }
    let existing = storage::load_usage_script(&profile.id)?;
    let config = config_from_request(&profile, request, existing.as_ref(), true)?;
    storage::save_usage_script(&config)?;
    load_usage_state(&profile.id)
}

pub fn test_usage_script(request: UsageScriptSaveRequest) -> Result<UsageQueryResult, String> {
    let profile = ensure_profile_exists(&request.profile_id)?;
    if is_codex_official_profile(&profile) {
        return Err(
            "Codex official OAuth usage can be queried directly; no custom script test is needed."
                .to_string(),
        );
    }
    let existing = storage::load_usage_script(&profile.id)?;
    let config = config_from_request(&profile, request, existing.as_ref(), false)?;
    execute_for_profile(&profile, &config, "test")
}

pub fn query_usage(profile_id: &str) -> Result<UsageQueryResult, String> {
    let profile = ensure_profile_exists(profile_id)?;
    if is_codex_official_profile(&profile) {
        let config = storage::load_usage_script(profile_id)?
            .ok_or_else(|| "Usage query is not configured for this profile.".to_string())?;
        if !config.enabled {
            return Err("Usage query is disabled for this profile.".to_string());
        }
        let result = query_codex_official_oauth_usage(&profile, &config)?;
        storage::save_usage_result(profile_id, &result)?;
        return Ok(result);
    }

    let config = storage::load_usage_script(profile_id)?
        .ok_or_else(|| "Usage query is not configured for this profile.".to_string())?;
    if !config.enabled {
        return Err("Usage query is disabled for this profile.".to_string());
    }
    let result = execute_for_profile(&profile, &config, "query")?;
    storage::save_usage_result(profile_id, &result)?;
    Ok(result)
}

pub fn delete_usage_script(profile_id: &str) -> Result<UsageScriptState, String> {
    ensure_profile_exists(profile_id)?;
    storage::delete_usage_script(profile_id)?;
    load_usage_state(profile_id)
}

fn ensure_profile_exists(profile_id: &str) -> Result<ProfileDraft, String> {
    profile::load_profile_by_id(profile_id)
}

fn is_codex_official_profile(profile: &ProfileDraft) -> bool {
    provider_is_official(&profile.provider) && is_codex_family_app(&profile.app)
}

fn provider_is_official(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("official")
}

fn is_codex_family_app(app: &str) -> bool {
    matches!(
        app.trim().to_ascii_lowercase().as_str(),
        "codex"
            | "codex-cli"
            | "codex-app"
            | "codex-client"
            | "codex-desktop"
            | "codex-vscode"
            | "codex-code-vscode"
            | "codex-vs-code"
    )
}

fn codex_official_usage_config_from_request(
    profile: &ProfileDraft,
    request: UsageScriptSaveRequest,
    existing: Option<&UsageScriptConfig>,
) -> Result<UsageScriptConfig, String> {
    let timeout_seconds = request.timeout_seconds.unwrap_or_else(|| {
        existing
            .map(|config| config.timeout_seconds)
            .unwrap_or(DEFAULT_USAGE_QUERY_TIMEOUT_SECONDS)
    });
    if !(2..=60).contains(&timeout_seconds) {
        return Err("Usage query timeout must be between 2 and 60 seconds.".to_string());
    }
    let auto_query_interval_minutes = request.auto_query_interval_minutes.unwrap_or_else(|| {
        existing
            .map(|config| config.auto_query_interval_minutes)
            .unwrap_or(0)
    });
    if auto_query_interval_minutes > 1440 {
        return Err("Usage query auto refresh interval cannot exceed 1440 minutes.".to_string());
    }

    Ok(UsageScriptConfig {
        profile_id: profile.id.clone(),
        enabled: request.enabled,
        template_type: UsageScriptTemplateType::General,
        code: String::new(),
        api_key: None,
        base_url: None,
        access_token: None,
        user_id: None,
        timeout_seconds,
        auto_query_interval_minutes,
        updated_at: Some(Utc::now().to_rfc3339()),
    })
}

fn config_from_request(
    profile: &ProfileDraft,
    request: UsageScriptSaveRequest,
    existing: Option<&UsageScriptConfig>,
    persist_secrets: bool,
) -> Result<UsageScriptConfig, String> {
    let code = if request.code.trim().is_empty()
        && request.template_type != UsageScriptTemplateType::Balance
    {
        default_usage_script(&request.template_type)
    } else {
        request.code.trim().to_string()
    };
    let timeout_seconds = request.timeout_seconds.unwrap_or_else(|| {
        existing
            .map(|config| config.timeout_seconds)
            .unwrap_or(DEFAULT_USAGE_QUERY_TIMEOUT_SECONDS)
    });
    if !(2..=60).contains(&timeout_seconds) {
        return Err("Usage query timeout must be between 2 and 60 seconds.".to_string());
    }
    let auto_query_interval_minutes = request.auto_query_interval_minutes.unwrap_or_else(|| {
        existing
            .map(|config| config.auto_query_interval_minutes)
            .unwrap_or(0)
    });
    if auto_query_interval_minutes > 1440 {
        return Err("Usage query auto refresh interval cannot exceed 1440 minutes.".to_string());
    }

    let api_key = normalize_optional_secret(request.api_key);
    let access_token = normalize_optional_secret(request.access_token);
    let api_key_ref = if persist_secrets {
        store_usage_secret(&profile.id, "usage_api_key", api_key.as_deref())?
    } else {
        api_key.or_else(|| existing.and_then(|config| config.api_key.clone()))
    };
    let access_token_ref = if persist_secrets {
        store_usage_secret(&profile.id, "usage_access_token", access_token.as_deref())?
    } else {
        access_token.or_else(|| existing.and_then(|config| config.access_token.clone()))
    };

    Ok(UsageScriptConfig {
        profile_id: profile.id.clone(),
        enabled: request.enabled,
        template_type: request.template_type,
        code,
        api_key: api_key_ref,
        base_url: normalize_optional_url(request.base_url),
        access_token: access_token_ref,
        user_id: normalize_optional_text(request.user_id),
        timeout_seconds,
        auto_query_interval_minutes,
        updated_at: Some(Utc::now().to_rfc3339()),
    })
}

fn store_usage_secret(
    profile_id: &str,
    account: &str,
    secret: Option<&str>,
) -> Result<Option<String>, String> {
    match secret {
        Some(value) if !value.trim().is_empty() => {
            let reference = usage_secret_ref(profile_id, account);
            credentials::store_keychain_secret(&reference, value.trim())?;
            Ok(Some(reference))
        }
        _ => Ok(None),
    }
}

fn usage_secret_ref(profile_id: &str, account: &str) -> String {
    format!("keychain:{KEYCHAIN_SERVICE}/{profile_id}/{account}")
}

fn normalize_optional_secret(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_optional_url(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().trim_end_matches('/').to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn execute_for_profile(
    profile: &ProfileDraft,
    config: &UsageScriptConfig,
    source: &str,
) -> Result<UsageQueryResult, String> {
    let (api_key, base_url, access_token, user_id) = resolve_usage_credentials(profile, config)?;
    let data = execute_usage_script(
        &config.code,
        &api_key,
        &base_url,
        u64::from(config.timeout_seconds),
        access_token.as_deref(),
        user_id.as_deref(),
        &config.template_type,
    )?;
    Ok(UsageQueryResult {
        success: true,
        data,
        error: None,
        queried_at: Utc::now().to_rfc3339(),
        source: source.to_string(),
    })
}

fn query_codex_official_oauth_usage(
    profile: &ProfileDraft,
    config: &UsageScriptConfig,
) -> Result<UsageQueryResult, String> {
    let auth = load_codex_oauth_usage_auth(profile)?;
    let timeout_secs = u64::from(config.timeout_seconds.clamp(5, 60));
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent(CODESTUDIO_USER_AGENT)
        .build()
        .map_err(|err| format!("Failed to create Codex official usage client: {err}"))?;

    let usage = send_codex_official_request(
        &client,
        &format!("{CODEX_CHATGPT_BASE_URL}/wham/usage"),
        &auth,
    )?;
    let profile_payload = send_codex_official_request(
        &client,
        &format!("{CODEX_CHATGPT_BASE_URL}/wham/profiles/me"),
        &auth,
    )
    .ok();
    let data = usage_data_from_codex_official_payloads(&usage, profile_payload.as_ref(), &auth)?;

    Ok(UsageQueryResult {
        success: true,
        data,
        error: None,
        queried_at: Utc::now().to_rfc3339(),
        source: "codex_official_oauth".to_string(),
    })
}

fn send_codex_official_request(
    client: &Client,
    url: &str,
    auth: &CodexOAuthUsageAuth,
) -> Result<Value, String> {
    let mut request = client
        .get(url)
        .bearer_auth(&auth.access_token)
        .header("Accept", "application/json");
    if let Some(account_id) = auth.account_id.as_deref().filter(|value| !value.is_empty()) {
        request = request.header("ChatGPT-Account-Id", account_id);
    }

    let response = request
        .send()
        .map_err(|err| format!("Codex official usage request failed: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|err| format!("Failed to read Codex official usage response: {err}"))?;

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(
            "Codex official OAuth token was rejected. Please complete Codex official login again."
                .to_string(),
        );
    }
    if !status.is_success() {
        return Err(format!(
            "Codex official usage request returned HTTP {status}: {}",
            truncate_for_message(&text, 240)
        ));
    }

    serde_json::from_str(&text)
        .map_err(|err| format!("Codex official usage response is not valid JSON: {err}"))
}

fn load_codex_oauth_usage_auth(profile: &ProfileDraft) -> Result<CodexOAuthUsageAuth, String> {
    let content = storage::load_codex_oauth_profile(&profile.id)?.ok_or_else(|| {
        format!(
            "Stored Codex OAuth profile could not be found for '{}'.",
            profile.name
        )
    })?;
    codex_oauth_usage_auth_from_json(&content)
}

fn codex_oauth_usage_auth_from_json(content: &str) -> Result<CodexOAuthUsageAuth, String> {
    let value: Value = serde_json::from_str(content)
        .map_err(|err| format!("Codex official auth.json is not valid JSON: {err}"))?;
    codex_oauth_usage_auth_from_value(&value)
}

fn codex_oauth_usage_auth_from_value(value: &Value) -> Result<CodexOAuthUsageAuth, String> {
    let auth_mode = value
        .get("auth_mode")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let has_api_key = string_at(value, &["openai_api_key"]).is_some()
        || string_at(value, &["OPENAI_API_KEY"]).is_some()
        || string_at(value, &["api_key"]).is_some();
    if auth_mode == "api_key" || (has_api_key && !has_chatgpt_oauth_markers(value)) {
        return Err(
            "Codex official OAuth usage query requires ChatGPT/OAuth login, not API-key login."
                .to_string(),
        );
    }

    let access_token = string_at(value, &["tokens", "access_token"])
        .or_else(|| string_at(value, &["access_token"]))
        .or_else(|| find_string_by_key(value, "access_token"))
        .ok_or_else(|| {
            "Codex official OAuth login cache is missing an access token. Please authorize Codex official login again."
                .to_string()
        })?;
    let id_token_value = value
        .get("tokens")
        .and_then(|tokens| tokens.get("id_token"))
        .or_else(|| value.get("id_token"));
    let id_token_claims = id_token_value.and_then(parse_id_token_claims);
    let account_id = string_at(value, &["tokens", "account_id"])
        .or_else(|| string_at(value, &["account_id"]))
        .or_else(|| string_at(value, &["tokens", "chatgpt_account_id"]))
        .or_else(|| string_at(value, &["chatgpt_account_id"]))
        .or_else(|| {
            id_token_claims.as_ref().and_then(|claims| {
                string_at(
                    claims,
                    &["https://api.openai.com/auth", "chatgpt_account_id"],
                )
            })
        })
        .or_else(|| {
            id_token_claims.as_ref().and_then(|claims| {
                string_at(claims, &["https://api.openai.com/auth.chatgpt_account_id"])
            })
        })
        .or_else(|| {
            id_token_claims
                .as_ref()
                .and_then(|claims| string_at(claims, &["chatgpt_account_id"]))
        });
    let plan_type = id_token_claims
        .as_ref()
        .and_then(|claims| {
            string_at(
                claims,
                &["https://api.openai.com/auth", "chatgpt_plan_type"],
            )
        })
        .or_else(|| {
            id_token_claims.as_ref().and_then(|claims| {
                string_at(claims, &["https://api.openai.com/auth.chatgpt_plan_type"])
            })
        })
        .or_else(|| {
            id_token_claims
                .as_ref()
                .and_then(|claims| string_at(claims, &["chatgpt_plan_type"]))
        })
        .or_else(|| string_at(value, &["tokens", "plan_type"]))
        .or_else(|| string_at(value, &["plan_type"]));

    Ok(CodexOAuthUsageAuth {
        access_token,
        account_id,
        plan_type,
    })
}

fn has_chatgpt_oauth_markers(value: &Value) -> bool {
    string_at(value, &["tokens", "access_token"]).is_some()
        || string_at(value, &["access_token"]).is_some()
        || string_at(value, &["tokens", "refresh_token"]).is_some()
        || string_at(value, &["refresh_token"]).is_some()
        || string_at(value, &["tokens", "id_token"]).is_some()
        || value
            .get("auth_mode")
            .and_then(Value::as_str)
            .map(|mode| mode.eq_ignore_ascii_case("chatgpt"))
            .unwrap_or(false)
}

fn usage_data_from_codex_official_payloads(
    usage: &Value,
    profile_payload: Option<&Value>,
    auth: &CodexOAuthUsageAuth,
) -> Result<Vec<UsageData>, String> {
    let mut rows = Vec::new();
    let plan_name = string_at(usage, &["plan_type"])
        .or_else(|| string_at(usage, &["plan"]))
        .or_else(|| string_at(usage, &["account", "plan_type"]))
        .or_else(|| {
            profile_payload.and_then(|payload| {
                string_at(payload, &["plan_type"])
                    .or_else(|| string_at(payload, &["plan"]))
                    .or_else(|| string_at(payload, &["account", "plan_type"]))
            })
        })
        .or_else(|| auth.plan_type.clone());

    append_codex_window_usage(
        &mut rows,
        usage,
        &["rate_limit", "primary_window"],
        "Codex 5h limit",
        plan_name.as_deref(),
    );
    append_codex_window_usage(
        &mut rows,
        usage,
        &["primary_window"],
        "Codex 5h limit",
        plan_name.as_deref(),
    );
    append_codex_window_usage(
        &mut rows,
        usage,
        &["rate_limit", "secondary_window"],
        "Codex weekly limit",
        plan_name.as_deref(),
    );
    append_codex_window_usage(
        &mut rows,
        usage,
        &["secondary_window"],
        "Codex weekly limit",
        plan_name.as_deref(),
    );
    append_codex_additional_rate_limits(&mut rows, usage, plan_name.as_deref());
    append_codex_credits_usage(&mut rows, usage);
    append_codex_spend_control_usage(&mut rows, usage);
    if let Some(payload) = profile_payload {
        append_codex_profile_stats(&mut rows, payload, plan_name.as_deref());
    }
    if rows.is_empty() {
        return Err(
            "Codex official usage response did not contain recognizable usage fields.".to_string(),
        );
    }
    Ok(rows)
}

fn append_codex_window_usage(
    rows: &mut Vec<UsageData>,
    root: &Value,
    path: &[&str],
    label: &str,
    plan_name: Option<&str>,
) {
    let Some(window) = value_at(root, path) else {
        return;
    };
    let Some(used_percent) = number_any(
        window,
        &[
            "used_percent",
            "usedPercent",
            "usage_percent",
            "usagePercent",
            "percent_used",
            "percentUsed",
        ],
    ) else {
        return;
    };
    let remaining = (100.0 - used_percent).max(0.0);
    let mut extra = Vec::new();
    if let Some(seconds) = number_any(
        window,
        &[
            "limit_window_seconds",
            "limitWindowSeconds",
            "window_seconds",
            "windowSeconds",
        ],
    ) {
        extra.push(format!("Window: {}", format_duration_seconds(seconds)));
    }
    if let Some(seconds) = number_any(
        window,
        &[
            "reset_after_seconds",
            "resetAfterSeconds",
            "resets_in_seconds",
            "resetsInSeconds",
        ],
    ) {
        extra.push(format!("Reset: {}", format_duration_seconds(seconds)));
    }
    if let Some(reset_at) = string_any(window, &["reset_at", "resetAt", "resets_at", "resetsAt"]) {
        extra.push(format!("Reset at: {reset_at}"));
    }

    rows.push(UsageData {
        is_valid: Some(true),
        invalid_message: None,
        remaining: Some(remaining),
        unit: Some("%".to_string()),
        plan_name: Some(match plan_name {
            Some(plan) if !plan.trim().is_empty() => format!("{label} ({plan})"),
            _ => label.to_string(),
        }),
        total: Some(100.0),
        used: Some(used_percent),
        extra: optional_join(extra),
    });
}

fn append_codex_additional_rate_limits(
    rows: &mut Vec<UsageData>,
    root: &Value,
    plan_name: Option<&str>,
) {
    let Some(limits) = root
        .get("additional_rate_limits")
        .or_else(|| root.get("additionalRateLimits"))
        .and_then(Value::as_array)
    else {
        return;
    };

    for (index, limit) in limits.iter().enumerate() {
        let label = string_any(limit, &["name", "label", "model", "key"])
            .unwrap_or_else(|| format!("Additional limit {}", index + 1));
        append_codex_window_usage(
            rows,
            &Value::Object(serde_json::Map::from_iter([(
                String::from("limit"),
                limit.clone(),
            )])),
            &["limit"],
            &label,
            plan_name,
        );
    }
}

fn append_codex_credits_usage(rows: &mut Vec<UsageData>, root: &Value) {
    let Some(credits) = root.get("credits") else {
        return;
    };
    if bool_any(credits, &["unlimited", "is_unlimited", "isUnlimited"]).unwrap_or(false) {
        rows.push(UsageData {
            is_valid: Some(true),
            invalid_message: None,
            remaining: None,
            unit: Some("credits".to_string()),
            plan_name: Some("Codex credits".to_string()),
            total: None,
            used: None,
            extra: Some("Unlimited".to_string()),
        });
        return;
    }
    let remaining = number_any(
        credits,
        &[
            "balance",
            "remaining",
            "available",
            "total_available",
            "totalAvailable",
        ],
    );
    if remaining.is_none() && bool_any(credits, &["has_credits", "hasCredits"]).is_none() {
        return;
    }
    rows.push(UsageData {
        is_valid: Some(bool_any(credits, &["has_credits", "hasCredits"]).unwrap_or(true)),
        invalid_message: None,
        remaining,
        unit: Some("credits".to_string()),
        plan_name: Some("Codex credits".to_string()),
        total: number_any(
            credits,
            &["total", "granted", "total_granted", "totalGranted"],
        ),
        used: number_any(credits, &["used", "total_used", "totalUsed"]),
        extra: string_any(credits, &["expires_at", "expiresAt"])
            .map(|value| format!("Expires at: {value}")),
    });
}

fn append_codex_spend_control_usage(rows: &mut Vec<UsageData>, root: &Value) {
    let Some(control) = root
        .get("spend_control")
        .or_else(|| root.get("spendControl"))
        .or_else(|| root.get("individual_limit"))
        .or_else(|| root.get("individualLimit"))
    else {
        return;
    };
    let total = number_any(control, &["limit", "hard_limit", "hardLimit", "total"]);
    let used = number_any(control, &["used", "usage", "current"]);
    if total.is_none() && used.is_none() {
        return;
    }
    let remaining = match (total, used) {
        (Some(total), Some(used)) => Some((total - used).max(0.0)),
        _ => number_any(control, &["remaining", "available"]),
    };
    rows.push(UsageData {
        is_valid: Some(true),
        invalid_message: None,
        remaining,
        unit: Some(string_any(control, &["unit", "currency"]).unwrap_or_else(|| "USD".to_string())),
        plan_name: Some("Codex spend control".to_string()),
        total,
        used,
        extra: string_any(control, &["period", "interval", "resets_at", "resetsAt"]),
    });
}

fn append_codex_profile_stats(rows: &mut Vec<UsageData>, root: &Value, plan_name: Option<&str>) {
    let stats = root.get("stats").unwrap_or(root);
    if let Some(lifetime) = number_any(
        stats,
        &[
            "lifetime_tokens",
            "lifetimeTokens",
            "total_tokens",
            "totalTokens",
        ],
    ) {
        let mut extra = Vec::new();
        if let Some(peak) = number_any(stats, &["peak_daily_tokens", "peakDailyTokens"]) {
            extra.push(format!("Peak daily: {}", format_plain_number(peak)));
        }
        if let Some(streak) = number_any(stats, &["streak_days", "streakDays"]) {
            extra.push(format!("Streak: {} days", format_plain_number(streak)));
        }
        rows.push(UsageData {
            is_valid: Some(true),
            invalid_message: None,
            remaining: None,
            unit: Some("tokens".to_string()),
            plan_name: Some(match plan_name {
                Some(plan) if !plan.trim().is_empty() => format!("Lifetime tokens ({plan})"),
                _ => "Lifetime tokens".to_string(),
            }),
            total: None,
            used: Some(lifetime),
            extra: optional_join(extra),
        });
    }
}

fn parse_id_token_claims(value: &Value) -> Option<Value> {
    if value.is_object() {
        return Some(value.clone());
    }
    let token = value.as_str()?.trim();
    let claims = token.split('.').nth(1)?;
    let bytes = decode_base64_url(claims)?;
    serde_json::from_slice(&bytes).ok()
}

fn decode_base64_url(input: &str) -> Option<Vec<u8>> {
    let mut buffer = 0u32;
    let mut bits = 0u8;
    let mut output = Vec::new();
    for ch in input.chars() {
        if ch == '=' {
            break;
        }
        let value = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => ch as u32 - 'a' as u32 + 26,
            '0'..='9' => ch as u32 - '0' as u32 + 52,
            '-' | '+' => 62,
            '_' | '/' => 63,
            _ if ch.is_whitespace() => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | value;
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(output)
}

fn value_at<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = root;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn string_at(root: &Value, path: &[&str]) -> Option<String> {
    let value = value_at(root, path)?;
    value_to_string(value)
}

fn string_any(root: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| root.get(*key).and_then(value_to_string))
}

fn value_to_string(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn find_string_by_key(root: &Value, target_key: &str) -> Option<String> {
    match root {
        Value::Object(map) => {
            for (key, value) in map {
                if key == target_key {
                    if let Some(found) = value_to_string(value) {
                        return Some(found);
                    }
                }
                if let Some(found) = find_string_by_key(value, target_key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items
            .iter()
            .find_map(|item| find_string_by_key(item, target_key)),
        _ => None,
    }
}

fn number_any(root: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| root.get(*key).and_then(value_to_number))
}

fn value_to_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn bool_any(root: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| root.get(*key).and_then(value_to_bool))
}

fn value_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(text) if text.eq_ignore_ascii_case("true") => Some(true),
        Value::String(text) if text.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

fn optional_join(items: Vec<String>) -> Option<String> {
    if items.is_empty() {
        None
    } else {
        Some(items.join(" / "))
    }
}

fn format_duration_seconds(seconds: f64) -> String {
    let rounded = seconds.max(0.0).round() as u64;
    let days = rounded / 86_400;
    let hours = (rounded % 86_400) / 3_600;
    let minutes = (rounded % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{rounded}s")
    }
}

fn format_plain_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
    }
}

fn resolve_usage_credentials(
    profile: &ProfileDraft,
    config: &UsageScriptConfig,
) -> Result<(String, String, Option<String>, Option<String>), String> {
    let api_key = match resolve_secret(config.api_key.as_deref())? {
        Some(value) => value,
        None => resolve_secret(profile.auth_ref.as_deref())?.unwrap_or_default(),
    };
    let base_url = config
        .base_url
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| profile.base_url.clone())
        .trim()
        .trim_end_matches('/')
        .to_string();
    let access_token = resolve_secret(config.access_token.as_deref())?;
    Ok((api_key, base_url, access_token, config.user_id.clone()))
}

fn resolve_secret(reference_or_plaintext: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = reference_or_plaintext
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if value.starts_with("keychain:") {
        credentials::load_keychain_secret(value).map(Some)
    } else {
        Ok(Some(value.to_string()))
    }
}

fn execute_usage_script(
    script_code: &str,
    api_key: &str,
    base_url: &str,
    timeout_secs: u64,
    access_token: Option<&str>,
    user_id: Option<&str>,
    template_type: &UsageScriptTemplateType,
) -> Result<Vec<UsageData>, String> {
    let is_custom_template = matches!(template_type, UsageScriptTemplateType::Custom);
    let script_with_vars =
        build_script_with_vars(script_code, api_key, base_url, access_token, user_id);

    if should_validate_base_url(base_url, is_custom_template) {
        validate_base_url(base_url)?;
    }

    let request_json = eval_request_config(&script_with_vars)?;
    let request: RequestConfig = serde_json::from_str(&request_json)
        .map_err(|err| format!("Invalid usage request config: {err}"))?;
    validate_request_url(&request.url, base_url, is_custom_template)?;
    let response_data = send_http_request(&request, timeout_secs)?;
    let result = eval_extractor(&script_with_vars, &response_data)?;
    validate_result(&result)?;
    usage_data_from_value(result)
}

fn eval_request_config(script_with_vars: &str) -> Result<String, String> {
    with_js_context(|ctx| {
        let config: rquickjs::Object = ctx
            .eval(script_with_vars.to_string())
            .map_err(|err| format!("Failed to parse usage script config: {err}"))?;
        let request: rquickjs::Object = config
            .get("request")
            .map_err(|err| format!("Usage script is missing request config: {err}"))?;
        stringify_js_value(ctx, request)
            .map_err(|err| format!("Failed to serialize usage request config: {err}"))
    })
}

fn eval_extractor(script_with_vars: &str, response_data: &str) -> Result<Value, String> {
    with_js_context(|ctx| {
        let config: rquickjs::Object = ctx
            .eval(script_with_vars.to_string())
            .map_err(|err| format!("Failed to re-parse usage script config: {err}"))?;
        let extractor: Function = config
            .get("extractor")
            .map_err(|err| format!("Usage script is missing extractor function: {err}"))?;
        let response_js: rquickjs::Value = ctx
            .json_parse(response_data)
            .map_err(|err| format!("Usage response is not valid JSON: {err}"))?;
        let result_js: rquickjs::Value = extractor
            .call((response_js,))
            .map_err(|err| format!("Failed to run usage extractor: {err}"))?;
        let result_json = stringify_js_value(ctx, result_js)
            .map_err(|err| format!("Failed to serialize usage result: {err}"))?;
        serde_json::from_str(&result_json)
            .map_err(|err| format!("Usage extractor returned invalid JSON: {err}"))
    })
}

fn with_js_context<T>(
    callback: impl for<'js> FnOnce(rquickjs::Ctx<'js>) -> Result<T, String>,
) -> Result<T, String> {
    let runtime = Runtime::new().map_err(|err| format!("Failed to create JS runtime: {err}"))?;
    let context =
        Context::full(&runtime).map_err(|err| format!("Failed to create JS context: {err}"))?;
    context.with(callback)
}

fn stringify_js_value<'js, T>(ctx: rquickjs::Ctx<'js>, value: T) -> Result<String, String>
where
    T: rquickjs::IntoJs<'js>,
{
    ctx.json_stringify(value)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "JSON serialization returned null.".to_string())?
        .get()
        .map_err(|err| err.to_string())
}

fn send_http_request(config: &RequestConfig, timeout_secs: u64) -> Result<String, String> {
    let method: Method = config
        .method
        .parse()
        .map_err(|_| format!("Unsupported HTTP method: {}", config.method))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs.clamp(2, 60)))
        .build()
        .map_err(|err| format!("Failed to create HTTP client: {err}"))?;
    let mut request = client.request(method, &config.url);
    for (key, value) in &config.headers {
        request = request.header(key, value);
    }
    if let Some(body) = &config.body {
        request = request.body(body.clone());
    }
    let response = request
        .send()
        .map_err(|err| format!("Usage request failed: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|err| format!("Failed to read usage response: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "HTTP {status}: {}",
            truncate_for_message(&text, 240)
        ));
    }
    Ok(text)
}

fn build_script_with_vars(
    script_code: &str,
    api_key: &str,
    base_url: &str,
    access_token: Option<&str>,
    user_id: Option<&str>,
) -> String {
    let mut replaced = script_code
        .replace("{{apiKey}}", api_key)
        .replace("{{baseUrl}}", base_url);
    if let Some(token) = access_token {
        replaced = replaced.replace("{{accessToken}}", token);
    }
    if let Some(uid) = user_id {
        replaced = replaced.replace("{{userId}}", uid);
    }
    replaced
}

fn validate_base_url(base_url: &str) -> Result<(), String> {
    if base_url.trim().is_empty() {
        return Err("Usage query Base URL cannot be empty.".to_string());
    }
    let parsed = Url::parse(base_url).map_err(|err| format!("Invalid usage Base URL: {err}"))?;
    let is_loopback = is_loopback_host(&parsed);
    if parsed.scheme() != "https" && !is_loopback {
        return Err("Usage query Base URL must use HTTPS unless it is localhost.".to_string());
    }
    let hostname = parsed
        .host_str()
        .ok_or_else(|| "Usage query Base URL must include a host.".to_string())?;
    if hostname.trim().is_empty() {
        return Err("Usage query Base URL host cannot be empty.".to_string());
    }
    Ok(())
}

fn should_validate_base_url(base_url: &str, is_custom_template: bool) -> bool {
    !base_url.trim().is_empty() && !is_custom_template
}

fn validate_request_url(
    request_url: &str,
    base_url: &str,
    is_custom_template: bool,
) -> Result<(), String> {
    let parsed_request =
        Url::parse(request_url).map_err(|err| format!("Invalid usage request URL: {err}"))?;
    let is_request_loopback = is_loopback_host(&parsed_request);
    if !is_custom_template && parsed_request.scheme() != "https" && !is_request_loopback {
        return Err("Usage request URL must use HTTPS unless it is localhost.".to_string());
    }

    if !base_url.trim().is_empty() && !is_custom_template {
        let parsed_base =
            Url::parse(base_url).map_err(|err| format!("Invalid usage Base URL: {err}"))?;
        if parsed_request.host_str() != parsed_base.host_str() {
            return Err(format!(
                "Usage request host {} must match Base URL host {}.",
                parsed_request.host_str().unwrap_or("unknown"),
                parsed_base.host_str().unwrap_or("unknown")
            ));
        }
        let request_port = parsed_request
            .port_or_known_default()
            .ok_or_else(|| "Unable to determine usage request port.".to_string())?;
        let base_port = parsed_base
            .port_or_known_default()
            .ok_or_else(|| "Unable to determine usage Base URL port.".to_string())?;
        if request_port != base_port {
            return Err(format!(
                "Usage request port {request_port} must match Base URL port {base_port}."
            ));
        }
    }

    Ok(())
}

fn is_loopback_host(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    }
}

fn validate_result(result: &Value) -> Result<(), String> {
    if let Some(items) = result.as_array() {
        if items.is_empty() {
            return Err("Usage script returned an empty array.".to_string());
        }
        for (index, item) in items.iter().enumerate() {
            validate_single_usage(item)
                .map_err(|err| format!("Usage result item {index} is invalid: {err}"))?;
        }
        return Ok(());
    }
    validate_single_usage(result)
}

fn validate_single_usage(result: &Value) -> Result<(), String> {
    let object = result
        .as_object()
        .ok_or_else(|| "Usage script must return an object or array of objects.".to_string())?;
    validate_optional_bool(object, result, "isValid")?;
    validate_optional_string(object, result, "invalidMessage")?;
    validate_optional_number(object, result, "remaining")?;
    validate_optional_string(object, result, "unit")?;
    validate_optional_string(object, result, "planName")?;
    validate_optional_number(object, result, "total")?;
    validate_optional_number(object, result, "used")?;
    validate_optional_string(object, result, "extra")?;
    Ok(())
}

fn validate_optional_bool(
    object: &serde_json::Map<String, Value>,
    result: &Value,
    key: &str,
) -> Result<(), String> {
    if object.contains_key(key) && !result[key].is_null() && !result[key].is_boolean() {
        return Err(format!("{key} must be a boolean or null."));
    }
    Ok(())
}

fn validate_optional_string(
    object: &serde_json::Map<String, Value>,
    result: &Value,
    key: &str,
) -> Result<(), String> {
    if object.contains_key(key) && !result[key].is_null() && !result[key].is_string() {
        return Err(format!("{key} must be a string or null."));
    }
    Ok(())
}

fn validate_optional_number(
    object: &serde_json::Map<String, Value>,
    result: &Value,
    key: &str,
) -> Result<(), String> {
    if object.contains_key(key) && !result[key].is_null() && !result[key].is_number() {
        return Err(format!("{key} must be a number or null."));
    }
    Ok(())
}

fn usage_data_from_value(value: Value) -> Result<Vec<UsageData>, String> {
    if value.is_array() {
        serde_json::from_value::<Vec<UsageData>>(value)
            .map_err(|err| format!("Invalid usage data array: {err}"))
    } else {
        serde_json::from_value::<UsageData>(value)
            .map(|item| vec![item])
            .map_err(|err| format!("Invalid usage data: {err}"))
    }
}

fn truncate_for_message(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

const GENERAL_TEMPLATE: &str = r#"({
  request: {
    url: "{{baseUrl}}/user/balance",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    return {
      isValid: response.is_active !== false,
      remaining: response.balance,
      unit: "USD"
    };
  }
})"#;

const NEW_API_TEMPLATE: &str = r#"({
  request: {
    url: "{{baseUrl}}/api/user/self",
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      "Authorization": "Bearer {{accessToken}}",
      "User-Agent": "codestudio-lite/1.0",
      "New-Api-User": "{{userId}}"
    }
  },
  extractor: function(response) {
    if (response.success && response.data) {
      return {
        planName: response.data.group || "Default",
        remaining: response.data.quota / 500000,
        used: response.data.used_quota / 500000,
        total: (response.data.quota + response.data.used_quota) / 500000,
        unit: "USD"
      };
    }
    return {
      isValid: false,
      invalidMessage: response.message || "Query failed"
    };
  }
})"#;

const BALANCE_TEMPLATE: &str = r#"({
  request: {
    url: "{{baseUrl}}/dashboard/billing/credit_grants",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    var total = response.total_granted || response.total_available || response.balance || 0;
    var used = response.total_used || 0;
    return {
      remaining: response.total_available !== undefined ? response.total_available : Math.max(total - used, 0),
      used: used,
      total: total,
      unit: "USD"
    };
  }
})"#;

const TOKEN_PLAN_TEMPLATE: &str = r#"({
  request: {
    url: "{{baseUrl}}/api/user/self",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    var data = response.data || response;
    var total = data.total || data.quota || data.entitlement || 0;
    var used = data.used || data.used_quota || 0;
    return {
      planName: data.plan || data.plan_name || data.group || "Token plan",
      remaining: data.remaining !== undefined ? data.remaining : Math.max(total - used, 0),
      used: used,
      total: total,
      unit: data.unit || "tokens"
    };
  }
})"#;

const CUSTOM_TEMPLATE: &str = r#"({
  request: {
    url: "{{baseUrl}}/user/balance",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    return {
      remaining: response.balance,
      unit: "USD"
    };
  }
})"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_http_non_loopback_base_url() {
        assert!(validate_base_url("http://example.com").is_err());
        assert!(validate_base_url("http://localhost:3000").is_ok());
    }

    #[test]
    fn enforces_same_origin_for_non_custom_templates() {
        assert!(validate_request_url(
            "https://api.example.com/user/balance",
            "https://api.example.com",
            false,
        )
        .is_ok());
        assert!(validate_request_url(
            "https://evil.example.com/user/balance",
            "https://api.example.com",
            false,
        )
        .is_err());
    }

    #[test]
    fn custom_template_allows_non_https_lan_url() {
        assert!(validate_request_url(
            "http://10.0.0.2:8080/usage",
            "https://api.example.com",
            true,
        )
        .is_ok());
    }

    #[test]
    fn extracts_codex_oauth_auth_without_accepting_api_key_only_login() {
        let value: Value = serde_json::from_str(
            r#"{
              "auth_mode": "chatgpt",
              "tokens": {
                "access_token": "secret-access-token",
                "account_id": "workspace-1",
                "id_token": {
                  "https://api.openai.com/auth": {
                    "chatgpt_account_id": "workspace-from-claims",
                    "chatgpt_plan_type": "plus"
                  }
                }
              }
            }"#,
        )
        .expect("auth json should parse");
        let auth = codex_oauth_usage_auth_from_value(&value).expect("OAuth auth should parse");

        assert_eq!(auth.access_token, "secret-access-token");
        assert_eq!(auth.account_id.as_deref(), Some("workspace-1"));
        assert_eq!(auth.plan_type.as_deref(), Some("plus"));

        let api_key_only: Value = serde_json::from_str(
            r#"{
              "auth_mode": "api_key",
              "openai_api_key": "sk-secret"
            }"#,
        )
        .expect("auth json should parse");

        assert!(codex_oauth_usage_auth_from_value(&api_key_only).is_err());
    }

    #[test]
    fn extracts_codex_oauth_auth_from_stored_profile_snapshot() {
        let stored_auth_json = r#"{
          "auth_mode": "chatgpt",
          "tokens": {
            "access_token": "stored-profile-token",
            "account_id": "stored-workspace",
            "id_token": {
              "https://api.openai.com/auth": {
                "chatgpt_plan_type": "team"
              }
            }
          }
        }"#;

        let auth = codex_oauth_usage_auth_from_json(stored_auth_json)
            .expect("stored OAuth profile should parse");

        assert_eq!(auth.access_token, "stored-profile-token");
        assert_eq!(auth.account_id.as_deref(), Some("stored-workspace"));
        assert_eq!(auth.plan_type.as_deref(), Some("team"));
    }

    #[test]
    fn maps_codex_official_usage_payload_to_usage_rows() {
        let usage: Value = serde_json::from_str(
            r#"{
              "plan_type": "pro",
              "rate_limit": {
                "primary_window": {
                  "used_percent": 42,
                  "limit_window_seconds": 18000,
                  "reset_after_seconds": 3600
                },
                "secondary_window": {
                  "used_percent": 7,
                  "limit_window_seconds": 604800
                }
              },
              "credits": {
                "has_credits": true,
                "unlimited": false,
                "balance": "25",
                "total_granted": 50,
                "total_used": 10
              },
              "spend_control": {
                "limit": 100,
                "used": 12,
                "currency": "USD"
              }
            }"#,
        )
        .expect("usage json should parse");
        let profile_payload: Value = serde_json::from_str(
            r#"{
              "stats": {
                "lifetime_tokens": 123456,
                "peak_daily_tokens": 3456,
                "streak_days": 9
              }
            }"#,
        )
        .expect("profile json should parse");
        let auth = CodexOAuthUsageAuth {
            access_token: "secret".to_string(),
            account_id: Some("workspace-1".to_string()),
            plan_type: None,
        };

        let rows = usage_data_from_codex_official_payloads(&usage, Some(&profile_payload), &auth)
            .expect("payload should map");

        assert!(rows.iter().any(
            |row| row.plan_name.as_deref() == Some("Codex 5h limit (pro)")
                && row.used == Some(42.0)
                && row.remaining == Some(58.0)
                && row.unit.as_deref() == Some("%")
        ));
        assert!(rows
            .iter()
            .any(|row| row.plan_name.as_deref() == Some("Codex credits")
                && row.remaining == Some(25.0)));
        assert!(rows.iter().any(
            |row| row.plan_name.as_deref() == Some("Lifetime tokens (pro)")
                && row.used == Some(123456.0)
        ));
    }
}
