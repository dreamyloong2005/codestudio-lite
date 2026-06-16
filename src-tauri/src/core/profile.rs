use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::backup;
use crate::core::codex_client;
use crate::core::credentials;
use crate::core::detector;
use crate::core::env_health;
use crate::core::gateway;
use crate::core::platform::{hidden_command, resolve_command, run_powershell};
use crate::core::process_control;
use crate::core::tool_registry;
use crate::core::types::{
    ActiveProfilesByMode, AppSettings, ApplyProfileRequest, ApplyProfileResult, ConfigState,
    DuplicateProfileDraftRequest, ExportProfilesResult, ImportProfilesRequest,
    ImportProfilesResult, InstallState, NativeConfigDiffLine, NativeConfigPreview,
    PreviewProfileApplyRequest, PreviewProfileApplyResult, PreviewProfileWriteRequest,
    PreviewProfileWriteResult, ProfileApplyPreviewItem, ProfileConnectionCheck, ProfileDraft,
    ProfileExportBundle, ProfileSummary, ProfileWritePreviewItem, ProviderApplyMode,
    ProviderApplyModePreview, SaveProfileDraftRequest, Severity, SwitchActiveProfileRequest,
    TestProfileConnectionRequest, TestProfileConnectionResult, UpdateAppSettingsRequest,
    UpdateProfileDraftRequest,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct AppConfig {
    #[serde(default)]
    active_profiles_by_mode: ActiveProfilesByMode,
    ui: UiConfig,
    security: SecurityConfig,
    paths: PathConfig,
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
}

#[derive(Debug, Serialize, Deserialize)]
struct SecurityConfig {
    backup_before_write: bool,
    redact_secrets: bool,
    confirm_install_commands: bool,
    confirm_config_writes: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct PathConfig {
    profiles_dir: String,
    backups_dir: String,
    logs_dir: String,
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
    timeout_seconds: u16,
    profile_path: std::path::PathBuf,
    secret_status: &'static str,
    auth_ref: Option<String>,
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
    ("codex", "Codex 官方", PROTOCOL_OPENAI_RESPONSES),
    (
        "claude-desktop",
        "Claude Desktop 官方",
        PROTOCOL_ANTHROPIC_MESSAGES,
    ),
    ("claude", "Claude Code 官方", PROTOCOL_ANTHROPIC_MESSAGES),
    ("gemini", "Gemini CLI 官方", PROTOCOL_GOOGLE_GEMINI),
    (
        "gemini-code-assist",
        "Gemini Code Assist 官方",
        PROTOCOL_GOOGLE_GEMINI,
    ),
    (
        "opencode",
        "OpenCode 官方",
        PROTOCOL_OPENAI_CHAT_COMPLETIONS,
    ),
    (
        "openclaw",
        "OpenClaw 官方",
        PROTOCOL_OPENAI_CHAT_COMPLETIONS,
    ),
    ("hermes", "Hermes 官方", PROTOCOL_OPENAI_CHAT_COMPLETIONS),
];

pub fn ensure_app_dirs() -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;

    if !paths.config_file.exists() {
        let config = AppConfig {
            active_profiles_by_mode: ActiveProfilesByMode::default(),
            ui: UiConfig {
                theme: "system".to_string(),
                language: "zh-CN".to_string(),
            },
            security: SecurityConfig {
                backup_before_write: true,
                redact_secrets: true,
                confirm_install_commands: true,
                confirm_config_writes: true,
            },
            paths: PathConfig {
                profiles_dir: "~/.codestudio-lite/profiles".to_string(),
                backups_dir: "~/.codestudio-lite/backups".to_string(),
                logs_dir: "~/.codestudio-lite/logs".to_string(),
            },
        };
        let toml = toml::to_string_pretty(&config).map_err(|err| err.to_string())?;
        write_atomic(&paths.config_file, toml.as_bytes())?;
    }

    remove_legacy_default_profile(&paths)?;

    Ok(())
}

pub fn load_profile_summary() -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mut config = read_app_config()?;
    let drafts = load_profiles()?;
    let active_profiles_changed = clean_active_profiles(&mut config, &drafts)
        | sync_active_profiles_from_native_configs(&mut config, &drafts, &paths);
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
        profiles_dir: display_path(&paths.profiles_dir),
        backups_dir: display_path(&paths.backups_dir),
        active_profile,
        active_profile_name,
        active_profiles_by_mode: config.active_profiles_by_mode,
        drafts,
    })
}

pub fn load_app_settings() -> Result<AppSettings, String> {
    ensure_app_dirs()?;
    let config = read_app_config()?;
    Ok(settings_from_config(&config))
}

pub fn update_app_settings(request: UpdateAppSettingsRequest) -> Result<AppSettings, String> {
    ensure_app_dirs()?;
    let mut config = read_app_config()?;

    if let Some(theme) = request.theme {
        config.ui.theme = normalize_theme(&theme)?;
    }
    if let Some(language) = request.language {
        config.ui.language = normalize_language(&language)?;
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
        request.timeout_seconds,
    )?;
    ensure_profile_tool_installed(&plan.app)?;
    let now = Utc::now().to_rfc3339();

    let content = format!(
        r#"id = "{id}"
name = "{name}"
app = "{app}"
provider = "{provider}"
mode = "{mode}"
protocol = "{protocol}"
model = "{model}"
base_url = "{base_url}"
timeout_seconds = {timeout_seconds}

[auth]
api_key = "{auth_ref}"

[metadata]
created_at = "{now}"
updated_at = "{now}"
last_test_status = "pending"
secret_status = "{secret_status}"
"#,
        id = escape_toml_string(&plan.id),
        name = escape_toml_string(&plan.name),
        app = escape_toml_string(&plan.app),
        provider = escape_toml_string(&plan.provider),
        mode = provider_apply_mode_value(&plan.mode),
        protocol = escape_toml_string(&plan.protocol),
        model = escape_toml_string(&plan.model),
        base_url = escape_toml_string(&plan.base_url),
        timeout_seconds = plan.timeout_seconds,
        auth_ref = escape_toml_string(plan.auth_ref.as_deref().unwrap_or("")),
        now = escape_toml_string(&now),
        secret_status = plan.secret_status
    );

    write_atomic(&plan.profile_path, content.as_bytes())?;
    if let (Some(auth_ref), Some(api_key)) = (plan.auth_ref.as_deref(), request.api_key.as_deref())
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
            plan.name, plan.app, plan.provider
        ),
    )?;

    Ok(ProfileDraft {
        id: plan.id,
        name: plan.name,
        app: plan.app,
        is_builtin: false,
        mode: plan.mode,
        provider: plan.provider,
        protocol: plan.protocol,
        model: plan.model,
        base_url: plan.base_url,
        auth_ref: plan.auth_ref,
        timeout_seconds: plan.timeout_seconds,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        last_test_status: Some("pending".to_string()),
    })
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
    let paths = app_paths().map_err(|err| err.to_string())?;
    let profile_path = paths.profiles_dir.join(format!("{profile_id}.toml"));
    if !profile_path.exists() {
        return Err(format!("Profile '{profile_id}' does not exist"));
    }

    let name = normalize_required("Profile Name", &request.name)?;
    let provider = normalize_token("Provider", &request.provider)?;
    if provider_is_official(&provider) {
        return Err(
            "Official profiles are built in and cannot be saved as custom profiles.".to_string(),
        );
    }
    let mode = normalize_profile_mode(&provider, request.mode.as_ref())?;
    let protocol = normalize_protocol(request.protocol.as_deref())?;
    let app = canonical_profile_app(&existing.app);
    ensure_profile_protocol_supported_for_mode(&app, mode, &provider, &protocol)?;
    let model = request.model.trim().to_string();
    let base_url = validate_base_url_for_provider(&provider, &request.base_url)?;
    let timeout_seconds = normalize_timeout(request.timeout_seconds)?;
    let now = Utc::now().to_rfc3339();
    let created_at = existing.created_at.clone().unwrap_or_else(|| now.clone());
    let api_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let auth_ref = if api_key.is_some() {
        Some(
            existing
                .auth_ref
                .clone()
                .unwrap_or_else(|| format!("keychain:codestudio-lite/{profile_id}/api_key")),
        )
    } else {
        existing.auth_ref.clone()
    };
    let secret_status = if auth_ref.is_some() {
        "keychain_reference"
    } else {
        "missing"
    };
    if provider_requires_api_key(&provider) && auth_ref.is_none() {
        return Err("Provider API key is required for non-official providers.".to_string());
    }
    let updated = ProfileDraft {
        id: profile_id.clone(),
        name,
        app,
        is_builtin: false,
        mode,
        provider,
        protocol,
        model,
        base_url,
        auth_ref,
        timeout_seconds,
        created_at: Some(created_at.clone()),
        updated_at: Some(now.clone()),
        last_test_status: Some("pending".to_string()),
    };
    let content = profile_toml_content(&updated, &created_at, &now, "pending", secret_status);

    backup::backup_files("update-profile", Some(&profile_id), &[profile_path.clone()])?;
    write_atomic(&profile_path, content.as_bytes())?;
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
    let paths = app_paths().map_err(|err| err.to_string())?;
    let profile_path = paths.profiles_dir.join(format!("{new_id}.toml"));
    let now = Utc::now().to_rfc3339();
    let auth_ref = source
        .auth_ref
        .as_ref()
        .map(|_| format!("keychain:codestudio-lite/{new_id}/api_key"));

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
        name: source.name,
        app: canonical_profile_app(&source.app),
        is_builtin: false,
        mode: source.mode,
        provider: source.provider,
        protocol: source.protocol,
        model: source.model,
        base_url: source.base_url,
        auth_ref,
        timeout_seconds: source.timeout_seconds,
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        last_test_status: source.last_test_status,
    };
    let secret_status = if duplicated.auth_ref.is_some() {
        "keychain_reference"
    } else {
        "missing"
    };
    let last_test_status = duplicated
        .last_test_status
        .as_deref()
        .unwrap_or("pending")
        .to_string();
    let content = profile_toml_content(&duplicated, &now, &now, &last_test_status, secret_status);

    write_atomic(&profile_path, content.as_bytes())?;
    activity_log::append(
        Severity::Ok,
        format!(
            "Duplicated profile draft '{}' for {}/{}.",
            duplicated.name, duplicated.app, duplicated.provider
        ),
    )?;

    Ok(duplicated)
}

pub fn export_profiles() -> Result<ExportProfilesResult, String> {
    ensure_app_dirs()?;

    let mut config = read_app_config()?;
    let profiles = load_profiles()?;
    if clean_active_profiles(&mut config, &profiles) {
        write_app_config(&config)?;
    }

    let exported_at = Utc::now();
    let export_profiles = profiles
        .into_iter()
        .filter(|profile| !profile.is_builtin)
        .map(|mut profile| {
            profile.auth_ref = None;
            profile
        })
        .collect();
    let bundle = ProfileExportBundle {
        schema_version: 2,
        app: "CodeStudio Lite".to_string(),
        exported_at: exported_at.to_rfc3339(),
        active_profiles_by_mode: config.active_profiles_by_mode,
        profiles: export_profiles,
        warnings: vec![
            "Provider API keys are not exported. Imported profiles need their API key saved again before direct config file mode can use them."
                .to_string(),
            "Importing profiles does not automatically enable them for any tool.".to_string(),
        ],
    };

    activity_log::append(
        Severity::Info,
        format!("Exported {} profile draft(s).", bundle.profiles.len()),
    )?;

    Ok(ExportProfilesResult {
        file_name: format!(
            "codestudio-lite-profiles-{}.json",
            exported_at.format("%Y%m%d-%H%M%S")
        ),
        bundle,
    })
}

