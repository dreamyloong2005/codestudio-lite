use crate::core::activity_log;
use crate::core::credentials;
use crate::core::gateway_request_log;
use crate::core::privacy_filter::{self, PrivacyFilterAction, PrivacyFilterMode};
use crate::core::profile;
use crate::core::storage;
use crate::core::types::{
    GatewayControlResult, GatewayRequestLogEntry, GatewayStatus, ProfileDraft, ProfileSummary,
    Severity, UpdateGatewaySettingsRequest,
};
use crate::core::upstream_http;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 43112;
const TOKEN_PREFIX: &str = "codestudio-local-";
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const CLIENT_PROVIDER_ID: &str = "codestudio-local";
const CLIENT_MODEL: &str = "codestudio-default";
const TOOL_SCOPED_PREFIX: &str = "/tools/";
const UPSTREAM_TIMEOUT_SECONDS: u16 = 120;
const PROTOCOL_OPENAI_CHAT_COMPLETIONS: &str = "openai-chat-completions";
const PROTOCOL_OPENAI_RESPONSES: &str = "openai-responses";
const PROTOCOL_ANTHROPIC_MESSAGES: &str = "anthropic-messages";
const PROTOCOL_GOOGLE_GEMINI: &str = "google-gemini";
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

#[derive(Default)]
struct GatewayRuntime {
    shutdown: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
    started_at: Option<String>,
    last_error: Option<String>,
}

static RUNTIME: OnceLock<Mutex<GatewayRuntime>> = OnceLock::new();
static GATEWAY_CONFIG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn status_gateway() -> Result<GatewayStatus, String> {
    profile::ensure_app_dirs()?;
    let config = load_or_create_gateway_config()?;
    build_status(&config)
}

