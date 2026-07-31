use super::{codex_home_dir, ChatGptDesktopSettings};
use crate::core::activity_log;
use crate::core::app_paths::display_path;
use crate::core::types::Severity;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const SCRIPT_TEMPLATE: &str = include_str!("codex_enhancements.js");
const SETTINGS_PLACEHOLDER: &str = "__CODESTUDIO_LITE_SETTINGS__";
const MARKETPLACES_PLACEHOLDER: &str = "__CODESTUDIO_LITE_PLUGIN_MARKETPLACES__";
const CODEX_PATCH_INJECTION_RETRY_COUNT: usize = 30;
const CODEX_PATCH_INJECTION_RETRY_MS: u64 = 500;
const CODEX_PATCH_WATCHDOG_POLL_MS: u64 = 2_000;
const CODEX_PATCH_WATCHDOG_MAX_MISSES: usize = 15;

pub(super) fn launch<F>(settings: &ChatGptDesktopSettings, launcher: F) -> Result<(), String>
where
    F: FnOnce(&[String]) -> Result<(), String>,
{
    EnhancementController::prepare(settings)?.launch_with(launcher, |controller| {
        thread::spawn(move || controller.run());
    })
}

pub(super) fn render_script(
    settings_json: &str,
    marketplaces_json: &str,
) -> Result<String, String> {
    for placeholder in [SETTINGS_PLACEHOLDER, MARKETPLACES_PLACEHOLDER] {
        if SCRIPT_TEMPLATE.matches(placeholder).count() != 1 {
            return Err(format!(
                "Codex enhancement script must contain exactly one {placeholder} placeholder."
            ));
        }
    }
    let rendered = SCRIPT_TEMPLATE
        .replace(SETTINGS_PLACEHOLDER, settings_json)
        .replace(MARKETPLACES_PLACEHOLDER, marketplaces_json);
    if rendered.contains(SETTINGS_PLACEHOLDER) || rendered.contains(MARKETPLACES_PLACEHOLDER) {
        return Err("Codex enhancement script contains an unresolved placeholder.".to_string());
    }
    Ok(rendered)
}

struct EnhancementController {
    debug_port: u16,
    settings: EnhancementSettings,
}

impl EnhancementController {
    fn prepare(settings: &ChatGptDesktopSettings) -> Result<Self, String> {
        Ok(Self {
            debug_port: select_debug_port()?,
            settings: codex_enhancement_settings_from(settings),
        })
    }

    fn launch_with<F, S>(self, launcher: F, start: S) -> Result<(), String>
    where
        F: FnOnce(&[String]) -> Result<(), String>,
        S: FnOnce(Self),
    {
        launcher(&self.launch_args())?;
        if self.settings.enabled() {
            start(self);
        }
        Ok(())
    }

    fn launch_args(&self) -> Vec<String> {
        codex_patch_launch_args(self.debug_port)
    }

    fn run(self) {
        match inject_codex_enhancements(self.debug_port, &self.settings) {
            Ok(active_url) => {
                let _ =
                    activity_log::append(Severity::Ok, "Applied Codex launch enhancement patch.");
                watch_codex_enhancement_target(self.debug_port, &self.settings, active_url);
            }
            Err(error) => {
                let _ = activity_log::append(
                    Severity::Error,
                    format!("Codex launch enhancement patch failed: {error}"),
                );
            }
        }
    }

