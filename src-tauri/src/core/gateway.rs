use crate::core::activity_log;
use crate::core::credentials;
use crate::core::gateway_request_log;
use crate::core::privacy_filter::{self, PrivacyFilterAction, PrivacyFilterMode};
use crate::core::profile;
use crate::core::storage;
use crate::core::tool_catalog::canonical_profile_tool_id;
use crate::core::types::{
    GatewayControlResult, GatewayRequestLogEntry, GatewayStatus, ProfileDraft, ProfileSummary,
    Severity, UpdateGatewaySettingsRequest,
};
use crate::core::upstream_http;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

mod auth;
mod privacy;
mod protocol;
mod route;
mod runtime;
mod server;
mod upstream;

use privacy::filter_metadata as gateway_privacy_filter_metadata;
use protocol::canonical::{
    ConvertedGatewayRequest, GatewayAssistantResponse, GatewayContentPart, GatewayMessage,
    GatewayProtocol, GatewayRequestParts, GatewayToolCall, GatewayToolSpec, GatewayUsage,
};
use protocol::{
    decode_anthropic_request as anthropic_request_parts,
    decode_gemini_request as gemini_request_parts,
    decode_openai_chat_request as chat_request_parts,
    decode_openai_responses_request as responses_request_parts,
    decode_response as assistant_response_from_protocol,
    encode_request as request_body_for_protocol, encode_response as response_body_for_protocol,
    from_route_path as gateway_protocol_from_route_path, gemini_route as gemini_route_from_path,
    merge_stream_usage, stream_text_delta_from_event, stream_update_from_event,
    write_protocol_stream_delta, write_protocol_stream_done, write_protocol_stream_start,
    write_protocol_stream_tool_call, ClientStreamState, SseBuffer, SseFrame,
};
use route::GatewayRouteTarget;
use server::{
    read_request as read_http_request, spawn_accept_loop,
    write_buffered_response as write_http_response, write_route_response, write_stream_headers,
    HttpRequest, HttpResponse, RouteResponse, StreamingResponse,
};
use upstream::{endpoint as upstream_endpoint, headers as upstream_headers_with_passthrough};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 43112;
const TOKEN_PREFIX: &str = "codestudio-local-";
const CLIENT_PROVIDER_ID: &str = "custom";
const CLIENT_MODEL: &str = "codestudio-default";
const UPSTREAM_TIMEOUT_SECONDS: u16 = 120;
#[cfg(test)]
const PROTOCOL_ANTHROPIC_MESSAGES: &str = "anthropic-messages";
const GATEWAY_CONFIG_STATE_KEY: &str = "gateway.config";

#[derive(Debug, Clone)]
pub struct GatewayClientConfig {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub base_url: String,
    pub health_url: String,
    pub token: String,
    pub token_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayConfig {
    token: String,
    host: String,
    port: u16,
    auth_enabled: bool,
    model_override: bool,
    privacy_filter_mode: PrivacyFilterMode,
}

#[derive(Debug, Deserialize)]
struct PartialGatewayConfig {
    token: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    auth_enabled: Option<bool>,
    model_override: Option<bool>,
    privacy_filter_mode: Option<PrivacyFilterMode>,
}

static GATEWAY_CONFIG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn status_gateway() -> Result<GatewayStatus, String> {
    profile::ensure_app_dirs()?;
    let config = load_or_create_gateway_config()?;
    build_status(&config)
}

pub fn start_gateway() -> Result<GatewayControlResult, String> {
    profile::ensure_app_dirs()?;
    let config = load_or_create_gateway_config()?;
    if runtime::is_running()? {
        apply_gateway_native_configs_after_start()?;
        return Ok(GatewayControlResult {
            status: build_status(&config)?,
        });
    }

    let address = format!("{}:{}", config.host, config.port);
    let listener = match TcpListener::bind(&address) {
        Ok(listener) => listener,
        Err(err) => {
            let message = format!("Could not start gateway on {address}: {err}");
            runtime::set_last_error(Some(message.clone()));
            activity_log::append(Severity::Error, message.clone())?;
            return Err(message);
        }
    };
    listener
        .set_nonblocking(true)
        .map_err(|err| err.to_string())?;

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let fallback_config = config.clone();
    let started_at = Utc::now().to_rfc3339();
    let handle = spawn_accept_loop(
        listener,
        shutdown_rx,
        move |stream| {
            let server_config =
                load_or_create_gateway_config().unwrap_or_else(|_| fallback_config.clone());
            let _ = handle_connection(stream, &server_config);
        },
        |message| runtime::set_last_error(Some(message)),
    );

    runtime::mark_started(shutdown_tx, handle, started_at)?;

    if let Err(err) = apply_gateway_native_configs_after_start() {
        let _ = stop_gateway_runtime(false);
        let message = format!("Started Local Gateway but could not update client configs: {err}");
        runtime::set_last_error(Some(message.clone()));
        activity_log::append(Severity::Error, message.clone())?;
        return Err(message);
    }

    activity_log::append(Severity::Ok, format!("Started Local Gateway on {address}."))?;
    Ok(GatewayControlResult {
        status: build_status(&config)?,
    })
}

pub fn stop_gateway() -> Result<GatewayControlResult, String> {
    profile::ensure_app_dirs()?;
    let config = load_or_create_gateway_config()?;
    if is_gateway_running()? {
        let restored = profile::restore_active_config_native_configs()?;
        if restored > 0 {
            activity_log::append(
                Severity::Info,
                format!("Restored {restored} client config file(s) after stopping Local Gateway."),
            )?;
        }
    }
    stop_gateway_runtime(true)?;

    Ok(GatewayControlResult {
        status: build_status(&config)?,
    })
}

pub fn restart_gateway() -> Result<GatewayControlResult, String> {
    let _ = stop_gateway()?;
    start_gateway()
}

pub fn update_gateway_settings(
    request: UpdateGatewaySettingsRequest,
) -> Result<GatewayControlResult, String> {
    profile::ensure_app_dirs()?;
    let mut config = load_or_create_gateway_config()?;
    if let Some(mode) = request.privacy_filter_mode {
        config.privacy_filter_mode = mode;
    }
    persist_gateway_config(&config)?;

    Ok(GatewayControlResult {
        status: build_status(&config)?,
    })
}

pub fn shutdown_for_app_exit() {
    if matches!(is_gateway_running(), Ok(true)) {
        let _ = profile::restore_active_config_native_configs();
    }
    let _ = stop_gateway_runtime(false);
}

pub fn client_config() -> Result<GatewayClientConfig, String> {
    client_config_with_tool(None)
}

pub fn client_config_for_tool(tool_id: &str) -> Result<GatewayClientConfig, String> {
    let tool_id = canonical_profile_tool_id(tool_id)
        .ok_or_else(|| format!("Invalid gateway tool id '{tool_id}'."))?;
    client_config_with_tool(Some(&tool_id))
}

fn client_config_with_tool(tool_id: Option<&str>) -> Result<GatewayClientConfig, String> {
    profile::ensure_app_dirs()?;
    let config = load_or_create_gateway_config()?;
    let base_path = tool_id
        .map(|tool_id| format!("/tools/{tool_id}/v1"))
        .unwrap_or_else(|| "/v1".to_string());
    Ok(GatewayClientConfig {
        provider_id: CLIENT_PROVIDER_ID.to_string(),
        provider_name: "CodeStudio Lite Local Gateway".to_string(),
        model: CLIENT_MODEL.to_string(),
        base_url: format!("http://{}:{}{}", config.host, config.port, base_path),
        health_url: format!("http://{}:{}/health", config.host, config.port),
        token_preview: mask_token(&config.token),
        token: config.token,
    })
}

fn is_gateway_running() -> Result<bool, String> {
    runtime::is_running()
}

fn stop_gateway_runtime(log_stop: bool) -> Result<bool, String> {
    let stopped = runtime::stop()?;
    if stopped {
        if log_stop {
            activity_log::append(
                Severity::Info,
                "Stopped Local Gateway. Connected AI clients will fail until it starts again.",
            )?;
        }
    }
    Ok(stopped)
}

fn apply_gateway_native_configs_after_start() -> Result<(), String> {
    let written = profile::apply_active_gateway_native_configs()?;
    if written > 0 {
        activity_log::append(
            Severity::Info,
            format!("Updated {written} client config file(s) for Local Gateway profiles."),
        )?;
    }
    Ok(())
}

fn build_status(config: &GatewayConfig) -> Result<GatewayStatus, String> {
    let runtime = runtime::snapshot()?;
    let active = active_profile();

    Ok(GatewayStatus {
        running: runtime.running,
        host: config.host.clone(),
        port: config.port,
        base_url: format!("http://{}:{}/v1", config.host, config.port),
        health_url: format!("http://{}:{}/health", config.host, config.port),
        auth_enabled: config.auth_enabled,
        token_preview: mask_token(&config.token),
        privacy_filter_mode: config.privacy_filter_mode,
        active_profile_id: active.as_ref().map(|profile| profile.id.clone()),
        active_profile_name: active.as_ref().map(|profile| profile.name.clone()),
        active_model: active.as_ref().and_then(|profile| profile_model(&profile)),
        started_at: runtime.started_at,
        last_error: runtime.last_error,
    })
}

fn active_profile() -> Option<ProfileDraft> {
    // Request handling must not import native configs as a side effect.
    let summary = profile::load_profile_summary_without_native_sync().ok()?;
    default_active_profile_from_summary(&summary)
}

fn active_profile_for_target(target: &GatewayRouteTarget) -> Option<ProfileDraft> {
    let summary = profile::load_profile_summary_without_native_sync().ok()?;
    if let Some(tool_id) = target.tool_id.as_deref() {
        if let Some(profile) = active_profile_for_tool(&summary, tool_id) {
            return Some(profile);
        }
        if target.strict_tool {
            return None;
        }
    }
    default_active_profile_from_summary(&summary)
}

fn active_profile_for_tool(summary: &ProfileSummary, tool_id: &str) -> Option<ProfileDraft> {
    let active_id = summary.active_profiles_by_mode.gateway.get(tool_id)?;
    summary
        .drafts
        .iter()
        .find(|draft| draft.id == *active_id && draft.app == tool_id)
        .cloned()
}

fn default_active_profile_from_summary(summary: &ProfileSummary) -> Option<ProfileDraft> {
    let active_id = summary.active_profile.as_ref()?;
    summary
        .drafts
        .iter()
        .find(|draft| draft.id == *active_id)
        .cloned()
}

fn load_or_create_gateway_config() -> Result<GatewayConfig, String> {
    if let Some(content) = storage::load_state_json(GATEWAY_CONFIG_STATE_KEY)? {
        if let Ok(partial) = toml::from_str::<PartialGatewayConfig>(&content) {
            let config = GatewayConfig {
                token: normalize_token(partial.token),
                host: partial.host.unwrap_or_else(|| DEFAULT_HOST.to_string()),
                port: partial.port.unwrap_or(DEFAULT_PORT),
                auth_enabled: partial.auth_enabled.unwrap_or(true),
                model_override: partial.model_override.unwrap_or(true),
                privacy_filter_mode: partial.privacy_filter_mode.unwrap_or_default(),
            };
            persist_gateway_config(&config)?;
            return Ok(config);
        }
    }

    let config = GatewayConfig {
        token: new_token(),
        host: DEFAULT_HOST.to_string(),
        port: DEFAULT_PORT,
        auth_enabled: true,
        model_override: true,
        privacy_filter_mode: PrivacyFilterMode::default(),
    };
    persist_gateway_config(&config)?;
    Ok(config)
}

fn persist_gateway_config(config: &GatewayConfig) -> Result<(), String> {
    let _guard = GATEWAY_CONFIG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|err| err.to_string())?;
    let content = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    storage::save_state_json(GATEWAY_CONFIG_STATE_KEY, &content)
}

fn normalize_token(token: Option<String>) -> String {
    token
        .filter(|value| value.starts_with(TOKEN_PREFIX) && value.len() > TOKEN_PREFIX.len() + 8)
        .unwrap_or_else(new_token)
}

fn new_token() -> String {
    format!("{TOKEN_PREFIX}{}", Uuid::new_v4().simple())
}

fn mask_token(token: &str) -> String {
    let suffix = token
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{TOKEN_PREFIX}****{suffix}")
}

fn handle_connection(mut stream: TcpStream, config: &GatewayConfig) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|err| err.to_string())?;
    let request = read_http_request(&mut stream)?;
    let started = Instant::now();
    let log_context = RequestLogContext::from_request(&request, config);
    let response = route_request(request, config);
    let expected_status = response.status();
    let write_result = write_route_response(&mut stream, response);
    let status = write_result.as_ref().copied().unwrap_or(expected_status);
    log_gateway_request(
        log_context,
        status,
        started.elapsed(),
        write_result.as_ref().err(),
    );
    write_result.map(|_| ())
}

struct RequestLogContext {
    client: String,
    method: String,
    path: String,
    provider: Option<String>,
    model: Option<String>,
    privacy_filter_mode: PrivacyFilterMode,
    privacy_filter_hit_count: usize,
    privacy_filter_action: PrivacyFilterAction,
}

