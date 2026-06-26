use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::codex_provider_sync;
use crate::core::platform::{hidden_command, package, run_powershell};
use crate::core::process_control;
use crate::core::storage;
use crate::core::types::{
    CodexClientInstallKinds, ConfigState, DesktopInstallKindInfo, InstallState, Severity,
    ToolCategory, ToolStatus,
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
const CODEX_MACOS_APP_NAME: &str = "Codex.app";
const CODEX_SHORTCUT_NAME: &str = "Codex.lnk";
const CODEX_UNINSTALL_KEY: &str =
    r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Codex";
const CODEX_MACOS_BUNDLE_ID: &str = "com.openai.codex";
const CODEX_CLIENT_SETTINGS_STATE_KEY: &str = "codex_client.settings";
const CODEX_CLIENT_MARKER_STATE_KEY: &str = "codex_client.managed_marker";
const CODEX_PATCH_INJECTION_RETRY_COUNT: usize = 30;
const CODEX_PATCH_INJECTION_RETRY_MS: u64 = 500;
pub const CODEX_CLIENT_PROGRESS_EVENT: &str = "codex-client://progress";
static DOWNLOAD_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientSettings {
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
    #[serde(default)]
    pub patch_force_plugin_unlock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCodexClientSettingsRequest {
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
    pub patch_force_plugin_unlock: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledCodexClient {
    pub path: String,
    pub version: String,
    pub arch: Option<String>,
    pub source: String,
    pub package_family_name: Option<String>,
    pub installed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientRelease {
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
pub struct CodexClientCapability {
    pub id: String,
    pub label: String,
    pub status: Severity,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientPlan {
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
    pub capabilities: Vec<CodexClientCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientState {
    pub install_kind: String,
    pub generated_at: String,
    pub platform: String,
    pub settings: CodexClientSettings,
    pub installed: Option<InstalledCodexClient>,
    pub install_class: String,
    pub release: Option<CodexClientRelease>,
    pub plan: Option<CodexClientPlan>,
    pub staging_dir: String,
    pub notes: Vec<String>,
    #[serde(default)]
    pub running: bool,
}

pub type CodexClientStateCache = BTreeMap<String, CodexClientState>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientStageReport {
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
pub struct CodexClientProgress {
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
pub struct CodexClientOperationResult {
    pub install_kind: String,
    pub success: bool,
    pub action: String,
    pub message: String,
    pub installed: Option<InstalledCodexClient>,
    pub stage: Option<CodexClientStageReport>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientUninstallRequest {
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
pub struct CodexClientInstallRequest {
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
pub struct PlanCodexClientUpdateRequest {
    #[serde(default)]
    pub install_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StageCodexClientUpdateRequest {
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

impl Default for CodexClientSettings {
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
            patch_force_plugin_unlock: false,
        }
    }
}

const CODEX_CLIENT_LATEST_CACHE_TTL: Duration = Duration::from_secs(600);
const CODEX_CLIENT_LATEST_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Default)]
struct CodexClientLatestCache {
    version: Option<String>,
    checked_at: Option<Instant>,
    in_progress: bool,
}

static CODEX_CLIENT_LATEST_CACHE: OnceLock<Mutex<CodexClientLatestCache>> = OnceLock::new();

fn codex_client_latest_cache() -> &'static Mutex<CodexClientLatestCache> {
    CODEX_CLIENT_LATEST_CACHE.get_or_init(|| Mutex::new(CodexClientLatestCache::default()))
}

/// Fetch the latest Codex version from the mirror manifest in a
/// background thread and cache the result in-process. Returns the cached
/// version if fresh, waits up to wait_budget for an in-flight fetch, and
/// otherwise returns whatever is cached so the caller is never blocked for
/// long. Mirrors the Claude Desktop latest-version cache in detector.rs.
pub fn latest_version_cached(wait_budget: Duration) -> Option<String> {
    let should_start = {
        let mut cache = codex_client_latest_cache().lock().unwrap();
        if cache
            .checked_at
            .map(|checked_at| checked_at.elapsed() < CODEX_CLIENT_LATEST_CACHE_TTL)
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
            let mut cache = codex_client_latest_cache().lock().unwrap();
            cache.version = version;
            cache.checked_at = Some(Instant::now());
            cache.in_progress = false;
        });
    }

    let started_at = Instant::now();
    loop {
        {
            let cache = codex_client_latest_cache().lock().unwrap();
            if !cache.in_progress
                || cache
                    .checked_at
                    .map(|checked_at| checked_at.elapsed() < CODEX_CLIENT_LATEST_CACHE_TTL)
                    .unwrap_or(false)
            {
                return cache.version.clone();
            }
            if started_at.elapsed() >= wait_budget {
                return cache.version.clone();
            }
        }
        thread::sleep(CODEX_CLIENT_LATEST_POLL_INTERVAL);
    }
}

/// Load the most recent Codex state cached to disk by inspect_state(true).
/// Used by the page to hydrate instantly on startup before an async re-fetch.
pub fn load_cached_state() -> Option<CodexClientState> {
    storage::load_codex_client_state().ok().flatten()
}

/// Load all cached Codex states keyed by install kind. Windows has independent
/// MSIX and portable plans, so one global row loses whichever tab was scanned
/// first.
pub fn load_cached_states() -> CodexClientStateCache {
    storage::load_codex_client_states().unwrap_or_default()
}

pub fn inspect_state(include_network: bool) -> Result<CodexClientState, String> {
    inspect_state_for_install_kind(include_network, None)
}

fn inspect_state_for_install_kind(
    include_network: bool,
    install_kind: Option<&str>,
) -> Result<CodexClientState, String> {
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
        "Codex management covers install, update, uninstall, launch, and mirror-source flows.".to_string(),
        "The Codex installer content is not modified; downloads are SHA-256 verified before installation.".to_string(),
    ];
    if cfg!(target_os = "macos") {
        notes.push(
            "macOS uses a DMG installer and copies Codex.app to the target Applications directory."
                .to_string(),
        );
    } else if !cfg!(target_os = "windows") {
        notes.push("The current platform does not provide an executable Codex desktop client install path yet.".to_string());
    }

    let state = CodexClientState {
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
        running: process_control::is_process_running("Codex"),
    };
    if include_network {
        let _ = storage::store_codex_client_state(&state);
    }
    Ok(state)
}

pub fn plan_update(request: PlanCodexClientUpdateRequest) -> Result<CodexClientState, String> {
    inspect_state_for_install_kind(true, request.install_kind.as_deref())
}

pub fn stage_update() -> Result<CodexClientStageReport, String> {
    stage_update_with_progress(StageCodexClientUpdateRequest::default(), |_| {})
}

pub fn stage_update_with_progress<F>(
    request: StageCodexClientUpdateRequest,
    on_progress: F,
) -> Result<CodexClientStageReport, String>
where
    F: Fn(CodexClientProgress),
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
    request: CodexClientInstallRequest,
) -> Result<CodexClientOperationResult, String> {
    install_or_update_with_progress(request, |_| {})
}

pub fn install_or_update_with_progress<F>(
    request: CodexClientInstallRequest,
    on_progress: F,
) -> Result<CodexClientOperationResult, String>
where
    F: Fn(CodexClientProgress),
{
    if !request.confirm {
        return Err(
            "Refused: installing or updating Codex requires explicit confirmation.".to_string(),
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
                "Codex state changed: expected version {expected}, current version is {}. Refresh and try again.",
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
            "codexClient.progressAlreadyUpToDate",
            Some(1),
            Some(1),
            Some(7),
            Some(7),
        );
        return Ok(CodexClientOperationResult {
            install_kind,
            success: true,
            action: "none".to_string(),
            message: "Codex is already up to date.".to_string(),
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
        return Err("The current platform does not provide an executable Codex desktop client install path yet.".to_string());
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
            if let Err(err) = package::quit_macos_app(CODEX_DISPLAY_NAME) {
                notes.push(format!("Failed to close Codex: {err}"));
            }
        }
    }
    let installed = if action == "portable-fallback" {
        emit_step_progress(
            &on_progress,
            &install_kind,
            "installing",
            "codexClient.progressInstallingPortable",
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
            "codexClient.progressInstallingMacos",
            None,
            None,
            Some(4),
            Some(7),
        );
        let report = package::install_macos_dmg(
            &staged_path,
            CODEX_MACOS_APP_NAME,
            &expand_env_path(&settings.install_root)?,
            Some(CODEX_MACOS_BUNDLE_ID),
        )?;
        notes.extend(report.notes);
        report.installed.map(installed_from_macos_app)
    } else {
        emit_step_progress(
            &on_progress,
            &install_kind,
            "msix-installing",
            "codexClient.progressInstallingMsix",
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
            "Installed or updated Codex to {} via {}.",
            release.version, action
        ),
    );

    emit_step_progress(
        &on_progress,
        &install_kind,
        "done",
        "codexClient.progressInstallDone",
        Some(1),
        Some(1),
        Some(7),
        Some(7),
    );

    Ok(CodexClientOperationResult {
        install_kind,
        success: installed.is_some(),
        action,
        message: installed
            .as_ref()
            .map(|item| format!("Codex is ready: {} ({})", item.version, item.source))
            .unwrap_or_else(|| {
                "Installation flow finished, but Codex was not detected again.".to_string()
            }),
        installed,
        stage: Some(stage),
        notes,
    })
}

pub fn uninstall(
    request: CodexClientUninstallRequest,
) -> Result<CodexClientOperationResult, String> {
    if !request.confirm {
        return Err("Refused: uninstalling Codex requires explicit confirmation.".to_string());
    }
    if !cfg!(target_os = "windows") && !cfg!(target_os = "macos") {
        return Err("The current platform does not provide an executable Codex desktop client uninstall path yet.".to_string());
    }

    let mut settings = load_settings()?;
    let install_kind = normalize_install_kind(request.install_kind.as_deref(), &settings);
    settings = settings_for_install_kind(settings, &install_kind);
    // When the caller specifies an install kind (from the page tab), detect
    // only that kind so uninstalling targets the version the user is viewing.
    let installed = detect_installed_for_kind(&settings, &install_kind);
    let Some(installed_before) = installed else {
        return Ok(CodexClientOperationResult {
            install_kind,
            success: true,
            action: "none".to_string(),
            message: "No uninstallable Codex was detected.".to_string(),
            installed: None,
            stage: None,
            notes: Vec::new(),
        });
    };

    let mut notes = Vec::new();
    if cfg!(target_os = "windows") {
        terminate_codex_process_for_uninstall(Some(Path::new(&installed_before.path)), &mut notes)?;
    } else if cfg!(target_os = "macos") {
        if let Err(err) = package::quit_macos_app(CODEX_DISPLAY_NAME) {
            notes.push(format!("Failed to close Codex: {err}"));
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
            "Unsupported Codex install type for uninstall: {}.",
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

    let _ = storage::delete_state_json(CODEX_CLIENT_MARKER_STATE_KEY);
    let _ = activity_log::append(Severity::Ok, "Uninstalled Codex.");

    Ok(CodexClientOperationResult {
        install_kind,
        success: true,
        action: action.to_string(),
        message: "Codex uninstalled.".to_string(),
        installed: None,
        stage: None,
        notes,
    })
}

pub fn launch() -> Result<(), String> {
    let mut notes = Vec::new();
    terminate_codex_process_for_restart(None, &mut notes)?;
    let settings = load_settings()?;
    sync_history_if_enabled(&settings)?;
    let installed =
        detect_installed(&settings).ok_or_else(|| "Codex was not detected.".to_string())?;
    let debug_port = select_debug_port()?;
    let args = codex_patch_launch_args(debug_port);
    launch_installed_codex(&installed, &args)?;
    if settings.patch_force_plugin_unlock {
        spawn_plugin_unlock_injection(debug_port);
    }
    let _ = activity_log::append(Severity::Info, "Launched Codex.");
    Ok(())
}

pub fn restart() -> Result<String, String> {
    let settings = load_settings()?;
    let _installed =
        detect_installed(&settings).ok_or_else(|| "Codex was not detected.".to_string())?;
    let mut notes = Vec::new();
    terminate_codex_process_for_restart(None, &mut notes)?;
    launch()?;
    let message = if notes.is_empty() {
        "Launched Codex.".to_string()
    } else {
        format!("{} Restarted Codex.", notes.join(" "))
    };
    let _ = activity_log::append(Severity::Info, "Restarted Codex after profile apply.");
    Ok(message)
}

pub fn update_settings(
    request: UpdateCodexClientSettingsRequest,
) -> Result<CodexClientSettings, String> {
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
    if let Some(unlock) = request.patch_force_plugin_unlock {
        settings.patch_force_plugin_unlock = unlock;
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
    let config_path = app_paths().ok().map(|paths| paths.home_dir.join(".codex"));
    ToolStatus {
        id: "codex-app".to_string(),
        name: "Codex".to_string(),
        category: ToolCategory::AiTool,
        command: if cfg!(target_os = "windows") {
            "Codex.exe".to_string()
        } else {
            "Codex.app".to_string()
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
        install_command: Some("Install or update from the Codex page".to_string()),
        details: installed
            .as_ref()
            .map(|item| format!("{} / {}", item.source, item.path))
            .or_else(|| Some("Official Codex desktop client was not detected".to_string())),
        install_kind: None,
        running: process_control::is_process_running("Codex"),
    }
}

fn build_plan(
    settings: &CodexClientSettings,
    installed: Option<&InstalledCodexClient>,
    release: &CodexClientRelease,
) -> Result<CodexClientPlan, String> {
    let capabilities = probe_capabilities();
    let current_version = installed.map(|item| item.version.clone());
    let up_to_date = current_version
        .as_deref()
        .map(|version| compare_versions(version, &release.version) != Ordering::Less)
        .unwrap_or(false);
    let route = select_install_route(settings, installed).to_string();
    let mut warnings = Vec::new();
    if route == "unsupported" {
        warnings.push("The current platform does not provide an executable Codex desktop client install path yet.".to_string());
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

    Ok(CodexClientPlan {
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
    settings: &CodexClientSettings,
    installed: Option<&InstalledCodexClient>,
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
    release: &CodexClientRelease,
    plan: &CodexClientPlan,
    on_progress: &F,
) -> Result<CodexClientStageReport, String>
where
    F: Fn(CodexClientProgress),
{
    if plan.up_to_date {
        emit_step_progress(
            on_progress,
            install_kind,
            "done",
            "codexClient.progressStageAlreadyUpToDate",
            Some(1),
            Some(1),
            Some(4),
            Some(4),
        );
        return Ok(CodexClientStageReport {
            install_kind: install_kind.to_string(),
            up_to_date: true,
            staged_path: None,
            package_moniker: release.package_moniker.clone(),
            download_size: 0,
            sha256: release.sha256.clone(),
            hash_verified: true,
            route: plan.route.clone(),
            notes: vec!["Codex is already up to date; no download is needed.".to_string()],
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
                "codexClient.progressFoundStaged",
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
        "codexClient.progressVerifying",
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
        format!("Staged Codex package {}.", release.package_moniker),
    );
    emit_step_progress(
        on_progress,
        install_kind,
        "done",
        "codexClient.progressStageDone",
        Some(size),
        Some(size),
        Some(4),
        Some(4),
    );

    Ok(CodexClientStageReport {
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

fn cleanup_staged_package(stage: &mut CodexClientStageReport, notes: &mut Vec<String>) {
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

fn load_release(settings: &CodexClientSettings) -> Result<CodexClientRelease, String> {
    let base = manifest_base(settings);
    let manifest_url = format!("{base}/latest/manifest");
    let checksums_url = format!("{base}/latest/checksums");
    let manifest_text = fetch_text(&manifest_url)?;
    let checksums_text = fetch_text(&checksums_url)?;
    let manifest: MirrorManifest = serde_json::from_str(&manifest_text)
        .map_err(|err| format!("Failed to parse Codex mirror manifest: {err}"))?;
    if manifest.schema_version < 2 {
        return Err(format!(
            "Unsupported Codex mirror manifest schemaVersion: {}",
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
            "Codex mirror manifest has no macOS installer information.".to_string()
        })?;
        let (source, arch) = current_macos_source(macos)?;
        let source_url = source
            .url
            .clone()
            .ok_or_else(|| format!("Codex mirror manifest has no macOS {arch} download URL."))?;
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
            .ok_or_else(|| format!("Codex mirror manifest has no macOS {arch} version."))?;

        return Ok(CodexClientRelease {
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

    Ok(CodexClientRelease {
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

/// Detect both install kinds (MSIX and portable) of the Codex desktop client
/// simultaneously so the UI can show a per-kind tab. Each kind is resolved
/// independently; a user may have both installed at once.
pub fn codex_client_install_kinds() -> CodexClientInstallKinds {
    if !cfg!(target_os = "windows") {
        return CodexClientInstallKinds {
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
    CodexClientInstallKinds { msix, portable }
}

fn detect_installed(settings: &CodexClientSettings) -> Option<InstalledCodexClient> {
    if cfg!(target_os = "windows") {
        package::detect_msix_package(PACKAGE_IDENTITY)
            .map(installed_from_msix)
            .or_else(|| {
                expand_env_path(&settings.install_root)
                    .ok()
                    .and_then(|root| detect_portable_install(&root))
            })
    } else if cfg!(target_os = "macos") {
        package::detect_macos_app(&macos_app_candidates(), Some(CODEX_MACOS_BUNDLE_ID))
            .map(installed_from_macos_app)
    } else {
        None
    }
}

fn normalize_install_kind(requested: Option<&str>, settings: &CodexClientSettings) -> String {
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
    mut settings: CodexClientSettings,
    install_kind: &str,
) -> CodexClientSettings {
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
    settings: &CodexClientSettings,
    install_kind: &str,
) -> Option<InstalledCodexClient> {
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

fn installed_from_msix(package: package::InstalledMsixPackage) -> InstalledCodexClient {
    InstalledCodexClient {
        installed_at: path_mtime(&PathBuf::from(&package.path)),
        path: package.path,
        version: package.version,
        arch: package.arch,
        source: "msix".to_string(),
        package_family_name: package.package_family_name,
    }
}

fn installed_from_macos_app(app: package::InstalledMacosApp) -> InstalledCodexClient {
    InstalledCodexClient {
        installed_at: path_mtime(&PathBuf::from(&app.path)),
        path: app.path,
        version: app.version,
        arch: None,
        source: "macos".to_string(),
        package_family_name: app.bundle_identifier,
    }
}

fn detect_portable_install(root: &Path) -> Option<InstalledCodexClient> {
    let exe = root.join("Codex.exe");
    if !exe.is_file() {
        return None;
    }
    let identity = fs::read_to_string(root.join("AppxManifest.xml"))
        .ok()
        .and_then(|xml| parse_msix_identity(&xml).ok());
    Some(InstalledCodexClient {
        path: root.to_string_lossy().to_string(),
        version: identity
            .as_ref()
            .map(|item| item.version.clone())
            .unwrap_or_else(|| "0.0.0.0".to_string()),
        arch: identity
            .as_ref()
            .map(|item| item.processor_architecture.clone()),
        source: "portable".to_string(),
        package_family_name: None,
        installed_at: path_mtime(&exe),
    })
}

fn macos_app_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications").join(CODEX_MACOS_APP_NAME)];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications").join(CODEX_MACOS_APP_NAME));
    }
    candidates
}

struct PortableInstallReport {
    installed: Option<InstalledCodexClient>,
    notes: Vec<String>,
}

fn install_portable<F>(
    msix_path: &Path,
    install_root: &Path,
    install_kind: &str,
    on_progress: &F,
) -> Result<PortableInstallReport, String>
where
    F: Fn(CodexClientProgress),
{
    emit_step_progress(
        on_progress,
        install_kind,
        "installing",
        "codexClient.progressPreparingPortableDir",
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
        .join(".codestudio-codex-client-staging")
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
        "codexClient.progressCopyingPortable",
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
        "codexClient.progressWritingInstall",
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
        "codexClient.progressFinalizingInstall",
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
        "codexClient.progressPortableWritten",
        Some(1),
        Some(1),
        Some(6),
        Some(7),
    );

    Ok(PortableInstallReport {
        installed: Some(InstalledCodexClient {
            path: install_root.to_string_lossy().to_string(),
            version: identity.version,
            arch: Some(identity.processor_architecture),
            source: "portable".to_string(),
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
    F: Fn(CodexClientProgress),
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
        "codexClient.progressExtractingMsix",
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
                "codexClient.progressExtractingMsix",
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

fn probe_capabilities() -> Vec<CodexClientCapability> {
    let capabilities = if cfg!(target_os = "macos") {
        package::probe_macos_dmg_capabilities()
    } else {
        package::probe_msix_capabilities()
    };
    capabilities
        .into_iter()
        .map(|capability| CodexClientCapability {
            id: capability.id,
            label: capability.label,
            status: capability.status,
            detail: capability.detail,
        })
        .collect()
}

fn manifest_base(_settings: &CodexClientSettings) -> String {
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
                "Codex mirror manifest has no macOS arm64 installer information.".to_string()
            })
    } else {
        macos
            .x64
            .as_ref()
            .map(|source| (source, "x64"))
            .ok_or_else(|| {
                "Codex mirror manifest has no macOS x64 installer information.".to_string()
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
    let output = hidden_command("curl")
        .args(["-fsSL", "--connect-timeout", "20", "--retry", "2", url])
        .output()
        .map_err(|err| format!("Failed to start curl: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to read {}: {}",
            url_host(url),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|err| format!("Response is not UTF-8: {err}"))
}

fn download_to_file<F>(
    url: &str,
    path: &Path,
    expected_total: Option<u64>,
    install_kind: &str,
    on_progress: &F,
) -> Result<(), String>
where
    F: Fn(CodexClientProgress),
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create download directory: {err}"))?;
    }
    let temp = download_temp_path(path);
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    emit_step_progress(
        on_progress,
        install_kind,
        "downloading",
        "codexClient.progressDownloading",
        Some(0),
        expected_total,
        Some(2),
        Some(4),
    );
    let mut child = hidden_command("curl")
        .args([
            "-fLsS",
            "--connect-timeout",
            "20",
            "--retry",
            "2",
            "--output",
            &temp.to_string_lossy(),
            url,
        ])
        .spawn()
        .map_err(|err| format!("Failed to start download: {err}"))?;
    let mut last_emit = Instant::now() - Duration::from_secs(2);
    loop {
        match child
            .try_wait()
            .map_err(|err| format!("Failed while waiting for download process: {err}"))?
        {
            Some(_) => break,
            None => {
                let downloaded = fs::metadata(&temp).ok().map(|metadata| metadata.len());
                if last_emit.elapsed() >= Duration::from_millis(500) {
                    emit_step_progress(
                        on_progress,
                        install_kind,
                        "downloading",
                        "codexClient.progressDownloading",
                        downloaded,
                        expected_total,
                        Some(2),
                        Some(4),
                    );
                    last_emit = Instant::now();
                }
                thread::sleep(Duration::from_millis(150));
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("Failed to read download result: {err}"))?;
    if !output.status.success() {
        let _ = fs::remove_file(&temp);
        return Err(format!(
            "Failed to download {}: {}",
            url_host(url),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let downloaded = fs::metadata(&temp).ok().map(|metadata| metadata.len());
    emit_step_progress(
        on_progress,
        install_kind,
        "downloading",
        "codexClient.progressDownloadComplete",
        downloaded,
        expected_total,
        Some(2),
        Some(4),
    );
    if path.exists() {
        fs::remove_file(path).map_err(|err| {
            format!(
                "Failed to replace staged download {}: {err}",
                display_path(path)
            )
        })?;
    }
    fs::rename(&temp, path).map_err(|err| {
        let _ = fs::remove_file(&temp);
        format!(
            "Failed to save downloaded file to {}: {err}",
            display_path(path)
        )
    })
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
    F: Fn(CodexClientProgress),
{
    let percent = match (downloaded, total) {
        (Some(done), Some(total)) if total > 0 => {
            Some(((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0))
        }
        _ => None,
    };
    on_progress(CodexClientProgress {
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

fn staged_package_path(release: &CodexClientRelease) -> Result<PathBuf, String> {
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
    let dir = paths.downloads_dir.join("codex-client");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn load_settings() -> Result<CodexClientSettings, String> {
    let Some(json) = storage::load_state_json(CODEX_CLIENT_SETTINGS_STATE_KEY)? else {
        let settings = CodexClientSettings::default();
        save_settings(&settings)?;
        return Ok(settings);
    };
    let mut settings: CodexClientSettings = serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse Codex settings: {err}"))?;
    settings.source = normalize_source(&settings.source);
    settings.custom_url = String::new();
    settings.signed_only = true;
    if settings.install_root.trim().is_empty() {
        settings.install_root = default_install_root();
    }
    Ok(settings)
}

fn save_settings(settings: &CodexClientSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    storage::save_state_json(CODEX_CLIENT_SETTINGS_STATE_KEY, &json)
        .map_err(|err| format!("Failed to save Codex settings: {err}"))
}

fn save_marker(marker: &ManagedInstallMarker) -> Result<(), String> {
    let json = serde_json::to_string_pretty(marker).map_err(|err| err.to_string())?;
    storage::save_state_json(CODEX_CLIENT_MARKER_STATE_KEY, &json)
        .map_err(|err| format!("Failed to save Codex managed marker: {err}"))
}

fn load_marker() -> Option<ManagedInstallMarker> {
    storage::load_state_json(CODEX_CLIENT_MARKER_STATE_KEY)
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn install_class(installed: Option<&InstalledCodexClient>) -> String {
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

fn validate_install_target(settings: &CodexClientSettings) -> Result<(), String> {
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
            "Install location must be an empty folder or an existing Codex portable directory."
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
        "/Applications/Codex.app".to_string()
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

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn terminate_codex_process_for_uninstall(
    root: Option<&Path>,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }
    let root_filter = root
        .map(|path| ps_quote(&path.to_string_lossy()))
        .unwrap_or_else(|| "$null".to_string());
    let script = format!(
        r#"
$RootFilter = {root_filter}
if ($null -ne $RootFilter) {{
  try {{ $RootFilter = [System.IO.Path]::GetFullPath($RootFilter).TrimEnd('\') }} catch {{}}
}}
function Get-TargetCodexProcess {{
  $all = Get-Process -Name Codex -ErrorAction SilentlyContinue
  foreach ($p in $all) {{
    if ($null -eq $RootFilter) {{
      $p
      continue
    }}
    try {{
      $path = [string]$p.Path
      if (-not $path) {{ continue }}
      $full = [System.IO.Path]::GetFullPath($path)
      if ($full.Equals($RootFilter, [System.StringComparison]::OrdinalIgnoreCase) -or
          $full.StartsWith($RootFilter + '\', [System.StringComparison]::OrdinalIgnoreCase)) {{
        $p
      }}
    }} catch {{}}
  }}
}}
$procs = @(Get-TargetCodexProcess)
$targetIds = @($procs | ForEach-Object {{ $_.Id }})
foreach ($p in $procs) {{
  try {{
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
  total = [int]$targetIds.Count
  forced = [int]$forced
  remaining = [int]$still.Count
}} | ConvertTo-Json -Compress
"#
    );
    let json = run_powershell(&script)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse Codex process termination result: {err}"))?;
    let total = value
        .get("total")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    let forced = value
        .get("forced")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    let remaining = value
        .get("remaining")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    if remaining > 0 {
        return Err(
            "A Codex desktop process is still running; uninstall was not continued.".to_string(),
        );
    }
    if total > 0 {
        if forced > 0 {
            notes.push(format!(
                "Codex desktop was running; force-closed {forced} process(es) before uninstalling."
            ));
        } else {
            notes.push("Codex desktop was running and was closed before uninstalling.".to_string());
        }
    }
    Ok(())
}

fn terminate_codex_process_for_restart(
    root: Option<&Path>,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        let _ = root;
        return terminate_macos_codex_process_for_restart(notes);
    }

    if !cfg!(target_os = "windows") {
        return Ok(());
    }
    let root_filter = root
        .map(|path| ps_quote(&path.to_string_lossy()))
        .unwrap_or_else(|| "$null".to_string());
    let script = format!(
        r#"
$RootFilter = {root_filter}
if ($null -ne $RootFilter) {{
  try {{ $RootFilter = [System.IO.Path]::GetFullPath($RootFilter).TrimEnd('\') }} catch {{}}
}}
function Get-TargetCodexProcess {{
  $all = Get-Process -Name Codex -ErrorAction SilentlyContinue
  foreach ($p in $all) {{
    if ($null -eq $RootFilter) {{
      $p
      continue
    }}
    try {{
      $path = [string]$p.Path
      if (-not $path) {{ continue }}
      $full = [System.IO.Path]::GetFullPath($path)
      if ($full.Equals($RootFilter, [System.StringComparison]::OrdinalIgnoreCase) -or
          $full.StartsWith($RootFilter + '\', [System.StringComparison]::OrdinalIgnoreCase)) {{
        $p
      }}
    }} catch {{}}
  }}
}}
$procs = @(Get-TargetCodexProcess)
$targetIds = @($procs | ForEach-Object {{ $_.Id }})
foreach ($p in $procs) {{
  try {{
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
  total = [int]$targetIds.Count
  forced = [int]$forced
  remaining = [int]$still.Count
}} | ConvertTo-Json -Compress
"#
    );
    let json = run_powershell(&script)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse Codex process restart result: {err}"))?;
    let total = value
        .get("total")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    let forced = value
        .get("forced")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    let remaining = value
        .get("remaining")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    if remaining > 0 {
        return Err(
            "A Codex desktop process is still running; restart was not continued.".to_string(),
        );
    }
    if total > 0 {
        if forced > 0 {
            notes.push(format!(
                "Force-closed {forced} running Codex desktop process(es)."
            ));
        } else {
            notes.push("Closed the running Codex desktop process.".to_string());
        }
    }
    Ok(())
}

fn terminate_macos_codex_process_for_restart(notes: &mut Vec<String>) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }

    let pids = macos_codex_process_ids()?;
    if pids.is_empty() {
        return Ok(());
    }

    for pid in &pids {
        let _ = hidden_command("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
    }
    wait_for_macos_codex_process_exit(&pids, Duration::from_secs(8));

    let remaining_after_term = pids
        .iter()
        .copied()
        .filter(|pid| macos_codex_pid_alive(*pid))
        .collect::<Vec<_>>();
    let mut forced = 0;
    for pid in &remaining_after_term {
        let output = hidden_command("kill")
            .args(["-KILL", &pid.to_string()])
            .output()
            .map_err(|err| format!("Failed to force-close Codex desktop: {err}"))?;
        if output.status.success() {
            forced += 1;
        }
    }
    wait_for_macos_codex_process_exit(&remaining_after_term, Duration::from_millis(500));

    let remaining = pids
        .iter()
        .copied()
        .filter(|pid| macos_codex_pid_alive(*pid))
        .count();
    if remaining > 0 {
        return Err(
            "A Codex desktop process is still running; restart was not continued.".to_string(),
        );
    }

    if forced > 0 {
        notes.push(format!(
            "Force-closed {forced} running Codex desktop process(es)."
        ));
    } else {
        notes.push("Closed the running Codex desktop process.".to_string());
    }
    Ok(())
}

fn macos_codex_process_ids() -> Result<Vec<u32>, String> {
    if !cfg!(target_os = "macos") {
        return Ok(Vec::new());
    }
    let output = hidden_command("pgrep")
        .args(["-x", "Codex"])
        .output()
        .map_err(|err| format!("Failed to inspect running Codex processes: {err}"))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let current_pid = std::process::id();
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter(|pid| *pid != current_pid)
        .collect())
}

fn wait_for_macos_codex_process_exit(pids: &[u32], timeout: Duration) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if pids.iter().all(|pid| !macos_codex_pid_alive(*pid)) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn macos_codex_pid_alive(pid: u32) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    hidden_command("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn launch_installed_codex(installed: &InstalledCodexClient, args: &[String]) -> Result<(), String> {
    if installed.source == "portable" {
        let exe = Path::new(&installed.path).join(CODEX_EXE_NAME);
        hidden_command(exe)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to launch Codex: {err}"))?;
    } else if cfg!(target_os = "windows") {
        package::launch_msix_package_with_args(PACKAGE_IDENTITY, args)
            .map(|_| ())
            .map_err(|err| format!("Failed to launch Codex with patch arguments: {err}"))?;
    } else if cfg!(target_os = "macos") {
        let path = Path::new(&installed.path);
        hidden_command("open")
            .arg(path)
            .arg("--args")
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to launch Codex with patch arguments: {err}"))?;
    } else {
        return Err("Launching Codex is not supported on the current platform.".to_string());
    }
    Ok(())
}

fn sync_history_if_enabled(settings: &CodexClientSettings) -> Result<(), String> {
    if !settings.sync_history_on_launch {
        return Ok(());
    }
    let report = codex_provider_sync::run_default_provider_sync()?;
    let _ = activity_log::append(
        Severity::Info,
        format!(
            "Synchronized Codex history provider to {} ({} session files, {} sqlite rows).",
            report.target_provider, report.changed_session_files, report.sqlite_rows_updated
        ),
    );
    Ok(())
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

fn inject_plugin_unlock(debug_port: u16) -> Result<(), String> {
    let mut last_error = None;
    for _ in 0..CODEX_PATCH_INJECTION_RETRY_COUNT {
        match try_inject_plugin_unlock(debug_port) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
                thread::sleep(Duration::from_millis(CODEX_PATCH_INJECTION_RETRY_MS));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "Codex patch injection failed.".to_string()))
}

fn spawn_plugin_unlock_injection(debug_port: u16) {
    thread::spawn(move || match inject_plugin_unlock(debug_port) {
        Ok(()) => {
            let _ = activity_log::append(Severity::Ok, "Applied Codex plugin unlock patch.");
        }
        Err(err) => {
            let _ = activity_log::append(
                Severity::Error,
                format!("Codex plugin unlock patch failed: {err}"),
            );
        }
    });
}

fn try_inject_plugin_unlock(debug_port: u16) -> Result<(), String> {
    let target = pick_cdp_target(debug_port)?;
    let ws_url = target
        .web_socket_debugger_url
        .ok_or_else(|| "Selected Codex CDP target has no WebSocket debugger URL.".to_string())?;
    evaluate_cdp_script(&ws_url, plugin_unlock_script())
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
                    if let Some(target) = targets.iter().find(|target| {
                        target.target_type == "page"
                            && target
                                .web_socket_debugger_url
                                .as_deref()
                                .is_some_and(|item| !item.is_empty())
                            && format!("{} {}", target.title, target.url)
                                .to_ascii_lowercase()
                                .contains("codex")
                    }) {
                        return Ok(target.clone());
                    }
                    if let Some(target) = targets.iter().find(|target| {
                        target.target_type == "page"
                            && target
                                .web_socket_debugger_url
                                .as_deref()
                                .is_some_and(|item| !item.is_empty())
                    }) {
                        return Ok(target.clone());
                    }
                    errors.push(format!("{url}: no page target"));
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

fn plugin_unlock_script() -> &'static str {
    r#"
(() => {
  if (window.__codestudioLitePluginUnlock === "1") {
    window.__codestudioLitePluginUnlockRefresh?.();
    return true;
  }
  window.__codestudioLitePluginUnlock = "1";
  const styleId = "codestudio-lite-plugin-unlock-style";
  const pluginMarketplaceUnlockVersion = "1";
  const modulePromises = new Map();
  const installSelector = 'button:disabled, button[aria-disabled="true"], [role="button"][aria-disabled="true"], button[data-disabled], [role="button"][data-disabled], button.cursor-not-allowed, [role="button"].cursor-not-allowed, button.pointer-events-none, [role="button"].pointer-events-none';
  const pluginNavSelector = 'nav[role="navigation"] button.h-token-nav-row.w-full';
  const pluginSvgSelector = 'svg path[d^="M7.94562 14.0277"]';

  function ensureStyle() {
    if (document.getElementById(styleId)) return;
    const style = document.createElement("style");
    style.id = styleId;
    style.textContent = `.codestudio-lite-force-install-unlocked{opacity:1!important;pointer-events:auto!important;cursor:pointer!important}`;
    document.head.appendChild(style);
  }

  function reactFiberFrom(element) {
    const key = Object.keys(element || {}).find((item) => item.startsWith("__reactFiber"));
    return key ? element[key] : null;
  }

  function authContextValueFrom(element) {
    for (let fiber = reactFiberFrom(element); fiber; fiber = fiber.return) {
      for (const value of [fiber.memoizedProps?.value, fiber.pendingProps?.value]) {
        if (value && typeof value === "object" && typeof value.setAuthMethod === "function" && "authMethod" in value) {
          return value;
        }
      }
    }
    return null;
  }

  function spoofChatGPTAuthMethod(element) {
    const auth = authContextValueFrom(element);
    if (!auth || auth.authMethod === "chatgpt") return false;
    try {
      auth.setAuthMethod("chatgpt");
      return true;
    } catch (_) {
      return false;
    }
  }

  function pluginEntryButton() {
    const byIcon = document.querySelector(`${pluginNavSelector} ${pluginSvgSelector}`)?.closest("button");
    if (byIcon) return byIcon;
    return Array.from(document.querySelectorAll(pluginNavSelector))
      .find((button) => /^(插件|Plugins)(\\s+-\\s+.*)?$/i.test((button.textContent || "").trim())) || null;
  }

  function enablePluginEntry() {
    const button = pluginEntryButton();
    if (!button) return;
    spoofChatGPTAuthMethod(button);
    button.disabled = false;
    button.removeAttribute("disabled");
    button.removeAttribute("aria-disabled");
    button.removeAttribute("data-disabled");
    button.style.display = "";
    button.querySelectorAll("*").forEach((node) => {
      node.style.display = "";
      node.removeAttribute?.("aria-disabled");
      node.removeAttribute?.("data-disabled");
    });
    const propsKey = Object.keys(button).find((key) => key.startsWith("__reactProps"));
    if (propsKey && button[propsKey]) {
      button[propsKey].disabled = false;
      button[propsKey]["aria-disabled"] = false;
    }
    if (button.dataset.codestudioLitePluginEntry !== "true") {
      button.dataset.codestudioLitePluginEntry = "true";
      button.addEventListener("click", () => spoofChatGPTAuthMethod(button), true);
    }
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

  async function loadCodexAppModule(namePart) {
    if (!modulePromises.has(namePart)) {
      const promise = Promise.resolve().then(async () => {
        const url = codexAppAssetUrl(namePart);
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
    if (hadMarketplaceKinds) delete next.marketplaceKinds;
    recordPluginUnlockDiagnostic("plugin_marketplace_request_expanded", {
      hadMarketplaceKinds,
      cwdCount: Array.isArray(next.cwds) ? next.cwds.length : 0,
    });
    return next;
  }

  function pluginMarketplaceAliasForName(name) {
    if (name === "openai-curated") return "codestudio-lite-openai-curated";
    if (name === "openai-primary-runtime") return "codestudio-lite-openai-primary-runtime";
    return "";
  }

  function displayNameForPluginMarketplaceName(name, fallback) {
    if (name === "openai-bundled" || name === "codestudio-lite-openai-bundled") return "OpenAI插件1(CodeStudio)";
    if (name === "openai-curated" || name === "codestudio-lite-openai-curated") return "OpenAI插件2(CodeStudio)";
    if (name === "openai-primary-runtime" || name === "codestudio-lite-openai-primary-runtime") return "OpenAI插件3(CodeStudio)";
    return fallback;
  }

  function patchPluginMarketplaceObject(marketplace) {
    if (!marketplace || typeof marketplace !== "object" || marketplace.__codestudioLiteMarketplaceUnlockPatched) return false;
    const alias = pluginMarketplaceAliasForName(marketplace.name);
    if (alias) marketplace.name = alias;
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

  function restorePluginMarketplaceName(name) {
    if (name === "codestudio-lite-openai-bundled" || name === "codex-plus-openai-bundled") return "openai-bundled";
    if (name === "codestudio-lite-openai-curated" || name === "codex-plus-openai-curated") return "openai-curated";
    if (name === "codestudio-lite-openai-primary-runtime" || name === "codex-plus-openai-primary-runtime") return "openai-primary-runtime";
    return name;
  }

  function codexPluginOfficialMarketplaceName(name) {
    const restored = restorePluginMarketplaceName(name);
    return restored === "openai-bundled" || restored === "openai-curated" || restored === "openai-primary-runtime";
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

  function installButtonLabel(element) {
    return (element.textContent || "").trim();
  }

  function isInstallButtonLabel(text) {
    return /^安装\\s*/.test(text) || /^Install\\s*/i.test(text) || text === "强制安装";
  }

  function patchReactDisabledProps(element) {
    Object.keys(element || {})
      .filter((key) => key.startsWith("__reactProps"))
      .forEach((key) => {
        const props = element[key];
        if (!props || typeof props !== "object") return;
        props.disabled = false;
        props["aria-disabled"] = false;
        props["data-disabled"] = undefined;
      });
  }

  function clearDisabledState(element) {
    if (!(element instanceof HTMLElement)) return;
    if ("disabled" in element) element.disabled = false;
    element.removeAttribute("disabled");
    element.removeAttribute("aria-disabled");
    element.removeAttribute("data-disabled");
    element.removeAttribute("inert");
    element.classList.remove("disabled", "opacity-50", "cursor-not-allowed", "pointer-events-none");
    element.classList.add("codestudio-lite-force-install-unlocked");
    element.style.pointerEvents = "auto";
    element.style.opacity = "";
    element.style.cursor = "pointer";
    element.tabIndex = 0;
    patchReactDisabledProps(element);
  }

  function unlockNodes(button) {
    const nodes = [button];
    button.querySelectorAll?.("button, [role='button'], [disabled], [aria-disabled], [data-disabled], .cursor-not-allowed, .pointer-events-none")
      .forEach((node) => nodes.push(node));
    let parent = button.parentElement;
    for (let depth = 0; parent && depth < 3; depth += 1, parent = parent.parentElement) {
      if (parent.matches?.("button, [role='button'], [disabled], [aria-disabled], [data-disabled], .cursor-not-allowed, .pointer-events-none")) {
        nodes.push(parent);
      }
    }
    return Array.from(new Set(nodes));
  }

  function labelForcedInstallButton(button) {
    const walker = document.createTreeWalker(button, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const node = walker.currentNode;
      if (isInstallButtonLabel((node.nodeValue || "").trim())) {
        node.nodeValue = "强制安装";
        return;
      }
    }
  }

  function unlockInstallButtons() {
    const nodes = Array.from(document.querySelectorAll(installSelector));
    const buttons = Array.from(new Set(nodes.map((node) => node.closest?.("button, [role='button']") || node)));
    buttons.forEach((button) => {
      if (!isInstallButtonLabel(installButtonLabel(button))) return;
      unlockNodes(button).forEach(clearDisabledState);
      labelForcedInstallButton(button);
      if (button.dataset.codestudioLiteForceInstall !== "true") {
        button.dataset.codestudioLiteForceInstall = "true";
        const keepUnlocked = () => unlockNodes(button).forEach(clearDisabledState);
        ["pointerdown", "mousedown", "mouseup", "click", "focus"].forEach((eventName) => {
          button.addEventListener(eventName, keepUnlocked, true);
        });
      }
    });
  }

  function refresh() {
    ensureStyle();
    installPluginBuildFlavorFilterPatch();
    installPluginMarketplaceRequestPatch();
    enablePluginEntry();
    unlockInstallButtons();
  }

  window.__codestudioLitePluginUnlockRefresh = refresh;
  refresh();
  if (!window.__codestudioLitePluginUnlockTimer) {
    window.__codestudioLitePluginUnlockTimer = setInterval(refresh, 1000);
  }
  if (!window.__codestudioLitePluginUnlockObserver) {
    const observer = new MutationObserver(() => refresh());
    observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled", "aria-disabled", "data-disabled", "class", "style"] });
    window.__codestudioLitePluginUnlockObserver = observer;
  }
  return true;
})()
"#
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

fn url_host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}

#[cfg(test)]
#[cfg(target_os = "windows")]
#[path = "codex_client_tests.rs"]
mod codex_client_tests;