pub fn start_gateway() -> Result<GatewayControlResult, String> {
    profile::ensure_app_dirs()?;
    let config = load_or_create_gateway_config()?;
    let runtime = runtime();

    {
        let guard = runtime.lock().map_err(|err| err.to_string())?;
        if guard.shutdown.is_some() {
            drop(guard);
            apply_gateway_native_configs_after_start()?;
            return Ok(GatewayControlResult {
                status: build_status(&config)?,
            });
        }
    }

    let address = format!("{}:{}", config.host, config.port);
    let listener = match TcpListener::bind(&address) {
        Ok(listener) => listener,
        Err(err) => {
            let message = format!("Could not start gateway on {address}: {err}");
            set_last_error(Some(message.clone()));
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
    let handle = thread::spawn(move || loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let server_config =
                    load_or_create_gateway_config().unwrap_or_else(|_| fallback_config.clone());
                thread::spawn(move || {
                    let _ = handle_connection(stream, &server_config);
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(35));
            }
            Err(err) => {
                set_last_error(Some(format!("Gateway accept failed: {err}")));
                thread::sleep(Duration::from_millis(100));
            }
        }
    });

    {
        let mut guard = runtime.lock().map_err(|err| err.to_string())?;
        guard.shutdown = Some(shutdown_tx);
        guard.handle = Some(handle);
        guard.started_at = Some(started_at);
        guard.last_error = None;
    }

    if let Err(err) = apply_gateway_native_configs_after_start() {
        let _ = stop_gateway_runtime(false);
        let message = format!("Started Local Gateway but could not update client configs: {err}");
        set_last_error(Some(message.clone()));
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
    let tool_id = normalize_gateway_tool_id(tool_id)
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

fn runtime() -> &'static Mutex<GatewayRuntime> {
    RUNTIME.get_or_init(|| Mutex::new(GatewayRuntime::default()))
}

fn set_last_error(message: Option<String>) {
    if let Ok(mut guard) = runtime().lock() {
        guard.last_error = message;
    }
}

fn is_gateway_running() -> Result<bool, String> {
    runtime()
        .lock()
        .map(|guard| guard.shutdown.is_some())
        .map_err(|err| err.to_string())
}

fn stop_gateway_runtime(log_stop: bool) -> Result<bool, String> {
    let handle = {
        let mut guard = runtime().lock().map_err(|err| err.to_string())?;
        if let Some(shutdown) = guard.shutdown.take() {
            let _ = shutdown.send(());
        }
        guard.started_at = None;
        guard.handle.take()
    };

    if let Some(handle) = handle {
        let _ = handle.join();
        if log_stop {
            activity_log::append(
                Severity::Info,
                "Stopped Local Gateway. Connected AI clients will fail until it starts again.",
            )?;
        }
        return Ok(true);
    }

    Ok(false)
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
    let (running, started_at, last_error) = {
        let guard = runtime().lock().map_err(|err| err.to_string())?;
        (
            guard.shutdown.is_some(),
            guard.started_at.clone(),
            guard.last_error.clone(),
        )
    };
    let active = active_profile();

    Ok(GatewayStatus {
        running,
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
        started_at,
        last_error,
    })
}

fn active_profile() -> Option<ProfileDraft> {
    let summary = profile::load_profile_summary().ok()?;
    default_active_profile_from_summary(&summary)
}

fn active_profile_for_target(target: &GatewayRouteTarget) -> Option<ProfileDraft> {
    let summary = profile::load_profile_summary().ok()?;
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

fn normalize_gateway_tool_id(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "codex" | "codex-cli" | "codex-app" | "codex-client" | "codex-desktop" => {
            Some("codex".to_string())
        }
        "claude-desktop" | "claude-app" | "claude-client" => Some("claude-desktop".to_string()),
        "claude" | "claude-code" => Some("claude".to_string()),
        "gemini" | "gemini-cli" => Some("gemini".to_string()),
        "gemini-code-assist" | "gemini-vscode" | "gemini-code-vscode" | "gemini-vs-code" => {
            Some("gemini-code-assist".to_string())
        }
        "opencode" | "open-code" => Some("opencode".to_string()),
        "openclaw" | "open-claw" => Some("openclaw".to_string()),
        "hermes" | "hermes-agent" => Some("hermes".to_string()),
        _ => None,
    }
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

struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

struct HttpResponse {
    status: u16,
    reason: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

#[derive(Debug, Clone)]
struct GatewayRouteTarget {
    original_path: String,
    route_path: String,
    tool_id: Option<String>,
    strict_tool: bool,
}

impl GatewayRouteTarget {
    fn from_request(request: &HttpRequest) -> Self {
        let original_path = request
            .path
            .split('?')
            .next()
            .unwrap_or(request.path.as_str())
            .to_string();

        if let Some(rest) = original_path.strip_prefix(TOOL_SCOPED_PREFIX) {
            if let Some((raw_tool_id, route_path)) = rest.split_once('/') {
                if let Some(tool_id) = normalize_gateway_tool_id(raw_tool_id) {
                    let route_path = format!("/{route_path}");
                    return Self {
                        original_path,
                        route_path,
                        tool_id: Some(tool_id),
                        strict_tool: true,
                    };
                }
            }
        }

        if let Some(tool_id) = explicit_tool_from_headers(request) {
            return Self {
                route_path: original_path.clone(),
                original_path,
                tool_id: Some(tool_id),
                strict_tool: true,
            };
        }

        if let Some(tool_id) = inferred_tool_from_headers(request) {
            return Self {
                route_path: original_path.clone(),
                original_path,
                tool_id: Some(tool_id),
                strict_tool: false,
            };
        }

        Self {
            route_path: original_path.clone(),
            original_path,
            tool_id: None,
            strict_tool: false,
        }
    }
}

enum RouteResponse {
    Buffered(HttpResponse),
    Stream(StreamingResponse),
}

impl RouteResponse {
    fn status(&self) -> u16 {
        match self {
            Self::Buffered(response) => response.status,
            Self::Stream(response) => response.expected_status,
        }
    }
}

struct StreamingResponse {
    expected_status: u16,
    run: Box<dyn FnOnce(&mut TcpStream) -> Result<u16, String> + Send>,
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
        let target = GatewayRouteTarget::from_request(request);
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
        let privacy_report = request_body
            .clone()
            .map(|mut value| {
                privacy_filter::filter_json_value(&mut value, config.privacy_filter_mode)
            })
            .unwrap_or(privacy_filter::PrivacyFilterReport { hit_count: 0 });

        Self {
            client: detect_client(request, target.tool_id.as_deref()),
            method: request.method.clone(),
            path: target.original_path,
            provider: active.map(|profile| profile.provider),
            model,
            privacy_filter_mode: config.privacy_filter_mode,
            privacy_filter_hit_count: privacy_report.hit_count,
            privacy_filter_action: privacy_report.action_for_mode(config.privacy_filter_mode),
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let read = stream.read(&mut chunk).map_err(|err| err.to_string())?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        if buffer.len() > MAX_REQUEST_BYTES {
            return Err("Request is too large".to_string());
        }

        if header_end.is_none() {
            if let Some(index) = find_header_end(&buffer) {
                header_end = Some(index + 4);
                let headers = String::from_utf8_lossy(&buffer[..index]);
                content_length = parse_content_length(&headers);
            }
        }

        if let Some(end) = header_end {
            if buffer.len() >= end + content_length {
                break;
            }
        }
    }

    let header_end = header_end.ok_or_else(|| "Invalid HTTP request".to_string())?;
    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| "Missing HTTP request line".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = HashMap::new();

    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let end = header_end + content_length;
    let body = buffer
        .get(header_end..end.min(buffer.len()))
        .unwrap_or_default()
        .to_vec();

    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn detect_client(request: &HttpRequest, tool_id: Option<&str>) -> String {
    if let Some(tool_id) = tool_id {
        return tool_label(tool_id).to_string();
    }

    let header_value = request
        .headers
        .get("x-codestudio-client")
        .or_else(|| request.headers.get("user-agent"))
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    if header_value.contains("codex") {
        "Codex".to_string()
    } else if header_value.contains("opencode") {
        "OpenCode".to_string()
    } else if header_value.contains("openclaw") {
        "OpenClaw".to_string()
    } else if header_value.contains("curl") {
        "curl".to_string()
    } else if header_value.is_empty() {
        "Unknown client".to_string()
    } else {
        header_value.chars().take(48).collect()
    }
}

fn explicit_tool_from_headers(request: &HttpRequest) -> Option<String> {
    request
        .headers
        .get("x-codestudio-tool")
        .or_else(|| request.headers.get("x-codestudio-client-tool"))
        .and_then(|value| normalize_gateway_tool_id(value))
        .or_else(|| {
            request
                .headers
                .get("x-codestudio-client")
                .and_then(|value| normalize_gateway_tool_id(value))
        })
}

fn inferred_tool_from_headers(request: &HttpRequest) -> Option<String> {
    let header_value = request
        .headers
        .get("x-codestudio-client")
        .or_else(|| request.headers.get("user-agent"))
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    if header_value.contains("codex-app")
        || header_value.contains("codex desktop")
        || header_value.contains("codex client")
    {
        Some("codex".to_string())
    } else if header_value.contains("codex") {
        Some("codex".to_string())
    } else if header_value.contains("claude desktop") {
        Some("claude-desktop".to_string())
    } else if header_value.contains("claude") {
        Some("claude".to_string())
    } else if header_value.contains("gemini-code-assist")
        || header_value.contains("geminicodeassist")
    {
        Some("gemini-code-assist".to_string())
    } else if header_value.contains("gemini") {
        Some("gemini".to_string())
    } else if header_value.contains("opencode") {
        Some("opencode".to_string())
    } else if header_value.contains("openclaw") {
        Some("openclaw".to_string())
    } else if header_value.contains("hermes") {
        Some("hermes".to_string())
    } else {
        None
    }
}

fn tool_label(tool_id: &str) -> &'static str {
    match tool_id {
        "codex" => "Codex",
        "claude-desktop" => "Claude Desktop",
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "gemini-code-assist" => "Gemini Code Assist",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        _ => "Unknown client",
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

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> usize {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

fn route_request(request: HttpRequest, config: &GatewayConfig) -> RouteResponse {
    let target = GatewayRouteTarget::from_request(&request);

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
        && !authorized(&request, &config.token)
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

fn authorized(request: &HttpRequest, token: &str) -> bool {
    request
        .headers
        .get("authorization")
        .map(|value| value == &format!("Bearer {token}"))
        .unwrap_or(false)
}

fn models_response(target: &GatewayRouteTarget) -> HttpResponse {
    if target.tool_id.as_deref() == Some("claude-desktop") {
        return claude_desktop_models_response(target);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatewayProtocol {
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
    GoogleGemini,
}

impl GatewayProtocol {
    fn from_profile_protocol(value: &str) -> Result<Self, String> {
        match value {
            PROTOCOL_OPENAI_CHAT_COMPLETIONS => Ok(Self::OpenAiChatCompletions),
            PROTOCOL_OPENAI_RESPONSES => Ok(Self::OpenAiResponses),
            PROTOCOL_ANTHROPIC_MESSAGES => Ok(Self::AnthropicMessages),
            PROTOCOL_GOOGLE_GEMINI => Ok(Self::GoogleGemini),
            _ => Err("Unsupported Provider API protocol.".to_string()),
        }
    }
}

#[derive(Debug, Clone)]
enum GatewayContentPart {
    Text(String),
    ImageUrl(String),
    ImageBase64 {
        mime_type: String,
        data: String,
    },
    ToolResult {
        tool_call_id: Option<String>,
        content: String,
    },
    Unknown(Value),
}

#[derive(Debug, Clone)]
struct GatewayToolCall {
    id: String,
    name: String,
    arguments: Value,
}

#[derive(Debug, Clone)]
struct GatewayToolSpec {
    name: String,
    description: Option<String>,
    schema: Option<Value>,
}

#[derive(Debug, Clone)]
struct GatewayMessage {
    role: String,
    content: Vec<GatewayContentPart>,
    tool_call_id: Option<String>,
    tool_calls: Vec<GatewayToolCall>,
}

#[derive(Debug, Clone)]
struct GatewayRequestParts {
    model: String,
    system: Option<String>,
    messages: Vec<GatewayMessage>,
    tools: Vec<GatewayToolSpec>,
    tool_choice: Option<Value>,
    max_tokens: Option<u64>,
    temperature: Option<Value>,
    top_p: Option<Value>,
}

#[derive(Debug, Clone)]
struct GatewayAssistantResponse {
    content: Vec<GatewayContentPart>,
    tool_calls: Vec<GatewayToolCall>,
    finish_reason: Option<String>,
    usage: GatewayUsage,
}

#[derive(Debug)]
struct ConvertedGatewayRequest {
    endpoint: String,
    headers: String,
    body: Value,
    model: String,
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

    if let Err(response) = apply_gateway_privacy_filter(&mut request_body, config) {
        return response;
    }

    let api_key = match load_gateway_profile_api_key(&profile) {
        Ok(api_key) => api_key,
        Err(response) => return response,
    };

    if client_protocol == upstream_protocol {
        if config.model_override && !profile.model.trim().is_empty() {
            request_body["model"] = Value::String(profile.model.clone());
        }
        let upstream_model = request_model(&request_body)
            .or_else(|| profile_model(&profile))
            .unwrap_or_else(|| CLIENT_MODEL.to_string());
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
    request_body: &mut Value,
    config: &GatewayConfig,
) -> Result<privacy_filter::PrivacyFilterReport, RouteResponse> {
    let report = privacy_filter::filter_json_value(request_body, config.privacy_filter_mode);
    if matches!(config.privacy_filter_mode, PrivacyFilterMode::Block) && report.hit_count > 0 {
        return Err(RouteResponse::Buffered(json_response(
            400,
            "Bad Request",
            json!({
                "error": {
                    "message": "Request blocked by Local Gateway privacy filter.",
                    "type": "privacy_filter_blocked",
                    "hit_count": report.hit_count
                }
            }),
        )));
    }
    Ok(report)
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
        .or_else(|| profile_model(profile))
        .unwrap_or_else(|| CLIENT_MODEL.to_string())
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

fn chat_request_parts(request_body: &Value, model: &str) -> GatewayRequestParts {
    let mut system = None;
    let mut messages = Vec::new();

    for item in request_body
        .get("messages")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
    {
        let role = item
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("user");
        if role == "system" {
            let content = text_from_value(item.get("content").unwrap_or(&Value::Null));
            if !content.is_empty() {
                append_system(&mut system, content);
            }
        } else {
            let mut message = GatewayMessage {
                role: normalize_message_role(role),
                content: content_parts_from_value(item.get("content").unwrap_or(&Value::Null)),
                tool_call_id: item
                    .get("tool_call_id")
                    .or_else(|| item.get("call_id"))
                    .or_else(|| item.get("name"))
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                tool_calls: openai_tool_calls_from_value(
                    item.get("tool_calls").unwrap_or(&Value::Null),
                ),
            };
            if message.role == "tool" {
                let content = content_text(&message.content);
                message.content = vec![GatewayContentPart::ToolResult {
                    tool_call_id: message.tool_call_id.clone(),
                    content,
                }];
            }
            push_message_if_useful(&mut messages, message);
        }
    }

    GatewayRequestParts {
        model: model.to_string(),
        system,
        messages,
        tools: openai_tool_specs_from_value(request_body.get("tools").unwrap_or(&Value::Null))
            .into_iter()
            .chain(openai_legacy_function_specs(
                request_body.get("functions").unwrap_or(&Value::Null),
            ))
            .collect(),
        tool_choice: request_body
            .get("tool_choice")
            .or_else(|| request_body.get("function_call"))
            .cloned(),
        max_tokens: numeric_field(request_body, &["max_completion_tokens", "max_tokens"]),
        temperature: request_body.get("temperature").cloned(),
        top_p: request_body.get("top_p").cloned(),
    }
}

fn responses_request_parts(request_body: &Value, model: &str) -> GatewayRequestParts {
    let mut system = request_body
        .get("instructions")
        .map(text_from_value)
        .filter(|value| !value.is_empty());
    let mut messages = Vec::new();

    match request_body.get("input") {
        Some(Value::String(input)) if !input.trim().is_empty() => messages.push(GatewayMessage {
            role: "user".to_string(),
            content: vec![GatewayContentPart::Text(input.trim().to_string())],
            tool_call_id: None,
            tool_calls: Vec::new(),
        }),
        Some(Value::Array(items)) => {
            for item in items {
                let item_type = item
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                if item_type == "function_call" {
                    let tool_call = responses_function_call_from_value(item);
                    push_message_if_useful(
                        &mut messages,
                        GatewayMessage {
                            role: "assistant".to_string(),
                            content: Vec::new(),
                            tool_call_id: None,
                            tool_calls: tool_call.into_iter().collect(),
                        },
                    );
                    continue;
                }
                if item_type == "function_call_output" {
                    let call_id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(|value| value.as_str())
                        .map(ToString::to_string);
                    let content = text_from_value(
                        item.get("output")
                            .or_else(|| item.get("content"))
                            .unwrap_or(&Value::Null),
                    );
                    push_message_if_useful(
                        &mut messages,
                        GatewayMessage {
                            role: "tool".to_string(),
                            content: vec![GatewayContentPart::ToolResult {
                                tool_call_id: call_id.clone(),
                                content,
                            }],
                            tool_call_id: call_id,
                            tool_calls: Vec::new(),
                        },
                    );
                    continue;
                }

                let role = item
                    .get("role")
                    .and_then(|value| value.as_str())
                    .unwrap_or("user");
                if role == "system" {
                    let content = text_from_value(
                        item.get("content")
                            .or_else(|| item.get("text"))
                            .unwrap_or(&Value::Null),
                    );
                    append_system(&mut system, content);
                } else {
                    push_message_if_useful(
                        &mut messages,
                        GatewayMessage {
                            role: normalize_message_role(role),
                            content: content_parts_from_value(
                                item.get("content")
                                    .or_else(|| item.get("text"))
                                    .unwrap_or(&Value::Null),
                            ),
                            tool_call_id: None,
                            tool_calls: Vec::new(),
                        },
                    );
                }
            }
        }
        Some(value) => {
            let content = text_from_value(value);
            if !content.is_empty() {
                messages.push(GatewayMessage {
                    role: "user".to_string(),
                    content: vec![GatewayContentPart::Text(content)],
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                });
            }
        }
        None => {}
    }

    GatewayRequestParts {
        model: model.to_string(),
        system,
        messages,
        tools: responses_tool_specs_from_value(request_body.get("tools").unwrap_or(&Value::Null)),
        tool_choice: request_body.get("tool_choice").cloned(),
        max_tokens: numeric_field(request_body, &["max_output_tokens", "max_tokens"]),
        temperature: request_body.get("temperature").cloned(),
        top_p: request_body.get("top_p").cloned(),
    }
}

fn anthropic_request_parts(request_body: &Value, model: &str) -> GatewayRequestParts {
    let mut messages = Vec::new();
    for item in request_body
        .get("messages")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
    {
        let role = item
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("user");
        let (content, tool_calls) =
            anthropic_content_and_tool_calls(item.get("content").unwrap_or(&Value::Null));
        push_message_if_useful(
            &mut messages,
            GatewayMessage {
                role: normalize_message_role(role),
                content,
                tool_call_id: None,
                tool_calls,
            },
        );
    }

    GatewayRequestParts {
        model: model.to_string(),
        system: request_body
            .get("system")
            .map(text_from_value)
            .filter(|value| !value.is_empty()),
        messages,
        tools: anthropic_tool_specs_from_value(request_body.get("tools").unwrap_or(&Value::Null)),
        tool_choice: request_body.get("tool_choice").cloned(),
        max_tokens: numeric_field(request_body, &["max_tokens"]),
        temperature: request_body.get("temperature").cloned(),
        top_p: request_body.get("top_p").cloned(),
    }
}

fn gemini_request_parts(request_body: &Value, model: &str) -> GatewayRequestParts {
    let mut messages = Vec::new();
    for item in request_body
        .get("contents")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
    {
        let role = item
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("user");
        let (content, tool_calls) =
            gemini_parts_and_tool_calls(item.get("parts").unwrap_or(&Value::Null));
        push_message_if_useful(
            &mut messages,
            GatewayMessage {
                role: if role == "model" {
                    "assistant".to_string()
                } else {
                    "user".to_string()
                },
                content,
                tool_call_id: None,
                tool_calls,
            },
        );
    }

    let generation_config = request_body.get("generationConfig").unwrap_or(&Value::Null);
    GatewayRequestParts {
        model: model.to_string(),
        system: request_body
            .get("systemInstruction")
            .map(text_from_value)
            .filter(|value| !value.is_empty()),
        messages,
        tools: gemini_tool_specs_from_value(request_body.get("tools").unwrap_or(&Value::Null)),
        tool_choice: request_body.get("toolConfig").cloned(),
        max_tokens: numeric_field(generation_config, &["maxOutputTokens"]),
        temperature: generation_config.get("temperature").cloned(),
        top_p: generation_config.get("topP").cloned(),
    }
}

fn request_body_for_protocol(
    protocol: GatewayProtocol,
    parts: &GatewayRequestParts,
    stream: bool,
) -> Value {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let mut messages = Vec::new();
            if let Some(system) = parts.system.as_deref() {
                messages.push(json!({ "role": "system", "content": system }));
            }
            for message in &parts.messages {
                messages.push(openai_chat_message_value(message));
            }
            if messages.is_empty() {
                messages.push(json!({ "role": "user", "content": "" }));
            }
            let mut body = json!({
                "model": parts.model,
                "messages": messages,
            });
            if stream {
                body["stream"] = Value::Bool(true);
            }
            set_optional_u64(&mut body, "max_tokens", parts.max_tokens);
            set_optional_value(&mut body, "temperature", parts.temperature.clone());
            set_optional_value(&mut body, "top_p", parts.top_p.clone());
            set_tools_for_protocol(&mut body, GatewayProtocol::OpenAiChatCompletions, parts);
            body
        }
        GatewayProtocol::OpenAiResponses => {
            let input: Vec<Value> = if parts.messages.is_empty() {
                vec![json!({
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "" }]
                })]
            } else {
                parts
                    .messages
                    .iter()
                    .flat_map(responses_input_items_for_message)
                    .collect()
            };
            let mut body = json!({
                "model": parts.model,
                "input": input,
            });
            if stream {
                body["stream"] = Value::Bool(true);
            }
            if let Some(system) = parts.system.as_deref() {
                body["instructions"] = Value::String(system.to_string());
            }
            set_optional_u64(&mut body, "max_output_tokens", parts.max_tokens);
            set_optional_value(&mut body, "temperature", parts.temperature.clone());
            set_optional_value(&mut body, "top_p", parts.top_p.clone());
            set_tools_for_protocol(&mut body, GatewayProtocol::OpenAiResponses, parts);
            body
        }
        GatewayProtocol::AnthropicMessages => {
            let messages: Vec<Value> = if parts.messages.is_empty() {
                vec![json!({ "role": "user", "content": "" })]
            } else {
                parts
                    .messages
                    .iter()
                    .map(|message| {
                        json!({
                            "role": if message.role == "assistant" { "assistant" } else { "user" },
                            "content": anthropic_content_value(message),
                        })
                    })
                    .collect()
            };
            let mut body = json!({
                "model": parts.model,
                "messages": messages,
                "max_tokens": parts.max_tokens.unwrap_or(4096),
            });
            if stream {
                body["stream"] = Value::Bool(true);
            }
            if let Some(system) = parts.system.as_deref() {
                body["system"] = Value::String(system.to_string());
            }
            set_optional_value(&mut body, "temperature", parts.temperature.clone());
            set_optional_value(&mut body, "top_p", parts.top_p.clone());
            set_tools_for_protocol(&mut body, GatewayProtocol::AnthropicMessages, parts);
            body
        }
        GatewayProtocol::GoogleGemini => {
            let contents: Vec<Value> = if parts.messages.is_empty() {
                vec![json!({
                    "role": "user",
                    "parts": [{ "text": "" }]
                })]
            } else {
                parts
                    .messages
                    .iter()
                    .map(|message| {
                        json!({
                            "role": if message.role == "assistant" { "model" } else { "user" },
                            "parts": gemini_parts_value(message),
                        })
                    })
                    .collect()
            };
            let mut generation_config = json!({});
            set_optional_u64(&mut generation_config, "maxOutputTokens", parts.max_tokens);
            set_optional_value(
                &mut generation_config,
                "temperature",
                parts.temperature.clone(),
            );
            set_optional_value(&mut generation_config, "topP", parts.top_p.clone());

            let mut body = json!({ "contents": contents });
            if generation_config
                .as_object()
                .map(|object| !object.is_empty())
                .unwrap_or(false)
            {
                body["generationConfig"] = generation_config;
            }
            if let Some(system) = parts.system.as_deref() {
                body["systemInstruction"] = json!({
                    "parts": [{ "text": system }]
                });
            }
            set_tools_for_protocol(&mut body, GatewayProtocol::GoogleGemini, parts);
            body
        }
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

#[derive(Debug, Clone, Default)]
struct GatewayUsage {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    cached_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
    audio_input_tokens: Option<u64>,
    audio_output_tokens: Option<u64>,
    image_input_tokens: Option<u64>,
    image_output_tokens: Option<u64>,
    raw_prompt_details: Option<Value>,
    raw_completion_details: Option<Value>,
}

fn response_body_for_protocol(
    protocol: GatewayProtocol,
    model: &str,
    response: &GatewayAssistantResponse,
) -> Value {
    let text = content_text(&response.content);
    let finish_reason = response
        .finish_reason
        .as_deref()
        .filter(|reason| !reason.is_empty())
        .unwrap_or(if response.tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        });
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let mut message = Map::new();
            message.insert("role".to_string(), Value::String("assistant".to_string()));
            message.insert(
                "content".to_string(),
                openai_chat_content_value(&response.content),
            );
            if !response.tool_calls.is_empty() {
                message.insert(
                    "tool_calls".to_string(),
                    Value::Array(
                        response
                            .tool_calls
                            .iter()
                            .map(openai_tool_call_value)
                            .collect(),
                    ),
                );
            }
            json!({
                "id": format!("chatcmpl-codestudio-{}", Uuid::new_v4().simple()),
                "object": "chat.completion",
                "created": unix_timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "message": Value::Object(message),
                    "finish_reason": if response.tool_calls.is_empty() { finish_reason } else { "tool_calls" }
                }],
                "usage": usage_value_for_protocol(GatewayProtocol::OpenAiChatCompletions, &response.usage)
            })
        }
        GatewayProtocol::OpenAiResponses => {
            let mut output = Vec::new();
            if !response.content.is_empty() || response.tool_calls.is_empty() {
                output.push(json!({
                    "id": format!("msg-codestudio-{}", Uuid::new_v4().simple()),
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": responses_output_content_parts(&response.content)
                }));
            }
            for tool_call in &response.tool_calls {
                output.push(json!({
                    "id": tool_call.id,
                    "type": "function_call",
                    "status": "completed",
                    "call_id": tool_call.id,
                    "name": tool_call.name,
                    "arguments": arguments_as_string(&tool_call.arguments)
                }));
            }
            json!({
                "id": format!("resp-codestudio-{}", Uuid::new_v4().simple()),
                "object": "response",
                "created_at": unix_timestamp(),
                "status": "completed",
                "model": model,
                "output": output,
                "output_text": text,
                "usage": usage_value_for_protocol(GatewayProtocol::OpenAiResponses, &response.usage)
            })
        }
        GatewayProtocol::AnthropicMessages => {
            let content = anthropic_assistant_content_blocks(response);
            json!({
                "id": format!("msg_codestudio_{}", Uuid::new_v4().simple()),
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": content,
                "stop_reason": if response.tool_calls.is_empty() { "end_turn" } else { "tool_use" },
                "usage": usage_value_for_protocol(GatewayProtocol::AnthropicMessages, &response.usage)
            })
        }
        GatewayProtocol::GoogleGemini => json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": gemini_assistant_parts(response)
                },
                "finishReason": if response.tool_calls.is_empty() { "STOP" } else { "TOOL_CALL" },
                "index": 0
            }],
            "usageMetadata": usage_value_for_protocol(GatewayProtocol::GoogleGemini, &response.usage),
            "modelVersion": model
        }),
    }
}