pub fn import_profiles(request: ImportProfilesRequest) -> Result<ImportProfilesResult, String> {
    ensure_app_dirs()?;

    let profiles = parse_profile_import_content(&request.content)?;
    if profiles.is_empty() {
        return Err("Import file does not contain any profiles.".to_string());
    }

    let paths = app_paths().map_err(|err| err.to_string())?;
    let installed_tool_ids = installed_profile_tool_ids()?;
    let now = Utc::now().to_rfc3339();
    let mut imported = Vec::new();
    let mut skipped = Vec::new();

    for (index, profile) in profiles.iter().enumerate() {
        let label = if profile.name.trim().is_empty() {
            format!("profile #{}", index + 1)
        } else {
            profile.name.trim().to_string()
        };

        match normalize_import_profile(profile, &now) {
            Ok(imported_profile) => {
                if !installed_tool_ids.contains(&canonical_profile_app(&imported_profile.app)) {
                    skipped.push(format!(
                        "{label}: {}",
                        profile_tool_not_installed_error(&imported_profile.app)
                    ));
                    continue;
                }
                let profile_path = paths
                    .profiles_dir
                    .join(format!("{}.toml", imported_profile.id));
                let created_at = imported_profile
                    .created_at
                    .as_deref()
                    .unwrap_or(now.as_str());
                let updated_at = imported_profile
                    .updated_at
                    .as_deref()
                    .unwrap_or(now.as_str());
                let content = profile_toml_content(
                    &imported_profile,
                    created_at,
                    updated_at,
                    imported_profile
                        .last_test_status
                        .as_deref()
                        .unwrap_or("pending"),
                    "missing",
                );
                write_atomic(&profile_path, content.as_bytes())?;
                imported.push(imported_profile);
            }
            Err(err) => skipped.push(format!("{label}: {err}")),
        }
    }

    activity_log::append(
        if imported.is_empty() {
            Severity::Warning
        } else {
            Severity::Ok
        },
        format!(
            "Imported {} profile draft(s); skipped {}.",
            imported.len(),
            skipped.len()
        ),
    )?;

    Ok(ImportProfilesResult {
        imported,
        skipped,
        summary: load_profile_summary()?,
    })
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
        request.timeout_seconds,
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
    let preview_profile = ProfileDraft {
        id: plan.id.clone(),
        name: plan.name.clone(),
        app: plan.app.clone(),
        is_builtin: false,
        mode: plan.mode,
        provider: plan.provider.clone(),
        protocol: plan.protocol.clone(),
        model: plan.model.clone(),
        base_url: plan.base_url.clone(),
        auth_ref: plan.auth_ref.clone(),
        timeout_seconds: plan.timeout_seconds,
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        last_test_status: Some("pending".to_string()),
    };
    let profile_content =
        profile_toml_content(&preview_profile, &now, &now, "pending", plan.secret_status);
    let mut items = vec![
        ProfileWritePreviewItem {
            label: "Profile draft".to_string(),
            path: Some(display_path(&plan.profile_path)),
            action: "create".to_string(),
            backup_required: false,
            detail: format!(
                "Save Profile Draft writes normalized metadata for {}/{} and excludes API keys.",
                plan.protocol, plan.provider
            ),
            content: Some(profile_content),
        },
        ProfileWritePreviewItem {
            label: "Active tool profile pointer".to_string(),
            path: Some(display_path(&paths.config_file)),
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
        profile_path: display_path(&plan.profile_path),
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
    let managed_apply_path = applied_profile_path(&profile.app)?;
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
    let native_diff = match profile.mode {
        ProviderApplyMode::Config => config_native_diff.clone(),
        ProviderApplyMode::Gateway => gateway_native_diff.clone(),
    };
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
                path: Some(display_path(&paths.config_file)),
                action: "update".to_string(),
                backup_required: true,
                detail: format!(
                    "Sets CodeStudio Lite active profile for '{}' to '{}' before refreshing detection.",
                    profile.app, profile.id
                ),
            },
            ProfileApplyPreviewItem {
                label: "Managed tool binding".to_string(),
                path: Some(display_path(&managed_apply_path)),
                action: if managed_apply_path.exists() {
                    "update".to_string()
                } else {
                    "create".to_string()
                },
                backup_required: true,
                detail: format!(
                    "Writes CodeStudio-managed adapter metadata for {}/{}. API keys are not written.",
                    profile.app, profile.provider
                ),
            },
            ProfileApplyPreviewItem {
                label: format!("{tool_name} native config"),
                path: native_config_path,
                action: if native_diff.is_some() {
                    "create_or_update".to_string()
                } else {
                    "not_modified".to_string()
                },
                backup_required: native_diff.is_some(),
                detail: if native_diff.is_some() {
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
    let gateway_writes_native_config = gateway_native_diff.is_some();
    let gateway_supported = !is_official;
    let config_blocked_reason = if !config_protocol_supported && !is_official {
        Some(format!(
            "Config file mode does not support {} for '{}'.",
            protocol_display_name(&profile.protocol),
            profile.app
        ))
    } else if !config_supported && !is_official {
        Some(format!(
            "Config file mode adapter is not implemented for '{}'.",
            profile.app
        ))
    } else if profile.auth_ref.is_none() && provider_requires_api_key(&profile.provider) {
        Some("Config file mode needs a stored Provider API key for this Provider.".to_string())
    } else {
        None
    };

    vec![
        ProviderApplyModePreview {
            mode: ProviderApplyMode::Config,
            label: "CC Switch config file mode".to_string(),
            description: "Back up and modify the target client's native provider config directly. This makes the client talk to the selected upstream Provider without CodeStudio Lite in the request path."
                .to_string(),
            supported: config_supported && config_blocked_reason.is_none(),
            recommended: is_official && config_supported && config_blocked_reason.is_none(),
            writes_native_config: config_native_diff.is_some(),
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
                    "Direct config file mode writes Provider connection details into the client config.".to_string(),
                    "Frequent Provider switching may require the client to reload its own config.".to_string(),
                ]
            } else {
                Vec::new()
            },
        },
        ProviderApplyModePreview {
            mode: ProviderApplyMode::Gateway,
            label: "Gateway mode".to_string(),
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
    let managed_apply_path = applied_profile_path(&profile.app)?;
    let mode = profile.mode;
    if request.restart_after_apply && mode != ProviderApplyMode::Config {
        return Err("Apply and restart is only available for Config file mode.".to_string());
    }
    let native_plans =
        build_native_apply_plan(&profile, &paths, &mode, request.sync_claude_vs_code)?;
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
    let mut backup_targets = vec![paths.config_file.clone(), managed_apply_path.clone()];
    for plan in &native_plans {
        backup_targets.push(plan.path.clone());
    }
    let backup = backup::backup_files("apply-profile", Some(&profile.id), &backup_targets)?;

    activate_profile_for_tool(&mut config, &profile, &profiles);
    write_app_config(&config)?;

    let applied_content = applied_profile_content(&profile, &mode);
    write_atomic(&managed_apply_path, applied_content.as_bytes())?;
    let verified = verify_applied_profile(&managed_apply_path, &profile)?;
    if !verified {
        return Err("Applied profile artifact did not pass verification".to_string());
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
        restart_tool_for_profile(&profile)?
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
                "Applied profile '{}' for {}/{} in Gateway mode.",
                profile.name, profile.app, profile.provider
            )
        } else if native_verified && mode == ProviderApplyMode::Config {
            format!(
                "Applied profile '{}' for {}/{} through direct client config file mode.",
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
        applied_path: display_path(&managed_apply_path),
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

        match build_native_apply_plan(&profile, &paths, &mode, false) {
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

#[derive(Clone, Copy)]
enum RestartLaunch {
    CodexClient,
    Command {
        command: &'static str,
        hidden: bool,
    },
    ExistingProcessPath {
        fallback_command: &'static str,
        hidden: bool,
    },
}

#[derive(Clone, Copy)]
struct RestartTarget {
    label: &'static str,
    process_names: &'static [&'static str],
    command_markers: &'static [&'static str],
    require_window: bool,
    launch: RestartLaunch,
}

fn restart_tool_for_profile(profile: &ProfileDraft) -> Result<RestartOutcome, String> {
    let app = canonical_profile_app(&profile.app);
    let targets = restart_targets_for_app(&app);
    if targets.is_empty() {
        return Ok(RestartOutcome {
            performed: false,
            message: Some(format!("工具 '{}' 暂无需要自动重启的客户端。", profile.app)),
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
            return Err(format!("仍有 {} 进程无法结束，未继续重启。", target.label));
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
                "未检测到正在运行的 {}，无需重启。",
                restart_category_label(&app)
            )),
        })
    }
}

fn restart_targets_for_app(app: &str) -> Vec<RestartTarget> {
    const CODEX_DESKTOP_NAMES: &[&str] = &["Codex.exe", "Codex"];
    const CODEX_CLI_MARKERS: &[&str] = &["@openai/codex", "@openai\\codex"];
    const VSCODE_NAMES: &[&str] = &["Code.exe", "Code", "Code - Insiders.exe", "Code - Insiders"];
    const CLAUDE_DESKTOP_NAMES: &[&str] = &["Claude.exe", "Claude"];
    const CLAUDE_CLI_MARKERS: &[&str] =
        &["@anthropic-ai/claude-code", "@anthropic-ai\\claude-code"];
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
                label: "Codex 客户端",
                process_names: CODEX_DESKTOP_NAMES,
                command_markers: EMPTY,
                require_window: true,
                launch: RestartLaunch::CodexClient,
            },
            RestartTarget {
                label: "Codex VS Code",
                process_names: VSCODE_NAMES,
                command_markers: EMPTY,
                require_window: true,
                launch: RestartLaunch::ExistingProcessPath {
                    fallback_command: "code",
                    hidden: false,
                },
            },
            RestartTarget {
                label: "Codex CLI",
                process_names: EMPTY,
                command_markers: CODEX_CLI_MARKERS,
                require_window: false,
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
            require_window: true,
            launch: RestartLaunch::ExistingProcessPath {
                fallback_command: "Claude",
                hidden: false,
            },
        }],
        "claude" => vec![
            RestartTarget {
                label: "Claude VS Code",
                process_names: VSCODE_NAMES,
                command_markers: EMPTY,
                require_window: true,
                launch: RestartLaunch::ExistingProcessPath {
                    fallback_command: "code",
                    hidden: false,
                },
            },
            RestartTarget {
                label: "Claude Code",
                process_names: EMPTY,
                command_markers: CLAUDE_CLI_MARKERS,
                require_window: false,
                launch: RestartLaunch::Command {
                    command: "claude",
                    hidden: true,
                },
            },
        ],
        "gemini" => vec![RestartTarget {
            label: "Gemini CLI",
            process_names: GEMINI_CLI_NAMES,
            command_markers: GEMINI_CLI_MARKERS,
            require_window: false,
            launch: RestartLaunch::Command {
                command: "gemini",
                hidden: true,
            },
        }],
        "gemini-code-assist" => vec![RestartTarget {
            label: "Gemini Code Assist",
            process_names: VSCODE_NAMES,
            command_markers: EMPTY,
            require_window: true,
            launch: RestartLaunch::ExistingProcessPath {
                fallback_command: "code",
                hidden: false,
            },
        }],
        "opencode" => vec![RestartTarget {
            label: "OpenCode",
            process_names: OPENCODE_NAMES,
            command_markers: OPENCODE_MARKERS,
            require_window: false,
            launch: RestartLaunch::Command {
                command: "opencode",
                hidden: true,
            },
        }],
        "openclaw" => vec![RestartTarget {
            label: "OpenClaw",
            process_names: OPENCLAW_NAMES,
            command_markers: EMPTY,
            require_window: false,
            launch: RestartLaunch::Command {
                command: "openclaw",
                hidden: true,
            },
        }],
        "hermes" => vec![RestartTarget {
            label: "Hermes",
            process_names: HERMES_NAMES,
            command_markers: EMPTY,
            require_window: false,
            launch: RestartLaunch::Command {
                command: "hermes",
                hidden: true,
            },
        }],
        _ => Vec::new(),
    }
}

