use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::backup;
use crate::core::chatgpt_desktop;
use crate::core::credentials;
use crate::core::detector;
use crate::core::env_health;
use crate::core::gateway;
use crate::core::platform::{
    hidden_command, hidden_command_with_args, package, resolve_command, run_powershell,
};
use crate::core::storage;
use crate::core::tool_catalog::{
    self, canonical_tool_id as canonical_profile_app, supports_config_protocol,
};
use crate::core::types::{
    ActiveProfilesByMode, AppSettings, ApplyProfileRequest, ApplyProfileResult, CodexAuthMethod,
    CodexAuthStatus, CodexAuthStorage, ConfigState, DeleteProfileDraftRequest,
    DuplicateProfileDraftRequest, InstallState, ListProfileModelsRequest, ListProfileModelsResult,
    NativeConfigDiffLine, NativeConfigPreview, PreviewProfileApplyRequest,
    PreviewProfileApplyResult, PreviewProfileWriteRequest, PreviewProfileWriteResult,
    ProfileApplyPreviewItem, ProfileConnectionCheck, ProfileDraft, ProfileModelMapping,
    ProfileModelOption, ProfileSummary, ProfileWritePreviewItem, ProviderApplyMode,
    ProviderApplyModePreview, ReorderProfileDraftsRequest, SaveProfileDraftRequest, Severity,
    StartCodexOAuthLoginResult, SwitchActiveProfileRequest, TestProfileConnectionRequest,
    TestProfileConnectionResult, UpdateAppSettingsRequest, UpdateProfileDraftRequest,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

mod manager;
mod native;
mod policy;
mod provider_http;
mod restart;
mod store;
#[cfg(test)]
use manager::attach_native_config_content_preview;
use manager::sync_active_profiles_from_native_configs;
pub use manager::{
    apply_profile, delete_profile_draft, duplicate_profile_draft, preview_profile_apply,
    preview_profile_write, reorder_profile_drafts, save_profile_draft, switch_active_profile,
    update_profile_draft,
};
#[cfg(test)]
use manager::{
    claude_desktop_config_matches_profile, detected_native_profile_matches_existing_key,
    detected_native_profile_matches_existing_reference, is_auto_detected_native_profile_name,
    normalize_detected_provider, sync_codex_config_profile, sync_native_config_profile,
};
use manager::{native_optional_model, provider_slug_from_base_url};
#[cfg(test)]
use native::claude::*;
#[cfg(test)]
use native::codex::{
    codex_auth_json_content_with_api_key, codex_auth_json_path, codex_direct_config_content,
    codex_direct_config_matches_profile, codex_direct_config_matches_profile_without_keychain,
    codex_gateway_config_content, codex_official_auth_json_content, codex_official_config_content,
    codex_official_config_matches_profile, detect_codex_native_profile,
    detect_codex_native_profile_with_auth,
    provider_id_for_profile as codex_provider_id_for_profile, repair_codex_preserved_auth_config,
    verify_codex_direct_config, verify_codex_native_config, CODEX_ACTOR_AUTHORIZATION_HEADER,
    CODEX_ACTOR_AUTHORIZATION_INLINE_TOML, CODEX_ACTOR_AUTHORIZATION_VALUE,
};
#[cfg(test)]
use native::gemini::*;
#[cfg(test)]
use native::gemini_code_assist::*;
#[cfg(test)]
use native::grok::*;
#[cfg(test)]
use native::hermes::*;
#[cfg(test)]
use native::openclaw::*;
#[cfg(test)]
use native::opencode::*;
#[cfg(test)]
use native::pi::*;
#[cfg(test)]
pub(crate) use native::plan::write_native_config;
use native::plan::{
    apply_native_config_write_plan, filter_native_write_plans, NativeConfigLifecyclePlan,
    NativeConfigWriteKind, NativeConfigWritePlan,
};
use policy::*;
pub use provider_http::{list_profile_models, test_profile_connection};
#[cfg(test)]
use provider_http::{profile_model_list_url, profile_model_options_from_payload};
use restart::*;
use store::*;

pub(crate) fn load_profile_by_id(profile_id: &str) -> Result<ProfileDraft, String> {
    store::load_profile_by_id(profile_id)
}

#[derive(Debug, Serialize, Deserialize)]
struct AppConfig {
    #[serde(default)]
    active_profiles_by_mode: ActiveProfilesByMode,
    ui: UiConfig,
    security: SecurityConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct UiConfig {
    theme: String,
    language: String,
    language_set_by_user: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SecurityConfig {
    backup_before_write: bool,
    redact_secrets: bool,
    confirm_install_commands: bool,
    confirm_config_writes: bool,
}

struct ProfileWritePlan {
    id: String,
    name: String,
    app: String,
    mode: ProviderApplyMode,
    provider: String,
    protocol: String,
    model: String,
    base_url: String,
    secret_status: &'static str,
    auth_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetectedNativeProfile {
    app: String,
    provider: String,
    protocol: String,
    model: String,
    review_model: Option<String>,
    base_url: String,
    api_key: String,
}

const BUILTIN_OFFICIAL_ID_PREFIX: &str = "builtin-official-";
const PROTOCOL_OPENAI_CHAT_COMPLETIONS: &str = "openai-chat-completions";
const PROTOCOL_OPENAI_RESPONSES: &str = "openai-responses";
const PROTOCOL_ANTHROPIC_MESSAGES: &str = "anthropic-messages";
const PROTOCOL_GOOGLE_GEMINI: &str = "google-gemini";
const GATEWAY_FALLBACK_MODEL: &str = "default";
const CLAUDE_VSCODE_PLUGIN_PRIMARY_API_KEY: &str = "any";
use native::claude_desktop::{
    build_apply_plan as build_claude_desktop_apply_plan,
    build_developer_settings_plans as build_claude_desktop_developer_settings_plans,
    detect_native_profile as detect_claude_desktop_native_profile,
    developer_mode_enabled as claude_desktop_developer_mode_enabled,
    direct_profile_content_with_api_key as claude_desktop_direct_profile_content_with_api_key,
    gateway_profile_content as claude_desktop_gateway_profile_content,
    is_official as claude_desktop_is_official, paths as claude_desktop_paths, ClaudeDesktopPaths,
};
pub(crate) use native::claude_desktop::{
    default_gateway_inference_models as claude_desktop_default_gateway_inference_models,
    gateway_inference_models as claude_desktop_gateway_inference_models,
};
#[cfg(test)]
use native::claude_desktop::{
    deployment_config_content as claude_desktop_deployment_config_content,
    developer_settings_content as claude_desktop_developer_settings_content,
    gateway_profile_base_url as claude_desktop_gateway_profile_base_url,
    macos_developer_settings_paths as macos_claude_desktop_developer_settings_paths,
    meta_content as claude_desktop_meta_content, paths_from_dirs as claude_desktop_paths_from_dirs,
    profile_value as claude_desktop_gateway_profile_value,
    safe_model_id as claude_desktop_safe_model_id,
    InferenceModelSpec as ClaudeDesktopInferenceModelSpec, PROFILE_ID as CLAUDE_DESKTOP_PROFILE_ID,
};
const BUILTIN_OFFICIAL_PROFILES: [(&str, &str, &str); 10] = [
    ("codex", "Codex Official", PROTOCOL_OPENAI_RESPONSES),
    (
        "claude-desktop",
        "Claude Desktop Official",
        PROTOCOL_ANTHROPIC_MESSAGES,
    ),
    (
        "claude",
        "Claude Code Official",
        PROTOCOL_ANTHROPIC_MESSAGES,
    ),
    ("gemini", "Gemini CLI Official", PROTOCOL_GOOGLE_GEMINI),
    (
        "gemini-code-assist",
        "Gemini Code Assist Official",
        PROTOCOL_GOOGLE_GEMINI,
    ),
    (
        "opencode",
        "OpenCode Official",
        PROTOCOL_OPENAI_CHAT_COMPLETIONS,
    ),
    (
        "openclaw",
        "OpenClaw Official",
        PROTOCOL_OPENAI_CHAT_COMPLETIONS,
    ),
    (
        "hermes",
        "Hermes Official",
        PROTOCOL_OPENAI_CHAT_COMPLETIONS,
    ),
    ("grok", "Grok Official", PROTOCOL_OPENAI_RESPONSES),
    ("pi", "Pi Agent Official", PROTOCOL_ANTHROPIC_MESSAGES),
];

pub(crate) fn profile_runtime_base_url_for_protocol(protocol: &str, base_url: &str) -> String {
    profile_runtime_base_url_with_v1_policy(base_url, protocol_uses_openai_v1_base(protocol))
}

fn profile_runtime_base_url_with_v1_policy(base_url: &str, add_v1: bool) -> String {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let trimmed = trimmed.trim_end_matches('/');
    let fallback = || {
        if add_v1 {
            let path = runtime_base_path_with_v1(trimmed);
            if path == trimmed {
                trimmed.to_string()
            } else {
                path
            }
        } else {
            trimmed.to_string()
        }
    };

    let Ok(mut parsed) = Url::parse(trimmed) else {
        return fallback();
    };
    if parsed.scheme().is_empty() || parsed.host_str().is_none() {
        return fallback();
    }

    let path = if add_v1 {
        runtime_base_path_with_v1(parsed.path())
    } else {
        parsed.path().trim_end_matches('/').to_string()
    };
    parsed.set_path(&path);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

fn profile_runtime_base_url_matches(
    protocol: &str,
    configured: &str,
    profile_base_url: &str,
) -> bool {
    profile_runtime_base_url_for_protocol(protocol, configured)
        == profile_runtime_base_url_for_protocol(protocol, profile_base_url)
}

fn protocol_uses_openai_v1_base(protocol: &str) -> bool {
    matches!(
        normalize_protocol(Some(protocol)).as_deref(),
        Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS) | Ok(PROTOCOL_OPENAI_RESPONSES)
    )
}

fn runtime_base_path_with_v1(path: &str) -> String {
    let clean = path.trim_end_matches('/');
    let last_segment = clean.rsplit('/').find(|segment| !segment.is_empty());
    if last_segment
        .map(|segment| {
            segment.eq_ignore_ascii_case("v1") || runtime_base_path_segment_is_version(segment)
        })
        .unwrap_or(false)
    {
        return if clean.is_empty() {
            "/v1".to_string()
        } else {
            clean.to_string()
        };
    }

    if clean.is_empty() {
        "/v1".to_string()
    } else {
        format!("{clean}/v1")
    }
}

fn runtime_base_path_segment_is_version(segment: &str) -> bool {
    let segment = segment.trim();
    let mut chars = segment.chars();
    if !matches!(chars.next(), Some('v' | 'V')) {
        return false;
    }

    let mut has_digit = false;
    for ch in chars {
        if ch.is_ascii_digit() {
            has_digit = true;
            continue;
        }
        if !has_digit || !(ch.is_ascii_alphabetic() || matches!(ch, '-' | '_' | '.')) {
            return false;
        }
    }
    has_digit
}

pub fn ensure_app_dirs() -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    storage::ensure_initialized()?;

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct LoadProfileSummaryOptions {
    /// When true, reconcile active config pointers from on-disk tool configs and
    /// import unmatched native configs as drafts. Only environment detection
    /// should enable this; mutation follow-ups must not, or a just-edited
    /// in-app profile can race the still-old on-disk config and spawn ghosts.
    sync_native: bool,
}

pub fn load_profile_summary() -> Result<ProfileSummary, String> {
    load_profile_summary_with_options(LoadProfileSummaryOptions { sync_native: true })
}

pub fn load_profile_summary_without_native_sync() -> Result<ProfileSummary, String> {
    load_profile_summary_with_options(LoadProfileSummaryOptions { sync_native: false })
}

fn load_profile_summary_with_options(
    options: LoadProfileSummaryOptions,
) -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mut config = read_app_config()?;
    let mut drafts = load_profiles()?;
    let mut active_profiles_changed = clean_active_profiles(&mut config, &drafts);
    if options.sync_native {
        active_profiles_changed |=
            sync_active_profiles_from_native_configs(&mut config, &mut drafts, &paths)?;
    }
    if active_profiles_changed {
        write_app_config(&config)?;
    }
    let active_profile =
        default_active_profile_id(&config.active_profiles_by_mode.gateway, &drafts);
    let active_profile_name = active_profile
        .as_ref()
        .and_then(|active_id| drafts.iter().find(|profile| profile.id == *active_id))
        .map(|profile| profile.name.clone());

    Ok(ProfileSummary {
        config_dir: display_path(&paths.config_dir),
        active_profile,
        active_profile_name,
        active_profiles_by_mode: config.active_profiles_by_mode,
        codex_auth: codex_auth_status(),
        drafts,
    })
}

pub fn load_app_settings() -> Result<AppSettings, String> {
    ensure_app_dirs()?;
    let config = read_app_config()?;
    Ok(settings_from_config(&config))
}

pub fn codex_auth_status() -> CodexAuthStatus {
    detect_codex_auth_status().unwrap_or_else(|err| CodexAuthStatus {
        available: false,
        method: CodexAuthMethod::Unknown,
        storage: CodexAuthStorage::Unknown,
        path: None,
        detail: format!("Codex auth status could not be inspected: {err}"),
    })
}

pub fn start_codex_oauth_login() -> Result<StartCodexOAuthLoginResult, String> {
    let codex = resolve_command("codex")
        .ok_or_else(|| "Codex CLI is not installed or is not on PATH.".to_string())?;
    let mut command = hidden_command_with_args(&codex, &["login"]);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("Failed to start Codex official login: {err}"))?;
    Ok(StartCodexOAuthLoginResult {
        started: true,
        command: Some(format!("{codex} login")),
        message: "Codex official login started. Complete the browser authorization, then return to CodeStudio Lite.".to_string(),
    })
}

pub fn update_app_settings(request: UpdateAppSettingsRequest) -> Result<AppSettings, String> {
    ensure_app_dirs()?;
    let mut config = read_app_config()?;

    if let Some(theme) = request.theme {
        config.ui.theme = normalize_theme(&theme)?;
    }
    if let Some(language) = request.language {
        config.ui.language = normalize_language(&language)?;
        config.ui.language_set_by_user = true;
    }

    write_app_config(&config)?;
    activity_log::append(
        Severity::Info,
        format!(
            "Updated application settings: language={}, theme={}.",
            config.ui.language, config.ui.theme
        ),
    )?;

    Ok(settings_from_config(&config))
}

pub(crate) fn apply_active_gateway_native_configs() -> Result<usize, String> {
    apply_active_native_configs(
        ProviderApplyMode::Gateway,
        false,
        "gateway-start-native-config",
    )
}

pub(crate) fn restore_active_config_native_configs() -> Result<usize, String> {
    apply_active_native_configs(
        ProviderApplyMode::Config,
        true,
        "gateway-stop-native-config",
    )
}

fn apply_active_native_configs(
    mode: ProviderApplyMode,
    include_gateway_targets: bool,
    backup_reason: &str,
) -> Result<usize, String> {
    ensure_app_dirs()?;

    let paths = app_paths().map_err(|err| err.to_string())?;
    let profiles = load_profiles()?;
    let mut config = read_app_config()?;
    if clean_active_profiles(&mut config, &profiles) {
        write_app_config(&config)?;
    }

    let target_apps = lifecycle_target_apps(
        &config.active_profiles_by_mode,
        mode,
        include_gateway_targets,
    );
    let mut lifecycle_plans = Vec::new();
    let mut errors = Vec::new();

    for app in target_apps {
        let profile =
            active_profile_for_lifecycle_app(&config, &profiles, &app, mode).or_else(|| {
                if mode == ProviderApplyMode::Config {
                    default_official_profile_for_app(&profiles, &app)
                } else {
                    None
                }
            });

        let Some(profile) = profile else {
            continue;
        };

        match build_native_apply_plan(&profile, &paths, &mode, false)
            .and_then(filter_native_write_plans)
        {
            Ok(plans) if !plans.is_empty() => {
                lifecycle_plans.extend(plans.into_iter().map(|plan| NativeConfigLifecyclePlan {
                    profile: profile.clone(),
                    mode,
                    plan,
                    verify_after_write: true,
                }));
            }
            Ok(_)
                if mode == ProviderApplyMode::Config && provider_is_official(&profile.provider) =>
            {
                match build_gateway_cleanup_plan(&profile, &paths) {
                    Ok(plans) => {
                        lifecycle_plans.extend(plans.into_iter().map(|plan| {
                            NativeConfigLifecyclePlan {
                                profile: profile.clone(),
                                mode,
                                plan,
                                verify_after_write: false,
                            }
                        }));
                    }
                    Err(err) => errors.push(format!("{}: {err}", profile.app)),
                }
            }
            Ok(_) => {}
            Err(err) => errors.push(format!("{}: {err}", profile.app)),
        }
    }

    if !errors.is_empty() {
        return Err(format!(
            "Could not prepare native config lifecycle writes: {}",
            errors.join("; ")
        ));
    }

    if lifecycle_plans.is_empty() {
        return Ok(0);
    }

    let backup_targets = lifecycle_plans
        .iter()
        .map(|item| item.plan.path.clone())
        .collect::<Vec<_>>();
    backup::backup_files(backup_reason, None, &backup_targets)?;

    let mut written = 0usize;
    let mut write_errors = Vec::new();
    for lifecycle_plan in lifecycle_plans {
        match apply_native_config_write_plan(&lifecycle_plan.plan).and_then(|_| {
            if lifecycle_plan.verify_after_write {
                verify_native_config_write(
                    &lifecycle_plan.plan,
                    &lifecycle_plan.profile,
                    &lifecycle_plan.mode,
                )
                .and_then(|verified| {
                    if verified {
                        Ok(())
                    } else {
                        Err("native config verification failed".to_string())
                    }
                })
            } else {
                Ok(())
            }
        }) {
            Ok(()) => written += 1,
            Err(err) => write_errors.push(format!(
                "{} at {}: {err}",
                lifecycle_plan.profile.app,
                display_path(&lifecycle_plan.plan.path)
            )),
        }
    }

    if !write_errors.is_empty() {
        return Err(format!(
            "Could not complete native config lifecycle writes: {}",
            write_errors.join("; ")
        ));
    }

    Ok(written)
}

fn lifecycle_target_apps(
    active_profiles: &ActiveProfilesByMode,
    mode: ProviderApplyMode,
    include_gateway_targets: bool,
) -> Vec<String> {
    let mut apps = match mode {
        ProviderApplyMode::Config => active_profiles.config.keys().cloned().collect::<Vec<_>>(),
        ProviderApplyMode::Gateway => active_profiles.gateway.keys().cloned().collect::<Vec<_>>(),
    };

    if include_gateway_targets {
        apps.extend(active_profiles.gateway.keys().cloned());
    }

    let mut apps = apps
        .into_iter()
        .map(|app| canonical_profile_app(&app))
        .collect::<Vec<_>>();
    apps.sort();
    apps.dedup();
    apps
}

fn active_profile_for_lifecycle_app(
    config: &AppConfig,
    profiles: &[ProfileDraft],
    app: &str,
    mode: ProviderApplyMode,
) -> Option<ProfileDraft> {
    let active_profiles = match mode {
        ProviderApplyMode::Config => &config.active_profiles_by_mode.config,
        ProviderApplyMode::Gateway => &config.active_profiles_by_mode.gateway,
    };
    let active_id = active_profile_id_for_app(active_profiles, app)?;
    profiles
        .iter()
        .find(|profile| {
            profile.id == *active_id
                && canonical_profile_app(&profile.app) == app
                && profile.mode == mode
        })
        .cloned()
}

fn default_official_profile_for_app(profiles: &[ProfileDraft], app: &str) -> Option<ProfileDraft> {
    profiles
        .iter()
        .find(|profile| {
            canonical_profile_app(&profile.app) == app
                && profile.mode == ProviderApplyMode::Config
                && provider_is_official(&profile.provider)
        })
        .cloned()
}
fn profile_is_active(config: &AppConfig, profile: &ProfileDraft) -> bool {
    let active_profiles = match profile.mode {
        ProviderApplyMode::Config => &config.active_profiles_by_mode.config,
        ProviderApplyMode::Gateway => &config.active_profiles_by_mode.gateway,
    };
    let app = canonical_profile_app(&profile.app);
    active_profile_id_for_app(active_profiles, &app)
        .map(|active_id| active_id == &profile.id)
        .unwrap_or(false)
}

fn active_profile_id_for_app<'a>(
    active_profiles: &'a HashMap<String, String>,
    app: &str,
) -> Option<&'a String> {
    active_profiles.get(app).or_else(|| {
        if app == "codex" {
            active_profiles
                .get("chatgpt-desktop")
                .or_else(|| active_profiles.get("codex-app"))
                .or_else(|| active_profiles.get("codex-client"))
                .or_else(|| active_profiles.get("codex-desktop"))
        } else {
            None
        }
    })
}

