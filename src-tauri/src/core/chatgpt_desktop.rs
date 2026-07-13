use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::codex_plugin_marketplace;
use crate::core::codex_provider_sync;
use crate::core::computer_use_guard;
use crate::core::download_http;
use crate::core::platform::{hidden_command, package};
use crate::core::process_control;
use crate::core::storage;
use crate::core::types::{
    ChatGptDesktopInstallKinds, ChatGptDesktopProductGeneration, ConfigState,
    DesktopInstallKindInfo, InstallState, Severity, ToolCategory, ToolStatus,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering as AtomicOrdering},
    Mutex, OnceLock,
};
use std::thread;
use std::time::{Duration, Instant};
use zip::ZipArchive;

const DEFAULT_MIRROR_BASE: &str = "https://codexapp.agentsmirror.com";
const OFFICIAL_MACOS_ARM64_URL: &str = "https://persistent.oaistatic.com/codex-app-prod/Codex.dmg";
const OFFICIAL_MACOS_X64_URL: &str =
    "https://persistent.oaistatic.com/codex-app-prod/Codex-latest-x64.dmg";
const PACKAGE_IDENTITY: &str = "OpenAI.Codex";
const CODEX_DISPLAY_NAME: &str = "Codex";
const CODEX_PUBLISHER: &str = "OpenAI";
const CODEX_EXE_NAME: &str = "Codex.exe";
const CHATGPT_EXE_NAME: &str = "ChatGPT.exe";
const CHATGPT_MACOS_APP_NAME: &str = "ChatGPT.app";
const LEGACY_CODEX_MACOS_APP_NAME: &str = "Codex.app";
const CHATGPT_MACOS_APP_CANDIDATES: &[&str] = &[
    CHATGPT_MACOS_APP_NAME,
    LEGACY_CODEX_MACOS_APP_NAME,
    "OpenAI Codex.app",
    "OpenAI.Codex.app",
];
const CODEX_SHORTCUT_NAME: &str = "Codex.lnk";
const CODEX_UNINSTALL_KEY: &str =
    r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Codex";
