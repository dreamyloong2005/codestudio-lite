use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::backup;
use crate::core::codex_client;
use crate::core::credentials;
use crate::core::detector;
use crate::core::env_health;
use crate::core::gateway;
use crate::core::platform::{
    hidden_command, hidden_command_with_args, package, resolve_command, run_powershell,
};
use crate::core::storage;
use crate::core::tool_registry;
use crate::core::types::{
    ActiveProfilesByMode, AppSettings, ApplyProfileRequest, ApplyProfileResult, CodexAuthMethod,
    CodexAuthStatus, CodexAuthStorage, ConfigState, DeleteProfileDraftRequest,
    DuplicateProfileDraftRequest, InstallState, NativeConfigDiffLine, NativeConfigPreview,
    PreviewProfileApplyRequest, PreviewProfileApplyResult, PreviewProfileWriteRequest,
    PreviewProfileWriteResult, ProfileApplyPreviewItem, ProfileConnectionCheck, ProfileDraft,
    ProfileSummary, ProfileWritePreviewItem, ProviderApplyMode, ProviderApplyModePreview,
    ReorderProfileDraftsRequest, SaveProfileDraftRequest, Severity, StartCodexOAuthLoginResult,
    SwitchActiveProfileRequest, TestProfileConnectionRequest, TestProfileConnectionResult,
    UpdateAppSettingsRequest, UpdateProfileDraftRequest,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize, Deserialize)]
struct AppConfig {
    #[serde(default)]
    active_profiles_by_mode: ActiveProfilesByMode,
    ui: UiConfig,
    security: SecurityConfig,
}

#[derive(Debug, Clone)]
struct NativeConfigWritePlan {
    path: PathBuf,
    content: String,
    kind: NativeConfigWriteKind,
    delete: bool,
}

impl NativeConfigWritePlan {
    fn write(path: PathBuf, content: String, kind: NativeConfigWriteKind) -> Self {
        Self {
            path,
            content,
            kind,
            delete: false,
        }
    }

    fn delete(path: PathBuf, kind: NativeConfigWriteKind) -> Self {
        Self {
            path,
            content: String::new(),
            kind,
            delete: true,
        }
    }
}

#[derive(Debug, Clone)]
struct NativeConfigLifecyclePlan {
    profile: ProfileDraft,
    mode: ProviderApplyMode,
    plan: NativeConfigWritePlan,
    verify_after_write: bool,
}

#[derive(Debug, Clone, Copy)]
enum NativeConfigWriteKind {
    ProfileConfig,
    CodexAuthJson,
    ClaudeVsCodePluginConfig,
    GeminiCodeAssistSettings,
    ClaudeDesktopDeploymentConfig,
    ClaudeDesktopProfileConfig,
    ClaudeDesktopMetaConfig,
    ClaudeDesktopDeveloperSettings,
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
    base_url: String,
    api_key: String,
}

#[derive(Debug, Clone)]
struct ClaudeDesktopPaths {
    normal_config_path: PathBuf,
    threep_config_path: PathBuf,
    profile_path: PathBuf,
    meta_path: PathBuf,
    developer_settings_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct ClaudeDesktopInferenceModelSpec {
    pub name: String,
    pub label_override: Option<String>,
    pub supports_1m: bool,
}

const BUILTIN_OFFICIAL_ID_PREFIX: &str = "builtin-official-";
const PROTOCOL_OPENAI_CHAT_COMPLETIONS: &str = "openai-chat-completions";
const PROTOCOL_OPENAI_RESPONSES: &str = "openai-responses";
const PROTOCOL_ANTHROPIC_MESSAGES: &str = "anthropic-messages";
const PROTOCOL_GOOGLE_GEMINI: &str = "google-gemini";
const GATEWAY_FALLBACK_MODEL: &str = "default";
const CLAUDE_VSCODE_PLUGIN_PRIMARY_API_KEY: &str = "any";
const GEMINI_CODE_ASSIST_API_KEY_SETTING: &str = "geminicodeassist.geminiApiKey";
const CLAUDE_DESKTOP_PROFILE_ID: &str = "00000000-0000-4000-8000-000000157210";
const CLAUDE_DESKTOP_PROFILE_NAME: &str = "CodeStudio Lite";
const CLAUDE_DESKTOP_CONFIG_FILE: &str = "claude_desktop_config.json";
const CLAUDE_DESKTOP_CONFIG_LIBRARY_DIR: &str = "configLibrary";
const CLAUDE_DESKTOP_ROUTE_PREFIX: &str = "claude-";
const CLAUDE_DESKTOP_ANTHROPIC_ROUTE_PREFIX: &str = "anthropic/claude-";
const CLAUDE_DESKTOP_ONE_M_CONTEXT_MARKER: &str = "[1m]";
const CLAUDE_DESKTOP_DEFAULT_ROUTE_ID: &str = "claude-sonnet-4-6";
const CLAUDE_DESKTOP_DEFAULT_ROUTES: [(&str, bool); 4] = [
    ("claude-sonnet-4-6", true),
    ("claude-opus-4-8", true),
    ("claude-haiku-4-5", true),
    ("claude-fable-5", true),
];
const BUILTIN_OFFICIAL_PROFILES: [(&str, &str, &str); 8] = [
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
];

pub fn ensure_app_dirs() -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    storage::ensure_initialized()?;

    Ok(())
}

pub fn load_profile_summary() -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mut config = read_app_config()?;
    let mut drafts = load_profiles()?;
    let active_profiles_changed = clean_active_profiles(&mut config, &drafts)
        | sync_active_profiles_from_native_configs(&mut config, &mut drafts, &paths)?;
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

pub fn save_profile_draft(request: SaveProfileDraftRequest) -> Result<ProfileDraft, String> {
    ensure_app_dirs()?;

    let plan = build_profile_write_plan(
        &request.name,
        &request.app,
        request.mode.as_ref(),
        &request.provider,
        request.protocol.as_deref(),
        &request.model,
        &request.base_url,
        request.secret_provided,
    )?;
    ensure_profile_tool_installed(&plan.app)?;
    let now = Utc::now().to_rfc3339();
    let sort_order = storage::next_profile_sort_order(&plan.app, &plan.mode)?;
    let draft = ProfileDraft {
        id: plan.id,
        name: plan.name,
        icon: normalize_profile_icon(request.icon.as_deref())?,
        remark: normalize_profile_remark(request.remark.as_deref()),
        app: plan.app,
        is_builtin: false,
        mode: plan.mode,
        provider: plan.provider,
        protocol: plan.protocol,
        model: plan.model,
        base_url: plan.base_url,
        auth_ref: plan.auth_ref,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        last_test_status: Some("pending".to_string()),
        usage_enabled: false,
        sort_order,
    };

    capture_codex_oauth_profile_if_needed(&draft)?;
    storage::save_profile(&draft)?;
    if let (Some(auth_ref), Some(api_key)) = (draft.auth_ref.as_deref(), request.api_key.as_deref())
    {
        let trimmed = api_key.trim();
        if !trimmed.is_empty() {
            credentials::store_keychain_secret(auth_ref, trimmed)?;
        }
    }
    activity_log::append(
        Severity::Ok,
        format!(
            "Saved profile draft '{}' for {}/{}.",
            draft.name, draft.app, draft.provider
        ),
    )?;

    Ok(draft)
}

pub fn update_profile_draft(request: UpdateProfileDraftRequest) -> Result<ProfileDraft, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    if is_builtin_profile_id(&profile_id) {
        return Err("Built-in official profiles cannot be modified.".to_string());
    }
    let existing = load_profile_by_id(&profile_id)?;
    if existing.is_builtin {
        return Err("Built-in official profiles cannot be modified.".to_string());
    }
    let name = normalize_required("Profile Name", &request.name)?;
    let provider = normalize_provider_token(&request.provider)?;
    let mode = normalize_profile_mode(&provider, request.mode.as_ref())?;
    let protocol = normalize_protocol(request.protocol.as_deref())?;
    let app = canonical_profile_app(&existing.app);
    ensure_custom_official_profile_allowed(&app, &provider, mode)?;
    ensure_profile_protocol_supported_for_mode(&app, mode, &provider, &protocol)?;
    let model = request.model.trim().to_string();
    let base_url = validate_base_url_for_provider(&provider, &request.base_url)?;
    let now = Utc::now().to_rfc3339();
    let created_at = existing.created_at.clone().unwrap_or_else(|| now.clone());
    let api_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let auth_ref = if provider_is_official(&provider) {
        None
    } else if api_key.is_some() {
        Some(
            existing
                .auth_ref
                .clone()
                .unwrap_or_else(|| format!("keychain:codestudio-lite/{profile_id}/api_key")),
        )
    } else {
        existing.auth_ref.clone()
    };
    if provider_requires_api_key(&provider) && auth_ref.is_none() {
        return Err("Provider API key is required for non-official providers.".to_string());
    }
    let updated = ProfileDraft {
        id: profile_id.clone(),
        name,
        icon: normalize_profile_icon(request.icon.as_deref())?,
        remark: normalize_profile_remark(request.remark.as_deref()),
        app,
        is_builtin: false,
        mode,
        provider,
        protocol,
        model,
        base_url,
        auth_ref,
        created_at: Some(created_at.clone()),
        updated_at: Some(now.clone()),
        last_test_status: Some("pending".to_string()),
        usage_enabled: existing.usage_enabled,
        sort_order: existing.sort_order,
    };

    storage::save_profile(&updated)?;
    if let (Some(auth_ref), Some(api_key)) = (updated.auth_ref.as_deref(), api_key) {
        credentials::store_keychain_secret(auth_ref, api_key)?;
    }
    let mut config = read_app_config()?;
    let drafts = load_profiles()?;
    if clean_active_profiles(&mut config, &drafts) {
        write_app_config(&config)?;
    }

    activity_log::append(
        Severity::Ok,
        format!(
            "Updated profile draft '{}' for {}/{}.",
            updated.name, updated.app, updated.provider
        ),
    )?;

    Ok(updated)
}

pub fn duplicate_profile_draft(
    request: DuplicateProfileDraftRequest,
) -> Result<ProfileDraft, String> {
    ensure_app_dirs()?;

    let source_id = normalize_token("Profile ID", &request.profile_id)?;
    let source = load_profile_by_id(&source_id)?;
    if source.is_builtin || is_builtin_profile_id(&source.id) {
        return Err("Built-in official profiles cannot be duplicated.".to_string());
    }
    ensure_profile_tool_installed(&canonical_profile_app(&source.app))?;
    let new_id = unique_profile_id(&slugify(&source.name))?;
    let now = Utc::now().to_rfc3339();
    let app = canonical_profile_app(&source.app);
    let sort_order = storage::next_profile_sort_order(&app, &source.mode)?;
    let auth_ref = if provider_is_official(&source.provider) {
        None
    } else {
        source
            .auth_ref
            .as_ref()
            .map(|_| format!("keychain:codestudio-lite/{new_id}/api_key"))
    };

    if let (Some(source_auth_ref), Some(target_auth_ref)) =
        (source.auth_ref.as_deref(), auth_ref.as_deref())
    {
        let secret = credentials::load_keychain_secret(source_auth_ref)?;
        let trimmed = secret.trim();
        if trimmed.is_empty() {
            return Err("Stored Provider API key is empty.".to_string());
        }
        credentials::store_keychain_secret(target_auth_ref, trimmed)?;
    }

    let duplicated = ProfileDraft {
        id: new_id,
        name: source.name.clone(),
        icon: source.icon.clone(),
        remark: source.remark.clone(),
        app,
        is_builtin: false,
        mode: source.mode,
        provider: source.provider.clone(),
        protocol: source.protocol.clone(),
        model: source.model.clone(),
        base_url: source.base_url.clone(),
        auth_ref,
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        last_test_status: source.last_test_status.clone(),
        usage_enabled: false,
        sort_order,
    };
    clone_codex_oauth_profile_if_needed(&source, &duplicated)?;
    storage::save_profile(&duplicated)?;
    activity_log::append(
        Severity::Ok,
        format!(
            "Duplicated profile draft '{}' for {}/{}.",
            duplicated.name, duplicated.app, duplicated.provider
        ),
    )?;

    Ok(duplicated)
}

pub fn delete_profile_draft(request: DeleteProfileDraftRequest) -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    if is_builtin_profile_id(&profile_id) {
        return Err("Built-in official profiles cannot be deleted.".to_string());
    }
    let source = load_profile_by_id(&profile_id)?;
    if source.is_builtin {
        return Err("Built-in official profiles cannot be deleted.".to_string());
    }

    if !storage::delete_profile(&profile_id)? {
        return Err(format!("Profile '{profile_id}' does not exist"));
    }
    delete_codex_oauth_profile_cache_if_needed(&source)?;

    let mut config = read_app_config()?;
    let mut changed =
        replace_deleted_active_profile_with_official(&mut config, &source.app, &profile_id);
    let drafts = load_profiles()?;
    changed |= clean_active_profiles(&mut config, &drafts);
    if changed {
        write_app_config(&config)?;
    }

    activity_log::append(
        Severity::Ok,
        format!(
            "Deleted profile draft '{}' for {}/{}.",
            source.name, source.app, source.provider
        ),
    )?;

    load_profile_summary()
}

pub fn reorder_profile_drafts(
    request: ReorderProfileDraftsRequest,
) -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;

    let app = canonical_profile_app(&normalize_token("Tool", &request.app)?);
    let mode = request.mode;
    let profiles = load_profiles()?;
    let expected_ids = profiles
        .iter()
        .filter(|profile| canonical_profile_app(&profile.app) == app && profile.mode == mode)
        .map(|profile| profile.id.clone())
        .collect::<HashSet<_>>();
    let requested_ids = request
        .profile_ids
        .iter()
        .map(|id| normalize_token("Profile ID", id))
        .collect::<Result<Vec<_>, _>>()?;
    let requested_set = requested_ids.iter().cloned().collect::<HashSet<_>>();
    if requested_set != expected_ids {
        return Err("Profile order must include every profile in this tool category.".to_string());
    }

    storage::reorder_profiles(&app, &mode, &requested_ids)?;
    activity_log::append(
        Severity::Info,
        format!(
            "Reordered {} profile draft(s) for {app}/{}.",
            requested_ids.len(),
            provider_apply_mode_value(&mode)
        ),
    )?;

    load_profile_summary()
}

pub fn preview_profile_write(
    request: PreviewProfileWriteRequest,
) -> Result<PreviewProfileWriteResult, String> {
    let plan = build_profile_write_plan(
        &request.name,
        &request.app,
        request.mode.as_ref(),
        &request.provider,
        request.protocol.as_deref(),
        &request.model,
        &request.base_url,
        request.secret_provided,
    )?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let base_id = slugify(&plan.name);
    let tool = tool_registry::ai_tools()
        .into_iter()
        .find(|tool| tool.id == plan.app);
    let target_tool_path = tool
        .as_ref()
        .and_then(|definition| definition.config_relative_path)
        .map(|relative| display_path(&paths.home_dir.join(relative)));
    let target_tool_name = tool
        .as_ref()
        .map(|definition| definition.name)
        .unwrap_or("Target tool");
    let mut warnings = Vec::new();

    if plan.id != base_id && !base_id.is_empty() {
        warnings.push(format!(
            "Profile id '{base_id}' already exists, so this draft will use '{}'.",
            plan.id
        ));
    }
    if tool.is_none() {
        warnings.push(format!("Tool '{}' is not in the local registry.", plan.app));
    }
    let now = Utc::now().to_rfc3339();
    let database_path = display_path(&paths.database_file);
    let preview_profile = ProfileDraft {
        id: plan.id.clone(),
        name: plan.name.clone(),
        icon: normalize_profile_icon(request.icon.as_deref())?,
        remark: normalize_profile_remark(request.remark.as_deref()),
        app: plan.app.clone(),
        is_builtin: false,
        mode: plan.mode,
        provider: plan.provider.clone(),
        protocol: plan.protocol.clone(),
        model: plan.model.clone(),
        base_url: plan.base_url.clone(),
        auth_ref: plan.auth_ref.clone(),
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        last_test_status: Some("pending".to_string()),
        usage_enabled: false,
        sort_order: 0,
    };
    let profile_content =
        profile_sql_preview_content(&preview_profile, plan.secret_status, "pending")?;
    let mut items = vec![
        ProfileWritePreviewItem {
            label: "Profile row".to_string(),
            path: Some(database_path.clone()),
            action: "create".to_string(),
            backup_required: false,
            detail: format!(
                "Save Profile Draft stores normalized metadata in SQLite for {}/{} and excludes API keys.",
                plan.protocol, plan.provider
            ),
            content: Some(profile_content),
        },
        ProfileWritePreviewItem {
            label: "Active tool profile pointer".to_string(),
            path: Some(database_path.clone()),
            action: "not_modified".to_string(),
            backup_required: false,
            detail: "Saving a draft does not switch the active profile.".to_string(),
            content: None,
        },
    ];

    items.push(ProfileWritePreviewItem {
        label: format!("{target_tool_name} config"),
        path: target_tool_path.clone(),
        action: "future_confirmation_required".to_string(),
        backup_required: target_tool_path.is_some(),
        detail: "Client config is not modified when saving a Provider Profile. Client Bootstrap remains a separate confirmation flow."
            .to_string(),
        content: None,
    });

    items.push(ProfileWritePreviewItem {
        label: "Credential".to_string(),
        path: None,
        action: plan.secret_status.to_string(),
        backup_required: false,
        detail: credential_detail(&plan.provider, request.secret_provided),
        content: None,
    });

    Ok(PreviewProfileWriteResult {
        generated_at: now,
        profile_id: plan.id,
        profile_path: database_path,
        target_tool_path,
        backup_required: false,
        items,
        warnings,
    })
}

pub fn preview_profile_apply(
    request: PreviewProfileApplyRequest,
) -> Result<PreviewProfileApplyResult, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    let profile = load_profile_by_id(&profile_id)?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let is_codex_tool = is_codex_family_app(&profile.app);
    let tool = tool_registry::ai_tools()
        .into_iter()
        .find(|tool| tool.id == profile.app);
    let native_config_path = native_config_path_for_profile_mode(&profile, &paths, profile.mode)?
        .map(|path| display_path(&path))
        .or_else(|| {
            tool.as_ref()
                .and_then(|definition| definition.config_relative_path)
                .map(|relative| display_path(&paths.home_dir.join(relative)))
        });
    let tool_name = tool
        .as_ref()
        .map(|definition| definition.name)
        .or_else(|| is_codex_tool.then_some("Codex Desktop"))
        .unwrap_or("Target tool");
    let config_native_diff = build_native_config_preview(
        &profile,
        native_config_path.as_deref(),
        &paths,
        ProviderApplyMode::Config,
    )?;
    let gateway_native_diff = build_native_config_preview(
        &profile,
        native_config_path.as_deref(),
        &paths,
        ProviderApplyMode::Gateway,
    )?;
    let config_native_diff = attach_native_config_content_preview(
        config_native_diff,
        &profile,
        &paths,
        ProviderApplyMode::Config,
    );
    let gateway_native_diff = attach_native_config_content_preview(
        gateway_native_diff,
        &profile,
        &paths,
        ProviderApplyMode::Gateway,
    );
    let native_diff = match profile.mode {
        ProviderApplyMode::Config => config_native_diff.clone(),
        ProviderApplyMode::Gateway => gateway_native_diff.clone(),
    };
    let native_write_enabled = native_diff
        .as_ref()
        .map(|diff| diff.write_enabled)
        .unwrap_or(false);
    let mode_previews =
        build_provider_mode_previews(&profile, &config_native_diff, &gateway_native_diff);
    let mut warnings = Vec::new();

    if tool.is_none() && !is_codex_tool {
        warnings.push(format!(
            "Tool '{}' is not in the local registry, so this profile cannot be applied yet.",
            profile.app
        ));
    }
    let env_conflicts = env_health::claude_env_conflicts_for_profile(&profile);

    Ok(PreviewProfileApplyResult {
        generated_at: Utc::now().to_rfc3339(),
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        app: profile.app.clone(),
        provider: profile.provider.clone(),
        can_apply: tool.is_some() || is_codex_tool,
        items: vec![
            ProfileApplyPreviewItem {
                label: "Active tool profile pointer".to_string(),
                path: Some(display_path(&paths.database_file)),
                action: "update".to_string(),
                backup_required: false,
                detail: format!(
                    "Sets the SQLite active profile pointer for '{}' to '{}' before refreshing detection.",
                    profile.app, profile.id
                ),
            },
            ProfileApplyPreviewItem {
                label: format!("{tool_name} native config"),
                path: native_config_path,
                action: if native_write_enabled {
                    "create_or_update".to_string()
                } else {
                    "not_modified".to_string()
                },
                backup_required: native_write_enabled,
                detail: if native_write_enabled {
                    "Selected mode writes this client config; detailed file changes are shown below."
                        .to_string()
                } else {
                    "This profile does not require a native client config write."
                        .to_string()
                },
            },
            ProfileApplyPreviewItem {
                label: "Credential".to_string(),
                path: None,
                action: "not_written".to_string(),
                backup_required: false,
                detail: "Apply writes no API keys or tokens. Existing official login/keychain state remains untouched."
                    .to_string(),
            },
        ],
        native_diff,
        mode_previews,
        warnings,
        env_conflicts,
    })
}

fn build_provider_mode_previews(
    profile: &ProfileDraft,
    config_native_diff: &Option<NativeConfigPreview>,
    gateway_native_diff: &Option<NativeConfigPreview>,
) -> Vec<ProviderApplyModePreview> {
    let is_codex_tool = is_codex_family_app(&profile.app);
    let is_official = provider_is_official(&profile.provider);
    let official_client_config = is_official && !is_codex_tool;
    let config_protocol_supported = config_file_protocol_supported(profile);
    let config_supported = config_native_diff.is_some() || official_client_config;
    let config_writes_native_config = native_preview_writes(config_native_diff);
    let gateway_writes_native_config = native_preview_writes(gateway_native_diff);
    let gateway_supported = !is_official;
    let config_blocked_reason = if !config_protocol_supported && !is_official {
        Some(format!(
            "Config profiles do not support {} for '{}'.",
            protocol_display_name(&profile.protocol),
            profile.app
        ))
    } else if !config_supported && !is_official {
        Some(format!(
            "Config profile adapter is not implemented for '{}'.",
            profile.app
        ))
    } else if profile.auth_ref.is_none() && provider_requires_api_key(&profile.provider) {
        Some("Config profiles need a stored Provider API key for this Provider.".to_string())
    } else {
        None
    };

    vec![
        ProviderApplyModePreview {
            mode: ProviderApplyMode::Config,
            label: "Client config profile".to_string(),
            description: "Back up and modify the target client's native provider config directly. This makes the client talk to the selected upstream Provider without CodeStudio Lite in the request path."
                .to_string(),
            supported: config_supported && config_blocked_reason.is_none(),
            recommended: is_official && config_supported && config_blocked_reason.is_none(),
            writes_native_config: config_writes_native_config,
            starts_gateway: false,
            blocked_reason: config_blocked_reason,
            native_diff: config_native_diff.clone(),
            warnings: if official_client_config {
                vec![
                    "Official provider uses the target client's own login.".to_string(),
                    "No Provider API key or model override is required.".to_string(),
                ]
            } else if config_supported {
                vec![
                    "Config profiles write Provider connection details into the client config.".to_string(),
                    "Frequent Provider switching may require the client to reload its own config.".to_string(),
                ]
            } else {
                Vec::new()
            },
        },
        ProviderApplyModePreview {
            mode: ProviderApplyMode::Gateway,
            label: "Gateway profile".to_string(),
            description: if gateway_writes_native_config {
                "Back up and point the client at the local CodeStudio Gateway once. This apply only switches the active Provider profile; start the Gateway from the sidebar when needed."
            } else {
                "Switch the active Provider profile for the local Gateway. This apply does not start the Gateway or modify this tool's native config."
            }
                .to_string(),
            supported: gateway_supported,
            recommended: gateway_supported && !is_official,
            writes_native_config: gateway_writes_native_config,
            starts_gateway: false,
            blocked_reason: if is_official {
                Some("Official provider uses the client login directly and does not run through the local gateway.".to_string())
            } else {
                None
            },
            native_diff: gateway_native_diff.clone(),
            warnings: if gateway_supported {
                let mut warnings = vec![
                    "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.".to_string(),
                    "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.".to_string(),
                ];
                if gateway_writes_native_config {
                    warnings.push(
                        "The client still needs to reload config after the first gateway bootstrap."
                            .to_string(),
                    );
                } else {
                    warnings.push(format!(
                        "No native gateway bootstrap is written for '{}'; configure the client to use the Gateway URL manually or wait for a validated adapter.",
                        profile.app
                    ));
                }
                warnings
            } else {
                Vec::new()
            },
        },
    ]
}

