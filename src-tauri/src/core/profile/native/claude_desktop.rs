use super::plan::{NativeConfigWriteKind, NativeConfigWritePlan};
use crate::core::app_paths::{display_path, AppPaths};
use crate::core::gateway;
use crate::core::types::{NativeConfigPreview, ProfileDraft, ProviderApplyMode};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(in crate::core::profile) const PROFILE_ID: &str = "00000000-0000-4000-8000-000000157210";
const CONFIG_FILE: &str = "claude_desktop_config.json";
const CONFIG_LIBRARY_DIR: &str = "configLibrary";
const PROFILE_NAME: &str = "CodeStudio Lite";
const ROUTE_PREFIX: &str = "claude-";
const ANTHROPIC_ROUTE_PREFIX: &str = "anthropic/claude-";
const ONE_M_CONTEXT_MARKER: &str = "[1m]";
const DEFAULT_ROUTE_ID: &str = "claude-sonnet-4-6";
const DEFAULT_ROUTES: [(&str, bool); 4] = [
    ("claude-sonnet-4-6", true),
    ("claude-opus-4-8", true),
    ("claude-haiku-4-5", true),
    ("claude-fable-5", true),
];

#[derive(Debug, Clone)]
pub(crate) struct InferenceModelSpec {
    pub name: String,
    pub label_override: Option<String>,
    pub supports_1m: bool,
}

#[derive(Debug, Clone)]
pub(in crate::core::profile) struct ClaudeDesktopPaths {
    pub normal_config_path: PathBuf,
    pub threep_config_path: PathBuf,
    pub profile_path: PathBuf,
    pub meta_path: PathBuf,
    pub developer_settings_paths: Vec<PathBuf>,
}

pub(in crate::core::profile) fn paths(app_paths: &AppPaths) -> Result<ClaudeDesktopPaths, String> {
    if cfg!(target_os = "windows") {
        let local_app_data = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| app_paths.home_dir.join("AppData").join("Local"));
        let roaming_app_data = env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| app_paths.home_dir.join("AppData").join("Roaming"));
        let normal_dir = pick_windows_dir(&local_app_data, false)
            .unwrap_or_else(|| local_app_data.join("Claude"));
        let threep_dir = pick_windows_dir(&local_app_data, true)
            .unwrap_or_else(|| local_app_data.join("Claude-3p"));
        return Ok(paths_from_dirs(
            normal_dir.clone(),
            threep_dir,
            vec![
                roaming_app_data
                    .join("Claude")
                    .join("developer_settings.json"),
                normal_dir.join("developer_settings.json"),
            ],
        ));
    }

    if cfg!(target_os = "macos") {
        let app_support = app_paths
            .home_dir
            .join("Library")
            .join("Application Support");
        let normal_dir = app_support.join("Claude");
        let threep_dir = app_support.join("Claude-3p");
        return Ok(paths_from_dirs(
            normal_dir.clone(),
            threep_dir.clone(),
            macos_developer_settings_paths(&normal_dir, &threep_dir),
        ));
    }

    Err("Claude Desktop 3P configuration is only supported on Windows and macOS.".to_string())
}

fn pick_windows_dir(local_app_data: &Path, threep: bool) -> Option<PathBuf> {
    let exact_name = if threep { "Claude-3p" } else { "Claude" };
    let exact = local_app_data.join(exact_name);
    if exact.exists() {
        return Some(exact);
    }

    let mut candidates = fs::read_dir(local_app_data)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|name| name.starts_with("Claude") && name.contains("-3p") == threep)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next()
}

pub(in crate::core::profile) fn macos_developer_settings_paths(
    normal_dir: &Path,
    threep_dir: &Path,
) -> Vec<PathBuf> {
    vec![
        normal_dir.join("developer_settings.json"),
        threep_dir.join("developer_settings.json"),
    ]
}

pub(in crate::core::profile) fn paths_from_dirs(
    normal_dir: PathBuf,
    threep_dir: PathBuf,
    developer_settings_paths: Vec<PathBuf>,
) -> ClaudeDesktopPaths {
    let config_library_path = threep_dir.join(CONFIG_LIBRARY_DIR);
    ClaudeDesktopPaths {
        normal_config_path: normal_dir.join(CONFIG_FILE),
        threep_config_path: threep_dir.join(CONFIG_FILE),
        profile_path: config_library_path.join(format!("{PROFILE_ID}.json")),
        meta_path: config_library_path.join("_meta.json"),
        developer_settings_paths: dedupe_paths(developer_settings_paths),
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(display_path(path).to_ascii_lowercase()))
        .collect()
}