fn verify_active_profile(config: &AppConfig, profile: &ProfileDraft) -> bool {
    profile_is_active(config, profile)
}

fn claude_vscode_plugin_config_matches(value: &serde_json::Value) -> bool {
    json_string_lookup(value, &["primaryApiKey"]).as_deref()
        == Some(CLAUDE_VSCODE_PLUGIN_PRIMARY_API_KEY)
}

fn managed_json_provider_key(key: &str) -> bool {
    key == "custom" || key == "codestudio" || key.starts_with("codestudio-")
}

fn remove_json_managed_provider_entries(root: &mut serde_json::Value, path: &[&str]) {
    for provider_id in json_object_keys(root, path)
        .into_iter()
        .filter(|provider_id| managed_json_provider_key(provider_id))
        .collect::<Vec<_>>()
    {
        let mut provider_path = path.to_vec();
        provider_path.push(&provider_id);
        remove_json_path(root, &provider_path);
    }
}

#[derive(Clone, Copy)]
enum SecretMatchMode {
    ExactKeychain,
    KeychainReference,
}

fn profile_api_key_matches_config_by_reading_keychain(profile: &ProfileDraft, token: &str) -> bool {
    profile_api_key_matches_config(profile, token, SecretMatchMode::ExactKeychain)
}

fn profile_api_key_matches_config_without_keychain(profile: &ProfileDraft, token: &str) -> bool {
    profile_api_key_matches_config(profile, token, SecretMatchMode::KeychainReference)
}