    #[cfg(test)]
    fn for_test(debug_port: u16, settings: EnhancementSettings) -> Self {
        Self {
            debug_port,
            settings,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnhancementSettings {
    plugin_marketplace_unlock: bool,
    plugin_auto_expand: bool,
    model_whitelist_unlock: bool,
    service_tier_controls: bool,
    model_catalog: CodexModelCatalog,
}

impl EnhancementSettings {
    fn enabled(&self) -> bool {
        self.plugin_marketplace_unlock
            || self.plugin_auto_expand
            || self.model_whitelist_unlock
            || self.service_tier_controls
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct CodexModelCatalog {
    status: String,
    model: String,
    #[serde(rename = "default_model")]
    default_model: String,
    #[serde(rename = "model_provider")]
    model_provider: String,
    #[serde(rename = "provider_name")]
    provider_name: String,
    models: Vec<String>,
    sources: Vec<String>,
    #[serde(rename = "responses_api")]
    responses_api: serde_json::Value,
}

fn codex_enhancement_settings_from(settings: &ChatGptDesktopSettings) -> EnhancementSettings {
    EnhancementSettings {
        plugin_marketplace_unlock: settings.plugin_marketplace_unlock_on_launch,
        plugin_auto_expand: settings.plugin_auto_expand_on_launch,
        model_whitelist_unlock: settings.model_whitelist_unlock_on_launch,
        service_tier_controls: settings.service_tier_controls_on_launch,
        model_catalog: codex_model_catalog_for_injection(),
    }
}

fn codex_model_catalog_for_injection() -> CodexModelCatalog {
    let mut catalog = CodexModelCatalog {
        status: "ok".to_string(),
        model: String::new(),
        default_model: String::new(),
        model_provider: String::new(),
        provider_name: String::new(),
        models: Vec::new(),
        sources: Vec::new(),
        responses_api: json!({ "status": "unknown", "message": "" }),
    };
    if let Ok(home) = codex_home_dir() {
        let config_path = home.join("config.toml");
        if let Ok(text) = fs::read_to_string(&config_path) {
            if let Ok(value) = text.parse::<toml::Value>() {
                collect_codex_model_catalog_from_toml(&home, &value, &mut catalog);
                catalog.sources.push(display_path(&config_path));
            }
        }
    }
    for key in ["CODEX_MODEL", "OPENAI_MODEL"] {
        if let Ok(model) = std::env::var(key) {
            push_unique_model(&mut catalog.models, model.trim());
            catalog.sources.push(format!("env:{key}"));
        }
    }
    if catalog.model.is_empty() {
        catalog.model = catalog.models.first().cloned().unwrap_or_default();
    }
    if catalog.default_model.is_empty() {
        catalog.default_model = catalog.model.clone();
    }
    catalog
}

fn collect_codex_model_catalog_from_toml(
    home: &Path,
    value: &toml::Value,
    catalog: &mut CodexModelCatalog,
) {
    if let Some(model) = codex_effective_config_value(value, "model").and_then(toml::Value::as_str)
    {
        catalog.model = model.trim().to_string();
        push_unique_model(&mut catalog.models, model);
    }
    if let Some(model) =
        codex_effective_config_value(value, "default_model").and_then(toml::Value::as_str)
    {
        catalog.default_model = model.trim().to_string();
        push_unique_model(&mut catalog.models, model);
    }
    if let Some(model_catalog_json) =
        codex_effective_config_value(value, "model_catalog_json").and_then(toml::Value::as_str)
    {
        let path = resolve_codex_config_path(home, model_catalog_json);
        let mut catalog_models = collect_codex_model_catalog_json_models(&path);
        for model in catalog_models.drain(..) {
            push_unique_model(&mut catalog.models, &model);
        }
        catalog.sources.push(display_path(&path));
    }
    let provider_id = codex_effective_config_value(value, "model_provider")
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    catalog.model_provider = provider_id.clone();
    if provider_id.is_empty() {
        return;
    }
    let Some(provider) = value
        .get("model_providers")
        .and_then(toml::Value::as_table)
        .and_then(|providers| providers.get(provider_id.as_str()))
    else {
        return;
    };
    catalog.provider_name = provider
        .get("name")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or(provider_id.as_str())
        .to_string();
    for key in ["model", "default_model"] {
        if let Some(model) = provider.get(key).and_then(toml::Value::as_str) {
            push_unique_model(&mut catalog.models, model);
        }
    }
    for key in ["models", "model_list", "available_models"] {
        if let Some(models) = provider.get(key).and_then(toml::Value::as_array) {
            for model in models.iter().filter_map(toml::Value::as_str) {
                push_unique_model(&mut catalog.models, model);
            }
        }
    }
}

fn codex_effective_config_value<'a>(value: &'a toml::Value, key: &str) -> Option<&'a toml::Value> {
    let profile_value = value
        .get("profile")
        .and_then(toml::Value::as_str)
        .and_then(|profile| {
            value
                .get("profiles")
                .and_then(toml::Value::as_table)
                .and_then(|profiles| profiles.get(profile))
        })
        .and_then(|profile| profile.get(key));
    profile_value.or_else(|| value.get(key))
}

fn resolve_codex_config_path(home: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    if path.is_absolute() {
        path
    } else {
        home.join(path)
    }
}

fn collect_codex_model_catalog_json_models(path: &Path) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return Vec::new();
    };
    let Some(models) = payload.get("models").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    models
        .iter()
        .filter(|model| codex_catalog_model_visible_in_api(model))
        .filter_map(|model| model.get("slug").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|slug| !slug.is_empty())
        .map(str::to_string)
        .collect()
}

fn codex_catalog_model_visible_in_api(model: &serde_json::Value) -> bool {
    let supported_in_api = model
        .get("supported_in_api")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    if !supported_in_api {
        return false;
    }
    let visibility = model
        .get("visibility")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("list")
        .trim();
    visibility.eq_ignore_ascii_case("list")
}

fn push_unique_model(models: &mut Vec<String>, model: &str) {
    let trimmed = model.trim();
    if trimmed.is_empty() || models.iter().any(|item| item == trimmed) {
        return;
    }
    models.push(trimmed.to_string());
}

fn codex_patch_launch_args(debug_port: u16) -> Vec<String> {
    vec![
        format!("--remote-debugging-port={debug_port}"),
        format!("--remote-allow-origins=http://127.0.0.1:{debug_port}"),
    ]
}

fn select_debug_port() -> Result<u16, String> {
    TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| format!("Failed to reserve a patch launch debug port: {err}"))
        .and_then(|listener| {
            listener
                .local_addr()
                .map(|addr| addr.port())
                .map_err(|err| format!("Failed to read patch launch debug port: {err}"))
        })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CdpTarget {
    #[serde(rename = "type")]
    target_type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

fn inject_codex_enhancements(
    debug_port: u16,
    settings: &EnhancementSettings,
) -> Result<String, String> {
    let mut last_error = None;
    for _ in 0..CODEX_PATCH_INJECTION_RETRY_COUNT {
        match try_inject_codex_enhancements(debug_port, settings) {
            Ok(websocket_url) => return Ok(websocket_url),
            Err(err) => {
                last_error = Some(err);
                thread::sleep(Duration::from_millis(CODEX_PATCH_INJECTION_RETRY_MS));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "Codex patch injection failed.".to_string()))
}

fn watch_codex_enhancement_target(
    debug_port: u16,
    settings: &EnhancementSettings,
    mut active_websocket_url: String,
) {
    let mut consecutive_misses = 0;
    loop {
        thread::sleep(Duration::from_millis(CODEX_PATCH_WATCHDOG_POLL_MS));
        let next_target = pick_cdp_target(debug_port).and_then(|target| {
            target
                .web_socket_debugger_url
                .filter(|websocket_url| !websocket_url.is_empty())
                .ok_or_else(|| {
                    "Selected Codex CDP target has no WebSocket debugger URL.".to_string()
                })
        });
        match next_target {
            Ok(websocket_url) => {
                if websocket_url != active_websocket_url {
                    let script = match codex_enhancement_script(settings) {
                        Ok(script) => script,
                        Err(_) => return,
                    };
                    match evaluate_cdp_script(&websocket_url, &script) {
                        Ok(()) => {
                            active_websocket_url = websocket_url;
                            consecutive_misses = 0;
                            let _ = activity_log::append(
                                Severity::Ok,
                                "Reapplied Codex enhancements to a recreated desktop page.",
                            );
                        }
                        Err(_) => consecutive_misses += 1,
                    }
                } else {
                    consecutive_misses = 0;
                }
            }
            Err(_) => consecutive_misses += 1,
        }
        if consecutive_misses >= CODEX_PATCH_WATCHDOG_MAX_MISSES {
            let _ = activity_log::append(
                Severity::Info,
                "Stopped the Codex enhancement page watchdog after the desktop CDP endpoint closed.",
            );
            return;
        }
    }
}

fn try_inject_codex_enhancements(
    debug_port: u16,
    settings: &EnhancementSettings,
) -> Result<String, String> {
    let target = pick_cdp_target(debug_port)?;
    let ws_url = target
        .web_socket_debugger_url
        .ok_or_else(|| "Selected Codex CDP target has no WebSocket debugger URL.".to_string())?;
    let script = codex_enhancement_script(settings)?;
    evaluate_cdp_script(&ws_url, &script)?;
    Ok(ws_url)
}

fn pick_cdp_target(debug_port: u16) -> Result<CdpTarget, String> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|err| format!("Failed to build CDP client: {err}"))?;
    let mut errors = Vec::new();
    for url in [
        format!("http://127.0.0.1:{debug_port}/json"),
        format!("http://[::1]:{debug_port}/json"),
    ] {
        match client.get(&url).send() {
            Ok(response) => match response.error_for_status() {
                Ok(response) => {
                    let targets = response
                        .json::<Vec<CdpTarget>>()
                        .map_err(|err| format!("Failed to parse CDP targets: {err}"))?;
                    match pick_cdp_target_from_targets(&targets) {
                        Ok(target) => return Ok(target),
                        Err(err) => errors.push(format!("{url}: {err}")),
                    }
                }
                Err(err) => errors.push(format!("{url}: {err}")),
            },
            Err(err) => errors.push(format!("{url}: {err}")),
        }
    }
    Err(format!(
        "Failed to find Codex CDP target: {}",
        errors.join("; ")
    ))
}

fn pick_cdp_target_from_targets(targets: &[CdpTarget]) -> Result<CdpTarget, String> {
    targets
        .iter()
        .filter(|target| {
            target.target_type == "page"
                && target
                    .web_socket_debugger_url
                    .as_deref()
                    .is_some_and(|websocket_url| !websocket_url.is_empty())
        })
        .find(|target| is_codex_or_chatgpt_desktop_page(target))
        .cloned()
        .ok_or_else(|| "no injectable Codex or ChatGPT Desktop page target".to_string())
}

fn is_codex_or_chatgpt_desktop_page(target: &CdpTarget) -> bool {
    let haystack = format!("{} {}", target.title, target.url).to_ascii_lowercase();
    haystack.contains("codex") || is_chatgpt_desktop_page(&target.title, &target.url)
}

fn is_chatgpt_desktop_page(title: &str, url: &str) -> bool {
    let title = title.trim().to_ascii_lowercase();
    let url = url.trim().to_ascii_lowercase();
    title == "chatgpt"
        && (url == "https://chatgpt.com"
            || url.starts_with("https://chatgpt.com/")
            || url == "https://chat.openai.com"
            || url.starts_with("https://chat.openai.com/")
            || url.starts_with("data:text/html"))
}

fn evaluate_cdp_script(websocket_url: &str, script: &str) -> Result<(), String> {
    let (mut socket, _) = tungstenite::connect(websocket_url)
        .map_err(|err| format!("Failed to connect Codex CDP WebSocket: {err}"))?;

    send_cdp_request(
        &mut socket,
        1,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": script }),
    )?;
    wait_for_cdp_response(&mut socket, 1, "Codex new-document patch registration")?;

    send_cdp_request(
        &mut socket,
        2,
        "Runtime.evaluate",
        json!({
            "expression": script,
            "awaitPromise": true,
            "returnByValue": true,
            "allowUnsafeEvalBlockedByCSP": true
        }),
    )?;
    wait_for_cdp_response(&mut socket, 2, "Codex patch script")
}

fn send_cdp_request(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> Result<(), String> {
    let request = serde_json::to_string(&json!({
        "id": id,
        "method": method,
        "params": params
    }))
    .map_err(|err| format!("Failed to encode CDP request: {err}"))?;
    socket
        .send(tungstenite::Message::Text(request.into()))
        .map_err(|err| format!("Failed to send CDP request {method}: {err}"))
}

fn wait_for_cdp_response(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    expected_id: u64,
    context: &str,
) -> Result<(), String> {
    for _ in 0..20 {
        let message = socket
            .read()
            .map_err(|err| format!("Failed to read {context} result: {err}"))?;
        if let tungstenite::Message::Text(text) = message {
            let value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|err| format!("Failed to parse {context} result: {err}"))?;
            if value.get("id").and_then(|item| item.as_u64()) != Some(expected_id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(format!("{context} failed: {error}"));
            }
            return Ok(());
        }
    }
    Err(format!("{context} result was not received."))
}

fn codex_enhancement_script(settings: &EnhancementSettings) -> Result<String, String> {
    let settings_json = serde_json::to_string(settings)
        .map_err(|err| format!("Failed to serialize Codex enhancement settings: {err}"))?;
    let plugin_marketplaces_json =
        serde_json::to_string(&codex_plugin_marketplaces_for_injection())
            .map_err(|err| format!("Failed to serialize Codex plugin marketplaces: {err}"))?;
    render_script(&settings_json, &plugin_marketplaces_json)
}

fn codex_plugin_marketplaces_for_injection() -> serde_json::Value {
    let Ok(home) = codex_home_dir() else {
        return json!([]);
    };
    codex_plugin_marketplaces_for_injection_from_home(&home)
}

fn codex_plugin_marketplaces_for_injection_from_home(home: &Path) -> serde_json::Value {
    let marketplace_path = home
        .join(".tmp")
        .join("plugins-remote")
        .join(".agents")
        .join("plugins")
        .join("marketplace.json");
    let Ok(text) = fs::read_to_string(&marketplace_path) else {
        return json!([]);
    };
    let Ok(mut marketplace) = serde_json::from_str::<serde_json::Value>(&text) else {
        return json!([]);
    };
    let marketplace_name = marketplace
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let marketplace_root = marketplace_path
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".tmp").join("plugins-remote"));
    if let Some(plugins) = marketplace
        .get_mut("plugins")
        .and_then(serde_json::Value::as_array_mut)
    {
        for plugin in plugins {
            let Some(plugin_object) = plugin.as_object_mut() else {
                continue;
            };
            let plugin_name = plugin_object
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if plugin_name.is_empty() {
                continue;
            }
            let manifest_path = marketplace_root
                .join("plugins")
                .join(&plugin_name)
                .join(".codex-plugin")
                .join("plugin.json");
            if let Ok(manifest_text) = fs::read_to_string(manifest_path) {
                if let Ok(serde_json::Value::Object(manifest)) =
                    serde_json::from_str::<serde_json::Value>(&manifest_text)
                {
                    for (key, value) in manifest {
                        plugin_object.entry(key).or_insert(value);
                    }
                }
            }
            plugin_object
                .entry("id".to_string())
                .or_insert_with(|| json!(format!("{plugin_name}@{marketplace_name}")));
            plugin_object
                .entry("marketplaceName".to_string())
                .or_insert_with(|| json!(marketplace_name.clone()));
            plugin_object
                .entry("marketplacePath".to_string())
                .or_insert_with(|| json!(marketplace_name.clone()));
        }
    }
    if let Some(object) = marketplace.as_object_mut() {
        object.entry("path".to_string()).or_insert_with(|| {
            serde_json::Value::String(marketplace_path.to_string_lossy().to_string())
        });
    }
    serde_json::Value::Array(vec![marketplace])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_settings() -> EnhancementSettings {
        EnhancementSettings {
            plugin_marketplace_unlock: true,
            plugin_auto_expand: false,
            model_whitelist_unlock: false,
            service_tier_controls: false,
            model_catalog: CodexModelCatalog::default(),
        }
    }

    #[test]
    fn launch_failure_never_starts_injection() {
        let controller = EnhancementController::for_test(4242, enabled_settings());
        let started = std::cell::Cell::new(false);
        let result = controller.launch_with(
            |args| {
                assert_eq!(args[0], "--remote-debugging-port=4242");
                Err("launch failed".to_string())
            },
            |_| started.set(true),
        );
        assert_eq!(result.unwrap_err(), "launch failed");
        assert!(!started.get());
    }

    #[test]
    fn successful_enabled_launch_starts_injection_once() {
        let controller = EnhancementController::for_test(4242, enabled_settings());
        let starts = std::cell::Cell::new(0);
        controller
            .launch_with(|_| Ok(()), |_| starts.set(starts.get() + 1))
            .unwrap();
        assert_eq!(starts.get(), 1);
    }

    #[test]
    fn renderer_replaces_every_required_placeholder() {
        let script = render_script(r#"{"enabled":true}"#, "[]").unwrap();
        assert!(script.contains(r#"{"enabled":true}"#));
        assert!(!script.contains(SETTINGS_PLACEHOLDER));
        assert!(!script.contains(MARKETPLACES_PLACEHOLDER));
    }
}