impl RequestLogContext {
    fn from_request(request: &HttpRequest, config: &GatewayConfig) -> Self {
        let target = GatewayRouteTarget::resolve(&request.path, &request.headers);
        let active = active_profile_for_target(&target);
        let request_body = if request.method == "POST"
            && matches!(
                target.route_path.as_str(),
                "/v1/chat/completions" | "/v1/responses" | "/v1/messages"
            )
            || gemini_route_from_path(&target.route_path).is_some()
        {
            serde_json::from_slice::<serde_json::Value>(&request.body).ok()
        } else {
            None
        };
        let model = active.as_ref().and_then(profile_model).or_else(|| {
            request_body
                .as_ref()
                .and_then(|value| value.get("model"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        });
        let (privacy_filter_hit_count, privacy_filter_action) = request_body
            .as_ref()
            .and_then(|value| {
                gateway_protocol_from_route_path(&target.route_path).map(|protocol| {
                    gateway_privacy_filter_metadata(protocol, value, config.privacy_filter_mode)
                })
            })
            .unwrap_or((0, PrivacyFilterAction::None));

        Self {
            client: route::detect_client(&request.headers, target.tool_id.as_deref()),
            method: request.method.clone(),
            path: target.original_path,
            provider: active.map(|profile| profile.provider),
            model,
            privacy_filter_mode: config.privacy_filter_mode,
            privacy_filter_hit_count,
            privacy_filter_action,
        }
    }
}

fn log_gateway_request(
    context: RequestLogContext,
    status: u16,
    latency: Duration,
    write_error: Option<&String>,
) {
    if context.path == "/health" {
        return;
    }

    let mut context = context;
    let entry = gateway_request_log_entry(&mut context, status, latency, write_error);
    let _ = gateway_request_log::append(&entry);
}

fn gateway_request_log_entry(
    context: &mut RequestLogContext,
    status: u16,
    latency: Duration,
    write_error: Option<&String>,
) -> GatewayRequestLogEntry {
    let error_summary = if let Some(err) = write_error {
        Some(format!("Gateway write failed: {err}"))
    } else if status >= 400 {
        Some(match status {
            401 => "Unauthorized local gateway request".to_string(),
            400 if matches!(context.privacy_filter_action, PrivacyFilterAction::Blocked) => {
                "Request blocked by privacy filter".to_string()
            }
            404 => "Gateway route not implemented".to_string(),
            _ => format!("Gateway returned HTTP {status}"),
        })
    } else {
        None
    };

    GatewayRequestLogEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        client: std::mem::take(&mut context.client),
        method: std::mem::take(&mut context.method),
        path: std::mem::take(&mut context.path),
        provider: context.provider.take(),
        model: context.model.take(),
        status,
        latency_ms: latency.as_millis(),
        error_summary,
        privacy_filter_mode: context.privacy_filter_mode,
        privacy_filter_hit_count: context.privacy_filter_hit_count,
        privacy_filter_action: context.privacy_filter_action,
    }
}

fn route_request(request: HttpRequest, config: &GatewayConfig) -> RouteResponse {
    let target = GatewayRouteTarget::resolve(&request.path, &request.headers);

    if request.method == "OPTIONS" {
        return RouteResponse::Buffered(empty_response(204, "No Content"));
    }

    if request.method == "GET" && target.route_path == "/health" {
        return RouteResponse::Buffered(json_response(
            200,
            "OK",
            json!({
                "status": "ok",
                "service": "codestudio-lite-gateway",
                "host": config.host,
                "port": config.port,
            }),
        ));
    }

    if (target.route_path.starts_with("/v1/") || target.route_path.starts_with("/v1beta/"))
        && config.auth_enabled
        && !auth::bearer_authorized(&request.headers, &config.token)
        && !auth::scoped_request_can_skip_local_auth(
            target.strict_tool,
            target.tool_id.as_deref(),
            &config.host,
        )
    {
        return RouteResponse::Buffered(json_response(
            401,
            "Unauthorized",
            json!({
                "error": {
                    "message": "Missing or invalid local CodeStudio token.",
                    "type": "codestudio_local_auth_error"
                }
            }),
        ));
    }

    match (request.method.as_str(), target.route_path.as_str()) {
        ("GET", "/v1/models") => RouteResponse::Buffered(models_response(&target)),
        ("POST", "/v1/responses") => {
            responses_response(&request.body, &request.headers, config, &target)
        }
        ("POST", "/v1/chat/completions") => {
            chat_completions_response(&request.body, &request.headers, config, &target)
        }
        ("POST", "/v1/messages") => {
            messages_response(&request.body, &request.headers, config, &target)
        }
        ("POST", "/v1/messages/count_tokens") => {
            count_tokens_response(&request.body, config, &target)
        }
        ("POST", "/v1/responses/compact") => unsupported_gateway_route_response(
            "responses/compact is not supported by the CodeStudio Lite provider gateway.",
            "codestudio_gateway_route_unsupported",
        ),
        ("POST", "/v1/images/generations") | ("POST", "/v1/images/edits") => {
            unsupported_gateway_route_response(
                "Image generation and editing endpoints are not supported by the CodeStudio Lite provider gateway.",
                "codestudio_gateway_route_unsupported",
            )
        }
        ("POST", route_path) if gemini_route_from_path(route_path).is_some() => {
            gemini_generate_content_response(&request.body, &request.headers, config, &target)
        }
        _ => RouteResponse::Buffered(json_response(
            404,
            "Not Found",
            json!({
                "error": {
                    "message": "Route is not implemented by the CodeStudio Lite gateway skeleton.",
                    "type": "codestudio_route_not_found"
                }
            }),
        )),
    }
}

fn models_response(target: &GatewayRouteTarget) -> HttpResponse {
    if target.tool_id.as_deref() == Some("claude-desktop") {
        return claude_desktop_models_response(target);
    }
    if target.tool_id.as_deref() == Some("claude") {
        return claude_code_models_response(target);
    }

    let active = active_profile_for_target(target);
    let mut data = vec![json!({
        "id": "codestudio-default",
        "object": "model",
        "owned_by": "codestudio-lite",
    })];

    if let Some(profile) = active {
        if let Some(model) = profile_model(&profile) {
            data.push(json!({
                "id": model,
                "object": "model",
                "owned_by": profile.provider,
                "codestudio_protocol": profile.protocol,
            }));
        }
    }

    json_response(
        200,
        "OK",
        json!({
            "object": "list",
            "data": data
        }),
    )
}

fn claude_desktop_models_response(target: &GatewayRouteTarget) -> HttpResponse {
    let model_specs = active_profile_for_target(target)
        .as_ref()
        .map(profile::claude_desktop_gateway_inference_models)
        .unwrap_or_else(profile::claude_desktop_default_gateway_inference_models);
    let data = model_specs
        .iter()
        .map(|spec| {
            let mut item = json!({
                "type": "model",
                "id": spec.name,
                "created_at": "2024-01-01T00:00:00Z"
            });
            if spec.supports_1m {
                item["supports1m"] = json!(true);
            }
            item
        })
        .collect::<Vec<_>>();
    let first_id = data
        .first()
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let last_id = data
        .last()
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);

    json_response(
        200,
        "OK",
        json!({
            "data": data,
            "has_more": false,
            "first_id": first_id,
            "last_id": last_id
        }),
    )
}

fn claude_code_models_response(target: &GatewayRouteTarget) -> HttpResponse {
    let items = active_profile_for_target(target)
        .as_ref()
        .map(claude_code_model_items)
        .unwrap_or_else(claude_default_model_items);
    let first_id = items
        .first()
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let last_id = items
        .last()
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);

    json_response(
        200,
        "OK",
        json!({
            "data": items,
            "has_more": false,
            "first_id": first_id,
            "last_id": last_id
        }),
    )
}

fn claude_code_model_items(profile: &ProfileDraft) -> Vec<Value> {
    if !profile.model_mappings.is_empty() {
        return profile
            .model_mappings
            .iter()
            .map(|mapping| {
                let mut item = claude_model_item(&mapping.alias, mapping.supports_1m);
                if let Some(description) = mapping.description.as_deref() {
                    item["display_name"] = json!(description);
                    item["description"] = json!(description);
                }
                item
            })
            .collect();
    }

    profile_model(profile)
        .map(|model| vec![claude_model_item(&model, false)])
        .unwrap_or_else(claude_default_model_items)
}

fn claude_default_model_items() -> Vec<Value> {
    profile::claude_desktop_default_gateway_inference_models()
        .iter()
        .map(|spec| claude_model_item(&spec.name, spec.supports_1m))
        .collect()
}

fn claude_model_item(id: &str, supports_1m: bool) -> Value {
    let mut item = json!({
        "type": "model",
        "id": id,
        "created_at": "2024-01-01T00:00:00Z"
    });
    if supports_1m {
        item["supports1m"] = json!(true);
    }
    item
}

fn responses_response(
    body: &[u8],
    request_headers: &HashMap<String, String>,
    config: &GatewayConfig,
    target: &GatewayRouteTarget,
) -> RouteResponse {
    let request_body =
        serde_json::from_slice::<serde_json::Value>(body).unwrap_or_else(|_| json!({}));
    let active = active_profile_for_target(target);
    let effective_model = active
        .as_ref()
        .and_then(profile_model)
        .or_else(|| {
            request_body
                .get("model")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| CLIENT_MODEL.to_string());
    let stream = request_body
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    if let Some(profile) = active {
        return forward_responses(request_body, request_headers, profile, config, stream);
    }

    if let Some(response) = missing_tool_profile_response(target) {
        return response;
    }

    RouteResponse::Buffered(json_response(
        200,
        "OK",
        json!({
            "id": format!("resp-codestudio-{}", Uuid::new_v4().simple()),
            "object": "response",
            "created_at": unix_timestamp(),
            "status": "completed",
            "model": effective_model,
            "output": [{
                "id": format!("msg-codestudio-{}", Uuid::new_v4().simple()),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "CodeStudio Lite Gateway mock response. Select an active Provider Profile with a stored API key to enable upstream forwarding."
                }]
            }],
            "output_text": "CodeStudio Lite Gateway mock response. Select an active Provider Profile with a stored API key to enable upstream forwarding.",
            "usage": {
                "input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": 0
            }
        }),
    ))
}

fn chat_completions_response(
    body: &[u8],
    request_headers: &HashMap<String, String>,
    config: &GatewayConfig,
    target: &GatewayRouteTarget,
) -> RouteResponse {
    let request_body =
        serde_json::from_slice::<serde_json::Value>(body).unwrap_or_else(|_| json!({}));
    let active = active_profile_for_target(target);
    let effective_model = active
        .as_ref()
        .and_then(profile_model)
        .or_else(|| {
            request_body
                .get("model")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "codestudio-default".to_string());
    let created = unix_timestamp();
    let stream = request_body
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    if let Some(profile) = active {
        return forward_chat_completion(request_body, request_headers, profile, config, stream);
    }

    if let Some(response) = missing_tool_profile_response(target) {
        return response;
    }

    RouteResponse::Buffered(json_response(
        200,
        "OK",
        json!({
            "id": "chatcmpl-codestudio-mock",
            "object": "chat.completion",
            "created": created,
            "model": effective_model,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "CodeStudio Lite Gateway mock response. Select an active Provider Profile with a stored API key to enable upstream forwarding."
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "total_tokens": 0
            }
        }),
    ))
}

fn messages_response(
    body: &[u8],
    request_headers: &HashMap<String, String>,
    config: &GatewayConfig,
    target: &GatewayRouteTarget,
) -> RouteResponse {
    let request_body =
        serde_json::from_slice::<serde_json::Value>(body).unwrap_or_else(|_| json!({}));
    let active = active_profile_for_target(target);
    let effective_model = active
        .as_ref()
        .and_then(profile_model)
        .or_else(|| {
            request_body
                .get("model")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "codestudio-default".to_string());
    let stream = request_body
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    if let Some(profile) = active {
        return forward_messages(request_body, request_headers, profile, config, stream);
    }

    if let Some(response) = missing_tool_profile_response(target) {
        return response;
    }

    RouteResponse::Buffered(json_response(
        200,
        "OK",
        json!({
            "id": format!("msg_codestudio_{}", Uuid::new_v4().simple()),
            "type": "message",
            "role": "assistant",
            "model": effective_model,
            "content": [{
                "type": "text",
                "text": "CodeStudio Lite Gateway mock response. Select an active Provider Profile with a stored API key to enable upstream forwarding."
            }],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 0,
                "output_tokens": 0
            }
        }),
    ))
}

fn gemini_generate_content_response(
    body: &[u8],
    request_headers: &HashMap<String, String>,
    config: &GatewayConfig,
    target: &GatewayRouteTarget,
) -> RouteResponse {
    let mut request_body =
        serde_json::from_slice::<serde_json::Value>(body).unwrap_or_else(|_| json!({}));
    let route = gemini_route_from_path(&target.route_path);
    if let Some((model, _)) = route.as_ref() {
        request_body["model"] = Value::String(model.clone());
    }
    let stream = route.map(|(_, stream)| stream).unwrap_or(false)
        || request_body
            .get("stream")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
    let active = active_profile_for_target(target);
    let effective_model = active
        .as_ref()
        .and_then(profile_model)
        .or_else(|| {
            request_body
                .get("model")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "codestudio-default".to_string());

    if let Some(profile) = active {
        return forward_gateway_request(
            GatewayProtocol::GoogleGemini,
            request_body,
            request_headers,
            profile,
            config,
            stream,
        );
    }

    if let Some(response) = missing_tool_profile_response(target) {
        return response;
    }

    RouteResponse::Buffered(json_response(
        200,
        "OK",
        json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "CodeStudio Lite Gateway mock response. Select an active Provider Profile with a stored API key to enable upstream forwarding."
                    }]
                },
                "finishReason": "STOP",
                "index": 0
            }],
            "usageMetadata": {
                "promptTokenCount": 0,
                "candidatesTokenCount": 0,
                "totalTokenCount": 0
            },
            "modelVersion": effective_model
        }),
    ))
}

fn count_tokens_response(
    body: &[u8],
    config: &GatewayConfig,
    target: &GatewayRouteTarget,
) -> RouteResponse {
    if let Some(response) = missing_tool_profile_response(target) {
        return response;
    }

    let request_body = serde_json::from_slice::<Value>(body).unwrap_or_else(|_| json!({}));
    let parts = request_parts_from_client(
        GatewayProtocol::AnthropicMessages,
        &request_body,
        CLIENT_MODEL,
    );
    let input_tokens = estimate_gateway_input_tokens(&parts, config);

    RouteResponse::Buffered(json_response(
        200,
        "OK",
        json!({
            "input_tokens": input_tokens
        }),
    ))
}

fn estimate_gateway_input_tokens(parts: &GatewayRequestParts, config: &GatewayConfig) -> u64 {
    let mut text = String::new();
    if let Some(system) = parts.system.as_deref() {
        text.push_str(system);
        text.push('\n');
    }
    for message in &parts.messages {
        text.push_str(&message.role);
        text.push('\n');
        text.push_str(&content_text(&message.content));
        text.push('\n');
        for tool_call in &message.tool_calls {
            text.push_str(&tool_call.name);
            text.push('\n');
            text.push_str(&arguments_as_string(&tool_call.arguments));
            text.push('\n');
        }
    }
    for tool in &parts.tools {
        text.push_str(&tool.name);
        text.push('\n');
        if let Some(description) = tool.description.as_deref() {
            text.push_str(description);
            text.push('\n');
        }
        if let Some(schema) = tool.schema.as_ref() {
            text.push_str(&schema.to_string());
            text.push('\n');
        }
    }
    if let Some(tool_choice) = parts.tool_choice.as_ref() {
        text.push_str(&tool_choice.to_string());
    }

    let mut value = Value::String(text);
    let _ = privacy_filter::filter_json_value(&mut value, config.privacy_filter_mode);
    let filtered_text = value.as_str().unwrap_or_default();
    let chars = filtered_text.chars().count() as u64;
    (chars / 4).max(1)
}