fn profile_api_key_matches_config(
    profile: &ProfileDraft,
    token: &str,
    secret_match: SecretMatchMode,
) -> bool {
    if token.trim().is_empty() || looks_like_local_gateway_token(token) {
        return false;
    }

    if matches!(secret_match, SecretMatchMode::KeychainReference) {
        return profile
            .auth_ref
            .as_deref()
            .map(str::trim)
            .map(|auth_ref| auth_ref.starts_with("keychain:") && auth_ref.len() > "keychain:".len())
            .unwrap_or(false);
    }

    let Some(auth_ref) = profile.auth_ref.as_deref() else {
        return false;
    };
    credentials::load_keychain_secret(auth_ref)
        .map(|expected| expected.trim() == token.trim())
        .unwrap_or(false)
}

fn rewrite_native_configs_for_profile(
    profile: &ProfileDraft,
    backup_reason: &str,
) -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mode = profile.mode;
    let plans = filter_native_write_plans(build_native_apply_plan(profile, &paths, &mode, false)?)?;
    if plans.is_empty() {
        return Ok(());
    }

    let backup_targets = plans
        .iter()
        .map(|plan| plan.path.clone())
        .collect::<Vec<_>>();
    backup::backup_files(backup_reason, Some(&profile.id), &backup_targets)?;
    for plan in &plans {
        apply_native_config_write_plan(plan)?;
        if !verify_native_config_write(plan, profile, &mode)? {
            return Err(format!(
                "Native config verification failed while updating active profile '{}' at {}",
                profile.name,
                display_path(&plan.path)
            ));
        }
    }
    Ok(())
}

fn read_app_config() -> Result<AppConfig, String> {
    let stored = storage::load_app_config()?;
    Ok(AppConfig {
        active_profiles_by_mode: stored.active_profiles_by_mode,
        ui: UiConfig {
            theme: stored.theme,
            language: stored.language,
            language_set_by_user: stored.language_set_by_user,
        },
        security: SecurityConfig {
            backup_before_write: stored.backup_before_write,
            redact_secrets: stored.redact_secrets,
            confirm_install_commands: stored.confirm_install_commands,
            confirm_config_writes: stored.confirm_config_writes,
        },
    })
}

fn write_app_config(config: &AppConfig) -> Result<(), String> {
    storage::save_app_config(&storage::StoredAppConfig {
        active_profiles_by_mode: config.active_profiles_by_mode.clone(),
        theme: config.ui.theme.clone(),
        language: config.ui.language.clone(),
        language_set_by_user: config.ui.language_set_by_user,
        backup_before_write: config.security.backup_before_write,
        redact_secrets: config.security.redact_secrets,
        confirm_install_commands: config.security.confirm_install_commands,
        confirm_config_writes: config.security.confirm_config_writes,
    })
}

fn settings_from_config(config: &AppConfig) -> AppSettings {
    AppSettings {
        theme: config.ui.theme.clone(),
        language: config.ui.language.clone(),
        backup_before_write: config.security.backup_before_write,
        redact_secrets: config.security.redact_secrets,
        confirm_install_commands: config.security.confirm_install_commands,
        confirm_config_writes: config.security.confirm_config_writes,
    }
}

fn detect_codex_auth_status() -> Result<CodexAuthStatus, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    let codex_dir = paths.home_dir.join(".codex");
    let config_path = codex_dir.join("config.toml");
    let auth_path = codex_dir.join("auth.json");
    let configured_store = read_codex_credentials_store(&config_path);
    let configured_store = configured_store.as_deref();

    if auth_path.exists() {
        return Ok(codex_auth_status_from_file(
            &auth_path,
            configured_store.unwrap_or("file"),
        ));
    }

    if let Some(storage) = codex_credentials_store_from_str(configured_store) {
        if matches!(
            storage,
            CodexAuthStorage::Keyring | CodexAuthStorage::Auto | CodexAuthStorage::Unknown
        ) {
            return Ok(CodexAuthStatus {
                available: false,
                method: CodexAuthMethod::Unknown,
                storage,
                path: None,
                detail:
                    "Codex is configured to use the OS credential store; CodeStudio Lite cannot verify keyring contents safely."
                        .to_string(),
            });
        }
    }

    Ok(CodexAuthStatus {
        available: false,
        method: CodexAuthMethod::None,
        storage: CodexAuthStorage::None,
        path: Some(display_path(&auth_path)),
        detail: "No Codex auth.json login cache was found.".to_string(),
    })
}