fn attach_native_config_content_preview(
    preview: Option<NativeConfigPreview>,
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
) -> Option<NativeConfigPreview> {
    let mut preview = preview?;
    normalize_native_config_preview(&mut preview);
    if !preview.write_enabled {
        preview.content = None;
        return Some(preview);
    }
    if let Ok(Some(content)) =
        build_native_config_content_preview(profile, paths, mode, &preview.path)
    {
        preview.content = Some(redact_native_config_preview_content(
            &content, profile, mode,
        ));
    }
    Some(preview)
}

fn build_native_config_content_preview(
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
    preview_path: &str,
) -> Result<Option<String>, String> {
    if provider_is_official(&profile.provider) && mode == ProviderApplyMode::Gateway {
        return Ok(None);
    }

    if canonical_profile_app(&profile.app) == "claude-desktop" {
        let desktop_paths = claude_desktop_paths(paths)?;
        if display_path(&desktop_paths.profile_path) != preview_path {
            return Ok(None);
        }
        if mode == ProviderApplyMode::Config && provider_is_official(&profile.provider) {
            return Ok(None);
        }
        let content = match mode {
            ProviderApplyMode::Config => claude_desktop_direct_profile_content_with_api_key(
                profile,
                secret_preview(profile),
            )?,
            ProviderApplyMode::Gateway => claude_desktop_gateway_profile_content(profile)?,
        };
        return Ok(Some(content));
    }

    let Some(path) = native_config_path_for_profile_mode(profile, paths, mode)? else {
        return Ok(None);
    };
    if display_path(&path) != preview_path {
        return Ok(None);
    }

    let current = read_file_if_exists(&path)?;
    let render = |current: &str| native_config_content_for_preview(current, profile, mode);
    match render(&current) {
        Ok(content) => Ok(Some(content)),
        Err(err) if preview_content_parse_error(&err) => render("").map(Some),
        Err(err) => Err(err),
    }
}

fn native_config_content_for_preview(
    current: &str,
    profile: &ProfileDraft,
    mode: ProviderApplyMode,
) -> Result<String, String> {
    match mode {
        ProviderApplyMode::Config => match canonical_profile_app(&profile.app).as_str() {
            "codex" => {
                if provider_is_official(&profile.provider) {
                    codex_official_config_content(current, profile)
                } else {
                    codex_direct_config_content(current, profile)
                }
            }
            "claude" => {
                if provider_is_official(&profile.provider) {
                    claude_official_config_content(current)
                } else {
                    claude_config_content_with_api_key(current, profile, secret_preview(profile))
                }
            }
            "gemini" => {
                if provider_is_official(&profile.provider) {
                    Ok(gemini_official_env_content(current))
                } else {
                    gemini_env_content_with_api_key(current, profile, secret_preview(profile))
                }
            }
            "gemini-code-assist" => {
                if provider_is_official(&profile.provider) {
                    gemini_code_assist_official_settings_content(current)
                } else {
                    gemini_code_assist_settings_content_with_api_key(
                        current,
                        profile,
                        secret_preview(profile),
                    )
                }
            }
            "opencode" => {
                if provider_is_official(&profile.provider) {
                    opencode_official_config_content(current)
                } else {
                    opencode_config_content_with_api_key(current, profile, secret_preview(profile))
                }
            }
            "openclaw" => {
                if provider_is_official(&profile.provider) {
                    openclaw_official_config_content(current)
                } else {
                    openclaw_config_content_with_api_key(current, profile, secret_preview(profile))
                }
            }
            "hermes" => {
                if provider_is_official(&profile.provider) {
                    hermes_official_config_content(current)
                } else {
                    hermes_config_content_with_api_key(current, profile, secret_preview(profile))
                }
            }
            _ => Err(format!(
                "Config profile adapter is not implemented for tool '{}'.",
                profile.app
            )),
        },
        ProviderApplyMode::Gateway => match canonical_profile_app(&profile.app).as_str() {
            "codex" => codex_gateway_config_content(current, profile),
            "claude" => claude_gateway_config_content(current, profile),
            "gemini" => gemini_gateway_env_content(current, profile),
            "opencode" => opencode_gateway_config_content(current, profile),
            "openclaw" => openclaw_gateway_config_content(current, profile),
            "hermes" => hermes_gateway_config_content(current, profile),
            _ => Err(format!(
                "Gateway profile adapter is not implemented for tool '{}'.",
                profile.app
            )),
        },
    }
}

fn preview_content_parse_error(err: &str) -> bool {
    err.starts_with("Existing ") && err.contains(" could not be parsed")
}

fn native_preview_writes(preview: &Option<NativeConfigPreview>) -> bool {
    preview
        .as_ref()
        .map(|preview| preview.write_enabled)
        .unwrap_or(false)
}

fn normalize_native_config_preview(preview: &mut NativeConfigPreview) {
    preview.changes.retain(native_config_change_writes);
    if preview.changes.is_empty() {
        preview.write_enabled = false;
    }
}

fn native_config_change_writes(change: &NativeConfigDiffLine) -> bool {
    change.action != "unchanged" && change.before != change.after
}

pub fn apply_profile(request: ApplyProfileRequest) -> Result<ApplyProfileResult, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    let profiles = load_profiles()?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("Profile '{profile_id}' does not exist"))?;
    let is_codex_tool = is_codex_family_app(&profile.app);
    let is_registered_tool = tool_registry::ai_tools()
        .into_iter()
        .any(|tool| tool.id == profile.app);
    if !is_registered_tool && !is_codex_tool {
        return Err(format!(
            "Tool '{}' is not in the local registry, so this profile cannot be applied yet.",
            profile.app
        ));
    }
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mode = profile.mode;
    if request.restart_after_apply && mode != ProviderApplyMode::Config {
        return Err("Apply and restart is only available for Config profiles.".to_string());
    }
    let native_plans = filter_native_write_plans(build_native_apply_plan(
        &profile,
        &paths,
        &mode,
        request.sync_claude_vs_code,
    )?)?;
    if request.restart_after_apply && native_plans.is_empty() {
        return Err(
            "Apply and restart requires a native client config write for this profile.".to_string(),
        );
    }
    let mut config = read_app_config()?;
    if clean_active_profiles(&mut config, &profiles) {
        write_app_config(&config)?;
    }
    if profile_is_active(&config, &profile) {
        return Err("Profile is already active for this tool and mode.".to_string());
    }
    let mut backup_targets = Vec::new();
    for plan in &native_plans {
        backup_targets.push(plan.path.clone());
    }
    let backup = backup::backup_files("apply-profile", Some(&profile.id), &backup_targets)?;

    activate_profile_for_tool(&mut config, &profile, &profiles);
    write_app_config(&config)?;
    let verified = verify_active_profile(&config, &profile);
    if !verified {
        return Err("Applied profile database record did not pass verification".to_string());
    }
    let native_verified = if native_plans.is_empty() {
        false
    } else {
        for plan in &native_plans {
            apply_native_config_write_plan(plan)?;
        }
        native_plans
            .iter()
            .map(|plan| verify_native_config_write(plan, &profile, &mode))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .all(|verified| verified)
    };
    let restart_outcome = if request.restart_after_apply {
        restart_tool_for_profile(
            &profile,
            RestartContext {
                sync_claude_vs_code: request.sync_claude_vs_code,
            },
        )?
    } else {
        RestartOutcome {
            performed: false,
            message: None,
        }
    };

    activity_log::append(
        Severity::Ok,
        if mode == ProviderApplyMode::Gateway {
            format!(
                "Applied profile '{}' for {}/{} in Gateway profile.",
                profile.name, profile.app, profile.provider
            )
        } else if native_verified && mode == ProviderApplyMode::Config {
            format!(
                "Applied profile '{}' for {}/{} through direct client config profile.",
                profile.name, profile.app, profile.provider
            )
        } else {
            format!(
                "Applied profile '{}' for {}/{}.",
                profile.name, profile.app, profile.provider
            )
        },
    )?;

    let env_conflicts = env_health::claude_env_conflicts_for_profile(&profile);

    Ok(ApplyProfileResult {
        summary: load_profile_summary()?,
        mode,
        backup,
        applied_path: display_path(&paths.database_file),
        verified,
        native_path: native_plans.first().map(|plan| display_path(&plan.path)),
        native_verified,
        restart_requested: request.restart_after_apply,
        restart_performed: restart_outcome.performed,
        restart_message: restart_outcome.message,
        gateway_status: None,
        env_conflicts,
    })
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
    let active_id = active_profiles.get(app)?;
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

struct RestartOutcome {
    performed: bool,
    message: Option<String>,
}

struct RestartProcessResult {
    total: u64,
    forced: u64,
    remaining: u64,
    paths: Vec<String>,
}

#[derive(Clone, Copy, Default)]
struct RestartContext {
    sync_claude_vs_code: bool,
}

#[derive(Clone, Copy)]
enum RestartLaunch {
    CloseOnly,
    CodexClient,
    Command {
        command: &'static str,
        hidden: bool,
    },
    ExistingProcessPath {
        fallback_command: &'static str,
        hidden: bool,
    },
    MsixPackage {
        package_identities: &'static [&'static str],
    },
}

#[derive(Clone, Copy)]
struct RestartTarget {
    label: &'static str,
    process_names: &'static [&'static str],
    command_markers: &'static [&'static str],
    exclude_command_markers: &'static [&'static str],
    require_window: bool,
    reject_window: bool,
    launch: RestartLaunch,
}

fn restart_tool_for_profile(
    profile: &ProfileDraft,
    context: RestartContext,
) -> Result<RestartOutcome, String> {
    let app = canonical_profile_app(&profile.app);
    let targets = restart_targets_for_app(&app, context);
    if targets.is_empty() {
        return Ok(RestartOutcome {
            performed: false,
            message: Some(format!(
                "Tool '{}' does not have a client that needs automatic restart.",
                profile.app
            )),
        });
    }

    let mut messages = Vec::new();
    let mut restarted_any = false;

    for target in targets {
        let result = stop_restart_target_processes(target)?;
        if result.total == 0 {
            continue;
        }
        if result.remaining > 0 {
            return Err(format!(
                "{} is still running; restart was not continued.",
                target.label
            ));
        }

        launch_restart_target(target, &result.paths)?;
        restarted_any = true;
        messages.push(restart_target_message(target, &result));
    }

    if restarted_any {
        Ok(RestartOutcome {
            performed: true,
            message: Some(messages.join(" ")),
        })
    } else {
        Ok(RestartOutcome {
            performed: false,
            message: Some(format!(
                "{} is not running, so no restart is needed.",
                restart_category_label(&app, context)
            )),
        })
    }
}

fn restart_targets_for_app(app: &str, context: RestartContext) -> Vec<RestartTarget> {
    const CODEX_DESKTOP_NAMES: &[&str] = &["Codex.exe", "Codex"];
    const CODEX_CLI_NAMES: &[&str] = &["codex.exe", "codex"];
    const CODEX_CLI_MARKERS: &[&str] = &[
        "@openai/codex",
        "@openai\\codex",
        "node_modules/@openai/codex",
        "node_modules\\@openai\\codex",
    ];
    const VSCODE_NAMES: &[&str] = &["Code.exe", "Code", "Code - Insiders.exe", "Code - Insiders"];
    const CODEX_VSCODE_BACKEND_MARKERS: &[&str] = &[
        ".vscode/extensions/openai.chatgpt",
        ".vscode\\extensions\\openai.chatgpt",
        "codex app-server",
        "codex.exe app-server",
    ];
    const CLAUDE_DESKTOP_NAMES: &[&str] = &["Claude.exe", "Claude"];
    const CLAUDE_CLI_NAMES: &[&str] = &["claude.exe", "claude"];
    const CLAUDE_CLI_MARKERS: &[&str] = &[
        "@anthropic-ai/claude-code",
        "@anthropic-ai\\claude-code",
        "node_modules/@anthropic-ai/claude-code",
        "node_modules\\@anthropic-ai\\claude-code",
    ];
    const CLAUDE_VSCODE_BACKEND_MARKERS: &[&str] = &[
        ".vscode/extensions/anthropic.claude-code",
        ".vscode\\extensions\\anthropic.claude-code",
        "resources/native-binary/claude",
        "resources\\native-binary\\claude",
    ];
    const GEMINI_CLI_NAMES: &[&str] = &["gemini.exe", "gemini"];
    const GEMINI_CLI_MARKERS: &[&str] = &["@google/gemini-cli", "@google\\gemini-cli"];
    const OPENCODE_NAMES: &[&str] = &["opencode.exe", "opencode"];
    const OPENCODE_MARKERS: &[&str] = &["opencode-ai"];
    const OPENCLAW_NAMES: &[&str] = &["openclaw.exe", "openclaw"];
    const HERMES_NAMES: &[&str] = &["hermes.exe", "hermes", "Hermes"];
    const EMPTY: &[&str] = &[];
    match app {
        "codex" => vec![
            RestartTarget {
                label: "Codex",
                process_names: CODEX_DESKTOP_NAMES,
                command_markers: EMPTY,
                exclude_command_markers: EMPTY,
                require_window: true,
                reject_window: false,
                launch: RestartLaunch::CodexClient,
            },
            RestartTarget {
                label: "Codex VS Code extension backend",
                process_names: EMPTY,
                command_markers: CODEX_VSCODE_BACKEND_MARKERS,
                exclude_command_markers: EMPTY,
                require_window: false,
                reject_window: false,
                launch: RestartLaunch::CloseOnly,
            },
            RestartTarget {
                label: "Codex CLI",
                process_names: CODEX_CLI_NAMES,
                command_markers: CODEX_CLI_MARKERS,
                exclude_command_markers: CODEX_VSCODE_BACKEND_MARKERS,
                require_window: false,
                reject_window: true,
                launch: RestartLaunch::Command {
                    command: "codex",
                    hidden: true,
                },
            },
        ],
        "claude-desktop" => vec![RestartTarget {
            label: "Claude Desktop",
            process_names: CLAUDE_DESKTOP_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: true,
            reject_window: false,
            launch: if cfg!(target_os = "windows") {
                RestartLaunch::MsixPackage {
                    package_identities: detector::claude_desktop_windows_package_identities(),
                }
            } else {
                RestartLaunch::ExistingProcessPath {
                    fallback_command: "Claude",
                    hidden: false,
                }
            },
        }],
        "claude" => {
            let mut targets = vec![RestartTarget {
                label: "Claude Code",
                process_names: CLAUDE_CLI_NAMES,
                command_markers: CLAUDE_CLI_MARKERS,
                exclude_command_markers: CLAUDE_VSCODE_BACKEND_MARKERS,
                require_window: false,
                reject_window: true,
                launch: RestartLaunch::Command {
                    command: "claude",
                    hidden: true,
                },
            }];
            if context.sync_claude_vs_code {
                targets.push(RestartTarget {
                    label: "Claude VS Code extension backend",
                    process_names: EMPTY,
                    command_markers: CLAUDE_VSCODE_BACKEND_MARKERS,
                    exclude_command_markers: EMPTY,
                    require_window: false,
                    reject_window: false,
                    launch: RestartLaunch::CloseOnly,
                });
            }
            targets
        }
        "gemini" => vec![RestartTarget {
            label: "Gemini CLI",
            process_names: GEMINI_CLI_NAMES,
            command_markers: GEMINI_CLI_MARKERS,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "gemini",
                hidden: true,
            },
        }],
        "gemini-code-assist" => vec![RestartTarget {
            label: "Gemini Code Assist",
            process_names: VSCODE_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: true,
            reject_window: false,
            launch: RestartLaunch::ExistingProcessPath {
                fallback_command: "code",
                hidden: false,
            },
        }],
        "opencode" => vec![RestartTarget {
            label: "OpenCode",
            process_names: OPENCODE_NAMES,
            command_markers: OPENCODE_MARKERS,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "opencode",
                hidden: true,
            },
        }],
        "openclaw" => vec![RestartTarget {
            label: "OpenClaw",
            process_names: OPENCLAW_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "openclaw",
                hidden: true,
            },
        }],
        "hermes" => vec![RestartTarget {
            label: "Hermes",
            process_names: HERMES_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "hermes",
                hidden: true,
            },
        }],
        _ => Vec::new(),
    }
}

fn restart_category_label(app: &str, context: RestartContext) -> &'static str {
    match app {
        "codex" => "Codex, Codex CLI, or Codex VS Code extension backend",
        "claude-desktop" => "Claude Desktop",
        "claude" if context.sync_claude_vs_code => {
            "Claude Code or Claude VS Code extension backend"
        }
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "gemini-code-assist" => "Gemini Code Assist",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        _ => "target tool",
    }
}

fn restart_target_message(target: RestartTarget, result: &RestartProcessResult) -> String {
    if matches!(target.launch, RestartLaunch::CloseOnly) {
        if result.forced > 0 {
            return format!(
                "Force-closed {} {} process(es); VS Code will restart the backend when needed.",
                result.forced, target.label
            );
        }
        return format!("Restarted {}.", target.label);
    }

    if result.forced > 0 {
        format!(
            "Force-closed {} {} process(es) and restarted.",
            result.forced, target.label
        )
    } else {
        format!("Restarted {}.", target.label)
    }
}

fn stop_restart_target_processes(target: RestartTarget) -> Result<RestartProcessResult, String> {
    if cfg!(target_os = "macos") {
        return stop_restart_target_processes_macos(target);
    }

    if !cfg!(target_os = "windows") {
        return Ok(RestartProcessResult {
            total: 0,
            forced: 0,
            remaining: 0,
            paths: Vec::new(),
        });
    }

    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$Names = {names}
$Markers = {markers}
$ExcludeMarkers = {exclude_markers}
$RequireWindow = ${require_window}
$RejectWindow = ${reject_window}
function Test-TargetProcess($process) {{
  $name = [string]$process.Name
  $nameMatch = $false
  foreach ($candidate in $Names) {{
    if ($name.Equals($candidate, [System.StringComparison]::OrdinalIgnoreCase)) {{
      $nameMatch = $true
      break
    }}
  }}
  $markerMatch = $false
  if ($Markers.Count -gt 0) {{
    $haystack = ((([string]$process.CommandLine) + "`n" + ([string]$process.ExecutablePath))).ToLowerInvariant()
    foreach ($marker in $Markers) {{
      if ($haystack.Contains(([string]$marker).ToLowerInvariant())) {{
        $markerMatch = $true
        break
      }}
    }}
  }}
	  if (-not ($nameMatch -or $markerMatch)) {{ return $false }}
	  if ($ExcludeMarkers.Count -gt 0) {{
	    $haystack = ((([string]$process.CommandLine) + "`n" + ([string]$process.ExecutablePath))).ToLowerInvariant()
	    foreach ($marker in $ExcludeMarkers) {{
	      if ($haystack.Contains(([string]$marker).ToLowerInvariant())) {{
	        return $false
	      }}
	    }}
	  }}
	  if ($RequireWindow) {{
	    try {{
	      $gp = Get-Process -Id $process.ProcessId -ErrorAction Stop
	      if ($gp.MainWindowHandle -eq 0) {{ return $false }}
	    }} catch {{
	      return $false
	    }}
	  }}
	  if ($RejectWindow) {{
	    try {{
	      $gp = Get-Process -Id $process.ProcessId -ErrorAction Stop
	      if ($gp.MainWindowHandle -ne 0) {{ return $false }}
	    }} catch {{}}
	  }}
	  return $true
	}}
$procs = @(Get-CimInstance Win32_Process | Where-Object {{ Test-TargetProcess $_ }})
$targetIds = @($procs | ForEach-Object {{ [int]$_.ProcessId }})
$paths = @($procs | ForEach-Object {{ [string]$_.ExecutablePath }} | Where-Object {{ $_ }} | Select-Object -Unique)
foreach ($id in $targetIds) {{
  try {{
    $p = Get-Process -Id $id -ErrorAction Stop
    if ($p.MainWindowHandle -ne 0) {{ [void]$p.CloseMainWindow() }}
  }} catch {{}}
}}
$deadline = (Get-Date).AddSeconds(8)
while ((Get-Date) -lt $deadline) {{
  Start-Sleep -Milliseconds 250
  $remaining = @()
  foreach ($id in $targetIds) {{
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -ne $p) {{ $remaining += $p }}
  }}
  if ($remaining.Count -eq 0) {{ break }}
}}
$remaining = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $remaining += $p }}
}}
$forced = 0
foreach ($p in $remaining) {{
  try {{
    Stop-Process -Id $p.Id -Force -ErrorAction Stop
    $forced += 1
  }} catch {{}}
}}
Start-Sleep -Milliseconds 300
$still = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $still += $p }}
}}
[pscustomobject]@{{
  total = [int](@($targetIds).Count)
  forced = [int]$forced
  remaining = [int](@($still).Count)
  paths = @($paths)
}} | ConvertTo-Json -Compress
"#,
        names = ps_array(target.process_names),
        markers = ps_array(target.command_markers),
        exclude_markers = ps_array(target.exclude_command_markers),
        require_window = if target.require_window {
            "true"
        } else {
            "false"
        },
        reject_window = if target.reject_window {
            "true"
        } else {
            "false"
        },
    );
    let json = run_powershell(&script)?;
    #[derive(Deserialize)]
    struct RawRestartProcessResult {
        total: Option<u64>,
        forced: Option<u64>,
        remaining: Option<u64>,
        #[serde(default)]
        paths: Vec<String>,
    }
    let value: RawRestartProcessResult = serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse {} restart result: {err}", target.label))?;
    Ok(RestartProcessResult {
        total: value.total.unwrap_or(0),
        forced: value.forced.unwrap_or(0),
        remaining: value.remaining.unwrap_or(0),
        paths: value.paths,
    })
}

fn launch_restart_target(target: RestartTarget, paths: &[String]) -> Result<(), String> {
    match target.launch {
        RestartLaunch::CloseOnly => Ok(()),
        RestartLaunch::CodexClient => codex_client::launch(),
        RestartLaunch::Command { command, hidden } => launch_process(command, hidden),
        RestartLaunch::ExistingProcessPath {
            fallback_command,
            hidden,
        } => {
            let mut launched = false;
            for path in paths.iter().filter(|path| !path.trim().is_empty()) {
                launch_process(path, hidden)?;
                launched = true;
            }
            if !launched {
                launch_process(fallback_command, hidden)?;
            }
            Ok(())
        }
        RestartLaunch::MsixPackage { package_identities } => {
            let args = Vec::new();
            package::launch_first_msix_package_with_args(package_identities, &args).map(|_| ())
        }
    }
}

fn stop_restart_target_processes_macos(
    target: RestartTarget,
) -> Result<RestartProcessResult, String> {
    let target_ids = collect_macos_restart_target_pids(target)?;
    if target_ids.is_empty() {
        return Ok(RestartProcessResult {
            total: 0,
            forced: 0,
            remaining: 0,
            paths: Vec::new(),
        });
    }

    let paths = if matches!(target.launch, RestartLaunch::ExistingProcessPath { .. }) {
        target_ids
            .iter()
            .filter_map(|pid| macos_restart_process_executable_path(*pid))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    for name in target.process_names {
        quit_macos_restart_app_by_name(name);
    }
    wait_for_macos_restart_process_exit(&target_ids, Duration::from_secs(8));

    let remaining_after_quit = target_ids
        .iter()
        .copied()
        .filter(|pid| macos_restart_pid_alive(*pid))
        .collect::<Vec<_>>();
    for pid in &remaining_after_quit {
        let _ = hidden_command_with_args("kill", &["-TERM", &pid.to_string()]).output();
    }
    wait_for_macos_restart_process_exit(&remaining_after_quit, Duration::from_secs(2));

    let remaining_after_term = remaining_after_quit
        .iter()
        .copied()
        .filter(|pid| macos_restart_pid_alive(*pid))
        .collect::<Vec<_>>();
    let mut forced = 0;
    for pid in &remaining_after_term {
        let output = hidden_command_with_args("kill", &["-KILL", &pid.to_string()])
            .output()
            .map_err(|err| format!("Failed to force-close {}: {err}", target.label))?;
        if output.status.success() {
            forced += 1;
        }
    }
    wait_for_macos_restart_process_exit(&remaining_after_term, Duration::from_millis(500));

    let remaining = target_ids
        .iter()
        .copied()
        .filter(|pid| macos_restart_pid_alive(*pid))
        .count() as u64;

    Ok(RestartProcessResult {
        total: target_ids.len() as u64,
        forced,
        remaining,
        paths,
    })
}

fn collect_macos_restart_target_pids(target: RestartTarget) -> Result<Vec<u32>, String> {
    let mut ids = BTreeSet::new();
    for name in target.process_names {
        let clean_name = macos_restart_process_name(name);
        if clean_name.is_empty() {
            continue;
        }
        for pid in pgrep_macos_for_restart(&["-x", clean_name.as_str()])? {
            ids.insert(pid);
        }
    }
    for marker in target.command_markers {
        if marker.trim().is_empty() {
            continue;
        }
        for pid in pgrep_macos_for_restart(&["-f", marker])? {
            ids.insert(pid);
        }
    }

    let current_pid = std::process::id();
    Ok(ids
        .into_iter()
        .filter(|pid| *pid != current_pid)
        .filter(|pid| !macos_restart_process_has_any_marker(*pid, target.exclude_command_markers))
        .collect())
}

fn pgrep_macos_for_restart(args: &[&str]) -> Result<Vec<u32>, String> {
    let output = hidden_command_with_args("pgrep", args)
        .output()
        .map_err(|err| format!("Failed to run pgrep: {err}"))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect())
}