const CODEX_MACOS_BUNDLE_ID: &str = "com.openai.codex";
const CHATGPT_DESKTOP_SETTINGS_STATE_KEY: &str = "chatgpt_desktop.settings";
const CHATGPT_DESKTOP_MARKER_STATE_KEY: &str = "chatgpt_desktop.managed_marker";
const LEGACY_CODEX_CLIENT_SETTINGS_STATE_KEY: &str = "codex_client.settings";
const LEGACY_CODEX_CLIENT_MARKER_STATE_KEY: &str = "codex_client.managed_marker";
const CODEX_PATCH_INJECTION_RETRY_COUNT: usize = 30;
const CODEX_PATCH_INJECTION_RETRY_MS: u64 = 500;
const CODEX_PATCH_WATCHDOG_POLL_MS: u64 = 2_000;
const CODEX_PATCH_WATCHDOG_MAX_MISSES: usize = 15;
const MIRROR_METADATA_TIMEOUT_SECS: u64 = 30;
const MIRROR_PACKAGE_TIMEOUT_SECS: u64 = 600;
pub const CHATGPT_DESKTOP_PROGRESS_EVENT: &str = "chatgpt-desktop://progress";
static DOWNLOAD_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopSettings {
    pub source: String,
    pub custom_url: String,
    pub auto_check: bool,
    pub ask_before: bool,
    pub signed_only: bool,
    pub windows_install_mode: String,
    pub install_root: String,
    pub keep_user_data_on_uninstall: bool,
    #[serde(default)]
    pub sync_history_on_launch: bool,
    #[serde(default = "default_true")]
    pub plugin_marketplace_unlock_on_launch: bool,
    #[serde(default = "default_true")]
    pub plugin_auto_expand_on_launch: bool,
    #[serde(default = "default_true")]
    pub model_whitelist_unlock_on_launch: bool,
    #[serde(default)]
    pub service_tier_controls_on_launch: bool,
    #[serde(default = "default_true")]
    pub official_remote_plugin_cache_on_launch: bool,
    #[serde(default)]
    pub computer_use_guard_on_launch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChatGptDesktopSettingsRequest {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub custom_url: Option<String>,
    #[serde(default)]
    pub auto_check: Option<bool>,
    #[serde(default)]
    pub ask_before: Option<bool>,
    #[serde(default)]
    pub windows_install_mode: Option<String>,
    #[serde(default)]
    pub install_root: Option<String>,
    #[serde(default)]
    pub keep_user_data_on_uninstall: Option<bool>,
    #[serde(default)]
    pub sync_history_on_launch: Option<bool>,
    #[serde(default)]
    pub plugin_marketplace_unlock_on_launch: Option<bool>,
    #[serde(default)]
    pub plugin_auto_expand_on_launch: Option<bool>,
    #[serde(default)]
    pub model_whitelist_unlock_on_launch: Option<bool>,
    #[serde(default)]
    pub service_tier_controls_on_launch: Option<bool>,
    #[serde(default)]
    pub official_remote_plugin_cache_on_launch: Option<bool>,
    #[serde(default)]
    pub computer_use_guard_on_launch: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledChatGptDesktop {
    pub path: String,
    pub version: String,
    pub arch: Option<String>,
    pub source: String,
    #[serde(default)]
    pub generation: ChatGptDesktopProductGeneration,
    pub package_family_name: Option<String>,
    pub installed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopRelease {
    pub version: String,
    pub package_moniker: String,
    pub architecture: Option<String>,
    pub package_kind: String,
    pub package_source: String,
    pub content_length: Option<u64>,
    pub etag: Option<String>,
    pub package_identity: Option<String>,
    pub package_url: String,
    pub checksums_url: String,
    pub manifest_url: String,
    pub sha256: String,
    pub macos_arm64_version: Option<String>,
    pub macos_x64_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopClientCapability {
    pub id: String,
    pub label: String,
    pub status: Severity,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopPlan {
    pub up_to_date: bool,
    pub current_version: Option<String>,
    pub latest_version: String,
    pub route: String,
    pub package_url: String,
    pub download_size: Option<u64>,
    pub sha256: String,
    pub staged_path: Option<String>,
    pub install_root: Option<String>,
    pub warnings: Vec<String>,
    pub capabilities: Vec<DesktopClientCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopState {
    pub install_kind: String,
    pub generated_at: String,
    pub platform: String,
    pub settings: ChatGptDesktopSettings,
    pub installed: Option<InstalledChatGptDesktop>,
    pub install_class: String,
    pub release: Option<ChatGptDesktopRelease>,
    pub plan: Option<ChatGptDesktopPlan>,
    pub staging_dir: String,
    pub notes: Vec<String>,
    #[serde(default)]
    pub running: bool,
}

pub type ChatGptDesktopStateCache = BTreeMap<String, ChatGptDesktopState>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopStageReport {
    pub install_kind: String,
    pub up_to_date: bool,
    pub staged_path: Option<String>,
    pub package_moniker: String,
    pub download_size: u64,
    pub sha256: String,
    pub hash_verified: bool,
    pub route: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopProgress {
    pub install_kind: String,
    pub phase: String,
    pub message: String,
    pub downloaded: Option<u64>,
    pub total: Option<u64>,
    pub percent: Option<f64>,
    pub step: Option<u64>,
    pub step_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopOperationResult {
    pub install_kind: String,
    pub success: bool,
    pub action: String,
    pub message: String,
    pub installed: Option<InstalledChatGptDesktop>,
    pub stage: Option<ChatGptDesktopStageReport>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopUninstallRequest {
    pub confirm: bool,
    #[serde(default)]
    pub purge_user_data: bool,
    /// Which install kind to uninstall ("msix" or "portable"). When None,
    /// the backend falls back to the detected install kind.
    #[serde(default)]
    pub install_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopInstallRequest {
    pub confirm: bool,
    #[serde(default)]
    pub expected_current_version: Option<String>,
    #[serde(default)]
    pub expected_latest_version: Option<String>,
    #[serde(default)]
    pub expected_route: Option<String>,
    /// Which install kind to use ("msix" or "portable"). Overrides the
    /// persisted windows_install_mode setting so the page tab selection drives
    /// the install route. When None, the persisted setting is used as before.
    #[serde(default)]
    pub install_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlanChatGptDesktopUpdateRequest {
    #[serde(default)]
    pub install_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StageChatGptDesktopUpdateRequest {
    #[serde(default)]
    pub install_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedInstallMarker {
    source: String,
    install_root: Option<String>,
    package_family_name: Option<String>,
    version: Option<String>,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorManifest {
    schema_version: u64,
    sources: ManifestSources,
}

#[derive(Debug, Deserialize)]
struct ManifestSources {
    windows: WindowsSource,
    #[serde(default)]
    macos: Option<MacosSources>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsSource {
    version: String,
    package_moniker: String,
    architecture: Option<String>,
    content_length: Option<u64>,
    etag: Option<String>,
    product_id: Option<String>,
    update_manifest: Option<WindowsUpdateManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsUpdateManifest {
    package_identity: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MacosSources {
    #[serde(default)]
    arm64: Option<MacosSource>,
    #[serde(default)]
    x64: Option<MacosSource>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacosSource {
    url: Option<String>,
    content_length: Option<u64>,
    etag: Option<String>,
    sha256: Option<String>,
    bundle_short_version: Option<String>,
    bundle_version: Option<String>,
    bundle_identifier: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MsixIdentity {
    name: String,
    publisher: String,
    version: String,
    processor_architecture: String,
}

impl Default for ChatGptDesktopSettings {
    fn default() -> Self {
        Self {
            source: "mirror".to_string(),
            custom_url: String::new(),
            auto_check: true,
            ask_before: true,
            signed_only: true,
            windows_install_mode: "msix".to_string(),
            install_root: default_install_root(),
            keep_user_data_on_uninstall: true,
            sync_history_on_launch: false,
            plugin_marketplace_unlock_on_launch: true,
            plugin_auto_expand_on_launch: true,
            model_whitelist_unlock_on_launch: true,
            service_tier_controls_on_launch: false,
            official_remote_plugin_cache_on_launch: true,
            computer_use_guard_on_launch: false,
        }
    }
}

fn default_true() -> bool {
    true
}

const CHATGPT_DESKTOP_LATEST_CACHE_TTL: Duration = Duration::from_secs(600);
const CHATGPT_DESKTOP_LATEST_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Default)]
struct ChatGptDesktopLatestCache {
    version: Option<String>,
    checked_at: Option<Instant>,
    in_progress: bool,
}

static CHATGPT_DESKTOP_LATEST_CACHE: OnceLock<Mutex<ChatGptDesktopLatestCache>> = OnceLock::new();

fn chatgpt_desktop_latest_cache() -> &'static Mutex<ChatGptDesktopLatestCache> {
    CHATGPT_DESKTOP_LATEST_CACHE.get_or_init(|| Mutex::new(ChatGptDesktopLatestCache::default()))
}

/// Fetch the latest Codex version from the mirror manifest in a
/// background thread and cache the result in-process. Returns the cached
/// version if fresh, waits up to wait_budget for an in-flight fetch, and
/// otherwise returns whatever is cached so the caller is never blocked for
/// long. Mirrors the Claude Desktop latest-version cache in detector.rs.
pub fn latest_version_cached(wait_budget: Duration) -> Option<String> {
    let should_start = {
        let mut cache = chatgpt_desktop_latest_cache().lock().unwrap();
        if cache
            .checked_at
            .map(|checked_at| checked_at.elapsed() < CHATGPT_DESKTOP_LATEST_CACHE_TTL)
            .unwrap_or(false)
        {
            return cache.version.clone();
        }
        if cache.in_progress {
            false
        } else {
            cache.in_progress = true;
            true
        }
    };

    if should_start {
        thread::spawn(|| {
            let version = (|| {
                let settings = load_settings().unwrap_or_default();
                load_release(&settings).ok().map(|release| release.version)
            })();
            let mut cache = chatgpt_desktop_latest_cache().lock().unwrap();
            finish_latest_cache(&mut cache, version);
        });
    }

    let started_at = Instant::now();
    loop {
        {
            let cache = chatgpt_desktop_latest_cache().lock().unwrap();
            if !cache.in_progress
                || cache
                    .checked_at
                    .map(|checked_at| checked_at.elapsed() < CHATGPT_DESKTOP_LATEST_CACHE_TTL)
                    .unwrap_or(false)
            {
                return cache.version.clone();
            }
            if started_at.elapsed() >= wait_budget {
                return cache.version.clone();
            }
        }
        thread::sleep(CHATGPT_DESKTOP_LATEST_POLL_INTERVAL);
    }
}

fn finish_latest_cache(cache: &mut ChatGptDesktopLatestCache, version: Option<String>) {
    if let Some(version) = version {
        cache.version = Some(version);
        cache.checked_at = Some(Instant::now());
    }
    cache.in_progress = false;
}

/// Load the most recent Codex state cached to disk by inspect_state(true).
/// Used by the page to hydrate instantly on startup before an async re-fetch.
pub fn load_cached_state() -> Option<ChatGptDesktopState> {
    storage::load_chatgpt_desktop_state().ok().flatten()
}

/// Load all cached Codex states keyed by install kind. Windows has independent
/// MSIX and portable plans, so one global row loses whichever tab was scanned
/// first.
pub fn load_cached_states() -> ChatGptDesktopStateCache {
    storage::load_chatgpt_desktop_states().unwrap_or_default()
}

pub fn inspect_state(include_network: bool) -> Result<ChatGptDesktopState, String> {
    inspect_state_for_install_kind(include_network, None)
}

fn inspect_state_for_install_kind(
    include_network: bool,
    install_kind: Option<&str>,
) -> Result<ChatGptDesktopState, String> {
    let settings = load_settings()?;
    let install_kind = normalize_install_kind(install_kind, &settings);
    let route_settings = settings_for_install_kind(settings.clone(), &install_kind);
    let installed = detect_installed_for_kind(&route_settings, &install_kind);
    let release = if include_network {
        Some(load_release(&route_settings)?)
    } else {
        None
    };
    let plan = release
        .as_ref()
        .map(|release| build_plan(&route_settings, installed.as_ref(), release))
        .transpose()?;
    let install_class = install_class(installed.as_ref());
    let mut notes = vec![
        "ChatGPT Desktop management covers install, update, uninstall, launch, and mirror-source flows.".to_string(),
        "The ChatGPT Desktop installer content is not modified; downloads are SHA-256 verified before installation.".to_string(),
    ];
    if cfg!(target_os = "macos") {
        notes.push(
            "macOS uses a DMG installer and copies ChatGPT.app to the target Applications directory; legacy Codex app bundle names remain detectable."
                .to_string(),
        );
    } else if !cfg!(target_os = "windows") {
        notes.push("The current platform does not provide an executable ChatGPT desktop client install path yet.".to_string());
    }
    let running = is_chatgpt_desktop_running(installed.as_ref());

    let state = ChatGptDesktopState {
        install_kind: install_kind.clone(),
        generated_at: Utc::now().to_rfc3339(),
        platform: platform_label(),
        settings,
        installed,
        install_class,
        release,
        plan,
        staging_dir: display_path(&staging_dir()?),
        notes,
        running,
    };
    if include_network {
        let _ = storage::store_chatgpt_desktop_state(&state);
    }
    Ok(state)
}

pub fn plan_update(
    request: PlanChatGptDesktopUpdateRequest,
) -> Result<ChatGptDesktopState, String> {
    inspect_state_for_install_kind(true, request.install_kind.as_deref())
}

pub fn stage_update() -> Result<ChatGptDesktopStageReport, String> {
    stage_update_with_progress(StageChatGptDesktopUpdateRequest::default(), |_| {})
}

pub fn stage_update_with_progress<F>(
    request: StageChatGptDesktopUpdateRequest,
    on_progress: F,
) -> Result<ChatGptDesktopStageReport, String>
where
    F: Fn(ChatGptDesktopProgress),
{
    let mut settings = load_settings()?;
    let install_kind = normalize_install_kind(request.install_kind.as_deref(), &settings);
    settings = settings_for_install_kind(settings, &install_kind);
    emit_step_progress(
        &on_progress,
        &install_kind,
        "preparing",
        "Reading mirror manifest and checksums...",
        None,
        None,
        Some(1),
        Some(4),
    );
    let release = load_release(&settings)?;
    let installed = detect_installed_for_kind(&settings, &install_kind);
    let plan = build_plan(&settings, installed.as_ref(), &release)?;
    stage_from_plan(&install_kind, &release, &plan, &on_progress)
}

pub fn install_or_update(
    request: ChatGptDesktopInstallRequest,
) -> Result<ChatGptDesktopOperationResult, String> {
    install_or_update_with_progress(request, |_| {})
}

pub fn install_or_update_with_progress<F>(
    request: ChatGptDesktopInstallRequest,
    on_progress: F,
) -> Result<ChatGptDesktopOperationResult, String>
where
    F: Fn(ChatGptDesktopProgress),
{
    if !request.confirm {
        return Err(
            "Refused: installing or updating ChatGPT Desktop requires explicit confirmation."
                .to_string(),
        );
    }

    let mut settings = load_settings()?;
    let install_kind = normalize_install_kind(request.install_kind.as_deref(), &settings);
    settings = settings_for_install_kind(settings, &install_kind);
    emit_step_progress(
        &on_progress,
        &install_kind,
        "preparing",
        "Confirming install state and update plan...",
        None,
        None,
        Some(1),
        Some(7),
    );
    validate_install_target(&settings)?;
    let release = load_release(&settings)?;
    let installed_before = detect_installed_for_kind(&settings, &install_kind);
    let plan = build_plan(&settings, installed_before.as_ref(), &release)?;

    if let Some(expected) = request.expected_current_version.as_deref() {
        let actual = installed_before.as_ref().map(|item| item.version.as_str());
        if actual != Some(expected) && !(expected.is_empty() && actual.is_none()) {
            return Err(format!(
                "ChatGPT Desktop state changed: expected version {expected}, current version is {}. Refresh and try again.",
                actual.unwrap_or("not installed")
            ));
        }
    }
    if let Some(expected) = request.expected_latest_version.as_deref() {
        if expected != release.version {
            return Err(format!(
                "Mirror latest version changed: expected {expected}, current version is {}. Refresh and try again.",
                release.version
            ));
        }
    }
    if let Some(expected) = request.expected_route.as_deref() {
        if expected != plan.route {
            return Err(format!(
                "Install route changed: expected {expected}, current route is {}. Refresh and try again.",
                plan.route
            ));
        }
    }

    if plan.up_to_date {
        emit_step_progress(
            &on_progress,
            &install_kind,
            "done",
            "chatgptDesktop.progressAlreadyUpToDate",
            Some(1),
            Some(1),
            Some(7),
            Some(7),
        );
        return Ok(ChatGptDesktopOperationResult {
            install_kind,
            success: true,
            action: "none".to_string(),
            message: "ChatGPT Desktop is already up to date.".to_string(),
            installed: installed_before,
            stage: None,
            notes: Vec::new(),
        });
    }

    let mut stage = stage_from_plan(&install_kind, &release, &plan, &on_progress)?;
    let staged_path = stage
        .staged_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "No staged file is available to install.".to_string())?;
    let mut notes = stage.notes.clone();
    if plan.route == "unsupported" {
        return Err("The current platform does not provide an executable ChatGPT desktop client install path yet.".to_string());
    }

    let action = plan.route.clone();
    if let Some(installed) = installed_before.as_ref() {
        if cfg!(target_os = "windows") {
            let mut termination = if installed.source == "msix" {
                process_control::close_appx_package_for_update("Codex", PACKAGE_IDENTITY)?
            } else {
                process_control::ProcessTerminationReport::default()
            };
            let fallback = process_control::close_processes_for_update(
                "Codex",
                &["Codex"],
                Some(Path::new(&installed.path)),
            )?;
            termination.total += fallback.total;
            termination.forced += fallback.forced;
            termination.remaining += fallback.remaining;
            if let Some(note) = termination.note("Codex") {
                notes.push(note);
            }
        } else if cfg!(target_os = "macos") {
            if let Err(err) = close_chatgpt_desktop_processes(installed, &mut notes) {
                notes.push(format!("Failed to close ChatGPT Desktop: {err}"));
            }
        }
    }
    let installed = if action == "portable-fallback" {
        emit_step_progress(
            &on_progress,
            &install_kind,
            "installing",
            "chatgptDesktop.progressInstallingPortable",
            None,
            None,
            Some(4),
            Some(7),
        );
        let report = install_portable(
            &staged_path,
            &expand_env_path(&settings.install_root)?,
            &install_kind,
            &on_progress,
        )?;
        notes.extend(report.notes);
        report.installed
    } else if action == "macos-dmg" {
        emit_step_progress(
            &on_progress,
            &install_kind,
            "installing",
            "chatgptDesktop.progressInstallingMacos",
            None,
            None,
            Some(4),
            Some(7),
        );
        let report = package::install_macos_dmg_with_app_candidates(
            &staged_path,
            CHATGPT_MACOS_APP_CANDIDATES,
            &expand_env_path(&settings.install_root)?,
            None,
        )?;
        notes.extend(report.notes);
        report.installed.map(installed_from_macos_app)
    } else {
        emit_step_progress(
            &on_progress,
            &install_kind,
            "msix-installing",
            "chatgptDesktop.progressInstallingMsix",
            None,
            None,
            Some(4),
            Some(7),
        );
        match package::install_msix_package(&staged_path, PACKAGE_IDENTITY) {
            Ok(report) if report.success => report
                .installed
                .map(installed_from_msix)
                .or_else(|| detect_installed(&settings)),
            Ok(report) => {
                notes.push(format!("MSIX install failed: {}", report.message));
                return Err(format!("MSIX install failed: {}.", report.message));
            }
            Err(err) => {
                notes.push(format!("MSIX install execution failed: {err}"));
                return Err(format!("MSIX install execution failed: {err}."));
            }
        }
    };

    let installed = installed.or_else(|| detect_installed_for_kind(&settings, &install_kind));
    if installed.is_some() {
        cleanup_staged_package(&mut stage, &mut notes);
    }
    save_marker(&ManagedInstallMarker {
        source: installed
            .as_ref()
            .map(|item| item.source.clone())
            .unwrap_or_else(|| action.clone()),
        install_root: Some(
            expand_env_path(&settings.install_root)?
                .to_string_lossy()
                .to_string(),
        ),
        package_family_name: installed
            .as_ref()
            .and_then(|item| item.package_family_name.clone()),
        version: installed.as_ref().map(|item| item.version.clone()),
        updated_at: Utc::now().to_rfc3339(),
    })?;
    let _ = activity_log::append(
        Severity::Ok,
        format!(
            "Installed or updated ChatGPT Desktop to {} via {}.",
            release.version, action
        ),
    );

    emit_step_progress(
        &on_progress,
        &install_kind,
        "done",
        "chatgptDesktop.progressInstallDone",
        Some(1),
        Some(1),
        Some(7),
        Some(7),
    );

    Ok(ChatGptDesktopOperationResult {
        install_kind,
        success: installed.is_some(),
        action,
        message: installed
            .as_ref()
            .map(|item| {
                format!(
                    "ChatGPT Desktop is ready: {} ({})",
                    item.version, item.source
                )
            })
            .unwrap_or_else(|| {
                "Installation flow finished, but ChatGPT Desktop was not detected again."
                    .to_string()
            }),
        installed,
        stage: Some(stage),
        notes,
    })
}

pub fn uninstall(
    request: ChatGptDesktopUninstallRequest,
) -> Result<ChatGptDesktopOperationResult, String> {
    if !request.confirm {
        return Err(
            "Refused: uninstalling ChatGPT Desktop requires explicit confirmation.".to_string(),
        );
    }
    if !cfg!(target_os = "windows") && !cfg!(target_os = "macos") {
        return Err("The current platform does not provide an executable ChatGPT desktop client uninstall path yet.".to_string());
    }

    let mut settings = load_settings()?;
    let install_kind = normalize_install_kind(request.install_kind.as_deref(), &settings);
    settings = settings_for_install_kind(settings, &install_kind);
    // When the caller specifies an install kind (from the page tab), detect
    // only that kind so uninstalling targets the version the user is viewing.
    let installed = detect_installed_for_kind(&settings, &install_kind);
    let Some(installed_before) = installed else {
        return Ok(ChatGptDesktopOperationResult {
            install_kind,
            success: true,
            action: "none".to_string(),
            message: "No uninstallable ChatGPT Desktop installation was detected.".to_string(),
            installed: None,
            stage: None,
            notes: Vec::new(),
        });
    };

    let mut notes = Vec::new();
    if cfg!(target_os = "windows") {
        close_chatgpt_desktop_processes(&installed_before, &mut notes)?;
    } else if cfg!(target_os = "macos") {
        if let Err(err) = close_chatgpt_desktop_processes(&installed_before, &mut notes) {
            notes.push(format!("Failed to close ChatGPT Desktop: {err}"));
        }
    }
    let action = if installed_before.source == "portable" {
        if Path::new(&installed_before.path).exists() {
            fs::remove_dir_all(&installed_before.path)
                .map_err(|err| format!("Failed to remove portable directory: {err}"))?;
        }
        if let Err(err) = package::remove_portable_start_menu_shortcut(CODEX_SHORTCUT_NAME) {
            notes.push(format!("Failed to clean Start menu shortcut: {err}"));
        }
        if let Err(err) = package::remove_portable_uninstall_entry(CODEX_UNINSTALL_KEY) {
            notes.push(format!("Failed to clean uninstall entry: {err}"));
        }
        "remove-portable"
    } else if installed_before.source == "macos" {
        let app_path = Path::new(&installed_before.path);
        if app_path.exists() {
            fs::remove_dir_all(app_path)
                .map_err(|err| format!("Failed to remove macOS app: {err}"))?;
        }
        "remove-macos"
    } else if installed_before.source == "msix" {
        let report = package::remove_msix_package(PACKAGE_IDENTITY)?;
        if !report.success {
            return Err(report.message);
        }
        notes.extend(report.notes);
        "remove-msix"
    } else {
        return Err(format!(
            "Unsupported ChatGPT Desktop install type for uninstall: {}.",
            installed_before.source
        ));
    };

    if request.purge_user_data {
        if purge_user_data()? {
            notes.push("Deleted ~/.codex user data.".to_string());
        } else {
            notes.push("No ~/.codex user data directory was found.".to_string());
        }
    } else {
        notes.push("Kept ~/.codex user data.".to_string());
    }

    let _ = storage::delete_state_json(CHATGPT_DESKTOP_MARKER_STATE_KEY);
    let _ = storage::delete_state_json(LEGACY_CODEX_CLIENT_MARKER_STATE_KEY);
    let _ = activity_log::append(Severity::Ok, "Uninstalled ChatGPT Desktop.");

    Ok(ChatGptDesktopOperationResult {
        install_kind,
        success: true,
        action: action.to_string(),
        message: "ChatGPT Desktop uninstalled.".to_string(),
        installed: None,
        stage: None,
        notes,
    })
}

pub fn launch() -> Result<(), String> {
    let mut notes = Vec::new();
    launch_with_restart_notes(&mut notes)
}

fn launch_with_restart_notes(notes: &mut Vec<String>) -> Result<(), String> {
    let settings = load_settings()?;
    let installed = detect_installed(&settings)
        .ok_or_else(|| "ChatGPT Desktop was not detected.".to_string())?;
    close_chatgpt_desktop_processes(&installed, notes)?;
    sync_history_if_enabled(&settings)?;
    ensure_official_remote_plugin_cache_if_enabled(&settings);
    ensure_computer_use_guard_if_enabled(&settings)?;
    let debug_port = select_debug_port()?;
    let args = codex_patch_launch_args(debug_port);
    launch_installed_codex(&installed, &args)?;
    start_computer_use_guard_watchdog_if_enabled(&settings);
    let enhancement_settings = codex_enhancement_settings_from(&settings);
    if enhancement_settings.enabled() {
        spawn_codex_enhancement_injection(debug_port, enhancement_settings);
    }
    let _ = activity_log::append(Severity::Info, "Launched ChatGPT Desktop.");
    Ok(())
}

pub fn restart() -> Result<String, String> {
    let mut notes = Vec::new();
    launch_with_restart_notes(&mut notes)?;
    let message = if notes.is_empty() {
        "Launched ChatGPT Desktop.".to_string()
    } else {
        format!("{} Restarted ChatGPT Desktop.", notes.join(" "))
    };
    let _ = activity_log::append(
        Severity::Info,
        "Restarted ChatGPT Desktop after profile apply.",
    );
    Ok(message)
}

pub fn update_settings(
    request: UpdateChatGptDesktopSettingsRequest,
) -> Result<ChatGptDesktopSettings, String> {
    let mut settings = load_settings()?;
    if let Some(source) = request.source {
        settings.source = normalize_source(&source);
    } else {
        settings.source = normalize_source(&settings.source);
    }
    settings.custom_url = String::new();
    if let Some(auto_check) = request.auto_check {
        settings.auto_check = auto_check;
    }
    if let Some(ask_before) = request.ask_before {
        settings.ask_before = ask_before;
    }
    if let Some(mode) = request.windows_install_mode {
        settings.windows_install_mode = if mode == "portable" {
            "portable"
        } else {
            "msix"
        }
        .to_string();
    }
    if let Some(root) = request.install_root {
        let expanded = expand_env_path(&root)?;
        validate_install_path_for_platform(&expanded)?;
        settings.install_root = expanded.to_string_lossy().to_string();
    }
    if let Some(keep) = request.keep_user_data_on_uninstall {
        settings.keep_user_data_on_uninstall = keep;
    }
    if let Some(sync) = request.sync_history_on_launch {
        settings.sync_history_on_launch = sync;
    }
    if let Some(enabled) = request.plugin_marketplace_unlock_on_launch {
        settings.plugin_marketplace_unlock_on_launch = enabled;
    }
    if let Some(enabled) = request.plugin_auto_expand_on_launch {
        settings.plugin_auto_expand_on_launch = enabled;
    }
    if let Some(enabled) = request.model_whitelist_unlock_on_launch {
        settings.model_whitelist_unlock_on_launch = enabled;
    }
    if let Some(enabled) = request.service_tier_controls_on_launch {
        settings.service_tier_controls_on_launch = enabled;
    }
    if let Some(enabled) = request.official_remote_plugin_cache_on_launch {
        settings.official_remote_plugin_cache_on_launch = enabled;
    }
    if let Some(enabled) = request.computer_use_guard_on_launch {
        settings.computer_use_guard_on_launch = enabled;
    }
    settings.signed_only = true;
    save_settings(&settings)?;
    Ok(settings)
}

pub fn open_path(kind: String) -> Result<(), String> {
    let settings = load_settings()?;
    let target = match kind.as_str() {
        "install" => detect_installed(&settings)
            .map(|installed| PathBuf::from(installed.path))
            .unwrap_or(expand_env_path(&settings.install_root)?),
        "staging" => staging_dir()?,
        "config" => app_paths()
            .map_err(|err| err.to_string())?
            .home_dir
            .join(".codex"),
        _ => return Err("Unknown path type.".to_string()),
    };
    open_folder(&target)
}

pub fn tool_status() -> ToolStatus {
    let settings = load_settings().unwrap_or_default();
    let installed = detect_installed(&settings);
    let generation = installed
        .as_ref()
        .map(|item| item.generation)
        .unwrap_or_default();
    let product_name = chatgpt_desktop_product_name(generation);
    let config_path = app_paths().ok().map(|paths| paths.home_dir.join(".codex"));
    ToolStatus {
        id: "chatgpt-desktop".to_string(),
        name: product_name.to_string(),
        category: ToolCategory::AiTool,
        command: if cfg!(target_os = "windows") {
            if generation == ChatGptDesktopProductGeneration::Current
                && installed.as_ref().is_some_and(|item| item.source == "msix")
            {
                CHATGPT_EXE_NAME.to_string()
            } else {
                CODEX_EXE_NAME.to_string()
            }
        } else {
            macos_tool_command(installed.as_ref())
        },
        path_repair: None,
        version: installed.as_ref().map(|item| item.version.clone()),
        latest_version: None,
        update_available: false,
        update_command: None,
        install_state: if installed.is_some() {
            InstallState::Installed
        } else {
            InstallState::Missing
        },
        config_state: match &config_path {
            Some(path) if path.exists() => ConfigState::Configured,
            Some(_) => ConfigState::Unconfigured,
            None => ConfigState::Unknown,
        },
        config_path: config_path.as_deref().map(display_path),
        install_path: None,
        install_command: Some(format!("Install or update from the {product_name} page")),
        details: installed
            .as_ref()
            .map(|item| format!("{} / {}", item.source, item.path))
            .or_else(|| Some(format!("Official {product_name} client was not detected"))),
        install_kind: None,
        running: is_chatgpt_desktop_running(installed.as_ref()),
    }
}

fn is_chatgpt_desktop_running(installed: Option<&InstalledChatGptDesktop>) -> bool {
    if cfg!(target_os = "windows") {
        return installed.is_some_and(|item| {
            if item.generation == ChatGptDesktopProductGeneration::Current && item.source == "msix"
            {
                process_control::is_process_running("ChatGPT")
            } else {
                process_control::is_process_running("Codex")
            }
        });
    }
    if cfg!(target_os = "macos") {
        return installed.is_some_and(|item| package::macos_app_running(Path::new(&item.path)));
    }
    false
}

fn build_plan(
    settings: &ChatGptDesktopSettings,
    installed: Option<&InstalledChatGptDesktop>,
    release: &ChatGptDesktopRelease,
) -> Result<ChatGptDesktopPlan, String> {
    let capabilities = probe_capabilities();
    let current_version = installed.map(|item| item.version.clone());
    let up_to_date = current_version
        .as_deref()
        .map(|version| compare_versions(version, &release.version) != Ordering::Less)
        .unwrap_or(false);
    let route = select_install_route(settings, installed).to_string();
    let mut warnings = Vec::new();
    if route == "unsupported" {
        warnings.push("The current platform does not provide an executable ChatGPT desktop client install path yet.".to_string());
    } else if route == "macos-dmg" {
        if settings.source == "official" {
            warnings.push(
                "The macOS official source uses the official stable DMG URL; version and SHA-256 still come from the mirror manifest."
                    .to_string(),
            );
        }
        if capabilities
            .iter()
            .any(|capability| capability.status == Severity::Error)
        {
            warnings.push("macOS DMG install dependencies are unavailable; restore hdiutil/ditto before installing.".to_string());
        }
    } else if route == "portable-fallback" {
        warnings.push("The current plan will install the portable build and register Start menu and uninstall entries.".to_string());
    }

    Ok(ChatGptDesktopPlan {
        up_to_date,
        current_version,
        latest_version: release.version.clone(),
        route,
        package_url: release.package_url.clone(),
        download_size: release.content_length,
        sha256: release.sha256.clone(),
        staged_path: staged_package_path(release)
            .ok()
            .filter(|path| path.exists())
            .map(|path| display_path(&path)),
        install_root: Some(
            expand_env_path(&settings.install_root)?
                .to_string_lossy()
                .to_string(),
        ),
        warnings,
        capabilities,
    })
}

fn select_install_route(
    settings: &ChatGptDesktopSettings,
    installed: Option<&InstalledChatGptDesktop>,
) -> &'static str {
    if cfg!(target_os = "macos") {
        return "macos-dmg";
    }
    if !cfg!(target_os = "windows") {
        return "unsupported";
    }
    let existing_source = installed.map(|item| item.source.as_str());
    if existing_source == Some("msix") {
        "msix-sideload"
    } else if existing_source == Some("portable") || settings.windows_install_mode == "portable" {
        "portable-fallback"
    } else {
        "msix-sideload"
    }
}

fn stage_from_plan<F>(
    install_kind: &str,
    release: &ChatGptDesktopRelease,
    plan: &ChatGptDesktopPlan,
    on_progress: &F,
) -> Result<ChatGptDesktopStageReport, String>
where
    F: Fn(ChatGptDesktopProgress),
{
    if plan.up_to_date {
        emit_step_progress(
            on_progress,
            install_kind,
            "done",
            "chatgptDesktop.progressStageAlreadyUpToDate",
            Some(1),
            Some(1),
            Some(4),
            Some(4),
        );
        return Ok(ChatGptDesktopStageReport {
            install_kind: install_kind.to_string(),
            up_to_date: true,
            staged_path: None,
            package_moniker: release.package_moniker.clone(),
            download_size: 0,
            sha256: release.sha256.clone(),
            hash_verified: true,
            route: plan.route.clone(),
            notes: vec![
                "ChatGPT Desktop is already up to date; no download is needed.".to_string(),
            ],
        });
    }

    let mut path = staged_package_path(release)?;
    match staged_package_target(&path, &release.sha256)? {
        StagedPackageTarget::Reuse => {
            let size = fs::metadata(&path).map_err(|err| err.to_string())?.len();
            emit_step_progress(
                on_progress,
                install_kind,
                "verifying",
                "chatgptDesktop.progressFoundStaged",
                Some(size),
                Some(size),
                Some(3),
                Some(4),
            );
        }
        StagedPackageTarget::Download(target) => {
            path = target;
            download_to_file(
                &release.package_url,
                &path,
                release.content_length,
                install_kind,
                on_progress,
            )?;
        }
    }

    emit_step_progress(
        on_progress,
        install_kind,
        "verifying",
        "chatgptDesktop.progressVerifying",
        None,
        None,
        Some(3),
        Some(4),
    );
    let actual = sha256_file(&path)?;
    if !actual.eq_ignore_ascii_case(&release.sha256) {
        let _ = fs::remove_file(&path);
        return Err(format!(
            "SHA-256 verification failed: expected {}, got {}.",
            release.sha256, actual
        ));
    }
    let size = fs::metadata(&path).map_err(|err| err.to_string())?.len();
    let _ = activity_log::append(
        Severity::Ok,
        format!(
            "Staged ChatGPT Desktop package {}.",
            release.package_moniker
        ),
    );
    emit_step_progress(
        on_progress,
        install_kind,
        "done",
        "chatgptDesktop.progressStageDone",
        Some(size),
        Some(size),
        Some(4),
        Some(4),
    );

    Ok(ChatGptDesktopStageReport {
        install_kind: install_kind.to_string(),
        up_to_date: false,
        staged_path: Some(display_path(&path)),
        package_moniker: release.package_moniker.clone(),
        download_size: size,
        sha256: release.sha256.clone(),
        hash_verified: true,
        route: plan.route.clone(),
        notes: vec!["Installer downloaded and passed SHA-256 verification.".to_string()],
    })
}

fn cleanup_staged_package(stage: &mut ChatGptDesktopStageReport, notes: &mut Vec<String>) {
    let Some(staged_path) = stage.staged_path.as_deref() else {
        return;
    };
    let path = PathBuf::from(staged_path);
    if !path.exists() {
        stage.staged_path = None;
        return;
    }
    match fs::remove_file(&path) {
        Ok(()) => {
            stage.staged_path = None;
            notes.push("Cleaned the staged installer used by this operation.".to_string());
        }
        Err(err) => {
            notes.push(format!(
                "Failed to clean staged installer: {}. You can delete {} later.",
                err,
                display_path(&path)
            ));
        }
    }
}

fn load_release(settings: &ChatGptDesktopSettings) -> Result<ChatGptDesktopRelease, String> {
    let base = manifest_base(settings);
    let manifest_url = format!("{base}/latest/manifest");
    let checksums_url = format!("{base}/latest/checksums");
    let manifest_text = fetch_text(&manifest_url)?;
    let checksums_text = fetch_text(&checksums_url)?;
    let manifest: MirrorManifest = serde_json::from_str(&manifest_text)
        .map_err(|err| format!("Failed to parse ChatGPT Desktop mirror manifest: {err}"))?;
    if manifest.schema_version < 2 {
        return Err(format!(
            "Unsupported ChatGPT Desktop mirror manifest schemaVersion: {}",
            manifest.schema_version
        ));
    }

    let macos_arm64_version = manifest
        .sources
        .macos
        .as_ref()
        .and_then(|macos| macos.arm64.as_ref())
        .and_then(|source| source.bundle_short_version.clone());
    let macos_x64_version = manifest
        .sources
        .macos
        .as_ref()
        .and_then(|macos| macos.x64.as_ref())
        .and_then(|source| source.bundle_short_version.clone());

    if cfg!(target_os = "macos") {
        let macos = manifest.sources.macos.as_ref().ok_or_else(|| {
            "ChatGPT Desktop mirror manifest has no macOS installer information.".to_string()
        })?;
        let (source, arch) = current_macos_source(macos)?;
        let source_url = source.url.clone().ok_or_else(|| {
            format!("ChatGPT Desktop mirror manifest has no macOS {arch} download URL.")
        })?;
        let package_url = if settings.source == "official" {
            official_macos_url(arch).to_string()
        } else {
            source_url
        };
        let checksum_name = format!("Codex-mac-{arch}.dmg");
        let package_moniker =
            package_filename(&package_url).unwrap_or_else(|| checksum_name.clone());
        let sha256 = source
            .sha256
            .clone()
            .or_else(|| checksum_for_name(&checksums_text, &checksum_name))
            .or_else(|| checksum_for_name(&checksums_text, &package_moniker))
            .ok_or_else(|| format!("SHA-256 for macOS {arch} DMG was not found in checksums."))?;
        let version = source
            .bundle_short_version
            .clone()
            .or_else(|| source.bundle_version.clone())
            .ok_or_else(|| {
                format!("ChatGPT Desktop mirror manifest has no macOS {arch} version.")
            })?;

        return Ok(ChatGptDesktopRelease {
            version,
            package_moniker,
            architecture: Some(arch.to_string()),
            package_kind: "dmg".to_string(),
            package_source: settings.source.clone(),
            content_length: source.content_length,
            etag: source.etag.clone(),
            package_identity: source
                .bundle_identifier
                .clone()
                .or_else(|| Some(CODEX_MACOS_BUNDLE_ID.to_string())),
            package_url,
            checksums_url,
            manifest_url,
            sha256,
            macos_arm64_version,
            macos_x64_version,
        });
    }

    let windows = manifest.sources.windows;
    let package_url = format!("{base}/latest/win");
    let sha256 =
        checksum_for_windows(&checksums_text, &windows.package_moniker).ok_or_else(|| {
            format!(
                "SHA-256 for {} was not found in checksums.",
                windows.package_moniker
            )
        })?;

    Ok(ChatGptDesktopRelease {
        version: windows.version,
        package_moniker: windows.package_moniker,
        architecture: windows.architecture,
        package_kind: "msix".to_string(),
        package_source: "mirror".to_string(),
        content_length: windows.content_length,
        etag: windows.etag,
        package_identity: windows
            .update_manifest
            .as_ref()
            .and_then(|item| item.package_identity.clone())
            .or(windows.product_id)
            .or_else(|| Some(PACKAGE_IDENTITY.to_string())),
        package_url,
        checksums_url,
        manifest_url,
        sha256,
        macos_arm64_version,
        macos_x64_version,
    })
}

/// Detect both install kinds (MSIX and portable) of the ChatGPT desktop client
/// simultaneously so the UI can show a per-kind tab. Each kind is resolved
/// independently; a user may have both installed at once.
pub fn chatgpt_desktop_install_kinds() -> ChatGptDesktopInstallKinds {
    if !cfg!(target_os = "windows") {
        return ChatGptDesktopInstallKinds {
            msix: DesktopInstallKindInfo {
                installed: false,
                version: None,
                path: None,
            },
            portable: DesktopInstallKindInfo {
                installed: false,
                version: None,
                path: None,
            },
        };
    }
    let settings = load_settings().unwrap_or_default();
    let msix = package::detect_msix_package(PACKAGE_IDENTITY)
        .map(|pkg| DesktopInstallKindInfo {
            installed: true,
            version: Some(pkg.version),
            path: Some(pkg.path),
        })
        .unwrap_or(DesktopInstallKindInfo {
            installed: false,
            version: None,
            path: None,
        });
    let portable = expand_env_path(&settings.install_root)
        .ok()
        .and_then(|root| detect_portable_install(&root))
        .map(|inst| DesktopInstallKindInfo {
            installed: true,
            version: Some(inst.version),
            path: Some(inst.path),
        })
        .unwrap_or(DesktopInstallKindInfo {
            installed: false,
            version: None,
            path: None,
        });
    ChatGptDesktopInstallKinds { msix, portable }
}

fn detect_installed(settings: &ChatGptDesktopSettings) -> Option<InstalledChatGptDesktop> {
    if cfg!(target_os = "windows") {
        package::detect_msix_package(PACKAGE_IDENTITY)
            .map(installed_from_msix)
            .or_else(|| {
                expand_env_path(&settings.install_root)
                    .ok()
                    .and_then(|root| detect_portable_install(&root))
            })
    } else if cfg!(target_os = "macos") {
        package::detect_macos_app(&macos_app_candidates(), None).map(installed_from_macos_app)
    } else {
        None
    }
}

pub fn detected_product_generation() -> ChatGptDesktopProductGeneration {
    let settings = load_settings().unwrap_or_default();
    detect_installed(&settings)
        .map(|installed| installed.generation)
        .unwrap_or_default()
}

fn chatgpt_desktop_product_name(generation: ChatGptDesktopProductGeneration) -> &'static str {
    match generation {
        ChatGptDesktopProductGeneration::Current => "ChatGPT Desktop",
        ChatGptDesktopProductGeneration::Legacy => "Codex Desktop",
    }
}

fn chatgpt_desktop_generation_from_windows_root(root: &Path) -> ChatGptDesktopProductGeneration {
    let executable_exists =
        |name: &str| root.join(name).is_file() || root.join("app").join(name).is_file();
    if executable_exists(CHATGPT_EXE_NAME) {
        ChatGptDesktopProductGeneration::Current
    } else if executable_exists(CODEX_EXE_NAME) {
        ChatGptDesktopProductGeneration::Legacy
    } else {
        ChatGptDesktopProductGeneration::Current
    }
}

fn chatgpt_desktop_generation_from_macos_identity(
    executable_name: Option<&str>,
    app_path: &Path,
) -> ChatGptDesktopProductGeneration {
    if let Some(executable_name) = executable_name {
        if executable_name.eq_ignore_ascii_case("ChatGPT") {
            return ChatGptDesktopProductGeneration::Current;
        }
        if executable_name.eq_ignore_ascii_case("Codex") {
            return ChatGptDesktopProductGeneration::Legacy;
        }
        return ChatGptDesktopProductGeneration::Current;
    }

    let app_name = app_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if CHATGPT_MACOS_APP_CANDIDATES
        .iter()
        .skip(1)
        .any(|candidate| candidate.eq_ignore_ascii_case(app_name))
    {
        ChatGptDesktopProductGeneration::Legacy
    } else {
        ChatGptDesktopProductGeneration::Current
    }
}

fn normalize_install_kind(requested: Option<&str>, settings: &ChatGptDesktopSettings) -> String {
    if cfg!(target_os = "windows") {
        match requested {
            Some("portable") => "portable".to_string(),
            Some("msix") => "msix".to_string(),
            _ if settings.windows_install_mode == "portable" => "portable".to_string(),
            _ => "msix".to_string(),
        }
    } else {
        "msix".to_string()
    }
}

fn settings_for_install_kind(
    mut settings: ChatGptDesktopSettings,
    install_kind: &str,
) -> ChatGptDesktopSettings {
    if cfg!(target_os = "windows") {
        settings.windows_install_mode = if install_kind == "portable" {
            "portable"
        } else {
            "msix"
        }
        .to_string();
    }
    settings
}

fn detect_installed_for_kind(
    settings: &ChatGptDesktopSettings,
    install_kind: &str,
) -> Option<InstalledChatGptDesktop> {
    if cfg!(target_os = "windows") {
        if install_kind == "portable" {
            return expand_env_path(&settings.install_root)
                .ok()
                .and_then(|root| detect_portable_install(&root));
        }
        return package::detect_msix_package(PACKAGE_IDENTITY).map(installed_from_msix);
    }
    detect_installed(settings)
}

fn installed_from_msix(package: package::InstalledMsixPackage) -> InstalledChatGptDesktop {
    let generation = chatgpt_desktop_generation_from_windows_root(Path::new(&package.path));
    InstalledChatGptDesktop {
        installed_at: path_mtime(&PathBuf::from(&package.path)),
        path: package.path,
        version: package.version,
        arch: package.arch,
        source: "msix".to_string(),
        generation,
        package_family_name: package.package_family_name,
    }
}

fn installed_from_macos_app(app: package::InstalledMacosApp) -> InstalledChatGptDesktop {
    let app_path = PathBuf::from(&app.path);
    let executable_name = package::macos_bundle_executable_name(&app_path);
    let generation =
        chatgpt_desktop_generation_from_macos_identity(executable_name.as_deref(), &app_path);
    InstalledChatGptDesktop {
        installed_at: path_mtime(&app_path),
        path: app.path,
        version: app.version,
        arch: None,
        source: "macos".to_string(),
        generation,
        package_family_name: app.bundle_identifier,
    }
}

fn detect_portable_install(root: &Path) -> Option<InstalledChatGptDesktop> {
    let exe = root.join("Codex.exe");
    if !exe.is_file() {
        return None;
    }
    let identity = fs::read_to_string(root.join("AppxManifest.xml"))
        .ok()
        .and_then(|xml| parse_msix_identity(&xml).ok());
    Some(InstalledChatGptDesktop {
        path: root.to_string_lossy().to_string(),
        version: identity
            .as_ref()
            .map(|item| item.version.clone())
            .unwrap_or_else(|| "0.0.0.0".to_string()),
        arch: identity
            .as_ref()
            .map(|item| item.processor_architecture.clone()),
        source: "portable".to_string(),
        generation: chatgpt_desktop_generation_from_windows_root(root),
        package_family_name: None,
        installed_at: path_mtime(&exe),
    })
}

fn macos_app_candidates() -> Vec<PathBuf> {
    macos_app_candidates_for_home(dirs::home_dir().as_deref())
}

fn macos_app_candidates_for_home(home: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = CHATGPT_MACOS_APP_CANDIDATES
        .iter()
        .map(|app_name| PathBuf::from("/Applications").join(app_name))
        .collect::<Vec<_>>();
    if let Some(home) = home {
        candidates.extend(
            CHATGPT_MACOS_APP_CANDIDATES
                .iter()
                .map(|app_name| home.join("Applications").join(app_name)),
        );
    }
    candidates
}

struct PortableInstallReport {
    installed: Option<InstalledChatGptDesktop>,
    notes: Vec<String>,
}

fn install_portable<F>(
    msix_path: &Path,
    install_root: &Path,
    install_kind: &str,
    on_progress: &F,
) -> Result<PortableInstallReport, String>
where
    F: Fn(ChatGptDesktopProgress),
{
    emit_step_progress(
        on_progress,
        install_kind,
        "installing",
        "chatgptDesktop.progressPreparingPortableDir",
        None,
        None,
        Some(4),
        Some(7),
    );
    validate_install_root(install_root)?;
    let mut notes = Vec::new();
    let termination =
        process_control::close_processes_for_update("Codex", &["Codex"], Some(install_root))?;
    if let Some(note) = termination.note("Codex") {
        notes.push(note);
    }
    let parent = install_root
        .parent()
        .ok_or_else(|| "Install directory has no parent directory.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("Failed to create install parent directory: {err}"))?;
    let work = parent
        .join(".codestudio-chatgpt-desktop-staging")
        .join(format!("portable-{}", std::process::id()));
    let extracted = work.join("extracted");
    let payload = work.join("payload");
    if work.exists() {
        fs::remove_dir_all(&work)
            .map_err(|err| format!("Failed to clean old staging directory: {err}"))?;
    }
    fs::create_dir_all(&extracted)
        .map_err(|err| format!("Failed to create staging directory: {err}"))?;

    let manifest_xml = extract_msix(msix_path, &extracted, install_kind, on_progress)?;
    let identity = parse_msix_identity(&manifest_xml)?;
    if identity.name != PACKAGE_IDENTITY {
        notes.push(format!(
            "MSIX Identity is {}, expected {}.",
            identity.name, PACKAGE_IDENTITY
        ));
    }
    if !identity.publisher.to_ascii_lowercase().contains("openai") {
        notes.push(format!(
            "MSIX Publisher does not appear to be OpenAI: {}.",
            identity.publisher
        ));
    }
    let exe = find_codex_exe(&extracted)?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "Codex.exe has no parent directory.".to_string())?;
    emit_step_progress(
        on_progress,
        install_kind,
        "copying",
        "chatgptDesktop.progressCopyingPortable",
        None,
        None,
        Some(5),
        Some(7),
    );
    copy_dir_all(exe_dir, &payload)
        .map_err(|err| format!("Failed to copy portable files: {err}"))?;
    fs::write(payload.join("AppxManifest.xml"), manifest_xml)
        .map_err(|err| format!("Failed to write AppxManifest.xml: {err}"))?;

    emit_step_progress(
        on_progress,
        install_kind,
        "writing",
        "chatgptDesktop.progressWritingInstall",
        None,
        None,
        Some(6),
        Some(7),
    );
    let rollback = parent.join("Codex.rollback");
    if rollback.exists() {
        fs::remove_dir_all(&rollback)
            .map_err(|err| format!("Failed to clean old rollback directory: {err}"))?;
    }
    let had_previous = install_root.exists();
    if had_previous {
        fs::rename(install_root, &rollback)
            .map_err(|err| format!("Failed to create rollback backup: {err}"))?;
    }
    if let Err(err) = fs::rename(&payload, install_root) {
        if had_previous && rollback.exists() {
            let _ = fs::rename(&rollback, install_root);
        }
        return Err(format!(
            "Failed to write portable install directory; rollback was attempted: {err}"
        ));
    }

    emit_step_progress(
        on_progress,
        install_kind,
        "finalizing",
        "chatgptDesktop.progressFinalizingInstall",
        None,
        None,
        Some(6),
        Some(7),
    );
    let registration = portable_registration(install_root, &identity.version);
    if let Err(err) = package::create_portable_start_menu_shortcut(&registration) {
        notes.push(format!("Failed to create Start menu shortcut: {err}"));
    }
    if let Err(err) = package::create_portable_uninstall_entry(&registration) {
        notes.push(format!("Failed to register uninstall entry: {err}"));
    }
    if had_previous && rollback.exists() {
        if let Err(err) = fs::remove_dir_all(&rollback) {
            notes.push(format!("Failed to clean rollback backup: {err}"));
        }
    }
    let _ = fs::remove_dir_all(&work);
    emit_step_progress(
        on_progress,
        install_kind,
        "finalizing",
        "chatgptDesktop.progressPortableWritten",
        Some(1),
        Some(1),
        Some(6),
        Some(7),
    );

    Ok(PortableInstallReport {
        installed: Some(InstalledChatGptDesktop {
            path: install_root.to_string_lossy().to_string(),
            version: identity.version,
            arch: Some(identity.processor_architecture),
            source: "portable".to_string(),
            generation: chatgpt_desktop_generation_from_windows_root(install_root),
            package_family_name: None,
            installed_at: path_mtime(&install_root.join("Codex.exe")),
        }),
        notes,
    })
}

fn extract_msix<F>(
    msix_path: &Path,
    dest: &Path,
    install_kind: &str,
    on_progress: &F,
) -> Result<String, String>
where
    F: Fn(ChatGptDesktopProgress),
{
    let file = File::open(msix_path).map_err(|err| format!("Failed to open MSIX: {err}"))?;
    let mut zip =
        ZipArchive::new(file).map_err(|err| format!("Failed to read MSIX ZIP structure: {err}"))?;
    let mut manifest_xml = None;
    let total_entries = zip.len();
    let total = total_entries as u64;
    emit_step_progress(
        on_progress,
        install_kind,
        "extracting",
        "chatgptDesktop.progressExtractingMsix",
        Some(0),
        Some(total),
        Some(4),
        Some(7),
    );

    for index in 0..total_entries {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| format!("Failed to read MSIX entry: {err}"))?;
        let Some(enclosed_name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let out_path = dest.join(&enclosed_name);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|err| format!("Failed to create extraction directory: {err}"))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Failed to create extraction parent directory: {err}"))?;
        }
        let mut out = File::create(&out_path)
            .map_err(|err| format!("Failed to create extracted file: {err}"))?;
        io::copy(&mut entry, &mut out)
            .map_err(|err| format!("Failed to write extracted file: {err}"))?;

        if enclosed_name
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("AppxManifest.xml"))
            && enclosed_name.components().count() == 1
        {
            let mut xml = String::new();
            File::open(&out_path)
                .and_then(|mut file| file.read_to_string(&mut xml))
                .map_err(|err| format!("Failed to read AppxManifest.xml: {err}"))?;
            manifest_xml = Some(xml);
        }
        if index == 0 || index + 1 == total_entries || index % 25 == 0 {
            emit_step_progress(
                on_progress,
                install_kind,
                "extracting",
                "chatgptDesktop.progressExtractingMsix",
                Some((index + 1) as u64),
                Some(total),
                Some(4),
                Some(7),
            );
        }
    }

    manifest_xml.ok_or_else(|| "MSIX is missing AppxManifest.xml.".to_string())
}

fn find_codex_exe(root: &Path) -> Result<PathBuf, String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|err| format!("Failed to scan extraction directory: {err}"))?
        {
            let entry =
                entry.map_err(|err| format!("Failed to read extraction directory entry: {err}"))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|err| format!("Failed to read file type: {err}"))?;
            if file_type.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("Codex.exe"))
            {
                return Ok(path);
            }
        }
    }
    Err("Codex.exe was not found in the MSIX.".to_string())
}

fn copy_dir_all(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_all(&source, &dest)?;
        } else if file_type.is_file() {
            fs::copy(source, dest)?;
        }
    }
    Ok(())
}

fn parse_msix_identity(xml: &str) -> Result<MsixIdentity, String> {
    let identity_tag = xml
        .split('<')
        .find(|part| part.trim_start().starts_with("Identity "))
        .ok_or_else(|| "AppxManifest.xml is missing Identity.".to_string())?;
    let get = |name: &str| -> Result<String, String> {
        let needle = format!("{name}=\"");
        let start = identity_tag
            .find(&needle)
            .ok_or_else(|| format!("Identity is missing {name}."))?
            + needle.len();
        let rest = &identity_tag[start..];
        let end = rest
            .find('"')
            .ok_or_else(|| format!("Identity {name} has invalid format."))?;
        Ok(rest[..end].to_string())
    };
    Ok(MsixIdentity {
        name: get("Name")?,
        publisher: get("Publisher")?,
        version: get("Version")?,
        processor_architecture: get("ProcessorArchitecture")?,
    })
}

fn probe_capabilities() -> Vec<DesktopClientCapability> {
    let capabilities = if cfg!(target_os = "macos") {
        package::probe_macos_dmg_capabilities()
    } else {
        package::probe_msix_capabilities()
    };
    capabilities
        .into_iter()
        .map(|capability| DesktopClientCapability {
            id: capability.id,
            label: capability.label,
            status: capability.status,
            detail: capability.detail,
        })
        .collect()
}

fn manifest_base(_settings: &ChatGptDesktopSettings) -> String {
    DEFAULT_MIRROR_BASE.to_string()
}

fn normalize_source(source: &str) -> String {
    match source.trim() {
        "official" if cfg!(target_os = "macos") => "official".to_string(),
        "mirror" => "mirror".to_string(),
        _ => "mirror".to_string(),
    }
}

fn current_macos_source(macos: &MacosSources) -> Result<(&MacosSource, &'static str), String> {
    if cfg!(target_arch = "aarch64") {
        macos
            .arm64
            .as_ref()
            .map(|source| (source, "arm64"))
            .ok_or_else(|| {
                "ChatGPT Desktop mirror manifest has no macOS arm64 installer information."
                    .to_string()
            })
    } else {
        macos
            .x64
            .as_ref()
            .map(|source| (source, "x64"))
            .ok_or_else(|| {
                "ChatGPT Desktop mirror manifest has no macOS x64 installer information."
                    .to_string()
            })
    }
}

fn official_macos_url(arch: &str) -> &'static str {
    if arch == "arm64" {
        OFFICIAL_MACOS_ARM64_URL
    } else {
        OFFICIAL_MACOS_X64_URL
    }
}

fn package_filename(url: &str) -> Option<String> {
    url.split('?')
        .next()
        .and_then(|part| part.rsplit('/').next())
        .filter(|part| !part.trim().is_empty())
        .map(ToString::to_string)
}

fn checksum_for_windows(text: &str, package_moniker: &str) -> Option<String> {
    let package_name = format!("{package_moniker}.Msix");
    checksum_for_name(text, &package_name)
        .or_else(|| checksum_for_name(text, package_moniker))
        .or_else(|| unique_windows_msix_checksum(text))
}

fn unique_windows_msix_checksum(text: &str) -> Option<String> {
    let mut matches = text.lines().filter_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if name.to_ascii_lowercase().ends_with(".msix") {
            Some(hash.to_string())
        } else {
            None
        }
    });
    let hash = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(hash)
    }
}