fn is_custom_codex_oauth_profile(profile: &ProfileDraft) -> bool {
    !profile.is_builtin
        && is_codex_family_app(&profile.app)
        && provider_is_official(&profile.provider)
        && profile.mode == ProviderApplyMode::Config
}

fn capture_codex_oauth_profile_if_needed(profile: &ProfileDraft) -> Result<(), String> {
    if !is_custom_codex_oauth_profile(profile) {
        return Ok(());
    }
    let paths = app_paths().map_err(|err| err.to_string())?;
    let source_path = native::codex::auth_json_path(&paths);
    let content = fs::read_to_string(&source_path).map_err(|err| {
        format!(
            "Codex OAuth auth.json could not be read at {}: {err}",
            display_path(&source_path)
        )
    })?;
    let status = codex_auth_status_from_file_content(&source_path, "file", &content);
    if !matches!(
        status.method,
        CodexAuthMethod::ChatGpt | CodexAuthMethod::AccessToken
    ) {
        return Err(
            "Codex OAuth authorization is required before saving this profile.".to_string(),
        );
    }
    storage::save_codex_oauth_profile(&profile.id, &content)?;
    Ok(())
}

fn clone_codex_oauth_profile_if_needed(
    source: &ProfileDraft,
    target: &ProfileDraft,
) -> Result<(), String> {
    if !is_custom_codex_oauth_profile(source) || !is_custom_codex_oauth_profile(target) {
        return Ok(());
    }
    storage::copy_codex_oauth_profile(&source.id, &target.id)?;
    Ok(())
}

fn delete_codex_oauth_profile_cache_if_needed(profile: &ProfileDraft) -> Result<(), String> {
    if !is_custom_codex_oauth_profile(profile) {
        return Ok(());
    }
    storage::delete_codex_oauth_profile(&profile.id)
}

fn load_codex_oauth_profile_content(profile: &ProfileDraft) -> Result<String, String> {
    storage::load_codex_oauth_profile(&profile.id)?.ok_or_else(|| {
        format!(
            "Stored Codex OAuth profile could not be found for '{}'.",
            profile.name
        )
    })
}

fn build_codex_auth_json_write_plan(
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
    mode: &ProviderApplyMode,
) -> Result<Option<NativeConfigWritePlan>, String> {
    if canonical_profile_app(&profile.app) != "codex" {
        return Ok(None);
    }

    let path = native::codex::auth_json_path(paths);
    let current = if path.exists() {
        fs::read_to_string(&path).map_err(|err| {
            format!(
                "Codex auth.json could not be read at {}: {err}",
                display_path(&path)
            )
        })?
    } else {
        String::new()
    };
    let content = match mode {
        ProviderApplyMode::Config if is_custom_codex_oauth_profile(profile) => {
            Some(load_codex_oauth_profile_content(profile)?)
        }
        ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
            native::codex::official_auth_json_content(&current)?
        }
        ProviderApplyMode::Config => Some(native::codex::auth_json_content_with_api_key(
            &current,
            &load_provider_api_key_for_direct_config(profile)?,
        )?),
        ProviderApplyMode::Gateway => Some(native::codex::auth_json_content_with_api_key(
            &current,
            &gateway::client_config_for_tool(&profile.app)?.token,
        )?),
    };

    Ok(content.map(|content| {
        NativeConfigWritePlan::write(path, content, NativeConfigWriteKind::CodexAuthJson)
    }))
}

fn codex_auth_status_from_file(auth_path: &Path, configured_store: &str) -> CodexAuthStatus {
    let storage = codex_credentials_store_from_str(Some(configured_store))
        .unwrap_or(CodexAuthStorage::AuthJson);
    let path = Some(display_path(auth_path));

    let Ok(content) = fs::read_to_string(auth_path) else {
        return CodexAuthStatus {
            available: true,
            method: CodexAuthMethod::Unknown,
            storage,
            path,
            detail: "Codex auth.json exists, but CodeStudio Lite could not read it.".to_string(),
        };
    };

    codex_auth_status_from_file_content(auth_path, configured_store, &content)
}

fn codex_auth_status_from_file_content(
    auth_path: &Path,
    configured_store: &str,
    content: &str,
) -> CodexAuthStatus {
    let storage = codex_credentials_store_from_str(Some(configured_store))
        .unwrap_or(CodexAuthStorage::AuthJson);
    let path = Some(display_path(auth_path));

    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return CodexAuthStatus {
            available: true,
            method: CodexAuthMethod::Unknown,
            storage,
            path,
            detail: "Codex auth.json exists, but its shape could not be parsed.".to_string(),
        };
    };

    let method = infer_codex_auth_method(&value);
    let detail = match method {
        CodexAuthMethod::ChatGpt => {
            "Codex ChatGPT/OAuth login cache detected in auth.json.".to_string()
        }
        CodexAuthMethod::ApiKey => "Codex API-key login cache detected in auth.json.".to_string(),
        CodexAuthMethod::AccessToken => {
            "Codex access-token login cache detected in auth.json.".to_string()
        }
        CodexAuthMethod::Unknown => {
            "Codex auth.json exists; credential type could not be identified without reading secret values."
                .to_string()
        }
        CodexAuthMethod::None => "Codex auth.json exists but no credential markers were found."
            .to_string(),
    };

    CodexAuthStatus {
        available: !matches!(method, CodexAuthMethod::None),
        method,
        storage,
        path,
        detail,
    }
}

fn read_codex_credentials_store(config_path: &Path) -> Option<String> {
    let content = fs::read_to_string(config_path).ok()?;
    let value = toml::from_str::<toml::Value>(&content).ok()?;
    read_toml_string(&value, "cli_auth_credentials_store")
}

fn codex_credentials_store_from_str(value: Option<&str>) -> Option<CodexAuthStorage> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some("file") => Some(CodexAuthStorage::AuthJson),
        Some("keyring") => Some(CodexAuthStorage::Keyring),
        Some("auto") => Some(CodexAuthStorage::Auto),
        Some(_) => Some(CodexAuthStorage::Unknown),
        None => None,
    }
}

fn infer_codex_auth_method(value: &serde_json::Value) -> CodexAuthMethod {
    let auth_mode = value
        .get("auth_mode")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if auth_mode.eq_ignore_ascii_case("apikey") || auth_mode.eq_ignore_ascii_case("api_key") {
        return CodexAuthMethod::ApiKey;
    }
    if auth_mode.eq_ignore_ascii_case("chatgpt") {
        return CodexAuthMethod::ChatGpt;
    }
    if auth_mode.eq_ignore_ascii_case("access_token") {
        return CodexAuthMethod::AccessToken;
    }

    if native::codex::auth_json_has_chatgpt_markers(value) {
        return CodexAuthMethod::ChatGpt;
    }

    let mut keys = Vec::new();
    collect_json_key_paths(value, String::new(), &mut keys);

    if keys.iter().any(|key| key.contains("access_token")) {
        return CodexAuthMethod::AccessToken;
    }

    if keys
        .iter()
        .any(|key| key.contains("api_key") || key.contains("apikey"))
    {
        return CodexAuthMethod::ApiKey;
    }

    if keys.is_empty() {
        CodexAuthMethod::None
    } else {
        CodexAuthMethod::Unknown
    }
}

fn collect_json_key_paths(value: &serde_json::Value, prefix: String, keys: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let lowered = key.to_ascii_lowercase();
                let next = if prefix.is_empty() {
                    lowered
                } else {
                    format!("{prefix}.{lowered}")
                };
                keys.push(next.clone());
                collect_json_key_paths(child, next, keys);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_json_key_paths(child, prefix.clone(), keys);
            }
        }
        _ => {}
    }
}

fn normalize_theme(value: &str) -> Result<String, String> {
    match value.trim() {
        "system" | "light" | "dark" => Ok(value.trim().to_string()),
        _ => Err("Theme must be system, light, or dark.".to_string()),
    }
}

fn normalize_language(value: &str) -> Result<String, String> {
    match value.trim() {
        "zh-CN" | "zh-TW" | "en-US" => Ok(value.trim().to_string()),
        _ => Err("Language must be zh-CN, zh-TW, or en-US.".to_string()),
    }
}

fn read_toml_string(value: &toml::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(ToString::to_string)
}

fn parse_toml_or_empty(current: &str, label: &str) -> Result<toml::Value, String> {
    if current.trim().is_empty() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    toml::from_str::<toml::Value>(current)
        .map_err(|err| format!("Existing {label} could not be parsed: {err}"))
}

fn format_config_state(state: &ConfigState) -> &'static str {
    match state {
        ConfigState::Configured => "Configured",
        ConfigState::Unconfigured => "Not configured",
        ConfigState::NotApplicable => "Not applicable",
        ConfigState::Unknown => "Unknown",
    }
}

fn aggregate_check_status(checks: &[ProfileConnectionCheck]) -> Severity {
    if checks
        .iter()
        .any(|check| matches!(check.status, Severity::Error))
    {
        Severity::Error
    } else if checks
        .iter()
        .any(|check| matches!(check.status, Severity::Warning))
    {
        Severity::Warning
    } else {
        Severity::Ok
    }
}