pub(in crate::core::profile) fn direct_inference_models(
    profile: &ProfileDraft,
) -> Vec<InferenceModelSpec> {
    super::super::profile_model(profile)
        .filter(|model| safe_model_id(model))
        .map(|model| {
            vec![InferenceModelSpec {
                name: model.to_string(),
                label_override: None,
                supports_1m: false,
            }]
        })
        .unwrap_or_default()
}

pub(crate) fn gateway_inference_models(profile: &ProfileDraft) -> Vec<InferenceModelSpec> {
    if let Some(model) = super::super::profile_model(profile) {
        if safe_model_id(model) {
            return vec![InferenceModelSpec {
                name: model.to_string(),
                label_override: None,
                supports_1m: true,
            }];
        }
        return vec![InferenceModelSpec {
            name: DEFAULT_ROUTE_ID.to_string(),
            label_override: Some(model.to_string()),
            supports_1m: true,
        }];
    }
    default_gateway_inference_models()
}

pub(crate) fn default_gateway_inference_models() -> Vec<InferenceModelSpec> {
    DEFAULT_ROUTES
        .iter()
        .map(|(name, supports_1m)| InferenceModelSpec {
            name: (*name).to_string(),
            label_override: None,
            supports_1m: *supports_1m,
        })
        .collect()
}

pub(crate) fn safe_model_id(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.contains(ONE_M_CONTEXT_MARKER) {
        return false;
    }
    let Some(route_tail) = normalized
        .strip_prefix(ANTHROPIC_ROUTE_PREFIX)
        .or_else(|| normalized.strip_prefix(ROUTE_PREFIX))
    else {
        return false;
    };
    ["sonnet-", "opus-", "haiku-", "fable-"]
        .iter()
        .any(|prefix| {
            route_tail
                .strip_prefix(prefix)
                .map(|rest| !rest.is_empty())
                .unwrap_or(false)
        })
}

pub(in crate::core::profile) fn inference_model_json(
    spec: &InferenceModelSpec,
) -> serde_json::Value {
    if spec.supports_1m || spec.label_override.is_some() {
        let mut item = serde_json::json!({ "name": spec.name });
        if let Some(label_override) = spec.label_override.as_deref() {
            item["labelOverride"] = serde_json::json!(label_override);
        }
        if spec.supports_1m {
            item["supports1m"] = serde_json::json!(true);
        }
        item
    } else {
        serde_json::Value::String(spec.name.clone())
    }
}

pub(in crate::core::profile) fn deployment_config_content(
    current: &str,
    mode: &str,
    remove_managed_enterprise_config: bool,
) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude Desktop deployment config")?;
    ensure_object(&mut value);
    set_value_path(&mut value, &["deploymentMode"], serde_json::json!(mode));
    if remove_managed_enterprise_config {
        if let Some(enterprise) = value
            .get_mut("enterpriseConfig")
            .and_then(serde_json::Value::as_object_mut)
        {
            for key in [
                "disableDeploymentModeChooser",
                "inferenceGatewayApiKey",
                "inferenceGatewayAuthScheme",
                "inferenceGatewayBaseUrl",
                "inferenceProvider",
            ] {
                enterprise.remove(key);
            }
        }
        if value
            .get("enterpriseConfig")
            .and_then(serde_json::Value::as_object)
            .map(|object| object.is_empty())
            .unwrap_or(false)
        {
            value
                .as_object_mut()
                .map(|object| object.remove("enterpriseConfig"));
        }
    }
    render_json(value, "Claude Desktop deployment config")
}

pub(in crate::core::profile) fn developer_mode_enabled(current: &str) -> Result<bool, String> {
    let value = parse_json5_or_empty(current, "Claude Desktop developer settings")?;
    Ok(value
        .get("allowDevTools")
        .and_then(serde_json::Value::as_bool)
        == Some(true))
}

pub(in crate::core::profile) fn developer_settings_content(
    current: &str,
) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude Desktop developer settings")?;
    ensure_object(&mut value);
    set_value_path(&mut value, &["allowDevTools"], serde_json::json!(true));
    render_json(value, "Claude Desktop developer settings")
}