fn unsupported_gateway_route_response(message: &str, error_type: &str) -> RouteResponse {
    RouteResponse::Buffered(json_response(
        501,
        "Not Implemented",
        json!({
            "error": {
                "message": message,
                "type": error_type
            }
        }),
    ))
}

fn missing_tool_profile_response(target: &GatewayRouteTarget) -> Option<RouteResponse> {
    if !target.strict_tool {
        return None;
    }

    let tool_id = target.tool_id.as_deref().unwrap_or("requested-tool");
    Some(RouteResponse::Buffered(json_response(
        400,
        "Bad Request",
        json!({
            "error": {
                "message": format!("No active Provider Profile is enabled for tool '{tool_id}'. Enable one in Profiles before using this scoped gateway URL."),
                "type": "codestudio_no_active_tool_profile"
            }
        }),
    )))
}

fn forward_responses(
    request_body: serde_json::Value,
    request_headers: &HashMap<String, String>,
    profile: ProfileDraft,
    config: &GatewayConfig,
    stream: bool,
) -> RouteResponse {
    forward_gateway_request(
        GatewayProtocol::OpenAiResponses,
        request_body,
        request_headers,
        profile,
        config,
        stream,
    )
}

fn forward_chat_completion(
    request_body: serde_json::Value,
    request_headers: &HashMap<String, String>,
    profile: ProfileDraft,
    config: &GatewayConfig,
    stream: bool,
) -> RouteResponse {
    forward_gateway_request(
        GatewayProtocol::OpenAiChatCompletions,
        request_body,
        request_headers,
        profile,
        config,
        stream,
    )
}

fn forward_messages(
    request_body: serde_json::Value,
    request_headers: &HashMap<String, String>,
    profile: ProfileDraft,
    config: &GatewayConfig,
    stream: bool,
) -> RouteResponse {
    forward_gateway_request(
        GatewayProtocol::AnthropicMessages,
        request_body,
        request_headers,
        profile,
        config,
        stream,
    )
}

fn forward_gateway_request(
    client_protocol: GatewayProtocol,
    mut request_body: Value,
    request_headers: &HashMap<String, String>,
    profile: ProfileDraft,
    config: &GatewayConfig,
    stream: bool,
) -> RouteResponse {
    let upstream_protocol = match GatewayProtocol::from_profile_protocol(&profile.protocol) {
        Ok(protocol) => protocol,
        Err(err) => return unsupported_protocol_response(err),
    };

    if let Err(response) = apply_gateway_privacy_filter(client_protocol, &mut request_body, config)
    {
        return response;
    }

    let api_key = match load_gateway_profile_api_key(&profile) {
        Ok(api_key) => api_key,
        Err(response) => return response,
    };

    if client_protocol == upstream_protocol {
        let requested_model = request_model(&request_body);
        let upstream_model = effective_upstream_model(&request_body, &profile, config);
        if requested_model.as_deref() != Some(upstream_model.as_str()) {
            request_body["model"] = Value::String(upstream_model.clone());
        }
        let endpoint = upstream_endpoint(upstream_protocol, &profile, &upstream_model, stream);
        let headers =
            upstream_headers_with_passthrough(upstream_protocol, &api_key, request_headers);

        if stream {
            return RouteResponse::Stream(StreamingResponse {
                expected_status: 200,
                run: Box::new(move |stream| {
                    stream_upstream_json_with_headers(
                        &endpoint,
                        &headers,
                        &request_body,
                        UPSTREAM_TIMEOUT_SECONDS,
                        stream,
                    )
                }),
            });
        }

        return forward_upstream_json_with_headers(
            &endpoint,
            &headers,
            &request_body,
            UPSTREAM_TIMEOUT_SECONDS,
        );
    }

    let converted = convert_gateway_request(
        client_protocol,
        upstream_protocol,
        &request_body,
        &profile,
        config,
        &api_key,
        request_headers,
        stream,
    );
    if stream {
        let endpoint = converted.endpoint;
        let headers = converted.headers;
        let body = converted.body;
        let model = converted.model;
        return RouteResponse::Stream(StreamingResponse {
            expected_status: 200,
            run: Box::new(move |stream| {
                stream_converted_gateway_response(
                    &endpoint,
                    &headers,
                    &body,
                    UPSTREAM_TIMEOUT_SECONDS,
                    upstream_protocol,
                    client_protocol,
                    &model,
                    stream,
                )
            }),
        });
    }
    let response = match upstream_http::post_json_with_headers(
        &converted.endpoint,
        &converted.headers,
        &converted.body,
        UPSTREAM_TIMEOUT_SECONDS,
    ) {
        Ok(response) => response,
        Err(err) => {
            return RouteResponse::Buffered(json_response(
                502,
                "Bad Gateway",
                json!({
                    "error": {
                        "message": format!("Upstream request failed: {err}"),
                        "type": "codestudio_upstream_request_error"
                    }
                }),
            ));
        }
    };

    if response.status >= 400 || client_protocol == upstream_protocol {
        return RouteResponse::Buffered(HttpResponse {
            status: response.status,
            reason: reason_for_status(response.status),
            content_type: response.content_type,
            headers: Vec::new(),
            body: response.body,
        });
    }

    let upstream_value = match serde_json::from_slice::<Value>(&response.body) {
        Ok(value) => value,
        Err(err) => {
            return RouteResponse::Buffered(json_response(
                502,
                "Bad Gateway",
                json!({
                    "error": {
                        "message": format!("Upstream response could not be converted because it was not valid JSON: {err}"),
                        "type": "codestudio_upstream_conversion_error"
                    }
                }),
            ));
        }
    };
    match convert_gateway_response(
        upstream_protocol,
        client_protocol,
        &upstream_value,
        &converted.model,
    ) {
        Ok(value) => RouteResponse::Buffered(json_response(
            response.status,
            reason_for_status(response.status),
            value,
        )),
        Err(err) => RouteResponse::Buffered(json_response(
            502,
            "Bad Gateway",
            json!({
                "error": {
                    "message": format!("Upstream response could not be converted: {err}"),
                    "type": "codestudio_upstream_conversion_error"
                }
            }),
        )),
    }
}

fn apply_gateway_privacy_filter(
    client_protocol: GatewayProtocol,
    request_body: &mut Value,
    config: &GatewayConfig,
) -> Result<privacy_filter::PrivacyFilterReport, RouteResponse> {
    privacy::apply(client_protocol, request_body, config.privacy_filter_mode).map_err(|hit_count| {
        RouteResponse::Buffered(json_response(
            400,
            "Bad Request",
            json!({
                "error": {
                    "message": "Request blocked by Local Gateway privacy filter.",
                    "type": "privacy_filter_blocked",
                    "hit_count": hit_count
                }
            }),
        ))
    })
}

fn unsupported_protocol_response(message: String) -> RouteResponse {
    RouteResponse::Buffered(json_response(
        400,
        "Bad Request",
        json!({
            "error": {
                "message": message,
                "type": "codestudio_unsupported_protocol"
            }
        }),
    ))
}

fn load_gateway_profile_api_key(profile: &ProfileDraft) -> Result<String, RouteResponse> {
    let Some(auth_ref) = profile.auth_ref.as_deref() else {
        if profile.provider.eq_ignore_ascii_case("official") {
            return Err(RouteResponse::Buffered(json_response(
                400,
                "Bad Request",
                json!({
                    "error": {
                        "message": "Official Provider Profile uses the client login directly and cannot be served through the local gateway.",
                        "type": "codestudio_official_profile_not_gateway_routable"
                    }
                }),
            )));
        }
        return Err(RouteResponse::Buffered(json_response(
            400,
            "Bad Request",
            json!({
                "error": {
                    "message": "Active Provider Profile has no keychain API key reference.",
                    "type": "codestudio_missing_provider_key"
                }
            }),
        )));
    };

    credentials::load_keychain_secret(auth_ref).map_err(|err| {
        RouteResponse::Buffered(json_response(
            400,
            "Bad Request",
            json!({
                "error": {
                    "message": err,
                    "type": "codestudio_provider_key_unavailable"
                }
            }),
        ))
    })
}

fn convert_gateway_request(
    client_protocol: GatewayProtocol,
    upstream_protocol: GatewayProtocol,
    request_body: &Value,
    profile: &ProfileDraft,
    config: &GatewayConfig,
    api_key: &str,
    request_headers: &HashMap<String, String>,
    stream: bool,
) -> ConvertedGatewayRequest {
    let model = effective_upstream_model(request_body, profile, config);
    let parts = request_parts_from_client(client_protocol, request_body, &model);
    let body = request_body_for_protocol(upstream_protocol, &parts, stream);
    let endpoint = upstream_endpoint(upstream_protocol, profile, &model, stream);
    let headers = upstream_headers_with_passthrough(upstream_protocol, api_key, request_headers);

    ConvertedGatewayRequest {
        endpoint,
        headers,
        body,
        model,
    }
}

fn effective_upstream_model(
    request_body: &Value,
    profile: &ProfileDraft,
    config: &GatewayConfig,
) -> String {
    if config.model_override && !profile.model.trim().is_empty() {
        return profile.model.trim().to_string();
    }
    request_model(request_body)
        .map(|model| mapped_profile_model(profile, &model).unwrap_or(model))
        .or_else(|| profile_model(profile))
        .or_else(|| default_mapped_profile_model(profile))
        .unwrap_or_else(|| CLIENT_MODEL.to_string())
}

fn mapped_profile_model(profile: &ProfileDraft, requested_model: &str) -> Option<String> {
    profile
        .model_mappings
        .iter()
        .find(|mapping| mapping.alias.eq_ignore_ascii_case(requested_model.trim()))
        .map(|mapping| mapping.model.trim())
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

fn default_mapped_profile_model(profile: &ProfileDraft) -> Option<String> {
    profile
        .model_mappings
        .first()
        .map(|mapping| mapping.model.trim())
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

fn request_model(request_body: &Value) -> Option<String> {
    request_body
        .get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn request_parts_from_client(
    client_protocol: GatewayProtocol,
    request_body: &Value,
    model: &str,
) -> GatewayRequestParts {
    match client_protocol {
        GatewayProtocol::OpenAiChatCompletions => chat_request_parts(request_body, model),
        GatewayProtocol::OpenAiResponses => responses_request_parts(request_body, model),
        GatewayProtocol::AnthropicMessages => anthropic_request_parts(request_body, model),
        GatewayProtocol::GoogleGemini => gemini_request_parts(request_body, model),
    }
}

fn convert_gateway_response(
    upstream_protocol: GatewayProtocol,
    client_protocol: GatewayProtocol,
    upstream_value: &Value,
    model: &str,
) -> Result<Value, String> {
    if upstream_protocol == client_protocol {
        return Ok(upstream_value.clone());
    }
    let response = assistant_response_from_protocol(upstream_protocol, upstream_value);
    Ok(response_body_for_protocol(
        client_protocol,
        model,
        &response,
    ))
}

fn assistant_text_from_response(protocol: GatewayProtocol, value: &Value) -> String {
    content_text(&assistant_response_from_protocol(protocol, value).content)
}

fn usage_from_response(protocol: GatewayProtocol, value: &Value) -> GatewayUsage {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let usage = value.get("usage").unwrap_or(&Value::Null);
            let prompt_details = usage.get("prompt_tokens_details").cloned();
            let completion_details = usage.get("completion_tokens_details").cloned();
            let input = usage
                .get("prompt_tokens")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("completion_tokens")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            GatewayUsage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: usage
                    .get("total_tokens")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(input + output),
                cached_input_tokens: nested_u64(usage, &["prompt_tokens_details", "cached_tokens"]),
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                reasoning_tokens: nested_u64(
                    usage,
                    &["completion_tokens_details", "reasoning_tokens"],
                ),
                audio_input_tokens: nested_u64(usage, &["prompt_tokens_details", "audio_tokens"]),
                audio_output_tokens: nested_u64(
                    usage,
                    &["completion_tokens_details", "audio_tokens"],
                ),
                image_input_tokens: nested_u64(usage, &["prompt_tokens_details", "image_tokens"]),
                image_output_tokens: nested_u64(
                    usage,
                    &["completion_tokens_details", "image_tokens"],
                ),
                raw_prompt_details: prompt_details,
                raw_completion_details: completion_details,
            }
        }
        GatewayProtocol::OpenAiResponses => {
            let usage = value.get("usage").unwrap_or(&Value::Null);
            let input_details = usage.get("input_tokens_details").cloned();
            let output_details = usage.get("output_tokens_details").cloned();
            let input = usage
                .get("input_tokens")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            GatewayUsage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: usage
                    .get("total_tokens")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(input + output),
                cached_input_tokens: nested_u64(usage, &["input_tokens_details", "cached_tokens"]),
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                reasoning_tokens: nested_u64(usage, &["output_tokens_details", "reasoning_tokens"]),
                audio_input_tokens: nested_u64(usage, &["input_tokens_details", "audio_tokens"]),
                audio_output_tokens: nested_u64(usage, &["output_tokens_details", "audio_tokens"]),
                image_input_tokens: nested_u64(usage, &["input_tokens_details", "image_tokens"]),
                image_output_tokens: nested_u64(usage, &["output_tokens_details", "image_tokens"]),
                raw_prompt_details: input_details,
                raw_completion_details: output_details,
            }
        }
        GatewayProtocol::AnthropicMessages => {
            let usage = value.get("usage").unwrap_or(&Value::Null);
            let input = usage
                .get("input_tokens")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            GatewayUsage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: input + output,
                cached_input_tokens: usage
                    .get("cache_read_input_tokens")
                    .and_then(|value| value.as_u64()),
                cache_creation_input_tokens: usage
                    .get("cache_creation_input_tokens")
                    .and_then(|value| value.as_u64()),
                cache_read_input_tokens: usage
                    .get("cache_read_input_tokens")
                    .and_then(|value| value.as_u64()),
                reasoning_tokens: None,
                audio_input_tokens: None,
                audio_output_tokens: None,
                image_input_tokens: None,
                image_output_tokens: None,
                raw_prompt_details: None,
                raw_completion_details: None,
            }
        }
        GatewayProtocol::GoogleGemini => {
            let usage = value.get("usageMetadata").unwrap_or(&Value::Null);
            let input = usage
                .get("promptTokenCount")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("candidatesTokenCount")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            GatewayUsage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: usage
                    .get("totalTokenCount")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(input + output),
                cached_input_tokens: usage
                    .get("cachedContentTokenCount")
                    .and_then(|value| value.as_u64()),
                cache_creation_input_tokens: None,
                cache_read_input_tokens: usage
                    .get("cachedContentTokenCount")
                    .and_then(|value| value.as_u64()),
                reasoning_tokens: None,
                audio_input_tokens: modality_tokens(usage.get("promptTokensDetails"), "AUDIO"),
                audio_output_tokens: modality_tokens(usage.get("candidatesTokensDetails"), "AUDIO"),
                image_input_tokens: modality_tokens(usage.get("promptTokensDetails"), "IMAGE"),
                image_output_tokens: modality_tokens(usage.get("candidatesTokensDetails"), "IMAGE"),
                raw_prompt_details: usage.get("promptTokensDetails").cloned(),
                raw_completion_details: usage.get("candidatesTokensDetails").cloned(),
            }
        }
    }
}