fn profile_sql_preview_content(
    profile: &ProfileDraft,
    secret_status: &str,
    last_test_status: &str,
) -> Result<String, String> {
    serde_json::to_string_pretty(&serde_json::json!({
        "table": "profiles",
        "row": {
            "id": profile.id,
            "name": profile.name,
            "icon": profile_icon_preview(profile.icon.as_deref()),
            "remark": profile.remark,
            "app": profile.app,
            "mode": provider_apply_mode_value(&profile.mode),
            "provider": profile.provider,
            "protocol": profile.protocol,
            "model": profile.model,
            "review_model": profile.review_model,
            "model_mappings": profile.model_mappings,
            "base_url": profile.base_url,
            "auth_ref": profile.auth_ref,
            "created_at": profile.created_at,
            "updated_at": profile.updated_at,
            "last_test_status": last_test_status,
            "secret_status": secret_status,
        },
        "secrets": "API keys are stored in the system keychain and never written into SQLite."
    }))
    .map_err(|err| err.to_string())
}

fn profile_icon_preview(icon: Option<&str>) -> Option<String> {
    icon.map(|value| {
        if value.starts_with("data:image/") {
            format!("image data url ({} bytes)", value.len())
        } else {
            value.to_string()
        }
    })
}

fn normalize_profile_remark(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_profile_review_model(app: &str, value: Option<&str>) -> Option<String> {
    if !is_codex_family_app(app) {
        return None;
    }
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn effective_profile_review_model(
    app: &str,
    value: Option<&str>,
    primary_model: &str,
) -> Option<String> {
    normalize_profile_review_model(app, value).or_else(|| {
        if !is_codex_family_app(app) {
            return None;
        }
        let primary_model = primary_model.trim();
        (!primary_model.is_empty()).then(|| primary_model.to_string())
    })
}

fn normalize_profile_model_mappings(
    app: &str,
    mappings: Option<&[ProfileModelMapping]>,
) -> Result<Vec<ProfileModelMapping>, String> {
    if canonical_profile_app(app) != "claude" {
        return Ok(Vec::new());
    }

    let Some(mappings) = mappings else {
        return Ok(Vec::new());
    };
    let mut normalized = Vec::new();
    let mut aliases = HashSet::new();

    for mapping in mappings {
        let alias = mapping.alias.trim();
        let model = mapping.model.trim();
        let description = mapping
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if alias.is_empty() && model.is_empty() && description.is_none() {
            continue;
        }
        if alias.is_empty() || model.is_empty() {
            return Err(
                "Claude Code model mappings require both alias and target model.".to_string(),
            );
        }
        let alias_key = alias.to_ascii_lowercase();
        if !aliases.insert(alias_key) {
            return Err(format!(
                "Claude Code model mapping alias '{alias}' is duplicated."
            ));
        }

        normalized.push(ProfileModelMapping {
            alias: alias.to_string(),
            model: model.to_string(),
            supports_1m: mapping.supports_1m,
            description,
        });
    }

    Ok(normalized)
}

fn build_profile_write_plan(
    name: &str,
    app: &str,
    mode: Option<&ProviderApplyMode>,
    provider: &str,
    protocol: Option<&str>,
    model: &str,
    base_url: &str,
    secret_provided: bool,
) -> Result<ProfileWritePlan, String> {
    let name = normalize_required("Profile Name", name)?;
    let app = canonical_profile_app(&normalize_token("Client", app)?);
    let provider = normalize_provider_token(provider)?;
    let mode = normalize_profile_mode(&provider, mode)?;
    ensure_custom_official_profile_allowed(&app, &provider, mode)?;
    let protocol = normalize_protocol(protocol)?;
    ensure_profile_protocol_supported_for_mode(&app, mode, &provider, &protocol)?;
    let model = model.trim().to_string();
    let base_url = validate_base_url_for_provider(&provider, base_url)?;
    if provider_requires_api_key(&provider) && !secret_provided {
        return Err("Provider API key is required for non-official providers.".to_string());
    }
    let id = unique_profile_id(&slugify(&name))?;
    let secret_status = if provider_is_official(&provider) {
        "oauth"
    } else if secret_provided {
        "pending_keychain"
    } else {
        "missing"
    };

    let auth_ref = if secret_provided {
        Some(format!("keychain:codestudio-lite/{id}/api_key"))
    } else {
        None
    };

    Ok(ProfileWritePlan {
        id,
        name,
        app,
        mode,
        provider,
        protocol,
        model,
        base_url,
        secret_status,
        auth_ref,
    })
}

fn ensure_profile_tool_installed(app: &str) -> Result<(), String> {
    let app = canonical_profile_app(app);
    let installed_tool_ids = installed_profile_tool_ids()?;
    if installed_tool_ids.contains(&app) {
        Ok(())
    } else {
        Err(profile_tool_not_installed_error(&app))
    }
}

fn installed_profile_tool_ids() -> Result<HashSet<String>, String> {
    // This is a fast "is the target tool installed?" guard used by profile
    // create/duplicate. Prefer the on-disk detection cache so it does not block
    // on a full live environment scan; fall back to a live detect only when no
    // cache exists (first run / cache cleared).
    let snapshot = storage::load_detection_cache()
        .ok()
        .flatten()
        .or_else(|| detector::detect_environment().ok());
    Ok(snapshot
        .map(|snapshot| {
            snapshot
                .tools
                .into_iter()
                .filter(|tool| tool.install_state == InstallState::Installed)
                .map(|tool| canonical_profile_app(&tool.id))
                .collect()
        })
        .unwrap_or_default())
}

fn profile_tool_not_installed_error(app: &str) -> String {
    format!(
        "Tool '{}' is not installed, so a profile cannot be created for it.",
        canonical_profile_app(app)
    )
}

fn provider_apply_mode_value(mode: &ProviderApplyMode) -> &'static str {
    match mode {
        ProviderApplyMode::Config => "config",
        ProviderApplyMode::Gateway => "gateway",
    }
}

fn native_config_path_for_profile(
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
) -> Result<PathBuf, String> {
    let app = canonical_profile_app(&profile.app);
    if let Some(adapter) = native::adapter(&app) {
        return Ok(adapter.target(paths));
    }
    match app.as_str() {
        "codex" => Ok(paths.home_dir.join(".codex").join("config.toml")),
        "claude-desktop" => Ok(claude_desktop_paths(paths)?.profile_path),
        _ => Err(format!(
            "Native writes are not implemented for tool '{}'.",
            profile.app
        )),
    }
}

fn native_config_path_for_profile_mode(
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
) -> Result<Option<PathBuf>, String> {
    let app = canonical_profile_app(&profile.app);
    if let Some(adapter) = native::adapter(&app) {
        return if adapter.supports_mode(mode) {
            Ok(Some(adapter.target(paths)))
        } else {
            Ok(None)
        };
    }
    if mode == ProviderApplyMode::Gateway {
        return match canonical_profile_app(&profile.app).as_str() {
            "codex" | "claude-desktop" | "claude" | "gemini" => {
                native_config_path_for_profile(profile, paths).map(Some)
            }
            _ => Ok(None),
        };
    }

    if canonical_profile_app(&profile.app) == "claude-desktop" {
        return native_config_path_for_profile(profile, paths).map(Some);
    }

    if provider_is_official(&profile.provider) && !is_codex_family_app(&profile.app) {
        return match canonical_profile_app(&profile.app).as_str() {
            "claude" | "gemini" | "gemini-code-assist" => {
                native_config_path_for_profile(profile, paths).map(Some)
            }
            _ => Ok(None),
        };
    }

    native_config_path_for_profile(profile, paths).map(Some)
}

fn claude_vscode_plugin_config_path(paths: &crate::core::app_paths::AppPaths) -> PathBuf {
    paths.home_dir.join(".claude").join("config.json")
}

fn vs_code_user_settings_path(paths: &crate::core::app_paths::AppPaths) -> PathBuf {
    if cfg!(target_os = "windows") {
        return env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.home_dir.join("AppData").join("Roaming"))
            .join("Code")
            .join("User")
            .join("settings.json");
    }

    if cfg!(target_os = "macos") {
        return paths
            .home_dir
            .join("Library")
            .join("Application Support")
            .join("Code")
            .join("User")
            .join("settings.json");
    }

    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.home_dir.join(".config"))
        .join("Code")
        .join("User")
        .join("settings.json")
}