fn macos_restart_process_executable_path(pid: u32) -> Option<String> {
    let pid = pid.to_string();
    let output = hidden_command_with_args("ps", &["-p", &pid, "-o", "comm="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

fn macos_restart_process_command_line(pid: u32) -> Option<String> {
    let pid = pid.to_string();
    let output = hidden_command_with_args("ps", &["-p", &pid, "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn macos_restart_process_has_any_marker(pid: u32, markers: &[&str]) -> bool {
    if markers.is_empty() {
        return false;
    }
    let Some(command_line) = macos_restart_process_command_line(pid) else {
        return false;
    };
    let haystack = command_line.to_ascii_lowercase();
    markers
        .iter()
        .map(|marker| marker.to_ascii_lowercase())
        .any(|marker| haystack.contains(&marker))
}

fn quit_macos_restart_app_by_name(name: &str) {
    let clean_name = macos_restart_process_name(name);
    if clean_name.is_empty() {
        return;
    }
    let script = format!("tell application \"{clean_name}\" to quit");
    let _ = hidden_command_with_args("osascript", &["-e", &script]).output();
}

fn wait_for_macos_restart_process_exit(pids: &[u32], timeout: Duration) {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if pids.iter().all(|pid| !macos_restart_pid_alive(*pid)) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn macos_restart_pid_alive(pid: u32) -> bool {
    let pid = pid.to_string();
    hidden_command_with_args("kill", &["-0", &pid])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn macos_restart_process_name(name: &str) -> String {
    name.trim()
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .trim_end_matches(".bat")
        .trim_end_matches(".ps1")
        .to_string()
}

fn launch_process(program: &str, hidden: bool) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        let window_style = if hidden { "Hidden" } else { "Normal" };
        let script = format!(
            "Start-Process -FilePath {program} -WindowStyle {window_style}",
            program = ps_quote(program),
            window_style = window_style
        );
        return run_powershell(&script).map(|_| ());
    }

    if cfg!(target_os = "macos") && !hidden {
        let path = Path::new(program);
        if path.exists() || program == "Claude" {
            let mut command = hidden_command("open");
            if let Some(app_bundle) = path
                .exists()
                .then(|| macos_app_bundle_for_path(path))
                .flatten()
            {
                command.arg(app_bundle);
            } else if path.exists() {
                command.arg(program);
            } else {
                command.args(["-a", program]);
            }
            return command
                .spawn()
                .map(|_| ())
                .map_err(|err| format!("Failed to start {program}: {err}"));
        }
    }

    let resolved = resolve_command(program).unwrap_or_else(|| program.to_string());
    hidden_command(&resolved)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Failed to start {program}: {err}"))
}

fn macos_app_bundle_for_path(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| {
            ancestor
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("app"))
                .unwrap_or(false)
        })
        .map(Path::to_path_buf)
}

fn ps_array(values: &[&str]) -> String {
    if values.is_empty() {
        "@()".to_string()
    } else {
        format!(
            "@({})",
            values
                .iter()
                .map(|value| ps_quote(value))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn profile_is_active(config: &AppConfig, profile: &ProfileDraft) -> bool {
    let active_profiles = match profile.mode {
        ProviderApplyMode::Config => &config.active_profiles_by_mode.config,
        ProviderApplyMode::Gateway => &config.active_profiles_by_mode.gateway,
    };
    let app = canonical_profile_app(&profile.app);
    active_profiles
        .get(&app)
        .or_else(|| {
            if app == "codex" {
                active_profiles.get("codex-app")
            } else {
                None
            }
        })
        .map(|active_id| active_id == &profile.id)
        .unwrap_or(false)
}

fn verify_active_profile(config: &AppConfig, profile: &ProfileDraft) -> bool {
    profile_is_active(config, profile)
}

fn sync_active_profiles_from_native_configs(
    config: &mut AppConfig,
    drafts: &mut Vec<ProfileDraft>,
    paths: &crate::core::app_paths::AppPaths,
) -> Result<bool, String> {
    let mut changed = false;

    let codex_config =
        fs::read_to_string(paths.home_dir.join(".codex").join("config.toml")).unwrap_or_default();
    if let Ok(codex_config) = parse_toml_or_empty(&codex_config, "Codex config") {
        let codex_auth = read_codex_auth_json(paths).ok();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "codex",
            |profile| {
                codex_direct_config_matches_profile(&codex_config, codex_auth.as_ref(), profile)
            },
            || detect_codex_native_profile_with_auth(&codex_config, codex_auth.as_ref()),
        )?;
    }

    changed |= sync_claude_desktop_config_profile(config, drafts, paths)?;

    let claude_config_path = paths.home_dir.join(".claude").join("settings.json");
    let claude_config = fs::read_to_string(claude_config_path).unwrap_or_default();
    if let Ok(claude_config) = parse_json5_or_empty(&claude_config, "Claude settings") {
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "claude",
            |profile| claude_config_matches_profile(&claude_config, profile),
            || detect_claude_native_profile(&claude_config),
        )?;
    }

    let gemini_env_path = paths.home_dir.join(".gemini").join(".env");
    let gemini_env = parse_env_content(&fs::read_to_string(gemini_env_path).unwrap_or_default());
    changed |= sync_or_import_native_config_profile(
        config,
        drafts,
        "gemini",
        |profile| gemini_env_matches_profile(&gemini_env, profile),
        || detect_gemini_native_profile(&gemini_env),
    )?;

    let gemini_code_assist_settings_path = vs_code_user_settings_path(paths);
    let gemini_code_assist_settings =
        fs::read_to_string(gemini_code_assist_settings_path).unwrap_or_default();
    if let Ok(settings) =
        parse_json5_or_empty(&gemini_code_assist_settings, "VS Code user settings")
    {
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "gemini-code-assist",
            |profile| gemini_code_assist_settings_match_profile(&settings, profile),
            || detect_gemini_code_assist_native_profile(&settings),
        )?;
    }

    let opencode_config_path = paths
        .home_dir
        .join(".config")
        .join("opencode")
        .join("opencode.json");
    let opencode_config = fs::read_to_string(opencode_config_path).unwrap_or_default();
    if let Ok(opencode_config) = parse_json5_or_empty(&opencode_config, "OpenCode config") {
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "opencode",
            |profile| opencode_config_matches_profile(&opencode_config, profile),
            || detect_opencode_native_profile(&opencode_config),
        )?;
    }

    let openclaw_config_path = paths.home_dir.join(".openclaw").join("openclaw.json");
    let openclaw_config = fs::read_to_string(openclaw_config_path).unwrap_or_default();
    if let Ok(openclaw_config) = parse_json5_or_empty(&openclaw_config, "OpenClaw config") {
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "openclaw",
            |profile| openclaw_config_matches_profile(&openclaw_config, profile),
            || detect_openclaw_native_profile(&openclaw_config),
        )?;
    }

    let hermes_config_path = paths.home_dir.join(".hermes").join("config.yaml");
    let hermes_config = fs::read_to_string(hermes_config_path).unwrap_or_default();
    if let Ok(hermes_config) = parse_yaml_or_empty(&hermes_config, "Hermes config") {
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "hermes",
            |profile| hermes_config_matches_profile(&hermes_config, profile),
            || detect_hermes_native_profile(&hermes_config),
        )?;
    }

    Ok(changed)
}

#[cfg(test)]
fn sync_codex_config_profile(
    config: &mut AppConfig,
    drafts: &[ProfileDraft],
    codex_config: &toml::Value,
) -> bool {
    sync_native_config_profile(config, drafts, "codex", |profile| {
        codex_direct_config_matches_profile(codex_config, None, profile)
    })
}

fn sync_claude_desktop_config_profile(
    config: &mut AppConfig,
    drafts: &mut Vec<ProfileDraft>,
    paths: &crate::core::app_paths::AppPaths,
) -> Result<bool, String> {
    let desktop_paths = claude_desktop_paths(paths).ok();
    let official = desktop_paths
        .as_ref()
        .map(claude_desktop_is_official)
        .unwrap_or(true);

    sync_or_import_native_config_profile(
        config,
        drafts,
        "claude-desktop",
        |profile| claude_desktop_config_matches_profile(profile, desktop_paths.as_ref(), official),
        || {
            desktop_paths
                .as_ref()
                .and_then(detect_claude_desktop_native_profile)
        },
    )
}

#[cfg(test)]
fn sync_native_config_profile<F>(
    config: &mut AppConfig,
    drafts: &[ProfileDraft],
    app: &str,
    matches_profile: F,
) -> bool
where
    F: Fn(&ProfileDraft) -> bool,
{
    let current_active_id = config.active_profiles_by_mode.config.get(app).cloned();
    let matching_profiles = matching_native_config_profiles(drafts, app, &matches_profile);

    let selected_profile_id = current_active_id
        .as_ref()
        .and_then(|active_id| {
            matching_profiles
                .iter()
                .find(|profile| profile.id == *active_id)
                .map(|profile| profile.id.clone())
        })
        .or_else(|| matching_profiles.first().map(|profile| profile.id.clone()));

    match selected_profile_id {
        Some(profile_id) if config.active_profiles_by_mode.config.get(app) != Some(&profile_id) => {
            config
                .active_profiles_by_mode
                .config
                .insert(app.to_string(), profile_id);
            true
        }
        Some(_) => false,
        None => config.active_profiles_by_mode.config.remove(app).is_some(),
    }
}

fn sync_or_import_native_config_profile<F, G>(
    config: &mut AppConfig,
    drafts: &mut Vec<ProfileDraft>,
    app: &str,
    matches_profile: F,
    detect_profile: G,
) -> Result<bool, String>
where
    F: Fn(&ProfileDraft) -> bool,
    G: FnOnce() -> Option<DetectedNativeProfile>,
{
    let app = canonical_profile_app(app);
    let current_active_id = config.active_profiles_by_mode.config.get(&app).cloned();
    let detected = detect_profile();
    let (selected_profile_id, should_correct_detected_profile) = {
        let matching_profiles = matching_native_config_profiles(drafts, &app, &matches_profile);
        let selected_profile_id = current_active_id
            .as_ref()
            .and_then(|active_id| {
                matching_profiles
                    .iter()
                    .find(|profile| profile.id == *active_id)
                    .map(|profile| profile.id.clone())
            })
            .or_else(|| matching_profiles.first().map(|profile| profile.id.clone()));
        let should_correct_detected_profile = detected
            .as_ref()
            .map(|detected| {
                matching_profiles
                    .iter()
                    .any(|profile| should_correct_detected_native_profile(profile, &app, detected))
            })
            .unwrap_or(false);
        (selected_profile_id, should_correct_detected_profile)
    };

    if should_correct_detected_profile {
        if let Some(detected) = detected {
            let imported = upsert_detected_native_profile(drafts, detected)?;
            let changed = config.active_profiles_by_mode.config.get(&app) != Some(&imported.id);
            config
                .active_profiles_by_mode
                .config
                .insert(app, imported.id);
            return Ok(changed);
        }
    }

    if let Some(profile_id) = selected_profile_id {
        if config.active_profiles_by_mode.config.get(&app) != Some(&profile_id) {
            config
                .active_profiles_by_mode
                .config
                .insert(app, profile_id);
            return Ok(true);
        }
        return Ok(false);
    }

    if let Some(detected) = detected {
        let imported = upsert_detected_native_profile(drafts, detected)?;
        let changed = config.active_profiles_by_mode.config.get(&app) != Some(&imported.id);
        config
            .active_profiles_by_mode
            .config
            .insert(app, imported.id);
        return Ok(changed);
    }

    Ok(config.active_profiles_by_mode.config.remove(&app).is_some())
}

fn matching_native_config_profiles<'a, F>(
    drafts: &'a [ProfileDraft],
    app: &str,
    matches_profile: &F,
) -> Vec<&'a ProfileDraft>
where
    F: Fn(&ProfileDraft) -> bool,
{
    drafts
        .iter()
        .filter(|profile| {
            canonical_profile_app(&profile.app) == app
                && profile.mode == ProviderApplyMode::Config
                && matches_profile(profile)
        })
        .collect()
}

fn should_correct_detected_native_profile(
    profile: &ProfileDraft,
    app: &str,
    detected: &DetectedNativeProfile,
) -> bool {
    if profile.is_builtin
        || canonical_profile_app(&profile.app) != app
        || profile.mode != ProviderApplyMode::Config
        || !is_auto_imported_native_profile(profile)
    {
        return false;
    }

    let provider = normalize_detected_provider(&detected.provider, &detected.base_url);
    if profile.provider == provider {
        return false;
    }
    let Ok(protocol) = normalize_protocol(Some(&detected.protocol)) else {
        return false;
    };
    let Ok(base_url) = validate_base_url(&detected.base_url) else {
        return false;
    };
    let model = native_optional_model(&detected.model).unwrap_or_default();

    profile.protocol == protocol
        && profile.model.trim() == model
        && profile.base_url.trim() == base_url
}

fn upsert_detected_native_profile(
    drafts: &mut Vec<ProfileDraft>,
    detected: DetectedNativeProfile,
) -> Result<ProfileDraft, String> {
    let app = canonical_profile_app(&normalize_token("Tool", &detected.app)?);
    let provider = normalize_detected_provider(&detected.provider, &detected.base_url);
    if provider_is_official(&provider) {
        return Err("Detected Provider cannot be official.".to_string());
    }
    let protocol = normalize_protocol(Some(&detected.protocol))?;
    ensure_profile_protocol_supported_for_mode(
        &app,
        ProviderApplyMode::Config,
        &provider,
        &protocol,
    )?;
    let base_url = validate_base_url(&detected.base_url)?;
    let api_key = detected.api_key.trim();
    if api_key.is_empty() || looks_like_local_gateway_token(api_key) {
        return Err("Detected Provider API key is not importable.".to_string());
    }
    let model = native_optional_model(&detected.model).unwrap_or_default();

    if let Some(existing) = drafts.iter().find(|profile| {
        !profile.is_builtin
            && canonical_profile_app(&profile.app) == app
            && profile.mode == ProviderApplyMode::Config
            && profile.provider == provider
            && profile.protocol == protocol
            && profile.model.trim() == model
            && profile.base_url.trim() == base_url
    }) {
        if let Some(auth_ref) = existing.auth_ref.as_deref() {
            credentials::store_keychain_secret(auth_ref, api_key)?;
            return Ok(existing.clone());
        }
    }

    if let Some(existing_index) = drafts.iter().position(|profile| {
        !profile.is_builtin
            && canonical_profile_app(&profile.app) == app
            && profile.mode == ProviderApplyMode::Config
            && profile.provider != provider
            && profile.protocol == protocol
            && profile.model.trim() == model
            && profile.base_url.trim() == base_url
            && is_auto_imported_native_profile(profile)
    }) {
        let now = Utc::now().to_rfc3339();
        let mut updated = drafts[existing_index].clone();
        let old_provider = updated.provider.clone();
        updated.provider = provider;
        if is_auto_detected_native_profile_name(&updated.name, &app, &old_provider) {
            updated.name = unique_detected_native_profile_name_excluding(
                drafts,
                &app,
                &updated.provider,
                Some(&updated.id),
            );
        }
        if updated.auth_ref.is_none() {
            updated.auth_ref = Some(format!("keychain:codestudio-lite/{}/api_key", updated.id));
        }
        updated.updated_at = Some(now);
        updated.last_test_status = Some("detected".to_string());

        storage::save_profile(&updated)?;
        if let Some(auth_ref) = updated.auth_ref.as_deref() {
            credentials::store_keychain_secret(auth_ref, api_key)?;
        }
        drafts[existing_index] = updated.clone();
        drafts.sort_by(compare_profiles);
        activity_log::append(
            Severity::Info,
            format!(
                "Updated imported native config profile '{}' for {}/{}.",
                updated.name, updated.app, updated.provider
            ),
        )?;

        return Ok(updated);
    }

    let name = unique_detected_native_profile_name(drafts, &app, &provider);
    let id = unique_profile_id(&slugify(&name))?;
    let now = Utc::now().to_rfc3339();
    let auth_ref = Some(format!("keychain:codestudio-lite/{id}/api_key"));
    let sort_order = storage::next_profile_sort_order(&app, &ProviderApplyMode::Config)?;
    let draft = ProfileDraft {
        id,
        name,
        icon: None,
        remark: None,
        app,
        is_builtin: false,
        mode: ProviderApplyMode::Config,
        provider,
        protocol,
        model,
        base_url,
        auth_ref,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        last_test_status: Some("detected".to_string()),
        usage_enabled: false,
        sort_order,
    };

    storage::save_profile(&draft)?;
    if let Some(auth_ref) = draft.auth_ref.as_deref() {
        credentials::store_keychain_secret(auth_ref, api_key)?;
    }
    drafts.push(draft.clone());
    drafts.sort_by(compare_profiles);
    activity_log::append(
        Severity::Info,
        format!(
            "Imported existing native config as profile '{}' for {}/{}.",
            draft.name, draft.app, draft.provider
        ),
    )?;

    Ok(draft)
}

fn unique_detected_native_profile_name(
    drafts: &[ProfileDraft],
    app: &str,
    provider: &str,
) -> String {
    unique_detected_native_profile_name_excluding(drafts, app, provider, None)
}

fn unique_detected_native_profile_name_excluding(
    drafts: &[ProfileDraft],
    app: &str,
    provider: &str,
    exclude_id: Option<&str>,
) -> String {
    let base = format!("{} {}", native_profile_tool_name(app), provider);
    let existing = drafts
        .iter()
        .filter(|profile| exclude_id != Some(profile.id.as_str()))
        .map(|profile| profile.name.as_str())
        .collect::<HashSet<_>>();
    for index in 0..1000 {
        let candidate = if index == 0 {
            base.clone()
        } else {
            format!("{base} {index}")
        };
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
    }
    base
}

fn is_auto_detected_native_profile_name(name: &str, app: &str, provider: &str) -> bool {
    let base = format!("{} {}", native_profile_tool_name(app), provider);
    if name == base {
        return true;
    }
    name.strip_prefix(&(base + " "))
        .map(|suffix| !suffix.is_empty() && suffix.chars().all(|item| item.is_ascii_digit()))
        .unwrap_or(false)
}

fn is_auto_imported_native_profile(profile: &ProfileDraft) -> bool {
    profile.last_test_status.as_deref() == Some("detected")
        || is_auto_detected_native_profile_name(&profile.name, &profile.app, &profile.provider)
}

fn native_profile_tool_name(app: &str) -> &'static str {
    match canonical_profile_app(app).as_str() {
        "codex" => "Codex",
        "claude-desktop" => "Claude Desktop",
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "gemini-code-assist" => "Gemini Code Assist",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        _ => "Tool",
    }
}

fn native_optional_model(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty() && trimmed != "codestudio-default").then(|| trimmed.to_string())
}

fn normalize_detected_provider(provider: &str, base_url: &str) -> String {
    let raw_provider = provider.trim();
    let raw_provider_lower = raw_provider.to_ascii_lowercase();
    let generated_codestudio_label = raw_provider_lower.starts_with("codestudio-")
        || raw_provider_lower.starts_with("codestudio ");
    let from_base_url = provider_slug_from_base_url(base_url);
    if generated_codestudio_label {
        if let Some(provider) = from_base_url.clone() {
            return provider;
        }
    }
    let from_provider = raw_provider
        .strip_prefix("codestudio-")
        .unwrap_or(raw_provider);
    if let Some(provider) = normalize_detected_provider_display_token(from_provider) {
        return provider;
    }
    let mut slug = slugify(from_provider);
    if let Some(stripped) = slug.strip_prefix("codestudio-") {
        slug = stripped.to_string();
    }
    if slug.is_empty()
        || matches!(
            slug.as_str(),
            "official" | "codestudio-local" | "custom" | "provider"
        )
    {
        slug = from_base_url.unwrap_or_else(|| "custom".to_string());
    }
    if slug == "official" || slug == "codestudio-local" {
        "custom".to_string()
    } else {
        slug
    }
}

fn normalize_detected_provider_display_token(provider: &str) -> Option<String> {
    let provider = provider.trim().to_ascii_lowercase();
    if !provider.contains('.')
        || provider.starts_with('.')
        || provider.ends_with('.')
        || !provider.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return None;
    }
    provider_slug_from_base_url(&provider).or(Some(provider))
}

fn provider_slug_from_base_url(base_url: &str) -> Option<String> {
    let host = base_url
        .trim()
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or_default()
        .split('@')
        .next_back()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_matches('.')
        .to_ascii_lowercase();
    let mut parts = host
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    while matches!(parts.first(), Some(&"api" | &"gateway" | &"router")) && parts.len() > 2 {
        parts.remove(0);
    }
    let provider = if parts.len() >= 2 {
        parts[parts.len() - 2..].join(".")
    } else {
        parts.join(".")
    };
    (!provider.is_empty())
        .then(|| provider)
        .filter(|slug| !slug.is_empty())
}

#[cfg(test)]
fn detect_codex_native_profile(value: &toml::Value) -> Option<DetectedNativeProfile> {
    detect_codex_native_profile_with_auth(value, None)
}

fn detect_codex_native_profile_with_auth(
    value: &toml::Value,
    auth: Option<&serde_json::Value>,
) -> Option<DetectedNativeProfile> {
    let provider_id = read_toml_string(value, "model_provider")?;
    if provider_id == "codestudio-local" {
        return None;
    }
    let base_url = toml_lookup(value, &format!("model_providers.{provider_id}.base_url"))
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())?;
    if looks_like_local_gateway_url(base_url) {
        return None;
    }
    let requires_openai_auth = toml_lookup(
        value,
        &format!("model_providers.{provider_id}.requires_openai_auth"),
    )
    .and_then(|item| item.as_bool())
    .unwrap_or(false);
    let auth_api_key = auth
        .filter(|_| requires_openai_auth)
        .and_then(codex_auth_api_key_from_value);
    let api_key = auth_api_key?;
    let wire_api = toml_lookup(value, &format!("model_providers.{provider_id}.wire_api"))
        .and_then(|item| item.as_str())
        .unwrap_or("responses");
    let protocol = protocol_for_codex_wire_api(wire_api)?;
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
        base_url: base_url.to_string(),
        api_key,
    })
}

fn protocol_for_codex_wire_api(value: &str) -> Option<&'static str> {
    match value.trim() {
        "responses" => Some(PROTOCOL_OPENAI_RESPONSES),
        "chat" => Some(PROTOCOL_OPENAI_CHAT_COMPLETIONS),
        _ => None,
    }
}

