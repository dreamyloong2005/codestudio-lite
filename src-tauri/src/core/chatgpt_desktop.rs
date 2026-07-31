mod enhancement;

use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::codex_plugin_marketplace;
use crate::core::codex_provider_sync;
use crate::core::computer_use_guard;
use crate::core::download_http;
use crate::core::platform::{
    hidden_command, macos_arm64_hardware_available, native_macos_arch_for_runtime, package,
    windows_native_architecture,
};
use crate::core::process_control;
use crate::core::storage;
use crate::core::types::{
    ChatGptDesktopInstallKinds, ChatGptDesktopProductGeneration, ConfigState,
    DesktopInstallKindInfo, InstallState, Severity, ToolCategory, ToolStatus,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
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
    #[serde(default)]
    architectures: BTreeMap<String, WindowsArchitectureSource>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsArchitectureSource {
    version: String,
    package_moniker: String,
    architecture: Option<String>,
    content_length: Option<u64>,
    etag: Option<String>,
}

#[derive(Debug)]
struct SelectedWindowsSource {
    version: String,
    package_moniker: String,
    architecture: String,
    content_length: Option<u64>,
    etag: Option<String>,
    product_id: Option<String>,
    package_identity: Option<String>,
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
    let settings = load_settings()?;
    let installed = detect_installed(&settings)
        .ok_or_else(|| "ChatGPT Desktop was not detected.".to_string())?;
    let running = is_chatgpt_desktop_running(Some(&installed));
    if settings.sync_history_on_launch && running {
        let mut notes = Vec::new();
        close_chatgpt_desktop_processes(&installed, &mut notes)?;
        sync_history_if_enabled(&settings)?;
    } else if !running {
        sync_history_if_enabled(&settings)?;
    }
    launch_detected_chatgpt_desktop(&settings, &installed)
}

fn launch_with_restart_notes(notes: &mut Vec<String>) -> Result<(), String> {
    let settings = load_settings()?;
    let installed = detect_installed(&settings)
        .ok_or_else(|| "ChatGPT Desktop was not detected.".to_string())?;
    close_chatgpt_desktop_processes(&installed, notes)?;
    sync_history_if_enabled(&settings)?;
    launch_detected_chatgpt_desktop(&settings, &installed)
}

fn launch_detected_chatgpt_desktop(
    settings: &ChatGptDesktopSettings,
    installed: &InstalledChatGptDesktop,
) -> Result<(), String> {
    ensure_official_remote_plugin_cache_if_enabled(&settings);
    ensure_computer_use_guard_if_enabled(&settings)?;
    enhancement::launch(settings, |args| launch_installed_codex(installed, args))?;
    start_computer_use_guard_watchdog_if_enabled(&settings);
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
    tool_status_with_generation().0
}

pub fn tool_status_with_generation() -> (ToolStatus, ChatGptDesktopProductGeneration) {
    let settings = load_settings().unwrap_or_default();
    let installed = detect_installed(&settings);
    let generation = installed
        .as_ref()
        .map(|item| item.generation)
        .unwrap_or_default();
    let product_name = chatgpt_desktop_product_name(generation);
    let config_path = app_paths().ok().map(|paths| paths.home_dir.join(".codex"));
    let status = ToolStatus {
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
    };
    (status, generation)
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
    let arch = windows_native_architecture()?;
    let windows = windows_source_for_arch(&windows, arch)?;
    let package_url = windows_package_url(&base, arch);
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
        architecture: Some(windows.architecture),
        package_kind: "msix".to_string(),
        package_source: "mirror".to_string(),
        content_length: windows.content_length,
        etag: windows.etag,
        package_identity: windows
            .package_identity
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
    let arch = macos_arch_for_runtime(std::env::consts::ARCH, macos_arm64_hardware_available())?;
    macos_source_for_arch(macos, arch)
}

fn macos_arch_for_runtime(
    process_arch: &str,
    arm64_hardware_available: bool,
) -> Result<&'static str, String> {
    native_macos_arch_for_runtime(process_arch, arm64_hardware_available)
        .map_err(|_| format!("Unsupported macOS architecture for ChatGPT Desktop: {process_arch}."))
}

fn macos_source_for_arch<'a>(
    macos: &'a MacosSources,
    arch: &str,
) -> Result<(&'a MacosSource, &'static str), String> {
    match arch {
        "arm64" | "aarch64" => macos
            .arm64
            .as_ref()
            .map(|source| (source, "arm64"))
            .ok_or_else(|| {
                "ChatGPT Desktop mirror manifest has no macOS arm64 installer information."
                    .to_string()
            }),
        "x64" | "x86_64" => macos
            .x64
            .as_ref()
            .map(|source| (source, "x64"))
            .ok_or_else(|| {
                "ChatGPT Desktop mirror manifest has no macOS x64 installer information."
                    .to_string()
            }),
        arch => Err(format!(
            "Unsupported macOS architecture for ChatGPT Desktop: {arch}."
        )),
    }
}

fn windows_source_for_arch(
    windows: &WindowsSource,
    arch: &str,
) -> Result<SelectedWindowsSource, String> {
    if let Some(source) = windows.architectures.get(arch) {
        return Ok(SelectedWindowsSource {
            version: source.version.clone(),
            package_moniker: source.package_moniker.clone(),
            architecture: source
                .architecture
                .clone()
                .unwrap_or_else(|| arch.to_string()),
            content_length: source.content_length,
            etag: source.etag.clone(),
            product_id: windows.product_id.clone(),
            package_identity: windows
                .update_manifest
                .as_ref()
                .and_then(|item| item.package_identity.clone()),
        });
    }

    let top_level_arch = windows.architecture.as_deref().unwrap_or("x64");
    if top_level_arch == arch {
        return Ok(SelectedWindowsSource {
            version: windows.version.clone(),
            package_moniker: windows.package_moniker.clone(),
            architecture: top_level_arch.to_string(),
            content_length: windows.content_length,
            etag: windows.etag.clone(),
            product_id: windows.product_id.clone(),
            package_identity: windows
                .update_manifest
                .as_ref()
                .and_then(|item| item.package_identity.clone()),
        });
    }

    Err(format!(
        "ChatGPT Desktop mirror manifest has no downloadable Windows {arch} installer."
    ))
}

fn windows_package_url(base: &str, arch: &str) -> String {
    if arch == "arm64" {
        format!("{base}/latest/win-arm64")
    } else {
        format!("{base}/latest/win")
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
    let report = codex_provider_sync::run_default_provider_sync();
    if report.status == codex_provider_sync::ProviderSyncStatus::Skipped {
        let _ = activity_log::append(
            Severity::Warning,
            format!(
                "ChatGPT Desktop history sync was skipped: {}",
                report.message
            ),
        );
        return Ok(());
    }
    let _ = activity_log::append(
        Severity::Info,
        format!(
            "Synchronized ChatGPT Desktop history provider to {} ({} session files, {} sqlite rows, {} workspace fields).",
            report.target_provider,
            report.changed_session_files,
            report.sqlite_rows_updated,
            report.updated_workspace_roots
        ),
    );
    if let Some(warning) = report.encrypted_content_warning {
        let _ = activity_log::append(Severity::Warning, warning);
    }
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