fn build_native_apply_plan(
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
    mode: &ProviderApplyMode,
    sync_claude_vs_code: bool,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    if provider_is_official(&profile.provider) && *mode == ProviderApplyMode::Gateway {
        return Err(
            "Official provider uses the client login directly and does not run through the local gateway."
                .to_string(),
        );
    }

    if canonical_profile_app(&profile.app) == "claude-desktop" {
        return build_claude_desktop_apply_plan(profile, paths, mode);
    }

    let Some(path) = native_config_path_for_profile_mode(profile, paths, *mode)? else {
        return Ok(Vec::new());
    };
    let current = if path.exists() {
        fs::read_to_string(&path).map_err(|err| err.to_string())?
    } else {
        String::new()
    };
    let app = canonical_profile_app(&profile.app);
    let adapter_content = native::adapter(&app)
        .map(|adapter| adapter.render(&current, profile, *mode))
        .transpose()?;
    let content = if let Some(content) = adapter_content {
        content
    } else {
        match mode {
            ProviderApplyMode::Config => match canonical_profile_app(&profile.app).as_str() {
                "codex" => native::codex::codex_direct_config_content(&current, profile)?,
                _ => {
                    return Err(format!(
                        "Config profile adapter is not implemented for tool '{}'.",
                        profile.app
                    ))
                }
            },
            ProviderApplyMode::Gateway => match canonical_profile_app(&profile.app).as_str() {
                "codex" => native::codex::codex_gateway_config_content(&current, profile)?,
                _ => {
                    return Err(format!(
                        "Gateway profile adapter is not implemented for tool '{}'.",
                        profile.app
                    ))
                }
            },
        }
    };

    let config_plan = NativeConfigWritePlan::write(
        path,
        content,
        match canonical_profile_app(&profile.app).as_str() {
            "gemini-code-assist" => NativeConfigWriteKind::GeminiCodeAssistSettings,
            _ => NativeConfigWriteKind::ProfileConfig,
        },
    );
    let mut plans = Vec::new();

    if let Some(auth_plan) = build_codex_auth_json_write_plan(profile, paths, mode)? {
        plans.push(auth_plan);
    }
    plans.push(config_plan);

    if *mode == ProviderApplyMode::Config
        && canonical_profile_app(&profile.app) == "claude"
        && sync_claude_vs_code
    {
        let plugin_path = claude_vscode_plugin_config_path(paths);
        let plugin_current = if plugin_path.exists() {
            fs::read_to_string(&plugin_path).map_err(|err| err.to_string())?
        } else {
            String::new()
        };
        plans.push(NativeConfigWritePlan::write(
            plugin_path,
            claude_vscode_plugin_config_content(&plugin_current)?,
            NativeConfigWriteKind::ClaudeVsCodePluginConfig,
        ));
    }

    Ok(plans)
}

pub(crate) fn ensure_claude_desktop_developer_mode() -> Result<usize, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    let desktop_paths = claude_desktop_paths(&paths)?;
    let plans = build_claude_desktop_developer_settings_plans(&desktop_paths)?;
    let mut written = 0usize;

    for plan in plans {
        apply_native_config_write_plan(&plan)?;
        let developer_settings = fs::read_to_string(&plan.path).map_err(|err| err.to_string())?;
        if !claude_desktop_developer_mode_enabled(&developer_settings)? {
            return Err(format!(
                "Claude Desktop developer settings verification failed at {}",
                display_path(&plan.path)
            ));
        }
        written += 1;
    }

    Ok(written)
}

fn read_file_if_exists(path: &Path) -> Result<String, String> {
    if path.exists() {
        fs::read_to_string(path).map_err(|err| err.to_string())
    } else {
        Ok(String::new())
    }
}

fn build_gateway_cleanup_plan(
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    if !provider_is_official(&profile.provider) {
        return Ok(Vec::new());
    }

    let app = canonical_profile_app(&profile.app);
    if native::adapter(&app).is_none() {
        return Ok(Vec::new());
    }

    let path = native_config_path_for_profile(profile, paths)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let current = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let content = if let Some(adapter) = native::adapter(&app) {
        adapter.cleanup_gateway(&current)?
    } else {
        match app.as_str() {
            _ => unreachable!(),
        }
    };

    if content == current {
        return Ok(Vec::new());
    }

    Ok(vec![NativeConfigWritePlan::write(
        path,
        content,
        NativeConfigWriteKind::ProfileConfig,
    )])
}

fn claude_vscode_plugin_config_content(current: &str) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude VS Code plugin config")?;
    set_json_string_path(
        &mut value,
        &["primaryApiKey"],
        CLAUDE_VSCODE_PLUGIN_PRIMARY_API_KEY,
    );
    render_json_config(value, "Claude VS Code plugin config")
}

fn verify_claude_vscode_plugin_config(path: &Path) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Claude VS Code plugin config")?;
    Ok(claude_vscode_plugin_config_matches(&value))
}

fn verify_native_config(
    path: &Path,
    profile: &ProfileDraft,
    mode: &ProviderApplyMode,
) -> Result<bool, String> {
    let app = canonical_profile_app(&profile.app);
    if let Some(adapter) = native::adapter(&app) {
        return adapter.verify(path, profile, *mode);
    }
    match (mode, app.as_str()) {
        (mode, "codex") => native::codex::verify_config(path, profile, *mode),
        (_, _) => Err(format!(
            "Native config verification is not implemented for tool '{}'.",
            profile.app
        )),
    }
}

fn verify_native_config_write(
    plan: &NativeConfigWritePlan,
    profile: &ProfileDraft,
    mode: &ProviderApplyMode,
) -> Result<bool, String> {
    if plan.delete {
        return Ok(!plan.path.exists());
    }

    match plan.kind {
        NativeConfigWriteKind::ProfileConfig => verify_native_config(&plan.path, profile, mode),
        NativeConfigWriteKind::CodexAuthJson => {
            native::codex::verify_auth_json_write(&plan.path, &plan.content)
        }
        NativeConfigWriteKind::ClaudeVsCodePluginConfig => {
            verify_claude_vscode_plugin_config(&plan.path)
        }
        NativeConfigWriteKind::GeminiCodeAssistSettings => native::adapter("gemini-code-assist")
            .ok_or_else(|| "Gemini Code Assist adapter is unavailable.".to_string())?
            .verify(&plan.path, profile, *mode),
        NativeConfigWriteKind::ClaudeDesktopDeploymentConfig
        | NativeConfigWriteKind::ClaudeDesktopProfileConfig
        | NativeConfigWriteKind::ClaudeDesktopMetaConfig
        | NativeConfigWriteKind::ClaudeDesktopDeveloperSettings => {
            native::claude_desktop::verify_write(plan.kind, &plan.path, profile, *mode)
        }
    }
}

fn custom_provider_id_for_profile(_profile: &ProfileDraft) -> String {
    "custom".to_string()
}

fn gateway_config_model_for_profile(profile: &ProfileDraft) -> &str {
    if canonical_profile_app(&profile.app) == "claude" {
        if let Some(alias) = profile
            .model_mappings
            .iter()
            .map(|mapping| mapping.alias.trim())
            .find(|alias| !alias.is_empty())
        {
            return alias;
        }
    }
    let model = profile.model.trim();
    if model.is_empty() {
        GATEWAY_FALLBACK_MODEL
    } else {
        model
    }
}

fn load_provider_api_key_for_direct_config(profile: &ProfileDraft) -> Result<String, String> {
    let Some(auth_ref) = profile.auth_ref.as_deref() else {
        if provider_requires_api_key(&profile.provider) {
            return Err(
                "Config profiles need a stored Provider API key. Edit this profile and save an API key first."
                    .to_string(),
            );
        }
        return Ok("codestudio-local-no-auth".to_string());
    };
    let api_key = credentials::load_keychain_secret(auth_ref)?;
    if api_key.trim().is_empty() {
        return Err("Stored Provider API key is empty.".to_string());
    }
    Ok(api_key)
}

fn redact_native_config_preview_content(
    content: &str,
    profile: &ProfileDraft,
    mode: ProviderApplyMode,
) -> String {
    let mut output = content.to_string();
    if mode == ProviderApplyMode::Gateway {
        if let Ok(client) = gateway::client_config_for_tool(&profile.app) {
            output = replace_nonempty(&output, &client.token, &client.token_preview);
        }
    }
    redact_oauth_like_tokens(&output)
}

fn replace_nonempty(content: &str, needle: &str, replacement: &str) -> String {
    let trimmed = needle.trim();
    if trimmed.is_empty() {
        content.to_string()
    } else {
        content.replace(trimmed, replacement)
    }
}

fn redact_oauth_like_tokens(content: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(content) else {
        return content.to_string();
    };
    redact_sensitive_json_value(&mut value);
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| content.to_string())
}

fn redact_sensitive_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if json_key_looks_sensitive(key) {
                    if let Some(text) = item.as_str() {
                        if !is_safe_secret_preview_value(text) {
                            *item = serde_json::Value::String("<redacted>".to_string());
                        }
                    } else {
                        redact_sensitive_json_value(item);
                    }
                } else {
                    redact_sensitive_json_value(item);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_sensitive_json_value(item);
            }
        }
        _ => {}
    }
}

fn json_key_looks_sensitive(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    lowered.contains("token")
        || lowered.contains("secret")
        || lowered.contains("api_key")
        || lowered.contains("apikey")
        || lowered == "password"
}