fn assistant_response_from_protocol(
    protocol: GatewayProtocol,
    value: &Value,
) -> GatewayAssistantResponse {
    let usage = usage_from_response(protocol, value);
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let choice = value
                .get("choices")
                .and_then(|choices| choices.as_array())
                .and_then(|choices| choices.first())
                .unwrap_or(&Value::Null);
            let message = choice
                .get("message")
                .or_else(|| choice.get("delta"))
                .unwrap_or(&Value::Null);
            GatewayAssistantResponse {
                content: content_parts_from_value(message.get("content").unwrap_or(&Value::Null)),
                tool_calls: openai_tool_calls_from_value(
                    message.get("tool_calls").unwrap_or(&Value::Null),
                ),
                finish_reason: choice
                    .get("finish_reason")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                usage,
            }
        }
        GatewayProtocol::OpenAiResponses => {
            let mut content = Vec::new();
            let mut tool_calls = Vec::new();
            for item in value
                .get("output")
                .and_then(|output| output.as_array())
                .into_iter()
                .flatten()
            {
                match item
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                {
                    "message" => {
                        content.extend(content_parts_from_value(
                            item.get("content").unwrap_or(&Value::Null),
                        ));
                    }
                    "function_call" => {
                        if let Some(tool_call) = responses_function_call_from_value(item) {
                            tool_calls.push(tool_call);
                        }
                    }
                    _ => {}
                }
            }
            if content.is_empty() {
                content.extend(content_parts_from_value(
                    value.get("output_text").unwrap_or(&Value::Null),
                ));
            }
            GatewayAssistantResponse {
                content,
                tool_calls,
                finish_reason: value
                    .get("status")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                usage,
            }
        }
        GatewayProtocol::AnthropicMessages => {
            let (content, tool_calls) =
                anthropic_content_and_tool_calls(value.get("content").unwrap_or(&Value::Null));
            GatewayAssistantResponse {
                content,
                tool_calls,
                finish_reason: value
                    .get("stop_reason")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                usage,
            }
        }
        GatewayProtocol::GoogleGemini => {
            let candidate = value
                .get("candidates")
                .and_then(|candidates| candidates.as_array())
                .and_then(|candidates| candidates.first())
                .unwrap_or(&Value::Null);
            let (content, tool_calls) = gemini_parts_and_tool_calls(
                candidate
                    .get("content")
                    .and_then(|content| content.get("parts"))
                    .unwrap_or(&Value::Null),
            );
            GatewayAssistantResponse {
                content,
                tool_calls,
                finish_reason: candidate
                    .get("finishReason")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                usage,
            }
        }
    }
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