fn restart_category_label(app: &str) -> &'static str {
    match app {
        "codex" => "Codex 客户端、Codex CLI 或 Codex VS Code",
        "claude-desktop" => "Claude Desktop",
        "claude" => "Claude Code 或 Claude VS Code",
        "gemini" => "Gemini CLI",
        "gemini-code-assist" => "Gemini Code Assist",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        _ => "目标工具",
    }
}

fn restart_target_message(target: RestartTarget, result: &RestartProcessResult) -> String {
    if result.forced > 0 {
        format!(
            "已强制结束 {} 个 {} 进程并重新启动。",
            result.forced, target.label
        )
    } else {
        format!("已重新启动 {}。", target.label)
    }
}

fn stop_restart_target_processes(target: RestartTarget) -> Result<RestartProcessResult, String> {
    if cfg!(target_os = "macos") {
        let report = process_control::close_processes(
            target.label,
            target.process_names,
            target.command_markers,
            None,
            8,
        )?;
        return Ok(RestartProcessResult {
            total: report.total,
            forced: report.forced,
            remaining: report.remaining,
            paths: Vec::new(),
        });
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
$RequireWindow = ${require_window}
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
  if ($RequireWindow) {{
    try {{
      $gp = Get-Process -Id $process.ProcessId -ErrorAction Stop
      if ($gp.MainWindowHandle -eq 0) {{ return $false }}
    }} catch {{
      return $false
    }}
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
        require_window = if target.require_window {
            "true"
        } else {
            "false"
        }
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
        .map_err(|err| format!("解析 {} 进程重启结果失败：{err}", target.label))?;
    Ok(RestartProcessResult {
        total: value.total.unwrap_or(0),
        forced: value.forced.unwrap_or(0),
        remaining: value.remaining.unwrap_or(0),
        paths: value.paths,
    })
}

fn launch_restart_target(target: RestartTarget, paths: &[String]) -> Result<(), String> {
    match target.launch {
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
    }
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
            if path.exists() {
                command.arg(program);
            } else {
                command.args(["-a", program]);
            }
            return command
                .spawn()
                .map(|_| ())
                .map_err(|err| format!("启动 {program} 失败：{err}"));
        }
    }

    let resolved = resolve_command(program).unwrap_or_else(|| program.to_string());
    hidden_command(&resolved)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("启动 {program} 失败：{err}"))
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

fn sync_active_profiles_from_native_configs(
    config: &mut AppConfig,
    drafts: &[ProfileDraft],
    paths: &crate::core::app_paths::AppPaths,
) -> bool {
    let mut changed = false;

    let codex_config_path = paths.home_dir.join(".codex").join("config.toml");
    if let Ok(content) = fs::read_to_string(codex_config_path) {
        if let Ok(codex_config) = toml::from_str::<toml::Value>(&content) {
            changed |= sync_codex_config_profile(config, drafts, &codex_config);
        }
    }

    let claude_config_path = paths.home_dir.join(".claude").join("settings.json");
    if let Ok(content) = fs::read_to_string(claude_config_path) {
        if let Ok(claude_config) = parse_json5_or_empty(&content, "Claude settings") {
            changed |= sync_native_config_profile(config, drafts, "claude", |profile| {
                claude_config_matches_profile(&claude_config, profile)
            });
        }
    }

    let gemini_env_path = paths.home_dir.join(".gemini").join(".env");
    if let Ok(content) = fs::read_to_string(gemini_env_path) {
        let env = parse_env_content(&content);
        changed |= sync_native_config_profile(config, drafts, "gemini", |profile| {
            gemini_env_matches_profile(&env, profile)
        });
    }

    let gemini_code_assist_settings_path = vs_code_user_settings_path(paths);
    if let Ok(content) = fs::read_to_string(gemini_code_assist_settings_path) {
        if let Ok(settings) = parse_json5_or_empty(&content, "VS Code user settings") {
            changed |=
                sync_native_config_profile(config, drafts, "gemini-code-assist", |profile| {
                    gemini_code_assist_settings_match_profile(&settings, profile)
                });
        }
    }

    let opencode_config_path = paths
        .home_dir
        .join(".config")
        .join("opencode")
        .join("opencode.json");
    if let Ok(content) = fs::read_to_string(opencode_config_path) {
        if let Ok(opencode_config) = parse_json5_or_empty(&content, "OpenCode config") {
            changed |= sync_native_config_profile(config, drafts, "opencode", |profile| {
                opencode_config_matches_profile(&opencode_config, profile)
            });
        }
    }

    let openclaw_config_path = paths.home_dir.join(".openclaw").join("openclaw.json");
    if let Ok(content) = fs::read_to_string(openclaw_config_path) {
        if let Ok(openclaw_config) = parse_json5_or_empty(&content, "OpenClaw config") {
            changed |= sync_native_config_profile(config, drafts, "openclaw", |profile| {
                openclaw_config_matches_profile(&openclaw_config, profile)
            });
        }
    }

    let hermes_config_path = paths.home_dir.join(".hermes").join("config.yaml");
    if let Ok(content) = fs::read_to_string(hermes_config_path) {
        if let Ok(hermes_config) = parse_yaml_or_empty(&content, "Hermes config") {
            changed |= sync_native_config_profile(config, drafts, "hermes", |profile| {
                hermes_config_matches_profile(&hermes_config, profile)
            });
        }
    }

    changed
}

fn sync_codex_config_profile(
    config: &mut AppConfig,
    drafts: &[ProfileDraft],
    codex_config: &toml::Value,
) -> bool {
    sync_native_config_profile(config, drafts, "codex", |profile| {
        codex_direct_config_matches_profile(codex_config, profile)
    })
}

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
    let matching_profiles = drafts
        .iter()
        .filter(|profile| {
            canonical_profile_app(&profile.app) == app
                && profile.mode == ProviderApplyMode::Config
                && matches_profile(profile)
        })
        .collect::<Vec<_>>();

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

fn codex_direct_config_matches_profile(value: &toml::Value, profile: &ProfileDraft) -> bool {
    if !is_codex_family_app(&profile.app) || profile.mode != ProviderApplyMode::Config {
        return false;
    }

    if provider_is_official(&profile.provider) {
        return codex_official_config_matches_profile(value, profile);
    }

    let provider_id = codex_provider_id_for_profile(profile);
    let model = if profile.model.trim().is_empty() {
        "codestudio-default"
    } else {
        profile.model.trim()
    };
    let Ok(wire_api) = codex_wire_api_for_protocol(&profile.protocol) else {
        return false;
    };
    let token_matches = toml_lookup(
        value,
        &format!("model_providers.{provider_id}.experimental_bearer_token"),
    )
    .and_then(|item| item.as_str())
    .map(|token| profile_api_key_matches_config(profile, token))
    .unwrap_or(false);

    read_toml_string(value, "model_provider").as_deref() == Some(provider_id.as_str())
        && read_toml_string(value, "model").as_deref() == Some(model)
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
            == Some(false)
        && token_matches
}

fn codex_official_config_matches_profile(value: &toml::Value, profile: &ProfileDraft) -> bool {
    let Ok(wire_api) = codex_wire_api_for_protocol(&profile.protocol) else {
        return false;
    };
    let model_matches = if profile.model.trim().is_empty() {
        read_toml_string(value, "model").is_none()
    } else {
        read_toml_string(value, "model").as_deref() == Some(profile.model.trim())
    };
    let token_is_absent = toml_lookup(value, "model_providers.openai.experimental_bearer_token")
        .and_then(|item| item.as_str())
        .map(|token| token.trim().is_empty())
        .unwrap_or(true);
    let base_url_is_absent = toml_lookup(value, "model_providers.openai.base_url")
        .and_then(|item| item.as_str())
        .map(|base_url| base_url.trim().is_empty())
        .unwrap_or(true);

    read_toml_string(value, "model_provider").as_deref() == Some("openai")
        && model_matches
        && toml_lookup(value, "model_providers.openai.wire_api").and_then(|item| item.as_str())
            == Some(wire_api)
        && toml_lookup(value, "model_providers.openai.requires_openai_auth")
            .and_then(|item| item.as_bool())
            == Some(true)
        && token_is_absent
        && base_url_is_absent
}

fn claude_config_matches_profile(value: &serde_json::Value, profile: &ProfileDraft) -> bool {
    if canonical_profile_app(&profile.app) != "claude"
        || profile.mode != ProviderApplyMode::Config
        || provider_is_official(&profile.provider)
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
        .map(|token| profile_api_key_matches_config(profile, &token))
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
    if canonical_profile_app(&profile.app) != "gemini"
        || profile.mode != ProviderApplyMode::Config
        || provider_is_official(&profile.provider)
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
        .map(|token| profile_api_key_matches_config(profile, token))
        .unwrap_or(false);

    env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str) == Some(profile.base_url.trim())
        && token_matches
        && model_matches
}

fn gemini_code_assist_settings_match_profile(
    value: &serde_json::Value,
    profile: &ProfileDraft,
) -> bool {
    if canonical_profile_app(&profile.app) != "gemini-code-assist"
        || profile.mode != ProviderApplyMode::Config
        || provider_is_official(&profile.provider)
        || normalize_protocol(Some(&profile.protocol)).as_deref() != Ok(PROTOCOL_GOOGLE_GEMINI)
    {
        return false;
    }

    json_string_lookup(value, &[GEMINI_CODE_ASSIST_API_KEY_SETTING])
        .map(|token| profile_api_key_matches_config(profile, &token))
        .unwrap_or(false)
}

fn opencode_config_matches_profile(value: &serde_json::Value, profile: &ProfileDraft) -> bool {
    if canonical_profile_app(&profile.app) != "opencode"
        || profile.mode != ProviderApplyMode::Config
        || provider_is_official(&profile.provider)
        || !matches!(
            normalize_protocol(Some(&profile.protocol)).as_deref(),
            Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS) | Ok(PROTOCOL_OPENAI_RESPONSES)
        )
    {
        return false;
    }

    let provider_id = managed_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => json_string_lookup(value, &["model"]).as_deref() == Some(model),
        None => json_string_lookup(value, &["model"]).is_none(),
    };
    let token_matches = json_string_lookup(value, &["provider", &provider_id, "options", "apiKey"])
        .map(|token| profile_api_key_matches_config(profile, &token))
        .unwrap_or(false);

    json_string_lookup(value, &["provider", &provider_id, "options", "baseURL"]).as_deref()
        == Some(profile.base_url.trim())
        && token_matches
        && model_matches
}