fn is_safe_secret_preview_value(value: &str) -> bool {
    matches!(
        value.trim(),
        "keychain:****" | "(no api key required)" | "(missing keychain secret)"
    )
}

fn parse_json5_or_empty(current: &str, label: &str) -> Result<serde_json::Value, String> {
    if current.trim().is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    json5::from_str::<serde_json::Value>(current)
        .map_err(|err| format!("Existing {label} could not be parsed: {err}"))
}

fn render_json_config(value: serde_json::Value, label: &str) -> Result<String, String> {
    let rendered = serde_json::to_string_pretty(&value)
        .map_err(|err| format!("Generated {label} could not be serialized: {err}"))?;
    json5::from_str::<serde_json::Value>(&rendered)
        .map_err(|err| format!("Generated {label} is invalid: {err}"))?;
    Ok(format!("{rendered}\n"))
}

fn set_json_string_path(root: &mut serde_json::Value, path: &[&str], value: &str) {
    set_json_value_path(
        root,
        path,
        serde_json::Value::String(value.trim().to_string()),
    );
}

fn set_json_value_path(root: &mut serde_json::Value, path: &[&str], value: serde_json::Value) {
    if path.is_empty() {
        *root = value;
        return;
    }

    let mut current = root;
    for segment in &path[..path.len() - 1] {
        if !current.is_object() {
            *current = serde_json::Value::Object(serde_json::Map::new());
        }
        let object = current.as_object_mut().expect("object was just ensured");
        current = object
            .entry((*segment).to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }

    if !current.is_object() {
        *current = serde_json::Value::Object(serde_json::Map::new());
    }
    current
        .as_object_mut()
        .expect("object was just ensured")
        .insert(path[path.len() - 1].to_string(), value);
}

fn remove_json_path(root: &mut serde_json::Value, path: &[&str]) {
    if path.is_empty() {
        return;
    }

    let mut current = root;
    for segment in &path[..path.len() - 1] {
        let Some(next) = current.get_mut(*segment) else {
            return;
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut() {
        object.remove(path[path.len() - 1]);
    }
}

fn remove_json_string_path_if(root: &mut serde_json::Value, path: &[&str], expected: &str) {
    if json_string_lookup(root, path).as_deref() == Some(expected) {
        remove_json_path(root, path);
    }
}

fn json_string_lookup(root: &serde_json::Value, path: &[&str]) -> Option<String> {
    json_lookup(root, path).and_then(|value| value.as_str().map(ToString::to_string))
}

fn json_object_keys(root: &serde_json::Value, path: &[&str]) -> Vec<String> {
    json_lookup(root, path)
        .and_then(serde_json::Value::as_object)
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn json_lookup<'a>(root: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut current = root;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn redacted_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) if looks_sensitive(text) => "<redacted>".to_string(),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Array(values) => format!("array[{}]", values.len()),
        serde_json::Value::Object(values) => format!("object[{}]", values.len()),
        serde_json::Value::Null => "null".to_string(),
    }
}

fn parse_yaml_or_empty(content: &str, label: &str) -> Result<serde_norway::Value, String> {
    if content.trim().is_empty() {
        return Ok(serde_norway::Value::Mapping(serde_norway::Mapping::new()));
    }
    serde_norway::from_str(content)
        .map_err(|err| format!("Existing {label} could not be parsed: {err}"))
}

fn render_yaml_config(value: serde_norway::Value, label: &str) -> Result<String, String> {
    let rendered = serde_norway::to_string(&value)
        .map_err(|err| format!("Generated {label} could not be serialized: {err}"))?;
    serde_norway::from_str::<serde_norway::Value>(&rendered)
        .map_err(|err| format!("Generated {label} is invalid: {err}"))?;
    Ok(rendered)
}

fn set_yaml_string_path(root: &mut serde_norway::Value, path: &[&str], value: &str) {
    set_yaml_value_path(
        root,
        path,
        serde_norway::Value::String(value.trim().to_string()),
    );
}

fn set_yaml_value_path(root: &mut serde_norway::Value, path: &[&str], value: serde_norway::Value) {
    if path.is_empty() {
        *root = value;
        return;
    }

    if !root.is_mapping() {
        *root = serde_norway::Value::Mapping(serde_norway::Mapping::new());
    }
    let mapping = root.as_mapping_mut().expect("mapping was just ensured");
    let key = serde_norway::Value::String(path[0].to_string());
    if path.len() == 1 {
        mapping.insert(key, value);
        return;
    }
    if !mapping.contains_key(&key) {
        mapping.insert(
            key.clone(),
            serde_norway::Value::Mapping(serde_norway::Mapping::new()),
        );
    }
    let next = mapping.get_mut(&key).expect("key was just inserted");
    set_yaml_value_path(next, &path[1..], value);
}

fn remove_yaml_path(root: &mut serde_norway::Value, path: &[&str]) {
    if path.is_empty() {
        return;
    }
    let Some(mapping) = root.as_mapping_mut() else {
        return;
    };
    let key = serde_norway::Value::String(path[0].to_string());
    if path.len() == 1 {
        mapping.remove(&key);
        return;
    }
    if let Some(next) = mapping.get_mut(&key) {
        remove_yaml_path(next, &path[1..]);
    }
}

fn remove_yaml_string_path_if(root: &mut serde_norway::Value, path: &[&str], expected: &str) {
    if yaml_string_lookup(root, path).as_deref() == Some(expected) {
        remove_yaml_path(root, path);
    }
}

fn yaml_string_lookup(root: &serde_norway::Value, path: &[&str]) -> Option<String> {
    yaml_lookup(root, path).and_then(|value| value.as_str().map(ToString::to_string))
}

fn yaml_lookup<'a>(
    root: &'a serde_norway::Value,
    path: &[&str],
) -> Option<&'a serde_norway::Value> {
    let mut current = root;
    for segment in path {
        let key = serde_norway::Value::String((*segment).to_string());
        current = current.as_mapping()?.get(&key)?;
    }
    Some(current)
}

fn redacted_yaml_value(value: &serde_norway::Value) -> String {
    match value {
        serde_norway::Value::String(text) if looks_sensitive(text) => "<redacted>".to_string(),
        serde_norway::Value::String(text) => text.clone(),
        serde_norway::Value::Bool(value) => value.to_string(),
        serde_norway::Value::Number(value) => value.to_string(),
        serde_norway::Value::Sequence(values) => format!("array[{}]", values.len()),
        serde_norway::Value::Mapping(values) => format!("object[{}]", values.len()),
        serde_norway::Value::Null => "null".to_string(),
        serde_norway::Value::Tagged(_) => "tagged".to_string(),
    }
}

fn update_env_content(current: &str, updates: &[(&str, Option<String>)]) -> String {
    let mut seen = HashSet::new();
    let mut output = Vec::new();

    for line in current.lines() {
        if let Some(key) = parse_env_assignment_key(line) {
            if let Some((_, value)) = updates.iter().find(|(candidate, _)| *candidate == key) {
                seen.insert(key.clone());
                if let Some(value) = value {
                    output.push(format!("{}={}", key, quote_env_value(value)));
                }
                continue;
            }
        }
        output.push(line.to_string());
    }

    for (key, value) in updates {
        if seen.contains(*key) {
            continue;
        }
        if let Some(value) = value {
            output.push(format!("{}={}", key, quote_env_value(value)));
        }
    }

    if output.is_empty() {
        String::new()
    } else {
        format!("{}\n", output.join("\n"))
    }
}

fn parse_env_assignment_key(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, _) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    Some(key.to_string())
}

fn quote_env_value(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

fn parse_env_content(content: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        values.insert(key.to_string(), unquote_env_value(value.trim()));
    }
    values
}

fn unquote_env_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut output = String::new();
        let mut chars = inner.chars();
        while let Some(character) = chars.next() {
            if character != '\\' {
                output.push(character);
                continue;
            }
            match chars.next() {
                Some('n') => output.push('\n'),
                Some('r') => output.push('\r'),
                Some('"') => output.push('"'),
                Some('\\') => output.push('\\'),
                Some(other) => output.push(other),
                None => output.push('\\'),
            }
        }
        output
    } else if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn build_native_config_preview(
    profile: &ProfileDraft,
    native_config_path: Option<&str>,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
) -> Result<Option<NativeConfigPreview>, String> {
    if mode == ProviderApplyMode::Config && !config_file_protocol_supported(profile) {
        return Ok(None);
    }

    let app = canonical_profile_app(&profile.app);
    if let Some(adapter) = native::adapter(&app) {
        let path_buf = adapter.target(paths);
        let path = native_config_path
            .map(ToString::to_string)
            .unwrap_or_else(|| display_path(&path_buf));
        return adapter.preview(profile, path_buf, path, mode).map(Some);
    }

    build_non_codex_native_config_preview(profile, native_config_path, paths, mode)
}