fn upstream_endpoint(
    protocol: GatewayProtocol,
    profile: &ProfileDraft,
    model: &str,
    stream: bool,
) -> String {
    let base_url = profile.base_url.trim_end_matches('/');
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => format!("{base_url}/chat/completions"),
        GatewayProtocol::OpenAiResponses => format!("{base_url}/responses"),
        GatewayProtocol::AnthropicMessages => format!("{base_url}/messages"),
        GatewayProtocol::GoogleGemini => {
            let model = normalize_gemini_model_name(model);
            if stream {
                format!("{base_url}/models/{model}:streamGenerateContent?alt=sse")
            } else {
                format!("{base_url}/models/{model}:generateContent")
            }
        }
    }
}

fn upstream_headers(protocol: GatewayProtocol, api_key: &str) -> String {
    match protocol {
        GatewayProtocol::AnthropicMessages => upstream_http::anthropic_json_headers(api_key),
        GatewayProtocol::GoogleGemini => upstream_http::gemini_json_headers(api_key),
        GatewayProtocol::OpenAiChatCompletions | GatewayProtocol::OpenAiResponses => {
            upstream_http::bearer_json_headers(api_key)
        }
    }
}

fn upstream_headers_with_passthrough(
    protocol: GatewayProtocol,
    api_key: &str,
    request_headers: &HashMap<String, String>,
) -> String {
    let mut headers = upstream_headers(protocol, api_key);
    let mut generated = generated_header_names(&headers);
    let mut passthrough_headers = safe_passthrough_headers(request_headers)
        .into_iter()
        .filter(|(name, _)| !generated.contains(*name))
        .collect::<Vec<_>>();
    passthrough_headers.sort_by(|left, right| left.0.cmp(right.0));

    for (name, value) in passthrough_headers {
        generated.insert(name.to_string());
        headers.push_str(canonical_passthrough_header_name(name));
        headers.push_str(": ");
        headers.push_str(value);
        headers.push_str("\r\n");
    }

    headers
}