pub(in crate::core::profile) fn meta_content(
    current: &str,
    applied: bool,
) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude Desktop config library metadata")?;
    ensure_object(&mut value);
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Claude Desktop metadata must be a JSON object.".to_string())?;
    let mut entries = object
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    entries.retain(|entry| entry.get("id").and_then(serde_json::Value::as_str) != Some(PROFILE_ID));
    if applied {
        entries.push(serde_json::json!({ "id": PROFILE_ID, "name": PROFILE_NAME }));
        object.insert("appliedId".to_string(), serde_json::json!(PROFILE_ID));
    } else if object.get("appliedId").and_then(serde_json::Value::as_str) == Some(PROFILE_ID) {
        if let Some(next_id) = entries
            .iter()
            .find_map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
        {
            object.insert("appliedId".to_string(), serde_json::json!(next_id));
        } else {
            object.remove("appliedId");
        }
    }
    object.insert("entries".to_string(), serde_json::Value::Array(entries));
    render_json(value, "Claude Desktop config library metadata")
}

fn ensure_object(value: &mut serde_json::Value) {
    if !value.is_object() {
        *value = serde_json::Value::Object(serde_json::Map::new());
    }
}

fn parse_json5_or_empty(current: &str, label: &str) -> Result<serde_json::Value, String> {
    if current.trim().is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    json5::from_str(current)
        .map_err(|error| format!("Existing {label} could not be parsed: {error}"))
}

fn render_json(value: serde_json::Value, label: &str) -> Result<String, String> {
    serde_json::to_string_pretty(&value)
        .map(|content| format!("{content}\n"))
        .map_err(|error| format!("Generated {label} could not be serialized: {error}"))
}

fn set_value_path(root: &mut serde_json::Value, path: &[&str], value: serde_json::Value) {
    let mut current = root;
    for key in &path[..path.len() - 1] {
        ensure_object(current);
        current = current
            .as_object_mut()
            .expect("object was initialized")
            .entry((*key).to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }
    ensure_object(current);
    current
        .as_object_mut()
        .expect("object was initialized")
        .insert(path[path.len() - 1].to_string(), value);
}

pub(in crate::core::profile) fn direct_profile_content(
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = super::super::load_provider_api_key_for_direct_config(profile)?;
    direct_profile_content_with_api_key(profile, &api_key)
}

pub(in crate::core::profile) fn direct_profile_content_with_api_key(
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    super::super::require_profile_protocol(profile, &[super::super::PROTOCOL_ANTHROPIC_MESSAGES])?;
    let model_specs = direct_inference_models(profile);
    let runtime_base_url =
        super::super::profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url);
    let value = profile_value(
        &runtime_base_url,
        api_key,
        (!model_specs.is_empty()).then_some(model_specs.as_slice()),
    );
    render_json(value, "Claude Desktop 3P profile")
}

pub(in crate::core::profile) fn gateway_profile_content(
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool("claude-desktop")?;
    let model_specs = gateway_inference_models(profile);
    let value = profile_value(
        &gateway_profile_base_url(&client.base_url),
        &client.token,
        Some(model_specs.as_slice()),
    );
    render_json(value, "Claude Desktop 3P profile")
}

pub(crate) fn gateway_profile_base_url(client_base_url: &str) -> String {
    client_base_url
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or_else(|| client_base_url.trim_end_matches('/'))
        .to_string()
}

pub(crate) fn profile_value(
    base_url: &str,
    api_key: &str,
    model_specs: Option<&[InferenceModelSpec]>,
) -> serde_json::Value {
    let mut profile = serde_json::json!({
        "coworkEgressAllowedHosts": ["*"],
        "disableDeploymentModeChooser": true,
        "inferenceGatewayApiKey": api_key,
        "inferenceGatewayAuthScheme": "bearer",
        "inferenceGatewayBaseUrl": base_url,
        "inferenceProvider": "gateway"
    });
    if let Some(model_specs) = model_specs {
        profile["inferenceModels"] =
            serde_json::Value::Array(model_specs.iter().map(inference_model_json).collect());
    }
    profile
}