fn push_message_if_useful(messages: &mut Vec<GatewayMessage>, message: GatewayMessage) {
    if !message.content.is_empty() || !message.tool_calls.is_empty() {
        messages.push(message);
    }
}

fn content_parts_from_value(value: &Value) -> Vec<GatewayContentPart> {
    match value {
        Value::Null => Vec::new(),
        Value::String(text) => trimmed_text_part(text),
        Value::Array(items) => items
            .iter()
            .flat_map(content_parts_from_value)
            .collect::<Vec<_>>(),
        Value::Object(object) => content_parts_from_object(object, value),
        _ => Vec::new(),
    }
}

fn content_parts_from_object(object: &Map<String, Value>, raw: &Value) -> Vec<GatewayContentPart> {
    let item_type = object
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    match item_type {
        "text" | "input_text" | "output_text" => {
            return object
                .get("text")
                .and_then(|value| value.as_str())
                .map(trimmed_text_part)
                .unwrap_or_default();
        }
        "image_url" | "input_image" => {
            if let Some(url) = object
                .get("image_url")
                .and_then(image_url_string)
                .or_else(|| {
                    object
                        .get("url")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
            {
                return vec![image_part_from_url(&url)];
            }
        }
        "image" => {
            if let Some(source) = object.get("source") {
                if let Some(part) = anthropic_image_part(source) {
                    return vec![part];
                }
            }
        }
        "tool_result" => {
            let tool_call_id = object
                .get("tool_use_id")
                .or_else(|| object.get("tool_call_id"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            return vec![GatewayContentPart::ToolResult {
                tool_call_id,
                content: text_from_value(object.get("content").unwrap_or(&Value::Null)),
            }];
        }
        _ => {}
    }

    if let Some(value) = object
        .get("inlineData")
        .or_else(|| object.get("inline_data"))
    {
        if let Some(part) = gemini_inline_data_part(value) {
            return vec![part];
        }
    }
    if let Some(value) = object.get("fileData").or_else(|| object.get("file_data")) {
        if let Some(uri) = value
            .get("fileUri")
            .or_else(|| value.get("file_uri"))
            .and_then(|value| value.as_str())
        {
            return vec![GatewayContentPart::ImageUrl(uri.to_string())];
        }
    }
    if let Some(value) = object
        .get("functionResponse")
        .or_else(|| object.get("function_response"))
    {
        let name = value
            .get("name")
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        return vec![GatewayContentPart::ToolResult {
            tool_call_id: name,
            content: text_from_value(value.get("response").unwrap_or(value)),
        }];
    }

    for key in ["text", "input_text", "output_text"] {
        if let Some(text) = object.get(key).and_then(|value| value.as_str()) {
            let parts = trimmed_text_part(text);
            if !parts.is_empty() {
                return parts;
            }
        }
    }

    let text = text_from_value(raw);
    if !text.is_empty() {
        vec![GatewayContentPart::Text(text)]
    } else {
        vec![GatewayContentPart::Unknown(raw.clone())]
    }
}

fn trimmed_text_part(text: &str) -> Vec<GatewayContentPart> {
    let text = text.trim();
    if text.is_empty() {
        Vec::new()
    } else {
        vec![GatewayContentPart::Text(text.to_string())]
    }
}

fn image_url_string(value: &Value) -> Option<String> {
    match value {
        Value::String(url) => Some(url.to_string()),
        Value::Object(object) => object
            .get("url")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        _ => None,
    }
}

fn anthropic_image_part(source: &Value) -> Option<GatewayContentPart> {
    let source_type = source.get("type").and_then(|value| value.as_str())?;
    match source_type {
        "base64" => Some(GatewayContentPart::ImageBase64 {
            mime_type: source
                .get("media_type")
                .and_then(|value| value.as_str())
                .unwrap_or("image/png")
                .to_string(),
            data: source.get("data")?.as_str()?.to_string(),
        }),
        "url" => Some(GatewayContentPart::ImageUrl(
            source.get("url")?.as_str()?.to_string(),
        )),
        _ => None,
    }
}

fn gemini_inline_data_part(value: &Value) -> Option<GatewayContentPart> {
    Some(GatewayContentPart::ImageBase64 {
        mime_type: value
            .get("mimeType")
            .or_else(|| value.get("mime_type"))
            .and_then(|value| value.as_str())
            .unwrap_or("image/png")
            .to_string(),
        data: value.get("data")?.as_str()?.to_string(),
    })
}

fn image_part_from_url(url: &str) -> GatewayContentPart {
    if let Some((mime_type, data)) = split_data_url(url) {
        GatewayContentPart::ImageBase64 { mime_type, data }
    } else {
        GatewayContentPart::ImageUrl(url.to_string())
    }
}

fn split_data_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("data:")?;
    let (mime_type, data) = rest.split_once(";base64,")?;
    Some((mime_type.to_string(), data.to_string()))
}

fn data_url(mime_type: &str, data: &str) -> String {
    format!("data:{mime_type};base64,{data}")
}

fn content_text(parts: &[GatewayContentPart]) -> String {
    parts
        .iter()
        .filter_map(|part| match part {
            GatewayContentPart::Text(text) => Some(text.clone()),
            GatewayContentPart::ToolResult { content, .. } => Some(content.clone()),
            GatewayContentPart::Unknown(value) => {
                let text = text_from_value(value);
                (!text.is_empty()).then_some(text)
            }
            GatewayContentPart::ImageUrl(_) | GatewayContentPart::ImageBase64 { .. } => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn openai_tool_calls_from_value(value: &Value) -> Vec<GatewayToolCall> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(openai_tool_call_from_value)
        .collect()
}

fn openai_tool_call_from_value(value: &Value) -> Option<GatewayToolCall> {
    let function = value.get("function").unwrap_or(value);
    let name = function
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    let id = value
        .get("id")
        .or_else(|| value.get("call_id"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("call_codestudio_{}", Uuid::new_v4().simple()));
    Some(GatewayToolCall {
        id,
        name,
        arguments: argument_value(function.get("arguments")),
    })
}

fn responses_function_call_from_value(value: &Value) -> Option<GatewayToolCall> {
    let name = value
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    let id = value
        .get("call_id")
        .or_else(|| value.get("id"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("call_codestudio_{}", Uuid::new_v4().simple()));
    Some(GatewayToolCall {
        id,
        name,
        arguments: argument_value(value.get("arguments")),
    })
}

fn anthropic_content_and_tool_calls(
    value: &Value,
) -> (Vec<GatewayContentPart>, Vec<GatewayToolCall>) {
    let mut content = Vec::new();
    let mut tool_calls = Vec::new();

    match value {
        Value::Array(items) => {
            for item in items {
                if item
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    == "tool_use"
                {
                    if let Some(tool_call) = anthropic_tool_call_from_value(item) {
                        tool_calls.push(tool_call);
                    }
                } else {
                    content.extend(content_parts_from_value(item));
                }
            }
        }
        _ => content.extend(content_parts_from_value(value)),
    }

    (content, tool_calls)
}

fn anthropic_tool_call_from_value(value: &Value) -> Option<GatewayToolCall> {
    let name = value
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    let id = value
        .get("id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("call_codestudio_{}", Uuid::new_v4().simple()));
    Some(GatewayToolCall {
        id,
        name,
        arguments: argument_value(value.get("input")),
    })
}

fn gemini_parts_and_tool_calls(value: &Value) -> (Vec<GatewayContentPart>, Vec<GatewayToolCall>) {
    let mut content = Vec::new();
    let mut tool_calls = Vec::new();

    for item in value.as_array().into_iter().flatten() {
        if let Some(function_call) = item
            .get("functionCall")
            .or_else(|| item.get("function_call"))
        {
            if let Some(tool_call) = gemini_tool_call_from_value(function_call) {
                tool_calls.push(tool_call);
            }
        } else {
            content.extend(content_parts_from_value(item));
        }
    }

    if !value.is_array() {
        content.extend(content_parts_from_value(value));
    }

    (content, tool_calls)
}

fn gemini_tool_call_from_value(value: &Value) -> Option<GatewayToolCall> {
    let name = value
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    Some(GatewayToolCall {
        id: format!("call_codestudio_{}", Uuid::new_v4().simple()),
        name,
        arguments: argument_value(value.get("args")),
    })
}

fn argument_value(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_string()))
        }
        Some(value) if !value.is_null() => value.clone(),
        _ => json!({}),
    }
}

fn arguments_as_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.to_string(),
        value => serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()),
    }
}

fn arguments_as_object(value: &Value) -> Value {
    match value {
        Value::Object(_) => value.clone(),
        Value::String(text) => serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({})),
        _ => json!({}),
    }
}

fn openai_chat_message_value(message: &GatewayMessage) -> Value {
    let mut object = Map::new();
    object.insert("role".to_string(), Value::String(message.role.clone()));
    object.insert(
        "content".to_string(),
        if message.role == "tool" {
            Value::String(content_text(&message.content))
        } else {
            openai_chat_content_value(&message.content)
        },
    );
    if let Some(tool_call_id) = message.tool_call_id.as_deref() {
        object.insert(
            "tool_call_id".to_string(),
            Value::String(tool_call_id.to_string()),
        );
    }
    if !message.tool_calls.is_empty() {
        object.insert(
            "tool_calls".to_string(),
            Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(openai_tool_call_value)
                    .collect(),
            ),
        );
    }
    Value::Object(object)
}

fn openai_chat_content_value(parts: &[GatewayContentPart]) -> Value {
    let content_blocks = openai_chat_content_blocks(parts);
    if content_blocks.is_empty() {
        return Value::String(String::new());
    }
    if content_blocks.len() == 1 {
        if let Some(text) = content_blocks[0]
            .get("text")
            .and_then(|value| value.as_str())
            .filter(|_| {
                content_blocks[0]
                    .get("type")
                    .and_then(|value| value.as_str())
                    == Some("text")
            })
        {
            return Value::String(text.to_string());
        }
    }
    Value::Array(content_blocks)
}

fn openai_chat_content_blocks(parts: &[GatewayContentPart]) -> Vec<Value> {
    let mut blocks = Vec::new();
    for part in parts {
        match part {
            GatewayContentPart::Text(text) if !text.is_empty() => {
                blocks.push(json!({ "type": "text", "text": text }));
            }
            GatewayContentPart::ImageUrl(url) => {
                blocks.push(json!({ "type": "image_url", "image_url": { "url": url } }));
            }
            GatewayContentPart::ImageBase64 { mime_type, data } => {
                blocks.push(json!({
                    "type": "image_url",
                    "image_url": { "url": data_url(mime_type, data) }
                }));
            }
            GatewayContentPart::ToolResult { content, .. } if !content.is_empty() => {
                blocks.push(json!({ "type": "text", "text": content }));
            }
            GatewayContentPart::Unknown(value) => blocks.push(value.clone()),
            _ => {}
        }
    }
    blocks
}

fn openai_tool_call_value(tool_call: &GatewayToolCall) -> Value {
    json!({
        "id": tool_call.id,
        "type": "function",
        "function": {
            "name": tool_call.name,
            "arguments": arguments_as_string(&tool_call.arguments)
        }
    })
}

fn responses_input_items_for_message(message: &GatewayMessage) -> Vec<Value> {
    if message.role == "tool" {
        return vec![json!({
            "type": "function_call_output",
            "call_id": message.tool_call_id.clone().unwrap_or_else(|| "call_codestudio_unknown".to_string()),
            "output": content_text(&message.content)
        })];
    }

    let mut items = Vec::new();
    if !message.content.is_empty() || message.tool_calls.is_empty() {
        items.push(json!({
            "type": "message",
            "role": message.role,
            "content": responses_input_content_parts(&message.content)
        }));
    }
    for tool_call in &message.tool_calls {
        items.push(json!({
            "type": "function_call",
            "call_id": tool_call.id,
            "name": tool_call.name,
            "arguments": arguments_as_string(&tool_call.arguments)
        }));
    }
    items
}

fn responses_input_content_parts(parts: &[GatewayContentPart]) -> Vec<Value> {
    responses_content_parts(parts, false)
}

fn responses_output_content_parts(parts: &[GatewayContentPart]) -> Vec<Value> {
    let parts = responses_content_parts(parts, true);
    if parts.is_empty() {
        vec![json!({ "type": "output_text", "text": "" })]
    } else {
        parts
    }
}

fn responses_content_parts(parts: &[GatewayContentPart], output: bool) -> Vec<Value> {
    let text_type = if output { "output_text" } else { "input_text" };
    let mut blocks = Vec::new();
    for part in parts {
        match part {
            GatewayContentPart::Text(text) if !text.is_empty() => {
                blocks.push(json!({ "type": text_type, "text": text }));
            }
            GatewayContentPart::ImageUrl(url) => {
                blocks.push(json!({ "type": "input_image", "image_url": url }));
            }
            GatewayContentPart::ImageBase64 { mime_type, data } => {
                blocks.push(json!({
                    "type": "input_image",
                    "image_url": data_url(mime_type, data)
                }));
            }
            GatewayContentPart::ToolResult { content, .. } if !content.is_empty() => {
                blocks.push(json!({ "type": text_type, "text": content }));
            }
            GatewayContentPart::Unknown(value) => blocks.push(value.clone()),
            _ => {}
        }
    }
    if blocks.is_empty() {
        blocks.push(json!({ "type": text_type, "text": "" }));
    }
    blocks
}

fn anthropic_content_value(message: &GatewayMessage) -> Value {
    let blocks = anthropic_message_content_blocks(message);
    if message.tool_calls.is_empty() && blocks.len() == 1 {
        if let Some(text) = blocks[0]
            .get("text")
            .and_then(|value| value.as_str())
            .filter(|_| blocks[0].get("type").and_then(|value| value.as_str()) == Some("text"))
        {
            return Value::String(text.to_string());
        }
    }
    Value::Array(blocks)
}