fn openclaw_config_matches_profile(value: &serde_json::Value, profile: &ProfileDraft) -> bool {
    if canonical_profile_app(&profile.app) != "openclaw"
        || profile.mode != ProviderApplyMode::Config
        || provider_is_official(&profile.provider)
        || normalize_protocol(Some(&profile.protocol)).as_deref()
            != Ok(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
    {
        return false;
    }

    let provider_id = managed_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => {
            json_string_lookup(value, &["agents", "defaults", "model", "primary"]).as_deref()
                == Some(model)
        }
        None => true,
    };
    let token_matches = json_string_lookup(value, &["models", "providers", &provider_id, "apiKey"])
        .map(|token| profile_api_key_matches_config(profile, &token))
        .unwrap_or(false);

    json_string_lookup(value, &["models", "providers", &provider_id, "baseUrl"]).as_deref()
        == Some(profile.base_url.trim())
        && token_matches
        && model_matches
}

fn hermes_config_matches_profile(value: &serde_norway::Value, profile: &ProfileDraft) -> bool {
    if canonical_profile_app(&profile.app) != "hermes"
        || profile.mode != ProviderApplyMode::Config
        || provider_is_official(&profile.provider)
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
        .map(|token| profile_api_key_matches_config(profile, &token))
        .unwrap_or(false);

    yaml_string_lookup(value, &["model", "provider"]).as_deref() == Some("custom")
        && yaml_string_lookup(value, &["model", "base_url"]).as_deref()
            == Some(profile.base_url.trim())
        && yaml_string_lookup(value, &["model", "api_mode"]).as_deref() == Some("chat_completions")
        && token_matches
        && model_matches
}

fn profile_api_key_matches_config(profile: &ProfileDraft, token: &str) -> bool {
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
    let provider = normalize_token("Provider", &request.provider)?;
    let protocol = normalize_protocol(request.protocol.as_deref())?;
    let model = request.model.trim().to_string();
    let base_url = validate_base_url_for_provider(&provider, &request.base_url)?;
    let timeout_seconds = normalize_timeout(request.timeout_seconds)?;
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
        detail: format!(
            "Network provider checks are not sent yet. Timeout is set to {timeout_seconds}s."
        ),
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

    let paths = app_paths().map_err(|err| err.to_string())?;
    let mut config = read_app_config()?;
    backup::backup_files(
        "switch-profile",
        Some(&profile.id),
        &[paths.config_file.clone()],
    )?;
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

fn remove_legacy_default_profile(paths: &crate::core::app_paths::AppPaths) -> Result<(), String> {
    let sample_profile = paths.profiles_dir.join("codex-openai.toml");
    if !sample_profile.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&sample_profile).map_err(|err| err.to_string())?;
    if content.contains("id = \"codex-openai\"")
        && content.contains("name = \"OpenAI GPT Gateway\"")
    {
        fs::remove_file(&sample_profile).map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn read_app_config() -> Result<AppConfig, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    let content = fs::read_to_string(paths.config_file).map_err(|err| err.to_string())?;
    toml::from_str(&content).map_err(|err| err.to_string())
}

fn write_app_config(config: &AppConfig) -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    let updated_config = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    write_atomic(&paths.config_file, updated_config.as_bytes())
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

fn parse_profile_import_content(content: &str) -> Result<Vec<ProfileDraft>, String> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|err| format!("Import file is not valid JSON: {err}"))?;
    let profiles_value = if value.is_array() {
        value
    } else if let Some(profiles) = value.get("profiles") {
        profiles.clone()
    } else if let Some(profiles) = value.get("drafts") {
        profiles.clone()
    } else if let Some(profiles) = value
        .get("bundle")
        .and_then(|bundle| bundle.get("profiles"))
    {
        profiles.clone()
    } else {
        return Err("Import file must contain a profiles array.".to_string());
    };

    if !profiles_value.is_array() {
        return Err("Import file field 'profiles' must be an array.".to_string());
    }

    serde_json::from_value::<Vec<ProfileDraft>>(profiles_value)
        .map_err(|err| format!("Profiles array could not be parsed: {err}"))
}

fn normalize_import_profile(profile: &ProfileDraft, now: &str) -> Result<ProfileDraft, String> {
    if profile.is_builtin {
        return Err("Built-in official profiles cannot be imported.".to_string());
    }
    let name = normalize_required("Profile Name", &profile.name)?;
    let app = canonical_profile_app(&normalize_token("Client", &profile.app)?);
    let provider = normalize_token("Provider", &profile.provider)?;
    if provider_is_official(&provider) {
        return Err("Official profiles are built in and cannot be imported.".to_string());
    }
    let mode = normalize_profile_mode(&provider, Some(&profile.mode))?;
    let protocol = normalize_protocol(Some(&profile.protocol))?;
    ensure_profile_protocol_supported_for_mode(&app, mode, &provider, &protocol)?;
    let model = profile.model.trim().to_string();
    let base_url = validate_base_url_for_provider(&provider, &profile.base_url)?;
    let timeout_seconds = normalize_timeout(Some(profile.timeout_seconds))?;
    let preferred_id = slugify(&profile.id);
    let fallback_id = slugify(&name);
    if is_builtin_profile_id(&preferred_id) || is_builtin_profile_id(&fallback_id) {
        return Err("Built-in official profile IDs are reserved.".to_string());
    }
    let id = unique_profile_id(if preferred_id.is_empty() {
        &fallback_id
    } else {
        &preferred_id
    })?;
    let created_at = profile
        .created_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(now)
        .to_string();

    Ok(ProfileDraft {
        id,
        name,
        app,
        is_builtin: false,
        mode,
        provider,
        protocol,
        model,
        base_url,
        auth_ref: None,
        timeout_seconds,
        created_at: Some(created_at),
        updated_at: Some(now.to_string()),
        last_test_status: Some("pending".to_string()),
    })
}

fn load_profiles() -> Result<Vec<ProfileDraft>, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mut profiles = builtin_official_profiles();

    for entry in fs::read_dir(paths.profiles_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }

        let content = fs::read_to_string(&path).map_err(|err| err.to_string())?;
        let value: toml::Value = toml::from_str(&content).map_err(|err| err.to_string())?;
        let app = read_toml_string(&value, "app").unwrap_or_else(|| "unknown".to_string());
        let provider =
            read_toml_string(&value, "provider").unwrap_or_else(|| "unknown".to_string());
        let id = read_toml_string(&value, "id").unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("profile")
                .to_string()
        });
        if provider_is_official(&provider) || is_builtin_profile_id(&id) {
            continue;
        }
        let mode = normalize_stored_profile_mode(&provider, read_toml_string(&value, "mode"));
        profiles.push(ProfileDraft {
            id,
            name: read_toml_string(&value, "name")
                .unwrap_or_else(|| "Untitled Profile".to_string()),
            app: canonical_profile_app(&app),
            is_builtin: false,
            mode,
            provider,
            protocol: normalize_protocol(read_toml_string(&value, "protocol").as_deref())?,
            model: read_toml_string(&value, "model").unwrap_or_else(|| "manual".to_string()),
            base_url: read_toml_string(&value, "base_url").unwrap_or_default(),
            auth_ref: read_toml_string_nested(&value, "auth", "api_key")
                .or_else(|| read_toml_string_nested(&value, "env", "OPENAI_API_KEY"))
                .and_then(normalize_auth_ref),
            timeout_seconds: read_toml_u16(&value, "timeout_seconds")
                .or_else(|| read_toml_u16_nested(&value, "limits", "timeout_seconds"))
                .unwrap_or(120),
            created_at: read_toml_string_nested(&value, "metadata", "created_at"),
            updated_at: read_toml_string_nested(&value, "metadata", "updated_at"),
            last_test_status: read_toml_string_nested(&value, "metadata", "last_test_status"),
        });
    }

    profiles.sort_by(compare_profiles);
    Ok(profiles)
}

fn builtin_official_profiles() -> Vec<ProfileDraft> {
    BUILTIN_OFFICIAL_PROFILES
        .iter()
        .map(|(app, name, protocol)| ProfileDraft {
            id: builtin_official_profile_id(app),
            name: (*name).to_string(),
            app: (*app).to_string(),
            is_builtin: true,
            mode: ProviderApplyMode::Config,
            provider: "official".to_string(),
            protocol: (*protocol).to_string(),
            model: String::new(),
            base_url: String::new(),
            auth_ref: None,
            timeout_seconds: 120,
            created_at: None,
            updated_at: None,
            last_test_status: Some("builtin".to_string()),
        })
        .collect()
}

fn builtin_official_profile_id(app: &str) -> String {
    format!("{BUILTIN_OFFICIAL_ID_PREFIX}{}", canonical_profile_app(app))
}

fn is_builtin_profile_id(id: &str) -> bool {
    id.starts_with(BUILTIN_OFFICIAL_ID_PREFIX)
}

fn compare_profiles(left: &ProfileDraft, right: &ProfileDraft) -> std::cmp::Ordering {
    left.app
        .cmp(&right.app)
        .then_with(|| right.is_builtin.cmp(&left.is_builtin))
        .then_with(|| left.name.cmp(&right.name))
}