fn detect_claude_desktop_native_profile(
    paths: &ClaudeDesktopPaths,
) -> Option<DetectedNativeProfile> {
    let content = fs::read_to_string(&paths.profile_path).ok()?;
    let value = parse_json5_or_empty(&content, "Claude Desktop 3P profile").ok()?;
    let base_url = json_string_lookup(&value, &["inferenceGatewayBaseUrl"])
        .map(|value| claude_desktop_direct_profile_base_url(&value))
        .filter(|item| !item.trim().is_empty())?;
    let api_key = json_string_lookup(&value, &["inferenceGatewayApiKey"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;
    if json_string_lookup(&value, &["inferenceProvider"]).as_deref() != Some("gateway") {
        return None;
    }

    Some(DetectedNativeProfile {
        app: "claude-desktop".to_string(),
        provider: provider_slug_from_base_url(&base_url).unwrap_or_else(|| "anthropic".to_string()),
        protocol: PROTOCOL_ANTHROPIC_MESSAGES.to_string(),
        model: claude_desktop_detected_model(&value).unwrap_or_default(),
        base_url,
        api_key,
    })
}

fn claude_desktop_config_matches_profile(
    profile: &ProfileDraft,
    paths: Option<&ClaudeDesktopPaths>,
    official: bool,
) -> bool {
    if canonical_profile_app(&profile.app) != "claude-desktop"
        || profile.mode != ProviderApplyMode::Config
    {
        return false;
    }
    if provider_is_official(&profile.provider) {
        return official;
    }
    if normalize_protocol(Some(&profile.protocol)).as_deref() != Ok(PROTOCOL_ANTHROPIC_MESSAGES) {
        return false;
    }
    let Some(paths) = paths else {
        return false;
    };
    let content = fs::read_to_string(&paths.profile_path).unwrap_or_default();
    let Ok(value) = parse_json5_or_empty(&content, "Claude Desktop 3P profile") else {
        return false;
    };
    let model_matches = match profile_model(profile) {
        Some(model) => claude_desktop_detected_model(&value).as_deref() == Some(model),
        None => claude_desktop_detected_model(&value).is_none(),
    };
    let token_matches = json_string_lookup(&value, &["inferenceGatewayApiKey"])
        .map(|token| profile_config_token_is_present(profile, &token))
        .unwrap_or(false);

    json_string_lookup(&value, &["inferenceProvider"]).as_deref() == Some("gateway")
        && json_string_lookup(&value, &["inferenceGatewayAuthScheme"])
            .map(|scheme| scheme.eq_ignore_ascii_case("bearer"))
            .unwrap_or(true)
        && json_string_lookup(&value, &["inferenceGatewayBaseUrl"])
            .map(|base_url| claude_desktop_direct_profile_base_url(&base_url))
            .as_deref()
            == Some(profile.base_url.trim())
        && token_matches
        && model_matches
}

fn claude_desktop_direct_profile_base_url(value: &str) -> String {
    value.trim().to_string()
}

fn claude_desktop_detected_model(value: &serde_json::Value) -> Option<String> {
    let models = value
        .get("inferenceModels")
        .and_then(serde_json::Value::as_array)?;
    models.first().and_then(|model| {
        model
            .as_str()
            .map(ToString::to_string)
            .or_else(|| json_string_lookup(model, &["labelOverride"]))
            .or_else(|| json_string_lookup(model, &["name"]))
    })
}

fn detect_claude_native_profile(value: &serde_json::Value) -> Option<DetectedNativeProfile> {
    let base_url = json_string_lookup(value, &["env", "ANTHROPIC_BASE_URL"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())?;
    let api_key = json_string_lookup(value, &["env", "ANTHROPIC_AUTH_TOKEN"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;
    let model = json_string_lookup(value, &["model"])
        .or_else(|| json_string_lookup(value, &["env", "ANTHROPIC_MODEL"]))
        .and_then(|model| native_optional_model(&model))
        .unwrap_or_default();

    Some(DetectedNativeProfile {
        app: "claude".to_string(),
        provider: provider_slug_from_base_url(&base_url).unwrap_or_else(|| "anthropic".to_string()),
        protocol: PROTOCOL_ANTHROPIC_MESSAGES.to_string(),
        model,
        base_url,
        api_key,
    })
}

fn detect_gemini_native_profile(env: &HashMap<String, String>) -> Option<DetectedNativeProfile> {
    let base_url = env
        .get("GOOGLE_GEMINI_BASE_URL")
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())?;
    let api_key = env
        .get("GEMINI_API_KEY")
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;

    Some(DetectedNativeProfile {
        app: "gemini".to_string(),
        provider: provider_slug_from_base_url(&base_url).unwrap_or_else(|| "gemini".to_string()),
        protocol: PROTOCOL_GOOGLE_GEMINI.to_string(),
        model: env
            .get("GEMINI_MODEL")
            .and_then(|model| native_optional_model(model))
            .unwrap_or_default(),
        base_url,
        api_key,
    })
}

fn detect_gemini_code_assist_native_profile(
    value: &serde_json::Value,
) -> Option<DetectedNativeProfile> {
    let api_key = json_string_lookup(value, &[GEMINI_CODE_ASSIST_API_KEY_SETTING])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;

    Some(DetectedNativeProfile {
        app: "gemini-code-assist".to_string(),
        provider: "gemini".to_string(),
        protocol: PROTOCOL_GOOGLE_GEMINI.to_string(),
        model: String::new(),
        base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        api_key,
    })
}

fn detect_opencode_native_profile(value: &serde_json::Value) -> Option<DetectedNativeProfile> {
    let provider_id = opencode_active_provider_id(value)?;
    if provider_id == "codestudio-local" {
        return None;
    }
    let base_url = json_string_lookup(value, &["provider", &provider_id, "options", "baseURL"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())?;
    if looks_like_local_gateway_url(&base_url) {
        return None;
    }
    let api_key = json_string_lookup(value, &["provider", &provider_id, "options", "apiKey"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;
    let provider = json_string_lookup(value, &["provider", &provider_id, "name"])
        .unwrap_or_else(|| provider_id.clone());

    Some(DetectedNativeProfile {
        app: "opencode".to_string(),
        provider,
        protocol: PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string(),
        model: opencode_model_from_ref(
            json_string_lookup(value, &["model"]).as_deref(),
            &provider_id,
        )
        .unwrap_or_default(),
        base_url,
        api_key,
    })
}

fn opencode_active_provider_id(value: &serde_json::Value) -> Option<String> {
    if let Some(model) = json_string_lookup(value, &["model"]) {
        if let Some((provider, _)) = model.split_once('/') {
            if !provider.trim().is_empty() {
                return Some(provider.trim().to_string());
            }
        }
    }
    json_object_keys(value, &["provider"])
        .into_iter()
        .find(|provider_id| {
            json_string_lookup(value, &["provider", provider_id, "options", "baseURL"]).is_some()
                && json_string_lookup(value, &["provider", provider_id, "options", "apiKey"])
                    .is_some()
        })
}

fn opencode_model_from_ref(value: Option<&str>, provider_id: &str) -> Option<String> {
    let value = value?.trim();
    let prefix = format!("{provider_id}/");
    value
        .strip_prefix(&prefix)
        .and_then(native_optional_model)
        .or_else(|| native_optional_model(value))
}

fn detect_openclaw_native_profile(value: &serde_json::Value) -> Option<DetectedNativeProfile> {
    let provider_id = openclaw_active_provider_id(value)?;
    if provider_id == "codestudio-local" {
        return None;
    }
    let base_url = json_string_lookup(value, &["models", "providers", &provider_id, "baseUrl"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())?;
    if looks_like_local_gateway_url(&base_url) {
        return None;
    }
    let api_key = json_string_lookup(value, &["models", "providers", &provider_id, "apiKey"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;
    let provider = json_string_lookup(value, &["models", "providers", &provider_id, "name"])
        .unwrap_or_else(|| provider_id.clone());

    Some(DetectedNativeProfile {
        app: "openclaw".to_string(),
        provider,
        protocol: PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string(),
        model: opencode_model_from_ref(
            json_string_lookup(value, &["agents", "defaults", "model", "primary"]).as_deref(),
            &provider_id,
        )
        .unwrap_or_default(),
        base_url,
        api_key,
    })
}

fn openclaw_active_provider_id(value: &serde_json::Value) -> Option<String> {
    if let Some(model) = json_string_lookup(value, &["agents", "defaults", "model", "primary"]) {
        if let Some((provider, _)) = model.split_once('/') {
            if !provider.trim().is_empty() {
                return Some(provider.trim().to_string());
            }
        }
    }
    json_object_keys(value, &["models", "providers"])
        .into_iter()
        .find(|provider_id| {
            json_string_lookup(value, &["models", "providers", provider_id, "baseUrl"]).is_some()
                && json_string_lookup(value, &["models", "providers", provider_id, "apiKey"])
                    .is_some()
        })
}

fn detect_hermes_native_profile(value: &serde_norway::Value) -> Option<DetectedNativeProfile> {
    if yaml_string_lookup(value, &["model", "provider"]).as_deref() != Some("custom") {
        return None;
    }
    let base_url = yaml_string_lookup(value, &["model", "base_url"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())?;
    let api_key = yaml_string_lookup(value, &["model", "api_key"])
        .map(|value| value.trim().to_string())
        .filter(|item| !item.is_empty())
        .filter(|item| !looks_like_local_gateway_token(item))?;
    if yaml_string_lookup(value, &["model", "api_mode"])
        .as_deref()
        .map(|mode| mode != "chat_completions")
        .unwrap_or(false)
    {
        return None;
    }

    Some(DetectedNativeProfile {
        app: "hermes".to_string(),
        provider: provider_slug_from_base_url(&base_url).unwrap_or_else(|| "openai".to_string()),
        protocol: PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string(),
        model: yaml_string_lookup(value, &["model", "default"])
            .and_then(|model| native_optional_model(&model))
            .unwrap_or_default(),
        base_url,
        api_key,
    })
}

fn claude_desktop_is_official(paths: &ClaudeDesktopPaths) -> bool {
    let normal_mode = read_json_string_from_file(&paths.normal_config_path, &["deploymentMode"]);
    let threep_mode = read_json_string_from_file(&paths.threep_config_path, &["deploymentMode"]);
    let applied_id = read_json_string_from_file(&paths.meta_path, &["appliedId"]);
    let profile_exists = paths.profile_path.exists();

    normal_mode.as_deref().unwrap_or("1p") == "1p"
        && threep_mode.as_deref().unwrap_or("1p") == "1p"
        && applied_id.as_deref() != Some(CLAUDE_DESKTOP_PROFILE_ID)
        && !profile_exists
}

fn read_json_string_from_file(path: &Path, keys: &[&str]) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| parse_json5_or_empty(&content, "native config").ok())
        .and_then(|value| json_string_lookup(&value, keys))
}

fn codex_direct_config_matches_profile(
    value: &toml::Value,
    _auth: Option<&serde_json::Value>,
    profile: &ProfileDraft,
) -> bool {
    if !is_codex_family_app(&profile.app) || profile.mode != ProviderApplyMode::Config {
        return false;
    }

    if provider_is_official(&profile.provider) {
        return codex_official_config_matches_profile(value, profile);
    }

    let Some(provider_id) = codex_active_provider_id_for_profile(value, profile) else {
        return false;
    };
    let model_matches = if profile.model.trim().is_empty() {
        read_toml_string(value, "model").is_none()
    } else {
        read_toml_string(value, "model").as_deref() == Some(profile.model.trim())
    };
    let Ok(wire_api) = codex_wire_api_for_protocol(&profile.protocol) else {
        return false;
    };

    read_toml_string(value, "model_provider").as_deref() == Some(provider_id.as_str())
        && model_matches
        && toml_lookup(value, &format!("model_providers.{provider_id}.base_url"))
            .and_then(|item| item.as_str())
            == Some(profile.base_url.trim())
        && toml_lookup(value, &format!("model_providers.{provider_id}.wire_api"))
            .and_then(|item| item.as_str())
            == Some(wire_api)
        && toml_lookup(
            value,
            &format!("model_providers.{provider_id}.requires_openai_auth"),
        )
        .and_then(|item| item.as_bool())
        .is_some()
}

fn codex_official_config_matches_profile(value: &toml::Value, profile: &ProfileDraft) -> bool {
    let provider_matches = match read_toml_string(value, "model_provider") {
        Some(provider) => provider == "openai",
        None => true,
    };
    let model_matches = if profile.model.trim().is_empty() {
        true
    } else {
        read_toml_string(value, "model").as_deref() == Some(profile.model.trim())
    };
    let base_url_is_absent = toml_lookup(value, "model_providers.openai.base_url")
        .and_then(|item| item.as_str())
        .map(|base_url| base_url.trim().is_empty())
        .unwrap_or(true);

    provider_matches && model_matches && base_url_is_absent
}

fn codex_active_provider_id_for_profile(
    value: &toml::Value,
    profile: &ProfileDraft,
) -> Option<String> {
    let active_provider = read_toml_string(value, "model_provider")?;
    let managed_provider = codex_provider_id_for_profile(profile);
    if active_provider == managed_provider {
        return Some(active_provider);
    }

    let base_url_matches = toml_lookup(
        value,
        &format!("model_providers.{active_provider}.base_url"),
    )
    .and_then(|item| item.as_str())
    .map(str::trim)
        == Some(profile.base_url.trim());
    let wire_api_matches = codex_wire_api_for_protocol(&profile.protocol)
        .ok()
        .and_then(|wire_api| {
            toml_lookup(
                value,
                &format!("model_providers.{active_provider}.wire_api"),
            )
            .and_then(|item| item.as_str())
            .map(|configured| configured == wire_api)
        })
        .unwrap_or(false);

    if base_url_matches && wire_api_matches {
        Some(active_provider)
    } else {
        None
    }
}

fn claude_config_matches_profile(value: &serde_json::Value, profile: &ProfileDraft) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "claude"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_ANTHROPIC_MESSAGES)
            && !claude_settings_have_managed_endpoint(value);
    }

    if canonical_profile_app(&profile.app) != "claude"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref() != Ok(PROTOCOL_ANTHROPIC_MESSAGES)
    {
        return false;
    }

    let model_matches = match profile_model(profile) {
        Some(model) => {
            json_string_lookup(value, &["model"]).as_deref() == Some(model)
                || json_string_lookup(value, &["env", "ANTHROPIC_MODEL"]).as_deref() == Some(model)
        }
        None => {
            json_string_lookup(value, &["model"]).is_none()
                && json_string_lookup(value, &["env", "ANTHROPIC_MODEL"]).is_none()
        }
    };
    let token_matches = json_string_lookup(value, &["env", "ANTHROPIC_AUTH_TOKEN"])
        .map(|token| profile_config_token_is_present(profile, &token))
        .unwrap_or(false);

    json_string_lookup(value, &["env", "ANTHROPIC_BASE_URL"]).as_deref()
        == Some(profile.base_url.trim())
        && token_matches
        && model_matches
}

fn claude_vscode_plugin_config_matches(value: &serde_json::Value) -> bool {
    json_string_lookup(value, &["primaryApiKey"]).as_deref()
        == Some(CLAUDE_VSCODE_PLUGIN_PRIMARY_API_KEY)
}

fn gemini_env_matches_profile(env: &HashMap<String, String>, profile: &ProfileDraft) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "gemini"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_GOOGLE_GEMINI)
            && !gemini_env_has_managed_endpoint(env);
    }

    if canonical_profile_app(&profile.app) != "gemini"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref() != Ok(PROTOCOL_GOOGLE_GEMINI)
    {
        return false;
    }

    let model_matches = match profile_model(profile) {
        Some(model) => env.get("GEMINI_MODEL").map(String::as_str) == Some(model),
        None => env.get("GEMINI_MODEL").is_none(),
    };
    let token_matches = env
        .get("GEMINI_API_KEY")
        .map(|token| profile_config_token_is_present(profile, token))
        .unwrap_or(false);

    env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str) == Some(profile.base_url.trim())
        && token_matches
        && model_matches
}

fn gemini_code_assist_settings_match_profile(
    value: &serde_json::Value,
    profile: &ProfileDraft,
) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "gemini-code-assist"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_GOOGLE_GEMINI)
            && !gemini_code_assist_settings_have_managed_endpoint(value);
    }

    if canonical_profile_app(&profile.app) != "gemini-code-assist"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref() != Ok(PROTOCOL_GOOGLE_GEMINI)
    {
        return false;
    }

    json_string_lookup(value, &[GEMINI_CODE_ASSIST_API_KEY_SETTING])
        .map(|token| profile_config_token_is_present(profile, &token))
        .unwrap_or(false)
}

fn opencode_config_matches_profile(value: &serde_json::Value, profile: &ProfileDraft) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "opencode"
            && profile.mode == ProviderApplyMode::Config
            && matches!(
                normalize_protocol(Some(&profile.protocol)).as_deref(),
                Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS) | Ok(PROTOCOL_OPENAI_RESPONSES)
            )
            && !opencode_config_has_managed_provider(value);
    }

    if canonical_profile_app(&profile.app) != "opencode"
        || profile.mode != ProviderApplyMode::Config
        || !matches!(
            normalize_protocol(Some(&profile.protocol)).as_deref(),
            Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS) | Ok(PROTOCOL_OPENAI_RESPONSES)
        )
    {
        return false;
    }

    let provider_id = custom_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => json_string_lookup(value, &["model"]).as_deref() == Some(model),
        None => json_string_lookup(value, &["model"]).is_none(),
    };
    let token_matches = json_string_lookup(value, &["provider", &provider_id, "options", "apiKey"])
        .map(|token| profile_config_token_is_present(profile, &token))
        .unwrap_or(false);

    json_string_lookup(value, &["provider", &provider_id, "options", "baseURL"]).as_deref()
        == Some(profile.base_url.trim())
        && token_matches
        && model_matches
}

fn openclaw_config_matches_profile(value: &serde_json::Value, profile: &ProfileDraft) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "openclaw"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
            && !openclaw_config_has_managed_provider(value);
    }

    if canonical_profile_app(&profile.app) != "openclaw"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref()
            != Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
    {
        return false;
    }

    let provider_id = custom_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => {
            json_string_lookup(value, &["agents", "defaults", "model", "primary"]).as_deref()
                == Some(model)
        }
        None => true,
    };
    let token_matches = json_string_lookup(value, &["models", "providers", &provider_id, "apiKey"])
        .map(|token| profile_config_token_is_present(profile, &token))
        .unwrap_or(false);

    json_string_lookup(value, &["models", "providers", &provider_id, "baseUrl"]).as_deref()
        == Some(profile.base_url.trim())
        && token_matches
        && model_matches
}

fn hermes_config_matches_profile(value: &serde_norway::Value, profile: &ProfileDraft) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "hermes"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
            && !hermes_config_has_managed_endpoint(value);
    }

    if canonical_profile_app(&profile.app) != "hermes"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref()
            != Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
    {
        return false;
    }

    let model_matches = match profile_model(profile) {
        Some(model) => yaml_string_lookup(value, &["model", "default"]).as_deref() == Some(model),
        None => yaml_string_lookup(value, &["model", "default"]).is_none(),
    };
    let token_matches = yaml_string_lookup(value, &["model", "api_key"])
        .map(|token| profile_config_token_is_present(profile, &token))
        .unwrap_or(false);

    yaml_string_lookup(value, &["model", "provider"]).as_deref() == Some("custom")
        && yaml_string_lookup(value, &["model", "base_url"]).as_deref()
            == Some(profile.base_url.trim())
        && yaml_string_lookup(value, &["model", "api_mode"]).as_deref() == Some("chat_completions")
        && token_matches
        && model_matches
}

fn claude_settings_have_managed_endpoint(value: &serde_json::Value) -> bool {
    json_string_lookup(value, &["env", "ANTHROPIC_BASE_URL"]).is_some()
        || json_string_lookup(value, &["env", "ANTHROPIC_AUTH_TOKEN"]).is_some()
}

fn gemini_env_has_managed_endpoint(env: &HashMap<String, String>) -> bool {
    env.get("GOOGLE_GEMINI_BASE_URL").is_some() || env.get("GEMINI_API_KEY").is_some()
}

fn gemini_code_assist_settings_have_managed_endpoint(value: &serde_json::Value) -> bool {
    json_string_lookup(value, &[GEMINI_CODE_ASSIST_API_KEY_SETTING]).is_some()
}

fn opencode_config_has_managed_provider(value: &serde_json::Value) -> bool {
    json_object_keys(value, &["provider"])
        .into_iter()
        .any(|key| managed_json_provider_key(&key))
}

fn openclaw_config_has_managed_provider(value: &serde_json::Value) -> bool {
    json_object_keys(value, &["models", "providers"])
        .into_iter()
        .any(|key| managed_json_provider_key(&key))
}

fn managed_json_provider_key(key: &str) -> bool {
    key == "custom" || key.starts_with("codestudio-")
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

fn hermes_config_has_managed_endpoint(value: &serde_norway::Value) -> bool {
    yaml_string_lookup(value, &["model", "base_url"]).is_some()
        || yaml_string_lookup(value, &["model", "api_key"]).is_some()
}

fn profile_config_token_is_present(profile: &ProfileDraft, token: &str) -> bool {
    profile
        .auth_ref
        .as_deref()
        .map(str::trim)
        .filter(|auth_ref| !auth_ref.is_empty())
        .is_some()
        && !token.trim().is_empty()
        && !looks_like_local_gateway_token(token)
}

fn profile_api_key_matches_config_by_reading_keychain(profile: &ProfileDraft, token: &str) -> bool {
    let Some(auth_ref) = profile.auth_ref.as_deref() else {
        return false;
    };
    credentials::load_keychain_secret(auth_ref)
        .map(|expected| expected.trim() == token.trim())
        .unwrap_or(false)
}

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

pub fn switch_active_profile(
    request: SwitchActiveProfileRequest,
) -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    let profiles = load_profiles()?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Profile '{profile_id}' does not exist"))?;
    let mode = profile.mode.clone();
    if mode == ProviderApplyMode::Gateway && provider_is_official(&profile.provider) {
        return Err(
            "Official provider uses the client login directly and does not run through the local gateway."
                .to_string(),
        );
    }

    let mut config = read_app_config()?;
    activate_profile_for_tool(&mut config, profile, &profiles);
    write_app_config(&config)?;
    activity_log::append(
        Severity::Ok,
        format!(
            "Switched active profile for '{}' to '{}' in {:?} mode.",
            profile.app, profile.name, mode
        ),
    )?;

    load_profile_summary()
}

fn clean_active_profiles(config: &mut AppConfig, drafts: &[ProfileDraft]) -> bool {
    clean_active_profile_map(
        &mut config.active_profiles_by_mode.config,
        ProviderApplyMode::Config,
        drafts,
    ) | clean_active_profile_map(
        &mut config.active_profiles_by_mode.gateway,
        ProviderApplyMode::Gateway,
        drafts,
    )
}

fn replace_deleted_active_profile_with_official(
    config: &mut AppConfig,
    app: &str,
    profile_id: &str,
) -> bool {
    let canonical_app = canonical_profile_app(app);
    let mut changed = false;

    let config_active = &mut config.active_profiles_by_mode.config;
    let config_keys = config_active.keys().cloned().collect::<Vec<_>>();
    for key in config_keys {
        if config_active.get(&key).map(String::as_str) == Some(profile_id) {
            config_active.remove(&key);
            config_active.insert(
                canonical_profile_app(&key),
                builtin_official_profile_id(&canonical_app),
            );
            changed = true;
        }
    }

    let gateway_active = &mut config.active_profiles_by_mode.gateway;
    let before = gateway_active.len();
    gateway_active.retain(|_, active_profile_id| active_profile_id != profile_id);
    changed |= gateway_active.len() != before;

    changed
}

fn clean_active_profile_map(
    active_profiles: &mut HashMap<String, String>,
    mode: ProviderApplyMode,
    drafts: &[ProfileDraft],
) -> bool {
    let mut changed = false;
    let current_profiles = active_profiles.clone();
    for (app, profile_id) in current_profiles {
        let canonical_app = canonical_profile_app(&app);
        let is_valid = drafts.iter().any(|profile| {
            profile.id == profile_id && profile.app == canonical_app && profile.mode == mode
        });
        if !is_valid {
            active_profiles.remove(&app);
            changed = true;
        } else if app != canonical_app {
            active_profiles.remove(&app);
            active_profiles
                .entry(canonical_app)
                .or_insert_with(|| profile_id.clone());
            changed = true;
        }
    }

    changed
}

fn activate_profile_for_tool(
    config: &mut AppConfig,
    profile: &ProfileDraft,
    drafts: &[ProfileDraft],
) {
    active_profiles_for_mode_mut(&mut config.active_profiles_by_mode, &profile.mode)
        .insert(profile.app.clone(), profile.id.clone());
    clean_active_profiles(config, drafts);
}

fn active_profiles_for_mode_mut<'a>(
    active_profiles: &'a mut ActiveProfilesByMode,
    mode: &ProviderApplyMode,
) -> &'a mut HashMap<String, String> {
    match mode {
        ProviderApplyMode::Config => &mut active_profiles.config,
        ProviderApplyMode::Gateway => &mut active_profiles.gateway,
    }
}

fn default_active_profile_id(
    active_profiles: &HashMap<String, String>,
    drafts: &[ProfileDraft],
) -> Option<String> {
    const PREFERRED_APPS: [&str; 8] = [
        "codex",
        "claude-desktop",
        "claude",
        "gemini",
        "gemini-code-assist",
        "opencode",
        "openclaw",
        "hermes",
    ];

    for app in PREFERRED_APPS {
        if let Some(profile_id) = active_profiles.get(app) {
            if drafts
                .iter()
                .any(|profile| profile.id == *profile_id && profile.app == app)
            {
                return Some(profile_id.clone());
            }
        }
    }

    let mut apps = active_profiles.keys().collect::<Vec<_>>();
    apps.sort();
    apps.into_iter().find_map(|app| {
        let profile_id = active_profiles.get(app)?;
        drafts
            .iter()
            .any(|profile| profile.id == *profile_id && profile.app == *app)
            .then(|| profile_id.clone())
    })
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

fn read_codex_auth_json(
    paths: &crate::core::app_paths::AppPaths,
) -> Result<serde_json::Value, String> {
    let auth_path = paths.home_dir.join(".codex").join("auth.json");
    let content = fs::read_to_string(&auth_path).map_err(|err| {
        format!(
            "Codex auth.json could not be read at {}: {err}",
            display_path(&auth_path)
        )
    })?;
    serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|err| format!("Codex auth.json is not valid JSON: {err}"))
}