fn generated_header_names(headers: &str) -> HashSet<String> {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_ascii_lowercase())
        .collect()
}

fn safe_passthrough_headers(headers: &HashMap<String, String>) -> Vec<(&str, &str)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let name = name.trim();
            let value = value.trim();
            if value.is_empty() || !is_safe_passthrough_header(name, value) {
                return None;
            }
            Some((name, value))
        })
        .collect()
}

fn is_safe_passthrough_header(name: &str, value: &str) -> bool {
    if !is_valid_header_name(name) || !is_safe_header_value(value) {
        return false;
    }
    if is_forbidden_passthrough_header(name) {
        return false;
    }
    name.starts_with("x-")
        || name.starts_with("anthropic-")
        || name.starts_with("openai-")
        || name.starts_with("cf-")
        || name.starts_with("helicone-")
        || name == "http-referer"
        || name == "referer"
        || name == "user-agent"
}

fn is_forbidden_passthrough_header(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "accept-encoding"
            | "authorization"
            | "connection"
            | "content-length"
            | "content-type"
            | "cookie"
            | "expect"
            | "host"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "x-api-key"
            | "x-codestudio-client"
            | "x-codestudio-client-tool"
            | "x-codestudio-tool"
            | "x-goog-api-key"
            | "anthropic-version"
    )
}

fn is_valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            matches!(
                byte,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
                    | b'0'..=b'9'
                    | b'a'..=b'z'
            )
        })
}

fn is_safe_header_value(value: &str) -> bool {
    !value.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
}

fn canonical_passthrough_header_name(name: &str) -> &str {
    match name {
        "http-referer" => "HTTP-Referer",
        "referer" => "Referer",
        "user-agent" => "User-Agent",
        "x-title" => "X-Title",
        "x-stainless-lang" => "X-Stainless-Lang",
        "x-stainless-package-version" => "X-Stainless-Package-Version",
        "x-stainless-os" => "X-Stainless-OS",
        "x-stainless-arch" => "X-Stainless-Arch",
        "x-stainless-runtime" => "X-Stainless-Runtime",
        "x-stainless-runtime-version" => "X-Stainless-Runtime-Version",
        "x-codestudio-client" => "X-CodeStudio-Client",
        "x-codestudio-tool" => "X-CodeStudio-Tool",
        "x-codestudio-client-tool" => "X-CodeStudio-Client-Tool",
        "anthropic-beta" => "anthropic-beta",
        "openai-beta" => "OpenAI-Beta",
        "cf-ray" => "CF-Ray",
        _ => name,
    }
}

fn normalize_gemini_model_name(model: &str) -> String {
    model
        .trim()
        .trim_start_matches("models/")
        .trim_start_matches('/')
        .to_string()
}