fn checksum_for_name(text: &str, expected_name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if name == expected_name || name.ends_with(&format!("/{expected_name}")) {
            Some(hash.to_string())
        } else {
            None
        }
    })
}

fn fetch_text(url: &str) -> Result<String, String> {
    download_http::fetch_text(
        url,
        Duration::from_secs(MIRROR_METADATA_TIMEOUT_SECS),
        download_http::DOWNLOAD_HTTP_MAX_ATTEMPTS,
    )
}

fn download_to_file<F>(
    url: &str,
    path: &Path,
    expected_total: Option<u64>,
    install_kind: &str,
    on_progress: &F,
) -> Result<(), String>
where
    F: Fn(ChatGptDesktopProgress),
{
    let temp = download_temp_path(path);
    emit_step_progress(
        on_progress,
        install_kind,
        "downloading",
        "chatgptDesktop.progressDownloading",
        Some(0),
        expected_total,
        Some(2),
        Some(4),
    );
    let downloaded = download_http::download_to_file(
        url,
        path,
        &temp,
        expected_total,
        Duration::from_secs(MIRROR_PACKAGE_TIMEOUT_SECS),
        download_http::DOWNLOAD_HTTP_MAX_ATTEMPTS,
        |downloaded, total| {
            emit_step_progress(
                on_progress,
                install_kind,
                "downloading",
                "chatgptDesktop.progressDownloading",
                Some(downloaded),
                total,
                Some(2),
                Some(4),
            );
        },
    )?;
    emit_step_progress(
        on_progress,
        install_kind,
        "downloading",
        "chatgptDesktop.progressDownloadComplete",
        Some(downloaded),
        expected_total.or(Some(downloaded)),
        Some(2),
        Some(4),
    );
    Ok(())
}