pub(in crate::core::profile) fn build_apply_plan(
    profile: &ProfileDraft,
    app_paths: &AppPaths,
    mode: &ProviderApplyMode,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    let desktop_paths = paths(app_paths)?;
    if *mode == ProviderApplyMode::Config && super::super::provider_is_official(&profile.provider) {
        return build_restore_official_plan(&desktop_paths);
    }
    let profile_content = match mode {
        ProviderApplyMode::Config => direct_profile_content(profile)?,
        ProviderApplyMode::Gateway => gateway_profile_content(profile)?,
    };
    let normal_current = read_file_if_exists(&desktop_paths.normal_config_path)?;
    let threep_current = read_file_if_exists(&desktop_paths.threep_config_path)?;
    let meta_current = read_file_if_exists(&desktop_paths.meta_path)?;
    let mut plans = build_developer_settings_plans(&desktop_paths)?;
    plans.extend([
        NativeConfigWritePlan::write(
            desktop_paths.normal_config_path,
            deployment_config_content(&normal_current, "3p", false)?,
            NativeConfigWriteKind::ClaudeDesktopDeploymentConfig,
        ),
        NativeConfigWritePlan::write(
            desktop_paths.threep_config_path,
            deployment_config_content(&threep_current, "3p", false)?,
            NativeConfigWriteKind::ClaudeDesktopDeploymentConfig,
        ),
        NativeConfigWritePlan::write(
            desktop_paths.profile_path,
            profile_content,
            NativeConfigWriteKind::ClaudeDesktopProfileConfig,
        ),
        NativeConfigWritePlan::write(
            desktop_paths.meta_path,
            meta_content(&meta_current, true)?,
            NativeConfigWriteKind::ClaudeDesktopMetaConfig,
        ),
    ]);
    Ok(plans)
}

fn build_restore_official_plan(
    paths: &ClaudeDesktopPaths,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    let normal_current = read_file_if_exists(&paths.normal_config_path)?;
    let threep_current = read_file_if_exists(&paths.threep_config_path)?;
    let meta_current = read_file_if_exists(&paths.meta_path)?;
    Ok(vec![
        NativeConfigWritePlan::write(
            paths.normal_config_path.clone(),
            deployment_config_content(&normal_current, "1p", false)?,
            NativeConfigWriteKind::ClaudeDesktopDeploymentConfig,
        ),
        NativeConfigWritePlan::write(
            paths.threep_config_path.clone(),
            deployment_config_content(&threep_current, "1p", true)?,
            NativeConfigWriteKind::ClaudeDesktopDeploymentConfig,
        ),
        NativeConfigWritePlan::delete(
            paths.profile_path.clone(),
            NativeConfigWriteKind::ClaudeDesktopProfileConfig,
        ),
        NativeConfigWritePlan::write(
            paths.meta_path.clone(),
            meta_content(&meta_current, false)?,
            NativeConfigWriteKind::ClaudeDesktopMetaConfig,
        ),
    ])
}

pub(in crate::core::profile) fn build_developer_settings_plans(
    paths: &ClaudeDesktopPaths,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    let mut plans = Vec::new();
    for path in &paths.developer_settings_paths {
        let current = read_file_if_exists(path)?;
        if developer_mode_enabled(&current)? {
            continue;
        }
        plans.push(NativeConfigWritePlan::write(
            path.clone(),
            developer_settings_content(&current)?,
            NativeConfigWriteKind::ClaudeDesktopDeveloperSettings,
        ));
    }
    Ok(plans)
}

fn read_file_if_exists(path: &Path) -> Result<String, String> {
    if path.exists() {
        fs::read_to_string(path).map_err(|error| error.to_string())
    } else {
        Ok(String::new())
    }
}