fn load_profile_by_id(profile_id: &str) -> Result<ProfileDraft, String> {
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

fn read_toml_string_nested(value: &toml::Value, table: &str, key: &str) -> Option<String> {
    value
        .get(table)
        .and_then(|item| item.get(key))
        .and_then(|item| item.as_str())
        .map(ToString::to_string)
}

fn normalize_auth_ref(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "keychain:codestudio-lite/pending/api_key" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn read_toml_u16(value: &toml::Value, key: &str) -> Option<u16> {
    value
        .get(key)
        .and_then(|item| item.as_integer())
        .and_then(|item| u16::try_from(item).ok())
}

fn read_toml_u16_nested(value: &toml::Value, table: &str, key: &str) -> Option<u16> {
    value
        .get(table)
        .and_then(|item| item.get(key))
        .and_then(|item| item.as_integer())
        .and_then(|item| u16::try_from(item).ok())
}

fn normalize_required(label: &str, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(trimmed.to_string())
    }
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

fn validate_base_url(value: &str) -> Result<String, String> {
    let trimmed = normalize_required("Base URL", value)?;
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return Err("Base URL must start with http:// or https://".to_string());
    };
    if !matches!(scheme, "http" | "https") {
        return Err("Base URL must start with http:// or https://".to_string());
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err("Base URL cannot contain whitespace".to_string());
    }

    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() || host.starts_with('.') {
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

fn normalize_timeout(value: Option<u16>) -> Result<u16, String> {
    let timeout = value.unwrap_or(120);
    if (5..=600).contains(&timeout) {
        Ok(timeout)
    } else {
        Err("Timeout must be between 5 and 600 seconds.".to_string())
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
            "Official provider uses the client login directly and cannot use Gateway mode."
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

fn build_profile_write_plan(
    name: &str,
    app: &str,
    mode: Option<&ProviderApplyMode>,
    provider: &str,
    protocol: Option<&str>,
    model: &str,
    base_url: &str,
    secret_provided: bool,
    timeout_seconds: Option<u16>,
) -> Result<ProfileWritePlan, String> {
    let name = normalize_required("Profile Name", name)?;
    let app = canonical_profile_app(&normalize_token("Client", app)?);
    let provider = normalize_token("Provider", provider)?;
    if provider_is_official(&provider) {
        return Err(
            "Official profiles are built in and cannot be saved as custom profiles.".to_string(),
        );
    }
    let mode = normalize_profile_mode(&provider, mode)?;
    let protocol = normalize_protocol(protocol)?;
    ensure_profile_protocol_supported_for_mode(&app, mode, &provider, &protocol)?;
    let model = model.trim().to_string();
    let base_url = validate_base_url_for_provider(&provider, base_url)?;
    let timeout_seconds = normalize_timeout(timeout_seconds)?;
    if provider_requires_api_key(&provider) && !secret_provided {
        return Err("Provider API key is required for non-official providers.".to_string());
    }
    let id = unique_profile_id(&slugify(&name))?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let profile_path = paths.profiles_dir.join(format!("{id}.toml"));
    let secret_status = if secret_provided {
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
        timeout_seconds,
        profile_path,
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
    Ok(detector::detect_environment()?
        .tools
        .into_iter()
        .filter(|tool| tool.install_state == InstallState::Installed)
        .map(|tool| canonical_profile_app(&tool.id))
        .collect())
}

fn profile_tool_not_installed_error(app: &str) -> String {
    format!(
        "Tool '{}' is not installed, so a profile cannot be created for it.",
        canonical_profile_app(app)
    )
}

fn profile_toml_content(
    profile: &ProfileDraft,
    created_at: &str,
    updated_at: &str,
    last_test_status: &str,
    secret_status: &str,
) -> String {
    format!(
        r#"id = "{id}"
name = "{name}"
app = "{app}"
provider = "{provider}"
mode = "{mode}"
protocol = "{protocol}"
model = "{model}"
base_url = "{base_url}"
timeout_seconds = {timeout_seconds}

[auth]
api_key = "{auth_ref}"

[metadata]
created_at = "{created_at}"
updated_at = "{updated_at}"
last_test_status = "{last_test_status}"
secret_status = "{secret_status}"
"#,
        id = escape_toml_string(&profile.id),
        name = escape_toml_string(&profile.name),
        app = escape_toml_string(&profile.app),
        provider = escape_toml_string(&profile.provider),
        mode = provider_apply_mode_value(&profile.mode),
        protocol = escape_toml_string(&profile.protocol),
        model = escape_toml_string(&profile.model),
        base_url = escape_toml_string(&profile.base_url),
        timeout_seconds = profile.timeout_seconds,
        auth_ref = escape_toml_string(profile.auth_ref.as_deref().unwrap_or("")),
        created_at = escape_toml_string(created_at),
        updated_at = escape_toml_string(updated_at),
        last_test_status = escape_toml_string(last_test_status),
        secret_status = escape_toml_string(secret_status)
    )
}

fn applied_profile_path(app: &str) -> Result<std::path::PathBuf, String> {
    let app = canonical_profile_app(&normalize_token("Tool", app)?);
    let paths = app_paths().map_err(|err| err.to_string())?;
    Ok(paths.applied_dir.join(format!("{app}-active.toml")))
}

fn applied_profile_content(profile: &ProfileDraft, mode: &ProviderApplyMode) -> String {
    let now = Utc::now().to_rfc3339();
    let mode_value = provider_apply_mode_value(mode);
    let native_config_write = match mode {
        ProviderApplyMode::Config => "direct_provider_config",
        ProviderApplyMode::Gateway => "local_gateway_relay",
    };
    format!(
        r#"profile_id = "{profile_id}"
profile_name = "{profile_name}"
app = "{app}"
provider = "{provider}"
model = "{model}"
base_url = "{base_url}"
protocol = "{protocol}"
timeout_seconds = {timeout_seconds}
apply_mode = "{mode_value}"
native_config_write = "{native_config_write}"
secret_policy = "never_write_plaintext"
applied_at = "{now}"
"#,
        profile_id = escape_toml_string(&profile.id),
        profile_name = escape_toml_string(&profile.name),
        app = escape_toml_string(&profile.app),
        provider = escape_toml_string(&profile.provider),
        protocol = escape_toml_string(&profile.protocol),
        model = escape_toml_string(&profile.model),
        base_url = escape_toml_string(&profile.base_url),
        timeout_seconds = profile.timeout_seconds,
        mode_value = mode_value,
        native_config_write = native_config_write,
        now = escape_toml_string(&now)
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
        return Ok(None);
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
            "claude" => claude_config_content(&current, profile)?,
            "gemini" => gemini_env_content(&current, profile)?,
            "gemini-code-assist" => gemini_code_assist_settings_content(&current, profile)?,
            "opencode" => opencode_config_content(&current, profile)?,
            "openclaw" => openclaw_config_content(&current, profile)?,
            "hermes" => hermes_config_content(&current, profile)?,
            _ => {
                return Err(format!(
                    "Config file mode is not implemented for tool '{}'.",
                    profile.app
                ))
            }
        },
        ProviderApplyMode::Gateway => match canonical_profile_app(&profile.app).as_str() {
            "codex" => codex_native_config_content(&current, &profile.app)?,
            "claude" => claude_gateway_config_content(&current, profile)?,
            "gemini" => gemini_gateway_env_content(&current, profile)?,
            "opencode" => opencode_gateway_config_content(&current, profile)?,
            "openclaw" => openclaw_gateway_config_content(&current, profile)?,
            "hermes" => hermes_gateway_config_content(&current, profile)?,
            _ => {
                return Err(format!(
                    "Gateway mode adapter is not implemented for tool '{}'.",
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

pub(crate) fn codex_native_config_content(current: &str, tool_id: &str) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| format!("Existing Codex config could not be parsed: {err}"))?;
    let provider_id = client.provider_id;
    let provider_name = client.provider_name;

    document["model_provider"] = toml_edit::value(provider_id.clone());
    document["model"] = toml_edit::value(client.model);
    document["model_providers"][&provider_id]["name"] = toml_edit::value(provider_name);
    document["model_providers"][&provider_id]["wire_api"] = toml_edit::value("responses");
    document["model_providers"][&provider_id]["base_url"] = toml_edit::value(client.base_url);
    document["model_providers"][&provider_id]["requires_openai_auth"] = toml_edit::value(true);
    document["model_providers"][&provider_id]["experimental_bearer_token"] =
        toml_edit::value(client.token);

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
    let provider_id = codex_provider_id_for_profile(profile);
    let provider_name = format!("CodeStudio {}", profile.provider);
    let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;
    let model = if profile.model.trim().is_empty() {
        "codestudio-default"
    } else {
        profile.model.trim()
    };
    let api_key = load_provider_api_key_for_direct_config(profile)?;

    document["model_provider"] = toml_edit::value(provider_id.clone());
    document["model"] = toml_edit::value(model);
    document["model_providers"][&provider_id]["name"] = toml_edit::value(provider_name);
    document["model_providers"][&provider_id]["wire_api"] = toml_edit::value(wire_api);
    document["model_providers"][&provider_id]["base_url"] =
        toml_edit::value(profile.base_url.trim().to_string());
    document["model_providers"][&provider_id]["requires_openai_auth"] = toml_edit::value(false);
    document["model_providers"][&provider_id]["experimental_bearer_token"] =
        toml_edit::value(api_key);

    let updated = document.to_string();
    toml::from_str::<toml::Value>(&updated)
        .map_err(|err| format!("Generated Codex config is invalid: {err}"))?;
    Ok(updated)
}

fn codex_official_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let mut document = current
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| format!("Existing Codex config could not be parsed: {err}"))?;
    let provider_id = "openai";
    let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;

    document["model_provider"] = toml_edit::value(provider_id);
    if profile.model.trim().is_empty() {
        document.remove("model");
    } else {
        document["model"] = toml_edit::value(profile.model.trim());
    }
    document["model_providers"][provider_id]["name"] = toml_edit::value("OpenAI");
    document["model_providers"][provider_id]["wire_api"] = toml_edit::value(wire_api);
    document["model_providers"][provider_id]["requires_openai_auth"] = toml_edit::value(true);
    if let Some(table) = document["model_providers"][provider_id].as_table_like_mut() {
        table.remove("base_url");
        table.remove("experimental_bearer_token");
    }

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
    require_profile_protocol(profile, &[PROTOCOL_ANTHROPIC_MESSAGES])?;
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    let model_specs = claude_desktop_direct_inference_models(profile);
    let value = claude_desktop_gateway_profile_value(
        profile.base_url.trim(),
        &api_key,
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
    require_profile_protocol(profile, &[PROTOCOL_ANTHROPIC_MESSAGES])?;
    let mut value = parse_json5_or_empty(current, "Claude settings")?;
    let api_key = load_provider_api_key_for_direct_config(profile)?;

    set_json_string_path(
        &mut value,
        &["env", "ANTHROPIC_BASE_URL"],
        profile.base_url.trim(),
    );
    set_json_string_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"], &api_key);
    if let Some(model) = profile_model(profile) {
        set_json_string_path(&mut value, &["model"], model);
        set_json_string_path(&mut value, &["env", "ANTHROPIC_MODEL"], model);
    } else {
        remove_json_path(&mut value, &["model"]);
        remove_json_path(&mut value, &["env", "ANTHROPIC_MODEL"]);
    }

    render_json_config(value, "Claude settings")
}

fn claude_gateway_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "Claude settings")?;

    set_json_string_path(&mut value, &["env", "ANTHROPIC_BASE_URL"], &client.base_url);
    set_json_string_path(&mut value, &["env", "ANTHROPIC_AUTH_TOKEN"], &client.token);
    set_json_string_path(&mut value, &["model"], &client.model);
    set_json_string_path(&mut value, &["env", "ANTHROPIC_MODEL"], &client.model);

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
    remove_json_string_path_if(&mut value, &["env", "ANTHROPIC_MODEL"], &client.model);

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
    require_profile_protocol(profile, &[PROTOCOL_GOOGLE_GEMINI])?;
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    let mut updates = vec![
        ("GEMINI_API_KEY", Some(api_key)),
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

fn gemini_gateway_env_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    Ok(update_env_content(
        current,
        &[
            ("GEMINI_API_KEY", Some(client.token)),
            ("GOOGLE_GEMINI_BASE_URL", Some(client.base_url)),
            ("GEMINI_MODEL", Some(client.model)),
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

    Ok(update_env_content(current, &updates))
}

fn gemini_code_assist_settings_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_GOOGLE_GEMINI])?;
    let mut value = parse_json5_or_empty(current, "VS Code user settings")?;
    let api_key = load_provider_api_key_for_direct_config(profile)?;

    set_json_string_path(&mut value, &[GEMINI_CODE_ASSIST_API_KEY_SETTING], &api_key);

    render_json_config(value, "VS Code user settings")
}

fn opencode_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    require_profile_protocol(
        profile,
        &[PROTOCOL_OPENAI_CHAT_COMPLETIONS, PROTOCOL_OPENAI_RESPONSES],
    )?;
    let mut value = parse_json5_or_empty(current, "OpenCode config")?;
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    let provider_id = managed_provider_id_for_profile(profile);
    let provider_name = format!("CodeStudio {}", profile.provider);

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
        &api_key,
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

fn opencode_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "OpenCode config")?;
    let provider_id = client.provider_id;

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
    set_json_string_path(
        &mut value,
        &["model"],
        &format!("{provider_id}/{}", client.model),
    );
    set_json_string_path(
        &mut value,
        &["provider", &provider_id, "models", &client.model, "name"],
        &client.model,
    );

    render_json_config(value, "OpenCode config")
}

fn opencode_gateway_cleanup_config_content(current: &str, tool_id: &str) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let mut value = parse_json5_or_empty(current, "OpenCode config")?;
    let provider_id = client.provider_id;
    let model_ref = format!("{provider_id}/{}", client.model);

    remove_json_string_path_if(&mut value, &["model"], &model_ref);
    remove_json_path(&mut value, &["provider", &provider_id]);

    render_json_config(value, "OpenCode config")
}

fn openclaw_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_OPENAI_CHAT_COMPLETIONS])?;
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    let provider_id = managed_provider_id_for_profile(profile);
    let provider_name = format!("CodeStudio {}", profile.provider);

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
        &api_key,
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

fn openclaw_gateway_config_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_json5_or_empty(current, "OpenClaw config")?;
    let provider_id = client.provider_id;

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
        &format!("{provider_id}/{}", client.model),
    );
    set_json_value_path(
        &mut value,
        &["models", "providers", &provider_id, "models"],
        serde_json::json!([
            {
                "id": client.model,
                "name": client.model,
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

    remove_json_string_path_if(
        &mut value,
        &["agents", "defaults", "model", "primary"],
        &model_ref,
    );
    remove_json_path(&mut value, &["models", "providers", &provider_id]);

    render_json_config(value, "OpenClaw config")
}

fn hermes_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_OPENAI_CHAT_COMPLETIONS])?;
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;
    let api_key = load_provider_api_key_for_direct_config(profile)?;

    set_yaml_string_path(&mut value, &["model", "provider"], "custom");
    set_yaml_string_path(&mut value, &["model", "base_url"], profile.base_url.trim());
    set_yaml_string_path(&mut value, &["model", "api_key"], &api_key);
    set_yaml_string_path(&mut value, &["model", "api_mode"], "chat_completions");
    if let Some(model) = profile_model(profile) {
        set_yaml_string_path(&mut value, &["model", "default"], model);
    } else {
        remove_yaml_path(&mut value, &["model", "default"]);
    }

    render_yaml_config(value, "Hermes config")
}

fn hermes_gateway_config_content(current: &str, profile: &ProfileDraft) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let mut value = parse_yaml_or_empty(current, "Hermes config")?;

    set_yaml_string_path(&mut value, &["model", "provider"], "custom");
    set_yaml_string_path(&mut value, &["model", "base_url"], &client.base_url);
    set_yaml_string_path(&mut value, &["model", "api_key"], &client.token);
    set_yaml_string_path(&mut value, &["model", "api_mode"], "chat_completions");
    set_yaml_string_path(&mut value, &["model", "default"], &client.model);

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

    Ok(
        read_toml_string(&value, "model_provider").as_deref() == Some(provider_id.as_str())
            && read_toml_string(&value, "model").as_deref() == Some(client.model.as_str())
            && toml_lookup(&value, &format!("model_providers.{provider_id}.base_url"))
                .map(redacted_toml_value)
                .as_deref()
                == Some(client.base_url.as_str())
            && toml_lookup(
                &value,
                &format!("model_providers.{provider_id}.requires_openai_auth"),
            )
            .and_then(|item| item.as_bool())
                == Some(true)
            && toml_lookup(
                &value,
                &format!("model_providers.{provider_id}.experimental_bearer_token"),
            )
            .and_then(|item| item.as_str())
                == Some(client.token.as_str()),
    )
}