fn gemini_route_from_path(route_path: &str) -> Option<(String, bool)> {
    let rest = route_path
        .strip_prefix("/v1beta/models/")
        .or_else(|| route_path.strip_prefix("/v1/models/"))?;
    if let Some(model) = rest.strip_suffix(":generateContent") {
        return (!model.trim().is_empty()).then(|| (model.to_string(), false));
    }
    let model = rest.strip_suffix(":streamGenerateContent")?;
    (!model.trim().is_empty()).then(|| (model.to_string(), true))
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
        body: serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec()),
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
    let mut parser = SseBuffer::default();
    let mut full_text = String::new();
    let mut full_tool_calls = Vec::new();
    let mut stream_usage = GatewayUsage::default();
    let stream_state = ClientStreamState::new(client_protocol, model);

    upstream_http::post_json_stream_with_headers(
        endpoint,
        headers,
        request_body,
        timeout_seconds,
        |event| match event {
            upstream_http::UpstreamStreamEvent::Headers(meta) => {
                status = meta.status;
                if meta.status >= 400 {
                    raw_passthrough = true;
                    return write_stream_headers(client_stream, meta.status, meta.content_type);
                }
                if meta.content_type != "text/event-stream" {
                    buffered_non_sse = true;
                    return Ok(());
                }
                write_stream_headers(client_stream, meta.status, "text/event-stream")?;
                write_client_stream_start(client_stream, &stream_state)
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
    )?;

    if raw_passthrough {
        return Ok(status);
    }

    if buffered_non_sse {
        let value = serde_json::from_slice::<Value>(&non_sse_body)
            .map_err(|err| format!("Upstream non-SSE stream response was not valid JSON: {err}"))?;
        let response = assistant_response_from_protocol(upstream_protocol, &value);
        let text = content_text(&response.content);
        write_stream_headers(client_stream, status, "text/event-stream")?;
        write_client_stream_start(client_stream, &stream_state)?;
        if !text.is_empty() {
            write_client_stream_delta(client_stream, &stream_state, &text)?;
        }
        for (index, tool_call) in response.tool_calls.iter().enumerate() {
            write_client_stream_tool_call(client_stream, &stream_state, tool_call, index)?;
        }
        write_client_stream_done(
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
    write_client_stream_done(
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
        write_client_stream_delta(client_stream, stream_state, &update.text_delta)?;
    }
    for tool_call in update.tool_calls {
        let index = full_tool_calls.len();
        write_client_stream_tool_call(client_stream, stream_state, &tool_call, index)?;
        full_tool_calls.push(tool_call);
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ClientStreamState {
    protocol: GatewayProtocol,
    id: String,
    item_id: String,
    model: String,
    created: u64,
}

impl ClientStreamState {
    fn new(protocol: GatewayProtocol, model: &str) -> Self {
        Self {
            protocol,
            id: format!("stream_codestudio_{}", Uuid::new_v4().simple()),
            item_id: format!("item_codestudio_{}", Uuid::new_v4().simple()),
            model: model.to_string(),
            created: unix_timestamp(),
        }
    }
}

fn write_client_stream_start(
    stream: &mut TcpStream,
    state: &ClientStreamState,
) -> Result<(), String> {
    match state.protocol {
        GatewayProtocol::OpenAiChatCompletions | GatewayProtocol::GoogleGemini => Ok(()),
        GatewayProtocol::OpenAiResponses => {
            write_sse_json(
                stream,
                Some("response.created"),
                &json!({
                    "type": "response.created",
                    "response": {
                        "id": state.id,
                        "object": "response",
                        "created_at": state.created,
                        "status": "in_progress",
                        "model": state.model,
                        "output": []
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.output_item.added"),
                &json!({
                    "type": "response.output_item.added",
                    "output_index": 0,
                    "item": {
                        "id": state.item_id,
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": []
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.content_part.added"),
                &json!({
                    "type": "response.content_part.added",
                    "item_id": state.item_id,
                    "output_index": 0,
                    "content_index": 0,
                    "part": {
                        "type": "output_text",
                        "text": ""
                    }
                }),
            )
        }
        GatewayProtocol::AnthropicMessages => {
            write_sse_json(
                stream,
                Some("message_start"),
                &json!({
                    "type": "message_start",
                    "message": {
                        "id": state.id,
                        "type": "message",
                        "role": "assistant",
                        "model": state.model,
                        "content": [],
                        "stop_reason": null,
                        "stop_sequence": null,
                        "usage": {
                            "input_tokens": 0,
                            "output_tokens": 0
                        }
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("content_block_start"),
                &json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": {
                        "type": "text",
                        "text": ""
                    }
                }),
            )
        }
    }
}

fn write_client_stream_delta(
    stream: &mut TcpStream,
    state: &ClientStreamState,
    delta: &str,
) -> Result<(), String> {
    match state.protocol {
        GatewayProtocol::OpenAiChatCompletions => write_sse_data(
            stream,
            &json!({
                "id": state.id,
                "object": "chat.completion.chunk",
                "created": state.created,
                "model": state.model,
                "choices": [{
                    "index": 0,
                    "delta": {
                        "content": delta
                    },
                    "finish_reason": null
                }]
            }),
        ),
        GatewayProtocol::OpenAiResponses => write_sse_json(
            stream,
            Some("response.output_text.delta"),
            &json!({
                "type": "response.output_text.delta",
                "item_id": state.item_id,
                "output_index": 0,
                "content_index": 0,
                "delta": delta
            }),
        ),
        GatewayProtocol::AnthropicMessages => write_sse_json(
            stream,
            Some("content_block_delta"),
            &json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "text_delta",
                    "text": delta
                }
            }),
        ),
        GatewayProtocol::GoogleGemini => write_sse_data(
            stream,
            &json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{ "text": delta }]
                    },
                    "index": 0
                }]
            }),
        ),
    }
}

fn write_client_stream_tool_call(
    stream: &mut TcpStream,
    state: &ClientStreamState,
    tool_call: &GatewayToolCall,
    index: usize,
) -> Result<(), String> {
    match state.protocol {
        GatewayProtocol::OpenAiChatCompletions => write_sse_data(
            stream,
            &json!({
                "id": state.id,
                "object": "chat.completion.chunk",
                "created": state.created,
                "model": state.model,
                "choices": [{
                    "index": 0,
                    "delta": {
                        "tool_calls": [{
                            "index": index,
                            "id": tool_call.id,
                            "type": "function",
                            "function": {
                                "name": tool_call.name,
                                "arguments": arguments_as_string(&tool_call.arguments)
                            }
                        }]
                    },
                    "finish_reason": null
                }]
            }),
        ),
        GatewayProtocol::OpenAiResponses => {
            write_sse_json(
                stream,
                Some("response.output_item.added"),
                &json!({
                    "type": "response.output_item.added",
                    "output_index": index + 1,
                    "item": {
                        "id": tool_call.id,
                        "type": "function_call",
                        "status": "in_progress",
                        "call_id": tool_call.id,
                        "name": tool_call.name,
                        "arguments": ""
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.function_call_arguments.delta"),
                &json!({
                    "type": "response.function_call_arguments.delta",
                    "item_id": tool_call.id,
                    "output_index": index + 1,
                    "delta": arguments_as_string(&tool_call.arguments)
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.output_item.done"),
                &json!({
                    "type": "response.output_item.done",
                    "output_index": index + 1,
                    "item": {
                        "id": tool_call.id,
                        "type": "function_call",
                        "status": "completed",
                        "call_id": tool_call.id,
                        "name": tool_call.name,
                        "arguments": arguments_as_string(&tool_call.arguments)
                    }
                }),
            )
        }
        GatewayProtocol::AnthropicMessages => {
            let block_index = index + 1;
            write_sse_json(
                stream,
                Some("content_block_start"),
                &json!({
                    "type": "content_block_start",
                    "index": block_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": tool_call.id,
                        "name": tool_call.name,
                        "input": {}
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("content_block_delta"),
                &json!({
                    "type": "content_block_delta",
                    "index": block_index,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": arguments_as_string(&tool_call.arguments)
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("content_block_stop"),
                &json!({
                    "type": "content_block_stop",
                    "index": block_index
                }),
            )
        }
        GatewayProtocol::GoogleGemini => write_sse_data(
            stream,
            &json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{
                            "functionCall": {
                                "name": tool_call.name,
                                "args": arguments_as_object(&tool_call.arguments)
                            }
                        }]
                    },
                    "index": 0
                }]
            }),
        ),
    }
}

fn write_client_stream_done(
    stream: &mut TcpStream,
    state: &ClientStreamState,
    full_text: &str,
    tool_calls: &[GatewayToolCall],
    usage: &GatewayUsage,
) -> Result<(), String> {
    match state.protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let mut done = json!({
                "id": state.id,
                "object": "chat.completion.chunk",
                "created": state.created,
                "model": state.model,
                "choices": [{
                    "index": 0,
                    "delta": {},
                    "finish_reason": if tool_calls.is_empty() { "stop" } else { "tool_calls" }
                }]
            });
            if usage_has_values(usage) {
                done["usage"] =
                    usage_value_for_protocol(GatewayProtocol::OpenAiChatCompletions, usage);
            }
            write_sse_data(stream, &done)?;
            write_sse_done(stream)
        }
        GatewayProtocol::OpenAiResponses => {
            write_sse_json(
                stream,
                Some("response.output_text.done"),
                &json!({
                    "type": "response.output_text.done",
                    "item_id": state.item_id,
                    "output_index": 0,
                    "content_index": 0,
                    "text": full_text
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.content_part.done"),
                &json!({
                    "type": "response.content_part.done",
                    "item_id": state.item_id,
                    "output_index": 0,
                    "content_index": 0,
                    "part": {
                        "type": "output_text",
                        "text": full_text
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.output_item.done"),
                &json!({
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": {
                        "id": state.item_id,
                        "type": "message",
                        "status": "completed",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": full_text
                        }]
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.completed"),
                &json!({
                    "type": "response.completed",
                    "response": response_body_for_protocol(
                        GatewayProtocol::OpenAiResponses,
                        &state.model,
                        &GatewayAssistantResponse {
                            content: vec![GatewayContentPart::Text(full_text.to_string())],
                            tool_calls: tool_calls.to_vec(),
                            finish_reason: Some(if tool_calls.is_empty() { "stop" } else { "tool_calls" }.to_string()),
                            usage: usage.clone()
                        }
                    )
                }),
            )
        }
        GatewayProtocol::AnthropicMessages => {
            write_sse_json(
                stream,
                Some("content_block_stop"),
                &json!({
                    "type": "content_block_stop",
                    "index": 0
                }),
            )?;
            write_sse_json(
                stream,
                Some("message_delta"),
                &json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": if tool_calls.is_empty() { "end_turn" } else { "tool_use" },
                        "stop_sequence": null
                    },
                    "usage": {
                        "output_tokens": usage.output_tokens
                    }
                }),
            )?;
            write_sse_json(
                stream,
                Some("message_stop"),
                &json!({
                    "type": "message_stop"
                }),
            )
        }
        GatewayProtocol::GoogleGemini => {
            let mut done = json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": gemini_assistant_parts(&GatewayAssistantResponse {
                            content: if full_text.is_empty() {
                                Vec::new()
                            } else {
                                vec![GatewayContentPart::Text(full_text.to_string())]
                            },
                            tool_calls: tool_calls.to_vec(),
                            finish_reason: Some("STOP".to_string()),
                            usage: usage.clone()
                        })
                    },
                    "finishReason": if tool_calls.is_empty() { "STOP" } else { "TOOL_CALL" },
                    "index": 0
                }]
            });
            if usage_has_values(usage) {
                done["usageMetadata"] =
                    usage_value_for_protocol(GatewayProtocol::GoogleGemini, usage);
            }
            write_sse_data(stream, &done)
        }
    }
}