fn codex_auth_json_path(paths: &crate::core::app_paths::AppPaths) -> PathBuf {
    paths.home_dir.join(".codex").join("auth.json")
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
    let source_path = codex_auth_json_path(&paths);
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

fn verify_codex_auth_json_write(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    if !is_custom_codex_oauth_profile(profile) {
        return Ok(true);
    }
    let expected = load_codex_oauth_profile_content(profile)?;
    let actual = fs::read_to_string(path).map_err(|err| err.to_string())?;
    Ok(actual == expected)
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
    let mut keys = Vec::new();
    collect_json_key_paths(value, String::new(), &mut keys);

    if keys.iter().any(|key| {
        key.contains("chatgpt")
            || key.contains("refresh_token")
            || key.contains("id_token")
            || key.contains("account_id")
            || key.contains("expires_at")
    }) {
        return CodexAuthMethod::ChatGpt;
    }

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

fn codex_auth_api_key_from_value(value: &serde_json::Value) -> Option<String> {
    ["OPENAI_API_KEY", "openai_api_key", "api_key"]
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

fn load_profiles() -> Result<Vec<ProfileDraft>, String> {
    let mut profiles = builtin_official_profiles();
    let usage_enabled_profile_ids = storage::load_usage_enabled_profile_ids()?;
    for mut profile in storage::load_profiles()? {
        let app = canonical_profile_app(&profile.app);
        if is_builtin_profile_id(&profile.id) {
            continue;
        }
        let mode = normalize_stored_profile_mode(
            &profile.provider,
            Some(provider_apply_mode_value(&profile.mode).to_string()),
        );
        if ensure_custom_official_profile_allowed(&app, &profile.provider, mode).is_err() {
            continue;
        }
        profile.app = app;
        profile.is_builtin = false;
        profile.mode = mode;
        profile.protocol = normalize_protocol(Some(profile.protocol.as_str()))?;
        profile.usage_enabled = usage_enabled_profile_ids.contains(&profile.id);
        profiles.push(profile);
    }

    apply_stored_profile_order(&mut profiles)?;
    profiles.sort_by(compare_profiles);
    Ok(profiles)
}

fn apply_stored_profile_order(profiles: &mut [ProfileDraft]) -> Result<(), String> {
    let groups = profiles
        .iter()
        .map(|profile| (canonical_profile_app(&profile.app), profile.mode))
        .collect::<HashSet<_>>();
    for (app, mode) in groups {
        let order = storage::load_profile_order(&app, &mode)?;
        if order.is_empty() {
            continue;
        }
        let order_by_id = order
            .iter()
            .enumerate()
            .map(|(index, profile_id)| (profile_id.as_str(), index as i64))
            .collect::<HashMap<_, _>>();
        let mut next_unordered_index = order.len() as i64;
        let mut group_indexes = profiles
            .iter()
            .enumerate()
            .filter(|(_, profile)| {
                canonical_profile_app(&profile.app) == app && profile.mode == mode
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        group_indexes.sort_by(|left, right| compare_profiles(&profiles[*left], &profiles[*right]));
        for (index, profile_id) in order.iter().enumerate() {
            if let Some(profile) = profiles.iter_mut().find(|profile| {
                profile.id == *profile_id
                    && canonical_profile_app(&profile.app) == app
                    && profile.mode == mode
            }) {
                profile.sort_order = index as i64;
            }
        }
        for profile_index in group_indexes {
            let profile = &mut profiles[profile_index];
            if order_by_id.contains_key(profile.id.as_str()) {
                continue;
            }
            profile.sort_order = next_unordered_index;
            next_unordered_index += 1;
        }
    }
    Ok(())
}

fn builtin_official_profiles() -> Vec<ProfileDraft> {
    BUILTIN_OFFICIAL_PROFILES
        .iter()
        .map(|(app, name, protocol)| ProfileDraft {
            id: builtin_official_profile_id(app),
            name: (*name).to_string(),
            icon: Some(default_builtin_profile_icon(app).to_string()),
            remark: None,
            app: (*app).to_string(),
            is_builtin: true,
            mode: ProviderApplyMode::Config,
            provider: "official".to_string(),
            protocol: (*protocol).to_string(),
            model: String::new(),
            base_url: String::new(),
            auth_ref: None,
            created_at: None,
            updated_at: None,
            last_test_status: Some("builtin".to_string()),
            usage_enabled: false,
            sort_order: 0,
        })
        .collect()
}

fn builtin_official_profile_id(app: &str) -> String {
    format!("{BUILTIN_OFFICIAL_ID_PREFIX}{}", canonical_profile_app(app))
}

fn default_builtin_profile_icon(app: &str) -> &'static str {
    match canonical_profile_app(app).as_str() {
        "codex" => "C",
        "claude-desktop" => "CD",
        "claude" => "CC",
        "gemini" => "G",
        "gemini-code-assist" => "GA",
        "opencode" => "OC",
        "openclaw" => "O",
        "hermes" => "H",
        _ => "?",
    }
}

fn is_builtin_profile_id(id: &str) -> bool {
    id.starts_with(BUILTIN_OFFICIAL_ID_PREFIX)
}

fn compare_profiles(left: &ProfileDraft, right: &ProfileDraft) -> std::cmp::Ordering {
    left.app
        .cmp(&right.app)
        .then_with(|| {
            provider_apply_mode_value(&left.mode).cmp(provider_apply_mode_value(&right.mode))
        })
        .then_with(|| left.sort_order.cmp(&right.sort_order))
        .then_with(|| left.name.cmp(&right.name))
}

pub(crate) fn load_profile_by_id(profile_id: &str) -> Result<ProfileDraft, String> {
    load_profiles()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Profile '{profile_id}' does not exist"))
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

fn normalize_required(label: &str, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(trimmed.to_string())
    }
}

fn normalize_profile_icon(value: Option<&str>) -> Result<Option<String>, String> {
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

fn normalize_token(label: &str, value: &str) -> Result<String, String> {
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

fn normalize_provider_token(value: &str) -> Result<String, String> {
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

fn validate_base_url(value: &str) -> Result<String, String> {
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

fn validate_base_url_for_provider(provider: &str, value: &str) -> Result<String, String> {
    if provider_is_official(provider) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(String::new());
        }
    }
    validate_base_url(value)
}

fn normalize_protocol(value: Option<&str>) -> Result<String, String> {
    let protocol = value.unwrap_or("").trim();
    match protocol {
        PROTOCOL_OPENAI_CHAT_COMPLETIONS => Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string()),
        PROTOCOL_OPENAI_RESPONSES => Ok(PROTOCOL_OPENAI_RESPONSES.to_string()),
        PROTOCOL_ANTHROPIC_MESSAGES => Ok(PROTOCOL_ANTHROPIC_MESSAGES.to_string()),
        PROTOCOL_GOOGLE_GEMINI => Ok(PROTOCOL_GOOGLE_GEMINI.to_string()),
        _ => Err("Unsupported Provider API protocol.".to_string()),
    }
}

fn protocol_display_name(protocol: &str) -> &'static str {
    match normalize_protocol(Some(protocol)).as_deref() {
        Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS) => "OpenAI Chat Completions",
        Ok(PROTOCOL_OPENAI_RESPONSES) => "OpenAI Responses API",
        Ok(PROTOCOL_ANTHROPIC_MESSAGES) => "Claude Messages API",
        Ok(PROTOCOL_GOOGLE_GEMINI) => "Gemini API",
        _ => "Unknown protocol",
    }
}

fn codex_wire_api_for_protocol(protocol: &str) -> Result<&'static str, String> {
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

fn credential_status(provider: &str, secret_provided: bool) -> Severity {
    if provider_is_official(provider) {
        Severity::Info
    } else if secret_provided {
        Severity::Ok
    } else {
        Severity::Error
    }
}

fn credential_detail(provider: &str, secret_provided: bool) -> String {
    if provider_is_official(provider) {
        "Official login flow does not require an API key in this profile draft.".to_string()
    } else if secret_provided {
        "The Provider API key will be stored in the system keychain when this profile is saved; it is not written to TOML or logs.".to_string()
    } else {
        "Provider API key is required for non-official providers.".to_string()
    }
}

fn provider_is_official(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("official")
}

fn provider_requires_api_key(provider: &str) -> bool {
    !provider_is_official(provider)
}

fn is_custom_codex_official_profile(app: &str, provider: &str, mode: ProviderApplyMode) -> bool {
    is_codex_family_app(app) && provider_is_official(provider) && mode == ProviderApplyMode::Config
}

fn ensure_custom_official_profile_allowed(
    app: &str,
    provider: &str,
    mode: ProviderApplyMode,
) -> Result<(), String> {
    if !provider_is_official(provider) || is_custom_codex_official_profile(app, provider, mode) {
        return Ok(());
    }

    Err("Only Codex OAuth profiles can be saved as custom official profiles.".to_string())
}

fn default_profile_mode(provider: &str) -> ProviderApplyMode {
    if provider_is_official(provider) {
        ProviderApplyMode::Config
    } else {
        ProviderApplyMode::Gateway
    }
}

fn normalize_profile_mode(
    provider: &str,
    requested: Option<&ProviderApplyMode>,
) -> Result<ProviderApplyMode, String> {
    let mode = requested
        .cloned()
        .unwrap_or_else(|| default_profile_mode(provider));
    if provider_is_official(provider) && mode == ProviderApplyMode::Gateway {
        return Err(
            "Official provider uses the client login directly and cannot use Gateway profiles."
                .to_string(),
        );
    }
    Ok(mode)
}

fn normalize_stored_profile_mode(provider: &str, value: Option<String>) -> ProviderApplyMode {
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

fn is_codex_family_app(app: &str) -> bool {
    canonical_profile_app(app) == "codex"
}

fn canonical_profile_app(app: &str) -> String {
    match app.trim().to_ascii_lowercase().as_str() {
        "codex" | "codex-cli" | "codex-app" | "codex-client" | "codex-desktop" | "codex-vscode"
        | "codex-code-vscode" | "codex-vs-code" => "codex".to_string(),
        "claude-desktop" | "claude-app" | "claude-client" => "claude-desktop".to_string(),
        "claude-vscode" | "claude-code-vscode" | "claude-vs-code" => "claude".to_string(),
        "gemini-code-assist" | "gemini-vscode" | "gemini-code-vscode" | "gemini-vs-code" => {
            "gemini-code-assist".to_string()
        }
        "hermes" | "hermes-agent" => "hermes".to_string(),
        other => other.to_string(),
    }
}

fn secret_preview(profile: &ProfileDraft) -> &'static str {
    if profile.auth_ref.is_some() {
        "keychain:****"
    } else if !provider_requires_api_key(&profile.provider) {
        "(no api key required)"
    } else {
        "(missing keychain secret)"
    }
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
    match canonical_profile_app(&profile.app).as_str() {
        "codex" => Ok(paths.home_dir.join(".codex").join("config.toml")),
        "claude-desktop" => Ok(claude_desktop_paths(paths)?.profile_path),
        "claude" => Ok(paths.home_dir.join(".claude").join("settings.json")),
        "gemini" => Ok(paths.home_dir.join(".gemini").join(".env")),
        "gemini-code-assist" => Ok(vs_code_user_settings_path(paths)),
        "opencode" => Ok(paths
            .home_dir
            .join(".config")
            .join("opencode")
            .join("opencode.json")),
        "openclaw" => Ok(paths.home_dir.join(".openclaw").join("openclaw.json")),
        "hermes" => Ok(paths.home_dir.join(".hermes").join("config.yaml")),
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
    if mode == ProviderApplyMode::Gateway {
        return match canonical_profile_app(&profile.app).as_str() {
            "codex" | "claude-desktop" | "claude" | "gemini" | "opencode" | "openclaw"
            | "hermes" => native_config_path_for_profile(profile, paths).map(Some),
            _ => Ok(None),
        };
    }

    if canonical_profile_app(&profile.app) == "claude-desktop" {
        return native_config_path_for_profile(profile, paths).map(Some);
    }

    if provider_is_official(&profile.provider) && !is_codex_family_app(&profile.app) {
        return match canonical_profile_app(&profile.app).as_str() {
            "claude" | "gemini" | "gemini-code-assist" | "opencode" | "openclaw" | "hermes" => {
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

fn claude_desktop_paths(
    paths: &crate::core::app_paths::AppPaths,
) -> Result<ClaudeDesktopPaths, String> {
    if cfg!(target_os = "windows") {
        let local_app_data = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.home_dir.join("AppData").join("Local"));
        let roaming_app_data = env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.home_dir.join("AppData").join("Roaming"));
        let normal_dir = pick_windows_claude_desktop_dir(&local_app_data, false)
            .unwrap_or_else(|| local_app_data.join("Claude"));
        let threep_dir = pick_windows_claude_desktop_dir(&local_app_data, true)
            .unwrap_or_else(|| local_app_data.join("Claude-3p"));
        return Ok(claude_desktop_paths_from_dirs(
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
        let app_support = paths.home_dir.join("Library").join("Application Support");
        let normal_dir = app_support.join("Claude");
        return Ok(claude_desktop_paths_from_dirs(
            normal_dir.clone(),
            app_support.join("Claude-3p"),
            vec![normal_dir.join("developer_settings.json")],
        ));
    }

    Err("Claude Desktop 3P configuration is only supported on Windows and macOS.".to_string())
}

fn pick_windows_claude_desktop_dir(local_app_data: &Path, threep: bool) -> Option<PathBuf> {
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
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                return false;
            };
            name.starts_with("Claude") && name.contains("-3p") == threep
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next()
}

fn claude_desktop_paths_from_dirs(
    normal_dir: PathBuf,
    threep_dir: PathBuf,
    developer_settings_paths: Vec<PathBuf>,
) -> ClaudeDesktopPaths {
    let config_library_path = threep_dir.join(CLAUDE_DESKTOP_CONFIG_LIBRARY_DIR);
    ClaudeDesktopPaths {
        normal_config_path: normal_dir.join(CLAUDE_DESKTOP_CONFIG_FILE),
        threep_config_path: threep_dir.join(CLAUDE_DESKTOP_CONFIG_FILE),
        profile_path: config_library_path.join(format!("{CLAUDE_DESKTOP_PROFILE_ID}.json")),
        meta_path: config_library_path.join("_meta.json"),
        developer_settings_paths: dedupe_paths(developer_settings_paths),
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for path in paths {
        let key = display_path(&path).to_ascii_lowercase();
        if seen.insert(key) {
            deduped.push(path);
        }
    }

    deduped
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
    let content = match mode {
        ProviderApplyMode::Config => match canonical_profile_app(&profile.app).as_str() {
            "codex" => codex_direct_config_content(&current, profile)?,
            "claude" if provider_is_official(&profile.provider) => {
                claude_official_config_content(&current)?
            }
            "gemini" if provider_is_official(&profile.provider) => {
                gemini_official_env_content(&current)
            }
            "gemini-code-assist" if provider_is_official(&profile.provider) => {
                gemini_code_assist_official_settings_content(&current)?
            }
            "opencode" if provider_is_official(&profile.provider) => {
                opencode_official_config_content(&current)?
            }
            "openclaw" if provider_is_official(&profile.provider) => {
                openclaw_official_config_content(&current)?
            }
            "hermes" if provider_is_official(&profile.provider) => {
                hermes_official_config_content(&current)?
            }
            "claude" => claude_config_content(&current, profile)?,
            "gemini" => gemini_env_content(&current, profile)?,
            "gemini-code-assist" => gemini_code_assist_settings_content(&current, profile)?,
            "opencode" => opencode_config_content(&current, profile)?,
            "openclaw" => openclaw_config_content(&current, profile)?,
            "hermes" => hermes_config_content(&current, profile)?,
            _ => {
                return Err(format!(
                    "Config profile adapter is not implemented for tool '{}'.",
                    profile.app
                ))
            }
        },
        ProviderApplyMode::Gateway => match canonical_profile_app(&profile.app).as_str() {
            "codex" => codex_gateway_config_content(&current, profile)?,
            "claude" => claude_gateway_config_content(&current, profile)?,
            "gemini" => gemini_gateway_env_content(&current, profile)?,
            "opencode" => opencode_gateway_config_content(&current, profile)?,
            "openclaw" => openclaw_gateway_config_content(&current, profile)?,
            "hermes" => hermes_gateway_config_content(&current, profile)?,
            _ => {
                return Err(format!(
                    "Gateway profile adapter is not implemented for tool '{}'.",
                    profile.app
                ))
            }
        },
    };

    let mut plans = vec![NativeConfigWritePlan::write(
        path,
        content,
        match canonical_profile_app(&profile.app).as_str() {
            "gemini-code-assist" => NativeConfigWriteKind::GeminiCodeAssistSettings,
            _ => NativeConfigWriteKind::ProfileConfig,
        },
    )];

    if *mode == ProviderApplyMode::Config && is_custom_codex_oauth_profile(profile) {
        plans.push(NativeConfigWritePlan::write(
            paths.home_dir.join(".codex").join("auth.json"),
            load_codex_oauth_profile_content(profile)?,
            NativeConfigWriteKind::CodexAuthJson,
        ));
    }

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

fn build_claude_desktop_apply_plan(
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
    mode: &ProviderApplyMode,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    let desktop_paths = claude_desktop_paths(paths)?;

    if *mode == ProviderApplyMode::Config && provider_is_official(&profile.provider) {
        return build_claude_desktop_restore_official_plan(&desktop_paths);
    }

    let profile_content = match mode {
        ProviderApplyMode::Config => claude_desktop_direct_profile_content(profile)?,
        ProviderApplyMode::Gateway => claude_desktop_gateway_profile_content(profile)?,
    };

    let normal_current = read_file_if_exists(&desktop_paths.normal_config_path)?;
    let threep_current = read_file_if_exists(&desktop_paths.threep_config_path)?;
    let meta_current = read_file_if_exists(&desktop_paths.meta_path)?;

    let mut plans = build_claude_desktop_developer_settings_plans(&desktop_paths)?;
    plans.extend([
        NativeConfigWritePlan::write(
            desktop_paths.normal_config_path,
            claude_desktop_deployment_config_content(&normal_current, "3p", false)?,
            NativeConfigWriteKind::ClaudeDesktopDeploymentConfig,
        ),
        NativeConfigWritePlan::write(
            desktop_paths.threep_config_path,
            claude_desktop_deployment_config_content(&threep_current, "3p", false)?,
            NativeConfigWriteKind::ClaudeDesktopDeploymentConfig,
        ),
        NativeConfigWritePlan::write(
            desktop_paths.profile_path,
            profile_content,
            NativeConfigWriteKind::ClaudeDesktopProfileConfig,
        ),
        NativeConfigWritePlan::write(
            desktop_paths.meta_path,
            claude_desktop_meta_content(&meta_current, true)?,
            NativeConfigWriteKind::ClaudeDesktopMetaConfig,
        ),
    ]);

    Ok(plans)
}

fn build_claude_desktop_restore_official_plan(
    paths: &ClaudeDesktopPaths,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    let normal_current = read_file_if_exists(&paths.normal_config_path)?;
    let threep_current = read_file_if_exists(&paths.threep_config_path)?;
    let meta_current = read_file_if_exists(&paths.meta_path)?;

    Ok(vec![
        NativeConfigWritePlan::write(
            paths.normal_config_path.clone(),
            claude_desktop_deployment_config_content(&normal_current, "1p", false)?,
            NativeConfigWriteKind::ClaudeDesktopDeploymentConfig,
        ),
        NativeConfigWritePlan::write(
            paths.threep_config_path.clone(),
            claude_desktop_deployment_config_content(&threep_current, "1p", true)?,
            NativeConfigWriteKind::ClaudeDesktopDeploymentConfig,
        ),
        NativeConfigWritePlan::delete(
            paths.profile_path.clone(),
            NativeConfigWriteKind::ClaudeDesktopProfileConfig,
        ),
        NativeConfigWritePlan::write(
            paths.meta_path.clone(),
            claude_desktop_meta_content(&meta_current, false)?,
            NativeConfigWriteKind::ClaudeDesktopMetaConfig,
        ),
    ])
}

fn build_claude_desktop_developer_settings_plans(
    paths: &ClaudeDesktopPaths,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    let mut plans = Vec::new();

    for path in &paths.developer_settings_paths {
        let current = read_file_if_exists(path)?;
        if claude_desktop_developer_mode_enabled(&current)? {
            continue;
        }

        plans.push(NativeConfigWritePlan::write(
            path.clone(),
            claude_desktop_developer_settings_content(&current)?,
            NativeConfigWriteKind::ClaudeDesktopDeveloperSettings,
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
        if !verify_claude_desktop_developer_settings(&plan.path)? {
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
    if !matches!(
        app.as_str(),
        "claude" | "gemini" | "opencode" | "openclaw" | "hermes"
    ) {
        return Ok(Vec::new());
    }

    let path = native_config_path_for_profile(profile, paths)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let current = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let content = match app.as_str() {
        "claude" => claude_gateway_cleanup_config_content(&current, &app)?,
        "gemini" => gemini_gateway_cleanup_env_content(&current, &app)?,
        "opencode" => opencode_gateway_cleanup_config_content(&current, &app)?,
        "openclaw" => openclaw_gateway_cleanup_config_content(&current, &app)?,
        "hermes" => hermes_gateway_cleanup_config_content(&current, &app)?,
        _ => unreachable!(),
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

pub(crate) fn write_native_config(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    write_atomic(path, content.as_bytes())
}

fn apply_native_config_write_plan(plan: &NativeConfigWritePlan) -> Result<(), String> {
    if plan.delete {
        return match fs::remove_file(&plan.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!(
                "Could not delete native config at {}: {err}",
                display_path(&plan.path)
            )),
        };
    }

    write_native_config(&plan.path, &plan.content)
}

fn filter_native_write_plans(
    plans: Vec<NativeConfigWritePlan>,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    plans
        .into_iter()
        .filter_map(|plan| match native_write_plan_changes_file(&plan) {
            Ok(true) => Some(Ok(plan)),
            Ok(false) => None,
            Err(err) => Some(Err(err)),
        })
        .collect()
}

fn native_write_plan_changes_file(plan: &NativeConfigWritePlan) -> Result<bool, String> {
    if plan.delete {
        return Ok(plan.path.exists());
    }

    if !plan.path.exists() {
        return Ok(true);
    }

    let current = fs::read(&plan.path).map_err(|err| err.to_string())?;
    Ok(current != plan.content.as_bytes())
}

fn codex_gateway_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| format!("Existing Codex config could not be parsed: {err}"))?;
    normalize_codex_model_providers_table(&mut document);
    remove_codex_legacy_managed_direct_providers(&mut document);
    let provider_id = client.provider_id;
    let provider_name = client.provider_name;
    let model = gateway_config_model_for_profile(profile);

    document["model_provider"] = toml_edit::value(provider_id.clone());
    document["model"] = toml_edit::value(model);
    remove_codex_provider_entry(&mut document, &provider_id);
    document["model_providers"][&provider_id] = toml_edit::Item::Table(toml_edit::Table::new());
    document["model_providers"][&provider_id]["name"] = toml_edit::value(provider_name);
    document["model_providers"][&provider_id]["wire_api"] = toml_edit::value("responses");
    document["model_providers"][&provider_id]["base_url"] = toml_edit::value(client.base_url);
    document["model_providers"][&provider_id]["requires_openai_auth"] = toml_edit::value(false);
    repair_codex_preserved_auth_config(&mut document);

    let updated = document.to_string();
    toml::from_str::<toml::Value>(&updated)
        .map_err(|err| format!("Generated Codex config is invalid: {err}"))?;
    Ok(updated)
}

fn codex_direct_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    if provider_is_official(&profile.provider) {
        return codex_official_config_content(current, profile);
    }
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| format!("Existing Codex config could not be parsed: {err}"))?;
    normalize_codex_model_providers_table(&mut document);
    remove_codex_legacy_managed_direct_providers(&mut document);
    let provider_id = codex_provider_id_for_profile(profile);
    let provider_name = profile.provider.trim();
    let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;
    let model = profile.model.trim();

    document["model_provider"] = toml_edit::value(provider_id.clone());
    if model.is_empty() {
        document.as_table_mut().remove("model");
    } else {
        document["model"] = toml_edit::value(model);
    }
    remove_codex_provider_entry(&mut document, &provider_id);
    document["model_providers"][&provider_id] = toml_edit::Item::Table(toml_edit::Table::new());
    document["model_providers"][&provider_id]["name"] = toml_edit::value(provider_name);
    document["model_providers"][&provider_id]["wire_api"] = toml_edit::value(wire_api);
    document["model_providers"][&provider_id]["base_url"] =
        toml_edit::value(profile.base_url.trim().to_string());
    document["model_providers"][&provider_id]["requires_openai_auth"] = toml_edit::value(false);
    repair_codex_preserved_auth_config(&mut document);

    let updated = document.to_string();
    toml::from_str::<toml::Value>(&updated)
        .map_err(|err| format!("Generated Codex config is invalid: {err}"))?;
    Ok(updated)
}

fn normalize_codex_model_providers_table(document: &mut toml_edit::DocumentMut) {
    let Some(item) = document.as_table_mut().remove("model_providers") else {
        return;
    };
    let table = item.into_table().unwrap_or_else(|item| {
        item.as_table_like()
            .map(table_like_to_table)
            .unwrap_or_default()
    });
    document["model_providers"] = toml_edit::Item::Table(table);
}

fn table_like_to_table(table_like: &dyn toml_edit::TableLike) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    for (key, value) in table_like.iter() {
        table[key] = value.clone();
    }
    table
}

fn remove_codex_legacy_managed_direct_providers(document: &mut toml_edit::DocumentMut) {
    let Some(table) = document
        .get_mut("model_providers")
        .and_then(|item| item.as_table_like_mut())
    else {
        return;
    };
    let legacy_keys = table
        .iter()
        .filter_map(|(key, _)| key.starts_with("codestudio-").then(|| key.to_string()))
        .collect::<Vec<_>>();
    for key in legacy_keys {
        table.remove(&key);
    }
}

fn remove_codex_provider_entry(document: &mut toml_edit::DocumentMut, provider_id: &str) {
    if let Some(table) = document
        .get_mut("model_providers")
        .and_then(|item| item.as_table_like_mut())
    {
        table.remove(provider_id);
    }
}

fn remove_empty_codex_model_providers_table(document: &mut toml_edit::DocumentMut) {
    let should_remove_table = document
        .get("model_providers")
        .and_then(|item| item.as_table_like())
        .map(|table| table.is_empty())
        .unwrap_or(false);
    if should_remove_table {
        document.as_table_mut().remove("model_providers");
    }
}

fn repair_codex_preserved_auth_config(document: &mut toml_edit::DocumentMut) {
    remove_codex_legacy_key_from_table(document, "auth", &["OPENAI_API_KEY", "api_key"]);
    remove_codex_legacy_key_from_table(document, "env", &["OPENAI_API_KEY"]);
}

fn remove_codex_legacy_key_from_table(
    document: &mut toml_edit::DocumentMut,
    table_name: &str,
    keys: &[&str],
) {
    let should_remove_table = {
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

    if should_remove_table {
        document.as_table_mut().remove(table_name);
    }
}

fn codex_official_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| format!("Existing Codex config could not be parsed: {err}"))?;
    let provider_id = "openai";

    normalize_codex_model_providers_table(&mut document);
    document["model_provider"] = toml_edit::value(provider_id);
    if profile.model.trim().is_empty() {
        document.remove("model");
    } else {
        document["model"] = toml_edit::value(profile.model.trim());
    }
    remove_codex_provider_entry(&mut document, provider_id);
    remove_empty_codex_model_providers_table(&mut document);
    repair_codex_preserved_auth_config(&mut document);

    let updated = document.to_string();
    toml::from_str::<toml::Value>(&updated)
        .map_err(|err| format!("Generated Codex config is invalid: {err}"))?;
    Ok(updated)
}

fn claude_desktop_deployment_config_content(
    current: &str,
    mode: &str,
    remove_managed_enterprise_config: bool,
) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude Desktop deployment config")?;
    if !value.is_object() {
        value = serde_json::Value::Object(serde_json::Map::new());
    }

    set_json_string_path(&mut value, &["deploymentMode"], mode);

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
            .map(|enterprise| enterprise.is_empty())
            .unwrap_or(false)
        {
            remove_json_path(&mut value, &["enterpriseConfig"]);
        }
    }

    render_json_config(value, "Claude Desktop deployment config")
}

fn claude_desktop_developer_mode_enabled(current: &str) -> Result<bool, String> {
    let value = parse_json5_or_empty(current, "Claude Desktop developer settings")?;
    Ok(value
        .get("allowDevTools")
        .and_then(serde_json::Value::as_bool)
        == Some(true))
}

fn claude_desktop_developer_settings_content(current: &str) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude Desktop developer settings")?;
    if !value.is_object() {
        value = serde_json::Value::Object(serde_json::Map::new());
    }

    set_json_value_path(
        &mut value,
        &["allowDevTools"],
        serde_json::Value::Bool(true),
    );

    render_json_config(value, "Claude Desktop developer settings")
}

fn claude_desktop_direct_profile_content(profile: &ProfileDraft) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    claude_desktop_direct_profile_content_with_api_key(profile, &api_key)
}

fn claude_desktop_direct_profile_content_with_api_key(
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_ANTHROPIC_MESSAGES])?;
    let model_specs = claude_desktop_direct_inference_models(profile);
    let value = claude_desktop_gateway_profile_value(
        profile.base_url.trim(),
        api_key,
        (!model_specs.is_empty()).then_some(model_specs.as_slice()),
    );
    render_json_config(value, "Claude Desktop 3P profile")
}

fn claude_desktop_gateway_profile_content(profile: &ProfileDraft) -> Result<String, String> {
    let client = gateway::client_config_for_tool("claude-desktop")?;
    let model_specs = claude_desktop_gateway_inference_models(profile);
    let value = claude_desktop_gateway_profile_value(
        &claude_desktop_gateway_profile_base_url(&client.base_url),
        &client.token,
        Some(model_specs.as_slice()),
    );
    render_json_config(value, "Claude Desktop 3P profile")
}

fn claude_desktop_gateway_profile_base_url(client_base_url: &str) -> String {
    client_base_url
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or_else(|| client_base_url.trim_end_matches('/'))
        .to_string()
}

fn claude_desktop_gateway_profile_value(
    base_url: &str,
    api_key: &str,
    model_specs: Option<&[ClaudeDesktopInferenceModelSpec]>,
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
        profile["inferenceModels"] = serde_json::Value::Array(
            model_specs
                .iter()
                .map(claude_desktop_inference_model_json)
                .collect(),
        );
    }

    profile
}

fn claude_desktop_direct_inference_models(
    profile: &ProfileDraft,
) -> Vec<ClaudeDesktopInferenceModelSpec> {
    profile_model(profile)
        .filter(|model| claude_desktop_safe_model_id(model))
        .map(|model| {
            vec![ClaudeDesktopInferenceModelSpec {
                name: model.to_string(),
                label_override: None,
                supports_1m: false,
            }]
        })
        .unwrap_or_default()
}

pub(crate) fn claude_desktop_gateway_inference_models(
    profile: &ProfileDraft,
) -> Vec<ClaudeDesktopInferenceModelSpec> {
    if let Some(model) = profile_model(profile) {
        if claude_desktop_safe_model_id(model) {
            return vec![ClaudeDesktopInferenceModelSpec {
                name: model.to_string(),
                label_override: None,
                supports_1m: true,
            }];
        }

        return vec![ClaudeDesktopInferenceModelSpec {
            name: CLAUDE_DESKTOP_DEFAULT_ROUTE_ID.to_string(),
            label_override: Some(model.to_string()),
            supports_1m: true,
        }];
    }

    claude_desktop_default_gateway_inference_models()
}

pub(crate) fn claude_desktop_default_gateway_inference_models(
) -> Vec<ClaudeDesktopInferenceModelSpec> {
    CLAUDE_DESKTOP_DEFAULT_ROUTES
        .iter()
        .map(|(name, supports_1m)| ClaudeDesktopInferenceModelSpec {
            name: (*name).to_string(),
            label_override: None,
            supports_1m: *supports_1m,
        })
        .collect()
}

pub(crate) fn claude_desktop_safe_model_id(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.contains(CLAUDE_DESKTOP_ONE_M_CONTEXT_MARKER) {
        return false;
    }

    let Some(route_tail) = normalized
        .strip_prefix(CLAUDE_DESKTOP_ANTHROPIC_ROUTE_PREFIX)
        .or_else(|| normalized.strip_prefix(CLAUDE_DESKTOP_ROUTE_PREFIX))
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

fn claude_desktop_inference_model_json(
    spec: &ClaudeDesktopInferenceModelSpec,
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

fn claude_desktop_meta_content(current: &str, applied: bool) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude Desktop config library metadata")?;
    if !value.is_object() {
        value = serde_json::Value::Object(serde_json::Map::new());
    }

    let obj = value
        .as_object_mut()
        .ok_or_else(|| "Claude Desktop metadata must be a JSON object.".to_string())?;
    let mut entries = obj
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    entries.retain(|entry| {
        entry.get("id").and_then(serde_json::Value::as_str) != Some(CLAUDE_DESKTOP_PROFILE_ID)
    });

    if applied {
        entries.push(serde_json::json!({
            "id": CLAUDE_DESKTOP_PROFILE_ID,
            "name": CLAUDE_DESKTOP_PROFILE_NAME
        }));
        obj.insert(
            "appliedId".to_string(),
            serde_json::Value::String(CLAUDE_DESKTOP_PROFILE_ID.to_string()),
        );
    } else if obj.get("appliedId").and_then(serde_json::Value::as_str)
        == Some(CLAUDE_DESKTOP_PROFILE_ID)
    {
        if let Some(next_id) = entries
            .iter()
            .find_map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
        {
            obj.insert(
                "appliedId".to_string(),
                serde_json::Value::String(next_id.to_string()),
            );
        } else {
            obj.remove("appliedId");
        }
    }

    obj.insert("entries".to_string(), serde_json::Value::Array(entries));
    render_json_config(value, "Claude Desktop config library metadata")
}

fn claude_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    claude_config_content_with_api_key(current, profile, &api_key)
}

fn claude_config_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_ANTHROPIC_MESSAGES])?;
    let mut value = parse_json5_or_empty(current, "Claude settings")?;

    set_json_string_path(
        &mut value,
        &["env", "ANTHROPIC_BASE_URL"],
        profile.base_url.trim(),
    );
    set_json_string_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"], api_key);
    if let Some(model) = profile_model(profile) {
        set_json_string_path(&mut value, &["model"], model);
        set_json_string_path(&mut value, &["env", "ANTHROPIC_MODEL"], model);
    } else {
        remove_json_path(&mut value, &["model"]);
        remove_json_path(&mut value, &["env", "ANTHROPIC_MODEL"]);
    }

    render_json_config(value, "Claude settings")
}