fn verify_codex_direct_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value: toml::Value = toml::from_str(&content).map_err(|err| err.to_string())?;
    if provider_is_official(&profile.provider) {
        let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;
        let model_matches = if profile.model.trim().is_empty() {
            read_toml_string(&value, "model").is_none()
        } else {
            read_toml_string(&value, "model").as_deref() == Some(profile.model.trim())
        };

        return Ok(
            read_toml_string(&value, "model_provider").as_deref() == Some("openai")
                && model_matches
                && toml_lookup(&value, "model_providers.openai.wire_api")
                    .and_then(|item| item.as_str())
                    == Some(wire_api)
                && toml_lookup(&value, "model_providers.openai.requires_openai_auth")
                    .and_then(|item| item.as_bool())
                    == Some(true)
                && toml_lookup(&value, "model_providers.openai.experimental_bearer_token")
                    .and_then(|item| item.as_str())
                    .map(|token| token.trim().is_empty())
                    .unwrap_or(true),
        );
    }

    let provider_id = codex_provider_id_for_profile(profile);
    let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;
    let model = if profile.model.trim().is_empty() {
        "codestudio-default"
    } else {
        profile.model.trim()
    };

    Ok(
        read_toml_string(&value, "model_provider").as_deref() == Some(provider_id.as_str())
            && read_toml_string(&value, "model").as_deref() == Some(model)
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
                == Some(false)
            && toml_lookup(
                &value,
                &format!("model_providers.{provider_id}.experimental_bearer_token"),
            )
            .and_then(|item| item.as_str())
            .map(|token| !token.trim().is_empty())
                == Some(true),
    )
}

fn verify_claude_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value = parse_json5_or_empty(&content, "Claude settings")?;
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
        .map(|token| profile_api_key_matches_config(profile, &token))
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

    Ok(
        json_string_lookup(&value, &["env", "ANTHROPIC_BASE_URL"]).as_deref()
            == Some(client.base_url.as_str())
            && json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"]).as_deref()
                == Some(client.token.as_str())
            && (json_string_lookup(&value, &["model"]).as_deref() == Some(client.model.as_str())
                || json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).as_deref()
                    == Some(client.model.as_str())),
    )
}

fn verify_gemini_env_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let env = parse_env_content(&content);
    let model_matches = match profile_model(profile) {
        Some(model) => env.get("GEMINI_MODEL").map(String::as_str) == Some(model),
        None => env.get("GEMINI_MODEL").is_none(),
    };
    let token_matches = env
        .get("GEMINI_API_KEY")
        .map(|token| profile_api_key_matches_config(profile, token))
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

    Ok(
        env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str) == Some(client.base_url.as_str())
            && env.get("GEMINI_API_KEY").map(String::as_str) == Some(client.token.as_str())
            && env.get("GEMINI_MODEL").map(String::as_str) == Some(client.model.as_str()),
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
    let provider_id = managed_provider_id_for_profile(profile);
    let expected_model = profile_model(profile).map(|model| format!("{provider_id}/{model}"));
    let model_matches = match expected_model.as_deref() {
        Some(model) => json_string_lookup(&value, &["model"]).as_deref() == Some(model),
        None => json_string_lookup(&value, &["model"]).is_none(),
    };
    let token_matches =
        json_string_lookup(&value, &["provider", &provider_id, "options", "apiKey"])
            .map(|token| profile_api_key_matches_config(profile, &token))
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
    let expected_model = format!("{provider_id}/{}", client.model);

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
    let provider_id = managed_provider_id_for_profile(profile);
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
            .map(|token| profile_api_key_matches_config(profile, &token))
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
    let expected_model = format!("{provider_id}/{}", client.model);

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
    let model_matches = match profile_model(profile) {
        Some(model) => yaml_string_lookup(&value, &["model", "default"]).as_deref() == Some(model),
        None => yaml_string_lookup(&value, &["model", "default"]).is_none(),
    };
    let token_matches = yaml_string_lookup(&value, &["model", "api_key"])
        .map(|token| profile_api_key_matches_config(profile, &token))
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

    Ok(
        yaml_string_lookup(&value, &["model", "provider"]).as_deref() == Some("custom")
            && yaml_string_lookup(&value, &["model", "base_url"]).as_deref()
                == Some(client.base_url.as_str())
            && yaml_string_lookup(&value, &["model", "api_key"]).as_deref()
                == Some(client.token.as_str())
            && yaml_string_lookup(&value, &["model", "api_mode"]).as_deref()
                == Some("chat_completions")
            && yaml_string_lookup(&value, &["model", "default"]).as_deref()
                == Some(client.model.as_str()),
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
    managed_provider_id_for_profile(profile)
}

fn managed_provider_id_for_profile(profile: &ProfileDraft) -> String {
    let mut id = format!("codestudio-{}", slugify(&profile.provider));
    if id == "codestudio-" {
        id = "codestudio-provider".to_string();
    }
    id
}