pub(in crate::core::profile) fn detect_native_profile(
    paths: &ClaudeDesktopPaths,
) -> Option<super::super::DetectedNativeProfile> {
    let content = fs::read_to_string(&paths.profile_path).ok()?;
    let value = parse_json5_or_empty(&content, "Claude Desktop 3P profile").ok()?;
    let base_url = json_string(&value, &["inferenceGatewayBaseUrl"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let api_key = json_string(&value, &["inferenceGatewayApiKey"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| !super::super::looks_like_local_gateway_token(value))?;
    if json_string(&value, &["inferenceProvider"]).as_deref() != Some("gateway") {
        return None;
    }
    Some(super::super::DetectedNativeProfile {
        app: "claude-desktop".to_string(),
        provider: super::super::provider_slug_from_base_url(&base_url)
            .unwrap_or_else(|| "anthropic".to_string()),
        protocol: super::super::PROTOCOL_ANTHROPIC_MESSAGES.to_string(),
        model: detected_model(&value).unwrap_or_default(),
        review_model: None,
        base_url,
        api_key,
    })
}

pub(in crate::core::profile) fn config_matches_profile(
    profile: &ProfileDraft,
    paths: Option<&ClaudeDesktopPaths>,
    official: bool,
    secret_match: super::super::SecretMatchMode,
) -> bool {
    if super::super::canonical_profile_app(&profile.app) != "claude-desktop"
        || profile.mode != ProviderApplyMode::Config
    {
        return false;
    }
    if super::super::provider_is_official(&profile.provider) {
        return official;
    }
    if super::super::normalize_protocol(Some(&profile.protocol)).as_deref()
        != Ok(super::super::PROTOCOL_ANTHROPIC_MESSAGES)
    {
        return false;
    }
    let Some(paths) = paths else {
        return false;
    };
    let content = fs::read_to_string(&paths.profile_path).unwrap_or_default();
    let Ok(value) = parse_json5_or_empty(&content, "Claude Desktop 3P profile") else {
        return false;
    };
    let model_matches = match super::super::profile_model(profile) {
        Some(model) => detected_model(&value).as_deref() == Some(model),
        None => detected_model(&value).is_none(),
    };
    let token_matches = json_string(&value, &["inferenceGatewayApiKey"])
        .map(|token| super::super::profile_api_key_matches_config(profile, &token, secret_match))
        .unwrap_or(false);
    json_string(&value, &["inferenceProvider"]).as_deref() == Some("gateway")
        && json_string(&value, &["inferenceGatewayAuthScheme"])
            .map(|scheme| scheme.eq_ignore_ascii_case("bearer"))
            .unwrap_or(true)
        && json_string(&value, &["inferenceGatewayBaseUrl"])
            .map(|base_url| {
                super::super::profile_runtime_base_url_matches(
                    &profile.protocol,
                    base_url.trim(),
                    &profile.base_url,
                )
            })
            .unwrap_or(false)
        && token_matches
        && model_matches
}

pub(in crate::core::profile) fn is_official(paths: &ClaudeDesktopPaths) -> bool {
    let normal_mode = read_json_string_from_file(&paths.normal_config_path, &["deploymentMode"]);
    let threep_mode = read_json_string_from_file(&paths.threep_config_path, &["deploymentMode"]);
    let applied_id = read_json_string_from_file(&paths.meta_path, &["appliedId"]);
    normal_mode.as_deref().unwrap_or("1p") == "1p"
        && threep_mode.as_deref().unwrap_or("1p") == "1p"
        && applied_id.as_deref() != Some(PROFILE_ID)
        && !paths.profile_path.exists()
}

fn detected_model(value: &serde_json::Value) -> Option<String> {
    value
        .get("inferenceModels")?
        .as_array()?
        .first()
        .and_then(|model| {
            model
                .as_str()
                .map(ToString::to_string)
                .or_else(|| json_string(model, &["labelOverride"]))
                .or_else(|| json_string(model, &["name"]))
        })
}

fn read_json_string_from_file(path: &Path, keys: &[&str]) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| parse_json5_or_empty(&content, "native config").ok())
        .and_then(|value| json_string(&value, keys))
}

fn json_string(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(ToString::to_string)
}

pub(in crate::core::profile) fn verify_write(
    kind: NativeConfigWriteKind,
    path: &Path,
    profile: &ProfileDraft,
    mode: ProviderApplyMode,
) -> Result<bool, String> {
    match kind {
        NativeConfigWriteKind::ClaudeDesktopDeploymentConfig => {
            let expected = if mode == ProviderApplyMode::Config
                && super::super::provider_is_official(&profile.provider)
            {
                "1p"
            } else {
                "3p"
            };
            let value = parse_json5_or_empty(
                &fs::read_to_string(path).map_err(|error| error.to_string())?,
                "Claude Desktop deployment config",
            )?;
            Ok(json_string(&value, &["deploymentMode"]).as_deref() == Some(expected))
        }
        NativeConfigWriteKind::ClaudeDesktopDeveloperSettings => {
            developer_mode_enabled(&fs::read_to_string(path).map_err(|error| error.to_string())?)
        }
        NativeConfigWriteKind::ClaudeDesktopProfileConfig => {
            if mode == ProviderApplyMode::Config
                && super::super::provider_is_official(&profile.provider)
            {
                return Ok(!path.exists());
            }
            let value = parse_json5_or_empty(
                &fs::read_to_string(path).map_err(|error| error.to_string())?,
                "Claude Desktop 3P profile",
            )?;
            let (expected_base_url, expected_api_key) = match mode {
                ProviderApplyMode::Config => (
                    super::super::profile_runtime_base_url_for_protocol(
                        &profile.protocol,
                        &profile.base_url,
                    ),
                    super::super::load_provider_api_key_for_direct_config(profile)?,
                ),
                ProviderApplyMode::Gateway => {
                    let client = gateway::client_config_for_tool("claude-desktop")?;
                    (gateway_profile_base_url(&client.base_url), client.token)
                }
            };
            Ok(
                json_string(&value, &["inferenceProvider"]).as_deref() == Some("gateway")
                    && json_string(&value, &["inferenceGatewayAuthScheme"]).as_deref()
                        == Some("bearer")
                    && json_string(&value, &["inferenceGatewayBaseUrl"]).as_deref()
                        == Some(expected_base_url.as_str())
                    && json_string(&value, &["inferenceGatewayApiKey"]).as_deref()
                        == Some(expected_api_key.as_str()),
            )
        }
        NativeConfigWriteKind::ClaudeDesktopMetaConfig => {
            let value = parse_json5_or_empty(
                &fs::read_to_string(path).map_err(|error| error.to_string())?,
                "Claude Desktop config library metadata",
            )?;
            let applied = !(mode == ProviderApplyMode::Config
                && super::super::provider_is_official(&profile.provider));
            let has_entry = value
                .get("entries")
                .and_then(serde_json::Value::as_array)
                .map(|entries| {
                    entries.iter().any(|entry| {
                        entry.get("id").and_then(serde_json::Value::as_str) == Some(PROFILE_ID)
                    })
                })
                .unwrap_or(false);
            let applied_id = json_string(&value, &["appliedId"]);
            if applied {
                Ok(has_entry && applied_id.as_deref() == Some(PROFILE_ID))
            } else {
                Ok(!has_entry && applied_id.as_deref() != Some(PROFILE_ID))
            }
        }
        _ => Err("Claude Desktop verifier received an unrelated write kind.".to_string()),
    }
}

pub(in crate::core::profile) fn preview(
    profile: &ProfileDraft,
    native_config_path: Option<&str>,
    app_paths: &AppPaths,
    mode: ProviderApplyMode,
) -> Result<Option<NativeConfigPreview>, String> {
    if mode == ProviderApplyMode::Config
        && !super::super::provider_is_official(&profile.provider)
        && !super::super::config_file_protocol_supported(profile)
    {
        return Ok(None);
    }
    let desktop_paths = paths(app_paths)?;
    let path = native_config_path
        .map(ToString::to_string)
        .unwrap_or_else(|| display_path(&desktop_paths.profile_path));
    let mut warnings = match mode {
        ProviderApplyMode::Config if super::super::provider_is_official(&profile.provider) => vec![
            "Claude Desktop official mode restores deploymentMode=1p and removes the CodeStudio Lite 3P profile entry.".to_string(),
            "No Provider API key or model override is required.".to_string(),
        ],
        ProviderApplyMode::Config => vec![
            "Claude Desktop config profile writes the 3P profile system used by Claude Desktop.".to_string(),
            "CodeStudio Lite enables Claude Desktop developer mode before writing the 3P profile if it is not already enabled.".to_string(),
            "The selected endpoint must be Anthropic Messages compatible; generic OpenAI-only endpoints need Gateway profiles.".to_string(),
            "Restart Claude Desktop after applying so it reloads the config library.".to_string(),
        ],
        ProviderApplyMode::Gateway => vec![
            "Claude Desktop gateway profile writes the 3P profile to the tool-scoped CodeStudio Lite Local Gateway URL.".to_string(),
            "CodeStudio Lite enables Claude Desktop developer mode before writing the Gateway profile if it is not already enabled.".to_string(),
            "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.".to_string(),
            "Restart Claude Desktop after applying so it reloads the config library.".to_string(),
        ],
    };
    warnings.push(format!(
        "Also updates {} and {}.",
        display_path(&desktop_paths.normal_config_path),
        display_path(&desktop_paths.threep_config_path)
    ));
    warnings.push(format!(
        "Also updates {}.",
        display_path(&desktop_paths.meta_path)
    ));
    let (json, status) = super::super::read_json_preview(
        &desktop_paths.profile_path,
        "Claude Desktop 3P profile",
        &mut warnings,
    )?;
    let changes = match mode {
        ProviderApplyMode::Config if super::super::provider_is_official(&profile.provider) => vec![
            super::super::diff_value_line(
                "deploymentMode".to_string(),
                None,
                Some("1p".to_string()),
                "Restores Claude Desktop to first-party official mode in both config files.",
            ),
            super::super::diff_value_line(
                "configLibrary/_meta.appliedId".to_string(),
                None,
                None,
                "Removes the CodeStudio Lite profile from Claude Desktop's 3P config library.",
            ),
            super::super::diff_value_line(
                format!("{PROFILE_ID}.json"),
                None,
                None,
                "Deletes the managed CodeStudio Lite Claude Desktop 3P profile file.",
            ),
        ],
        ProviderApplyMode::Config => {
            let model_specs = direct_inference_models(profile);
            let runtime_base_url = super::super::profile_runtime_base_url_for_protocol(
                &profile.protocol,
                &profile.base_url,
            );
            let mut changes = vec![
                super::super::diff_value_line("developer_settings.allowDevTools".to_string(), None, Some("true".to_string()), "Enables Claude Desktop developer mode before applying the managed 3P profile."),
                super::super::diff_value_line("deploymentMode".to_string(), None, Some("3p".to_string()), "Switches Claude Desktop to third-party provider mode in both config files."),
                super::super::json_diff_line(&json, &["inferenceProvider"], "gateway", "Uses Claude Desktop's built-in 3P inference gateway provider."),
                super::super::json_diff_line(&json, &["inferenceGatewayAuthScheme"], "bearer", "Authenticates the 3P profile with a bearer token."),
                super::super::json_diff_line(&json, &["inferenceGatewayBaseUrl"], &runtime_base_url, "Points Claude Desktop directly at the selected Anthropic-compatible Provider Base URL."),
                super::super::json_diff_line(&json, &["inferenceGatewayApiKey"], super::super::secret_preview(profile), "Stores the selected Provider API key in Claude Desktop's 3P profile."),
            ];
            if model_specs.is_empty() {
                changes.push(super::super::json_diff_remove_line(
                    &json,
                    &["inferenceModels"],
                    "Model is optional; no Claude Desktop model menu override will be written.",
                ));
            } else {
                changes.push(super::super::json_diff_line(
                    &json,
                    &["inferenceModels"],
                    &model_specs_preview(&model_specs),
                    "Exposes the selected Claude-safe model in Claude Desktop's model menu.",
                ));
            }
            changes
        }
        ProviderApplyMode::Gateway => {
            let client = gateway::client_config_for_tool("claude-desktop")?;
            let base_url = gateway_profile_base_url(&client.base_url);
            let model_specs = gateway_inference_models(profile);
            vec![
                super::super::diff_value_line("developer_settings.allowDevTools".to_string(), None, Some("true".to_string()), "Enables Claude Desktop developer mode before applying the managed Gateway profile."),
                super::super::diff_value_line("deploymentMode".to_string(), None, Some("3p".to_string()), "Switches Claude Desktop to third-party provider mode in both config files."),
                super::super::json_diff_line(&json, &["inferenceProvider"], "gateway", "Uses Claude Desktop's built-in 3P inference gateway provider."),
                super::super::json_diff_line(&json, &["inferenceGatewayAuthScheme"], "bearer", "Authenticates the 3P profile with the local CodeStudio token."),
                super::super::json_diff_line(&json, &["inferenceGatewayBaseUrl"], &base_url, "Points Claude Desktop at the tool-scoped CodeStudio Lite Local Gateway."),
                super::super::json_diff_line(&json, &["inferenceGatewayApiKey"], &client.token_preview, "Stores only the local CodeStudio token, not the real upstream Provider API key."),
                super::super::json_diff_line(&json, &["inferenceModels"], &model_specs_preview(&model_specs), "Exposes Claude Desktop-safe route IDs while the Gateway resolves the real upstream model."),
            ]
        }
    };
    Ok(Some(NativeConfigPreview {
        tool: "claude-desktop".to_string(),
        path,
        status,
        write_enabled: true,
        changes,
        warnings,
        content: None,
    }))
}

fn model_specs_preview(specs: &[InferenceModelSpec]) -> String {
    serde_json::to_string(&serde_json::Value::Array(
        specs.iter().map(inference_model_json).collect(),
    ))
    .unwrap_or_else(|_| "[]".to_string())
}