fn claude_official_config_content(current: &str) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "Claude settings")?;
    remove_json_path(&mut value, &["env", "ANTHROPIC_BASE_URL"]);
    remove_json_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"]);
    remove_json_path(&mut value, &["model"]);
    remove_json_path(&mut value, &["env", "ANTHROPIC_MODEL"]);
    render_json_config(value, "Claude settings")
}

fn claude_gateway_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "Claude settings")?;
    let model = gateway_config_model_for_profile(profile);

    set_json_string_path(&mut value, &["env", "ANTHROPIC_BASE_URL"], &client.base_url);
    set_json_string_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"], &client.token);
    set_json_string_path(&mut value, &["model"], model);
    set_json_string_path(&mut value, &["env", "ANTHROPIC_MODEL"], model);

    render_json_config(value, "Claude settings")
}

fn claude_gateway_cleanup_config_content(current: &str, tool_id: &str) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_json5_or_empty(current, "Claude settings")?;

    remove_json_string_path_if(&mut value, &["env", "ANTHROPIC_BASE_URL"], &client.base_url);
    if json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"])
        .as_deref()
        .map(looks_like_local_gateway_token)
        .unwrap_or(false)
    {
        remove_json_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"]);
    }
    remove_json_string_path_if(&mut value, &["model"], &client.model);
    remove_json_string_path_if(&mut value, &["model"], GATEWAY_FALLBACK_MODEL);
    remove_json_string_path_if(&mut value, &["env", "ANTHROPIC_MODEL"], &client.model);
    remove_json_string_path_if(
        &mut value,
        &["env", "ANTHROPIC_MODEL"],
        GATEWAY_FALLBACK_MODEL,
    );

    render_json_config(value, "Claude settings")
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

fn gemini_env_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    gemini_env_content_with_api_key(current, profile, &api_key)
}

fn gemini_env_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_GOOGLE_GEMINI])?;
    let mut updates = vec![
        ("GEMINI_API_KEY", Some(api_key.to_string())),
        (
            "GOOGLE_GEMINI_BASE_URL",
            Some(profile.base_url.trim().to_string()),
        ),
    ];
    updates.push((
        "GEMINI_MODEL",
        profile_model(profile).map(ToString::to_string),
    ));

    Ok(update_env_content(current, &updates))
}

fn gemini_official_env_content(current: &str) -> String {
    update_env_content(
        current,
        &[
            ("GEMINI_API_KEY", None),
            ("GOOGLE_GEMINI_BASE_URL", None),
            ("GEMINI_MODEL", None),
        ],
    )
}

fn gemini_gateway_env_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let model = gateway_config_model_for_profile(profile);
    Ok(update_env_content(
        current,
        &[
            ("GEMINI_API_KEY", Some(client.token)),
            ("GOOGLE_GEMINI_BASE_URL", Some(client.base_url)),
            ("GEMINI_MODEL", Some(model.to_string())),
        ],
    ))
}

fn gemini_gateway_cleanup_env_content(current: &str, tool_id: &str) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let env = parse_env_content(current);
    let mut updates = Vec::new();

    if env
        .get("GEMINI_API_KEY")
        .map(String::as_str)
        .map(looks_like_local_gateway_token)
        .unwrap_or(false)
    {
        updates.push(("GEMINI_API_KEY", None));
    }
    if env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str) == Some(client.base_url.as_str()) {
        updates.push(("GOOGLE_GEMINI_BASE_URL", None));
    }
    if env.get("GEMINI_MODEL").map(String::as_str) == Some(client.model.as_str()) {
        updates.push(("GEMINI_MODEL", None));
    }
    if env.get("GEMINI_MODEL").map(String::as_str) == Some(GATEWAY_FALLBACK_MODEL) {
        updates.push(("GEMINI_MODEL", None));
    }

    Ok(update_env_content(current, &updates))
}

fn gemini_code_assist_settings_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    gemini_code_assist_settings_content_with_api_key(current, profile, &api_key)
}

fn gemini_code_assist_settings_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_GOOGLE_GEMINI])?;
    let mut value = parse_json5_or_empty(current, "VS Code user settings")?;

    set_json_string_path(&mut value, &[GEMINI_CODE_ASSIST_API_KEY_SETTING], api_key);

    render_json_config(value, "VS Code user settings")
}

fn gemini_code_assist_official_settings_content(current: &str) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "VS Code user settings")?;
    remove_json_path(&mut value, &[GEMINI_CODE_ASSIST_API_KEY_SETTING]);
    render_json_config(value, "VS Code user settings")
}

fn opencode_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    opencode_config_content_with_api_key(current, profile, &api_key)
}

fn opencode_config_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(
        profile,
        &[PROTOCOL_OPENAI_CHAT_COMPLETIONS, PROTOCOL_OPENAI_RESPONSES],
    )?;
    let mut value = parse_json5_or_empty(current, "OpenCode config")?;
    let provider_id = custom_provider_id_for_profile(profile);
    let provider_name = profile.provider.trim();
    remove_json_managed_provider_entries(&mut value, &["provider"]);

    set_json_string_path(&mut value, &["$schema"], "https://opencode.ai/config.json");
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "npm"],
        "@ai-sdk/openai-compatible",
    );
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "name"],
        &provider_name,
    );
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "options", "baseURL"],
        profile.base_url.trim(),
    );
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "options", "apiKey"],
        api_key,
    );

    if let Some(model) = profile_model(profile) {
        set_json_string_path(&mut value, &["model"], &format!("{provider_id}/{model}"));
        set_json_string_path(
            &mut value,
            &["provider", &provider_id, "models", model, "name"],
            model,
        );
    } else {
        remove_json_path(&mut value, &["model"]);
    }

    render_json_config(value, "OpenCode config")
}

fn opencode_official_config_content(current: &str) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "OpenCode config")?;
    for provider_id in json_object_keys(&value, &["provider"])
        .into_iter()
        .filter(|provider_id| managed_json_provider_key(provider_id))
        .collect::<Vec<_>>()
    {
        let model_prefix = format!("{provider_id}/");
        if json_string_lookup(&value, &["model"])
            .as_deref()
            .map(|model| model.starts_with(&model_prefix))
            .unwrap_or(false)
        {
            remove_json_path(&mut value, &["model"]);
        }
        remove_json_path(&mut value, &["provider", &provider_id]);
    }
    render_json_config(value, "OpenCode config")
}

fn opencode_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "OpenCode config")?;
    let provider_id = client.provider_id;
    let model = gateway_config_model_for_profile(profile);

    set_json_string_path(&mut value, &["$schema"], "https://opencode.ai/config.json");
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "npm"],
        "@ai-sdk/openai-compatible",
    );
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "name"],
        &client.provider_name,
    );
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "options", "baseURL"],
        &client.base_url,
    );
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "options", "apiKey"],
        &client.token,
    );
    set_json_string_path(&mut value, &["model"], &format!("{provider_id}/{model}"));
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "models", model, "name"],
        model,
    );

    render_json_config(value, "OpenCode config")
}

fn opencode_gateway_cleanup_config_content(current: &str, tool_id: &str) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_json5_or_empty(current, "OpenCode config")?;
    let provider_id = client.provider_id;
    let model_ref = format!("{provider_id}/{}", client.model);
    let fallback_model_ref = format!("{provider_id}/{GATEWAY_FALLBACK_MODEL}");

    remove_json_string_path_if(&mut value, &["model"], &model_ref);
    remove_json_string_path_if(&mut value, &["model"], &fallback_model_ref);
    remove_json_path(&mut value, &["provider", &provider_id]);

    render_json_config(value, "OpenCode config")
}

fn openclaw_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    openclaw_config_content_with_api_key(current, profile, &api_key)
}

fn openclaw_config_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_OPENAI_CHAT_COMPLETIONS])?;
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    let provider_id = custom_provider_id_for_profile(profile);
    let provider_name = profile.provider.trim();
    remove_json_managed_provider_entries(&mut value, &["models", "providers"]);

    set_json_string_path(&mut value, &["models", "mode"], "merge");
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "name"],
        &provider_name,
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "api"],
        "openai-completions",
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "baseUrl"],
        profile.base_url.trim(),
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "apiKey"],
        api_key,
    );

    if let Some(model) = profile_model(profile) {
        set_json_string_path(
            &mut value,
            &["agents", "defaults", "model", "primary"],
            &format!("{provider_id}/{model}"),
        );
        set_json_value_path(
            &mut value,
            &["models", "providers", &provider_id, "models"],
            serde_json::json!([
                {
                    "id": model,
                    "name": model,
                    "input": ["text"],
                    "output": ["text"]
                }
            ]),
        );
    }

    render_json_config(value, "OpenClaw config")
}

fn openclaw_official_config_content(current: &str) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    for provider_id in json_object_keys(&value, &["models", "providers"])
        .into_iter()
        .filter(|provider_id| managed_json_provider_key(provider_id))
        .collect::<Vec<_>>()
    {
        let model_prefix = format!("{provider_id}/");
        if json_string_lookup(&value, &["agents", "defaults", "model", "primary"])
            .as_deref()
            .map(|model| model.starts_with(&model_prefix))
            .unwrap_or(false)
        {
            remove_json_path(&mut value, &["agents", "defaults", "model", "primary"]);
        }
        remove_json_path(&mut value, &["models", "providers", &provider_id]);
    }
    render_json_config(value, "OpenClaw config")
}

fn openclaw_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    let provider_id = client.provider_id;
    let model = gateway_config_model_for_profile(profile);

    set_json_string_path(&mut value, &["models", "mode"], "merge");
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "name"],
        &client.provider_name,
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "api"],
        "openai-completions",
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "baseUrl"],
        &client.base_url,
    );
    set_json_string_path(
        &mut value,
        &["models", "providers", &provider_id, "apiKey"],
        &client.token,
    );
    set_json_string_path(
        &mut value,
        &["agents", "defaults", "model", "primary"],
        &format!("{provider_id}/{model}"),
    );
    set_json_value_path(
        &mut value,
        &["models", "providers", &provider_id, "models"],
        serde_json::json!([
            {
                "id": model,
                "name": model,
                "input": ["text"],
                "output": ["text"]
            }
        ]),
    );

    render_json_config(value, "OpenClaw config")
}

fn openclaw_gateway_cleanup_config_content(current: &str, tool_id: &str) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    let provider_id = client.provider_id;
    let model_ref = format!("{provider_id}/{}", client.model);
    let fallback_model_ref = format!("{provider_id}/{GATEWAY_FALLBACK_MODEL}");

    remove_json_string_path_if(
        &mut value,
        &["agents", "defaults", "model", "primary"],
        &model_ref,
    );
    remove_json_string_path_if(
        &mut value,
        &["agents", "defaults", "model", "primary"],
        &fallback_model_ref,
    );
    remove_json_path(&mut value, &["models", "providers", &provider_id]);

    render_json_config(value, "OpenClaw config")
}

fn hermes_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    hermes_config_content_with_api_key(current, profile, &api_key)
}

fn hermes_config_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_OPENAI_CHAT_COMPLETIONS])?;
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;

    set_yaml_string_path(&mut value, &["model", "provider"], "custom");
    set_yaml_string_path(&mut value, &["model", "base_url"], profile.base_url.trim());
    set_yaml_string_path(&mut value, &["model", "api_key"], api_key);
    set_yaml_string_path(&mut value, &["model", "api_mode"], "chat_completions");
    if let Some(model) = profile_model(profile) {
        set_yaml_string_path(&mut value, &["model", "default"], model);
    } else {
        remove_yaml_path(&mut value, &["model", "default"]);
    }

    render_yaml_config(value, "Hermes config")
}

fn hermes_official_config_content(current: &str) -> Result<String, String> {
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;
    remove_yaml_string_path_if(&mut value, &["model", "provider"], "custom");
    remove_yaml_path(&mut value, &["model", "base_url"]);
    remove_yaml_path(&mut value, &["model", "api_key"]);
    remove_yaml_path(&mut value, &["model", "api_mode"]);
    remove_yaml_path(&mut value, &["model", "default"]);
    render_yaml_config(value, "Hermes config")
}

fn hermes_gateway_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;
    let model = gateway_config_model_for_profile(profile);

    set_yaml_string_path(&mut value, &["model", "provider"], "custom");
    set_yaml_string_path(&mut value, &["model", "base_url"], &client.base_url);
    set_yaml_string_path(&mut value, &["model", "api_key"], &client.token);
    set_yaml_string_path(&mut value, &["model", "api_mode"], "chat_completions");
    set_yaml_string_path(&mut value, &["model", "default"], model);

    render_yaml_config(value, "Hermes config")
}

fn hermes_gateway_cleanup_config_content(current: &str, tool_id: &str) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;

    remove_yaml_string_path_if(&mut value, &["model", "base_url"], &client.base_url);
    if yaml_string_lookup(&value, &["model", "api_key"])
        .as_deref()
        .map(looks_like_local_gateway_token)
        .unwrap_or(false)
    {
        remove_yaml_path(&mut value, &["model", "api_key"]);
    }
    remove_yaml_string_path_if(&mut value, &["model", "api_mode"], "chat_completions");
    remove_yaml_string_path_if(&mut value, &["model", "default"], &client.model);
    remove_yaml_string_path_if(&mut value, &["model", "default"], GATEWAY_FALLBACK_MODEL);
    if yaml_string_lookup(&value, &["model", "base_url"]).is_none()
        && yaml_string_lookup(&value, &["model", "api_key"]).is_none()
    {
        remove_yaml_string_path_if(&mut value, &["model", "provider"], "custom");
    }

    render_yaml_config(value, "Hermes config")
}

fn verify_claude_desktop_deployment_config(
    path: &Path,
    profile: &ProfileDraft,
    mode: &ProviderApplyMode,
) -> Result<bool, String> {
    let expected_mode =
        if *mode == ProviderApplyMode::Config && provider_is_official(&profile.provider) {
            "1p"
        } else {
            "3p"
        };
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Claude Desktop deployment config")?;

    Ok(json_string_lookup(&value, &["deploymentMode"]).as_deref() == Some(expected_mode))
}

fn verify_claude_desktop_developer_settings(path: &Path) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    claude_desktop_developer_mode_enabled(&content)
}

fn verify_claude_desktop_profile_config(
    path: &Path,
    profile: &ProfileDraft,
    mode: &ProviderApplyMode,
) -> Result<bool, String> {
    if *mode == ProviderApplyMode::Config && provider_is_official(&profile.provider) {
        return Ok(!path.exists());
    }

    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Claude Desktop 3P profile")?;
    let (expected_base_url, expected_api_key) = match mode {
        ProviderApplyMode::Config => (
            profile.base_url.trim().to_string(),
            load_provider_api_key_for_direct_config(profile)?,
        ),
        ProviderApplyMode::Gateway => {
            let client = gateway::client_config_for_tool("claude-desktop")?;
            (
                claude_desktop_gateway_profile_base_url(&client.base_url),
                client.token,
            )
        }
    };

    Ok(
        json_string_lookup(&value, &["inferenceProvider"]).as_deref() == Some("gateway")
            && json_string_lookup(&value, &["inferenceGatewayAuthScheme"]).as_deref()
                == Some("bearer")
            && json_string_lookup(&value, &["inferenceGatewayBaseUrl"]).as_deref()
                == Some(expected_base_url.as_str())
            && json_string_lookup(&value, &["inferenceGatewayApiKey"]).as_deref()
                == Some(expected_api_key.as_str()),
    )
}