fn write_sse_json(
    stream: &mut TcpStream,
    event: Option<&str>,
    value: &Value,
) -> Result<(), String> {
    if let Some(event) = event {
        stream
            .write_all(format!("event: {event}\n").as_bytes())
            .map_err(|err| err.to_string())?;
    }
    write_sse_data(stream, value)
}

fn write_sse_data(stream: &mut TcpStream, value: &Value) -> Result<(), String> {
    let data = serde_json::to_string(value).map_err(|err| err.to_string())?;
    stream
        .write_all(format!("data: {data}\n\n").as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|err| err.to_string())
}

fn write_sse_done(stream: &mut TcpStream) -> Result<(), String> {
    stream
        .write_all(b"data: [DONE]\n\n")
        .and_then(|_| stream.flush())
        .map_err(|err| err.to_string())
}

#[derive(Debug, Clone, Default)]
struct SseBuffer {
    buffer: String,
}

impl SseBuffer {
    fn push_chunk(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
        self.drain_frames(false)
    }

    fn finish(&mut self) -> Vec<SseFrame> {
        self.drain_frames(true)
    }

    fn drain_frames(&mut self, include_remainder: bool) -> Vec<SseFrame> {
        let mut frames = Vec::new();
        loop {
            let Some((index, separator_len)) = next_sse_separator(&self.buffer) else {
                break;
            };
            let raw = self.buffer[..index].to_string();
            self.buffer.drain(..index + separator_len);
            if let Some(frame) = parse_sse_frame(&raw) {
                frames.push(frame);
            }
        }

        if include_remainder && !self.buffer.trim().is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            if let Some(frame) = parse_sse_frame(&raw) {
                frames.push(frame);
            }
        }