fn load_provider_api_key_for_direct_config(profile: &ProfileDraft) -> Result<String, String> {
    let Some(auth_ref) = profile.auth_ref.as_deref() else {
        if provider_requires_api_key(&profile.provider) {
            return Err(
                "Config file mode needs a stored Provider API key. Edit this profile and save an API key first."
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

fn require_profile_protocol(profile: &ProfileDraft, supported: &[&str]) -> Result<(), String> {
    let protocol = normalize_protocol(Some(&profile.protocol))?;
    if supported.iter().any(|candidate| *candidate == protocol) {
        Ok(())
    } else {
        Err(format!(
            "{} does not support {} in Config file mode.",
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
        "Config file mode does not support {} for '{}'.",
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
            "Config file mode writes Codex's provider entry directly to the selected upstream Provider.".to_string(),
            "The preview masks the Provider API key. The actual key is loaded from the system keychain during apply.".to_string(),
            "Changing Codex config usually requires restarting Codex or opening a new Codex session.".to_string(),
        ],
        ProviderApplyMode::Gateway => vec![
            "Gateway mode is a one-time relay injection target, not a direct Provider switch.".to_string(),
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
                let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;
                let mut changes = vec![
                    diff_line(
                        &value,
                        "model_provider",
                        "openai",
                        "Selects Codex's official OpenAI provider.",
                    ),
                    diff_line(
                        &value,
                        "model_providers.openai.name",
                        "OpenAI",
                        "Keeps a readable label for the official provider.",
                    ),
                    diff_line(
                        &value,
                        "model_providers.openai.wire_api",
                        wire_api,
                        "Uses Codex's supported official provider wire API.",
                    ),
                    diff_line(
                        &value,
                        "model_providers.openai.requires_openai_auth",
                        "true",
                        "Keeps Codex official login as the authentication source.",
                    ),
                    diff_remove_line(
                        &value,
                        "model_providers.openai.experimental_bearer_token",
                        "Official login does not require a Provider API key.",
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
                changes
            } else {
                let provider_id = codex_provider_id_for_profile(profile);
                let provider_name = format!("CodeStudio {}", profile.provider);
                let wire_api = codex_wire_api_for_protocol(&profile.protocol)?;
                let model = if profile.model.trim().is_empty() {
                    "codestudio-default"
                } else {
                    profile.model.trim()
                };
                vec![
                    diff_line(
                        &value,
                        "model_provider",
                        &provider_id,
                        "Selects the direct provider entry managed by CodeStudio Lite.",
                    ),
                    diff_line(
                        &value,
                        "model",
                        model,
                        "Sets Codex to the selected upstream model.",
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
                    diff_line(
                        &value,
                        &format!("model_providers.{provider_id}.experimental_bearer_token"),
                        secret_preview(profile),
                        "Stores the selected Provider API key from the system keychain.",
                    ),
                ]
            }
        }
        ProviderApplyMode::Gateway => {
            let provider_id = client.provider_id;
            let provider_name = client.provider_name;
            vec![
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
                    "true",
                    "Keeps the official Codex login path available while routing model requests through the Local Gateway.",
                ),
                diff_line(
                    &value,
                    &format!("model_providers.{provider_id}.experimental_bearer_token"),
                    &client.token_preview,
                    "Stores only the local CodeStudio token, not the real upstream Provider API key.",
                ),
            ]
        }
    };

    Ok(Some(NativeConfigPreview {
        tool: "codex".to_string(),
        path,
        status,
        write_enabled: true,
        changes,
        warnings,
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

    if provider_is_official(&profile.provider) {
        return Ok(None);
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
    let provider_id = managed_provider_id_for_profile(profile);
    let mut warnings = match app.as_str() {
        "claude" => vec![
            "Config file mode writes Claude Code user settings under the env section."
                .to_string(),
            "The selected endpoint must be Anthropic/Claude-compatible; generic OpenAI-only endpoints need a translator."
                .to_string(),
            "Restart Claude Code or open a new session after applying so settings reload."
                .to_string(),
        ],
        "gemini" => vec![
            "Gemini CLI reads API key and base URL from environment variables, so this adapter writes ~/.gemini/.env."
                .to_string(),
            "Restart Gemini CLI or open a new terminal session after applying so environment variables reload."
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
        "opencode" => vec![
            "OpenCode custom providers are written to opencode.json using the OpenAI-compatible provider package."
                .to_string(),
            "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        "openclaw" => vec![
            "OpenClaw providers are written in models.mode=merge so existing provider definitions can stay available."
                .to_string(),
            "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        "hermes" => vec![
            "Hermes custom providers are written to ~/.hermes/config.yaml under the model section."
                .to_string(),
            "Existing YAML comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
            "Hermes config file mode currently targets OpenAI Chat Completions endpoints."
                .to_string(),
        ],
        _ => return Ok(None),
    };

    let (status, changes) = match app.as_str() {
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
            let (json, status) = read_json_preview(&path_buf, "Claude settings", &mut warnings)?;
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
            let (json, status) = read_json_preview(&path_buf, "OpenCode config", &mut warnings)?;
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
            let (json, status) = read_json_preview(&path_buf, "OpenClaw config", &mut warnings)?;
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
    };

    Ok(Some(NativeConfigPreview {
        tool: app,
        path,
        status,
        write_enabled: true,
        changes,
        warnings,
    }))
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
            "Claude Desktop config file mode writes the 3P profile system used by Claude Desktop.".to_string(),
            "CodeStudio Lite enables Claude Desktop developer mode before writing the 3P profile if it is not already enabled.".to_string(),
            "The selected endpoint must be Anthropic Messages compatible; generic OpenAI-only endpoints need Gateway mode.".to_string(),
            "Restart Claude Desktop after applying so it reloads the config library.".to_string(),
        ],
        ProviderApplyMode::Gateway => vec![
            "Claude Desktop gateway mode writes the 3P profile to the tool-scoped CodeStudio Lite Local Gateway URL.".to_string(),
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
    let model_ref = format!("{provider_id}/{}", client.model);
    let mut warnings = match app.as_str() {
        "claude" => vec![
            "Gateway mode writes Claude Code settings to the tool-scoped local gateway URL."
                .to_string(),
            "Restart Claude Code or open a new session after applying so settings reload."
                .to_string(),
        ],
        "gemini" => vec![
            "Gateway mode writes Gemini CLI environment values to the tool-scoped local gateway URL."
                .to_string(),
            "Restart Gemini CLI or open a new terminal session after applying so environment variables reload."
                .to_string(),
        ],
        "opencode" => vec![
            "Gateway mode writes OpenCode's provider entry to the tool-scoped local gateway URL."
                .to_string(),
            "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        "openclaw" => vec![
            "Gateway mode writes OpenClaw's provider entry to the tool-scoped local gateway URL."
                .to_string(),
            "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file."
                .to_string(),
        ],
        "hermes" => vec![
            "Gateway mode writes Hermes custom provider settings to the tool-scoped local gateway URL."
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
                        &client.model,
                        "Sets Claude Code to the virtual model name resolved by the Local Gateway.",
                    ),
                    json_diff_line(
                        &json,
                        &["env", "ANTHROPIC_MODEL"],
                        &client.model,
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
                        &client.model,
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
                        &["provider", &provider_id, "models", &client.model, "name"],
                        &client.model,
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
                        &client.model,
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

fn verify_applied_profile(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let value: toml::Value = toml::from_str(&content).map_err(|err| err.to_string())?;

    Ok(
        read_toml_string(&value, "profile_id").as_deref() == Some(profile.id.as_str())
            && read_toml_string(&value, "app").as_deref() == Some(profile.app.as_str())
            && read_toml_string(&value, "provider").as_deref() == Some(profile.provider.as_str())
            && read_toml_string(&value, "protocol").as_deref() == Some(profile.protocol.as_str())
            && read_toml_string(&value, "model").as_deref() == Some(profile.model.as_str())
            && read_toml_string(&value, "base_url").as_deref() == Some(profile.base_url.as_str())
            && read_toml_string(&value, "secret_policy").as_deref()
                == Some("never_write_plaintext"),
    )
}

fn unique_profile_id(base_id: &str) -> Result<String, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    let base_id = if base_id.is_empty() {
        "profile"
    } else {
        base_id
    };

    for index in 0..1000 {
        let candidate = if index == 0 {
            base_id.to_string()
        } else {
            format!("{base_id}-{index}")
        };
        if !is_builtin_profile_id(&candidate)
            && !paths
                .profiles_dir
                .join(format!("{candidate}.toml"))
                .exists()
        {
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
mod tests {
    use super::*;

    fn test_app_config() -> AppConfig {
        AppConfig {
            active_profiles_by_mode: ActiveProfilesByMode::default(),
            ui: UiConfig {
                theme: "system".to_string(),
                language: "zh-CN".to_string(),
            },
            security: SecurityConfig {
                backup_before_write: true,
                redact_secrets: true,
                confirm_install_commands: true,
                confirm_config_writes: true,
            },
            paths: PathConfig {
                profiles_dir: "~/.codestudio-lite/profiles".to_string(),
                backups_dir: "~/.codestudio-lite/backups".to_string(),
                logs_dir: "~/.codestudio-lite/logs".to_string(),
            },
        }
    }

    #[test]
    fn sync_codex_config_profile_marks_matching_official_profile_active() {
        let mut config = test_app_config();
        let drafts = builtin_official_profiles();
        let codex_config: toml::Value = toml::from_str(
            r#"
model_provider = "openai"

[model_providers.openai]
wire_api = "responses"
requires_openai_auth = true
"#,
        )
        .expect("config should parse");

        assert!(sync_codex_config_profile(
            &mut config,
            &drafts,
            &codex_config
        ));
        assert_eq!(
            config.active_profiles_by_mode.config.get("codex"),
            Some(&builtin_official_profile_id("codex"))
        );
    }

    #[test]
    fn sync_codex_config_profile_clears_stale_config_active_profile() {
        let mut config = test_app_config();
        config
            .active_profiles_by_mode
            .config
            .insert("codex".to_string(), builtin_official_profile_id("codex"));
        let drafts = builtin_official_profiles();
        let codex_config: toml::Value = toml::from_str(
            r#"
model_provider = "other"

[model_providers.other]
requires_openai_auth = true
"#,
        )
        .expect("config should parse");

        assert!(sync_codex_config_profile(
            &mut config,
            &drafts,
            &codex_config
        ));
        assert!(!config.active_profiles_by_mode.config.contains_key("codex"));
    }

    #[test]
    fn codex_native_config_uses_relay_injection_with_official_auth() {
        let config = codex_native_config_content("", "codex").expect("config should render");
        let value: toml::Value = toml::from_str(&config).expect("config should parse");

        assert_eq!(
            read_toml_string(&value, "model_provider").as_deref(),
            Some("codestudio-local")
        );
        assert_eq!(
            toml_lookup(&value, "model_providers.codestudio-local.wire_api")
                .and_then(|item| item.as_str()),
            Some("responses")
        );
        assert_eq!(
            toml_lookup(
                &value,
                "model_providers.codestudio-local.requires_openai_auth"
            )
            .and_then(|item| item.as_bool()),
            Some(true)
        );
        assert_eq!(
            toml_lookup(&value, "model_providers.codestudio-local.base_url")
                .and_then(|item| item.as_str()),
            Some("http://127.0.0.1:43112/tools/codex/v1")
        );
    }

    #[test]
    fn claude_desktop_profile_uses_3p_gateway_shape() {
        let value = claude_desktop_gateway_profile_value(
            "http://127.0.0.1:43112/tools/claude-desktop",
            "local-token",
            Some(&[ClaudeDesktopInferenceModelSpec {
                name: "claude-sonnet-4-6".to_string(),
                label_override: Some("Upstream Model".to_string()),
                supports_1m: true,
            }]),
        );

        assert_eq!(value["inferenceProvider"].as_str(), Some("gateway"));
        assert_eq!(
            value["inferenceGatewayBaseUrl"].as_str(),
            Some("http://127.0.0.1:43112/tools/claude-desktop")
        );
        assert_eq!(
            value["inferenceGatewayApiKey"].as_str(),
            Some("local-token")
        );
        assert_eq!(
            value["inferenceModels"][0]["name"].as_str(),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            value["inferenceModels"][0]["labelOverride"].as_str(),
            Some("Upstream Model")
        );
        assert_eq!(
            value["inferenceModels"][0]["supports1m"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn claude_desktop_meta_apply_and_restore_updates_managed_entry() {
        let applied = claude_desktop_meta_content(
            r#"{"entries":[{"id":"other","name":"Other"}],"appliedId":"other"}"#,
            true,
        )
        .expect("meta should render");
        let value = parse_json5_or_empty(&applied, "meta").expect("json");
        assert_eq!(
            json_string_lookup(&value, &["appliedId"]).as_deref(),
            Some(CLAUDE_DESKTOP_PROFILE_ID)
        );
        assert!(value["entries"].as_array().unwrap().iter().any(|entry| {
            entry.get("id").and_then(serde_json::Value::as_str) == Some(CLAUDE_DESKTOP_PROFILE_ID)
        }));

        let restored =
            claude_desktop_meta_content(&applied, false).expect("restore meta should render");
        let value = parse_json5_or_empty(&restored, "meta").expect("json");
        assert_ne!(
            json_string_lookup(&value, &["appliedId"]).as_deref(),
            Some(CLAUDE_DESKTOP_PROFILE_ID)
        );
        assert!(!value["entries"].as_array().unwrap().iter().any(|entry| {
            entry.get("id").and_then(serde_json::Value::as_str) == Some(CLAUDE_DESKTOP_PROFILE_ID)
        }));
    }

    #[test]
    fn claude_desktop_deployment_mode_preserves_unrelated_config() {
        let config = claude_desktop_deployment_config_content(
            r#"{"foo":"bar","enterpriseConfig":{"inferenceProvider":"gateway","keep":"yes"}}"#,
            "1p",
            true,
        )
        .expect("deployment config should render");
        let value = parse_json5_or_empty(&config, "deployment").expect("json");

        assert_eq!(
            json_string_lookup(&value, &["deploymentMode"]).as_deref(),
            Some("1p")
        );
        assert_eq!(json_string_lookup(&value, &["foo"]).as_deref(), Some("bar"));
        assert_eq!(
            json_string_lookup(&value, &["enterpriseConfig", "keep"]).as_deref(),
            Some("yes")
        );
        assert!(json_string_lookup(&value, &["enterpriseConfig", "inferenceProvider"]).is_none());
    }

    #[test]
    fn claude_desktop_developer_settings_enable_devtools_preserving_values() {
        let config =
            claude_desktop_developer_settings_content(r#"{"foo":"bar","allowDevTools":false}"#)
                .expect("developer settings should render");
        let value = parse_json5_or_empty(&config, "developer settings").expect("json");

        assert_eq!(json_string_lookup(&value, &["foo"]).as_deref(), Some("bar"));
        assert_eq!(
            value
                .get("allowDevTools")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(claude_desktop_developer_mode_enabled(&config).expect("settings should parse"));
    }

    #[test]
    fn claude_desktop_developer_settings_plan_only_when_disabled() {
        let mut paths = claude_desktop_paths_from_dirs(
            PathBuf::from("C:/Users/example/AppData/Local/Claude"),
            PathBuf::from("C:/Users/example/AppData/Local/Claude-3p"),
            vec![PathBuf::from(
                "C:/Users/example/AppData/Roaming/Claude/developer_settings.json",
            )],
        );
        paths.developer_settings_paths = vec![paths
            .developer_settings_paths
            .first()
            .expect("path")
            .clone()];

        let plans =
            build_claude_desktop_developer_settings_plans(&paths).expect("plan should build");
        assert_eq!(plans.len(), 1);
        assert!(plans[0].content.contains("\"allowDevTools\": true"));
    }

    #[test]
    fn claude_desktop_gateway_base_url_strips_v1_suffix() {
        assert_eq!(
            claude_desktop_gateway_profile_base_url(
                "http://127.0.0.1:43112/tools/claude-desktop/v1"
            ),
            "http://127.0.0.1:43112/tools/claude-desktop"
        );
    }

    #[test]
    fn claude_desktop_safe_model_ids_match_desktop_routes() {
        assert!(claude_desktop_safe_model_id("claude-sonnet-4-6"));
        assert!(claude_desktop_safe_model_id("anthropic/claude-haiku-4-5"));
        assert!(!claude_desktop_safe_model_id("claude-sonnet-4-6[1m]"));
        assert!(!claude_desktop_safe_model_id("gpt-5.5"));
    }

    #[test]
    fn native_config_paths_route_supported_tools() {
        let paths = test_paths();
        let mut profile = test_profile("claude", ProviderApplyMode::Config);
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Config)
                .expect("path should resolve"),
            Some(paths.home_dir.join(".claude").join("settings.json"))
        );
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
                .expect("gateway path should resolve"),
            Some(paths.home_dir.join(".claude").join("settings.json"))
        );

        profile.app = "opencode".to_string();
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Config)
                .expect("path should resolve"),
            Some(
                paths
                    .home_dir
                    .join(".config")
                    .join("opencode")
                    .join("opencode.json")
            )
        );
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
                .expect("gateway path should resolve"),
            Some(
                paths
                    .home_dir
                    .join(".config")
                    .join("opencode")
                    .join("opencode.json")
            )
        );

        profile.app = "hermes".to_string();
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Config)
                .expect("path should resolve"),
            Some(paths.home_dir.join(".hermes").join("config.yaml"))
        );
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
                .expect("gateway path should resolve"),
            Some(paths.home_dir.join(".hermes").join("config.yaml"))
        );

        profile.app = "codex-app".to_string();
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
                .expect("codex gateway path should resolve"),
            Some(paths.home_dir.join(".codex").join("config.toml"))
        );

        profile.app = "gemini".to_string();
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
                .expect("gateway path should resolve"),
            Some(paths.home_dir.join(".gemini").join(".env"))
        );

        profile.app = "openclaw".to_string();
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
                .expect("gateway path should resolve"),
            Some(paths.home_dir.join(".openclaw").join("openclaw.json"))
        );

        profile.app = "claude-desktop".to_string();
        let claude_desktop_profile_path = claude_desktop_paths(&paths)
            .expect("claude desktop paths should resolve")
            .profile_path;
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Config)
                .expect("claude desktop config path should resolve"),
            Some(claude_desktop_profile_path.clone())
        );
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
                .expect("claude desktop gateway path should resolve"),
            Some(claude_desktop_profile_path)
        );

        profile.app = "gemini-code-assist".to_string();
        assert_eq!(
            native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
                .expect("unsupported gateway path should not resolve"),
            None
        );
    }

    #[test]
    fn lifecycle_restore_targets_include_gateway_apps() {
        let mut active = ActiveProfilesByMode::default();
        active
            .config
            .insert("codex-app".to_string(), "codex-config".to_string());
        active
            .gateway
            .insert("claude-vscode".to_string(), "claude-gateway".to_string());

        assert_eq!(
            lifecycle_target_apps(&active, ProviderApplyMode::Config, true),
            vec!["claude".to_string(), "codex".to_string()]
        );
        assert_eq!(
            lifecycle_target_apps(&active, ProviderApplyMode::Gateway, false),
            vec!["claude".to_string()]
        );
    }

    #[test]
    fn official_cleanup_removes_only_gateway_fields() {
        let profile = test_profile("claude", ProviderApplyMode::Gateway);
        let gateway_content = claude_gateway_config_content(
            r#"{
  "env": {
    "OTHER_VALUE": "keep"
  }
}"#,
            &profile,
        )
        .expect("gateway config should render");
        let cleaned = claude_gateway_cleanup_config_content(&gateway_content, "claude")
            .expect("cleanup config should render");
        let value = parse_json5_or_empty(&cleaned, "Claude settings").expect("cleaned JSON");

        assert_eq!(
            json_string_lookup(&value, &["env", "OTHER_VALUE"]).as_deref(),
            Some("keep")
        );
        assert!(json_string_lookup(&value, &["env", "ANTHROPIC_BASE_URL"]).is_none());
        assert!(json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"]).is_none());
        assert!(json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).is_none());
        assert!(json_string_lookup(&value, &["model"]).is_none());

        let profile = test_profile("opencode", ProviderApplyMode::Gateway);
        let gateway_content =
            opencode_gateway_config_content("{}", &profile).expect("gateway config should render");
        let cleaned = opencode_gateway_cleanup_config_content(&gateway_content, "opencode")
            .expect("cleanup config should render");
        let value = parse_json5_or_empty(&cleaned, "OpenCode config").expect("cleaned JSON");

        assert!(json_lookup(&value, &["provider", "codestudio-local"]).is_none());
        assert!(json_string_lookup(&value, &["model"]).is_none());
    }

    #[test]
    fn gemini_env_update_preserves_unrelated_values_and_removes_empty_model() {
        let current = "# user note\nOTHER=1\nGEMINI_MODEL=\"old\"\n";
        let updated = update_env_content(
            current,
            &[
                ("GEMINI_API_KEY", Some("secret-key".to_string())),
                (
                    "GOOGLE_GEMINI_BASE_URL",
                    Some("https://example.test/v1".to_string()),
                ),
                ("GEMINI_MODEL", None),
            ],
        );
        let env = parse_env_content(&updated);

        assert!(updated.contains("# user note"));
        assert_eq!(env.get("OTHER").map(String::as_str), Some("1"));
        assert_eq!(
            env.get("GEMINI_API_KEY").map(String::as_str),
            Some("secret-key")
        );
        assert_eq!(
            env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str),
            Some("https://example.test/v1")
        );
        assert!(!env.contains_key("GEMINI_MODEL"));
    }

    #[test]
    fn json5_preview_parser_accepts_comments() {
        let value = parse_json5_or_empty(
            r#"
            {
              // comment from a JSONC-style config
              provider: {
                codestudio_openai: {
                  options: {
                    baseURL: "https://example.test/v1",
                  },
                },
              },
            }
            "#,
            "test config",
        )
        .expect("json5 should parse");

        assert_eq!(
            json_string_lookup(
                &value,
                &["provider", "codestudio_openai", "options", "baseURL"]
            )
            .as_deref(),
            Some("https://example.test/v1")
        );
    }

    #[test]
    fn legacy_protocol_alias_is_rejected() {
        assert!(normalize_protocol(Some("openai-compatible")).is_err());
        assert!(normalize_protocol(Some("claude-messages")).is_err());
        assert!(normalize_protocol(None).is_err());
        assert_eq!(
            normalize_protocol(Some("openai-responses")).as_deref(),
            Ok(PROTOCOL_OPENAI_RESPONSES)
        );
        assert_eq!(
            normalize_protocol(Some("anthropic-messages")).as_deref(),
            Ok(PROTOCOL_ANTHROPIC_MESSAGES)
        );
    }

    #[test]
    fn builtin_official_profiles_use_tool_native_protocols() {
        let profiles = builtin_official_profiles();

        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.app == "codex")
                .map(|profile| profile.protocol.as_str()),
            Some(PROTOCOL_OPENAI_RESPONSES)
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.app == "claude")
                .map(|profile| profile.protocol.as_str()),
            Some(PROTOCOL_ANTHROPIC_MESSAGES)
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.app == "claude-desktop")
                .map(|profile| profile.protocol.as_str()),
            Some(PROTOCOL_ANTHROPIC_MESSAGES)
        );
        assert_eq!(
            profiles
                .iter()
                .any(|profile| profile.app == "claude-vscode"),
            false
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.app == "gemini")
                .map(|profile| profile.protocol.as_str()),
            Some(PROTOCOL_GOOGLE_GEMINI)
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.app == "gemini-code-assist")
                .map(|profile| profile.protocol.as_str()),
            Some(PROTOCOL_GOOGLE_GEMINI)
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.app == "openclaw")
                .map(|profile| profile.protocol.as_str()),
            Some(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.app == "hermes")
                .map(|profile| profile.protocol.as_str()),
            Some(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
        );
    }

    #[test]
    fn claude_vscode_alias_uses_claude_profile_category() {
        assert_eq!(canonical_profile_app("claude-vscode"), "claude");
        assert_eq!(canonical_profile_app("claude-code-vscode"), "claude");
        assert_eq!(
            builtin_official_profile_id("claude-vscode"),
            "builtin-official-claude"
        );
    }

    fn test_profile(app: &str, mode: ProviderApplyMode) -> ProfileDraft {
        ProfileDraft {
            id: format!("{app}-custom"),
            name: "Custom".to_string(),
            app: app.to_string(),
            is_builtin: false,
            mode,
            provider: "openai".to_string(),
            protocol: PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string(),
            model: String::new(),
            base_url: "https://example.test/v1".to_string(),
            auth_ref: Some(format!("keychain:test/{app}/api_key")),
            timeout_seconds: 120,
            created_at: None,
            updated_at: None,
            last_test_status: None,
        }
    }

    fn test_paths() -> crate::core::app_paths::AppPaths {
        let root = PathBuf::from("C:/Users/example");
        crate::core::app_paths::AppPaths {
            home_dir: root.clone(),
            config_dir: root.join(".codestudio-lite"),
            profiles_dir: root.join(".codestudio-lite").join("profiles"),
            applied_dir: root.join(".codestudio-lite").join("applied"),
            backups_dir: root.join(".codestudio-lite").join("backups"),
            logs_dir: root.join(".codestudio-lite").join("logs"),
            config_file: root.join(".codestudio-lite").join("config.toml"),
            activity_log_file: root
                .join(".codestudio-lite")
                .join("logs")
                .join("activity.jsonl"),
            gateway_request_log_file: root
                .join(".codestudio-lite")
                .join("logs")
                .join("gateway-requests.jsonl"),
        }
    }
}

fn escape_toml_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
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