fn verify_claude_desktop_meta_config(
    path: &Path,
    profile: &ProfileDraft,
    mode: &ProviderApplyMode,
) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Claude Desktop config library metadata")?;
    let applied = !(*mode == ProviderApplyMode::Config && provider_is_official(&profile.provider));
    let has_entry = value
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries.iter().any(|entry| {
                entry.get("id").and_then(serde_json::Value::as_str)
                    == Some(CLAUDE_DESKTOP_PROFILE_ID)
            })
        })
        .unwrap_or(false);
    let applied_id = json_string_lookup(&value, &["appliedId"]);

    if applied {
        Ok(has_entry && applied_id.as_deref() == Some(CLAUDE_DESKTOP_PROFILE_ID))
    } else {
        Ok(!has_entry && applied_id.as_deref() != Some(CLAUDE_DESKTOP_PROFILE_ID))
    }
}

fn verify_codex_native_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value: toml::Value = toml::from_str(&content).map_err(|err| err.to_string())?;
    let provider_id = client.provider_id;
    let model = gateway_config_model_for_profile(profile);

    Ok(
        read_toml_string(&value, "model_provider").as_deref() == Some(provider_id.as_str())
            && read_toml_string(&value, "model").as_deref() == Some(model)
            && toml_lookup(&value, &format!("model_providers.{provider_id}.base_url"))
                .map(redacted_toml_value)
                .as_deref()
                == Some(client.base_url.as_str())
            && toml_lookup(
                &value,
                &format!("model_providers.{provider_id}.requires_openai_auth"),
            )
            .and_then(|item| item.as_bool())
                == Some(false),
    )
}

fn verify_codex_direct_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value: toml::Value = toml::from_str(&content).map_err(|err| err.to_string())?;
    if provider_is_official(&profile.provider) {
        let model_matches = if profile.model.trim().is_empty() {
            read_toml_string(&value, "model").is_none()
        } else {
            read_toml_string(&value, "model").as_deref() == Some(profile.model.trim())
        };

        return Ok(
            read_toml_string(&value, "model_provider").as_deref() == Some("openai")
                && model_matches
                && toml_lookup(&value, "model_providers.openai").is_none(),
        );
    }

    let provider_id = codex_provider_id_for_profile(profile);
    let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;
    let model_matches = if profile.model.trim().is_empty() {
        read_toml_string(&value, "model").is_none()
    } else {
        read_toml_string(&value, "model").as_deref() == Some(profile.model.trim())
    };

    Ok(
        read_toml_string(&value, "model_provider").as_deref() == Some(provider_id.as_str())
            && model_matches
            && toml_lookup(&value, &format!("model_providers.{provider_id}.wire_api"))
                .and_then(|item| item.as_str())
                == Some(wire_api)
            && toml_lookup(&value, &format!("model_providers.{provider_id}.base_url"))
                .map(redacted_toml_value)
                .as_deref()
                == Some(profile.base_url.trim())
            && toml_lookup(
                &value,
                &format!("model_providers.{provider_id}.requires_openai_auth"),
            )
            .and_then(|item| item.as_bool())
                == Some(false),
    )
}

fn verify_claude_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Claude settings")?;
    if provider_is_official(&profile.provider) {
        return Ok(claude_config_matches_profile(&value, profile));
    }
    let model_matches = match profile_model(profile) {
        Some(model) => {
            json_string_lookup(&value, &["model"]).as_deref() == Some(model)
                || json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).as_deref() == Some(model)
        }
        None => {
            json_string_lookup(&value, &["model"]).is_none()
                && json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).is_none()
        }
    };
    let token_matches = json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"])
        .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, &token))
        .unwrap_or(false);

    Ok(
        json_string_lookup(&value, &["env", "ANTHROPIC_BASE_URL"]).as_deref()
            == Some(profile.base_url.trim())
            && token_matches
            && model_matches,
    )
}

fn verify_claude_gateway_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Claude settings")?;
    let model = gateway_config_model_for_profile(profile);

    Ok(
        json_string_lookup(&value, &["env", "ANTHROPIC_BASE_URL"]).as_deref()
            == Some(client.base_url.as_str())
            && json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"]).as_deref()
                == Some(client.token.as_str())
            && (json_string_lookup(&value, &["model"]).as_deref() == Some(model)
                || json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).as_deref()
                    == Some(model)),
    )
}

fn verify_gemini_env_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let env = parse_env_content(&content);
    if provider_is_official(&profile.provider) {
        return Ok(gemini_env_matches_profile(&env, profile));
    }
    let model_matches = match profile_model(profile) {
        Some(model) => env.get("GEMINI_MODEL").map(String::as_str) == Some(model),
        None => env.get("GEMINI_MODEL").is_none(),
    };
    let token_matches = env
        .get("GEMINI_API_KEY")
        .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, token))
        .unwrap_or(false);

    Ok(
        env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str) == Some(profile.base_url.trim())
            && token_matches
            && model_matches,
    )
}

fn verify_gemini_gateway_env_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let env = parse_env_content(&content);
    let model = gateway_config_model_for_profile(profile);

    Ok(
        env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str) == Some(client.base_url.as_str())
            && env.get("GEMINI_API_KEY").map(String::as_str) == Some(client.token.as_str())
            && env.get("GEMINI_MODEL").map(String::as_str) == Some(model),
    )
}

fn verify_claude_vscode_plugin_config(path: &Path) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Claude VS Code plugin config")?;
    Ok(claude_vscode_plugin_config_matches(&value))
}

fn verify_gemini_code_assist_settings(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "VS Code user settings")?;
    Ok(gemini_code_assist_settings_match_profile(&value, profile))
}

fn verify_opencode_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "OpenCode config")?;
    if provider_is_official(&profile.provider) {
        return Ok(opencode_config_matches_profile(&value, profile));
    }
    let provider_id = custom_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => json_string_lookup(&value, &["model"]).as_deref() == Some(model),
        None => json_string_lookup(&value, &["model"]).is_none(),
    };
    let token_matches =
        json_string_lookup(&value, &["provider", &provider_id, "options", "apiKey"])
            .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, &token))
            .unwrap_or(false);

    Ok(
        json_string_lookup(&value, &["provider", &provider_id, "options", "baseURL"]).as_deref()
            == Some(profile.base_url.trim())
            && token_matches
            && model_matches,
    )
}

fn verify_opencode_gateway_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "OpenCode config")?;
    let provider_id = client.provider_id;
    let expected_model = format!(
        "{provider_id}/{}",
        gateway_config_model_for_profile(profile)
    );

    Ok(
        json_string_lookup(&value, &["provider", &provider_id, "options", "baseURL"]).as_deref()
            == Some(client.base_url.as_str())
            && json_string_lookup(&value, &["provider", &provider_id, "options", "apiKey"])
                .as_deref()
                == Some(client.token.as_str())
            && json_string_lookup(&value, &["model"]).as_deref() == Some(expected_model.as_str()),
    )
}

fn verify_openclaw_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "OpenClaw config")?;
    if provider_is_official(&profile.provider) {
        return Ok(openclaw_config_matches_profile(&value, profile));
    }
    let provider_id = custom_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => {
            json_string_lookup(&value, &["agents", "defaults", "model", "primary"]).as_deref()
                == Some(model)
        }
        None => true,
    };
    let token_matches =
        json_string_lookup(&value, &["models", "providers", &provider_id, "apiKey"])
            .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, &token))
            .unwrap_or(false);

    Ok(
        json_string_lookup(&value, &["models", "providers", &provider_id, "baseUrl"]).as_deref()
            == Some(profile.base_url.trim())
            && token_matches
            && model_matches,
    )
}

fn verify_openclaw_gateway_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "OpenClaw config")?;
    let provider_id = client.provider_id;
    let expected_model = format!(
        "{provider_id}/{}",
        gateway_config_model_for_profile(profile)
    );

    Ok(
        json_string_lookup(&value, &["models", "providers", &provider_id, "baseUrl"]).as_deref()
            == Some(client.base_url.as_str())
            && json_string_lookup(&value, &["models", "providers", &provider_id, "apiKey"])
                .as_deref()
                == Some(client.token.as_str())
            && json_string_lookup(&value, &["agents", "defaults", "model", "primary"]).as_deref()
                == Some(expected_model.as_str()),
    )
}

fn verify_hermes_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_yaml_or_empty(&content, "Hermes config")?;
    if provider_is_official(&profile.provider) {
        return Ok(hermes_config_matches_profile(&value, profile));
    }
    let model_matches = match profile_model(profile) {
        Some(model) => yaml_string_lookup(&value, &["model", "default"]).as_deref() == Some(model),
        None => yaml_string_lookup(&value, &["model", "default"]).is_none(),
    };
    let token_matches = yaml_string_lookup(&value, &["model", "api_key"])
        .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, &token))
        .unwrap_or(false);

    Ok(
        yaml_string_lookup(&value, &["model", "provider"]).as_deref() == Some("custom")
            && yaml_string_lookup(&value, &["model", "base_url"]).as_deref()
                == Some(profile.base_url.trim())
            && yaml_string_lookup(&value, &["model", "api_mode"]).as_deref()
                == Some("chat_completions")
            && token_matches
            && model_matches,
    )
}

fn verify_hermes_gateway_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_yaml_or_empty(&content, "Hermes config")?;
    let model = gateway_config_model_for_profile(profile);

    Ok(
        yaml_string_lookup(&value, &["model", "provider"]).as_deref() == Some("custom")
            && yaml_string_lookup(&value, &["model", "base_url"]).as_deref()
                == Some(client.base_url.as_str())
            && yaml_string_lookup(&value, &["model", "api_key"]).as_deref()
                == Some(client.token.as_str())
            && yaml_string_lookup(&value, &["model", "api_mode"]).as_deref()
                == Some("chat_completions")
            && yaml_string_lookup(&value, &["model", "default"]).as_deref() == Some(model),
    )
}

fn verify_native_config(
    path: &Path,
    profile: &ProfileDraft,
    mode: &ProviderApplyMode,
) -> Result<bool, String> {
    match (mode, canonical_profile_app(&profile.app).as_str()) {
        (ProviderApplyMode::Config, "codex") => verify_codex_direct_config(path, profile),
        (ProviderApplyMode::Config, "claude") => verify_claude_config(path, profile),
        (ProviderApplyMode::Config, "gemini") => verify_gemini_env_config(path, profile),
        (ProviderApplyMode::Config, "gemini-code-assist") => {
            verify_gemini_code_assist_settings(path, profile)
        }
        (ProviderApplyMode::Config, "opencode") => verify_opencode_config(path, profile),
        (ProviderApplyMode::Config, "openclaw") => verify_openclaw_config(path, profile),
        (ProviderApplyMode::Config, "hermes") => verify_hermes_config(path, profile),
        (ProviderApplyMode::Gateway, "codex") => verify_codex_native_config(path, profile),
        (ProviderApplyMode::Gateway, "claude") => verify_claude_gateway_config(path, profile),
        (ProviderApplyMode::Gateway, "gemini") => verify_gemini_gateway_env_config(path, profile),
        (ProviderApplyMode::Gateway, "opencode") => verify_opencode_gateway_config(path, profile),
        (ProviderApplyMode::Gateway, "openclaw") => verify_openclaw_gateway_config(path, profile),
        (ProviderApplyMode::Gateway, "hermes") => verify_hermes_gateway_config(path, profile),
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
        NativeConfigWriteKind::CodexAuthJson => verify_codex_auth_json_write(&plan.path, profile),
        NativeConfigWriteKind::ClaudeVsCodePluginConfig => {
            verify_claude_vscode_plugin_config(&plan.path)
        }
        NativeConfigWriteKind::GeminiCodeAssistSettings => {
            verify_gemini_code_assist_settings(&plan.path, profile)
        }
        NativeConfigWriteKind::ClaudeDesktopDeploymentConfig => {
            verify_claude_desktop_deployment_config(&plan.path, profile, mode)
        }
        NativeConfigWriteKind::ClaudeDesktopProfileConfig => {
            verify_claude_desktop_profile_config(&plan.path, profile, mode)
        }
        NativeConfigWriteKind::ClaudeDesktopMetaConfig => {
            verify_claude_desktop_meta_config(&plan.path, profile, mode)
        }
        NativeConfigWriteKind::ClaudeDesktopDeveloperSettings => {
            verify_claude_desktop_developer_settings(&plan.path)
        }
    }
}

fn codex_provider_id_for_profile(profile: &ProfileDraft) -> String {
    if is_codex_family_app(&profile.app) && profile.mode == ProviderApplyMode::Config {
        return "custom".to_string();
    }
    custom_provider_id_for_profile(profile)
}

fn custom_provider_id_for_profile(_profile: &ProfileDraft) -> String {
    "custom".to_string()
}