fn emit_step_progress<F>(
    on_progress: &F,
    install_kind: &str,
    phase: &str,
    message: impl Into<String>,
    downloaded: Option<u64>,
    total: Option<u64>,
    step: Option<u64>,
    step_total: Option<u64>,
) where
    F: Fn(ChatGptDesktopProgress),
{
    let percent = match (downloaded, total) {
        (Some(done), Some(total)) if total > 0 => {
            Some(((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0))
        }
        _ => None,
    };
    on_progress(ChatGptDesktopProgress {
        install_kind: install_kind.to_string(),
        phase: phase.to_string(),
        message: message.into(),
        downloaded,
        total,
        percent,
        step,
        step_total,
    });
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|err| format!("Failed to open file for SHA-256 calculation: {err}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 128];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("Failed to read file for SHA-256 calculation: {err}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn staged_package_path(release: &ChatGptDesktopRelease) -> Result<PathBuf, String> {
    let dir = staging_dir()?;
    let lower = release.package_moniker.to_ascii_lowercase();
    let file = if lower.ends_with(".msix") || lower.ends_with(".dmg") || lower.ends_with(".zip") {
        release.package_moniker.clone()
    } else if release.package_kind == "dmg" {
        format!("{}.dmg", release.package_moniker)
    } else {
        format!("{}.Msix", release.package_moniker)
    };
    Ok(dir.join(file))
}

enum StagedPackageTarget {
    Reuse,
    Download(PathBuf),
}

fn staged_package_target(path: &Path, sha256: &str) -> Result<StagedPackageTarget, String> {
    if !path.exists() {
        return Ok(StagedPackageTarget::Download(path.to_path_buf()));
    }

    if sha256_file(path)
        .ok()
        .is_some_and(|actual| actual.eq_ignore_ascii_case(sha256))
    {
        return Ok(StagedPackageTarget::Reuse);
    }

    match fs::remove_file(path) {
        Ok(()) => Ok(StagedPackageTarget::Download(path.to_path_buf())),
        Err(_) if path.exists() => Ok(StagedPackageTarget::Download(
            alternate_staged_package_path(path, sha256),
        )),
        Err(_) => Ok(StagedPackageTarget::Download(path.to_path_buf())),
    }
}

fn alternate_staged_package_path(path: &Path, sha256: &str) -> PathBuf {
    let short_sha: String = sha256.chars().take(8).collect();
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("download");
    let extension = path.extension().and_then(|extension| extension.to_str());
    let file_name = match extension {
        Some(extension) if !extension.is_empty() => format!("{stem}-{short_sha}.{extension}"),
        _ => format!("{stem}-{short_sha}"),
    };
    path.with_file_name(file_name)
}

fn download_temp_path(path: &Path) -> PathBuf {
    let sequence = DOWNLOAD_TEMP_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    path.with_file_name(format!(
        "{file_name}.download.{}.{}.{}",
        std::process::id(),
        sequence,
        nanos
    ))
}

fn staging_dir() -> Result<PathBuf, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    let dir = paths.downloads_dir.join("chatgpt-desktop");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn load_settings() -> Result<ChatGptDesktopSettings, String> {
    let (json, migrate_legacy) = match storage::load_state_json(CHATGPT_DESKTOP_SETTINGS_STATE_KEY)?
    {
        Some(json) => (json, false),
        None => match storage::load_state_json(LEGACY_CODEX_CLIENT_SETTINGS_STATE_KEY)? {
            Some(json) => (json, true),
            None => {
                let settings = ChatGptDesktopSettings::default();
                save_settings(&settings)?;
                return Ok(settings);
            }
        },
    };
    let mut settings: ChatGptDesktopSettings = serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse ChatGPT Desktop settings: {err}"))?;
    let mut settings_changed = false;
    settings.source = normalize_source(&settings.source);
    settings.custom_url = String::new();
    settings.signed_only = true;
    if settings.install_root.trim().is_empty() {
        settings.install_root = default_install_root();
        settings_changed = true;
    } else if cfg!(target_os = "macos")
        && settings.install_root == format!("/Applications/{LEGACY_CODEX_MACOS_APP_NAME}")
        && !Path::new(&settings.install_root).exists()
    {
        settings.install_root = default_macos_install_root();
        settings_changed = true;
    }
    if migrate_legacy || settings_changed {
        save_settings(&settings)?;
    }
    if migrate_legacy {
        let _ = storage::delete_state_json(LEGACY_CODEX_CLIENT_SETTINGS_STATE_KEY);
    }
    Ok(settings)
}

fn save_settings(settings: &ChatGptDesktopSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    storage::save_state_json(CHATGPT_DESKTOP_SETTINGS_STATE_KEY, &json)
        .map_err(|err| format!("Failed to save ChatGPT Desktop settings: {err}"))
}

fn save_marker(marker: &ManagedInstallMarker) -> Result<(), String> {
    let json = serde_json::to_string_pretty(marker).map_err(|err| err.to_string())?;
    storage::save_state_json(CHATGPT_DESKTOP_MARKER_STATE_KEY, &json)
        .map_err(|err| format!("Failed to save ChatGPT Desktop managed marker: {err}"))
}

fn load_marker() -> Option<ManagedInstallMarker> {
    if let Some(marker) = storage::load_state_json(CHATGPT_DESKTOP_MARKER_STATE_KEY)
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok())
    {
        return Some(marker);
    }
    let marker = storage::load_state_json(LEGACY_CODEX_CLIENT_MARKER_STATE_KEY)
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok())?;
    if save_marker(&marker).is_ok() {
        let _ = storage::delete_state_json(LEGACY_CODEX_CLIENT_MARKER_STATE_KEY);
    }
    Some(marker)
}

fn install_class(installed: Option<&InstalledChatGptDesktop>) -> String {
    let Some(installed) = installed else {
        return "none".to_string();
    };
    let Some(marker) = load_marker() else {
        return "external".to_string();
    };
    let marker_matches = marker
        .version
        .as_deref()
        .map(|version| compare_versions(version, &installed.version) == Ordering::Equal)
        .unwrap_or(true);
    if marker_matches {
        "managed".to_string()
    } else {
        "external".to_string()
    }
}

fn validate_install_target(settings: &ChatGptDesktopSettings) -> Result<(), String> {
    let path = expand_env_path(&settings.install_root)?;
    validate_install_path_for_platform(&path)
}

fn validate_install_path_for_platform(path: &Path) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        validate_install_root(path)
    } else if cfg!(target_os = "macos") {
        validate_macos_install_target(path)
    } else {
        Ok(())
    }
}

fn validate_macos_install_target(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Install location must be an absolute path.".to_string());
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("app"))
        != Some(true)
    {
        return Err("macOS install location must point to an .app bundle.".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "macOS install location has no parent directory.".to_string())?;
    if !parent.exists() {
        return Err("macOS install location parent directory does not exist.".to_string());
    }
    if path.exists() && !path.is_dir() {
        return Err(
            "macOS install location already exists but is not an app directory.".to_string(),
        );
    }
    Ok(())
}

fn validate_install_root(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Install location must be an absolute path.".to_string());
    }
    if path.parent().is_none() {
        return Err("Install location cannot be the disk root.".to_string());
    }
    if path.exists() && !path.is_dir() {
        return Err("Install location must be a folder.".to_string());
    }
    if path.exists() && !is_empty_dir(path)? && !is_existing_portable_root(path) {
        return Err(
        "Install location must be an empty folder or an existing ChatGPT Desktop portable directory."
                .to_string(),
        );
    }
    let protected = protected_roots();
    if protected
        .iter()
        .any(|root| path_is_equal_or_child(path, root))
    {
        return Err(
            "Install location cannot be inside a system or administrator directory.".to_string(),
        );
    }
    Ok(())
}

fn protected_roots() -> Vec<PathBuf> {
    [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "SystemRoot",
        "WINDIR",
    ]
    .iter()
    .filter_map(|name| std::env::var_os(name))
    .map(PathBuf::from)
    .collect()
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn path_is_equal_or_child(path: &Path, root: &Path) -> bool {
    let path = path_key(path);
    let root = path_key(root);
    path == root || path.starts_with(&format!("{root}\\"))
}

fn is_empty_dir(path: &Path) -> Result<bool, String> {
    Ok(fs::read_dir(path)
        .map_err(|err| format!("Failed to read install directory: {err}"))?
        .next()
        .is_none())
}

fn is_existing_portable_root(path: &Path) -> bool {
    path.join("Codex.exe").is_file() && path.join("AppxManifest.xml").is_file()
}

fn expand_env_path(raw: &str) -> Result<PathBuf, String> {
    let mut value = raw.trim().to_string();
    if cfg!(windows) {
        for (key, env_key) in [
            ("%LOCALAPPDATA%", "LOCALAPPDATA"),
            ("%APPDATA%", "APPDATA"),
            ("%USERPROFILE%", "USERPROFILE"),
        ] {
            if value.to_ascii_uppercase().starts_with(key) {
                let replacement = std::env::var(env_key)
                    .map_err(|_| format!("Environment variable {env_key} is unavailable."))?;
                value = format!("{replacement}{}", &value[key.len()..]);
            }
        }
    }
    Ok(PathBuf::from(value))
}

fn default_install_root() -> String {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join("AppData").join("Local")))
            .unwrap_or_else(|| PathBuf::from("C:\\Users\\Public\\AppData\\Local"))
            .join("Programs")
            .join("Codex")
            .to_string_lossy()
            .to_string()
    } else if cfg!(target_os = "macos") {
        default_macos_install_root()
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local")
            .join("share")
            .join("Codex")
            .to_string_lossy()
            .to_string()
    }
}

fn default_macos_install_root() -> String {
    format!("/Applications/{CHATGPT_MACOS_APP_NAME}")
}

fn platform_label() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        "unknown".to_string()
    }
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let len = left_parts.len().max(right_parts.len());
    for index in 0..len {
        let left = *left_parts.get(index).unwrap_or(&0);
        let right = *right_parts.get(index).unwrap_or(&0);
        match left.cmp(&right) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    Ordering::Equal
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn path_mtime(path: &Path) -> Option<String> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0))
        .map(|time| time.to_rfc3339())
}

fn close_chatgpt_desktop_processes(
    installed: &InstalledChatGptDesktop,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        let app = Path::new(&installed.path);
        let was_running = package::macos_app_running(app);
        let process_name = macos_process_name_for_installed(installed);
        package::quit_macos_app_bundle(app)
            .map_err(|err| format!("Failed to close {process_name}: {err}"))?;
        if was_running {
            notes.push("Closed the running ChatGPT desktop process.".to_string());
        }
        return Ok(());
    }

    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let report = if installed.source == "msix" {
        process_control::close_appx_package_for_update("ChatGPT Desktop", PACKAGE_IDENTITY)?
    } else {
        process_control::close_processes_for_update(
            "ChatGPT Desktop",
            &[CODEX_EXE_NAME],
            Some(Path::new(&installed.path)),
        )?
    };
    if report.total > 0 {
        if report.forced > 0 {
            notes.push(format!(
                "Force-closed {} running ChatGPT desktop process(es).",
                report.forced
            ));
        } else {
            notes.push("Closed the running ChatGPT desktop process.".to_string());
        }
    }
    Ok(())
}