fn anthropic_message_content_blocks(message: &GatewayMessage) -> Vec<Value> {
    let mut blocks = anthropic_content_blocks(&message.content);
    for tool_call in &message.tool_calls {
        blocks.push(json!({
            "type": "tool_use",
            "id": tool_call.id,
            "name": tool_call.name,
            "input": arguments_as_object(&tool_call.arguments)
        }));
    }
    if blocks.is_empty() {
        blocks.push(json!({ "type": "text", "text": "" }));
    }
    blocks
}

fn anthropic_assistant_content_blocks(response: &GatewayAssistantResponse) -> Vec<Value> {
    let mut message = GatewayMessage {
        role: "assistant".to_string(),
        content: response.content.clone(),
        tool_call_id: None,
        tool_calls: response.tool_calls.clone(),
    };
    if message.content.is_empty() && message.tool_calls.is_empty() {
        message
            .content
            .push(GatewayContentPart::Text(String::new()));
    }
    anthropic_message_content_blocks(&message)
}

fn anthropic_content_blocks(parts: &[GatewayContentPart]) -> Vec<Value> {
    let mut blocks = Vec::new();
    for part in parts {
        match part {
            GatewayContentPart::Text(text) if !text.is_empty() => {
                blocks.push(json!({ "type": "text", "text": text }));
            }
            GatewayContentPart::ImageBase64 { mime_type, data } => {
                blocks.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": mime_type,
                        "data": data
                    }
                }));
            }
            GatewayContentPart::ImageUrl(url) => {
                blocks.push(json!({
                    "type": "image",
                    "source": {
                        "type": "url",
                        "url": url
                    }
                }));
            }
            GatewayContentPart::ToolResult {
                tool_call_id,
                content,
            } => {
                blocks.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_call_id.clone().unwrap_or_else(|| "call_codestudio_unknown".to_string()),
                    "content": content
                }));
            }
            GatewayContentPart::Unknown(value) => {
                blocks.push(json!({ "type": "text", "text": value.to_string() }));
            }
            _ => {}
        }
    }
    blocks
}

fn gemini_parts_value(message: &GatewayMessage) -> Vec<Value> {
    let mut parts = gemini_content_parts(&message.content);
    for tool_call in &message.tool_calls {
        parts.push(json!({
            "functionCall": {
                "name": tool_call.name,
                "args": arguments_as_object(&tool_call.arguments)
            }
        }));
    }
    if parts.is_empty() {
        parts.push(json!({ "text": "" }));
    }
    parts
}

fn gemini_assistant_parts(response: &GatewayAssistantResponse) -> Vec<Value> {
    let message = GatewayMessage {
        role: "assistant".to_string(),
        content: response.content.clone(),
        tool_call_id: None,
        tool_calls: response.tool_calls.clone(),
    };
    gemini_parts_value(&message)
}

fn gemini_content_parts(parts: &[GatewayContentPart]) -> Vec<Value> {
    let mut blocks = Vec::new();
    for part in parts {
        match part {
            GatewayContentPart::Text(text) if !text.is_empty() => {
                blocks.push(json!({ "text": text }));
            }
            GatewayContentPart::ImageBase64 { mime_type, data } => {
                blocks.push(json!({
                    "inlineData": {
                        "mimeType": mime_type,
                        "data": data
                    }
                }));
            }
            GatewayContentPart::ImageUrl(url) => {
                blocks.push(json!({
                    "fileData": {
                        "fileUri": url
                    }
                }));
            }
            GatewayContentPart::ToolResult {
                tool_call_id,
                content,
            } => {
                blocks.push(json!({
                    "functionResponse": {
                        "name": tool_call_id.clone().unwrap_or_else(|| "tool".to_string()),
                        "response": { "content": content }
                    }
                }));
            }
            GatewayContentPart::Unknown(value) => blocks.push(value.clone()),
            _ => {}
        }
    }
    blocks
}

fn openai_tool_specs_from_value(value: &Value) -> Vec<GatewayToolSpec> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let function = item.get("function").unwrap_or(item);
            function_tool_spec(
                function.get("name")?.as_str()?,
                function
                    .get("description")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                function.get("parameters").cloned(),
            )
        })
        .collect()
}

fn openai_legacy_function_specs(value: &Value) -> Vec<GatewayToolSpec> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            function_tool_spec(
                item.get("name")?.as_str()?,
                item.get("description")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                item.get("parameters").cloned(),
            )
        })
        .collect()
}

fn responses_tool_specs_from_value(value: &Value) -> Vec<GatewayToolSpec> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            function_tool_spec(
                item.get("name")
                    .or_else(|| {
                        item.get("function")
                            .and_then(|function| function.get("name"))
                    })?
                    .as_str()?,
                item.get("description")
                    .or_else(|| {
                        item.get("function")
                            .and_then(|function| function.get("description"))
                    })
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                item.get("parameters")
                    .or_else(|| item.get("input_schema"))
                    .or_else(|| {
                        item.get("function")
                            .and_then(|function| function.get("parameters"))
                    })
                    .cloned(),
            )
        })
        .collect()
}

fn anthropic_tool_specs_from_value(value: &Value) -> Vec<GatewayToolSpec> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            function_tool_spec(
                item.get("name")?.as_str()?,
                item.get("description")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                item.get("input_schema")
                    .or_else(|| item.get("parameters"))
                    .cloned(),
            )
        })
        .collect()
}

fn gemini_tool_specs_from_value(value: &Value) -> Vec<GatewayToolSpec> {
    let mut tools = Vec::new();
    for item in value.as_array().into_iter().flatten() {
        for declaration in item
            .get("functionDeclarations")
            .or_else(|| item.get("function_declarations"))
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
        {
            let Some(name) = declaration.get("name").and_then(|value| value.as_str()) else {
                continue;
            };
            if let Some(spec) = function_tool_spec(
                name,
                declaration
                    .get("description")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                declaration
                    .get("parameters")
                    .or_else(|| declaration.get("input_schema"))
                    .cloned(),
            ) {
                tools.push(spec);
            }
        }
    }
    tools
}

fn function_tool_spec(
    name: &str,
    description: Option<String>,
    schema: Option<Value>,
) -> Option<GatewayToolSpec> {
    let name = name.trim();
    (!name.is_empty()).then(|| GatewayToolSpec {
        name: name.to_string(),
        description,
        schema,
    })
}

fn set_tools_for_protocol(
    body: &mut Value,
    protocol: GatewayProtocol,
    parts: &GatewayRequestParts,
) {
    if !parts.tools.is_empty() {
        body["tools"] = Value::Array(
            parts
                .tools
                .iter()
                .map(|tool| tool_spec_for_protocol(protocol, tool))
                .collect(),
        );
    }
    if let Some(tool_choice) = parts.tool_choice.as_ref() {
        let key = match protocol {
            GatewayProtocol::GoogleGemini => "toolConfig",
            _ => "tool_choice",
        };
        if let Some(value) = tool_choice_for_protocol(protocol, tool_choice) {
            body[key] = value;
        }
    }
}

fn tool_spec_for_protocol(protocol: GatewayProtocol, tool: &GatewayToolSpec) -> Value {
    let schema = tool
        .schema
        .clone()
        .unwrap_or_else(|| json!({ "type": "object" }));
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => json!({
            "type": "function",
            "function": {
                "name": tool.name,
                "description": tool.description.clone().unwrap_or_default(),
                "parameters": schema
            }
        }),
        GatewayProtocol::OpenAiResponses => json!({
            "type": "function",
            "name": tool.name,
            "description": tool.description.clone().unwrap_or_default(),
            "parameters": schema
        }),
        GatewayProtocol::AnthropicMessages => json!({
            "name": tool.name,
            "description": tool.description.clone().unwrap_or_default(),
            "input_schema": schema
        }),
        GatewayProtocol::GoogleGemini => json!({
            "functionDeclarations": [{
                "name": tool.name,
                "description": tool.description.clone().unwrap_or_default(),
                "parameters": schema
            }]
        }),
    }
}

fn tool_choice_for_protocol(protocol: GatewayProtocol, value: &Value) -> Option<Value> {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions | GatewayProtocol::OpenAiResponses => match value {
            Value::String(choice) if choice == "none" || choice == "auto" => Some(value.clone()),
            Value::String(choice) if choice == "required" => {
                Some(Value::String("required".to_string()))
            }
            Value::Object(object) => {
                if object.get("type").and_then(|value| value.as_str()) == Some("none") {
                    Some(Value::String("none".to_string()))
                } else if object.get("type").and_then(|value| value.as_str()) == Some("auto") {
                    Some(Value::String("auto".to_string()))
                } else if object.get("type").and_then(|value| value.as_str()) == Some("any") {
                    Some(Value::String("required".to_string()))
                } else if let Some(name) = tool_choice_function_name(value) {
                    Some(json!({
                        "type": "function",
                        "function": { "name": name }
                    }))
                } else if let Some(config) = object.get("functionCallingConfig") {
                    let mode = config
                        .get("mode")
                        .and_then(|value| value.as_str())
                        .unwrap_or("AUTO");
                    if mode == "NONE" {
                        Some(Value::String("none".to_string()))
                    } else if let Some(name) = config
                        .get("allowedFunctionNames")
                        .and_then(|value| value.as_array())
                        .and_then(|values| values.first())
                        .and_then(|value| value.as_str())
                    {
                        Some(json!({
                            "type": "function",
                            "function": { "name": name }
                        }))
                    } else if mode == "ANY" {
                        Some(Value::String("required".to_string()))
                    } else {
                        Some(Value::String("auto".to_string()))
                    }
                } else {
                    Some(value.clone())
                }
            }
            _ => None,
        },
        GatewayProtocol::AnthropicMessages => match value {
            Value::String(choice) if choice == "none" => Some(json!({ "type": "none" })),
            Value::String(choice) if choice == "required" => Some(json!({ "type": "any" })),
            Value::String(choice) if choice == "auto" => Some(json!({ "type": "auto" })),
            Value::Object(_) => tool_choice_function_name(value)
                .map(|name| json!({ "type": "tool", "name": name }))
                .or_else(|| Some(value.clone())),
            _ => None,
        },
        GatewayProtocol::GoogleGemini => match value {
            Value::String(choice) => {
                let mode = match choice.as_str() {
                    "none" => "NONE",
                    "required" => "ANY",
                    _ => "AUTO",
                };
                Some(json!({ "functionCallingConfig": { "mode": mode } }))
            }
            Value::Object(_) => {
                if let Some(name) = tool_choice_function_name(value) {
                    Some(json!({
                        "functionCallingConfig": {
                            "mode": "ANY",
                            "allowedFunctionNames": [name]
                        }
                    }))
                } else {
                    Some(value.clone())
                }
            }
            _ => None,
        },
    }
}

fn tool_choice_function_name(value: &Value) -> Option<String> {
    value
        .get("function")
        .and_then(|function| function.get("name"))
        .or_else(|| value.get("name"))
        .and_then(|value| value.as_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToString::to_string)
}

fn usage_value_for_protocol(protocol: GatewayProtocol, usage: &GatewayUsage) -> Value {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let mut object = Map::new();
            object.insert("prompt_tokens".to_string(), usage.input_tokens.into());
            object.insert("completion_tokens".to_string(), usage.output_tokens.into());
            object.insert("total_tokens".to_string(), usage.total_tokens.into());
            if let Some(details) = usage_details_value(usage, true, protocol) {
                object.insert("prompt_tokens_details".to_string(), details);
            }
            if let Some(details) = usage_details_value(usage, false, protocol) {
                object.insert("completion_tokens_details".to_string(), details);
            }
            Value::Object(object)
        }
        GatewayProtocol::OpenAiResponses => {
            let mut object = Map::new();
            object.insert("input_tokens".to_string(), usage.input_tokens.into());
            object.insert("output_tokens".to_string(), usage.output_tokens.into());
            object.insert("total_tokens".to_string(), usage.total_tokens.into());
            if let Some(details) = usage_details_value(usage, true, protocol) {
                object.insert("input_tokens_details".to_string(), details);
            }
            if let Some(details) = usage_details_value(usage, false, protocol) {
                object.insert("output_tokens_details".to_string(), details);
            }
            Value::Object(object)
        }
        GatewayProtocol::AnthropicMessages => {
            let mut object = Map::new();
            object.insert("input_tokens".to_string(), usage.input_tokens.into());
            object.insert("output_tokens".to_string(), usage.output_tokens.into());
            if let Some(value) = usage.cache_creation_input_tokens {
                object.insert("cache_creation_input_tokens".to_string(), value.into());
            }
            if let Some(value) = usage.cache_read_input_tokens.or(usage.cached_input_tokens) {
                object.insert("cache_read_input_tokens".to_string(), value.into());
            }
            Value::Object(object)
        }
        GatewayProtocol::GoogleGemini => {
            let mut object = Map::new();
            object.insert("promptTokenCount".to_string(), usage.input_tokens.into());
            object.insert(
                "candidatesTokenCount".to_string(),
                usage.output_tokens.into(),
            );
            object.insert("totalTokenCount".to_string(), usage.total_tokens.into());
            if let Some(value) = usage.cached_input_tokens {
                object.insert("cachedContentTokenCount".to_string(), value.into());
            }
            if let Some(value) = usage.raw_prompt_details.clone() {
                object.insert("promptTokensDetails".to_string(), value);
            }
            if let Some(value) = usage.raw_completion_details.clone() {
                object.insert("candidatesTokensDetails".to_string(), value);
            }
            Value::Object(object)
        }
    }
}

fn usage_details_value(
    usage: &GatewayUsage,
    input: bool,
    protocol: GatewayProtocol,
) -> Option<Value> {
    let raw = if input {
        usage.raw_prompt_details.clone()
    } else {
        usage.raw_completion_details.clone()
    };
    let mut object = raw
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    if input {
        if let Some(value) = usage.cached_input_tokens {
            object.insert("cached_tokens".to_string(), value.into());
        }
        if let Some(value) = usage.audio_input_tokens {
            object.insert("audio_tokens".to_string(), value.into());
        }
        if let Some(value) = usage.image_input_tokens {
            object.insert("image_tokens".to_string(), value.into());
        }
    } else {
        if let Some(value) = usage.reasoning_tokens {
            object.insert("reasoning_tokens".to_string(), value.into());
        }
        if let Some(value) = usage.audio_output_tokens {
            object.insert("audio_tokens".to_string(), value.into());
        }
        if let Some(value) = usage.image_output_tokens {
            object.insert("image_tokens".to_string(), value.into());
        }
    }

    if protocol == GatewayProtocol::OpenAiResponses && object.is_empty() {
        return None;
    }
    (!object.is_empty()).then_some(Value::Object(object))
}