fn gateway_config_model_for_profile(profile: &ProfileDraft) -> &str {
    profile_model(profile).unwrap_or(GATEWAY_FALLBACK_MODEL)
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
    if let Ok(api_key) = load_provider_api_key_for_direct_config(profile) {
        output = replace_nonempty(&output, &api_key, secret_preview(profile));
    }
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

fn require_profile_protocol(profile: &ProfileDraft, supported: &[&str]) -> Result<(), String> {
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

fn config_file_protocol_supported_fields(app: &str, provider: &str, protocol: &str) -> bool {
    if provider_is_official(provider) {
        return true;
    }
    let Ok(protocol) = normalize_protocol(Some(protocol)) else {
        return false;
    };
    match canonical_profile_app(app).as_str() {
        "codex" => codex_wire_api_for_protocol(&protocol).is_ok(),
        "claude-desktop" => protocol == PROTOCOL_ANTHROPIC_MESSAGES,
        "claude" => protocol == PROTOCOL_ANTHROPIC_MESSAGES,
        "gemini" => protocol == PROTOCOL_GOOGLE_GEMINI,
        "gemini-code-assist" => protocol == PROTOCOL_GOOGLE_GEMINI,
        "opencode" => {
            matches!(
                protocol.as_str(),
                PROTOCOL_OPENAI_CHAT_COMPLETIONS | PROTOCOL_OPENAI_RESPONSES
            )
        }
        "openclaw" => protocol == PROTOCOL_OPENAI_CHAT_COMPLETIONS,
        "hermes" => protocol == PROTOCOL_OPENAI_CHAT_COMPLETIONS,
        _ => false,
    }
}

fn profile_protocol_supported_for_mode(
    app: &str,
    mode: ProviderApplyMode,
    provider: &str,
    protocol: &str,
) -> bool {
    if provider_is_official(provider) || mode == ProviderApplyMode::Gateway {
        return true;
    }
    config_file_protocol_supported_fields(app, provider, protocol)
}

fn ensure_profile_protocol_supported_for_mode(
    app: &str,
    mode: ProviderApplyMode,
    provider: &str,
    protocol: &str,
) -> Result<(), String> {
    if profile_protocol_supported_for_mode(app, mode, provider, protocol) {
        return Ok(());
    }
    Err(format!(
        "Config profiles do not support {} for '{}'.",
        protocol_display_name(protocol),
        canonical_profile_app(app)
    ))
}

fn config_file_protocol_supported(profile: &ProfileDraft) -> bool {
    config_file_protocol_supported_fields(&profile.app, &profile.provider, &profile.protocol)
}

fn profile_model(profile: &ProfileDraft) -> Option<&str> {
    let model = profile.model.trim();
    (!model.is_empty()).then_some(model)
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

    if !is_codex_family_app(&profile.app) {
        return build_non_codex_native_config_preview(profile, native_config_path, paths, mode);
    }

    let codex_config_path = paths.home_dir.join(".codex").join("config.toml");
    let path = native_config_path
        .map(ToString::to_string)
        .unwrap_or_else(|| display_path(&codex_config_path));
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut warnings = match mode {
        ProviderApplyMode::Config if provider_is_official(&profile.provider) => vec![
            "Official provider uses the target client's own login.".to_string(),
            "No Provider API key or model override is required.".to_string(),
            "Changing Codex config usually requires restarting Codex or opening a new Codex session.".to_string(),
        ],
        ProviderApplyMode::Config => vec![
            "Config profiles write Codex's provider entry directly to the selected upstream Provider.".to_string(),
            "The preview masks the Provider API key. The actual key is loaded from the system keychain during apply.".to_string(),
            "Changing Codex config usually requires restarting Codex or opening a new Codex session.".to_string(),
        ],
        ProviderApplyMode::Gateway => vec![
            "Gateway profiles are a one-time relay injection target, not a direct Provider switch.".to_string(),
            "Switching profiles later changes only the Gateway active profile for this tool.".to_string(),
            "The preview masks the local CodeStudio token. Real Provider API keys are never written to Codex config.".to_string(),
            "Codex official login is still required for the desktop app; the Local Gateway only takes over model requests.".to_string(),
            "If Codex is already running, restart Codex or open a new Codex session after bootstrap so it reloads config.toml.".to_string(),
        ],
    };
    let mut value = toml::Value::Table(toml::map::Map::new());
    let status = if codex_config_path.exists() {
        let content = fs::read_to_string(&codex_config_path).map_err(|err| err.to_string())?;
        match toml::from_str::<toml::Value>(&content) {
            Ok(parsed) => {
                value = parsed;
                "parsed".to_string()
            }
            Err(err) => {
                warnings.push(format!(
                    "Existing Codex config could not be parsed, so only create-style preview is available: {err}"
                ));
                "parse_error".to_string()
            }
        }
    } else {
        warnings.push(
            "Codex config does not exist yet; adapter would create it after confirmation."
                .to_string(),
        );
        "missing".to_string()
    };

    let changes = match mode {
        ProviderApplyMode::Config => {
            if provider_is_official(&profile.provider) {
                let mut changes = vec![
                    diff_line(
                        &value,
                        "model_provider",
                        "openai",
                        "Selects Codex's official OpenAI provider.",
                    ),
                    diff_remove_line(
                        &value,
                        "model_providers.openai",
                        "Removes an OpenAI provider override because Codex's official provider cannot be overridden.",
                    ),
                ];
                if profile.model.trim().is_empty() {
                    changes.push(diff_remove_line(
                        &value,
                        "model",
                        "Official provider can use Codex's own model default.",
                    ));
                } else {
                    changes.push(diff_line(
                        &value,
                        "model",
                        profile.model.trim(),
                        "Sets Codex to the selected official model.",
                    ));
                }
                changes.extend(codex_preserved_auth_repair_diff_lines(&value));
                changes
            } else {
                let provider_id = codex_provider_id_for_profile(profile);
                let provider_name = profile.provider.trim();
                let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;
                let mut changes = vec![
                    diff_line(
                        &value,
                        "model_provider",
                        &provider_id,
                        "Selects the direct provider entry managed by CodeStudio Lite.",
                    ),
                    diff_line(
                        &value,
                        &format!("model_providers.{provider_id}.name"),
                        &provider_name,
                        "Adds a readable provider label for this upstream Provider.",
                    ),
                    diff_line(
                        &value,
                        &format!("model_providers.{provider_id}.wire_api"),
                        wire_api,
                        "Uses Codex's supported provider wire API for custom providers.",
                    ),
                    diff_line(
                        &value,
                        &format!("model_providers.{provider_id}.base_url"),
                        profile.base_url.trim(),
                        "Points Codex directly at the upstream Provider Base URL.",
                    ),
                    diff_line(
                        &value,
                        &format!("model_providers.{provider_id}.requires_openai_auth"),
                        "false",
                        "Disables Codex official OpenAI auth for this custom upstream entry.",
                    ),
                ];
                changes.extend(codex_preserved_auth_repair_diff_lines(&value));
                if profile.model.trim().is_empty() {
                    changes.push(diff_remove_line(
                        &value,
                        "model",
                        "No model override is required when the profile has no selected model.",
                    ));
                } else {
                    changes.push(diff_line(
                        &value,
                        "model",
                        profile.model.trim(),
                        "Sets Codex to the selected upstream model.",
                    ));
                }
                changes
            }
        }
        ProviderApplyMode::Gateway => {
            let provider_id = client.provider_id;
            let provider_name = client.provider_name;
            let mut changes = vec![
                diff_line(
                    &value,
                    "model_provider",
                    &provider_id,
                    "Selects the CodeStudio Lite localhost provider.",
                ),
                diff_line(
                    &value,
                    "model",
                    &client.model,
                    "Sets Codex to the virtual model name resolved by the Local Gateway.",
                ),
                diff_line(
                    &value,
                    &format!("model_providers.{provider_id}.name"),
                    &provider_name,
                    "Adds a readable provider label for the Local Gateway.",
                ),
                diff_line(
                    &value,
                    &format!("model_providers.{provider_id}.wire_api"),
                    "responses",
                    "Uses Codex's supported provider wire API for custom providers.",
                ),
                diff_line(
                    &value,
                    &format!("model_providers.{provider_id}.base_url"),
                    &client.base_url,
                    "Points Codex at the CodeStudio Lite Local Gateway.",
                ),
                diff_line(
                    &value,
                    &format!("model_providers.{provider_id}.requires_openai_auth"),
                    "false",
                    "Disables Codex official OpenAI auth for the Local Gateway provider entry.",
                ),
            ];
            changes.extend(codex_preserved_auth_repair_diff_lines(&value));
            changes
        }
    };

    Ok(Some(NativeConfigPreview {
        tool: "codex".to_string(),
        path,
        status,
        write_enabled: true,
        changes,
        warnings,
        content: None,
    }))
}

fn build_non_codex_native_config_preview(
    profile: &ProfileDraft,
    native_config_path: Option<&str>,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
) -> Result<Option<NativeConfigPreview>, String> {
    if canonical_profile_app(&profile.app) == "claude-desktop" {
        return build_claude_desktop_native_config_preview(
            profile,
            native_config_path,
            paths,
            mode,
        );
    }

    if mode == ProviderApplyMode::Gateway {
        return build_non_codex_gateway_native_config_preview(profile, native_config_path, paths);
    }

    if mode != ProviderApplyMode::Config {
        return Ok(None);
    }

    let Some(path_buf) = native_config_path_for_profile_mode(profile, paths, mode)? else {
        return Ok(None);
    };
    let path = native_config_path
        .map(ToString::to_string)
        .unwrap_or_else(|| display_path(&path_buf));
    let app = canonical_profile_app(&profile.app);
    let provider_id = custom_provider_id_for_profile(profile);
    let is_official = provider_is_official(&profile.provider);
    let mut warnings = match app.as_str() {
        "claude" if is_official => vec![
            "Official provider restores Claude Code to its own login.".to_string(),
            "CodeStudio Lite removes managed API or Gateway fields from Claude settings."
                .to_string(),
        ],
        "claude" => vec![
            "Config profiles write Claude Code user settings under the env section."
                .to_string(),
            "The selected endpoint must be Anthropic/Claude-compatible; generic OpenAI-only endpoints need a translator."
                .to_string(),
            "Restart Claude Code or open a new session after applying so settings reload."
                .to_string(),
        ],
        "gemini" if is_official => vec![
            "Official provider restores Gemini CLI to its own login.".to_string(),
            "CodeStudio Lite removes managed API or Gateway values from ~/.gemini/.env."
                .to_string(),
        ],
        "gemini" => vec![
            "Gemini CLI reads API key and base URL from environment variables, so this adapter writes ~/.gemini/.env."
                .to_string(),
            "Restart Gemini CLI or open a new terminal session after applying so environment variables reload."
                .to_string(),
        ],
        "gemini-code-assist" if is_official => vec![
            "Official provider restores Gemini Code Assist to its own login.".to_string(),
            "CodeStudio Lite removes the managed API key setting from VS Code user settings."
                .to_string(),
        ],
        "gemini-code-assist" => vec![
            "Gemini Code Assist stores its API key in VS Code user settings."
                .to_string(),
            "The public Gemini Code Assist VS Code setting exposes the API key; Provider Base URL and model are kept in CodeStudio Lite but are not written to the extension config."
                .to_string(),
            "Restart VS Code or reload the Gemini Code Assist extension after applying so settings reload."
                .to_string(),
        ],
        "opencode" if is_official => vec![
            "Official provider removes CodeStudio Lite managed OpenCode provider entries."
                .to_string(),
        ],
        "opencode" => vec![
            "OpenCode custom providers are written to opencode.json using the OpenAI-compatible provider package."
                .to_string(),
            "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        "openclaw" if is_official => vec![
            "Official provider removes CodeStudio Lite managed OpenClaw provider entries."
                .to_string(),
        ],
        "openclaw" => vec![
            "OpenClaw providers are written in models.mode=merge so existing provider definitions can stay available."
                .to_string(),
            "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        "hermes" if is_official => vec![
            "Official provider removes CodeStudio Lite managed Hermes custom endpoint fields."
                .to_string(),
        ],
        "hermes" => vec![
            "Hermes custom providers are written to ~/.hermes/config.yaml under the model section."
                .to_string(),
            "Existing YAML comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
            "Hermes config profiles currently target OpenAI Chat Completions endpoints."
                .to_string(),
        ],
        _ => return Ok(None),
    };

    let (status, changes) = if is_official {
        build_non_codex_official_native_config_preview_changes(&app, &path_buf, &mut warnings)?
    } else {
        match app.as_str() {
            "gemini" => {
                let (env, status) = read_env_preview(&path_buf, &mut warnings)?;
                let mut changes = vec![
                    env_diff_line(
                        &env,
                        "GEMINI_API_KEY",
                        secret_preview(profile),
                        "Stores the selected Provider API key for Gemini CLI.",
                    ),
                    env_diff_line(
                        &env,
                        "GOOGLE_GEMINI_BASE_URL",
                        profile.base_url.trim(),
                        "Points Gemini CLI at the selected upstream Provider Base URL.",
                    ),
                ];
                if let Some(model) = profile_model(profile) {
                    changes.push(env_diff_line(
                        &env,
                        "GEMINI_MODEL",
                        model,
                        "Sets Gemini CLI to the selected upstream model.",
                    ));
                } else {
                    changes.push(env_diff_remove_line(
                        &env,
                        "GEMINI_MODEL",
                        "Model is optional; no Gemini model override will be written.",
                    ));
                }
                (status, changes)
            }
            "claude" => {
                let (json, status) =
                    read_json_preview(&path_buf, "Claude settings", &mut warnings)?;
                let mut changes = vec![
                    json_diff_line(
                        &json,
                        &["env", "ANTHROPIC_BASE_URL"],
                        profile.base_url.trim(),
                        "Points Claude Code at the selected upstream Provider Base URL.",
                    ),
                    json_diff_line(
                        &json,
                        &["env", "ANTHROPIC_AUTH_TOKEN"],
                        secret_preview(profile),
                        "Stores the selected Provider API key as Claude Code's bearer token.",
                    ),
                ];
                if let Some(model) = profile_model(profile) {
                    changes.push(json_diff_line(
                        &json,
                        &["model"],
                        model,
                        "Sets Claude Code to the selected upstream model.",
                    ));
                    changes.push(json_diff_line(
                        &json,
                        &["env", "ANTHROPIC_MODEL"],
                        model,
                        "Keeps the model override available to Claude Code environment consumers.",
                    ));
                } else {
                    changes.push(json_diff_remove_line(
                        &json,
                        &["model"],
                        "Model is optional; no Claude model override will be written.",
                    ));
                    changes.push(json_diff_remove_line(
                        &json,
                        &["env", "ANTHROPIC_MODEL"],
                        "Model is optional; no Claude model environment override will be written.",
                    ));
                }
                (status, changes)
            }
            "gemini-code-assist" => {
                let (json, status) =
                    read_json_preview(&path_buf, "VS Code user settings", &mut warnings)?;
                let mut changes = vec![json_diff_line(
                    &json,
                    &[GEMINI_CODE_ASSIST_API_KEY_SETTING],
                    secret_preview(profile),
                    "Stores the selected Provider API key for Gemini Code Assist.",
                )];
                changes.push(diff_value_line(
                "Provider Base URL".to_string(),
                None,
                Some(profile.base_url.trim().to_string()),
                "Gemini Code Assist does not expose a VS Code setting for custom Base URL; this stays in the CodeStudio Lite profile.",
            ));
                if let Some(model) = profile_model(profile) {
                    changes.push(diff_value_line(
                    "Model".to_string(),
                    None,
                    Some(model.to_string()),
                    "Gemini Code Assist does not expose a VS Code setting for model override; this stays in the CodeStudio Lite profile.",
                ));
                }
                (status, changes)
            }
            "opencode" => {
                let (json, status) =
                    read_json_preview(&path_buf, "OpenCode config", &mut warnings)?;
                let mut changes = vec![
                    json_diff_line(
                        &json,
                        &["$schema"],
                        "https://opencode.ai/config.json",
                        "Keeps OpenCode config aligned with the published schema.",
                    ),
                    json_diff_line(
                        &json,
                        &["provider", &provider_id, "npm"],
                        "@ai-sdk/openai-compatible",
                        "Uses OpenCode's OpenAI-compatible provider package.",
                    ),
                    json_diff_line(
                        &json,
                        &["provider", &provider_id, "options", "baseURL"],
                        profile.base_url.trim(),
                        "Points OpenCode at the selected upstream Provider Base URL.",
                    ),
                    json_diff_line(
                        &json,
                        &["provider", &provider_id, "options", "apiKey"],
                        secret_preview(profile),
                        "Stores the selected Provider API key for OpenCode.",
                    ),
                ];
                if let Some(model) = profile_model(profile) {
                    changes.push(json_diff_line(
                        &json,
                        &["model"],
                        &format!("{provider_id}/{model}"),
                        "Selects the provider/model pair in OpenCode.",
                    ));
                    changes.push(json_diff_line(
                        &json,
                        &["provider", &provider_id, "models", model, "name"],
                        model,
                        "Registers the selected model under the managed provider.",
                    ));
                } else {
                    changes.push(json_diff_remove_line(
                        &json,
                        &["model"],
                        "Model is optional; no OpenCode model override will be written.",
                    ));
                }
                (status, changes)
            }
            "openclaw" => {
                let (json, status) =
                    read_json_preview(&path_buf, "OpenClaw config", &mut warnings)?;
                let mut changes = vec![
                json_diff_line(
                    &json,
                    &["models", "mode"],
                    "merge",
                    "Merges CodeStudio Lite provider definitions with existing OpenClaw providers.",
                ),
                json_diff_line(
                    &json,
                    &["models", "providers", &provider_id, "api"],
                    "openai-completions",
                    "Uses OpenClaw's OpenAI-compatible API adapter.",
                ),
                json_diff_line(
                    &json,
                    &["models", "providers", &provider_id, "baseUrl"],
                    profile.base_url.trim(),
                    "Points OpenClaw at the selected upstream Provider Base URL.",
                ),
                json_diff_line(
                    &json,
                    &["models", "providers", &provider_id, "apiKey"],
                    secret_preview(profile),
                    "Stores the selected Provider API key for OpenClaw.",
                ),
            ];
                if let Some(model) = profile_model(profile) {
                    changes.push(json_diff_line(
                        &json,
                        &["agents", "defaults", "model", "primary"],
                        &format!("{provider_id}/{model}"),
                        "Selects the provider/model pair as OpenClaw's primary default.",
                    ));
                }
                (status, changes)
            }
            "hermes" => {
                let (yaml, status) = read_yaml_preview(&path_buf, "Hermes config", &mut warnings)?;
                let mut changes = vec![
                    yaml_diff_line(
                        &yaml,
                        &["model", "provider"],
                        "custom",
                        "Selects Hermes custom provider mode.",
                    ),
                    yaml_diff_line(
                        &yaml,
                        &["model", "base_url"],
                        profile.base_url.trim(),
                        "Points Hermes at the selected upstream Provider Base URL.",
                    ),
                    yaml_diff_line(
                        &yaml,
                        &["model", "api_key"],
                        secret_preview(profile),
                        "Stores the selected Provider API key for Hermes.",
                    ),
                    yaml_diff_line(
                        &yaml,
                        &["model", "api_mode"],
                        "chat_completions",
                        "Uses Hermes' OpenAI Chat Completions custom endpoint mode.",
                    ),
                ];
                if let Some(model) = profile_model(profile) {
                    changes.push(yaml_diff_line(
                        &yaml,
                        &["model", "default"],
                        model,
                        "Sets Hermes to the selected upstream model.",
                    ));
                } else {
                    changes.push(yaml_diff_remove_line(
                        &yaml,
                        &["model", "default"],
                        "Model is optional; no Hermes model override will be written.",
                    ));
                }
                (status, changes)
            }
            _ => unreachable!(),
        }
    };

    Ok(Some(NativeConfigPreview {
        tool: app,
        path,
        status,
        write_enabled: true,
        changes,
        warnings,
        content: None,
    }))
}

fn build_non_codex_official_native_config_preview_changes(
    app: &str,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<(String, Vec<NativeConfigDiffLine>), String> {
    match app {
        "claude" => {
            let (json, status) = read_json_preview(path, "Claude settings", warnings)?;
            Ok((
                status,
                vec![
                    json_diff_remove_line(
                        &json,
                        &["env", "ANTHROPIC_BASE_URL"],
                        "Restores Claude Code to the client's own official endpoint.",
                    ),
                    json_diff_remove_line(
                        &json,
                        &["env", "ANTHROPIC_AUTH_TOKEN"],
                        "Removes the CodeStudio Lite managed API token from Claude settings.",
                    ),
                    json_diff_remove_line(
                        &json,
                        &["model"],
                        "Removes the CodeStudio Lite managed model override.",
                    ),
                    json_diff_remove_line(
                        &json,
                        &["env", "ANTHROPIC_MODEL"],
                        "Removes the CodeStudio Lite managed model environment override.",
                    ),
                ],
            ))
        }
        "gemini" => {
            let (env, status) = read_env_preview(path, warnings)?;
            Ok((
                status,
                vec![
                    env_diff_remove_line(
                        &env,
                        "GEMINI_API_KEY",
                        "Removes the CodeStudio Lite managed Gemini API key.",
                    ),
                    env_diff_remove_line(
                        &env,
                        "GOOGLE_GEMINI_BASE_URL",
                        "Restores Gemini CLI to the client's own official endpoint.",
                    ),
                    env_diff_remove_line(
                        &env,
                        "GEMINI_MODEL",
                        "Removes the CodeStudio Lite managed model override.",
                    ),
                ],
            ))
        }
        "gemini-code-assist" => {
            let (json, status) = read_json_preview(path, "VS Code user settings", warnings)?;
            Ok((
                status,
                vec![json_diff_remove_line(
                    &json,
                    &[GEMINI_CODE_ASSIST_API_KEY_SETTING],
                    "Removes the CodeStudio Lite managed Gemini Code Assist API key.",
                )],
            ))
        }
        "opencode" => {
            let (json, status) = read_json_preview(path, "OpenCode config", warnings)?;
            Ok((
                status,
                vec![
                    diff_value_line(
                        "provider.codestudio-*".to_string(),
                        Some("managed provider entries".to_string()),
                        None,
                        "Removes CodeStudio Lite managed OpenCode provider entries.",
                    ),
                    json_diff_remove_line(
                        &json,
                        &["model"],
                        "Removes the active model only when it points to a CodeStudio Lite managed provider.",
                    ),
                ],
            ))
        }
        "openclaw" => {
            let (json, status) = read_json_preview(path, "OpenClaw config", warnings)?;
            Ok((
                status,
                vec![
                    diff_value_line(
                        "models.providers.codestudio-*".to_string(),
                        Some("managed provider entries".to_string()),
                        None,
                        "Removes CodeStudio Lite managed OpenClaw provider entries.",
                    ),
                    json_diff_remove_line(
                        &json,
                        &["agents", "defaults", "model", "primary"],
                        "Removes the primary model only when it points to a CodeStudio Lite managed provider.",
                    ),
                ],
            ))
        }
        "hermes" => {
            let (yaml, status) = read_yaml_preview(path, "Hermes config", warnings)?;
            Ok((
                status,
                vec![
                    yaml_diff_remove_line(
                        &yaml,
                        &["model", "provider"],
                        "Restores Hermes away from the CodeStudio Lite managed custom provider mode.",
                    ),
                    yaml_diff_remove_line(
                        &yaml,
                        &["model", "base_url"],
                        "Removes the CodeStudio Lite managed Base URL.",
                    ),
                    yaml_diff_remove_line(
                        &yaml,
                        &["model", "api_key"],
                        "Removes the CodeStudio Lite managed API key.",
                    ),
                    yaml_diff_remove_line(
                        &yaml,
                        &["model", "api_mode"],
                        "Removes the CodeStudio Lite managed API mode.",
                    ),
                    yaml_diff_remove_line(
                        &yaml,
                        &["model", "default"],
                        "Removes the CodeStudio Lite managed model override.",
                    ),
                ],
            ))
        }
        _ => Ok(("unsupported".to_string(), Vec::new())),
    }
}

fn build_claude_desktop_native_config_preview(
    profile: &ProfileDraft,
    native_config_path: Option<&str>,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
) -> Result<Option<NativeConfigPreview>, String> {
    if mode == ProviderApplyMode::Config
        && !provider_is_official(&profile.provider)
        && !config_file_protocol_supported(profile)
    {
        return Ok(None);
    }

    let desktop_paths = claude_desktop_paths(paths)?;
    let path = native_config_path
        .map(ToString::to_string)
        .unwrap_or_else(|| display_path(&desktop_paths.profile_path));
    let mut warnings = match mode {
        ProviderApplyMode::Config if provider_is_official(&profile.provider) => vec![
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

    let (json, status) = read_json_preview(
        &desktop_paths.profile_path,
        "Claude Desktop 3P profile",
        &mut warnings,
    )?;
    let changes = match mode {
        ProviderApplyMode::Config if provider_is_official(&profile.provider) => vec![
            diff_value_line(
                "deploymentMode".to_string(),
                None,
                Some("1p".to_string()),
                "Restores Claude Desktop to first-party official mode in both config files.",
            ),
            diff_value_line(
                "configLibrary/_meta.appliedId".to_string(),
                None,
                None,
                "Removes the CodeStudio Lite profile from Claude Desktop's 3P config library.",
            ),
            diff_value_line(
                format!("{CLAUDE_DESKTOP_PROFILE_ID}.json"),
                None,
                None,
                "Deletes the managed CodeStudio Lite Claude Desktop 3P profile file.",
            ),
        ],
        ProviderApplyMode::Config => {
            let model_specs = claude_desktop_direct_inference_models(profile);
            let mut changes = vec![
                diff_value_line(
                    "developer_settings.allowDevTools".to_string(),
                    None,
                    Some("true".to_string()),
                    "Enables Claude Desktop developer mode before applying the managed 3P profile.",
                ),
                diff_value_line(
                    "deploymentMode".to_string(),
                    None,
                    Some("3p".to_string()),
                    "Switches Claude Desktop to third-party provider mode in both config files.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceProvider"],
                    "gateway",
                    "Uses Claude Desktop's built-in 3P inference gateway provider.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceGatewayAuthScheme"],
                    "bearer",
                    "Authenticates the 3P profile with a bearer token.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceGatewayBaseUrl"],
                    profile.base_url.trim(),
                    "Points Claude Desktop directly at the selected Anthropic-compatible Provider Base URL.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceGatewayApiKey"],
                    secret_preview(profile),
                    "Stores the selected Provider API key in Claude Desktop's 3P profile.",
                ),
            ];
            if model_specs.is_empty() {
                changes.push(json_diff_remove_line(
                    &json,
                    &["inferenceModels"],
                    "Model is optional; no Claude Desktop model menu override will be written.",
                ));
            } else {
                changes.push(json_diff_line(
                    &json,
                    &["inferenceModels"],
                    &claude_desktop_model_specs_preview(&model_specs),
                    "Exposes the selected Claude-safe model in Claude Desktop's model menu.",
                ));
            }
            changes
        }
        ProviderApplyMode::Gateway => {
            let client = gateway::client_config_for_tool("claude-desktop")?;
            let base_url = claude_desktop_gateway_profile_base_url(&client.base_url);
            let model_specs = claude_desktop_gateway_inference_models(profile);
            vec![
                diff_value_line(
                    "developer_settings.allowDevTools".to_string(),
                    None,
                    Some("true".to_string()),
                    "Enables Claude Desktop developer mode before applying the managed Gateway profile.",
                ),
                diff_value_line(
                    "deploymentMode".to_string(),
                    None,
                    Some("3p".to_string()),
                    "Switches Claude Desktop to third-party provider mode in both config files.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceProvider"],
                    "gateway",
                    "Uses Claude Desktop's built-in 3P inference gateway provider.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceGatewayAuthScheme"],
                    "bearer",
                    "Authenticates the 3P profile with the local CodeStudio token.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceGatewayBaseUrl"],
                    &base_url,
                    "Points Claude Desktop at the tool-scoped CodeStudio Lite Local Gateway.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceGatewayApiKey"],
                    &client.token_preview,
                    "Stores only the local CodeStudio token, not the real upstream Provider API key.",
                ),
                json_diff_line(
                    &json,
                    &["inferenceModels"],
                    &claude_desktop_model_specs_preview(&model_specs),
                    "Exposes Claude Desktop-safe route IDs while the Gateway resolves the real upstream model.",
                ),
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

fn claude_desktop_model_specs_preview(specs: &[ClaudeDesktopInferenceModelSpec]) -> String {
    let value = serde_json::Value::Array(
        specs
            .iter()
            .map(claude_desktop_inference_model_json)
            .collect(),
    );
    serde_json::to_string(&value).unwrap_or_else(|_| "[]".to_string())
}

fn build_non_codex_gateway_native_config_preview(
    profile: &ProfileDraft,
    native_config_path: Option<&str>,
    paths: &crate::core::app_paths::AppPaths,
) -> Result<Option<NativeConfigPreview>, String> {
    let Some(path_buf) =
        native_config_path_for_profile_mode(profile, paths, ProviderApplyMode::Gateway)?
    else {
        return Ok(None);
    };
    let path = native_config_path
        .map(ToString::to_string)
        .unwrap_or_else(|| display_path(&path_buf));
    let app = canonical_profile_app(&profile.app);
    let client = gateway::client_config_for_tool(&app)?;
    let provider_id = client.provider_id.clone();
    let model = gateway_config_model_for_profile(profile);
    let model_ref = format!("{provider_id}/{model}");
    let mut warnings = match app.as_str() {
        "claude" => vec![
            "Gateway profiles write Claude Code settings to the tool-scoped local gateway URL."
                .to_string(),
            "Restart Claude Code or open a new session after applying so settings reload."
                .to_string(),
        ],
        "gemini" => vec![
            "Gateway profiles write Gemini CLI environment values to the tool-scoped local gateway URL."
                .to_string(),
            "Restart Gemini CLI or open a new terminal session after applying so environment variables reload."
                .to_string(),
        ],
        "opencode" => vec![
            "Gateway profiles write OpenCode's provider entry to the tool-scoped local gateway URL."
                .to_string(),
            "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        "openclaw" => vec![
            "Gateway profiles write OpenClaw's provider entry to the tool-scoped local gateway URL."
                .to_string(),
            "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        "hermes" => vec![
            "Gateway profiles write Hermes custom provider settings to the tool-scoped local gateway URL."
                .to_string(),
            "Existing YAML comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        _ => return Ok(None),
    };
    warnings.push(
        "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running."
            .to_string(),
    );
    warnings.push(
        "Real upstream Provider API keys stay in the system keychain and are used by the local gateway."
            .to_string(),
    );

    let (status, changes) = match app.as_str() {
        "claude" => {
            let (json, status) = read_json_preview(&path_buf, "Claude settings", &mut warnings)?;
            (
                status,
                vec![
                    json_diff_line(
                        &json,
                        &["env", "ANTHROPIC_BASE_URL"],
                        &client.base_url,
                        "Points Claude Code at the tool-scoped CodeStudio Lite Local Gateway.",
                    ),
                    json_diff_line(
                        &json,
                        &["env", "ANTHROPIC_AUTH_TOKEN"],
                        &client.token_preview,
                        "Stores only the local CodeStudio token, not the real upstream Provider API key.",
                    ),
                    json_diff_line(
                        &json,
                        &["model"],
                        model,
                        "Sets Claude Code to the virtual model name resolved by the Local Gateway.",
                    ),
                    json_diff_line(
                        &json,
                        &["env", "ANTHROPIC_MODEL"],
                        model,
                        "Keeps the local gateway virtual model available to Claude Code environment consumers.",
                    ),
                ],
            )
        }
        "gemini" => {
            let (env, status) = read_env_preview(&path_buf, &mut warnings)?;
            (
                status,
                vec![
                    env_diff_line(
                        &env,
                        "GEMINI_API_KEY",
                        &client.token_preview,
                        "Stores only the local CodeStudio token, not the real upstream Provider API key.",
                    ),
                    env_diff_line(
                        &env,
                        "GOOGLE_GEMINI_BASE_URL",
                        &client.base_url,
                        "Points Gemini CLI at the tool-scoped CodeStudio Lite Local Gateway.",
                    ),
                    env_diff_line(
                        &env,
                        "GEMINI_MODEL",
                        model,
                        "Sets Gemini CLI to the virtual model name resolved by the Local Gateway.",
                    ),
                ],
            )
        }
        "opencode" => {
            let (json, status) = read_json_preview(&path_buf, "OpenCode config", &mut warnings)?;
            (
                status,
                vec![
                    json_diff_line(
                        &json,
                        &["$schema"],
                        "https://opencode.ai/config.json",
                        "Keeps OpenCode config aligned with the published schema.",
                    ),
                    json_diff_line(
                        &json,
                        &["provider", &provider_id, "npm"],
                        "@ai-sdk/openai-compatible",
                        "Uses OpenCode's OpenAI-compatible provider package.",
                    ),
                    json_diff_line(
                        &json,
                        &["provider", &provider_id, "options", "baseURL"],
                        &client.base_url,
                        "Points OpenCode at the tool-scoped CodeStudio Lite Local Gateway.",
                    ),
                    json_diff_line(
                        &json,
                        &["provider", &provider_id, "options", "apiKey"],
                        &client.token_preview,
                        "Stores only the local CodeStudio token, not the real upstream Provider API key.",
                    ),
                    json_diff_line(
                        &json,
                        &["model"],
                        &model_ref,
                        "Selects the local gateway provider/model pair in OpenCode.",
                    ),
                    json_diff_line(
                        &json,
                        &["provider", &provider_id, "models", model, "name"],
                        model,
                        "Registers the local gateway virtual model under the managed provider.",
                    ),
                ],
            )
        }
        "openclaw" => {
            let (json, status) = read_json_preview(&path_buf, "OpenClaw config", &mut warnings)?;
            (
                status,
                vec![
                    json_diff_line(
                        &json,
                        &["models", "mode"],
                        "merge",
                        "Merges CodeStudio Lite provider definitions with existing OpenClaw providers.",
                    ),
                    json_diff_line(
                        &json,
                        &["models", "providers", &provider_id, "api"],
                        "openai-completions",
                        "Uses OpenClaw's OpenAI-compatible API adapter.",
                    ),
                    json_diff_line(
                        &json,
                        &["models", "providers", &provider_id, "baseUrl"],
                        &client.base_url,
                        "Points OpenClaw at the tool-scoped CodeStudio Lite Local Gateway.",
                    ),
                    json_diff_line(
                        &json,
                        &["models", "providers", &provider_id, "apiKey"],
                        &client.token_preview,
                        "Stores only the local CodeStudio token, not the real upstream Provider API key.",
                    ),
                    json_diff_line(
                        &json,
                        &["agents", "defaults", "model", "primary"],
                        &model_ref,
                        "Selects the local gateway provider/model pair as OpenClaw's primary default.",
                    ),
                ],
            )
        }
        "hermes" => {
            let (yaml, status) = read_yaml_preview(&path_buf, "Hermes config", &mut warnings)?;
            (
                status,
                vec![
                    yaml_diff_line(
                        &yaml,
                        &["model", "provider"],
                        "custom",
                        "Selects Hermes custom provider mode.",
                    ),
                    yaml_diff_line(
                        &yaml,
                        &["model", "base_url"],
                        &client.base_url,
                        "Points Hermes at the tool-scoped CodeStudio Lite Local Gateway.",
                    ),
                    yaml_diff_line(
                        &yaml,
                        &["model", "api_key"],
                        &client.token_preview,
                        "Stores only the local CodeStudio token, not the real upstream Provider API key.",
                    ),
                    yaml_diff_line(
                        &yaml,
                        &["model", "api_mode"],
                        "chat_completions",
                        "Uses Hermes' OpenAI Chat Completions custom endpoint mode.",
                    ),
                    yaml_diff_line(
                        &yaml,
                        &["model", "default"],
                        model,
                        "Sets Hermes to the virtual model name resolved by the Local Gateway.",
                    ),
                ],
            )
        }
        _ => unreachable!(),
    };

    Ok(Some(NativeConfigPreview {
        tool: app,
        path,
        status,
        write_enabled: true,
        changes,
        warnings,
        content: None,
    }))
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

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let tmp_path = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp_path).map_err(|err| err.to_string())?;
        file.write_all(bytes).map_err(|err| err.to_string())?;
        file.sync_all().map_err(|err| err.to_string())?;
    }
    fs::rename(&tmp_path, path).map_err(|err| err.to_string())
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod profile_tests;