fn macos_process_name_for_installed(installed: &InstalledChatGptDesktop) -> String {
    let app = Path::new(&installed.path);
    package::macos_app_executable_name(app)
        .or_else(|| {
            app.file_stem()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "ChatGPT".to_string())
}

fn macos_tool_command(installed: Option<&InstalledChatGptDesktop>) -> String {
    installed
        .and_then(|item| {
            Path::new(&item.path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| CHATGPT_MACOS_APP_NAME.to_string())
}

fn macos_open_command(installed: &InstalledChatGptDesktop, args: &[String]) -> Vec<String> {
    let mut command = vec![
        "open".to_string(),
        "-a".to_string(),
        installed.path.clone(),
        "--args".to_string(),
    ];
    command.extend(args.iter().cloned());
    command
}

fn launch_installed_codex(
    installed: &InstalledChatGptDesktop,
    args: &[String],
) -> Result<(), String> {
    if installed.source == "portable" {
        let exe = Path::new(&installed.path).join(CODEX_EXE_NAME);
        hidden_command(exe)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to launch ChatGPT Desktop: {err}"))?;
    } else if cfg!(target_os = "windows") {
        package::launch_msix_package_with_args(PACKAGE_IDENTITY, args)
            .map(|_| ())
            .map_err(|err| {
                format!("Failed to launch ChatGPT Desktop with patch arguments: {err}")
            })?;
    } else if cfg!(target_os = "macos") {
        let command = macos_open_command(installed, args);
        hidden_command(&command[0])
            .args(&command[1..])
            .spawn()
            .map(|_| ())
            .map_err(|err| {
                format!("Failed to launch ChatGPT Desktop with patch arguments: {err}")
            })?;
    } else {
        return Err(
            "Launching ChatGPT Desktop is not supported on the current platform.".to_string(),
        );
    }
    Ok(())
}

fn sync_history_if_enabled(settings: &ChatGptDesktopSettings) -> Result<(), String> {
    if !settings.sync_history_on_launch {
        return Ok(());
    }
    let report = codex_provider_sync::run_default_provider_sync()?;
    let _ = activity_log::append(
        Severity::Info,
        format!(
            "Synchronized ChatGPT Desktop history provider to {} ({} session files, {} sqlite rows).",
            report.target_provider, report.changed_session_files, report.sqlite_rows_updated
        ),
    );
    Ok(())
}

const COMPUTER_USE_GUARD_POST_LAUNCH_SECONDS: &[u64] = &[1, 3, 7, 15, 30, 60];
const COMPUTER_USE_GUARD_STABLE_ATTEMPTS: usize = 3;

fn ensure_official_remote_plugin_cache_if_enabled(settings: &ChatGptDesktopSettings) {
    if !settings.official_remote_plugin_cache_on_launch {
        return;
    }
    let home = match codex_home_dir() {
        Ok(home) => home,
        Err(error) => {
            let _ = activity_log::append(
                Severity::Warning,
                &format!("Skipped official remote plugin cache: {error}"),
            );
            return;
        }
    };
    match codex_plugin_marketplace::ensure_official_remote_plugin_cache(&home) {
        Ok(result) => {
            let message = if result.initialized {
                "Prepared official remote plugin cache from the bundled snapshot."
            } else if result.configured {
                "Registered official remote plugin cache in Codex config."
            } else {
                "Official remote plugin cache is already ready."
            };
            let _ = activity_log::append(Severity::Info, message);
        }
        Err(error) => {
            let _ = activity_log::append(
                Severity::Warning,
                &format!("Official remote plugin cache repair failed: {error}"),
            );
        }
    }
}

fn ensure_computer_use_guard_if_enabled(settings: &ChatGptDesktopSettings) -> Result<(), String> {
    if !settings.computer_use_guard_on_launch {
        return Ok(());
    }
    let home = codex_home_dir()?;
    let artifacts = computer_use_guard::resolve_computer_use_guard_artifacts(&home)?;
    let result = computer_use_guard::ensure_computer_use_config_with_artifacts(&home, &artifacts)?;
    let _ = activity_log::append(
        Severity::Info,
        if result.changed {
            "Prepared Codex Computer Use Guard launch configuration."
        } else {
            "Codex Computer Use Guard launch configuration is already ready."
        },
    );
    Ok(())
}

fn start_computer_use_guard_watchdog_if_enabled(settings: &ChatGptDesktopSettings) {
    if !settings.computer_use_guard_on_launch || !cfg!(target_os = "windows") {
        return;
    }
    let Ok(home) = codex_home_dir() else {
        return;
    };
    thread::spawn(move || run_post_launch_computer_use_guard(home));
}

fn codex_home_dir() -> Result<PathBuf, String> {
    app_paths()
        .map(|paths| paths.home_dir.join(".codex"))
        .map_err(|err| format!("Could not locate the Codex home directory: {err}"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexEnhancementInjectionSettings {
    plugin_marketplace_unlock: bool,
    plugin_auto_expand: bool,
    model_whitelist_unlock: bool,
    service_tier_controls: bool,
    model_catalog: CodexModelCatalog,
}

impl CodexEnhancementInjectionSettings {
    fn enabled(&self) -> bool {
        self.plugin_marketplace_unlock
            || self.plugin_auto_expand
            || self.model_whitelist_unlock
            || self.service_tier_controls
    }
}

#[derive(Debug, Clone, Serialize)]
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

fn codex_enhancement_settings_from(
    settings: &ChatGptDesktopSettings,
) -> CodexEnhancementInjectionSettings {
    CodexEnhancementInjectionSettings {
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

fn post_launch_guard_artifacts_ready(artifacts: &computer_use_guard::GuardArtifacts) -> bool {
    artifacts.notify_exe.is_some()
        && artifacts.marketplace_path.is_some()
        && (!artifacts.runtime_exports_needed || artifacts.sky_package_json.is_some())
}

fn should_stop_post_launch_computer_use_guard(
    stable_unchanged_attempts: usize,
    artifacts: &computer_use_guard::GuardArtifacts,
) -> bool {
    stable_unchanged_attempts >= COMPUTER_USE_GUARD_STABLE_ATTEMPTS
        && post_launch_guard_artifacts_ready(artifacts)
}

fn run_post_launch_computer_use_guard(home: PathBuf) {
    let mut previous_delay = 0_u64;
    let mut stable_unchanged_attempts = 0_usize;
    for (index, delay) in COMPUTER_USE_GUARD_POST_LAUNCH_SECONDS
        .iter()
        .copied()
        .enumerate()
    {
        let wait_seconds = delay.saturating_sub(previous_delay);
        previous_delay = delay;
        if wait_seconds > 0 {
            thread::sleep(Duration::from_secs(wait_seconds));
        }
        let attempt = index + 1;
        let artifacts = match computer_use_guard::resolve_computer_use_guard_artifacts(&home) {
            Ok(artifacts) => artifacts,
            Err(error) => {
                stable_unchanged_attempts = 0;
                let _ = activity_log::append(
                    Severity::Warning,
                    format!(
                        "Codex Computer Use Guard retry {attempt} could not resolve artifacts: {error}"
                    ),
                );
                continue;
            }
        };
        let artifacts_ready = post_launch_guard_artifacts_ready(&artifacts);
        match computer_use_guard::ensure_computer_use_config_with_artifacts(&home, &artifacts) {
            Ok(result) => {
                if !result.changed && artifacts_ready {
                    stable_unchanged_attempts += 1;
                } else {
                    stable_unchanged_attempts = 0;
                }
                if should_stop_post_launch_computer_use_guard(stable_unchanged_attempts, &artifacts)
                {
                    let _ = activity_log::append(
                        Severity::Info,
                        "Codex Computer Use Guard stopped after stable post-launch checks.",
                    );
                    return;
                }
            }
            Err(error) => {
                stable_unchanged_attempts = 0;
                let _ = activity_log::append(
                    Severity::Warning,
                    format!("Codex Computer Use Guard retry {attempt} failed: {error}"),
                );
            }
        }
    }
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
    settings: &CodexEnhancementInjectionSettings,
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

fn spawn_codex_enhancement_injection(debug_port: u16, settings: CodexEnhancementInjectionSettings) {
    thread::spawn(
        move || match inject_codex_enhancements(debug_port, &settings) {
            Ok(active_websocket_url) => {
                let _ =
                    activity_log::append(Severity::Ok, "Applied Codex launch enhancement patch.");
                watch_codex_enhancement_target(debug_port, &settings, active_websocket_url);
            }
            Err(err) => {
                let _ = activity_log::append(
                    Severity::Error,
                    format!("Codex launch enhancement patch failed: {err}"),
                );
            }
        },
    );
}

fn watch_codex_enhancement_target(
    debug_port: u16,
    settings: &CodexEnhancementInjectionSettings,
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
    settings: &CodexEnhancementInjectionSettings,
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

fn codex_enhancement_script(
    settings: &CodexEnhancementInjectionSettings,
) -> Result<String, String> {
    let settings_json = serde_json::to_string(settings)
        .map_err(|err| format!("Failed to serialize Codex enhancement settings: {err}"))?;
    let plugin_marketplaces_json =
        serde_json::to_string(&codex_plugin_marketplaces_for_injection())
            .map_err(|err| format!("Failed to serialize Codex plugin marketplaces: {err}"))?;
    let script = r#"
(() => {
  const codestudioLiteInjectedSettings = __CODESTUDIO_LITE_SETTINGS__;
  const codestudioLiteLocalPluginMarketplaces = __CODESTUDIO_LITE_PLUGIN_MARKETPLACES__;
  const codestudioLiteCodexEnhancementsVersion = "5";
  window.__codestudioLiteCodexEnhancementSettings = codestudioLiteInjectedSettings;
  window.__codestudioLitePluginMarketplaces = codestudioLiteLocalPluginMarketplaces;
  function codestudioLiteSettings() {
    return window.__codestudioLiteCodexEnhancementSettings || codestudioLiteInjectedSettings;
  }
  if (window.__codestudioLiteCodexEnhancements === codestudioLiteCodexEnhancementsVersion) {
    window.__codestudioLiteCodexEnhancementsRefresh?.();
    return true;
  }
  if (window.__codestudioLiteCodexEnhancementsTimer) {
    clearInterval(window.__codestudioLiteCodexEnhancementsTimer);
    window.__codestudioLiteCodexEnhancementsTimer = null;
  }
  if (window.__codestudioLiteCodexEnhancementsObserver) {
    window.__codestudioLiteCodexEnhancementsObserver.disconnect?.();
    window.__codestudioLiteCodexEnhancementsObserver = null;
  }
  window.__codestudioLiteCodexEnhancements = codestudioLiteCodexEnhancementsVersion;
  const styleId = "codestudio-lite-codex-enhancement-style";
  const pluginMarketplaceUnlockVersion = "3";
  const codexPluginAutoExpandVersion = "1";
  const codexPluginAutoExpandMaxClicks = 80;
  const codexPluginAutoExpandClickDelayMs = 90;
  const codexModelJsonResponsePatchVersion = "2";
  const codexModelMessagePatchVersion = "2";
  const codexStatsigModelWhitelistPatchVersion = "2";
  const codexAppServerModelRequestPatchVersion = "2";
  const codexServiceTierRequestOverrideVersion = "3";
  const codexServiceTierBadgeClass = "codestudio-lite-service-tier-badge";
  const codexServiceTierBadgeVersion = "3";
  const codexThreadServiceTierVersion = "1";
  const codexThreadServiceTierKey = "codestudioLiteCodexThreadServiceTierOverrides";
  const codexThreadServiceTierMaxEntries = 120;
  const codexThreadServiceTierDraftBindWindowMs = 60 * 1000;
  const codexDefaultServiceTierSetting = { key: "default-service-tier", default: null };
  const codexServiceTierFallbackFastValue = "priority";
  const codexServiceTierSupportedFastModels = new Set([
    "gpt-5.4",
    "gpt-5.5",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
  ]);
  const codexThreadServiceTierModes = new Set(["inherit", "standard", "fast"]);
  const codexServiceTierControlModes = new Set(["inherit", "global-standard", "global-fast", "custom"]);
  const modulePromises = new Map();
  let codexModelCatalog = normalizeModelCatalog(codestudioLiteSettings().modelCatalog);
  let codexModelCatalogPromise = null;
  let codexModelCatalogLoadedAt = 0;
  let codexModelWhitelistRefreshTimer = 0;
  let codexModelWhitelistRefreshUntil = 0;
  let codestudioLiteRefreshScheduled = false;
  let codestudioLitePendingMutations = null;
  let codestudioLiteSlowRefreshCount = 0;
  let codestudioLiteRefreshDisabledUntil = 0;
  let codexServiceTierComposerCache = { element: null, expiresAt: 0 };
  let codexServiceTierStateLoadStarted = false;
  let codexServiceTierState = {
    status: "loading",
    serviceTier: null,
    message: "正在读取…",
    fastTierValue: "priority",
    controlMode: "inherit",
    defaultMode: "inherit",
    activeThreadId: "",
    threadMode: "inherit",
    effectiveServiceTier: null,
    effectiveMode: "standard",
    fastModelName: "",
    fastSupported: false,
  };
  const codexModelListRequestIds = new Set();

  function ensureStyle() {
    if (document.getElementById(styleId)) return;
    const style = document.createElement("style");
    style.id = styleId;
    style.textContent = `
      .${codexServiceTierBadgeClass} {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex: 0 0 auto;
        height: 24px;
        min-width: 54px;
        box-sizing: border-box;
        border: 1px solid rgba(148,163,184,.28);
        border-radius: 999px;
        padding: 0 8px;
        font: 600 11px system-ui, sans-serif;
        color: inherit;
        background: rgba(148,163,184,.11);
        cursor: pointer;
      }
      .${codexServiceTierBadgeClass}:hover { border-color: rgba(16,163,127,.44); background: rgba(16,163,127,.13); }
      .${codexServiceTierBadgeClass}[data-tier="fast"] { border-color: rgba(16,163,127,.55); background: rgba(16,163,127,.18); color: #0f8f6a; }
      .${codexServiceTierBadgeClass}[data-tier="unsupported"] { border-color: rgba(251,191,36,.48); background: rgba(251,191,36,.13); color: #a16207; }
      .${codexServiceTierBadgeClass}[data-tier="loading"],
      .${codexServiceTierBadgeClass}[data-disabled="true"] { opacity: .62; cursor: not-allowed; }
      .codestudio-lite-codex-toast {
        position: fixed;
        left: 50%;
        bottom: 24px;
        transform: translateX(-50%);
        z-index: 2147483647;
        max-width: min(420px, calc(100vw - 32px));
        box-sizing: border-box;
        border: 1px solid rgba(148,163,184,.3);
        border-radius: 8px;
        padding: 9px 12px;
        color: #f8fafc;
        background: rgba(15,23,42,.94);
        box-shadow: 0 14px 40px rgba(15,23,42,.28);
        font: 500 12px/1.35 system-ui, sans-serif;
      }
    `;
    document.head.appendChild(style);
  }

  function recordPluginUnlockDiagnostic(event, payload = {}) {
    window.__codestudioLitePluginUnlockDiagnostics = window.__codestudioLitePluginUnlockDiagnostics || [];
    window.__codestudioLitePluginUnlockDiagnostics.push({ event, payload, at: Date.now() });
    if (window.__codestudioLitePluginUnlockDiagnostics.length > 80) {
      window.__codestudioLitePluginUnlockDiagnostics.splice(0, window.__codestudioLitePluginUnlockDiagnostics.length - 80);
    }
  }

  function codexAppAssetUrl(namePart) {
    const resources = [
      ...Array.from(document.scripts || []).map((script) => script.src),
      ...Array.from(document.querySelectorAll("link[href]") || []).map((link) => link.href),
      ...performance.getEntriesByType("resource").map((entry) => entry.name),
    ].filter(Boolean);
    return resources.find((url) => url.includes("/assets/") && url.includes(namePart) && url.split("?")[0].endsWith(".js")) || "";
  }

  async function codexAppAssetUrlFromScriptText(namePart) {
    const scripts = Array.from(document.scripts || []).map((script) => script.src).filter(Boolean);
    for (const src of scripts) {
      if (!src.includes("/assets/") || !src.split("?")[0].endsWith(".js")) continue;
      try {
        const text = await fetch(src).then((response) => response.ok ? response.text() : "");
        const escaped = namePart.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        const match = text.match(new RegExp(`["'](\\./assets/${escaped}[^"']+\\.js)["']`));
        if (match) return new URL(match[1], src).href;
      } catch {
      }
    }
    return "";
  }

  async function loadCodexAppModule(namePart) {
    if (!modulePromises.has(namePart)) {
      const promise = Promise.resolve().then(async () => {
        const url = codexAppAssetUrl(namePart) || await codexAppAssetUrlFromScriptText(namePart);
        if (!url) throw new Error(`Codex asset not found: ${namePart}`);
        return await import(url);
      }).catch((error) => {
        modulePromises.delete(namePart);
        throw error;
      });
      modulePromises.set(namePart, promise);
    }
    return await modulePromises.get(namePart);
  }

  async function codexSettingStorageModule() {
    const module = await loadCodexAppModule("setting-storage-");
    if (typeof module.n !== "function" || typeof module.s !== "function") {
      throw new Error("Codex setting-storage interface unavailable");
    }
    return module;
  }

  async function getCodexServiceTierSetting() {
    const settingStorage = await codexSettingStorageModule();
    return await settingStorage.n(codexDefaultServiceTierSetting);
  }

  function appServerPluginRequestMethod(method, params) {
    if (method === "send-cli-request-for-host" && params?.method) return String(params.method);
    return String(method || "");
  }

  function patchPluginMarketplaceRequestParams(method, params) {
    if (method === "list-plugins") {
      if (!params || typeof params !== "object") return params;
    } else {
      return params;
    }
    const next = { ...params };
    const hadMarketplaceKinds = Object.prototype.hasOwnProperty.call(next, "marketplaceKinds");
    const nextKinds = Array.isArray(next.marketplaceKinds)
      ? next.marketplaceKinds.map((kind) => restorePluginMarketplaceName(kind))
      : ["local"];
    if (!nextKinds.includes("vertical")) nextKinds.push("vertical");
    next.marketplaceKinds = Array.from(new Set(nextKinds));
    recordPluginUnlockDiagnostic("plugin_marketplace_request_expanded", {
      hadMarketplaceKinds,
      marketplaceKinds: next.marketplaceKinds,
      cwdCount: Array.isArray(next.cwds) ? next.cwds.length : 0,
    });
    return next;
  }

  function displayNameForPluginMarketplaceName(name, fallback) {
    if (name === "openai-bundled" || name === "codestudio-lite-openai-bundled") return "OpenAI插件1(CodeStudio)";
    if (name === "openai-curated" || name === "codestudio-lite-openai-curated") return "OpenAI插件2(CodeStudio)";
    if (name === "openai-primary-runtime" || name === "codestudio-lite-openai-primary-runtime") return "OpenAI插件3(CodeStudio)";
    if (name === "openai-api-curated" || name === "codestudio-lite-openai-api-curated") return "OpenAI插件4(CodeStudio)";
    if (name === "openai-curated-remote" || name === "codestudio-lite-openai-curated-remote") return "OpenAI插件5(CodeStudio)";
    return fallback;
  }

  function patchPluginMarketplaceObject(marketplace) {
    if (!marketplace || typeof marketplace !== "object" || marketplace.__codestudioLiteMarketplaceUnlockPatched) return false;
    const displayName = displayNameForPluginMarketplaceName(marketplace.name, marketplace.displayName || marketplace.title || marketplace.label || marketplace.name);
    if (!displayName || displayName === marketplace.name) return false;
    marketplace.displayName = displayName;
    marketplace.title = displayName;
    marketplace.label = displayName;
    if (marketplace.interface && typeof marketplace.interface === "object") {
      marketplace.interface = {
        ...marketplace.interface,
        displayName,
        name: displayName,
        title: displayName,
        label: displayName,
      };
    } else {
      marketplace.interface = { displayName, name: displayName, title: displayName, label: displayName };
    }
    marketplace.__codestudioLiteMarketplaceUnlockPatched = true;
    return true;
  }

  function cloneCodexPluginMarketplace(value) {
    if (!value || typeof value !== "object") return null;
    try {
      return JSON.parse(JSON.stringify(value));
    } catch (_) {
      return null;
    }
  }

  function pluginMarketplacePluginKey(plugin) {
    if (!plugin || typeof plugin !== "object") return "";
    return String(plugin.name || plugin.id || plugin.pluginName || "").trim();
  }

  function normalizeLocalPluginMarketplacePlugin(plugin, marketplaceName) {
    const cloned = cloneCodexPluginMarketplace(plugin);
    if (!cloned || typeof cloned !== "object") return null;
    const name = String(cloned.name || cloned.id || cloned.pluginName || "").trim();
    if (!name) return null;
    if (!cloned.name) cloned.name = name;
    if (!cloned.id) cloned.id = `${name}@${marketplaceName}`;
    if (!cloned.marketplaceName) cloned.marketplaceName = marketplaceName;
    if (!cloned.marketplacePath) cloned.marketplacePath = marketplaceName;
    if (!cloned.interface || typeof cloned.interface !== "object") cloned.interface = {};
    if (!cloned.interface.displayName) cloned.interface.displayName = name;
    if (!Array.isArray(cloned.keywords)) cloned.keywords = [];
    return cloned;
  }

  function mergePluginMarketplacePlugins(target, source) {
    if (!target || !source || !Array.isArray(source.plugins)) return 0;
    if (!Array.isArray(target.plugins)) target.plugins = [];
    const marketplaceName = restorePluginMarketplaceName(target.name || source.name || "");
    const existing = new Set(target.plugins.map(pluginMarketplacePluginKey).filter(Boolean));
    let added = 0;
    source.plugins.forEach((plugin) => {
      const key = pluginMarketplacePluginKey(plugin);
      if (!key || existing.has(key)) return;
      const cloned = normalizeLocalPluginMarketplacePlugin(plugin, marketplaceName);
      if (!cloned) return;
      target.plugins.push(cloned);
      existing.add(key);
      added += 1;
    });
    return added;
  }

  function mergeLocalPluginMarketplaces(result) {
    if (!result || typeof result !== "object" || !Array.isArray(result.marketplaces)) {
      return { addedMarketplaces: 0, addedPlugins: 0 };
    }
    const localMarketplaces = Array.isArray(window.__codestudioLitePluginMarketplaces)
      ? window.__codestudioLitePluginMarketplaces
      : [];
    if (!localMarketplaces.length) return { addedMarketplaces: 0, addedPlugins: 0 };
    const byName = new Map();
    result.marketplaces.forEach((marketplace) => {
      const name = restorePluginMarketplaceName(marketplace?.name || "");
      if (name) byName.set(name, marketplace);
    });
    let addedMarketplaces = 0;
    let addedPlugins = 0;
    localMarketplaces.forEach((marketplace) => {
      const name = restorePluginMarketplaceName(marketplace?.name || "");
      if (!name) return;
      const existing = byName.get(name);
      if (existing) {
        addedPlugins += mergePluginMarketplacePlugins(existing, marketplace);
        return;
      }
      const cloned = cloneCodexPluginMarketplace(marketplace);
      if (!cloned) return;
      cloned.plugins = Array.isArray(cloned.plugins)
        ? cloned.plugins.map((plugin) => normalizeLocalPluginMarketplacePlugin(plugin, name)).filter(Boolean)
        : [];
      result.marketplaces.push(cloned);
      byName.set(name, cloned);
      addedMarketplaces += 1;
      addedPlugins += cloned.plugins.length;
    });
    if (addedMarketplaces > 0 || addedPlugins > 0) {
      recordPluginUnlockDiagnostic("plugin_marketplace_local_merged", { addedMarketplaces, addedPlugins });
    }
    return { addedMarketplaces, addedPlugins };
  }

  function restorePluginMarketplaceName(name) {
    if (name === "codestudio-lite-openai-bundled" || name === "codex-plus-openai-bundled") return "openai-bundled";
    if (name === "codestudio-lite-openai-curated" || name === "codex-plus-openai-curated") return "openai-curated";
    if (name === "codestudio-lite-openai-primary-runtime" || name === "codex-plus-openai-primary-runtime") return "openai-primary-runtime";
    if (name === "codestudio-lite-openai-api-curated" || name === "codex-plus-openai-api-curated") return "openai-api-curated";
    if (name === "codestudio-lite-openai-curated-remote" || name === "codex-plus-openai-curated-remote") return "openai-curated-remote";
    return name;
  }

  function codexPluginOfficialMarketplaceName(name) {
    const restored = restorePluginMarketplaceName(name);
    return restored === "openai-bundled" || restored === "openai-curated" || restored === "openai-primary-runtime" || restored === "openai-api-curated" || restored === "openai-curated-remote";
  }

  function isCodexPluginBuildFlavorFilter(callback, sample) {
    if (!Array.isArray(sample) || sample.length === 0 || typeof callback !== "function") return false;
    let source = "";
    try {
      source = Function.prototype.toString.call(callback);
    } catch (_) {
      return false;
    }
    if (!source.includes("!u(e.marketplaceName)||e.marketplaceName===r")) return false;
    if (!sample.some((plugin) => codexPluginOfficialMarketplaceName(plugin?.marketplaceName))) return false;
    return sample.some((plugin) => codexPluginOfficialMarketplaceName(plugin?.marketplaceName) && !callback(plugin));
  }

  function isCodexPluginMarketplaceHiddenFilter(callback, sample) {
    if (!Array.isArray(sample) || sample.length === 0 || typeof callback !== "function") return false;
    let source = "";
    try {
      source = Function.prototype.toString.call(callback);
    } catch (_) {
      return false;
    }
    if (!source.includes("!t.includes(e.name)")) return false;
    if (!sample.some((marketplace) => codexPluginOfficialMarketplaceName(marketplace?.name))) return false;
    return sample.some((marketplace) => codexPluginOfficialMarketplaceName(marketplace?.name) && !callback(marketplace));
  }

  function installPluginBuildFlavorFilterPatch() {
    if (window.__codestudioLitePluginBuildFlavorFilterPatch === pluginMarketplaceUnlockVersion) return;
    const originalFilter = Array.prototype.__codestudioLitePluginBuildFlavorOriginalFilter || Array.prototype.filter;
    if (!Array.prototype.__codestudioLitePluginBuildFlavorOriginalFilter) {
      Object.defineProperty(Array.prototype, "__codestudioLitePluginBuildFlavorOriginalFilter", {
        value: originalFilter,
        configurable: true,
        writable: true,
      });
    }
    if (Array.prototype.filter.__codestudioLitePluginBuildFlavorPatched === pluginMarketplaceUnlockVersion) {
      window.__codestudioLitePluginBuildFlavorFilterPatch = pluginMarketplaceUnlockVersion;
      return;
    }
    const patchedFilter = function codestudioLitePluginBuildFlavorFilterPatch(callback, thisArg) {
      if (isCodexPluginBuildFlavorFilter(callback, this)) {
        recordPluginUnlockDiagnostic("plugin_build_flavor_filter_bypassed", { pluginCount: this.length });
        return Array.from(this);
      }
      if (isCodexPluginMarketplaceHiddenFilter(callback, this)) {
        recordPluginUnlockDiagnostic("plugin_marketplace_hidden_filter_bypassed", { marketplaceCount: this.length });
        return Array.from(this);
      }
      return originalFilter.call(this, callback, thisArg);
    };
    patchedFilter.__codestudioLitePluginBuildFlavorPatched = pluginMarketplaceUnlockVersion;
    Array.prototype.filter = patchedFilter;
    window.__codestudioLitePluginBuildFlavorFilterPatch = pluginMarketplaceUnlockVersion;
    recordPluginUnlockDiagnostic("plugin_build_flavor_filter_patch_installed");
  }

  function restorePluginMarketplaceRequestParams(params, method = "") {
    if (!params || typeof params !== "object") return params;
    let next = params;
    if (Array.isArray(params.marketplaceKinds)) {
      const nextKinds = params.marketplaceKinds.map((kind) => {
        if (kind === "remote:openai-curated") return "openai-curated";
        return restorePluginMarketplaceName(kind);
      });
      next = { ...next, marketplaceKinds: Array.from(new Set(nextKinds)) };
    }
    if (method === "install-plugin") {
      next = next === params ? { ...params } : { ...next };
      if (next.remoteMarketplaceName) next.remoteMarketplaceName = restorePluginMarketplaceName(next.remoteMarketplaceName);
      if (typeof next.marketplacePath === "string" && next.marketplacePath.startsWith("remote:")) {
        const remoteMarketplaceName = next.marketplacePath.slice("remote:".length);
        delete next.marketplacePath;
        next.remoteMarketplaceName = restorePluginMarketplaceName(remoteMarketplaceName);
      }
    }
    return next;
  }

  function patchPluginMarketplaceResult(method, result) {
    if (method !== "list-plugins") return result;
    let patchedCount = 0;
    try {
      if (Array.isArray(result?.marketplaces)) {
        mergeLocalPluginMarketplaces(result);
        result.marketplaces.forEach((marketplace) => {
          if (patchPluginMarketplaceObject(marketplace)) patchedCount += 1;
        });
      }
      if (patchedCount > 0) {
        recordPluginUnlockDiagnostic("plugin_marketplace_response_expanded", { patchedCount });
      }
    } catch (error) {
      recordPluginUnlockDiagnostic("plugin_marketplace_response_patch_failed", {
        errorName: error?.name || "",
        errorMessage: error?.message || String(error),
      });
    }
    return result;
  }

  function patchPluginMarketplaceRequestClient(client) {
    if (!client || typeof client.sendRequest !== "function") return false;
    if (client.__codestudioLitePluginMarketplaceUnlockPatch === pluginMarketplaceUnlockVersion) return true;
    const originalSendRequest = client.__codestudioLitePluginMarketplaceOriginalSendRequest || client.sendRequest.bind(client);
    client.__codestudioLitePluginMarketplaceOriginalSendRequest = originalSendRequest;
    client.sendRequest = async function codestudioLitePluginMarketplacePatchedSendRequest(method, params, options) {
      const requestMethod = appServerPluginRequestMethod(String(method || ""), params);
      const requestParams = patchPluginMarketplaceRequestParams(requestMethod, restorePluginMarketplaceRequestParams(params, requestMethod));
      if (requestMethod === "install-plugin") {
        recordPluginUnlockDiagnostic("plugin_install_request_debug", {
          method: String(method || ""),
          requestMarketplacePath: requestParams?.marketplacePath || null,
          requestRemoteMarketplaceName: requestParams?.remoteMarketplaceName || null,
          requestPluginName: requestParams?.pluginName || null,
        });
      }
      const result = await originalSendRequest(method, requestParams, options);
      return patchPluginMarketplaceResult(requestMethod, result);
    };
    client.__codestudioLitePluginMarketplaceUnlockPatch = pluginMarketplaceUnlockVersion;
    return true;
  }

  function installPluginMarketplaceRequestPatch() {
    if (window.__codestudioLitePluginMarketplaceUnlockInstalled === pluginMarketplaceUnlockVersion) return;
    if (window.__codestudioLitePluginMarketplaceUnlockPending) return;
    window.__codestudioLitePluginMarketplaceUnlockPending = true;
    Promise.resolve().then(async () => {
      const module = await loadCodexAppModule("app-server-manager-signals-");
      const candidates = Object.values(module).filter((value) => value && typeof value === "object");
      let patchedCount = 0;
      for (const candidate of candidates) {
        if (patchPluginMarketplaceRequestClient(candidate)) patchedCount += 1;
        if (typeof candidate.sendRequest !== "function" && typeof candidate.get === "function") {
          try {
            if (patchPluginMarketplaceRequestClient(candidate.get())) patchedCount += 1;
          } catch (_) {
          }
        }
      }
      if (patchedCount > 0) {
        window.__codestudioLitePluginMarketplaceUnlockInstalled = pluginMarketplaceUnlockVersion;
        recordPluginUnlockDiagnostic("plugin_marketplace_request_patch_installed", {
          candidateCount: candidates.length,
          patchedCount,
        });
      } else {
        recordPluginUnlockDiagnostic("plugin_marketplace_request_patch_not_found", {
          exportCount: Object.keys(module || {}).length,
          candidateCount: candidates.length,
        });
      }
    }).catch((error) => {
      recordPluginUnlockDiagnostic("plugin_marketplace_request_patch_failed", {
        errorName: error?.name || "",
        errorMessage: error?.message || String(error),
      });
    }).finally(() => {
      window.__codestudioLitePluginMarketplaceUnlockPending = false;
    });
  }

  function recordCodexEnhancementDiagnostic(event, payload = {}) {
    recordPluginUnlockDiagnostic(event, payload);
  }

  function uniqueValues(values) {
    return Array.from(new Set((values || []).map((value) => String(value || "").trim()).filter(Boolean)));
  }

  function setCodestudioLiteText(node, value) {
    const next = String(value ?? "");
    if (node.textContent !== next) node.textContent = next;
  }

  function setCodestudioLiteAttribute(node, name, value) {
    const next = String(value ?? "");
    if (node.getAttribute(name) !== next) node.setAttribute(name, next);
  }

  function setCodestudioLiteProperty(node, name, value) {
    if (node[name] !== value) node[name] = value;
  }

  function setCodestudioLiteBooleanProperty(node, name, value) {
    const next = !!value;
    if (node[name] !== next) node[name] = next;
  }

  function setCodestudioLiteDataset(node, name, value) {
    const next = String(value ?? "");
    if (node.dataset[name] !== next) node.dataset[name] = next;
  }

  function normalizeModelCatalog(value) {
    const source = value && typeof value === "object" ? value : {};
    return {
      status: source.status || "ok",
      model: String(source.model || ""),
      default_model: String(source.default_model || source.defaultModel || ""),
      model_provider: String(source.model_provider || source.modelProvider || ""),
      provider_name: String(source.provider_name || source.providerName || ""),
      models: uniqueValues(source.models || []),
      sources: Array.isArray(source.sources) ? source.sources : [],
      responses_api: source.responses_api || source.responsesApi || { status: "unknown", message: "" },
    };
  }

  function finiteNonNegativeNumber(value) {
    const numeric = Number(value);
    return Number.isFinite(numeric) && numeric >= 0 ? numeric : 0;
  }

  function validThreadScrollSessionKey(sessionId) {
    const key = String(sessionId || "").trim();
    if (!key || key === "__proto__" || key === "prototype" || key === "constructor") return "";
    return /^[A-Za-z0-9_.-]{8,128}$/.test(key) ? key : "";
  }

  function locationThreadId() {
    const source = `${window.location.pathname}${window.location.search}${window.location.hash}`;
    const match = source.match(/(?:session|conversation|thread)(?:\/|=|:|-)([A-Za-z0-9_.-]+)/i)
      || source.match(/\/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})(?:[/?#]|$)/)
      || source.match(/\/([A-Za-z0-9_-]{24,})(?:[/?#]|$)/);
    return match ? decodeURIComponent(match[1]) : "";
  }

  function currentSessionRef() {
    return { session_id: locationThreadId(), title: "" };
  }

  function showToast(message) {
    document.querySelectorAll(".codestudio-lite-codex-toast").forEach((node) => node.remove());
    const toast = document.createElement("div");
    toast.className = "codestudio-lite-codex-toast";
    toast.textContent = message;
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 5000);
  }

  function pluginAutoExpandVisibleElement(el) {
    if (!(el instanceof HTMLElement) || !el.isConnected) return false;
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden" || style.pointerEvents === "none") return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  function pluginAutoExpandPageLooksRelevant() {
    const text = String(document.body?.innerText || "");
    return /插件|Plugins?|Marketplace|市场/i.test(text) && !!document.querySelector('button, [role="button"]');
  }

  function pluginAutoExpandButtonLooksScoped(button) {
    let node = button;
    for (let depth = 0; node instanceof HTMLElement && node !== document.body && depth < 8; depth += 1, node = node.parentElement) {
      const text = String(node.innerText || "");
      if (text.length > 16000) continue;
      if (/插件|Plugins?|Marketplace|市场/i.test(text)) return true;
    }
    return false;
  }

  function pluginAutoExpandButtonText(button) {
    return String(button?.textContent || button?.getAttribute?.("aria-label") || button?.getAttribute?.("title") || "")
      .replace(/\s+/g, " ")
      .trim();
  }

  function pluginAutoExpandButtonLooksLikeMore(button) {
    const text = pluginAutoExpandButtonText(button);
    if (!text || text.length > 120) return false;
    if (/^(更多|显示更多|查看更多|加载更多|Show more|Load more|More)$/i.test(text)) return true;
    if (/^查看\s+.+以及另外\s*\d+\s*个$/i.test(text)) return true;
    if (/^View\s+.+\s+and\s+\d+\s+more$/i.test(text)) return true;
    if (/^Show\s+.+\s+and\s+\d+\s+more$/i.test(text)) return true;
    return false;
  }

  function pluginAutoExpandButtonCandidates() {
    if (!codestudioLiteSettings().pluginAutoExpand || !pluginAutoExpandPageLooksRelevant()) return [];
    return Array.from(document.querySelectorAll('button, [role="button"]'))
      .filter(pluginAutoExpandVisibleElement)
      .filter((button) => !button.disabled && button.getAttribute("aria-disabled") !== "true")
      .filter(pluginAutoExpandButtonLooksLikeMore)
      .filter(pluginAutoExpandButtonLooksScoped)
      .filter((button) => !button.closest?.(`#${styleId}, .${codexServiceTierBadgeClass}`));
  }

  function pluginAutoExpandSignature() {
    return pluginAutoExpandButtonCandidates()
      .map((button) => {
        const rect = button.getBoundingClientRect();
        return `${pluginAutoExpandButtonText(button)}:${Math.round(rect.top)}:${Math.round(rect.left)}`;
      })
      .join("|");
  }

  function schedulePluginAutoExpand(force = false) {
    if (!codestudioLiteSettings().pluginAutoExpand) return;
    if (window.__codexPluginAutoExpandRunning && !force) return;
    clearTimeout(window.__codexPluginAutoExpandTimer);
    window.__codexPluginAutoExpandTimer = setTimeout(() => runPluginAutoExpand(force), force ? 30 : 180);
  }

  function runPluginAutoExpand(force = false) {
    if (!codestudioLiteSettings().pluginAutoExpand) return;
    const currentSignature = pluginAutoExpandSignature();
    if (!force && currentSignature && currentSignature === window.__codexPluginAutoExpandLastSignature) return;
    window.__codexPluginAutoExpandLastSignature = currentSignature;
    window.__codexPluginAutoExpandRunning = true;
    window.__codexPluginAutoExpandClicks = 0;
    const clickNext = () => {
      if (!codestudioLiteSettings().pluginAutoExpand) {
        window.__codexPluginAutoExpandRunning = false;
        return;
      }
      const button = pluginAutoExpandButtonCandidates()[0];
      if (!button || window.__codexPluginAutoExpandClicks >= codexPluginAutoExpandMaxClicks) {
        window.__codexPluginAutoExpandRunning = false;
        recordCodexEnhancementDiagnostic("plugin_auto_expand_finished", {
          version: codexPluginAutoExpandVersion,
          clicks: window.__codexPluginAutoExpandClicks || 0,
          exhausted: !!button,
        });
        return;
      }
      window.__codexPluginAutoExpandClicks = (window.__codexPluginAutoExpandClicks || 0) + 1;
      button.dataset.codexPluginAutoExpandClicked = String(Date.now());
      button.click();
      setTimeout(clickNext, codexPluginAutoExpandClickDelayMs);
    };
    clickNext();
  }

  function codexPlusModelUnlockEnabled() {
    return !!codestudioLiteSettings().modelWhitelistUnlock;
  }

  function codexPlusModelNames() {
    codexModelCatalog = normalizeModelCatalog(codestudioLiteSettings().modelCatalog || codexModelCatalog);
    return uniqueValues([
      codexModelCatalog.default_model,
      codexModelCatalog.model,
      ...(Array.isArray(codexModelCatalog.models) ? codexModelCatalog.models : []),
    ]);
  }

  async function loadCodexModelCatalog(force = false) {
    if (!force && codexModelCatalogPromise) return codexModelCatalogPromise;
    if (!force && codexModelCatalogLoadedAt && Date.now() - codexModelCatalogLoadedAt < 10000) return codexModelCatalog;
    codexModelCatalogPromise = Promise.resolve().then(() => {
      codexModelCatalog = normalizeModelCatalog(codestudioLiteSettings().modelCatalog);
      codexModelCatalogLoadedAt = Date.now();
      scheduleCodexModelWhitelistRefresh();
      return codexModelCatalog;
    }).finally(() => {
      codexModelCatalogPromise = null;
    });
    return codexModelCatalogPromise;
  }

  function modelReasoningEfforts() {
    return ["minimal", "low", "medium", "high", "xhigh"].map((reasoningEffort) => ({ reasoningEffort, description: `${reasoningEffort} effort` }));
  }

  function codexPlusModelDescriptor(modelName) {
    return {
      model: modelName,
      id: modelName,
      slug: modelName,
      name: modelName,
      displayName: modelName,
      description: codexModelCatalog.provider_name || codexModelCatalog.model_provider || "Custom model",
      hidden: false,
      isDefault: (codexModelCatalog.default_model || codexModelCatalog.model) === modelName,
      defaultReasoningEffort: "medium",
      supportedReasoningEfforts: modelReasoningEfforts(),
    };
  }

  function modelArrayLooksPatchable(value, allowEmpty = false) {
    return Array.isArray(value)
      && (allowEmpty || value.length > 0)
      && value.every((item) => item && typeof item === "object" && typeof item.model === "string");
  }

  function stringArrayLooksPatchable(value) {
    return Array.isArray(value) && value.every((item) => typeof item === "string");
  }

  function patchModelNameArray(models) {
    if (!stringArrayLooksPatchable(models)) return false;
    const customModels = codexPlusModelNames();
    if (!customModels.length) return false;
    let changed = false;
    customModels.forEach((modelName) => {
      if (!models.includes(modelName)) {
        models.push(modelName);
        changed = true;
      }
    });
    return changed;
  }

  function patchModelArray(models, allowEmpty = false) {
    if (!modelArrayLooksPatchable(models, allowEmpty)) return false;
    const customModels = codexPlusModelNames();
    if (!customModels.length) return false;
    let changed = false;
    const existing = new Map(models.map((item) => [item.model, item]));
    models.forEach((item) => {
      if (customModels.includes(item.model) && item.hidden !== false) {
        item.hidden = false;
        changed = true;
      }
    });
    customModels.forEach((modelName) => {
      if (!existing.has(modelName)) {
        models.push(codexPlusModelDescriptor(modelName));
        changed = true;
      }
    });
    return changed;
  }

  function patchModelContainer(value) {
    if (!value || typeof value !== "object") return false;
    let changed = false;
    if (patchModelArray(value.models, "defaultModel" in value || "availableModels" in value)) changed = true;
    if (patchModelNameArray(value.models)) changed = true;
    if (patchModelArray(value.data)) changed = true;
    if (patchModelArray(value.result)) changed = true;
    if (patchModelArray(value.pages?.[0]?.data)) changed = true;
    if (patchModelArray(value.result?.data)) changed = true;
    if (patchModelArray(value.result?.models)) changed = true;
    if (patchModelArray(value.message?.result?.data)) changed = true;
    if (patchModelArray(value.message?.result?.models)) changed = true;
    const names = codexPlusModelNames();
    for (const key of ["availableModels", "available_models"]) {
      if (value[key] instanceof Set) {
        names.forEach((name) => {
          if (!value[key].has(name)) {
            value[key].add(name);
            changed = true;
          }
        });
      } else if (Array.isArray(value[key])) {
        names.forEach((name) => {
          if (!value[key].includes(name)) {
            value[key].push(name);
            changed = true;
          }
        });
      }
    }
    for (const key of ["hiddenModels", "hidden_models"]) {
      if (Array.isArray(value[key])) {
        const before = value[key].length;
        value[key] = value[key].filter((name) => !names.includes(name));
        if (value[key].length !== before) changed = true;
      }
    }
    const customNames = codexPlusModelUnlockEnabled() ? codexPlusModelNames() : [];
    if (value.defaultModel == null && customNames.length > 0) {
      value.defaultModel = codexPlusModelDescriptor(customNames[0]);
      changed = true;
    } else if (typeof value.defaultModel === "string" && customNames.includes(value.defaultModel) && value.model == null) {
      value.model = value.defaultModel;
      changed = true;
    }
    return changed;
  }

  function patchObjectGraphForModels(root, visited, depth = 0) {
    if (!root || typeof root !== "object" || visited.has(root) || depth > 5) return false;
    visited.add(root);
    let changed = patchModelContainer(root);
    if (root instanceof Element || root === window || root === document || root === document.body || root === document.documentElement) return changed;
    for (const key of Object.keys(root)) {
      if (key === "ownerDocument" || key === "parentElement" || key === "parentNode" || key === "children" || key === "childNodes") continue;
      let value;
      try {
        value = root[key];
      } catch {
        continue;
      }
      if (value && typeof value === "object" && patchObjectGraphForModels(value, visited, depth + 1)) changed = true;
    }
    return changed;
  }

  async function patchModelJsonResponse(payload) {
    if (!codexPlusModelUnlockEnabled()) return payload;
    if (!codexPlusModelNames().length) await loadCodexModelCatalog();
    if (!payload || typeof payload !== "object") return payload;
    try {
      patchModelContainer(payload);
      patchObjectGraphForModels(payload, new WeakSet(), 0);
    } catch (error) {
      recordCodexEnhancementDiagnostic("model_json_patch_failed", { errorMessage: String(error?.message || error) });
    }
    return payload;
  }

  function installModelJsonResponsePatch() {
    if (window.__codestudioLiteModelJsonResponsePatchInstalled === codexModelJsonResponsePatchVersion) return;
    window.__codestudioLiteModelJsonResponsePatchInstalled = codexModelJsonResponsePatchVersion;
    window.__codestudioLiteModelJsonResponseOriginals = window.__codestudioLiteModelJsonResponseOriginals || {};
    const originals = window.__codestudioLiteModelJsonResponseOriginals;
    originals.responseJson = originals.responseJson || Response.prototype.json;
    if (typeof originals.responseJson !== "function") return;
    Response.prototype.json = async function codestudioLitePatchedResponseJson(...args) {
      const payload = await originals.responseJson.apply(this, args);
      return await patchModelJsonResponse(payload);
    };
  }

  function patchStatsigModelDynamicConfig(config) {
    const names = codexPlusModelNames();
    const value = config?.value;
    if (!names.length || !value || typeof value !== "object") return config;
    const availableModels = Array.isArray(value.available_models) ? [...value.available_models] : [];
    let changed = false;
    names.forEach((name) => {
      if (!availableModels.includes(name)) {
        availableModels.push(name);
        changed = true;
      }
    });
    const nextValue = {
      ...value,
      available_models: availableModels,
      default_model: names[0] || value.default_model,
    };
    if (!changed && nextValue.default_model === value.default_model) return config;
    try {
      config.value = nextValue;
    } catch {
      return { ...config, value: nextValue };
    }
    return config;
  }

  function statsigClients() {
    const root = window.__STATSIG__ || globalThis.__STATSIG__;
    if (!root || typeof root !== "object") return [];
    const clients = [root.firstInstance, typeof root.instance === "function" ? root.instance() : null];
    if (root.instances && typeof root.instances === "object") clients.push(...Object.values(root.instances));
    return clients.filter((client, index, array) => client && typeof client === "object" && array.indexOf(client) === index);
  }

  function patchStatsigModelWhitelist() {
    statsigClients().forEach((client) => {
      if (typeof client.getDynamicConfig !== "function") return;
      if (client.__codestudioLiteModelWhitelistPatched !== codexStatsigModelWhitelistPatchVersion) {
        const originalGetDynamicConfig = client.getDynamicConfig.bind(client);
        client.getDynamicConfig = (name, options) => {
          const result = originalGetDynamicConfig(name, options);
          return patchStatsigModelDynamicConfig(result);
        };
        client.__codestudioLiteModelWhitelistPatched = codexStatsigModelWhitelistPatchVersion;
      }
      try {
        patchStatsigModelDynamicConfig(client.getDynamicConfig("107580212", { disableExposureLog: true }));
      } catch {
      }
    });
  }

  function reactFiberKeys(element) {
    return Object.keys(element || {}).filter((key) => key.startsWith("__reactFiber") || key.startsWith("__reactInternalInstance") || key.startsWith("__reactProps"));
  }

  function patchReactModelStateNodes() {
    const selector = "[role='menu'], [role='dialog'], [role='listbox'], [data-radix-popper-content-wrapper]";
    return [document.body, ...document.querySelectorAll(selector)].filter(Boolean);
  }

  function patchReactModelState() {
    const visited = new WeakSet();
    let changed = false;
    for (const node of patchReactModelStateNodes().slice(0, 220)) {
      for (const key of reactFiberKeys(node)) {
        if (patchObjectGraphForModels(node[key], visited)) changed = true;
      }
    }
    return changed;
  }

  function shouldScheduleReactModelStatePatch(mutations) {
    if (!codexPlusModelUnlockEnabled() || !codexPlusModelNames().length || !mutations) return false;
    const selector = "[role='menu'], [role='dialog'], [role='listbox'], [data-radix-popper-content-wrapper]";
    return mutations.some((mutation) => [...mutation.addedNodes].some((node) => (
      node.nodeType === 1 && (!!node.matches?.(selector) || !!node.querySelector?.(selector))
    )));
  }

  function patchAppServerModelMessages() {
    if (window.__codestudioLiteModelMessagePatchInstalled === codexModelMessagePatchVersion) return;
    window.__codestudioLiteModelMessagePatchInstalled = codexModelMessagePatchVersion;
    const originalDispatchEvent = window.dispatchEvent;
    window.dispatchEvent = function patchedCodestudioLiteDispatchEvent(event) {
      try {
        const detail = event?.detail;
        const request = detail?.request;
        if (event?.type === "codex-message-from-view" && detail?.type === "mcp-request" && request?.method === "model/list") {
          request.params = { ...(request.params || {}), includeHidden: true };
          if (request.id != null) codexModelListRequestIds.add(String(request.id));
        }
        if (event?.type === "message") patchMcpModelResponseData(event.data);
      } catch (error) {
        recordCodexEnhancementDiagnostic("model_message_patch_failed", { errorMessage: String(error?.message || error) });
      }
      return originalDispatchEvent.call(this, event);
    };
    window.addEventListener("message", (event) => {
      try {
        patchMcpModelResponseData(event?.data);
      } catch {
      }
    }, true);
  }

  function patchMcpModelResponseData(data) {
    if (data?.type !== "mcp-response") return false;
    const message = data.message || data.response;
    const requestId = message?.id != null ? String(message.id) : "";
    if (codexModelListRequestIds.size > 0 && !codexModelListRequestIds.has(requestId)) return false;
    codexModelListRequestIds.delete(requestId);
    return patchModelContainer(data) || patchModelContainer(message) || patchModelContainer(message?.result) || patchModelContainer(message?.result?.data);
  }

  function appServerModelRequestMethod(method, params) {
    if (method === "send-cli-request-for-host" && params?.method) return String(params.method);
    if (method === "vscode://codex/list-plugins") return "list-plugins";
    if (method === "vscode://codex/plugin/install") return "install-plugin";
    if (method === "vscode://codex/plugin/uninstall") return "uninstall-plugin";
    if (method === "plugin/list") return "list-plugins";
    if (method === "plugin/install") return "install-plugin";
    if (method === "plugin/uninstall") return "uninstall-plugin";
    return String(method || "");
  }

  function patchAppServerModelResult(method, result) {
    if (method !== "list-models-for-host") return result;
    try {
      if (Array.isArray(result)) patchModelArray(result, true);
      if (Array.isArray(result?.data)) patchModelArray(result.data, true);
      if (Array.isArray(result?.models)) patchModelArray(result.models, true);
      patchModelContainer(result);
      patchObjectGraphForModels(result, new WeakSet(), 0);
    } catch (error) {
      recordCodexEnhancementDiagnostic("model_app_server_result_patch_failed", { errorMessage: String(error?.message || error) });
    }
    return result;
  }

  function patchAppServerModelRequestClient(client) {
    if (!client || typeof client.sendRequest !== "function") return false;
    if (client.__codestudioLiteModelRequestPatch === codexAppServerModelRequestPatchVersion) return true;
    const originalSendRequest = client.__codestudioLiteModelOriginalSendRequest || client.sendRequest.bind(client);
    client.__codestudioLiteModelOriginalSendRequest = originalSendRequest;
    client.sendRequest = async function codestudioLiteModelPatchedSendRequest(method, params, options) {
      const result = await originalSendRequest(method, params, options);
      if (!codexPlusModelUnlockEnabled()) return result;
      if (!codexPlusModelNames().length) await loadCodexModelCatalog();
      return patchAppServerModelResult(appServerModelRequestMethod(String(method || ""), params), result);
    };
    client.__codestudioLiteModelRequestPatch = codexAppServerModelRequestPatchVersion;
    return true;
  }

  function installAppServerModelRequestPatch() {
    if (window.__codestudioLiteAppServerModelRequestPatchInstalled === codexAppServerModelRequestPatchVersion) return;
    const patch = async () => {
      try {
        const module = await loadCodexAppModule("app-server-manager-signals-");
        const candidates = Object.values(module).filter((value) => value && typeof value === "object");
        let patchedCount = 0;
        for (const candidate of candidates) {
          if (patchAppServerModelRequestClient(candidate)) patchedCount += 1;
          if (typeof candidate.sendRequest !== "function" && typeof candidate.get === "function") {
            try {
              if (patchAppServerModelRequestClient(candidate.get())) patchedCount += 1;
            } catch {
            }
          }
        }
        if (patchedCount > 0) window.__codestudioLiteAppServerModelRequestPatchInstalled = codexAppServerModelRequestPatchVersion;
      } catch (error) {
        recordCodexEnhancementDiagnostic("model_app_server_request_patch_failed", { errorMessage: String(error?.message || error) });
      }
    };
    void patch();
  }

  function ensureCodexModelWhitelistInstalls() {
    if (!codexPlusModelUnlockEnabled()) return;
    installModelJsonResponsePatch();
    patchAppServerModelMessages();
    installAppServerModelRequestPatch();
  }

  function runCodexModelWhitelistRefreshPass() {
    if (!codexPlusModelUnlockEnabled() || !codexPlusModelNames().length) return false;
    let changed = false;
    try {
      patchStatsigModelWhitelist();
      if (patchReactModelState()) changed = true;
      installAppServerModelRequestPatch();
    } catch (error) {
      recordCodexEnhancementDiagnostic("model_whitelist_refresh_failed", { errorMessage: String(error?.message || error) });
    }
    return changed;
  }

  function scheduleCodexModelWhitelistRefresh(durationMs = 2500) {
    if (!codexPlusModelUnlockEnabled()) return;
    codexModelWhitelistRefreshUntil = Math.max(codexModelWhitelistRefreshUntil, Date.now() + durationMs);
    if (codexModelWhitelistRefreshTimer) return;
    const tick = () => {
      codexModelWhitelistRefreshTimer = 0;
      runCodexModelWhitelistRefreshPass();
      if (Date.now() < codexModelWhitelistRefreshUntil) {
        codexModelWhitelistRefreshTimer = window.setTimeout(tick, 120);
      }
    };
    tick();
  }

  function patchCodexModelWhitelist(mutations = null) {
    ensureCodexModelWhitelistInstalls();
    if (!codexPlusModelNames().length) {
      void loadCodexModelCatalog();
      return;
    }
    if (shouldScheduleReactModelStatePatch(mutations)) {
      scheduleCodexModelWhitelistRefresh();
    } else {
      runCodexModelWhitelistRefreshPass();
    }
  }

  function refreshCodexModelWhitelistFromScan(mutations) {
    patchCodexModelWhitelist(mutations);
  }

  function normalizeCodexServiceTierModelName(model) {
    return String(model || "").trim().toLowerCase();
  }

  function isFastServiceTierValue(value) {
    const normalized = String(value || "").trim().toLowerCase();
    return normalized === "fast" || normalized === "priority";
  }

  function codexFastServiceTierValue() {
    return codexServiceTierState.fastTierValue || codexServiceTierFallbackFastValue;
  }

  function codexServiceTierFastModelListLabel() {
    return Array.from(codexServiceTierSupportedFastModels).join(" / ");
  }

  function codexServiceTierModelFromValue(value, visited = new WeakSet(), depth = 0) {
    if (typeof value === "string") return value.trim();
    if (!value || typeof value !== "object" || visited.has(value) || depth > 3) return "";
    visited.add(value);
    for (const key of ["model", "modelId", "model_id", "selectedModel", "selected_model", "defaultModel", "default_model"]) {
      const model = codexServiceTierModelFromValue(value[key], visited, depth + 1);
      if (model) return model;
    }
    for (const key of ["params", "request", "payload", "body", "config", "options"]) {
      const model = codexServiceTierModelFromValue(value[key], visited, depth + 1);
      if (model) return model;
    }
    return "";
  }

  function codexServiceTierCurrentModelName() {
    return codexServiceTierModelFromValue(codexModelCatalog.model) || codexServiceTierModelFromValue(codexModelCatalog.default_model);
  }

  function codexServiceTierModelForRequest(params, modelHint = "") {
    return codexServiceTierModelFromValue(params) || codexServiceTierModelFromValue(modelHint) || codexServiceTierCurrentModelName();
  }

  function codexServiceTierFastSupportedForModel(modelName) {
    return codexServiceTierSupportedFastModels.has(normalizeCodexServiceTierModelName(modelName));
  }

  function codexServiceTierMaybeLoadModelCatalog(force = false) {
    if (codexModelCatalogPromise) return;
    if (!force && codexModelCatalog.status === "failed") return;
    if (!force && codexModelCatalogLoadedAt && Date.now() - codexModelCatalogLoadedAt < 10000) return;
    loadCodexModelCatalog(force).then(() => {
      refreshCodexServiceTierControls();
    }).catch(() => {
      refreshCodexServiceTierControls();
    });
  }

  function codexServiceTierFastAvailability(modelName = codexServiceTierCurrentModelName()) {
    const normalizedModel = normalizeCodexServiceTierModelName(modelName);
    return {
      modelName: modelName || "",
      supported: !!normalizedModel && codexServiceTierSupportedFastModels.has(normalizedModel),
    };
  }

  function codexServiceTierFastUnsupportedMessage(modelName = codexServiceTierCurrentModelName()) {
    const modelText = modelName ? `当前模型 ${modelName} 不支持` : "当前模型未读取";
    return `Fast 仅支持 ${codexServiceTierFastModelListLabel()}，${modelText}`;
  }

  function codexServiceTierValueForMode(mode) {
    if (mode === "fast") return codexFastServiceTierValue();
    if (mode === "standard") return null;
    return codexServiceTierState.serviceTier || null;
  }

  function codexServiceTierDefaultModeForControlMode(controlMode, fallback = "inherit") {
    if (controlMode === "global-fast") return "fast";
    if (controlMode === "global-standard") return "standard";
    if (controlMode === "inherit") return "inherit";
    return normalizeCodexThreadServiceTierMode(fallback);
  }

  function codexServiceTierEffectiveThreadMode(threadMode = "inherit", defaultMode = "inherit") {
    const normalizedThreadMode = normalizeCodexThreadServiceTierMode(threadMode);
    if (normalizedThreadMode !== "inherit") return normalizedThreadMode;
    return normalizeCodexThreadServiceTierMode(defaultMode);
  }

  function codexServiceTierValueForControlMode(controlMode, threadMode = "inherit", defaultMode = "inherit") {
    if (controlMode === "global-fast") return codexFastServiceTierValue();
    if (controlMode === "global-standard") return null;
    if (controlMode === "custom") return codexServiceTierValueForMode(codexServiceTierEffectiveThreadMode(threadMode, defaultMode));
    return codexServiceTierState.serviceTier || null;
  }

  function codexServiceTierEffectiveMode(value) {
    return isFastServiceTierValue(value) ? "fast" : "standard";
  }

  function normalizeCodexThreadServiceTierMode(mode) {
    const normalized = String(mode || "").trim().toLowerCase();
    return codexThreadServiceTierModes.has(normalized) ? normalized : "inherit";
  }

  function normalizeCodexServiceTierControlMode(mode) {
    const normalized = String(mode || "").trim().toLowerCase();
    return codexServiceTierControlModes.has(normalized) ? normalized : "inherit";
  }

  function serviceTierGlobalStatusMessage(serviceTier) {
    if (isFastServiceTierValue(serviceTier)) return "Fast 已开启";
    if (!serviceTier) return "默认服务模式";
    return `当前：${serviceTier}`;
  }

  function serviceTierStatusMessage(
    controlMode = codexServiceTierState.controlMode || "inherit",
    threadMode = codexServiceTierState.threadMode || "inherit",
    effectiveMode = codexServiceTierState.effectiveMode || "standard",
    defaultMode = codexServiceTierState.defaultMode || "inherit"
  ) {
    if (codexServiceTierState.status === "loading") return "正在读取…";
    if (codexServiceTierState.status === "failed") return "读取失败";
    if (controlMode === "inherit") return `继承 config.toml：${effectiveMode}`;
    if (controlMode === "global-standard") return "全局 Standard";
    if (controlMode === "global-fast") return "全局 Fast";
    if (threadMode === "inherit") return `自定义：默认 ${defaultMode}`;
    return `自定义：当前 thread ${threadMode}`;
  }

  function readThreadServiceTierState() {
    try {
      const parsed = JSON.parse(localStorage.getItem(codexThreadServiceTierKey) || "{}");
      const rawEntries = parsed?.version === codexThreadServiceTierVersion && parsed?.entries && typeof parsed.entries === "object"
        ? parsed.entries
        : {};
      const entries = Object.create(null);
      Object.entries(rawEntries).forEach(([key, value]) => {
        const safeKey = validThreadScrollSessionKey(key);
        const mode = normalizeCodexThreadServiceTierMode(value?.mode);
        if (safeKey && mode !== "inherit") entries[safeKey] = { mode, at: finiteNonNegativeNumber(value?.at) || Date.now() };
      });
      const draft = normalizeThreadServiceTierDraft(parsed?.draft);
      const hasCustomState = !!draft || Object.keys(entries).length > 0;
      const mode = parsed?.mode ? normalizeCodexServiceTierControlMode(parsed.mode) : (hasCustomState ? "custom" : "inherit");
      return {
        mode,
        defaultMode: normalizeCodexThreadServiceTierMode(parsed?.defaultMode || codexServiceTierDefaultModeForControlMode(mode)),
        entries,
        draft,
      };
    } catch (_) {
      return { mode: "inherit", defaultMode: "inherit", entries: Object.create(null), draft: null };
    }
  }

  function writeThreadServiceTierState(state) {
    const mode = normalizeCodexServiceTierControlMode(state?.mode);
    const defaultMode = normalizeCodexThreadServiceTierMode(state?.defaultMode || codexServiceTierDefaultModeForControlMode(mode));
    const rawEntries = state?.entries && typeof state.entries === "object" ? state.entries : {};
    const entries = Object.create(null);
    Object.entries(rawEntries)
      .map(([key, value]) => {
        const safeKey = validThreadScrollSessionKey(key);
        const mode = normalizeCodexThreadServiceTierMode(value?.mode);
        return safeKey && mode !== "inherit" ? [safeKey, { mode, at: finiteNonNegativeNumber(value?.at) || Date.now() }] : null;
      })
      .filter(Boolean)
      .sort((left, right) => right[1].at - left[1].at)
      .slice(0, codexThreadServiceTierMaxEntries)
      .forEach(([key, value]) => {
        entries[key] = value;
      });
    const draft = normalizeThreadServiceTierDraft(state?.draft);
    try {
      localStorage.setItem(codexThreadServiceTierKey, JSON.stringify({
        version: codexThreadServiceTierVersion,
        mode,
        defaultMode,
        entries,
        ...(draft ? { draft } : {}),
      }));
    } catch (_) {}
  }

  function normalizeThreadServiceTierDraft(value) {
    if (!value || typeof value !== "object") return null;
    const mode = normalizeCodexThreadServiceTierMode(value.mode);
    if (mode === "inherit") return null;
    const at = finiteNonNegativeNumber(value.at) || Date.now();
    return { mode, at };
  }

  function codexThreadServiceTierOverride(threadId) {
    const key = validThreadScrollSessionKey(threadId);
    if (!key) return null;
    const entry = readThreadServiceTierState().entries[key];
    const mode = normalizeCodexThreadServiceTierMode(entry?.mode);
    return mode === "inherit" ? null : { mode, at: finiteNonNegativeNumber(entry?.at) || 0 };
  }

  function codexThreadServiceTierDraft() {
    const draft = readThreadServiceTierState().draft;
    if (!draft) return null;
    if (Date.now() - draft.at > codexThreadServiceTierDraftBindWindowMs) return null;
    return draft;
  }

  function setCodexThreadServiceTierOverride(threadId, mode) {
    const normalizedMode = normalizeCodexThreadServiceTierMode(mode);
    const state = readThreadServiceTierState();
    state.mode = "custom";
    const key = validThreadScrollSessionKey(threadId);
    if (key) {
      if (normalizedMode === "inherit") {
        delete state.entries[key];
      } else {
        state.entries[key] = { mode: normalizedMode, at: Date.now() };
      }
    } else if (normalizedMode === "inherit") {
      state.draft = null;
    } else {
      state.draft = { mode: normalizedMode, at: Date.now() };
    }
    writeThreadServiceTierState(state);
  }

  function bindDraftServiceTierToThread(threadId) {
    const key = validThreadScrollSessionKey(threadId);
    const draft = codexThreadServiceTierDraft();
    if (!key || !draft) return false;
    const state = readThreadServiceTierState();
    if (normalizeCodexServiceTierControlMode(state.mode) !== "custom") {
      state.draft = null;
      writeThreadServiceTierState(state);
      return false;
    }
    if (!state.entries[key]) state.entries[key] = { mode: draft.mode, at: Date.now() };
    state.draft = null;
    writeThreadServiceTierState(state);
    return true;
  }

  function setCodexServiceTierControlMode(mode) {
    const normalizedMode = normalizeCodexServiceTierControlMode(mode);
    if (normalizedMode === "global-fast") {
      const fastAvailability = codexServiceTierFastAvailability();
      if (!fastAvailability.supported) {
        codexServiceTierMaybeLoadModelCatalog(true);
        showToast(codexServiceTierFastUnsupportedMessage(fastAvailability.modelName));
        refreshCodexServiceTierControls();
        return;
      }
    }
    const state = readThreadServiceTierState();
    state.mode = normalizedMode;
    if (normalizedMode !== "custom") {
      state.defaultMode = codexServiceTierDefaultModeForControlMode(normalizedMode);
      state.entries = Object.create(null);
      state.draft = null;
    } else {
      state.defaultMode = normalizeCodexThreadServiceTierMode(state.defaultMode);
    }
    writeThreadServiceTierState(state);
    refreshCodexServiceTierControls();
    const labels = {
      inherit: "继承 config.toml",
      "global-standard": "全局 Standard",
      "global-fast": "全局 Fast",
      custom: "自定义",
    };
    showToast(`服务模式：${labels[normalizedMode] || normalizedMode}`);
  }

  function syncCodexServiceTierEffectiveState() {
    if (!codestudioLiteSettings().serviceTierControls) {
      codexServiceTierState = {
        ...codexServiceTierState,
        activeThreadId: "",
        threadMode: "inherit",
        effectiveServiceTier: codexServiceTierState.serviceTier || null,
        effectiveMode: codexServiceTierEffectiveMode(codexServiceTierState.serviceTier),
        message: "未启用",
      };
      return;
    }
    const activeThreadId = validThreadScrollSessionKey(currentSessionRef().session_id);
    if (activeThreadId) bindDraftServiceTierToThread(activeThreadId);
    const storedState = readThreadServiceTierState();
    const controlMode = normalizeCodexServiceTierControlMode(storedState.mode);
    const defaultMode = normalizeCodexThreadServiceTierMode(storedState.defaultMode);
    const override = activeThreadId ? codexThreadServiceTierOverride(activeThreadId) : codexThreadServiceTierDraft();
    const threadMode = normalizeCodexThreadServiceTierMode(override?.mode);
    const effectiveServiceTier = codexServiceTierValueForControlMode(controlMode, threadMode, defaultMode);
    const effectiveMode = codexServiceTierEffectiveMode(effectiveServiceTier);
    const fastAvailability = codexServiceTierFastAvailability();
    const message = effectiveMode === "fast" && !fastAvailability.supported
      ? codexServiceTierFastUnsupportedMessage(fastAvailability.modelName)
      : serviceTierStatusMessage(controlMode, threadMode, effectiveMode, defaultMode);
    codexServiceTierState = {
      ...codexServiceTierState,
      controlMode,
      defaultMode,
      activeThreadId,
      threadMode,
      effectiveServiceTier,
      effectiveMode,
      fastModelName: fastAvailability.modelName,
      fastSupported: fastAvailability.supported,
      message,
    };
  }

  function codexServiceTierBadgeState() {
    if (codexServiceTierState.status === "loading") return { tier: "loading", label: "...", disabled: true, title: "服务模式：正在读取" };
    if (codexServiceTierState.status === "failed") return { tier: "failed", label: "?", title: "服务模式：读取失败" };
    const fastAvailability = codexServiceTierFastAvailability();
    const effectiveMode = codexServiceTierState.effectiveMode || "standard";
    const scope = codexServiceTierState.controlMode === "custom" && codexServiceTierState.threadMode !== "inherit"
      ? `当前 thread：${codexServiceTierState.threadMode}`
      : serviceTierStatusMessage(codexServiceTierState.controlMode, codexServiceTierState.threadMode, effectiveMode, codexServiceTierState.defaultMode);
    const title = [
      `服务模式：${scope}`,
      "Standard：使用标准处理；不在请求上设置 priority。",
      `Fast：仅支持 ${codexServiceTierFastModelListLabel()}；对支持模型使用 service_tier=\"priority\"，官方说明其延迟更低且更一致，但会按更高价格计费；rate limit 与 Standard 共享，流量快速上涨时可能回落到 Standard。`,
    ].join("\n");
    if (effectiveMode === "fast" && !fastAvailability.supported) {
      return { tier: "unsupported", label: "不支持", title: `${title}\n${codexServiceTierFastUnsupportedMessage(fastAvailability.modelName)}；当前请求会按 Standard 发送。` };
    }
    if (effectiveMode === "fast") return { tier: "fast", label: "fast", title };
    return { tier: "standard", label: "standard", title };
  }

  function refreshCodexServiceTierBadges() {
    const state = codexServiceTierBadgeState();
    document.querySelectorAll(`[data-codex-service-tier-badge="true"]`).forEach((node) => {
      setCodestudioLiteDataset(node, "tier", state.tier);
      setCodestudioLiteDataset(node, "disabled", String(!!state.disabled));
      setCodestudioLiteText(node, state.label);
      setCodestudioLiteProperty(node, "title", state.title);
      setCodestudioLiteAttribute(node, "aria-label", state.title);
    });
  }

  function refreshCodexServiceTierControls() {
    syncCodexServiceTierEffectiveState();
    if (codestudioLiteSettings().serviceTierControls) codexServiceTierMaybeLoadModelCatalog();
    const fastAvailability = codexServiceTierFastAvailability();
    const fastDisabled = !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading" || !fastAvailability.supported;
    const fastTitle = fastAvailability.supported
      ? "Fast：使用 service_tier=\"priority\""
      : codexServiceTierFastUnsupportedMessage(fastAvailability.modelName);
    const fastUnsupportedActive = codexServiceTierState.effectiveMode === "fast" && !fastAvailability.supported;
    document.querySelectorAll("[data-codex-service-tier-controls]").forEach((node) => {
      setCodestudioLiteBooleanProperty(node, "hidden", !codestudioLiteSettings().serviceTierControls);
    });
    document.querySelectorAll("[data-codex-service-tier-status]").forEach((node) => {
      setCodestudioLiteDataset(node, "status", fastUnsupportedActive ? "unsupported" : (codexServiceTierState.status || "loading"));
      setCodestudioLiteText(node, codexServiceTierState.message || "未读取");
    });
    document.querySelectorAll("[data-codex-service-tier-inherit]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "inherit"));
    });
    document.querySelectorAll("[data-codex-service-tier-standard]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "global-standard"));
    });
    document.querySelectorAll("[data-codex-service-tier-fast]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", fastDisabled);
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "global-fast"));
      setCodestudioLiteProperty(button, "title", fastTitle);
    });
    document.querySelectorAll("[data-codex-service-tier-custom]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "custom"));
    });
    document.querySelectorAll("[data-codex-service-tier-thread-inherit]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "custom" && codexServiceTierState.threadMode === "inherit"));
      setCodestudioLiteProperty(button, "title", `当前 thread 不单独覆盖，继承自定义默认 ${codexServiceTierState.defaultMode || "inherit"}`);
    });
    document.querySelectorAll("[data-codex-service-tier-thread-standard]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "custom" && codexServiceTierState.threadMode === "standard"));
    });
    document.querySelectorAll("[data-codex-service-tier-thread-fast]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", fastDisabled);
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "custom" && codexServiceTierState.threadMode === "fast"));
      setCodestudioLiteProperty(button, "title", fastTitle);
    });
    refreshCodexServiceTierBadges();
  }

  async function loadCodexServiceTierState() {
    if (!codestudioLiteSettings().serviceTierControls) {
      codexServiceTierState = { ...codexServiceTierState, status: "idle", message: "未启用" };
      refreshCodexServiceTierControls();
      return;
    }
    codexServiceTierState = { ...codexServiceTierState, status: "loading", message: "正在读取…" };
    refreshCodexServiceTierControls();
    try {
      const serviceTier = await getCodexServiceTierSetting();
      codexServiceTierState = {
        ...codexServiceTierState,
        status: "ok",
        serviceTier,
        message: serviceTierGlobalStatusMessage(serviceTier),
      };
    } catch (error) {
      codexServiceTierState = {
        ...codexServiceTierState,
        status: "failed",
        message: "读取失败",
      };
      recordCodexEnhancementDiagnostic("service_tier_read_failed", { errorMessage: String(error?.message || error) });
    } finally {
      refreshCodexServiceTierControls();
    }
  }

  function ensureCodexServiceTierStateLoaded() {
    if (!codestudioLiteSettings().serviceTierControls) {
      codexServiceTierStateLoadStarted = false;
      return;
    }
    if (codexServiceTierStateLoadStarted) return;
    codexServiceTierStateLoadStarted = true;
    void loadCodexServiceTierState();
  }

  function setCodexThreadServiceTierMode(mode) {
    const normalizedMode = normalizeCodexThreadServiceTierMode(mode);
    if (normalizedMode === "fast") {
      const fastAvailability = codexServiceTierFastAvailability();
      if (!fastAvailability.supported) {
        codexServiceTierMaybeLoadModelCatalog(true);
        showToast(codexServiceTierFastUnsupportedMessage(fastAvailability.modelName));
        refreshCodexServiceTierControls();
        return;
      }
    }
    const threadId = validThreadScrollSessionKey(currentSessionRef().session_id);
    setCodexThreadServiceTierOverride(threadId, normalizedMode);
    refreshCodexServiceTierControls();
    const target = threadId ? "当前 thread" : "新 thread 草稿";
    showToast(`${target}服务模式：${normalizedMode === "inherit" ? "继承" : normalizedMode}`);
  }

  function toggleCodexServiceTierFromBadge() {
    syncCodexServiceTierEffectiveState();
    const nextMode = codexServiceTierState.effectiveMode === "fast" ? "standard" : "fast";
    if (nextMode === "fast") {
      const fastAvailability = codexServiceTierFastAvailability();
      if (!fastAvailability.supported) {
        codexServiceTierMaybeLoadModelCatalog(true);
        showToast(codexServiceTierFastUnsupportedMessage(fastAvailability.modelName));
        refreshCodexServiceTierControls();
        return;
      }
    }
    setCodexThreadServiceTierMode(nextMode);
  }

  function codexServiceTierBadgeVisibleElement(element) {
    if (!(element instanceof HTMLElement) || !element.isConnected) return false;
    const style = getComputedStyle(element);
    if (style.display === "none" || style.visibility === "hidden") return false;
    const rect = element.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  function codexServiceTierBadgeText(element) {
    const text = String(element?.textContent || "");
    return (text.length > 4000 ? text.slice(-4000) : text).replace(/\s+/g, " ").trim();
  }

  function codexServiceTierKnownProviderNames() {
    return uniqueValues([
      codexModelCatalog.provider_name,
      codexModelCatalog.model_provider,
    ]).map((value) => value.toLowerCase());
  }

  function codexServiceTierLooksLikeProviderButton(button, providerNames) {
    const text = codexServiceTierBadgeText(button);
    if (!text || text.length > 32) return false;
    const lower = text.toLowerCase();
    if (providerNames.includes(lower)) return true;
    if (/\s/.test(text)) return false;
    if (!/[a-z]/i.test(text)) return false;
    if (!/^[a-z0-9][a-z0-9._-]{1,31}$/i.test(text)) return false;
    if (/^(local|remote|cloud|standard|default|fast|worktree|new|send|stop|codex)$/i.test(text)) return false;
    if (/^(gpt|o[1-9]|claude|gemini|deepseek|qwen|kimi|moonshot|mistral|llama|sonnet|opus|haiku)[a-z0-9._-]*$/i.test(text)) return false;
    return true;
  }

  function codexServiceTierBadgeButtonCandidates(composer) {
    const composerRect = composer.getBoundingClientRect();
    return Array.from(composer.querySelectorAll("button, [role='button']"))
      .filter((button) => !button.closest?.(`[data-codex-service-tier-badge="true"]`))
      .filter(codexServiceTierBadgeVisibleElement)
      .filter((button) => {
        const rect = button.getBoundingClientRect();
        return rect.bottom >= composerRect.top + composerRect.height * 0.35;
      })
      .sort((left, right) => {
        const leftRect = left.getBoundingClientRect();
        const rightRect = right.getBoundingClientRect();
        return (rightRect.bottom - leftRect.bottom) || (leftRect.left - rightRect.left);
      });
  }

  function codexServiceTierVisibleComposerFooters(root = document) {
    const footers = [
      ...(root?.matches?.(".composer-footer") ? [root] : []),
      ...Array.from(root?.querySelectorAll?.(".composer-footer") || []),
    ];
    return footers
      .filter(codexServiceTierBadgeVisibleElement)
      .sort((left, right) => {
        const leftRect = left.getBoundingClientRect();
        const rightRect = right.getBoundingClientRect();
        return (rightRect.bottom - leftRect.bottom) || (rightRect.width - leftRect.width);
      });
  }

  function codexServiceTierComposerScore(composer) {
    const text = codexServiceTierBadgeText(composer).toLowerCase();
    const providerNames = codexServiceTierKnownProviderNames();
    let score = 0;
    if (providerNames.some((name) => name && text.includes(name))) score += 40;
    if (/完全访问权限|full access|model|超高|high|sub2api|provider/i.test(text)) score += 20;
    if (/本地模式|local mode|worktree|branch|codex\//i.test(text)) score -= 30;
    if (composer.matches?.(".composer-footer")) score += 4;
    if (composer.querySelector?.(".composer-footer")) score += 8;
    const buttons = Array.from(composer.querySelectorAll?.("button, [role='button']") || []).filter(codexServiceTierBadgeVisibleElement);
    if (buttons.some((button) => codexServiceTierLooksLikeProviderButton(button, providerNames))) score += 30;
    score += Math.min(10, buttons.length);
    return score;
  }

  function codexServiceTierComposerCandidates() {
    const candidates = new Set();
    codexServiceTierVisibleComposerFooters().forEach((footer) => {
      candidates.add(footer);
      let node = footer.parentElement;
      for (let depth = 0; node instanceof HTMLElement && depth < 6; depth += 1, node = node.parentElement) {
        if (codexServiceTierBadgeVisibleElement(node)) candidates.add(node);
      }
    });
    Array.from(document.querySelectorAll("form, textarea, [role='textbox'], [contenteditable='true']"))
      .filter(codexServiceTierBadgeVisibleElement)
      .forEach((node) => {
        candidates.add(node);
        let parent = node.parentElement;
        for (let depth = 0; parent instanceof HTMLElement && depth < 4; depth += 1, parent = parent.parentElement) {
          if (codexServiceTierBadgeVisibleElement(parent)) candidates.add(parent);
        }
      });
    if (!candidates.size) {
      Array.from(document.querySelectorAll("main"))
        .filter(codexServiceTierBadgeVisibleElement)
        .slice(-2)
        .forEach((node) => candidates.add(node));
    }
    return Array.from(candidates);
  }

  function codexServiceTierBestComposerFooter(root = document) {
    return codexServiceTierVisibleComposerFooters(root)
      .map((footer, index) => ({ footer, index, score: codexServiceTierComposerScore(footer) }))
      .sort((left, right) => (right.score - left.score) || (left.index - right.index))[0]?.footer || null;
  }

  function codexServiceTierFindComposerEl() {
    const now = Date.now();
    if (codexServiceTierComposerCache.element?.isConnected && now < codexServiceTierComposerCache.expiresAt) {
      return codexServiceTierComposerCache.element;
    }
    const composer = codexServiceTierComposerCandidates()
      .map((composer, index) => ({ composer, index, score: codexServiceTierComposerScore(composer) }))
      .sort((left, right) => (right.score - left.score) || (left.index - right.index))[0]?.composer || null;
    codexServiceTierComposerCache = { element: composer, expiresAt: composer ? now + 1500 : 0 };
    return composer;
  }

  function codexServiceTierBadgeAnchor(composer) {
    const providerNames = codexServiceTierKnownProviderNames();
    const buttons = codexServiceTierBadgeButtonCandidates(composer);
    const exact = buttons.find((button) => providerNames.includes(codexServiceTierBadgeText(button).toLowerCase()));
    if (exact) return exact;
    const composerRect = composer.getBoundingClientRect();
    return buttons.find((button) => {
      const rect = button.getBoundingClientRect();
      return rect.left >= composerRect.left + composerRect.width * 0.42 && codexServiceTierLooksLikeProviderButton(button, providerNames);
    }) || null;
  }

  function codexServiceTierComposerFooter(composer) {
    if (composer?.matches?.(".composer-footer")) return composer;
    return codexServiceTierBestComposerFooter(composer) || codexServiceTierBestComposerFooter() || null;
  }

  function codexServiceTierBadgeFooterGroup(composer) {
    const footer = codexServiceTierComposerFooter(composer);
    if (!footer) return null;
    const children = Array.from(footer.children).filter(codexServiceTierBadgeVisibleElement);
    if (!children.length) return footer;
    const providerNames = codexServiceTierKnownProviderNames();
    const providerGroup = children.find((child) => {
      const text = codexServiceTierBadgeText(child).toLowerCase();
      return providerNames.some((name) => name && text.includes(name));
    });
    return providerGroup || children[children.length - 1] || footer;
  }

  function codexServiceTierBadgePlacement(composer) {
    const anchor = composer ? codexServiceTierBadgeAnchor(composer) : null;
    if (anchor?.parentElement) return { parent: anchor.parentElement, before: anchor };
    const group = composer ? codexServiceTierBadgeFooterGroup(composer) : null;
    if (group) return { parent: group, before: group.firstChild };
    return null;
  }

  function wireCodexServiceTierBadge(badge) {
    if (!badge || badge.dataset.codexServiceTierBadgeWired === codexServiceTierBadgeVersion) return;
    badge.dataset.codexServiceTierBadgeWired = codexServiceTierBadgeVersion;
    badge.setAttribute("role", "button");
    badge.setAttribute("tabindex", "0");
    badge.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      if (codexServiceTierState.status === "loading") return;
      toggleCodexServiceTierFromBadge();
    });
    badge.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      event.stopPropagation();
      if (codexServiceTierState.status === "loading") return;
      toggleCodexServiceTierFromBadge();
    });
  }

  function installCodexServiceTierBadge() {
    if (!codestudioLiteSettings().serviceTierControls) {
      removeCodexServiceTierBadges();
      return;
    }
    const composer = codexServiceTierFindComposerEl();
    const placement = composer ? codexServiceTierBadgePlacement(composer) : null;
    const existingBadges = Array.from(document.querySelectorAll(`[data-codex-service-tier-badge="true"]`));
    if (!composer || !placement?.parent) {
      existingBadges.forEach((badge) => badge.remove());
      return;
    }
    let badge = existingBadges.find((node) => node.closest?.(".composer-footer") || node.closest?.("button") == null) || existingBadges[0];
    existingBadges.forEach((node) => {
      if (node !== badge) node.remove();
    });
    if (!badge || badge.dataset.codexServiceTierBadgeVersion !== codexServiceTierBadgeVersion) {
      badge?.remove();
      badge = document.createElement("span");
      badge.className = codexServiceTierBadgeClass;
      badge.dataset.codexServiceTierBadge = "true";
      badge.dataset.codexServiceTierBadgeVersion = codexServiceTierBadgeVersion;
    }
    wireCodexServiceTierBadge(badge);
    const before = placement.before?.parentElement === placement.parent ? placement.before : null;
    if (badge.parentElement !== placement.parent || badge.nextSibling !== before) {
      placement.parent.insertBefore(badge, before);
    }
    refreshCodexServiceTierBadges();
  }

  function removeCodexServiceTierBadges() {
    document.querySelectorAll(`[data-codex-service-tier-badge="true"]`).forEach((badge) => badge.remove());
  }

  function codexServiceTierRequestMethods() {
    return new Set(["thread/start", "thread/resume", "turn/start"]);
  }

  function codexServiceTierThreadIdForRequest(method, params, threadIdHint = "") {
    if (method === "thread/start") return validThreadScrollSessionKey(params?.threadId || threadIdHint);
    return validThreadScrollSessionKey(params?.threadId || params?.conversationId || threadIdHint || currentSessionRef().session_id);
  }

  function codexServiceTierOverrideResult(method, params, threadIdHint, mode, requestedServiceTier, modelHint = "") {
    const threadId = codexServiceTierThreadIdForRequest(method, params, threadIdHint);
    const requestedFast = isFastServiceTierValue(requestedServiceTier);
    const modelName = codexServiceTierModelForRequest(params, modelHint);
    const fastSupported = !requestedFast || codexServiceTierFastSupportedForModel(modelName);
    return {
      threadId,
      mode,
      serviceTier: requestedFast && fastSupported ? codexFastServiceTierValue() : null,
      requestedServiceTier: requestedServiceTier || null,
      modelName,
      fastSupported,
      fastBlocked: requestedFast && !fastSupported,
    };
  }

  function codexServiceTierOverrideForRequest(method, params, threadIdHint = "") {
    if (!codestudioLiteSettings().serviceTierControls) return null;
    if (!codexServiceTierRequestMethods().has(method) || !params || typeof params !== "object") return null;
    const state = readThreadServiceTierState();
    const controlMode = normalizeCodexServiceTierControlMode(state.mode);
    const defaultMode = normalizeCodexThreadServiceTierMode(state.defaultMode);
    if (controlMode === "inherit") {
      const inheritedServiceTier = params.serviceTier ?? params.service_tier ?? codexServiceTierState.serviceTier;
      const override = codexServiceTierOverrideResult(method, params, threadIdHint, "inherit", inheritedServiceTier);
      return override.fastBlocked ? override : null;
    }
    if (controlMode === "global-standard" || controlMode === "global-fast") {
      return codexServiceTierOverrideResult(
        method,
        params,
        threadIdHint,
        controlMode,
        controlMode === "global-fast" ? codexFastServiceTierValue() : null
      );
    }
    const threadId = codexServiceTierThreadIdForRequest(method, params, threadIdHint);
    const override = threadId ? codexThreadServiceTierOverride(threadId) : codexThreadServiceTierDraft();
    const mode = codexServiceTierEffectiveThreadMode(override?.mode, defaultMode);
    if (mode === "inherit") {
      const inheritedServiceTier = params.serviceTier ?? params.service_tier ?? codexServiceTierState.serviceTier;
      const inheritedOverride = codexServiceTierOverrideResult(method, params, threadIdHint, "inherit", inheritedServiceTier);
      return inheritedOverride.fastBlocked ? { ...inheritedOverride, threadId, mode } : null;
    }
    return {
      ...codexServiceTierOverrideResult(method, params, threadIdHint, mode, mode === "fast" ? codexFastServiceTierValue() : null),
      threadId,
      mode,
    };
  }

  function applyCodexServiceTierRequestOverride(method, params, threadIdHint = "") {
    const override = codexServiceTierOverrideForRequest(method, params, threadIdHint);
    if (!override) return params;
    const nextParams = { ...(params || {}), serviceTier: override.serviceTier };
    if (Object.prototype.hasOwnProperty.call(nextParams, "service_tier") || override.fastBlocked) {
      nextParams.service_tier = override.serviceTier;
    }
    recordCodexEnhancementDiagnostic("service_tier_request_override_applied", {
      method,
      threadId: override.threadId || "",
      mode: override.mode,
      serviceTier: override.serviceTier || "standard",
      model: override.modelName || "",
      fastSupported: override.fastSupported !== false,
      fastBlocked: !!override.fastBlocked,
    });
    return nextParams;
  }

  function codexServiceTierRequestOverride(message) {
    if (!codestudioLiteSettings().serviceTierControls || !message || typeof message !== "object") return message;
    if (message.type === "send-cli-request-for-host") {
      const method = String(message.method || "");
      const params = applyCodexServiceTierRequestOverride(method, message.params);
      return params === message.params ? message : { ...message, params };
    }
    if (message.type === "mcp-request" && message.request && typeof message.request === "object") {
      const method = String(message.request.method || "");
      const params = applyCodexServiceTierRequestOverride(method, message.request.params);
      if (params === message.request.params) return message;
      return { ...message, request: { ...message.request, params } };
    }
    if (message.type === "worker-request" && message.request && typeof message.request === "object") {
      const method = String(message.request.method || "");
      const params = applyCodexServiceTierRequestOverride(method, message.request.params);
      if (params === message.request.params) return message;
      return { ...message, request: { ...message.request, params } };
    }
    if (message.type === "thread-prewarm-start" && message.request && typeof message.request === "object") {
      const params = applyCodexServiceTierRequestOverride("thread/start", message.request.params);
      if (params === message.request.params) return message;
      return { ...message, request: { ...message.request, params } };
    }
    if (message.type === "start-conversation") {
      const nextMessage = applyCodexServiceTierRequestOverride("thread/start", message);
      return nextMessage === message ? message : nextMessage;
    }
    if (message.type === "prewarm-thread-start-for-host" && message.params && typeof message.params === "object") {
      const params = applyCodexServiceTierRequestOverride("thread/start", message.params);
      return params === message.params ? message : { ...message, params };
    }
    if (message.type === "start-thread-for-host") {
      const params = applyCodexServiceTierRequestOverride("thread/start", message);
      return params === message ? message : params;
    }
    if (message.type === "start-turn-for-host" && message.params && typeof message.params === "object") {
      const params = applyCodexServiceTierRequestOverride("turn/start", message.params, message.conversationId);
      return params === message.params ? message : { ...message, params };
    }
    return message;
  }

  function installCodexServiceTierDispatcherPatch() {
    if (window.__codestudioLiteServiceTierRequestOverrideInstalled === codexServiceTierRequestOverrideVersion) return;
    const patch = async () => {
      try {
        const module = await loadCodexAppModule("setting-storage-");
        const dispatcherClass = typeof module.v === "function" && String(module.v).includes("dispatchMessage") ? module.v : null;
        const dispatcher = dispatcherClass?.getInstance?.();
        if (!dispatcher || typeof dispatcher.dispatchMessage !== "function") throw new Error("Codex dispatcher unavailable");
        if (dispatcher.__codestudioLiteServiceTierOriginalDispatchMessage) {
          window.__codestudioLiteServiceTierRequestOverrideInstalled = codexServiceTierRequestOverrideVersion;
          return;
        }
        dispatcher.__codestudioLiteServiceTierOriginalDispatchMessage = dispatcher.dispatchMessage.bind(dispatcher);
        dispatcher.dispatchMessage = (type, payload) => {
          const message = codexServiceTierRequestOverride({ ...(payload || {}), type });
          const nextType = message?.type || type;
          const { type: _type, ...nextPayload } = message || {};
          return dispatcher.__codestudioLiteServiceTierOriginalDispatchMessage(nextType, nextPayload);
        };
        window.__codestudioLiteServiceTierRequestOverrideInstalled = codexServiceTierRequestOverrideVersion;
      } catch (error) {
        recordCodexEnhancementDiagnostic("service_tier_dispatcher_patch_failed", { errorMessage: String(error?.message || error) });
      }
    };
    void patch();
  }

  function codestudioLiteOwnedMutationNode(node) {
    if (!node) return false;
    const element = node.nodeType === Node.ELEMENT_NODE ? node : node.parentElement;
    return !!element?.closest?.(`[data-codex-service-tier-badge="true"], .codestudio-lite-codex-toast, #${styleId}`);
  }

  function codestudioLiteMutationTouchesOnlyOwnNodes(mutation) {
    const changedNodes = [...(mutation.addedNodes || []), ...(mutation.removedNodes || [])];
    if (changedNodes.length > 0) {
      return codestudioLiteOwnedMutationNode(mutation.target) || changedNodes.every(codestudioLiteOwnedMutationNode);
    }
    return codestudioLiteOwnedMutationNode(mutation.target);
  }

  function shouldIgnoreCodestudioLiteMutations(mutations) {
    return Array.isArray(mutations) && mutations.length > 0 && mutations.every(codestudioLiteMutationTouchesOnlyOwnNodes);
  }

  function refresh(mutations = null) {
    ensureStyle();
    const settings = codestudioLiteSettings();
    if (settings.pluginMarketplaceUnlock) {
      installPluginBuildFlavorFilterPatch();
      installPluginMarketplaceRequestPatch();
    }
    if (settings.pluginAutoExpand) {
      schedulePluginAutoExpand();
    } else {
      clearTimeout(window.__codexPluginAutoExpandTimer);
      window.__codexPluginAutoExpandTimer = null;
      window.__codexPluginAutoExpandRunning = false;
    }
    if (settings.modelWhitelistUnlock) {
      patchCodexModelWhitelist(mutations);
    }
    if (settings.serviceTierControls) {
      ensureCodexServiceTierStateLoaded();
      installCodexServiceTierDispatcherPatch();
      installCodexServiceTierBadge();
      refreshCodexServiceTierControls();
    } else {
      codexServiceTierStateLoadStarted = false;
      removeCodexServiceTierBadges();
    }
  }

  function runCodestudioLiteRefresh(mutations = null) {
    const now = Date.now();
    if (now < codestudioLiteRefreshDisabledUntil) return;
    const started = typeof performance !== "undefined" && performance.now ? performance.now() : now;
    try {
      refresh(mutations);
    } finally {
      const ended = typeof performance !== "undefined" && performance.now ? performance.now() : Date.now();
      const elapsed = ended - started;
      if (elapsed > 50) {
        codestudioLiteSlowRefreshCount += 1;
        if (codestudioLiteSlowRefreshCount === 1 || codestudioLiteSlowRefreshCount === 3) {
          recordCodexEnhancementDiagnostic("enhancement_refresh_slow", { elapsedMs: Math.round(elapsed), count: codestudioLiteSlowRefreshCount });
        }
        if (codestudioLiteSlowRefreshCount >= 5) {
          codestudioLiteRefreshDisabledUntil = Date.now() + 5000;
          codestudioLiteSlowRefreshCount = 0;
          recordCodexEnhancementDiagnostic("enhancement_refresh_temporarily_throttled", { disabledMs: 5000 });
        }
      } else {
        codestudioLiteSlowRefreshCount = 0;
      }
    }
  }

  function scheduleCodestudioLiteRefresh(mutations = null) {
    if (shouldIgnoreCodestudioLiteMutations(mutations)) return;
    if (Array.isArray(mutations) && mutations.length > 0) {
      codestudioLitePendingMutations = [...(codestudioLitePendingMutations || []), ...mutations].slice(-80);
    }
    if (codestudioLiteRefreshScheduled) return;
    codestudioLiteRefreshScheduled = true;
    const scheduleFrame = window.requestAnimationFrame || ((callback) => window.setTimeout(callback, 16));
    scheduleFrame(() => {
      codestudioLiteRefreshScheduled = false;
      const pending = codestudioLitePendingMutations;
      codestudioLitePendingMutations = null;
      runCodestudioLiteRefresh(pending);
    });
  }

  window.__codestudioLiteCodexEnhancementsRefresh = () => scheduleCodestudioLiteRefresh();
  runCodestudioLiteRefresh();
  if (!window.__codestudioLiteCodexEnhancementsTimer) {
    window.__codestudioLiteCodexEnhancementsTimer = setInterval(() => scheduleCodestudioLiteRefresh(), 1000);
  }
  if (!window.__codestudioLiteCodexEnhancementsObserver) {
    const observer = new MutationObserver((mutations) => scheduleCodestudioLiteRefresh(mutations));
    observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled", "aria-disabled", "class", "style"] });
    window.__codestudioLiteCodexEnhancementsObserver = observer;
  }
  return true;
})()
"#;
    Ok(script
        .replace("__CODESTUDIO_LITE_SETTINGS__", &settings_json)
        .replace(
            "__CODESTUDIO_LITE_PLUGIN_MARKETPLACES__",
            &plugin_marketplaces_json,
        ))
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

fn portable_registration<'a>(
    install_root: &'a Path,
    version: &'a str,
) -> package::PortableAppRegistration<'a> {
    package::PortableAppRegistration {
        display_name: CODEX_DISPLAY_NAME,
        publisher: CODEX_PUBLISHER,
        install_root,
        executable_name: CODEX_EXE_NAME,
        shortcut_name: CODEX_SHORTCUT_NAME,
        version,
        uninstall_key: CODEX_UNINSTALL_KEY,
    }
}

fn purge_user_data() -> Result<bool, String> {
    let home =
        dirs::home_dir().ok_or_else(|| "Could not locate the user home directory.".to_string())?;
    let path = home.join(".codex");
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(path).map_err(|err| format!("Failed to delete ~/.codex: {err}"))?;
    Ok(true)
}

fn open_folder(path: &Path) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        hidden_command("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to open path: {err}"))
    } else if cfg!(target_os = "macos") {
        hidden_command("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to open path: {err}"))
    } else {
        hidden_command("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to open path: {err}"))
    }
}

#[cfg(test)]
#[cfg(target_os = "windows")]
#[path = "chatgpt_desktop_tests.rs"]
mod chatgpt_desktop_tests;