        frames
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SseFrame {
    event: Option<String>,
    data: String,
}

fn next_sse_separator(value: &str) -> Option<(usize, usize)> {
    let lf = value.find("\n\n").map(|index| (index, 2));
    let crlf = value.find("\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn parse_sse_frame(raw: &str) -> Option<SseFrame> {
    let mut event = None;
    let mut data_lines = Vec::new();

    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        None
    } else {
        Some(SseFrame {
            event,
            data: data_lines.join("\n"),
        })
    }
}

#[derive(Debug, Clone, Default)]
struct GatewayStreamUpdate {
    text_delta: String,
    tool_calls: Vec<GatewayToolCall>,
    usage: Option<GatewayUsage>,
}

fn stream_update_from_event(
    protocol: GatewayProtocol,
    frame: &SseFrame,
    value: &Value,
) -> GatewayStreamUpdate {
    GatewayStreamUpdate {
        text_delta: stream_text_delta_from_event(protocol, frame, value),
        tool_calls: stream_tool_calls_from_event(protocol, frame, value),
        usage: stream_usage_from_event(protocol, value),
    }
}

#[allow(dead_code)]
fn stream_delta_from_event(protocol: GatewayProtocol, frame: &SseFrame, value: &Value) -> String {
    stream_text_delta_from_event(protocol, frame, value)
}

fn stream_text_delta_from_event(
    protocol: GatewayProtocol,
    frame: &SseFrame,
    value: &Value,
) -> String {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta").or_else(|| choice.get("message")))
            .and_then(|message| message.get("content"))
            .map(text_from_value)
            .unwrap_or_default(),
        GatewayProtocol::OpenAiResponses => {
            let event_type = value
                .get("type")
                .and_then(|item| item.as_str())
                .or(frame.event.as_deref())
                .unwrap_or_default();
            if event_type.contains("delta") {
                value
                    .get("delta")
                    .or_else(|| value.get("text"))
                    .map(text_from_value)
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
        GatewayProtocol::AnthropicMessages => {
            let event_type = value
                .get("type")
                .and_then(|item| item.as_str())
                .or(frame.event.as_deref())
                .unwrap_or_default();
            if event_type == "content_block_delta" {
                value
                    .get("delta")
                    .and_then(|delta| delta.get("text"))
                    .map(text_from_value)
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
        GatewayProtocol::GoogleGemini => {
            assistant_text_from_response(GatewayProtocol::GoogleGemini, value)
        }
    }
}

fn stream_tool_calls_from_event(
    protocol: GatewayProtocol,
    frame: &SseFrame,
    value: &Value,
) -> Vec<GatewayToolCall> {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta").or_else(|| choice.get("message")))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, item)| openai_stream_tool_call_from_value(index, item))
            .collect(),
        GatewayProtocol::OpenAiResponses => {
            let event_type = value
                .get("type")
                .and_then(|item| item.as_str())
                .or(frame.event.as_deref())
                .unwrap_or_default();
            if event_type == "response.output_item.added"
                || event_type == "response.output_item.done"
            {
                value
                    .get("item")
                    .filter(|item| {
                        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    })
                    .and_then(responses_stream_tool_call_from_value)
                    .into_iter()
                    .collect()
            } else if event_type == "response.function_call_arguments.delta" {
                let delta = value
                    .get("delta")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                if delta.is_empty() {
                    Vec::new()
                } else {
                    vec![GatewayToolCall {
                        id: value
                            .get("item_id")
                            .or_else(|| value.get("call_id"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("call_codestudio_stream")
                            .to_string(),
                        name: value
                            .get("name")
                            .and_then(|value| value.as_str())
                            .unwrap_or("tool")
                            .to_string(),
                        arguments: Value::String(delta.to_string()),
                    }]
                }
            } else {
                Vec::new()
            }
        }
        GatewayProtocol::AnthropicMessages => {
            let event_type = value
                .get("type")
                .and_then(|item| item.as_str())
                .or(frame.event.as_deref())
                .unwrap_or_default();
            if event_type == "content_block_start" {
                value
                    .get("content_block")
                    .filter(|block| {
                        block.get("type").and_then(|value| value.as_str()) == Some("tool_use")
                    })
                    .and_then(anthropic_tool_call_from_value)
                    .into_iter()
                    .collect()
            } else if event_type == "content_block_delta" {
                let delta = value.get("delta").unwrap_or(&Value::Null);
                if delta.get("type").and_then(|value| value.as_str()) == Some("input_json_delta") {
                    delta
                        .get("partial_json")
                        .and_then(|value| value.as_str())
                        .filter(|text| !text.is_empty())
                        .map(|text| GatewayToolCall {
                            id: "call_codestudio_stream".to_string(),
                            name: "tool".to_string(),
                            arguments: Value::String(text.to_string()),
                        })
                        .into_iter()
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        }
        GatewayProtocol::GoogleGemini => {
            assistant_response_from_protocol(GatewayProtocol::GoogleGemini, value).tool_calls
        }
    }
}

fn openai_stream_tool_call_from_value(index: usize, value: &Value) -> Option<GatewayToolCall> {
    let function = value.get("function").unwrap_or(value);
    let name = function
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("tool")
        .to_string();
    let arguments = function.get("arguments");
    if name == "tool" && arguments.is_none() {
        return None;
    }
    Some(GatewayToolCall {
        id: value
            .get("id")
            .or_else(|| value.get("call_id"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("call_codestudio_stream_{index}")),
        name,
        arguments: argument_value(arguments),
    })
}

fn responses_stream_tool_call_from_value(value: &Value) -> Option<GatewayToolCall> {
    let name = value
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("tool")
        .to_string();
    Some(GatewayToolCall {
        id: value
            .get("call_id")
            .or_else(|| value.get("id"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("call_codestudio_{}", Uuid::new_v4().simple())),
        name,
        arguments: argument_value(value.get("arguments")),
    })
}

fn stream_usage_from_event(protocol: GatewayProtocol, value: &Value) -> Option<GatewayUsage> {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => value
            .get("usage")
            .map(|_| usage_from_response(protocol, value)),
        GatewayProtocol::OpenAiResponses => {
            if value.get("usage").is_some() {
                Some(usage_from_response(protocol, value))
            } else if let Some(response) = value.get("response") {
                response
                    .get("usage")
                    .map(|_| usage_from_response(protocol, response))
            } else {
                None
            }
        }
        GatewayProtocol::AnthropicMessages => {
            if value.get("usage").is_some() {
                Some(usage_from_response(protocol, value))
            } else if let Some(message) = value.get("message") {
                message
                    .get("usage")
                    .map(|_| usage_from_response(protocol, message))
            } else {
                None
            }
        }
        GatewayProtocol::GoogleGemini => value
            .get("usageMetadata")
            .map(|_| usage_from_response(protocol, value)),
    }
}

fn merge_stream_usage(target: &mut GatewayUsage, update: GatewayUsage) {
    if update.input_tokens > 0 {
        target.input_tokens = update.input_tokens;
    }
    if update.output_tokens > 0 {
        target.output_tokens = update.output_tokens;
    }
    if update.total_tokens > 0 {
        target.total_tokens = update.total_tokens;
    } else if target.total_tokens == 0 {
        target.total_tokens = target.input_tokens + target.output_tokens;
    }
    target.cached_input_tokens = update.cached_input_tokens.or(target.cached_input_tokens);
    target.cache_creation_input_tokens = update
        .cache_creation_input_tokens
        .or(target.cache_creation_input_tokens);
    target.cache_read_input_tokens = update
        .cache_read_input_tokens
        .or(target.cache_read_input_tokens);
    target.reasoning_tokens = update.reasoning_tokens.or(target.reasoning_tokens);
    target.audio_input_tokens = update.audio_input_tokens.or(target.audio_input_tokens);
    target.audio_output_tokens = update.audio_output_tokens.or(target.audio_output_tokens);
    target.image_input_tokens = update.image_input_tokens.or(target.image_input_tokens);
    target.image_output_tokens = update.image_output_tokens.or(target.image_output_tokens);
    target.raw_prompt_details = update
        .raw_prompt_details
        .or_else(|| target.raw_prompt_details.clone());
    target.raw_completion_details = update
        .raw_completion_details
        .or_else(|| target.raw_completion_details.clone());
}

fn usage_has_values(usage: &GatewayUsage) -> bool {
    usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.total_tokens > 0
        || usage.cached_input_tokens.is_some()
        || usage.cache_creation_input_tokens.is_some()
        || usage.cache_read_input_tokens.is_some()
        || usage.reasoning_tokens.is_some()
        || usage.audio_input_tokens.is_some()
        || usage.audio_output_tokens.is_some()
        || usage.image_input_tokens.is_some()
        || usage.image_output_tokens.is_some()
        || usage.raw_prompt_details.is_some()
        || usage.raw_completion_details.is_some()
}

fn stream_upstream_json_with_headers(
    endpoint: &str,
    headers: &str,
    request_body: &serde_json::Value,
    timeout_seconds: u16,
    client_stream: &mut TcpStream,
) -> Result<u16, String> {
    let mut status = 200;
    upstream_http::post_json_stream_with_headers(
        endpoint,
        headers,
        request_body,
        timeout_seconds,
        |event| match event {
            upstream_http::UpstreamStreamEvent::Headers(meta) => {
                status = meta.status;
                write_stream_headers(client_stream, meta.status, meta.content_type)
            }
            upstream_http::UpstreamStreamEvent::Chunk(chunk) => client_stream
                .write_all(chunk)
                .and_then(|_| client_stream.flush())
                .map_err(|err| err.to_string()),
        },
    )?;

    Ok(status)
}

fn write_route_response(stream: &mut TcpStream, response: RouteResponse) -> Result<u16, String> {
    match response {
        RouteResponse::Buffered(response) => write_http_response(stream, response),
        RouteResponse::Stream(response) => (response.run)(stream),
    }
}

fn write_http_response(stream: &mut TcpStream, response: HttpResponse) -> Result<u16, String> {
    let status = response.status;
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.reason,
        response.content_type,
        response.body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(&response.body))
        .and_then(|_| stream.flush())
        .map_err(|err| err.to_string())?;

    Ok(status)
}

fn write_stream_headers(
    stream: &mut TcpStream,
    status: u16,
    content_type: &'static str,
) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nCache-Control: no-cache\r\nConnection: close\r\nX-Accel-Buffering: no\r\n\r\n",
        status,
        reason_for_status(status),
        content_type
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn response_body_json(response: RouteResponse) -> Value {
        match response {
            RouteResponse::Buffered(response) => {
                serde_json::from_slice(&response.body).expect("response body should be JSON")
            }
            RouteResponse::Stream(_) => panic!("expected buffered response"),
        }
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
            base_url: "https://api.example.test/v1".to_string(),
            auth_ref: Some("test-key".to_string()),
            created_at: None,
            updated_at: None,
            last_test_status: None,
            usage_enabled: false,
            sort_order: 0,
        }
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
        let result = apply_gateway_privacy_filter(&mut request_body, &config);

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
        let result = apply_gateway_privacy_filter(&mut request_body, &config);

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
            let report = match apply_gateway_privacy_filter(request_body, &config) {
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
        assert_ne!(response.status(), 404);
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
    fn messages_route_is_implemented_for_anthropic_clients() {
        let config = test_config(true);
        let response = route_request(post("/v1/messages", Some(&config.token)), &config);
        assert_ne!(response.status(), 404);
        assert_ne!(response.status(), 401);
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
    fn codex_client_alias_uses_codex_gateway_scope() {
        assert_eq!(
            normalize_gateway_tool_id("codex-app").as_deref(),
            Some("codex")
        );
        assert_eq!(
            normalize_gateway_tool_id("codex-client").as_deref(),
            Some("codex")
        );
    }
}