fn build_non_codex_native_config_preview(
    profile: &ProfileDraft,
    native_config_path: Option<&str>,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
) -> Result<Option<NativeConfigPreview>, String> {
    let app = canonical_profile_app(&profile.app);
    if let Some(adapter) = native::adapter(&app) {
        let path_buf = adapter.target(paths);
        let path = native_config_path
            .map(ToString::to_string)
            .unwrap_or_else(|| display_path(&path_buf));
        return adapter.preview(profile, path_buf, path, mode).map(Some);
    }
    if canonical_profile_app(&profile.app) == "claude-desktop" {
        return native::claude_desktop::preview(profile, native_config_path, paths, mode);
    }

    Ok(None)
}

fn read_json_preview(
    path: &Path,
    label: &str,
    warnings: &mut Vec<String>,
) -> Result<(serde_json::Value, String), String> {
    if !path.exists() {
        warnings.push(format!(
            "{label} does not exist yet; adapter would create it after confirmation."
        ));
        return Ok((
            serde_json::Value::Object(serde_json::Map::new()),
            "missing".to_string(),
        ));
    }

    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    match parse_json5_or_empty(&content, label) {
        Ok(value) => Ok((value, "parsed".to_string())),
        Err(err) => {
            warnings.push(format!(
                "Existing {label} could not be parsed, so only create-style preview is available: {err}"
            ));
            Ok((
                serde_json::Value::Object(serde_json::Map::new()),
                "parse_error".to_string(),
            ))
        }
    }
}

fn read_env_preview(
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<(HashMap<String, String>, String), String> {
    if !path.exists() {
        warnings.push(
            "Gemini environment file does not exist yet; adapter would create it after confirmation."
                .to_string(),
        );
        return Ok((HashMap::new(), "missing".to_string()));
    }

    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    Ok((parse_env_content(&content), "parsed".to_string()))
}

fn read_yaml_preview(
    path: &Path,
    label: &str,
    warnings: &mut Vec<String>,
) -> Result<(serde_norway::Value, String), String> {
    if !path.exists() {
        warnings.push(format!(
            "{label} does not exist yet; adapter would create it after confirmation."
        ));
        return Ok((
            serde_norway::Value::Mapping(serde_norway::Mapping::new()),
            "missing".to_string(),
        ));
    }

    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    match parse_yaml_or_empty(&content, label) {
        Ok(value) => Ok((value, "parsed".to_string())),
        Err(err) => {
            warnings.push(format!(
                "Existing {label} could not be parsed, so only create-style preview is available: {err}"
            ));
            Ok((
                serde_norway::Value::Mapping(serde_norway::Mapping::new()),
                "parse_error".to_string(),
            ))
        }
    }
}

fn read_toml_preview(
    path: &Path,
    label: &str,
    warnings: &mut Vec<String>,
) -> Result<(toml::Value, String), String> {
    if !path.exists() {
        warnings.push(format!(
            "{label} does not exist yet; adapter would create it after confirmation."
        ));
        return Ok((
            toml::Value::Table(toml::map::Map::new()),
            "missing".to_string(),
        ));
    }

    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    match parse_toml_or_empty(&content, label) {
        Ok(value) => Ok((value, "parsed".to_string())),
        Err(err) => {
            warnings.push(format!(
                "Existing {label} could not be parsed, so only create-style preview is available: {err}"
            ));
            Ok((
                toml::Value::Table(toml::map::Map::new()),
                "parse_error".to_string(),
            ))
        }
    }
}

fn json_diff_line(
    root: &serde_json::Value,
    path: &[&str],
    after: &str,
    detail: &str,
) -> NativeConfigDiffLine {
    diff_value_line(
        path.join("."),
        json_lookup(root, path).map(redacted_json_value),
        Some(after.to_string()),
        detail,
    )
}

fn json_diff_remove_line(
    root: &serde_json::Value,
    path: &[&str],
    detail: &str,
) -> NativeConfigDiffLine {
    diff_value_line(
        path.join("."),
        json_lookup(root, path).map(redacted_json_value),
        None,
        detail,
    )
}

fn env_diff_line(
    env: &HashMap<String, String>,
    key: &str,
    after: &str,
    detail: &str,
) -> NativeConfigDiffLine {
    diff_value_line(
        key.to_string(),
        env.get(key).map(|value| {
            if looks_sensitive(value) {
                "<redacted>".to_string()
            } else {
                value.clone()
            }
        }),
        Some(after.to_string()),
        detail,
    )
}

fn env_diff_remove_line(
    env: &HashMap<String, String>,
    key: &str,
    detail: &str,
) -> NativeConfigDiffLine {
    diff_value_line(
        key.to_string(),
        env.get(key).map(|value| {
            if looks_sensitive(value) {
                "<redacted>".to_string()
            } else {
                value.clone()
            }
        }),
        None,
        detail,
    )
}

fn yaml_diff_line(
    root: &serde_norway::Value,
    path: &[&str],
    after: &str,
    detail: &str,
) -> NativeConfigDiffLine {
    diff_value_line(
        path.join("."),
        yaml_lookup(root, path).map(redacted_yaml_value),
        Some(after.to_string()),
        detail,
    )
}

fn yaml_diff_remove_line(
    root: &serde_norway::Value,
    path: &[&str],
    detail: &str,
) -> NativeConfigDiffLine {
    diff_value_line(
        path.join("."),
        yaml_lookup(root, path).map(redacted_yaml_value),
        None,
        detail,
    )
}

fn diff_value_line(
    key: String,
    before: Option<String>,
    after: Option<String>,
    detail: &str,
) -> NativeConfigDiffLine {
    let action = match (before.as_deref(), after.as_deref()) {
        (None, Some(_)) => "add",
        (Some(current), Some(next)) if current == next => "unchanged",
        (Some(_), Some(_)) => "update",
        (Some(_), None) => "remove",
        (None, None) => "unchanged",
    };

    NativeConfigDiffLine {
        key,
        action: action.to_string(),
        before,
        after,
        detail: detail.to_string(),
    }
}

fn diff_line(
    root: &toml::Value,
    dotted_key: &str,
    after: &str,
    detail: &str,
) -> NativeConfigDiffLine {
    diff_value_line(
        dotted_key.to_string(),
        toml_lookup(root, dotted_key).map(redacted_toml_value),
        Some(after.to_string()),
        detail,
    )
}

fn diff_remove_line(root: &toml::Value, dotted_key: &str, detail: &str) -> NativeConfigDiffLine {
    diff_value_line(
        dotted_key.to_string(),
        toml_lookup(root, dotted_key).map(redacted_toml_value),
        None,
        detail,
    )
}

fn toml_lookup<'a>(root: &'a toml::Value, dotted_key: &str) -> Option<&'a toml::Value> {
    let mut current = root;
    for segment in dotted_key.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn redacted_toml_value(value: &toml::Value) -> String {
    match value {
        toml::Value::String(text) if looks_sensitive(text) => "<redacted>".to_string(),
        toml::Value::String(text) => text.clone(),
        toml::Value::Boolean(value) => value.to_string(),
        toml::Value::Integer(value) => value.to_string(),
        toml::Value::Float(value) => value.to_string(),
        toml::Value::Datetime(value) => value.to_string(),
        toml::Value::Array(values) => format!("array[{}]", values.len()),
        toml::Value::Table(values) => format!("table[{}]", values.len()),
    }
}

fn looks_sensitive(value: &str) -> bool {
    let lowered = value.to_lowercase();
    lowered.contains("token")
        || lowered.contains("secret")
        || lowered.contains("api_key")
        || lowered.contains("apikey")
        || lowered.contains("keychain:")
        || lowered.contains("codestudio-local-")
        || value.len() > 80
}

fn looks_like_local_gateway_token(value: &str) -> bool {
    value.trim().starts_with("codestudio-local-")
}

fn looks_like_local_gateway_url(value: &str) -> bool {
    let trimmed = value.trim().to_ascii_lowercase();
    trimmed.starts_with("http://127.0.0.1:")
        && (trimmed.contains("/tools/") || trimmed.ends_with("/v1"))
}

fn unique_profile_id(base_id: &str) -> Result<String, String> {
    let base_id = if base_id.is_empty() {
        "profile"
    } else {
        base_id
    };
    let existing_ids = load_profiles()?
        .into_iter()
        .map(|profile| profile.id)
        .collect::<HashSet<_>>();

    for index in 0..1000 {
        let candidate = if index == 0 {
            base_id.to_string()
        } else {
            format!("{base_id}-{index}")
        };
        if !is_builtin_profile_id(&candidate) && !existing_ids.contains(&candidate) {
            return Ok(candidate);
        }
    }

    Err("Could not create a unique profile id".to_string())
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in value.trim().to_lowercase().chars() {
        let next = if character.is_ascii_alphanumeric() {
            Some(character)
        } else if matches!(character, '-' | '_' | ' ') {
            Some('-')
        } else {
            None
        };

        if let Some(next) = next {
            if next == '-' {
                if previous_dash || slug.is_empty() {
                    continue;
                }
                previous_dash = true;
            } else {
                previous_dash = false;
            }
            slug.push(next);
        }
    }

    slug.trim_matches('-').to_string()
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod profile_tests;