fn nested_u64(value: &Value, path: &[&str]) -> Option<u64> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_u64()
}

fn modality_tokens(value: Option<&Value>, modality: &str) -> Option<u64> {
    let total = value?
        .as_array()?
        .iter()
        .filter(|item| {
            item.get("modality")
                .and_then(|value| value.as_str())
                .map(|value| value.eq_ignore_ascii_case(modality))
                .unwrap_or(false)
        })
        .filter_map(|item| item.get("tokenCount").and_then(|value| value.as_u64()))
        .sum::<u64>();
    (total > 0).then_some(total)
}

fn text_from_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_string(),
        Value::Array(items) => items
            .iter()
            .map(text_from_value)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => {
            for key in ["text", "input_text", "output_text"] {
                if let Some(value) = object.get(key) {
                    let text = text_from_value(value);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
            for key in ["content", "parts", "message"] {
                if let Some(value) = object.get(key) {
                    let text = text_from_value(value);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn normalize_message_role(role: &str) -> String {
    match role {
        "assistant" | "model" => "assistant".to_string(),
        "tool" | "function" => "tool".to_string(),
        _ => "user".to_string(),
    }
}

fn append_system(system: &mut Option<String>, content: String) {
    match system {
        Some(existing) if !existing.is_empty() => {
            existing.push('\n');
            existing.push_str(&content);
        }
        _ => *system = Some(content),
    }
}

fn numeric_field(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|item| item.as_u64()))
}

fn set_optional_u64(value: &mut Value, key: &str, item: Option<u64>) {
    if let Some(item) = item {
        value[key] = Value::Number(item.into());
    }
}

fn set_optional_value(value: &mut Value, key: &str, item: Option<Value>) {
    if let Some(item) = item {
        if !item.is_null() {
            value[key] = item;
        }
    }
}

fn forward_upstream_json_with_headers(
    endpoint: &str,
    headers: &str,
    request_body: &serde_json::Value,
    timeout_seconds: u16,
) -> RouteResponse {
    match upstream_http::post_json_with_headers(endpoint, headers, request_body, timeout_seconds) {
        Ok(response) => {
            let status = response.status;
            let reason = reason_for_status(status);
            let content_type = response.content_type;

            RouteResponse::Buffered(HttpResponse {
                status,
                reason,
                content_type,
                headers: Vec::new(),
                body: response.body,
            })
        }
        Err(err) => RouteResponse::Buffered(json_response(
            502,
            "Bad Gateway",
            json!({
                "error": {
                    "message": format!("Upstream request failed: {err}"),
                    "type": "codestudio_upstream_request_error"
                }
            }),
        )),
    }
}

fn profile_model(profile: &ProfileDraft) -> Option<String> {
    let model = profile.model.trim();
    if model.is_empty() {
        None
    } else {
        Some(model.to_string())
    }
}

fn reason_for_status(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Upstream Response",
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn json_response(status: u16, reason: &'static str, value: serde_json::Value) -> HttpResponse {
    HttpResponse {
        status,
        reason,
        content_type: "application/json",
        headers: Vec::new(),
        body: serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec()),
    }
}

fn empty_response(status: u16, reason: &'static str) -> HttpResponse {
    HttpResponse {
        status,
        reason,
        content_type: "text/plain",
        headers: Vec::new(),
        body: Vec::new(),
    }
}

fn stream_converted_gateway_response(
    endpoint: &str,
    headers: &str,
    request_body: &Value,
    timeout_seconds: u16,
    upstream_protocol: GatewayProtocol,
    client_protocol: GatewayProtocol,
    model: &str,
    client_stream: &mut TcpStream,
) -> Result<u16, String> {
    let mut status = 200;
    let mut raw_passthrough = false;
    let mut buffered_non_sse = false;
    let mut non_sse_body = Vec::new();
    let mut client_response_started = false;
    let mut parser = SseBuffer::default();
    let mut full_text = String::new();
    let mut full_tool_calls = Vec::new();
    let mut stream_usage = GatewayUsage::default();
    let stream_state = ClientStreamState::new(client_protocol, model);

    let upstream_result = upstream_http::post_json_stream_with_headers(
        endpoint,
        headers,
        request_body,
        timeout_seconds,
        |event| match event {
            upstream_http::UpstreamStreamEvent::Headers(meta) => {
                status = meta.status;
                if meta.status >= 400 {
                    raw_passthrough = true;
                    client_response_started = true;
                    return write_stream_headers(
                        client_stream,
                        meta.status,
                        reason_for_status(meta.status),
                        meta.content_type,
                    );
                }
                if meta.content_type != "text/event-stream" {
                    buffered_non_sse = true;
                    return Ok(());
                }
                client_response_started = true;
                write_stream_headers(
                    client_stream,
                    meta.status,
                    reason_for_status(meta.status),
                    "text/event-stream",
                )?;
                write_protocol_stream_start(client_stream, &stream_state)
            }
            upstream_http::UpstreamStreamEvent::Chunk(chunk) => {
                if raw_passthrough {
                    return client_stream
                        .write_all(chunk)
                        .and_then(|_| client_stream.flush())
                        .map_err(|err| err.to_string());
                }
                if buffered_non_sse {
                    non_sse_body.extend_from_slice(chunk);
                    return Ok(());
                }
                for frame in parser.push_chunk(chunk) {
                    write_converted_stream_frame(
                        client_stream,
                        &stream_state,
                        upstream_protocol,
                        &frame,
                        &mut full_text,
                        &mut full_tool_calls,
                        &mut stream_usage,
                    )?;
                }
                Ok(())
            }
        },
    );

    if let Err(err) = upstream_result {
        if !client_response_started {
            return write_upstream_stream_error_response(client_stream, &err);
        }
        return Err(err);
    }

    if raw_passthrough {
        return Ok(status);
    }

    if buffered_non_sse {
        let value = match serde_json::from_slice::<Value>(&non_sse_body) {
            Ok(value) => value,
            Err(err) => {
                return write_upstream_stream_error_response(
                    client_stream,
                    &format!("Upstream non-SSE stream response was not valid JSON: {err}"),
                );
            }
        };
        let response = assistant_response_from_protocol(upstream_protocol, &value);
        let text = content_text(&response.content);
        write_stream_headers(
            client_stream,
            status,
            reason_for_status(status),
            "text/event-stream",
        )?;
        write_protocol_stream_start(client_stream, &stream_state)?;
        if !text.is_empty() {
            write_protocol_stream_delta(client_stream, &stream_state, &text)?;
        }
        for (index, tool_call) in response.tool_calls.iter().enumerate() {
            write_protocol_stream_tool_call(client_stream, &stream_state, tool_call, index)?;
        }
        write_protocol_stream_done(
            client_stream,
            &stream_state,
            &text,
            &response.tool_calls,
            &response.usage,
        )?;
        return Ok(status);
    }

    for frame in parser.finish() {
        write_converted_stream_frame(
            client_stream,
            &stream_state,
            upstream_protocol,
            &frame,
            &mut full_text,
            &mut full_tool_calls,
            &mut stream_usage,
        )?;
    }
    write_protocol_stream_done(
        client_stream,
        &stream_state,
        &full_text,
        &full_tool_calls,
        &stream_usage,
    )?;
    Ok(status)
}

fn write_converted_stream_frame(
    client_stream: &mut TcpStream,
    stream_state: &ClientStreamState,
    upstream_protocol: GatewayProtocol,
    frame: &SseFrame,
    full_text: &mut String,
    full_tool_calls: &mut Vec<GatewayToolCall>,
    stream_usage: &mut GatewayUsage,
) -> Result<(), String> {
    if frame.data.trim() == "[DONE]" {
        return Ok(());
    }
    let Ok(value) = serde_json::from_str::<Value>(&frame.data) else {
        return Ok(());
    };
    let update = stream_update_from_event(upstream_protocol, frame, &value);
    if let Some(usage) = update.usage {
        merge_stream_usage(stream_usage, usage);
    }
    if !update.text_delta.is_empty() {
        full_text.push_str(&update.text_delta);
        write_protocol_stream_delta(client_stream, stream_state, &update.text_delta)?;
    }
    for tool_call in update.tool_calls {
        let index = full_tool_calls.len();
        write_protocol_stream_tool_call(client_stream, stream_state, &tool_call, index)?;
        full_tool_calls.push(tool_call);
    }
    Ok(())
}

#[allow(dead_code)]
fn stream_delta_from_event(protocol: GatewayProtocol, frame: &SseFrame, value: &Value) -> String {
    stream_text_delta_from_event(protocol, frame, value)
}

fn stream_upstream_json_with_headers(
    endpoint: &str,
    headers: &str,
    request_body: &serde_json::Value,
    timeout_seconds: u16,
    client_stream: &mut TcpStream,
) -> Result<u16, String> {
    let mut status = 200;
    let mut client_response_started = false;
    let upstream_result = upstream_http::post_json_stream_with_headers(
        endpoint,
        headers,
        request_body,
        timeout_seconds,
        |event| match event {
            upstream_http::UpstreamStreamEvent::Headers(meta) => {
                status = meta.status;
                client_response_started = true;
                write_stream_headers(
                    client_stream,
                    meta.status,
                    reason_for_status(meta.status),
                    meta.content_type,
                )
            }
            upstream_http::UpstreamStreamEvent::Chunk(chunk) => client_stream
                .write_all(chunk)
                .and_then(|_| client_stream.flush())
                .map_err(|err| err.to_string()),
        },
    );

    if let Err(err) = upstream_result {
        if !client_response_started {
            return write_upstream_stream_error_response(client_stream, &err);
        }
        return Err(err);
    }

    Ok(status)
}

fn write_upstream_stream_error_response(stream: &mut TcpStream, err: &str) -> Result<u16, String> {
    write_http_response(
        stream,
        json_response(
            502,
            "Bad Gateway",
            json!({
                "error": {
                    "message": format!("Upstream request failed: {err}"),
                    "type": "codestudio_upstream_request_error"
                }
            }),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn gateway_facade_does_not_reclaim_extracted_module_ownership() {
        let facade = include_str!("gateway.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("gateway production source");
        for (prefix, suffix) in [
            ("fn read_http_", "request("),
            ("fn find_header_", "end("),
            ("fn parse_content_", "length("),
            ("fn write_client_stream_", "delta("),
            ("fn write_client_stream_", "tool_call("),
            ("fn write_client_stream_", "done("),
            ("struct Gateway", "Runtime"),
            ("static RUN", "TIME:"),
            ("listener.", "accept()"),
        ] {
            let forbidden = format!("{prefix}{suffix}");
            assert!(
                !facade.contains(&forbidden),
                "gateway facade reclaimed extracted ownership: {forbidden}"
            );
        }

        for module in [
            include_str!("gateway/server.rs"),
            include_str!("gateway/runtime.rs"),
            include_str!("gateway/protocol/stream.rs"),
        ] {
            assert!(!module.trim().is_empty());
        }
    }

    fn test_config(auth_enabled: bool) -> GatewayConfig {
        GatewayConfig {
            token: "codestudio-local-testtoken1234567890".to_string(),
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            auth_enabled,
            model_override: true,
            privacy_filter_mode: PrivacyFilterMode::Off,
        }
    }

    fn test_config_with_privacy_filter(
        auth_enabled: bool,
        privacy_filter_mode: PrivacyFilterMode,
    ) -> GatewayConfig {
        GatewayConfig {
            privacy_filter_mode,
            ..test_config(auth_enabled)
        }
    }

    fn post(path: &str, token: Option<&str>) -> HttpRequest {
        let mut headers = HashMap::new();
        if let Some(token) = token {
            headers.insert("authorization".to_string(), format!("Bearer {token}"));
        }

        HttpRequest {
            method: "POST".to_string(),
            path: path.to_string(),
            headers,
            body: br#"{"model":"codestudio-default","input":"ping"}"#.to_vec(),
        }
    }

    fn get(path: &str, token: Option<&str>) -> HttpRequest {
        let mut headers = HashMap::new();
        if let Some(token) = token {
            headers.insert("authorization".to_string(), format!("Bearer {token}"));
        }

        HttpRequest {
            method: "GET".to_string(),
            path: path.to_string(),
            headers,
            body: Vec::new(),
        }
    }

    fn options(path: &str) -> HttpRequest {
        HttpRequest {
            method: "OPTIONS".to_string(),
            path: path.to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }

    fn response_body_json(response: RouteResponse) -> Value {
        match response {
            RouteResponse::Buffered(response) => {
                serde_json::from_slice(&response.body).expect("response body should be JSON")
            }
            RouteResponse::Stream(_) => panic!("expected buffered response"),
        }
    }

    fn assert_not_skeleton_route_not_found(response: RouteResponse, route: &str) -> u16 {
        let status = response.status();
        if status != 404 {
            return status;
        }

        let body = response_body_json(response);
        assert_ne!(
            body["error"]["type"].as_str(),
            Some("codestudio_route_not_found"),
            "{route} should not fall through to the local route fallback"
        );
        status
    }

    fn connected_stream_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should expose an address");
        let client = TcpStream::connect(address).expect("test client should connect");
        let (server, _) = listener.accept().expect("test server should accept");
        (server, client)
    }

    fn test_profile(protocol: &str) -> ProfileDraft {
        ProfileDraft {
            id: "profile-test".to_string(),
            name: "Test".to_string(),
            icon: None,
            remark: None,
            app: "codex".to_string(),
            is_builtin: false,
            mode: Default::default(),
            provider: "test-provider".to_string(),
            protocol: protocol.to_string(),
            model: "test-model".to_string(),
            review_model: None,
            model_mappings: Vec::new(),
            base_url: "https://api.example.test/v1".to_string(),
            auth_ref: Some("test-key".to_string()),
            created_at: None,
            updated_at: None,
            last_test_status: None,
            usage_enabled: false,
            sort_order: 0,
        }
    }

    fn test_profile_with_base_url(protocol: &str, base_url: &str) -> ProfileDraft {
        ProfileDraft {
            base_url: base_url.to_string(),
            ..test_profile(protocol)
        }
    }

    #[test]
    fn claude_code_model_items_use_profile_mappings_and_1m_flags() {
        let mut profile = test_profile(PROTOCOL_ANTHROPIC_MESSAGES);
        profile.app = "claude".to_string();
        profile.model_mappings = vec![
            crate::core::types::ProfileModelMapping {
                alias: "claude-sonnet-4-6".to_string(),
                model: "provider-sonnet".to_string(),
                supports_1m: true,
                description: Some("Provider Sonnet".to_string()),
            },
            crate::core::types::ProfileModelMapping {
                alias: "claude-haiku-4-5".to_string(),
                model: "provider-haiku".to_string(),
                supports_1m: false,
                description: None,
            },
        ];

        let items = claude_code_model_items(&profile);

        assert_eq!(items[0]["id"].as_str(), Some("claude-sonnet-4-6"));
        assert_eq!(items[0]["supports1m"].as_bool(), Some(true));
        assert_eq!(items[0]["display_name"].as_str(), Some("Provider Sonnet"));
        assert_eq!(items[1]["id"].as_str(), Some("claude-haiku-4-5"));
        assert!(items[1]["supports1m"].is_null());
    }

    #[test]
    fn effective_upstream_model_maps_claude_code_alias_to_provider_model() {
        let mut profile = test_profile(PROTOCOL_ANTHROPIC_MESSAGES);
        profile.app = "claude".to_string();
        profile.model = String::new();
        profile.model_mappings = vec![crate::core::types::ProfileModelMapping {
            alias: "claude-sonnet-4-6".to_string(),
            model: "provider-sonnet".to_string(),
            supports_1m: true,
            description: None,
        }];
        let config = GatewayConfig {
            model_override: false,
            ..test_config(false)
        };

        assert_eq!(
            effective_upstream_model(&json!({ "model": "claude-sonnet-4-6" }), &profile, &config),
            "provider-sonnet"
        );
    }

    fn header_map(items: &[(&str, &str)]) -> HashMap<String, String> {
        items
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    fn contains_header(headers: &str, name: &str, value: &str) -> bool {
        headers.lines().filter_map(|line| line.split_once(':')).any(
            |(header_name, header_value)| {
                header_name.eq_ignore_ascii_case(name) && header_value.trim() == value
            },
        )
    }

    fn header_value(headers: &str, name: &str) -> Option<String> {
        headers
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim().to_string())
    }

    #[test]
    fn chat_request_can_convert_to_anthropic_messages_body() {
        let request = json!({
            "model": "codestudio-default",
            "messages": [
                { "role": "system", "content": "Be concise." },
                { "role": "user", "content": "Ping" }
            ],
            "max_tokens": 64,
            "temperature": 0.2
        });
        let parts =
            request_parts_from_client(GatewayProtocol::OpenAiChatCompletions, &request, "claude");
        let body = request_body_for_protocol(GatewayProtocol::AnthropicMessages, &parts, false);

        assert_eq!(body["model"].as_str(), Some("claude"));
        assert_eq!(body["system"].as_str(), Some("Be concise."));
        assert_eq!(body["max_tokens"].as_u64(), Some(64));
        assert_eq!(body["messages"][0]["role"].as_str(), Some("user"));
        assert_eq!(body["messages"][0]["content"].as_str(), Some("Ping"));
        assert_eq!(body["temperature"].as_f64(), Some(0.2));
    }

    #[test]
    fn anthropic_response_can_convert_to_responses_body() {
        let upstream = json!({
            "content": [{ "type": "text", "text": "Pong" }],
            "usage": {
                "input_tokens": 3,
                "output_tokens": 5
            }
        });
        let converted = convert_gateway_response(
            GatewayProtocol::AnthropicMessages,
            GatewayProtocol::OpenAiResponses,
            &upstream,
            "claude",
        )
        .expect("response should convert");

        assert_eq!(converted["object"].as_str(), Some("response"));
        assert_eq!(converted["output_text"].as_str(), Some("Pong"));
        assert_eq!(converted["usage"]["input_tokens"].as_u64(), Some(3));
        assert_eq!(converted["usage"]["output_tokens"].as_u64(), Some(5));
        assert_eq!(converted["usage"]["total_tokens"].as_u64(), Some(8));
    }

    #[test]
    fn chat_multimodal_request_converts_to_anthropic_blocks() {
        let request = json!({
            "model": "codestudio-default",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "Describe this." },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/png;base64,AAAA"
                        }
                    }
                ]
            }]
        });
        let parts =
            request_parts_from_client(GatewayProtocol::OpenAiChatCompletions, &request, "claude");
        let body = request_body_for_protocol(GatewayProtocol::AnthropicMessages, &parts, false);

        assert_eq!(
            body["messages"][0]["content"][0]["text"].as_str(),
            Some("Describe this.")
        );
        assert_eq!(
            body["messages"][0]["content"][1]["source"]["type"].as_str(),
            Some("base64")
        );
        assert_eq!(
            body["messages"][0]["content"][1]["source"]["media_type"].as_str(),
            Some("image/png")
        );
    }

    #[test]
    fn anthropic_tool_use_response_converts_to_chat_tool_calls() {
        let upstream = json!({
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "lookup_docs",
                "input": { "query": "gateway" }
            }],
            "stop_reason": "tool_use",
            "usage": {
                "input_tokens": 11,
                "output_tokens": 7,
                "cache_read_input_tokens": 3
            }
        });
        let converted = convert_gateway_response(
            GatewayProtocol::AnthropicMessages,
            GatewayProtocol::OpenAiChatCompletions,
            &upstream,
            "claude",
        )
        .expect("response should convert");

        assert_eq!(
            converted["choices"][0]["finish_reason"].as_str(),
            Some("tool_calls")
        );
        assert_eq!(
            converted["choices"][0]["message"]["tool_calls"][0]["function"]["name"].as_str(),
            Some("lookup_docs")
        );
        assert!(
            converted["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
                .as_str()
                .unwrap_or_default()
                .contains("gateway")
        );
        assert_eq!(
            converted["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64(),
            Some(3)
        );
    }

    #[test]
    fn gemini_function_call_response_converts_to_responses_function_call() {
        let upstream = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "search",
                            "args": { "q": "codex" }
                        }
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 4,
                "candidatesTokenCount": 2,
                "totalTokenCount": 6
            }
        });
        let converted = convert_gateway_response(
            GatewayProtocol::GoogleGemini,
            GatewayProtocol::OpenAiResponses,
            &upstream,
            "gemini",
        )
        .expect("response should convert");

        assert_eq!(
            converted["output"][0]["type"].as_str(),
            Some("function_call")
        );
        assert_eq!(converted["output"][0]["name"].as_str(), Some("search"));
        assert!(converted["output"][0]["arguments"]
            .as_str()
            .unwrap_or_default()
            .contains("codex"));
        assert_eq!(converted["usage"]["total_tokens"].as_u64(), Some(6));
    }

    #[test]
    fn responses_usage_details_convert_to_chat_usage_details() {
        let upstream = json!({
            "output_text": "ok",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 4,
                "total_tokens": 14,
                "input_tokens_details": {
                    "cached_tokens": 6,
                    "audio_tokens": 1
                },
                "output_tokens_details": {
                    "reasoning_tokens": 2
                }
            }
        });
        let converted = convert_gateway_response(
            GatewayProtocol::OpenAiResponses,
            GatewayProtocol::OpenAiChatCompletions,
            &upstream,
            "gpt",
        )
        .expect("response should convert");

        assert_eq!(converted["usage"]["prompt_tokens"].as_u64(), Some(10));
        assert_eq!(
            converted["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64(),
            Some(6)
        );
        assert_eq!(
            converted["usage"]["prompt_tokens_details"]["audio_tokens"].as_u64(),
            Some(1)
        );
        assert_eq!(
            converted["usage"]["completion_tokens_details"]["reasoning_tokens"].as_u64(),
            Some(2)
        );
    }

    #[test]
    fn gemini_generate_content_route_extracts_model() {
        assert_eq!(
            gemini_route_from_path("/v1beta/models/gemini-2.5-pro:generateContent"),
            Some(("gemini-2.5-pro".to_string(), false))
        );
        assert_eq!(
            gemini_route_from_path("/v1/models/gemini-2.5-flash:generateContent"),
            Some(("gemini-2.5-flash".to_string(), false))
        );
        assert_eq!(
            gemini_route_from_path("/v1beta/models/gemini-2.5-pro:streamGenerateContent"),
            Some(("gemini-2.5-pro".to_string(), true))
        );
        assert!(gemini_route_from_path("/v1/models").is_none());
    }

    #[test]
    fn stream_request_body_sets_upstream_stream_flag() {
        let request = json!({
            "model": "codestudio-default",
            "input": "Ping"
        });
        let parts = request_parts_from_client(GatewayProtocol::OpenAiResponses, &request, "claude");
        let body = request_body_for_protocol(GatewayProtocol::AnthropicMessages, &parts, true);

        assert_eq!(body["stream"].as_bool(), Some(true));
        assert_eq!(body["messages"][0]["content"].as_str(), Some("Ping"));
    }

    #[test]
    fn upstream_headers_pass_through_safe_custom_context_headers() {
        let request_headers = header_map(&[
            ("x-provider-model-family", "gpt-5"),
            ("anthropic-beta", "tools-2025-01-01"),
            ("openai-beta", "responses=v1"),
            ("cf-ray", "abc123"),
            ("helicone-auth", "Bearer helicone-test"),
            ("http-referer", "https://codestudio.design"),
            ("user-agent", "Codex CLI"),
        ]);
        let headers = upstream_headers_with_passthrough(
            GatewayProtocol::OpenAiResponses,
            "upstream-key",
            &request_headers,
        );

        assert!(contains_header(
            &headers,
            "x-provider-model-family",
            "gpt-5"
        ));
        assert!(contains_header(
            &headers,
            "anthropic-beta",
            "tools-2025-01-01"
        ));
        assert!(contains_header(&headers, "openai-beta", "responses=v1"));
        assert!(contains_header(&headers, "cf-ray", "abc123"));
        assert!(contains_header(
            &headers,
            "helicone-auth",
            "Bearer helicone-test"
        ));
        assert!(contains_header(
            &headers,
            "http-referer",
            "https://codestudio.design"
        ));
        assert!(contains_header(&headers, "user-agent", "Codex CLI"));
    }

    #[test]
    fn upstream_headers_do_not_pass_through_auth_transport_or_protocol_headers() {
        let request_headers = header_map(&[
            ("authorization", "Bearer local-token"),
            ("host", "127.0.0.1:43112"),
            ("content-length", "999"),
            ("connection", "keep-alive"),
            ("content-type", "text/plain"),
            ("accept", "text/plain"),
            ("x-api-key", "incoming-anthropic-key"),
            ("x-goog-api-key", "incoming-gemini-key"),
            ("anthropic-version", "2099-01-01"),
            ("x-codestudio-tool", "codex"),
            ("x-custom-safe", "ok"),
        ]);
        let headers = upstream_headers_with_passthrough(
            GatewayProtocol::AnthropicMessages,
            "upstream-key",
            &request_headers,
        );

        assert_eq!(
            header_value(&headers, "x-api-key").as_deref(),
            Some("upstream-key")
        );
        assert_eq!(
            header_value(&headers, "anthropic-version").as_deref(),
            Some("2023-06-01")
        );
        assert_eq!(
            header_value(&headers, "content-type").as_deref(),
            Some("application/json")
        );
        assert_eq!(
            header_value(&headers, "accept").as_deref(),
            Some("application/json")
        );
        assert!(contains_header(&headers, "x-custom-safe", "ok"));
        assert!(!contains_header(
            &headers,
            "authorization",
            "Bearer local-token"
        ));
        assert!(!contains_header(&headers, "host", "127.0.0.1:43112"));
        assert!(!contains_header(&headers, "content-length", "999"));
        assert!(!contains_header(&headers, "connection", "keep-alive"));
        assert!(!contains_header(
            &headers,
            "x-goog-api-key",
            "incoming-gemini-key"
        ));
        assert!(!contains_header(&headers, "x-codestudio-tool", "codex"));
    }

    #[test]
    fn upstream_headers_reject_header_injection_and_unknown_headers() {
        let request_headers = header_map(&[
            ("x-safe", "ok"),
            ("x-injected", "hello\r\nAuthorization: Bearer leaked"),
            ("forwarded", "for=127.0.0.1"),
            ("bad name", "nope"),
        ]);
        let headers = upstream_headers_with_passthrough(
            GatewayProtocol::OpenAiChatCompletions,
            "upstream-key",
            &request_headers,
        );

        assert!(contains_header(&headers, "x-safe", "ok"));
        assert!(!headers.contains("Bearer leaked"));
        assert!(!contains_header(&headers, "forwarded", "for=127.0.0.1"));
        assert!(!contains_header(&headers, "bad name", "nope"));
    }

    #[test]
    fn stream_upstream_pre_header_error_returns_502_instead_of_disconnect() {
        let (mut server, mut client) = connected_stream_pair();
        let status = stream_upstream_json_with_headers(
            "not-a-url",
            "",
            &json!({ "stream": true }),
            1,
            &mut server,
        )
        .expect("pre-header upstream failures should be serialized as a gateway response");
        drop(server);

        let mut raw_response = String::new();
        client
            .read_to_string(&mut raw_response)
            .expect("client should read fallback response");

        assert_eq!(status, 502);
        assert!(raw_response.starts_with("HTTP/1.1 502 Bad Gateway"));
        assert!(raw_response.contains("codestudio_upstream_request_error"));
        assert!(raw_response.contains("Upstream request failed"));
    }

    #[test]
    fn converted_gateway_request_uses_safe_passthrough_headers() {
        let request_body = json!({
            "model": "codestudio-default",
            "messages": [{ "role": "user", "content": "Ping" }]
        });
        let request_headers = header_map(&[
            ("x-provider-model-family", "claude"),
            ("authorization", "Bearer local-token"),
            ("anthropic-version", "2099-01-01"),
        ]);
        let profile = test_profile(PROTOCOL_ANTHROPIC_MESSAGES);
        let converted = convert_gateway_request(
            GatewayProtocol::OpenAiChatCompletions,
            GatewayProtocol::AnthropicMessages,
            &request_body,
            &profile,
            &test_config(false),
            "upstream-key",
            &request_headers,
            false,
        );

        assert!(contains_header(
            &converted.headers,
            "x-provider-model-family",
            "claude"
        ));
        assert_eq!(
            header_value(&converted.headers, "x-api-key").as_deref(),
            Some("upstream-key")
        );
        assert_eq!(
            header_value(&converted.headers, "anthropic-version").as_deref(),
            Some("2023-06-01")
        );
        assert!(!contains_header(
            &converted.headers,
            "authorization",
            "Bearer local-token"
        ));
    }

    #[test]
    fn privacy_filter_redacts_request_body_before_protocol_conversion() {
        let mut request_body = json!({
            "model": "codestudio-default",
            "messages": [{
                "role": "user",
                "content": "Send alice@example.com with token sk-test1234567890abcdef"
            }]
        });
        let config = test_config_with_privacy_filter(false, PrivacyFilterMode::Redact);
        let result = apply_gateway_privacy_filter(
            GatewayProtocol::OpenAiChatCompletions,
            &mut request_body,
            &config,
        );

        assert!(result.is_ok());
        let parts = request_parts_from_client(
            GatewayProtocol::OpenAiChatCompletions,
            &request_body,
            "claude",
        );
        let body = request_body_for_protocol(GatewayProtocol::AnthropicMessages, &parts, false);
        let content = body["messages"][0]["content"].as_str().unwrap();

        assert!(content.contains("[邮箱]"));
        assert!(content.contains("[密钥]"));
        assert!(!content.contains("alice@example.com"));
        assert!(!content.contains("sk-test"));
    }

    #[test]
    fn privacy_filter_block_mode_rejects_sensitive_request_body() {
        let mut request_body = json!({
            "input": "My email is alice@example.com"
        });
        let config = test_config_with_privacy_filter(false, PrivacyFilterMode::Block);
        let result = apply_gateway_privacy_filter(
            GatewayProtocol::OpenAiResponses,
            &mut request_body,
            &config,
        );

        assert!(result.is_err());
        let response = result.err().unwrap();
        assert_eq!(response.status(), 400);
        let body = response_body_json(response);
        assert_eq!(
            body["error"]["type"].as_str(),
            Some("privacy_filter_blocked")
        );
    }

    #[test]
    fn privacy_filter_block_mode_allows_clean_latest_input_after_sensitive_history() {
        let mut request_body = json!({
            "model": "gpt",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "My API key is sk-11111" }]
                },
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "I cannot see it." }]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "1" }]
                }
            ]
        });
        let config = test_config_with_privacy_filter(false, PrivacyFilterMode::Block);
        let result = apply_gateway_privacy_filter(
            GatewayProtocol::OpenAiResponses,
            &mut request_body,
            &config,
        );

        assert!(
            result.is_ok(),
            "clean latest input should not be blocked by sensitive history"
        );
        let history_text = request_body["input"][0]["content"][0]["text"]
            .as_str()
            .unwrap();
        let latest_text = request_body["input"][2]["content"][0]["text"]
            .as_str()
            .unwrap();
        assert!(history_text.contains("[密钥]"));
        assert!(!history_text.contains("sk-11111"));
        assert_eq!(latest_text, "1");
    }

    #[test]
    fn privacy_filter_metadata_records_action_without_prompt_content() {
        let mut context = RequestLogContext {
            client: "Codex".to_string(),
            method: "POST".to_string(),
            path: "/v1/responses".to_string(),
            provider: Some("test-provider".to_string()),
            model: Some("test-model".to_string()),
            privacy_filter_mode: PrivacyFilterMode::Redact,
            privacy_filter_hit_count: 2,
            privacy_filter_action: PrivacyFilterAction::Redacted,
        };

        let entry = gateway_request_log_entry(&mut context, 200, Duration::from_millis(12), None);

        assert_eq!(entry.privacy_filter_mode, PrivacyFilterMode::Redact);
        assert_eq!(entry.privacy_filter_hit_count, 2);
        assert_eq!(entry.privacy_filter_action, PrivacyFilterAction::Redacted);
        let serialized = serde_json::to_string(&entry).expect("entry should serialize");
        assert!(!serialized.contains("alice@example.com"));
        assert!(!serialized.contains("sk-test"));
    }

    #[test]
    fn privacy_filter_metadata_detects_all_client_protocol_shapes() {
        let config = test_config_with_privacy_filter(false, PrivacyFilterMode::Detect);
        let mut requests = vec![
            json!({
                "model": "gpt",
                "input": "alice@example.com"
            }),
            json!({
                "model": "gpt",
                "messages": [{ "role": "user", "content": "alice@example.com" }]
            }),
            json!({
                "model": "claude",
                "messages": [{ "role": "user", "content": [{ "type": "text", "text": "alice@example.com" }] }]
            }),
            json!({
                "contents": [{ "parts": [{ "text": "alice@example.com" }] }]
            }),
        ];

        for request_body in &mut requests {
            let report = match apply_gateway_privacy_filter(
                GatewayProtocol::OpenAiResponses,
                request_body,
                &config,
            ) {
                Ok(report) => report,
                Err(_) => panic!("detect mode should not block"),
            };
            assert_eq!(report.hit_count, 1);
        }
    }

    #[test]
    fn sse_buffer_parses_split_frames_and_extracts_anthropic_delta() {
        let mut buffer = SseBuffer::default();
        assert!(buffer
            .push_chunk(b"event: content_block_delta\ndata: {\"type\":\"content_")
            .is_empty());
        let frames =
            buffer.push_chunk(br#"block_delta","delta":{"type":"text_delta","text":"Hi"}}"#);
        assert!(frames.is_empty());
        let frames = buffer.push_chunk(b"\n\n");

        assert_eq!(frames.len(), 1);
        assert_eq!(
            stream_delta_from_event(
                GatewayProtocol::AnthropicMessages,
                &frames[0],
                &serde_json::from_str::<Value>(&frames[0].data).expect("json frame")
            ),
            "Hi"
        );
    }

    #[test]
    fn stream_delta_extracts_openai_and_gemini_text() {
        let chat_frame = SseFrame {
            event: None,
            data: String::new(),
        };
        let chat = json!({
            "choices": [{
                "delta": {
                    "content": "hello"
                }
            }]
        });
        assert_eq!(
            stream_delta_from_event(GatewayProtocol::OpenAiChatCompletions, &chat_frame, &chat),
            "hello"
        );

        let gemini = json!({
            "candidates": [{
                "content": {
                    "parts": [{ "text": "world" }]
                }
            }]
        });
        assert_eq!(
            stream_delta_from_event(GatewayProtocol::GoogleGemini, &chat_frame, &gemini),
            "world"
        );
    }

    #[test]
    fn stream_update_extracts_tool_call_delta_and_usage() {
        let frame = SseFrame {
            event: None,
            data: String::new(),
        };
        let value = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "lookup",
                            "arguments": "{\"q\":\"gateway\"}"
                        }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 8,
                "completion_tokens": 3,
                "total_tokens": 11,
                "completion_tokens_details": {
                    "reasoning_tokens": 2
                }
            }
        });
        let update =
            stream_update_from_event(GatewayProtocol::OpenAiChatCompletions, &frame, &value);

        assert_eq!(update.tool_calls.len(), 1);
        assert_eq!(update.tool_calls[0].id, "call_1");
        assert_eq!(update.tool_calls[0].name, "lookup");
        assert_eq!(
            update.tool_calls[0].arguments["q"].as_str(),
            Some("gateway")
        );
        assert_eq!(
            update.usage.as_ref().map(|usage| usage.total_tokens),
            Some(11)
        );
        assert_eq!(
            update.usage.and_then(|usage| usage.reasoning_tokens),
            Some(2)
        );
    }

    #[test]
    fn responses_route_requires_local_auth_when_enabled() {
        let response = route_request(post("/v1/responses", None), &test_config(true));
        assert_eq!(response.status(), 401);
    }

    #[test]
    fn responses_route_is_implemented_for_codex_wire_api() {
        let config = test_config(true);
        let response = route_request(post("/v1/responses", Some(&config.token)), &config);
        assert_not_skeleton_route_not_found(response, "/v1/responses");
    }

    #[test]
    fn options_request_returns_cors_preflight_response_without_auth() {
        let response = route_request(options("/v1/responses"), &test_config(true));
        assert_eq!(response.status(), 204);
        match response {
            RouteResponse::Buffered(response) => {
                assert_eq!(response.content_type, "text/plain");
            }
            RouteResponse::Stream(_) => panic!("expected buffered response"),
        }
    }

    #[test]
    fn extra_gateway_routes_return_specific_api_errors_instead_of_skeleton_404() {
        let config = test_config(true);
        let routes = [
            "/v1/responses/compact",
            "/v1/messages/count_tokens",
            "/v1/images/generations",
            "/v1/images/edits",
        ];

        for route in routes {
            let response = route_request(post(route, Some(&config.token)), &config);
            assert_not_skeleton_route_not_found(response, route);
        }
    }

    #[test]
    fn count_tokens_route_returns_anthropic_token_count_shape() {
        let config = test_config(true);
        let response = route_request(
            post("/v1/messages/count_tokens", Some(&config.token)),
            &config,
        );
        assert_eq!(response.status(), 200);
        let body = response_body_json(response);

        assert!(body["input_tokens"].as_u64().unwrap_or(0) > 0);
    }

    #[test]
    fn upstream_endpoint_preserves_openai_and_provider_version_roots() {
        let cases = [
            (
                "https://api.example.com",
                GatewayProtocol::OpenAiChatCompletions,
                "https://api.example.com/v1/chat/completions",
            ),
            (
                "https://api.example.com/v1/",
                GatewayProtocol::OpenAiChatCompletions,
                "https://api.example.com/v1/chat/completions",
            ),
            (
                "https://api.example.com/v1/chat/completions",
                GatewayProtocol::OpenAiChatCompletions,
                "https://api.example.com/v1/chat/completions",
            ),
            (
                "https://open.bigmodel.cn/api/coding/paas/v4",
                GatewayProtocol::OpenAiChatCompletions,
                "https://open.bigmodel.cn/api/coding/paas/v4/chat/completions",
            ),
            (
                "https://open.bigmodel.cn/api/coding/paas/v4",
                GatewayProtocol::OpenAiResponses,
                "https://open.bigmodel.cn/api/coding/paas/v4/responses",
            ),
            (
                "https://api.anthropic.com",
                GatewayProtocol::AnthropicMessages,
                "https://api.anthropic.com/messages",
            ),
            (
                "https://api.anthropic.com/v1/messages",
                GatewayProtocol::AnthropicMessages,
                "https://api.anthropic.com/v1/messages",
            ),
            (
                "https://api.example.com/v1?ignored=1",
                GatewayProtocol::OpenAiChatCompletions,
                "https://api.example.com/v1/chat/completions",
            ),
        ];

        for (base_url, protocol, expected) in cases {
            let profile = test_profile_with_base_url("unused", base_url);
            assert_eq!(
                upstream_endpoint(protocol, &profile, "test-model", false),
                expected
            );
        }
    }

    #[test]
    fn upstream_endpoint_adds_v1_only_for_openai_root_profiles() {
        let cases = [
            (
                "https://api.apikey.fun",
                GatewayProtocol::OpenAiResponses,
                "https://api.apikey.fun/v1/responses",
            ),
            (
                "https://api.apikey.fun/",
                GatewayProtocol::OpenAiResponses,
                "https://api.apikey.fun/v1/responses",
            ),
            (
                "https://api.apikey.fun/v1",
                GatewayProtocol::OpenAiResponses,
                "https://api.apikey.fun/v1/responses",
            ),
            (
                "https://generativelanguage.googleapis.com",
                GatewayProtocol::GoogleGemini,
                "https://generativelanguage.googleapis.com/models/test-model:generateContent",
            ),
            (
                "https://generativelanguage.googleapis.com/v1beta",
                GatewayProtocol::GoogleGemini,
                "https://generativelanguage.googleapis.com/v1beta/models/test-model:generateContent",
            ),
        ];

        for (base_url, protocol, expected) in cases {
            let profile = test_profile_with_base_url("unused", base_url);
            assert_eq!(
                upstream_endpoint(protocol, &profile, "test-model", false),
                expected
            );
        }
    }

    #[test]
    fn scoped_responses_route_is_implemented_for_tool_gateway_url() {
        let config = test_config(true);
        let response = route_request(
            post("/tools/codex/v1/responses", Some(&config.token)),
            &config,
        );
        assert_ne!(response.status(), 404);
        assert_ne!(response.status(), 401);
    }

    #[test]
    fn codex_scoped_route_accepts_request_without_local_token() {
        let config = test_config(true);
        let response = route_request(post("/tools/codex/v1/responses", None), &config);
        assert_ne!(response.status(), 401);
    }

    #[test]
    fn unscoped_route_rejects_request_without_local_token() {
        let config = test_config(true);
        let response = route_request(post("/v1/responses", None), &config);
        assert_eq!(response.status(), 401);
    }

    #[test]
    fn non_codex_scoped_route_rejects_request_without_local_token() {
        let config = test_config(true);
        let response = route_request(post("/tools/claude/v1/messages", None), &config);
        assert_eq!(response.status(), 401);
    }

    #[test]
    fn codex_scoped_route_requires_local_token_when_host_is_not_loopback() {
        let config = GatewayConfig {
            host: "0.0.0.0".to_string(),
            ..test_config(true)
        };
        let response = route_request(post("/tools/codex/v1/responses", None), &config);
        assert_eq!(response.status(), 401);
    }

    #[test]
    fn messages_route_is_implemented_for_anthropic_clients() {
        let config = test_config(true);
        let response = route_request(post("/v1/messages", Some(&config.token)), &config);
        let status = assert_not_skeleton_route_not_found(response, "/v1/messages");
        assert_ne!(status, 401);
    }

    #[test]
    fn claude_desktop_scoped_models_use_anthropic_catalog_shape() {
        let config = test_config(true);
        let response = route_request(
            get("/tools/claude-desktop/v1/models", Some(&config.token)),
            &config,
        );
        assert_eq!(response.status(), 200);
        let body = response_body_json(response);

        assert_eq!(body["has_more"].as_bool(), Some(false));
        assert_eq!(body["data"][0]["type"].as_str(), Some("model"));
        assert!(body["data"][0]["id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("claude-"));
    }

    #[test]
    fn client_config_for_tool_uses_scoped_base_url() {
        let config = client_config_for_tool("codex").expect("client config should render");
        assert!(config.base_url.ends_with("/tools/codex/v1"));
    }

    #[test]
    fn chatgpt_desktop_alias_uses_codex_gateway_scope() {
        assert_eq!(
            canonical_profile_tool_id("chatgpt-desktop").as_deref(),
            Some("codex")
        );
        assert_eq!(
            canonical_profile_tool_id("codex-app").as_deref(),
            Some("codex")
        );
        assert_eq!(
            canonical_profile_tool_id("codex-client").as_deref(),
            Some("codex")
        );
    }
}
