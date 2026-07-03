use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path};
use crate::core::codex_client;
use crate::core::env_health;
use crate::core::npm_global;
use crate::core::platform::{hidden_command_with_args, package, resolve_command};
use crate::core::process_control;
use crate::core::profile;
use crate::core::storage;
use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};
use crate::core::types::{
    ClaudeDesktopInstallKinds, ConfigState, DesktopInstallKindInfo, DetectionSnapshot,
    DetectionSource, InstallState, Problem, Severity, ToolCategory, ToolStatus,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const VERSION_CHECK_TIMEOUT: Duration = Duration::from_millis(6000);
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_millis(15000);
const UPDATE_CACHE_TTL: Duration = Duration::from_secs(600);
// Update checks spawn background threads that hit the network / spawn package
// managers (npm outdated, winget upgrade, claude.ai release JSON). These are
// slow (multi-second) and only populate the "update available" badge, not the
// install/version detection that drives the scan. The scan must not block on
// them: detect_environment passes a near-zero wait budget so it kicks off the
// background fetch and returns the local detection result immediately. The
// fetched update status lands in the shared cache and is surfaced on the next
// (cached) scan — e.g. the dashboard's 30s background re-scan.
const NPM_UPDATE_WAIT_BUDGET: Duration = Duration::from_millis(1);
const WINGET_UPDATE_WAIT_BUDGET: Duration = Duration::from_millis(1);
const CLAUDE_DESKTOP_UPDATE_WAIT_BUDGET: Duration = Duration::from_millis(1);
const CODEX_CLIENT_UPDATE_WAIT_BUDGET: Duration = Duration::from_millis(1);
const FOREGROUND_UPDATE_WAIT_BUDGET: Duration = Duration::from_millis(6000);
const CLAUDE_DESKTOP_INSTALL_CACHE_TTL: Duration = Duration::from_secs(30);
const UPDATE_CACHE_POLL_INTERVAL: Duration = Duration::from_millis(50);
const CLAUDE_DESKTOP_LATEST_WINDOWS_URL: &str =
    "https://downloads.claude.ai/releases/win32/x64/.latest";
const CLAUDE_DESKTOP_LATEST_MACOS_URL: &str =
    "https://downloads.claude.ai/releases/darwin/universal/.latest";
const CLAUDE_DESKTOP_WINDOWS_UPDATE_COMMAND: &str =
    "Download and install the latest Claude Desktop MSIX from https://claude.ai/api/desktop/win32/x64/msix/latest/redirect with Add-AppxPackage -Path";
#[cfg(target_os = "windows")]
const CLAUDE_DESKTOP_WINDOWS_PACKAGE_SUFFIX: &str = "pzs8sxrjxfjjc";

#[derive(Debug)]
enum VersionCheck {
    Found(String),
    NotFound(String),
    Failed,
    TimedOut,
}

pub fn load_cached_detection() -> Option<DetectionSnapshot> {
    storage::load_detection_cache().ok().flatten()
}

#[derive(Debug, Clone, Copy)]
pub struct DetectionOptions {
    pub wait_for_updates: bool,
}

impl DetectionOptions {
    fn update_wait_budget(self, default_budget: Duration) -> Duration {
        if self.wait_for_updates {
            FOREGROUND_UPDATE_WAIT_BUDGET
        } else {
            default_budget
        }
    }
}

impl Default for DetectionOptions {
    fn default() -> Self {
        Self {
            wait_for_updates: false,
        }
    }
}

pub fn detect_environment() -> Result<DetectionSnapshot, String> {
    detect_environment_with_options(DetectionOptions::default())
}

pub fn detect_environment_with_options(
    options: DetectionOptions,
) -> Result<DetectionSnapshot, String> {
    profile::ensure_app_dirs()?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mut tools = detect_tools(ai_tools_for_environment(resolve_command("code").is_some()));
    if supports_codex_desktop_client() {
        tools.push(codex_client::tool_status());
    }
    let mut system = detect_tools(system_tools());
    annotate_update_status(&mut tools, &mut system, options);
    let profile_summary = profile::load_profile_summary()?;
    let active_profile = profile_summary.active_profile.clone();
    let active_profile_name = profile_summary.active_profile_name.clone();
    let codex_auth = profile_summary.codex_auth.clone();
    let env_conflicts = env_health::claude_env_conflicts_for_active_config(
        &profile_summary.drafts,
        &profile_summary.active_profiles_by_mode.config,
    );
    let mut problems = Vec::new();

    for tool in tools.iter().chain(system.iter()) {
        if tool.install_state == InstallState::Missing {
            problems.push(Problem {
                id: format!("missing-{}", tool.id),
                severity: Severity::Warning,
                title: format!("{} is missing", tool.name),
                detail: tool
                    .install_command
                    .as_ref()
                    .map(|command| format!("Suggested command: {command}"))
                    .unwrap_or_else(|| "Install it before using related workflows.".to_string()),
                action_label: tool.install_command.as_ref().map(|_| "Install".to_string()),
            });
        } else if tool.category == ToolCategory::AiTool
            && tool.config_state == ConfigState::Unconfigured
        {
            problems.push(Problem {
                id: format!("unconfigured-{}", tool.id),
                severity: Severity::Info,
                title: format!("{} is not configured", tool.name),
                detail:
                    "Bootstrap this client to the Local Gateway after creating a Provider Profile."
                        .to_string(),
                action_label: Some("Configure".to_string()),
            });
        }
    }
    for conflict in &env_conflicts {
        problems.push(Problem {
            id: format!(
                "env-conflict-{}-{}-{}",
                conflict.tool_id, conflict.scope, conflict.variable
            ),
            severity: conflict.severity.clone(),
            title: format!("{} environment variable conflict", conflict.tool_name),
            detail: conflict.message.clone(),
            action_label: Some("Clear environment variables".to_string()),
        });
    }

    let _ = activity_log::append(Severity::Ok, "Completed local environment detection.");

    // Per-kind install detection for the desktop-client page tabs. Filled
    // here so it is cached alongside the snapshot and the tabs render
    // instantly from the on-disk cache before a fresh scan completes.
    let claude_install_kinds = if cfg!(target_os = "windows") {
        Some(claude_desktop_install_kinds())
    } else {
        None
    };
    let codex_install_kinds = if supports_codex_desktop_client() {
        Some(codex_client::codex_client_install_kinds())
    } else {
        None
    };

    let snapshot = DetectionSnapshot {
        generated_at: Utc::now().to_rfc3339(),
        source: DetectionSource::Live,
        platform: current_platform_label(),
        home_dir: display_path(&paths.home_dir),
        app_config_dir: display_path(&paths.config_dir),
        active_profile,
        active_profile_name,
        codex_auth,
        tools,
        system,
        problems,
        env_conflicts,
        claude_install_kinds,
        codex_install_kinds,
    };
    let _ = storage::store_detection_cache(&snapshot);
    Ok(snapshot)
}

/// Force a fresh install re-detection for a manual per-tool page refresh.
///
/// Unlike a plain `detect_environment`, this invalidates only the in-process
/// install cache (so a just-completed install is re-resolved instead of
/// serving a stale MSIX "not found") and preserves the slow, network-fetched
/// latest-version cache. It then blocks briefly for the Claude Desktop latest
/// version when it isn't cached yet, so a manual refresh surfaces the latest
/// version instead of showing "unknown" while the background fetch races the
/// fast scan's near-zero wait budget.
pub fn detect_environment_fresh() -> Result<DetectionSnapshot, String> {
    invalidate_install_cache();
    let mut snapshot = detect_environment_with_options(DetectionOptions {
        wait_for_updates: true,
    })?;
    surface_claude_desktop_latest(&mut snapshot, Duration::from_millis(3000));
    surface_codex_client_latest(&mut snapshot, Duration::from_millis(3000));
    // detect_environment already stored the fast snapshot; re-store so the
    // latest-version field populated above is persisted for cached reads.
    let _ = storage::store_detection_cache(&snapshot);
    Ok(snapshot)
}

/// Apply the cached Claude Desktop latest version to the snapshot's
/// claude-desktop tool, waiting up to `wait_budget` for an in-flight background
/// fetch to complete. Sets `latest_version` always and `update_available` only
/// when the tool is installed (mirroring `annotate_update_status`).
fn surface_claude_desktop_latest(snapshot: &mut DetectionSnapshot, wait_budget: Duration) {
    let Some(latest) = cached_claude_desktop_latest(wait_budget) else {
        return;
    };
    apply_claude_desktop_latest_to_tools(&mut snapshot.tools, &latest);
}

/// Pure helper: write the Claude Desktop latest version into the
/// claude-desktop tool entry. `latest_version` is always set (so the page can
/// show what would be installed even when Claude is missing); `update_available`
/// is only recomputed for an installed tool.
fn apply_claude_desktop_latest_to_tools(tools: &mut [ToolStatus], latest: &str) {
    for tool in tools.iter_mut() {
        if tool.id != "claude-desktop" {
            continue;
        }
        tool.latest_version = Some(latest.to_string());
        if tool.install_state == InstallState::Installed {
            tool.update_available = tool
                .version
                .as_deref()
                .map(|current| compare_versions(current, latest) == Ordering::Less)
                .unwrap_or(true);
        }
        break;
    }
}

/// Apply the cached Codex client latest version to the snapshot codex-app
/// tool, waiting up to wait_budget for an in-flight background fetch. Sets
/// latest_version always and update_available only when installed, mirroring
/// the Claude Desktop latest-version surfacing path.
fn surface_codex_client_latest(snapshot: &mut DetectionSnapshot, wait_budget: Duration) {
    let Some(latest) = codex_client::latest_version_cached(wait_budget) else {
        return;
    };
    apply_codex_client_latest_to_tools(&mut snapshot.tools, &latest);
}

fn apply_codex_client_latest_to_tools(tools: &mut [ToolStatus], latest: &str) {
    for tool in tools.iter_mut() {
        if tool.id != "codex-app" {
            continue;
        }
        tool.latest_version = Some(latest.to_string());
        if tool.install_state == InstallState::Installed {
            tool.update_available = tool
                .version
                .as_deref()
                .map(|current| compare_versions(current, latest) == Ordering::Less)
                .unwrap_or(true);
        }
        break;
    }
}

pub fn invalidate_update_cache() {
    {
        let mut cache = npm_update_cache().lock().unwrap();
        cache.packages.clear();
        cache.checked_at = None;
    }
    {
        let mut cache = winget_update_cache().lock().unwrap();
        cache.packages.clear();
        cache.checked_at = None;
    }
    {
        let mut cache = claude_desktop_update_cache().lock().unwrap();
        cache.version = None;
        cache.checked_at = None;
    }
    {
        let mut cache = claude_desktop_install_cache().lock().unwrap();
        cache.detected = None;
        cache.checked_at = None;
    }
}

/// Invalidate only the in-process install-detection cache (the MSIX package
/// lookup), preserving the network-fetched latest-version caches. Used by a
/// manual refresh so a just-completed install is re-resolved without throwing
/// away a known latest version (which `invalidate_update_cache` would discard,
/// leaving "latest version: unknown" until the next slow background fetch).
pub fn invalidate_install_cache() {
    let mut cache = claude_desktop_install_cache().lock().unwrap();
    cache.detected = None;
    cache.checked_at = None;
}

fn detect_tools(definitions: Vec<ToolDefinition>) -> Vec<ToolStatus> {
    definitions
        .into_iter()
        .map(|definition| thread::spawn(move || detect_tool(&definition)))
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|handle| handle.join().ok())
        .collect()
}

fn ai_tools_for_environment(vscode_available: bool) -> Vec<ToolDefinition> {
    ai_tools()
        .into_iter()
        .filter(|tool| vscode_available || !is_vscode_extension_tool(tool.id))
        .filter(|tool| tool.id != "claude-desktop" || supports_claude_desktop_client())
        .collect()
}

fn current_platform_label() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        std::env::consts::OS.to_string()
    }
}

fn supports_codex_desktop_client() -> bool {
    supports_codex_desktop_client_for_platform(&current_platform_label())
}

fn supports_codex_desktop_client_for_platform(platform: &str) -> bool {
    matches!(platform, "windows" | "macos")
}

fn supports_claude_desktop_client() -> bool {
    supports_claude_desktop_client_for_platform(&current_platform_label())
}

fn supports_claude_desktop_client_for_platform(platform: &str) -> bool {
    matches!(platform, "windows" | "macos")
}

fn is_vscode_extension_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        "codex-vscode" | "claude-vscode" | "gemini-code-assist"
    )
}

fn detect_tool(definition: &ToolDefinition) -> ToolStatus {
    if definition.id == "claude-desktop" {
        return detect_claude_desktop_tool(definition);
    }

    let resolved_command = resolve_command(definition.command);
    let npm_package_version = npm_package_for_tool(definition.id)
        .and_then(read_npm_global_package_version)
        .map(|version| VersionCheck::Found(version));
    let version_check = match (resolved_command.as_ref(), npm_package_version) {
        (Some(_), Some(version_check)) => Some(version_check),
        (Some(_), None) if definition.version_args.is_empty() => {
            Some(VersionCheck::Found("installed".to_string()))
        }
        (Some(command), None) => run_version(
            command,
            definition.version_args,
            definition.version_output_contains,
        ),
        _ => None,
    };
    let version = match &version_check {
        Some(VersionCheck::Found(version)) => Some(version.clone()),
        _ => None,
    };
    let install_state = match (&resolved_command, &version_check) {
        (Some(_), Some(VersionCheck::Found(_))) => InstallState::Installed,
        (Some(_), Some(VersionCheck::NotFound(_))) => InstallState::Missing,
        (Some(_), Some(VersionCheck::Failed | VersionCheck::TimedOut)) | (Some(_), None) => {
            InstallState::Unknown
        }
        _ => InstallState::Missing,
    };
    let config_path = definition
        .config_relative_path
        .and_then(|relative| app_paths().ok().map(|paths| paths.home_dir.join(relative)));
    let config_state = match (&definition.category, &config_path) {
        (ToolCategory::System, _) => ConfigState::NotApplicable,
        (_, Some(path)) if path.exists() => ConfigState::Configured,
        (_, Some(_)) => ConfigState::Unconfigured,
        _ => ConfigState::Unknown,
    };
    let details = match (&resolved_command, &version_check) {
        (Some(command), Some(VersionCheck::Found(_))) => Some(format!("Resolved: {command}")),
        (Some(command), Some(VersionCheck::TimedOut)) => Some(format!(
            "Version check timed out after {}ms: {command}",
            VERSION_CHECK_TIMEOUT.as_millis()
        )),
        (_, Some(VersionCheck::NotFound(detail))) => Some(detail.clone()),
        (Some(command), Some(VersionCheck::Failed)) | (Some(command), None) => {
            Some(format!("Version check failed: {command}"))
        }
        _ => Some("Command not found".to_string()),
    };
    let hermes_update_available = definition.id == "hermes"
        && matches!(version_check.as_ref(), Some(VersionCheck::Found(version)) if hermes_version_reports_update_available(version));

    ToolStatus {
        id: definition.id.to_string(),
        name: definition.name.to_string(),
        category: definition.category.clone(),
        command: definition.command.to_string(),
        path_repair: env_health::path_repair_hint(definition),
        version,
        latest_version: hermes_update_available.then(|| "latest".to_string()),
        update_available: hermes_update_available,
        update_command: update_command_for_tool(definition.id),
        install_state,
        config_state,
        config_path: config_path.as_deref().map(display_path),
        install_path: None,
        install_command: definition.install_command.map(ToString::to_string),
        details,
        install_kind: None,
        running: false,
    }
}

#[derive(Debug, Clone)]
struct DesktopAppDetection {
    path: String,
    version: String,
    source: &'static str,
}

fn detect_claude_desktop_tool(definition: &ToolDefinition) -> ToolStatus {
    let detected = detect_claude_desktop_installation();
    let config_path = definition
        .config_relative_path
        .and_then(|relative| app_paths().ok().map(|paths| paths.home_dir.join(relative)));
    let config_state = match &config_path {
        Some(path) if path.exists() => ConfigState::Configured,
        Some(_) => ConfigState::Unconfigured,
        None => ConfigState::Unknown,
    };
    let (install_state, install_kind, install_path, version, details) =
        claude_desktop_status_from_detection(detected.as_ref(), config_path.as_deref());

    ToolStatus {
        id: definition.id.to_string(),
        name: definition.name.to_string(),
        category: definition.category.clone(),
        command: definition.command.to_string(),
        path_repair: None,
        version,
        latest_version: None,
        update_available: false,
        update_command: update_command_for_tool(definition.id),
        install_state,
        config_state,
        config_path: config_path.as_deref().map(display_path),
        install_path,
        install_command: definition.install_command.map(ToString::to_string),
        details,
        install_kind,
        running: process_control::is_process_running("Claude"),
    }
}

fn claude_desktop_status_from_detection(
    detected: Option<&DesktopAppDetection>,
    config_path: Option<&Path>,
) -> (
    InstallState,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if let Some(app) = detected.filter(|app| app.source == "appx-stale") {
        return (
            InstallState::Missing,
            None,
            None,
            None,
            Some(format!(
                "MSIX/AppX package files are present but not registered: {}",
                app.path
            )),
        );
    }

    let install_kind = detected.map(|app| {
        if app.source.starts_with("appx") || app.source == "app-bundle" {
            "msix".to_string()
        } else {
            "exe".to_string()
        }
    });
    let details = detected
        .map(|app| format!("Resolved: {} ({})", app.path, app.source))
        .or_else(|| claude_desktop_missing_detail(config_path));

    (
        if detected.is_some() {
            InstallState::Installed
        } else {
            InstallState::Missing
        },
        install_kind,
        detected.map(|app| display_path(std::path::Path::new(&app.path))),
        detected.map(|app| app.version.clone()),
        details,
    )
}

fn detect_claude_desktop_installation() -> Option<DesktopAppDetection> {
    if cfg!(target_os = "windows") {
        return detect_claude_desktop_windows();
    }
    if cfg!(target_os = "macos") {
        return detect_claude_desktop_macos();
    }
    None
}

fn detect_claude_desktop_windows() -> Option<DesktopAppDetection> {
    detect_claude_desktop_windows_native_exe()
        .or_else(detect_claude_desktop_windows_registered_msix)
        // Last-resort fallback for a winget .exe (non-MSIX) install whose
        // install folder name we did not enumerate: scan %LOCALAPPDATA% one
        // level deep for a directory containing Claude.exe. Cheap (a single
        // read_dir + is_file per entry) and catches Anthropic's native
        // installer wherever it landed under the user's local app data.
        .or_else(detect_claude_desktop_windows_localappdata_scan)
}

fn detect_claude_desktop_windows_localappdata_scan() -> Option<DesktopAppDetection> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let local_app_data = env::var_os("LOCALAPPDATA")?;
    scan_localappdata_for_claude_exe(&PathBuf::from(local_app_data))
}

/// Public wrapper around the broad LOCALAPPDATA scan: returns the on-disk
/// path to the native (non-MSIX) Claude Desktop install that detection would
/// surface, or `None` when no such install is found. The returned path points
/// at the versioned `app-<version>/Claude.exe` image when present (so a real
/// version label is recoverable), or the bare launcher otherwise. Used by the
/// localized launch path as a fallback so it resolves the same install
/// detection finds, even when the explicit candidate list misses a location.
pub fn claude_desktop_windows_native_install_path() -> Option<PathBuf> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let local_app_data = env::var_os("LOCALAPPDATA")?;
    scan_localappdata_for_claude_exe(&PathBuf::from(local_app_data))
        .map(|detection| PathBuf::from(detection.path))
}
/// Detect both install kinds (MSIX and native .exe) of Claude Desktop
/// simultaneously so the UI can show a per-kind tab. Each kind is resolved
/// independently; a user may have both installed at once.
pub fn claude_desktop_install_kinds() -> ClaudeDesktopInstallKinds {
    if !cfg!(target_os = "windows") {
        return ClaudeDesktopInstallKinds {
            msix: DesktopInstallKindInfo {
                installed: false,
                version: None,
                path: None,
            },
            exe: DesktopInstallKindInfo {
                installed: false,
                version: None,
                path: None,
            },
        };
    }
    let msix = detect_claude_desktop_windows_registered_msix()
        .map(|app| DesktopInstallKindInfo {
            installed: true,
            version: Some(app.version.clone()),
            path: Some(app.path.clone()),
        })
        .unwrap_or(DesktopInstallKindInfo {
            installed: false,
            version: None,
            path: None,
        });
    let exe = detect_claude_desktop_windows_native_exe()
        .map(|app| DesktopInstallKindInfo {
            installed: true,
            version: Some(app.version.clone()),
            path: Some(app.path.clone()),
        })
        .unwrap_or(DesktopInstallKindInfo {
            installed: false,
            version: None,
            path: None,
        });
    ClaudeDesktopInstallKinds { msix, exe }
}

/// Search a LOCALAPPDATA-style root for a Claude Desktop native (.exe) install.
/// Last-resort fallback when MSIX detection and the explicit candidate paths
/// miss a winget `.exe` install. Descends only into top-level directories whose
/// name contains "claude"/"anthropic" plus the "programs" folder, then runs a
/// bounded search for `Claude.exe` inside each so the whole tree is never scanned.
fn scan_localappdata_for_claude_exe(root: &Path) -> Option<DesktopAppDetection> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_ascii_lowercase(),
            None => continue,
        };
        let relevant = name.contains("claude") || name.contains("anthropic") || name == "programs";
        if !relevant {
            continue;
        }
        if let Some(hit) = search_subtree_for_claude_exe(&path, 0) {
            return Some(hit);
        }
    }
    None
}

/// Bounded recursive search for `Claude.exe` within `dir`. The depth limit keeps
/// the fallback cheap and prevents descending into large unrelated subtrees.
/// Prefers the electron-builder `app-<version>/Claude.exe` layout so a real
/// version label can be recovered; otherwise returns the first `Claude.exe`.
/// Check whether a Claude.exe has its companion `resources/app.asar`, i.e.
/// the install is functional rather than a leftover exe from a partial
/// uninstall. Older localized builds may also have left an orphaned exe after
/// NSIS removed every companion file, so detection must not mistake that
/// leftover for a working install.
fn claude_exe_has_companion_asar(exe: &Path) -> bool {
    exe.parent()
        .map(|dir| dir.join("resources").join("app.asar").is_file())
        .unwrap_or(false)
}

/// Best-effort removal of an orphaned Claude.exe left behind by a partial
/// uninstall. Deletes the exe, and if it sat in an `app-<version>/` directory
/// that is now empty (or contains only leftover non-essential files), removes
/// that directory too so a fresh install is not confused by stale artifacts.
/// All errors are silently ignored: the caller already decided the install is
/// non-functional, so cleanup failure just means the orphan persists until the
/// user or OS removes it manually.
fn try_remove_orphaned_claude_exe(exe: &Path) {
    let _ = fs::remove_file(exe);
    if let Some(parent) = exe.parent() {
        if let Some(name) = parent.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("app-") {
                let _ = fs::remove_dir(parent);
            }
        }
    }
}

fn search_subtree_for_claude_exe(dir: &Path, depth: usize) -> Option<DesktopAppDetection> {
    const MAX_DEPTH: usize = 2;
    // Prefer the electron-builder `app-<version>/Claude.exe` (the real
    // Electron image carrying a version label) over a bare `Claude.exe`
    // (the Squirrel launcher at the install root, which has no version and no
    // asar-integrity fuse). Scanning children first means a Squirrel install
    // like `AnthropicClaude/{claude.exe, app-1.14271.0/claude.exe}` resolves
    // to the versioned image with its real version, not "installed".
    if depth < MAX_DEPTH {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(Result::ok) {
                let child = entry.path();
                if !child.is_dir() {
                    continue;
                }
                if let Some(child_name) = child.file_name().and_then(|n| n.to_str()) {
                    if child_name.starts_with("app-") {
                        let candidate = child.join("Claude.exe");
                        if candidate.is_file() {
                            if claude_exe_has_companion_asar(&candidate) {
                                return Some(DesktopAppDetection {
                                    path: candidate.to_string_lossy().to_string(),
                                    version: child_name.trim_start_matches("app-").to_string(),
                                    source: "app-path",
                                });
                            }
                            try_remove_orphaned_claude_exe(&candidate);
                        }
                    }
                }
                if let Some(hit) = search_subtree_for_claude_exe(&child, depth + 1) {
                    return Some(hit);
                }
            }
        }
    }
    // Fallback: a bare Claude.exe with no app-<version> sibling (non-Squirrel
    // layout, or the only exe present). No version label is recoverable here.
    let exe = dir.join("Claude.exe");
    if exe.is_file() {
        if claude_exe_has_companion_asar(&exe) {
            return Some(DesktopAppDetection {
                path: exe.to_string_lossy().to_string(),
                version: "installed".to_string(),
                source: "app-path",
            });
        }
        try_remove_orphaned_claude_exe(&exe);
    }
    None
}

fn detect_claude_desktop_windows_registered_msix() -> Option<DesktopAppDetection> {
    cached_claude_desktop_windows_msix_package().map(|package| DesktopAppDetection {
        path: package.path,
        // MSIX package versions are 4-part (e.g. 1.14271.0.0) but the upstream
        // release feed and winget report 3-part (1.14271.0). Normalize so the
        // displayed current version matches the latest version and version
        // comparison isn't skewed by a trailing build segment.
        version: normalized_claude_desktop_version(&package.version),
        source: "appx",
    })
}

pub(crate) fn claude_desktop_windows_registered_msix_installed() -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }
    detect_claude_desktop_windows_registered_msix().is_some()
}

/// Normalize an MSIX/install-detected Claude Desktop version to the 3-part
/// label used by the release feed. `Get-AppxPackage` reports 4-part versions
/// (e.g. 1.14271.0.0); the upstream feed and winget report 3-part
/// (1.14271.0). Drop a trailing zero 4th segment so the displayed current
/// version matches the latest version. Falls back to the raw value if
/// normalization fails so detection never loses the version.
fn normalized_claude_desktop_version(version: &str) -> String {
    let Some(label) = normalized_version_label(version) else {
        return version.to_string();
    };
    let parts: Vec<&str> = label.split('.').collect();
    if parts.len() == 4 && parts[3] == "0" {
        format!("{}.{}.{}", parts[0], parts[1], parts[2])
    } else {
        label
    }
}

fn detect_claude_desktop_macos() -> Option<DesktopAppDetection> {
    package::detect_macos_app(
        &claude_desktop_macos_app_candidates(),
        Some("com.anthropic.claudefordesktop"),
    )
    .or_else(|| package::detect_macos_app(&claude_desktop_macos_app_candidates(), None))
    .map(|app| DesktopAppDetection {
        path: app.path,
        version: app.version,
        source: "app-bundle",
    })
}

fn claude_desktop_missing_detail(config_path: Option<&Path>) -> Option<String> {
    if !cfg!(any(target_os = "windows", target_os = "macos")) {
        return Some("Unsupported platform".to_string());
    }
    if let Some(path) = config_path {
        if path.exists() {
            return Some(format!(
                "Claude Desktop config exists, but the application was not found: {}",
                display_path(path)
            ));
        }
    }
    Some("Claude Desktop application not found".to_string())
}

/// Resolve the native (non-MSIX) Claude Desktop exe from the explicit
/// candidate list, preferring the electron-builder `app-<version>/Claude.exe`
/// image (which carries a real version label and the asar-integrity fuse) over
/// the bare Squirrel launcher a candidate points at. For a Squirrel install
/// like `AnthropicClaude/{claude.exe, app-1.14271.0/claude.exe}`, a candidate
/// that hits the root launcher resolves to the versioned image with version
/// `1.14271.0` instead of "installed".
fn detect_claude_desktop_windows_native_exe() -> Option<DesktopAppDetection> {
    for candidate in claude_desktop_windows_exe_candidates() {
        if !candidate.is_file() {
            continue;
        }
        let root = candidate.parent()?;
        if let Some(app_version_dir) = newest_squirrel_app_version_dir(root) {
            let image = app_version_dir.join("Claude.exe");
            if image.is_file() {
                if claude_exe_has_companion_asar(&image) {
                    return Some(DesktopAppDetection {
                        path: image.to_string_lossy().to_string(),
                        version: app_version_dir
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.trim_start_matches("app-").to_string())
                            .unwrap_or_else(|| "installed".to_string()),
                        source: "app-path",
                    });
                }
                try_remove_orphaned_claude_exe(&image);
            }
        }
        if claude_exe_has_companion_asar(&candidate) {
            return Some(DesktopAppDetection {
                path: candidate.to_string_lossy().to_string(),
                version: "installed".to_string(),
                source: "app-path",
            });
        }
        try_remove_orphaned_claude_exe(&candidate);
    }
    None
}

/// Find the newest `app-<version>/` directory under `root`, returning its path.
/// Used to resolve a Squirrel/electron-builder install's versioned image.
fn newest_squirrel_app_version_dir(root: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    let mut versions: Vec<(PathBuf, Vec<u64>)> = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(version) = name.strip_prefix("app-") else {
            continue;
        };
        let parts: Vec<u64> = version
            .split('.')
            .filter_map(|part| part.parse::<u64>().ok())
            .collect();
        if parts.is_empty() {
            continue;
        }
        versions.push((path, parts));
    }
    versions.sort_by(|a, b| b.1.cmp(&a.1));
    versions.first().map(|(path, _)| path.clone())
}

fn claude_desktop_windows_exe_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        push_claude_desktop_windows_local_candidates(&mut candidates, Path::new(&local_app_data));
    }
    if let Ok(paths) = app_paths() {
        push_claude_desktop_windows_local_candidates(
            &mut candidates,
            &paths.home_dir.join("AppData").join("Local"),
        );
    }
    if let Some(program_files) = env::var_os("ProgramFiles") {
        push_claude_desktop_windows_program_files_candidates(
            &mut candidates,
            Path::new(&program_files),
        );
    }
    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        push_claude_desktop_windows_program_files_candidates(
            &mut candidates,
            Path::new(&program_files_x86),
        );
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn push_claude_desktop_windows_local_candidates(candidates: &mut Vec<PathBuf>, root: &Path) {
    candidates.push(root.join("Programs").join("Claude").join("Claude.exe"));
    candidates.push(root.join("Claude").join("Claude.exe"));
    // Anthropic's native Windows installer (distributed via winget as an .exe
    // installer, not MSIX) is an electron-builder/NSIS app that lands under
    // %LOCALAPPDATA% with a few possible folder names. Cover the common ones so
    // a winget-installed Claude on a clean VM is detected without relying on
    // Get-AppxPackage (which returns nothing for a non-MSIX install).
    candidates.push(root.join("Programs").join("claude").join("Claude.exe"));
    candidates.push(
        root.join("Programs")
            .join("AnthropicClaude")
            .join("Claude.exe"),
    );
    candidates.push(root.join("AnthropicClaude").join("Claude.exe"));
    candidates.push(
        root.join("Programs")
            .join("Anthropic")
            .join("Claude")
            .join("Claude.exe"),
    );
}

fn push_claude_desktop_windows_program_files_candidates(
    candidates: &mut Vec<PathBuf>,
    root: &Path,
) {
    candidates.push(root.join("Claude").join("Claude.exe"));
    candidates.push(root.join("Anthropic").join("Claude").join("Claude.exe"));
}

pub(crate) fn claude_desktop_windows_package_identities() -> &'static [&'static str] {
    &["Claude", "Anthropic.Claude"]
}

#[cfg(target_os = "windows")]
pub(crate) fn claude_desktop_windows_stale_msix_manifest() -> Option<PathBuf> {
    find_latest_claude_desktop_windows_stale_msix_dir()
        .map(|path| path.join("AppxManifest.xml"))
        .filter(|path| path.is_file())
}

#[cfg(target_os = "windows")]
pub(crate) fn claude_desktop_windows_cached_stale_msix_manifest() -> Option<PathBuf> {
    let snapshot = storage::load_detection_cache().ok()??;
    snapshot
        .tools
        .iter()
        .find(|tool| tool.id == "claude-desktop")
        .and_then(|tool| tool.details.as_deref())
        .and_then(|details| stale_msix_manifest_from_detection_details(details, "appx-stale"))
}

#[cfg(target_os = "windows")]
pub(crate) fn claude_desktop_windows_known_stale_msix_manifest() -> Option<PathBuf> {
    for version in claude_desktop_windows_known_versions() {
        for candidate in claude_desktop_windows_known_manifest_candidates(&version) {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn stale_msix_manifest_from_detection_details(
    details: &str,
    source_fragment: &str,
) -> Option<PathBuf> {
    if !details.contains(source_fragment) {
        return None;
    }
    let path = details
        .strip_prefix("Resolved: ")
        .unwrap_or(details)
        .split(" (")
        .next()?
        .trim();
    if path.is_empty() {
        return None;
    }
    let manifest = PathBuf::from(path).join("AppxManifest.xml");
    manifest.is_file().then_some(manifest)
}

#[cfg(target_os = "windows")]
fn claude_desktop_windows_known_versions() -> Vec<String> {
    let mut versions = Vec::new();
    if let Ok(Some(snapshot)) = storage::load_detection_cache() {
        if let Some(tool) = snapshot
            .tools
            .iter()
            .find(|tool| tool.id == "claude-desktop")
        {
            if let Some(version) = tool.version.as_deref() {
                push_claude_desktop_version_candidates(&mut versions, version);
            }
            if let Some(version) = tool.latest_version.as_deref() {
                push_claude_desktop_version_candidates(&mut versions, version);
            }
        }
    }
    if let Some(version) = cached_claude_desktop_latest(Duration::from_millis(0)) {
        push_claude_desktop_version_candidates(&mut versions, &version);
    }
    versions.sort();
    versions.dedup();
    versions
}

#[cfg(target_os = "windows")]
fn push_claude_desktop_version_candidates(versions: &mut Vec<String>, version: &str) {
    let Some(version) = normalized_version_label(version) else {
        return;
    };
    versions.push(version.clone());
    if version.split('.').count() == 3 {
        versions.push(format!("{version}.0"));
    }
}

#[cfg(target_os = "windows")]
fn claude_desktop_windows_known_manifest_candidates(version: &str) -> Vec<PathBuf> {
    let root = PathBuf::from(r"C:\Program Files\WindowsApps");
    vec![
        root.join(format!(
            "Claude_{version}_x64__{CLAUDE_DESKTOP_WINDOWS_PACKAGE_SUFFIX}"
        ))
        .join("AppxManifest.xml"),
        root.join(format!(
            "Anthropic.Claude_{version}_x64__{CLAUDE_DESKTOP_WINDOWS_PACKAGE_SUFFIX}"
        ))
        .join("AppxManifest.xml"),
    ]
}

#[cfg(target_os = "windows")]
fn find_latest_claude_desktop_windows_stale_msix_dir() -> Option<PathBuf> {
    if cached_claude_desktop_windows_msix_package().is_some() {
        return None;
    }
    let root = PathBuf::from(r"C:\Program Files\WindowsApps");
    let mut matches = fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !name.starts_with("Claude_") || !name.contains("_x64__") {
                return None;
            }
            let manifest = path.join("AppxManifest.xml");
            let exe = path.join("app").join("Claude.exe");
            if manifest.is_file() && exe.is_file() {
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| compare_stale_claude_desktop_dirs(right, left));
    matches.into_iter().next()
}

#[cfg(target_os = "windows")]
fn stale_claude_desktop_version(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    name.strip_prefix("Claude_")?
        .split('_')
        .next()
        .map(ToString::to_string)
}

#[cfg(target_os = "windows")]
fn compare_stale_claude_desktop_dirs(left: &Path, right: &Path) -> Ordering {
    compare_versions(
        stale_claude_desktop_version(left).as_deref().unwrap_or("0"),
        stale_claude_desktop_version(right)
            .as_deref()
            .unwrap_or("0"),
    )
}

fn claude_desktop_macos_app_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications/Claude.app")];
    if let Ok(paths) = app_paths() {
        candidates.push(paths.home_dir.join("Applications").join("Claude.app"));
    }
    candidates
}

fn cached_claude_desktop_windows_msix_package() -> Option<package::InstalledMsixPackage> {
    let now = Instant::now();
    let mut cache = claude_desktop_install_cache().lock().unwrap();
    if cache
        .checked_at
        .map(|checked_at| now.duration_since(checked_at) < CLAUDE_DESKTOP_INSTALL_CACHE_TTL)
        .unwrap_or(false)
    {
        return cache.detected.clone();
    }

    let detected = package::detect_first_msix_package(claude_desktop_windows_package_identities());
    cache.detected = detected.clone();
    cache.checked_at = Some(now);
    detected
}

#[derive(Debug, Clone)]
struct NpmOutdatedPackage {
    latest: String,
}

#[derive(Debug, Default)]
struct NpmUpdateCache {
    packages: HashMap<String, NpmOutdatedPackage>,
    checked_at: Option<Instant>,
    in_progress: bool,
}

#[derive(Debug, Default)]
struct WingetUpdateCache {
    packages: HashMap<String, String>,
    checked_at: Option<Instant>,
    in_progress: bool,
}

#[derive(Debug, Default)]
struct ClaudeDesktopUpdateCache {
    version: Option<String>,
    checked_at: Option<Instant>,
    in_progress: bool,
}

#[derive(Debug, Default)]
struct ClaudeDesktopInstallCache {
    detected: Option<package::InstalledMsixPackage>,
    checked_at: Option<Instant>,
}

static NPM_UPDATE_CACHE: OnceLock<Mutex<NpmUpdateCache>> = OnceLock::new();
static WINGET_UPDATE_CACHE: OnceLock<Mutex<WingetUpdateCache>> = OnceLock::new();
static CLAUDE_DESKTOP_UPDATE_CACHE: OnceLock<Mutex<ClaudeDesktopUpdateCache>> = OnceLock::new();
static CLAUDE_DESKTOP_INSTALL_CACHE: OnceLock<Mutex<ClaudeDesktopInstallCache>> = OnceLock::new();

fn annotate_update_status(
    tools: &mut [ToolStatus],
    system: &mut [ToolStatus],
    options: DetectionOptions,
) {
    let npm_outdated =
        cached_npm_global_outdated(options.update_wait_budget(NPM_UPDATE_WAIT_BUDGET));
    let winget_outdated =
        cached_winget_outdated(options.update_wait_budget(WINGET_UPDATE_WAIT_BUDGET));
    let claude_desktop_latest =
        cached_claude_desktop_latest(options.update_wait_budget(CLAUDE_DESKTOP_UPDATE_WAIT_BUDGET));
    let codex_client_latest = codex_client::latest_version_cached(
        options.update_wait_budget(CODEX_CLIENT_UPDATE_WAIT_BUDGET),
    );
    for tool in tools.iter_mut().chain(system.iter_mut()) {
        tool.update_command = update_command_for_tool(&tool.id);
        // Surface the latest Claude Desktop version even when it is not
        // installed, so the page can show what would be installed. The
        // update_available flag only applies to installed tools below.
        if tool.id == "claude-desktop" {
            if let Some(latest) = claude_desktop_latest.as_deref() {
                tool.latest_version = Some(latest.to_string());
                if tool.install_state != InstallState::Installed {
                    continue;
                }
                apply_latest_version(tool, latest);
                continue;
            }
        }
        if tool.id == "codex-app" {
            if let Some(latest) = codex_client_latest.as_deref() {
                tool.latest_version = Some(latest.to_string());
                if tool.install_state != InstallState::Installed {
                    continue;
                }
                apply_latest_version(tool, latest);
                continue;
            }
        }

        if tool.install_state != InstallState::Installed {
            continue;
        }
    }

    apply_package_update_status(tools, system, &npm_outdated, &winget_outdated);
}

fn apply_package_update_status(
    tools: &mut [ToolStatus],
    system: &mut [ToolStatus],
    npm_outdated: &HashMap<String, NpmOutdatedPackage>,
    winget_outdated: &HashMap<String, String>,
) {
    for tool in tools.iter_mut().chain(system.iter_mut()) {
        if tool.install_state != InstallState::Installed {
            continue;
        }

        if let Some(package) = npm_package_for_tool(&tool.id) {
            if let Some(outdated) = npm_outdated.get(package) {
                tool.latest_version = Some(outdated.latest.clone());
                tool.update_available = true;
            }
        }
        if let Some(package_id) = winget_package_for_tool(&tool.id) {
            if let Some(latest) = winget_outdated.get(package_id) {
                tool.latest_version = Some(latest.clone());
                tool.update_available = true;
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ClaudeDesktopLatest {
    version: String,
}

fn read_claude_desktop_latest_version() -> Option<String> {
    let url = if cfg!(target_os = "windows") {
        CLAUDE_DESKTOP_LATEST_WINDOWS_URL
    } else if cfg!(target_os = "macos") {
        CLAUDE_DESKTOP_LATEST_MACOS_URL
    } else {
        return None;
    };
    read_claude_desktop_latest_version_from_url(url)
}

fn claude_desktop_update_cache() -> &'static Mutex<ClaudeDesktopUpdateCache> {
    CLAUDE_DESKTOP_UPDATE_CACHE.get_or_init(|| Mutex::new(ClaudeDesktopUpdateCache::default()))
}

fn claude_desktop_install_cache() -> &'static Mutex<ClaudeDesktopInstallCache> {
    CLAUDE_DESKTOP_INSTALL_CACHE.get_or_init(|| Mutex::new(ClaudeDesktopInstallCache::default()))
}

fn cached_claude_desktop_latest(wait_budget: Duration) -> Option<String> {
    if !cfg!(any(target_os = "windows", target_os = "macos")) {
        return None;
    }
    let should_start = {
        let mut cache = claude_desktop_update_cache().lock().unwrap();
        if cache
            .checked_at
            .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
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
            let version = read_claude_desktop_latest_version();
            let mut cache = claude_desktop_update_cache().lock().unwrap();
            cache.version = version;
            cache.checked_at = Some(Instant::now());
            cache.in_progress = false;
        });
    }

    wait_for_claude_desktop_update_cache(wait_budget)
}

fn wait_for_claude_desktop_update_cache(wait_budget: Duration) -> Option<String> {
    let started_at = Instant::now();
    loop {
        {
            let cache = claude_desktop_update_cache().lock().unwrap();
            if !cache.in_progress
                || cache
                    .checked_at
                    .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
                    .unwrap_or(false)
            {
                return cache.version.clone();
            }
            if started_at.elapsed() >= wait_budget {
                return cache.version.clone();
            }
        }
        thread::sleep(UPDATE_CACHE_POLL_INTERVAL);
    }
}

fn read_claude_desktop_latest_version_from_url(url: &str) -> Option<String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(3000))
        .user_agent("CodeStudio Lite")
        .build()
        .ok()?
        .get(url)
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let latest = response.json::<ClaudeDesktopLatest>().ok()?;
    normalized_version_label(&latest.version)
}

fn apply_latest_version(tool: &mut ToolStatus, latest: &str) {
    tool.latest_version = Some(latest.to_string());
    tool.update_available = tool
        .version
        .as_deref()
        .map(|current| compare_versions(current, latest) == Ordering::Less)
        .unwrap_or(true);
}

fn hermes_version_reports_update_available(version: &str) -> bool {
    version
        .lines()
        .any(|line| line.to_ascii_lowercase().contains("update available"))
}

fn normalized_version_label(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .trim_start_matches('v')
        .split('-')
        .next()
        .unwrap_or("")
        .trim();
    if normalized.is_empty() || !normalized.chars().any(|ch| ch.is_ascii_digit()) {
        None
    } else {
        Some(normalized.to_string())
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

fn npm_update_cache() -> &'static Mutex<NpmUpdateCache> {
    NPM_UPDATE_CACHE.get_or_init(|| Mutex::new(NpmUpdateCache::default()))
}

fn winget_update_cache() -> &'static Mutex<WingetUpdateCache> {
    WINGET_UPDATE_CACHE.get_or_init(|| Mutex::new(WingetUpdateCache::default()))
}

fn cached_npm_global_outdated(wait_budget: Duration) -> HashMap<String, NpmOutdatedPackage> {
    let should_start = {
        let mut cache = npm_update_cache().lock().unwrap();
        if cache
            .checked_at
            .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
            .unwrap_or(false)
        {
            return cache.packages.clone();
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
            let packages = read_npm_global_outdated();
            let mut cache = npm_update_cache().lock().unwrap();
            cache.packages = packages;
            cache.checked_at = Some(Instant::now());
            cache.in_progress = false;
        });
    }

    wait_for_npm_update_cache(wait_budget)
}

fn cached_winget_outdated(wait_budget: Duration) -> HashMap<String, String> {
    if !cfg!(target_os = "windows") {
        return HashMap::new();
    }
    let should_start = {
        let mut cache = winget_update_cache().lock().unwrap();
        if cache
            .checked_at
            .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
            .unwrap_or(false)
        {
            return cache.packages.clone();
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
            let packages = read_winget_outdated();
            let mut cache = winget_update_cache().lock().unwrap();
            cache.packages = packages;
            cache.checked_at = Some(Instant::now());
            cache.in_progress = false;
        });
    }

    wait_for_winget_update_cache(wait_budget)
}

fn wait_for_npm_update_cache(wait_budget: Duration) -> HashMap<String, NpmOutdatedPackage> {
    let started_at = Instant::now();
    loop {
        {
            let cache = npm_update_cache().lock().unwrap();
            if !cache.in_progress
                || cache
                    .checked_at
                    .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
                    .unwrap_or(false)
            {
                return cache.packages.clone();
            }
            if started_at.elapsed() >= wait_budget {
                return cache.packages.clone();
            }
        }
        thread::sleep(UPDATE_CACHE_POLL_INTERVAL);
    }
}

fn wait_for_winget_update_cache(wait_budget: Duration) -> HashMap<String, String> {
    let started_at = Instant::now();
    loop {
        {
            let cache = winget_update_cache().lock().unwrap();
            if !cache.in_progress
                || cache
                    .checked_at
                    .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
                    .unwrap_or(false)
            {
                return cache.packages.clone();
            }
            if started_at.elapsed() >= wait_budget {
                return cache.packages.clone();
            }
        }
        thread::sleep(UPDATE_CACHE_POLL_INTERVAL);
    }
}

fn npm_package_for_tool(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        "codex" => Some("@openai/codex"),
        "claude" => Some("@anthropic-ai/claude-code"),
        "gemini" => Some("@google/gemini-cli"),
        "opencode" => Some("opencode-ai"),
        "openclaw" => Some("openclaw"),
        "pnpm" => Some("pnpm"),
        "npm" => Some("npm"),
        _ => None,
    }
}

fn winget_package_for_tool(tool_id: &str) -> Option<&'static str> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    match tool_id {
        "node" => Some("OpenJS.NodeJS.LTS"),
        "git" => Some("Git.Git"),
        "bun" => Some("Oven-sh.Bun"),
        _ => None,
    }
}

fn update_command_for_tool(tool_id: &str) -> Option<String> {
    match tool_id {
        "codex" => Some(npm_global_update_command("@openai/codex")),
        "codex-vscode" => Some("code --install-extension openai.chatgpt --force".to_string()),
        "claude" => Some(npm_global_update_command("@anthropic-ai/claude-code")),
        "claude-desktop" if cfg!(target_os = "macos") => {
            Some(
                "Download and install the latest Claude Desktop official DMG from downloads.claude.ai"
                    .to_string(),
            )
        }
        "claude-desktop" => Some(CLAUDE_DESKTOP_WINDOWS_UPDATE_COMMAND.to_string()),
        "claude-vscode" => {
            Some("code --install-extension anthropic.claude-code --force".to_string())
        }
        "gemini" => Some(npm_global_update_command("@google/gemini-cli")),
        "gemini-code-assist" => {
            Some("code --install-extension Google.geminicodeassist --force".to_string())
        }
        "opencode" => Some(npm_global_update_command("opencode-ai")),
        "openclaw" => Some(npm_global_update_command("openclaw")),
        "hermes" => Some("hermes update".to_string()),
        "node" if cfg!(target_os = "macos") => Some(
            r#"bash -lc 'set -e; tmp="$(mktemp -d)"; trap '"'"'rm -rf "$tmp"'"'"' EXIT; version="$(curl -fsSL https://nodejs.org/dist/index.json | grep -m 1 '"'"'"lts":"[^"]*"'"'"' | sed -E '"'"'s/.*"version":"([^"]+)".*/\1/'"'"')"; if [ -z "$version" ]; then echo "Unable to resolve latest Node.js LTS version." >&2; exit 1; fi; pkg="$tmp/node-$version.pkg"; curl -fL "https://nodejs.org/dist/$version/node-$version.pkg" -o "$pkg"; sudo installer -pkg "$pkg" -target /'"#.to_string(),
        ),
        "node" if cfg!(target_os = "linux") => Some(
            "bash -lc 'curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash - && sudo apt-get install -y nodejs'"
                .to_string(),
        ),
        "node" => Some(
            "winget upgrade --id OpenJS.NodeJS.LTS --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
                .to_string(),
        ),
        "git" if cfg!(target_os = "macos") => Some("xcode-select --install".to_string()),
        "git" if cfg!(target_os = "linux") => {
            Some("bash -lc 'sudo apt-get update && sudo apt-get install -y git'".to_string())
        }
        "git" => Some(
            "winget upgrade --id Git.Git --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
                .to_string(),
        ),
        "pnpm" => Some(npm_global_update_command("pnpm")),
        "bun" if cfg!(target_os = "macos") => {
            Some("bash -lc 'curl -fsSL https://bun.sh/install | bash'".to_string())
        }
        "bun" if cfg!(target_os = "linux") => {
            Some("bash -lc 'curl -fsSL https://bun.sh/install | bash'".to_string())
        }
        "bun" => Some(
            "winget upgrade --id Oven-sh.Bun --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
                .to_string(),
        ),
        _ => None,
    }
}

fn npm_global_update_command(package: &str) -> String {
    let command = format!("npm install -g {package}@latest");
    if !cfg!(target_os = "macos") {
        return command;
    }
    let prefix = match resolve_command("npm") {
        Some(npm) => npm_global::user_prefix_override_for(&npm),
        None => npm_global::user_prefix(),
    };
    let Some(prefix) = prefix else {
        return command;
    };
    format!("{} {command}", npm_global::shell_prefix_assignment(&prefix))
}

fn read_npm_global_outdated() -> HashMap<String, NpmOutdatedPackage> {
    let Some(npm) = resolve_command("npm") else {
        return HashMap::new();
    };
    let mut command = hidden_command_with_args(&npm, &["outdated", "-g", "--json", "--depth=0"]);
    if npm_global::configure_command_for_global_packages(&mut command, &npm).is_err() {
        return HashMap::new();
    }
    let Some(output) = run_command_with_timeout(command, UPDATE_CHECK_TIMEOUT) else {
        return HashMap::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return HashMap::new();
    }

    let Ok(Value::Object(packages)) = serde_json::from_str::<Value>(&stdout) else {
        return HashMap::new();
    };

    packages
        .into_iter()
        .filter_map(|(package, value)| {
            let latest = value
                .get("latest")
                .or_else(|| value.get("wanted"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|latest| !latest.is_empty())?
                .to_string();
            Some((package, NpmOutdatedPackage { latest }))
        })
        .collect()
}

fn read_npm_global_package_version(package: &str) -> Option<String> {
    npm_global_package_roots()
        .into_iter()
        .filter_map(|root| read_npm_package_version(&root, package))
        .next()
}

fn read_npm_package_version(root: &PathBuf, package: &str) -> Option<String> {
    let manifest = package
        .split('/')
        .fold(root.clone(), |path, segment| path.join(segment))
        .join("package.json");
    let text = fs::read_to_string(manifest).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    value
        .get("version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(ToString::to_string)
}

fn npm_global_package_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(windows) {
        if let Some(app_data) = env::var_os("APPDATA") {
            roots.push(PathBuf::from(app_data).join("npm").join("node_modules"));
        }
        if let Ok(paths) = app_paths() {
            roots.push(
                paths
                    .home_dir
                    .join("AppData")
                    .join("Roaming")
                    .join("npm")
                    .join("node_modules"),
            );
        }
    }
    if let Some(prefix) = env::var_os("NPM_CONFIG_PREFIX") {
        roots.push(PathBuf::from(prefix).join("node_modules"));
    }
    if let Some(npm) = resolve_command("npm") {
        if let Some(root) = npm_global_root_from_command(&npm) {
            roots.push(root);
        }
        let npm_path = PathBuf::from(npm);
        if let Some(parent) = npm_path.parent() {
            roots.push(parent.join("node_modules"));
        }
    }
    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/opt/homebrew/lib/node_modules"));
        roots.push(PathBuf::from("/usr/local/lib/node_modules"));
        if let Ok(paths) = app_paths() {
            roots.push(
                paths
                    .home_dir
                    .join(".npm-global")
                    .join("lib")
                    .join("node_modules"),
            );
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

fn npm_global_root_from_command(npm: &str) -> Option<PathBuf> {
    let command = hidden_command_with_args(npm, &["root", "-g"]);
    let output = run_command_with_timeout(command, Duration::from_millis(1200))?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(PathBuf::from(stdout))
    }
}

fn read_winget_outdated() -> HashMap<String, String> {
    let Some(winget) = resolve_command("winget") else {
        return HashMap::new();
    };
    let output_command = hidden_command_with_args(
        &winget,
        &["upgrade", "--source", "winget", "--disable-interactivity"],
    );
    let Some(output) = run_command_with_timeout(output_command, UPDATE_CHECK_TIMEOUT) else {
        return HashMap::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let package_ids = [
        "Anthropic.Claude",
        "OpenJS.NodeJS.LTS",
        "Git.Git",
        "Oven-sh.Bun",
    ];

    stdout
        .lines()
        .filter_map(|line| {
            let tokens = line.split_whitespace().collect::<Vec<_>>();
            let package_id = package_ids
                .iter()
                .find(|package_id| tokens.iter().any(|token| *token == **package_id))?;
            let index = tokens.iter().position(|token| *token == *package_id)?;
            let latest = tokens.get(index + 2)?;
            Some(((*package_id).to_string(), (*latest).to_string()))
        })
        .collect()
}

fn run_command_with_timeout(
    mut command_builder: std::process::Command,
    timeout: Duration,
) -> Option<std::process::Output> {
    let mut child = command_builder.spawn().ok()?;
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if started_at.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn run_version(
    command: &str,
    args: &[&str],
    output_contains: Option<&str>,
) -> Option<VersionCheck> {
    let mut command_builder = hidden_command_with_args(command, args);
    let mut child = command_builder.spawn().ok()?;
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().ok()?;
                if !output.status.success() {
                    return Some(VersionCheck::Failed);
                }

                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let output_text = if !stdout.is_empty() { &stdout } else { &stderr };

                if let Some(needle) = output_contains {
                    let needle_lower = needle.to_ascii_lowercase();
                    return Some(
                        output_text
                            .lines()
                            .find(|line| line.to_ascii_lowercase().contains(&needle_lower))
                            .map(|line| VersionCheck::Found(line.trim().to_string()))
                            .unwrap_or_else(|| {
                                VersionCheck::NotFound(format!(
                                    "Required marker not found in command output: {needle}"
                                ))
                            }),
                    );
                }

                return Some(if !stdout.is_empty() {
                    VersionCheck::Found(stdout.lines().next().unwrap_or_default().to_string())
                } else if !stderr.is_empty() {
                    VersionCheck::Found(stderr.lines().next().unwrap_or_default().to_string())
                } else {
                    VersionCheck::Found("installed".to_string())
                });
            }
            Ok(None) if started_at.elapsed() >= VERSION_CHECK_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(VersionCheck::TimedOut);
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(VersionCheck::Failed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_extension_tools_are_hidden_without_vscode() {
        let tools = ai_tools_for_environment(false);
        assert!(!tools.iter().any(|tool| tool.id == "codex-vscode"));
        assert!(!tools.iter().any(|tool| tool.id == "claude-vscode"));
        assert!(!tools.iter().any(|tool| tool.id == "gemini-code-assist"));
        assert!(tools.iter().any(|tool| tool.id == "codex"));
        assert!(tools.iter().any(|tool| tool.id == "claude"));
    }

    #[test]
    fn vscode_extension_tools_are_visible_with_vscode() {
        let tools = ai_tools_for_environment(true);
        assert!(tools.iter().any(|tool| tool.id == "codex-vscode"));
        assert!(tools.iter().any(|tool| tool.id == "claude-vscode"));
        assert!(tools.iter().any(|tool| tool.id == "gemini-code-assist"));
    }

    #[test]
    fn linux_platform_does_not_track_codex_desktop_client() {
        assert!(!supports_codex_desktop_client_for_platform("linux"));
        assert!(supports_codex_desktop_client_for_platform("windows"));
        assert!(supports_codex_desktop_client_for_platform("macos"));
    }

    #[test]
    fn linux_platform_does_not_track_claude_desktop_client() {
        assert!(!supports_claude_desktop_client_for_platform("linux"));
        assert!(supports_claude_desktop_client_for_platform("windows"));
        assert!(supports_claude_desktop_client_for_platform("macos"));
    }

    #[test]
    fn claude_desktop_windows_local_candidates_do_not_require_path() {
        let mut candidates = Vec::new();
        let local_app_data = PathBuf::from(r"C:\Users\Dream\AppData\Local");

        push_claude_desktop_windows_local_candidates(&mut candidates, &local_app_data);

        assert!(candidates.contains(
            &local_app_data
                .join("Programs")
                .join("Claude")
                .join("Claude.exe")
        ));
        assert!(candidates.contains(&local_app_data.join("Claude").join("Claude.exe")));
    }

    #[test]
    fn claude_desktop_windows_program_files_candidates_cover_anthropic_folder() {
        let mut candidates = Vec::new();
        let program_files = PathBuf::from(r"C:\Program Files");

        push_claude_desktop_windows_program_files_candidates(&mut candidates, &program_files);

        assert!(candidates.contains(&program_files.join("Claude").join("Claude.exe")));
        assert!(candidates.contains(
            &program_files
                .join("Anthropic")
                .join("Claude")
                .join("Claude.exe")
        ));
    }

    #[test]
    fn claude_desktop_windows_localappdata_scan_finds_direct_exe() {
        let root = std::env::temp_dir().join(format!("cs-lite-scan-direct-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let install = root.join("Claude");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("Claude.exe"), b"stub").unwrap();
        fs::create_dir_all(install.join("resources")).unwrap();
        fs::write(install.join("resources").join("app.asar"), b"asar").unwrap();
        let hit = scan_localappdata_for_claude_exe(&root).expect("should detect direct exe");
        assert_eq!(
            hit.path,
            install.join("Claude.exe").to_string_lossy().to_string()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn newest_squirrel_app_version_dir_picks_highest_version() {
        let root = std::env::temp_dir().join(format!("cs-squirrel-det-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("app-1.13576.0")).unwrap();
        fs::create_dir_all(root.join("app-1.14271.0")).unwrap();
        let picked =
            newest_squirrel_app_version_dir(&root).expect("should find an app-<version> dir");
        assert!(picked.ends_with("app-1.14271.0"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn newest_squirrel_app_version_dir_returns_none_without_app_dirs() {
        let root = std::env::temp_dir().join(format!("cs-nosquirrel-det-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Claude.exe"), b"launcher").unwrap();
        assert_eq!(newest_squirrel_app_version_dir(&root), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_desktop_windows_localappdata_scan_prefers_app_version_over_root_launcher() {
        // Squirrel/electron-builder layout: a tiny root launcher (Claude.exe)
        // next to an app-<version>/Claude.exe real image. The scan must prefer
        // the versioned image so the detected version is the real release
        // label, not "installed".
        let root =
            std::env::temp_dir().join(format!("cs-lite-scan-squirrel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let install = root.join("AnthropicClaude");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("Claude.exe"), b"launcher").unwrap();
        let app_dir = install.join("app-1.14271.0");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(app_dir.join("Claude.exe"), b"real").unwrap();
        fs::create_dir_all(app_dir.join("resources")).unwrap();
        fs::write(app_dir.join("resources").join("app.asar"), b"asar").unwrap();
        let hit = scan_localappdata_for_claude_exe(&root).expect("should detect squirrel install");
        assert_eq!(hit.version, "1.14271.0");
        assert!(hit
            .path
            .replace('\\', "/")
            .ends_with("app-1.14271.0/Claude.exe"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_desktop_windows_localappdata_scan_finds_app_version_layout() {
        let root = std::env::temp_dir().join(format!("cs-lite-scan-appver-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let install = root.join("AnthropicClaude").join("app-1.14271.0");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("Claude.exe"), b"stub").unwrap();
        fs::create_dir_all(install.join("resources")).unwrap();
        fs::write(install.join("resources").join("app.asar"), b"asar").unwrap();
        let hit = scan_localappdata_for_claude_exe(&root).expect("should detect app- layout");
        assert_eq!(hit.version, "1.14271.0");
        assert!(hit.path.ends_with("Claude.exe"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_desktop_windows_localappdata_scan_finds_nested_programs_install() {
        let root = std::env::temp_dir().join(format!("cs-lite-scan-nested-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let install = root.join("Programs").join("Anthropic").join("Claude");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("Claude.exe"), b"stub").unwrap();
        fs::create_dir_all(install.join("resources")).unwrap();
        fs::write(install.join("resources").join("app.asar"), b"asar").unwrap();
        let hit =
            scan_localappdata_for_claude_exe(&root).expect("should detect nested programs install");
        assert_eq!(
            hit.path,
            install.join("Claude.exe").to_string_lossy().to_string()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_desktop_windows_localappdata_scan_skips_orphaned_exe_without_asar() {
        // Simulates a partial uninstall where the NSIS uninstaller removed
        // everything except the patched Claude.exe (hash mismatch). Detection
        // must not report this as a working install.
        let root = std::env::temp_dir().join(format!("cs-lite-scan-orphan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let install = root.join("AnthropicClaude").join("app-1.14271.0");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("Claude.exe"), b"patched").unwrap();
        // No resources/app.asar — this is an orphaned exe
        let hit = scan_localappdata_for_claude_exe(&root);
        assert!(
            hit.is_none(),
            "orphaned exe without asar should not be detected"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_desktop_windows_package_identities_include_current_official_package() {
        assert_eq!(
            claude_desktop_windows_package_identities(),
            &["Claude", "Anthropic.Claude"]
        );
    }

    #[test]
    fn claude_desktop_stale_msix_detection_does_not_mark_tool_installed() {
        let stale = DesktopAppDetection {
            path: r"C:\Program Files\WindowsApps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc".to_string(),
            version: "1.14271.0".to_string(),
            source: "appx-stale",
        };

        let (install_state, install_kind, install_path, version, details) =
            claude_desktop_status_from_detection(Some(&stale), None);

        assert_eq!(install_state, InstallState::Missing);
        assert!(install_kind.is_none());
        assert!(install_path.is_none());
        assert!(version.is_none());
        assert!(details
            .as_deref()
            .unwrap_or_default()
            .contains("MSIX/AppX package files are present but not registered"));
    }

    #[test]
    fn claude_desktop_update_command_uses_platform_official_route() {
        let command = update_command_for_tool("claude-desktop").expect("update command");

        if cfg!(target_os = "macos") {
            assert!(command.contains("official DMG"));
            assert!(command.contains("downloads.claude.ai"));
            assert!(!command.contains("Homebrew"));
        } else {
            assert!(command.contains("claude.ai/api/desktop/win32/x64/msix/latest/redirect"));
            assert!(command.contains("Add-AppxPackage -Path"));
            assert!(
                !command.contains("winget upgrade --id Anthropic.Claude"),
                "Claude Desktop Windows updates must not use deprecated winget routing: {command}"
            );
        }
    }

    #[test]
    fn claude_desktop_macos_candidates_include_app_bundle() {
        let candidates = claude_desktop_macos_app_candidates();

        assert!(candidates.contains(&PathBuf::from("/Applications/Claude.app")));
    }

    #[test]
    fn claude_desktop_macos_app_bundle_status_is_installed() {
        let detected = DesktopAppDetection {
            path: "/Applications/Claude.app".to_string(),
            version: "1.14271.0".to_string(),
            source: "app-bundle",
        };

        let (install_state, install_kind, install_path, version, details) =
            claude_desktop_status_from_detection(Some(&detected), None);

        assert_eq!(install_state, InstallState::Installed);
        assert_eq!(install_kind.as_deref(), Some("msix"));
        assert_eq!(install_path.as_deref(), Some("/Applications/Claude.app"));
        assert_eq!(version.as_deref(), Some("1.14271.0"));
        assert!(details
            .as_deref()
            .unwrap_or_default()
            .contains("app-bundle"));
    }

    #[test]
    fn parses_claude_desktop_latest_json() {
        let latest: ClaudeDesktopLatest =
            serde_json::from_str(r#"{"version":"1.14271.0","hash":"abc"}"#).expect("json");

        assert_eq!(latest.version, "1.14271.0");
    }

    #[test]
    fn claude_desktop_latest_detects_winget_lag() {
        let mut tool = ToolStatus {
            id: "claude-desktop".to_string(),
            name: "Claude Desktop".to_string(),
            category: ToolCategory::AiTool,
            command: "Claude".to_string(),
            path_repair: None,
            version: Some("1.13576.0".to_string()),
            latest_version: None,
            update_available: false,
            update_command: None,
            install_state: InstallState::Installed,
            config_state: ConfigState::Configured,
            config_path: None,
            install_path: None,
            install_command: None,
            details: None,
            install_kind: None,
            running: false,
        };

        apply_latest_version(&mut tool, "1.14271.0");

        assert_eq!(tool.latest_version.as_deref(), Some("1.14271.0"));
        assert!(tool.update_available);
    }

    #[test]
    fn apply_claude_desktop_latest_to_tools_sets_update_when_installed_and_behind() {
        let mut tools = vec![ToolStatus {
            id: "claude-desktop".to_string(),
            name: "Claude Desktop".to_string(),
            category: ToolCategory::AiTool,
            command: "Claude".to_string(),
            path_repair: None,
            version: Some("1.13576.0".to_string()),
            latest_version: None,
            update_available: false,
            update_command: None,
            install_state: InstallState::Installed,
            config_state: ConfigState::Configured,
            config_path: None,
            install_path: None,
            install_command: None,
            details: None,
            install_kind: None,
            running: false,
        }];
        apply_claude_desktop_latest_to_tools(&mut tools, "1.14271.0");
        assert_eq!(tools[0].latest_version.as_deref(), Some("1.14271.0"));
        assert!(tools[0].update_available);
    }

    #[test]
    fn apply_claude_desktop_latest_to_tools_marks_up_to_date_when_current_matches_latest() {
        let mut tools = vec![ToolStatus {
            id: "claude-desktop".to_string(),
            name: "Claude Desktop".to_string(),
            category: ToolCategory::AiTool,
            command: "Claude".to_string(),
            path_repair: None,
            version: Some("1.14271.0".to_string()),
            latest_version: None,
            update_available: false,
            update_command: None,
            install_state: InstallState::Installed,
            config_state: ConfigState::Configured,
            config_path: None,
            install_path: None,
            install_command: None,
            details: None,
            install_kind: None,
            running: false,
        }];
        apply_claude_desktop_latest_to_tools(&mut tools, "1.14271.0");
        assert_eq!(tools[0].latest_version.as_deref(), Some("1.14271.0"));
        assert!(!tools[0].update_available);
    }

    #[test]
    fn apply_claude_desktop_latest_to_tools_surfaces_latest_without_update_when_missing() {
        let mut tools = vec![ToolStatus {
            id: "claude-desktop".to_string(),
            name: "Claude Desktop".to_string(),
            category: ToolCategory::AiTool,
            command: "Claude".to_string(),
            path_repair: None,
            version: None,
            latest_version: None,
            update_available: false,
            update_command: None,
            install_state: InstallState::Missing,
            config_state: ConfigState::Unconfigured,
            config_path: None,
            install_path: None,
            install_command: None,
            details: None,
            install_kind: None,
            running: false,
        }];
        apply_claude_desktop_latest_to_tools(&mut tools, "1.14271.0");
        assert_eq!(tools[0].latest_version.as_deref(), Some("1.14271.0"));
        // Not installed: latest is surfaced for display, update_available stays off.
        assert!(!tools[0].update_available);
    }

    #[test]
    fn apply_codex_client_latest_to_tools_sets_update_when_installed_and_behind() {
        let mut tools = vec![ToolStatus {
            id: "codex-app".to_string(),
            name: "Codex".to_string(),
            category: ToolCategory::AiTool,
            command: "Codex.exe".to_string(),
            path_repair: None,
            version: Some("0.9.0".to_string()),
            latest_version: None,
            update_available: false,
            update_command: None,
            install_state: InstallState::Installed,
            config_state: ConfigState::Configured,
            config_path: None,
            install_path: None,
            install_command: None,
            details: None,
            install_kind: None,
            running: false,
        }];
        apply_codex_client_latest_to_tools(&mut tools, "0.10.0");
        assert_eq!(tools[0].latest_version.as_deref(), Some("0.10.0"));
        assert!(tools[0].update_available);
    }

    #[test]
    fn apply_codex_client_latest_to_tools_marks_up_to_date_when_current_matches_latest() {
        let mut tools = vec![ToolStatus {
            id: "codex-app".to_string(),
            name: "Codex".to_string(),
            category: ToolCategory::AiTool,
            command: "Codex.exe".to_string(),
            path_repair: None,
            version: Some("0.10.0".to_string()),
            latest_version: None,
            update_available: false,
            update_command: None,
            install_state: InstallState::Installed,
            config_state: ConfigState::Configured,
            config_path: None,
            install_path: None,
            install_command: None,
            details: None,
            install_kind: None,
            running: false,
        }];
        apply_codex_client_latest_to_tools(&mut tools, "0.10.0");
        assert_eq!(tools[0].latest_version.as_deref(), Some("0.10.0"));
        assert!(!tools[0].update_available);
    }

    #[test]
    fn apply_codex_client_latest_to_tools_surfaces_latest_without_update_when_missing() {
        let mut tools = vec![ToolStatus {
            id: "codex-app".to_string(),
            name: "Codex".to_string(),
            category: ToolCategory::AiTool,
            command: "Codex.exe".to_string(),
            path_repair: None,
            version: None,
            latest_version: None,
            update_available: false,
            update_command: None,
            install_state: InstallState::Missing,
            config_state: ConfigState::Unconfigured,
            config_path: None,
            install_path: None,
            install_command: None,
            details: None,
            install_kind: None,
            running: false,
        }];
        apply_codex_client_latest_to_tools(&mut tools, "0.10.0");
        assert_eq!(tools[0].latest_version.as_deref(), Some("0.10.0"));
        // Not installed: latest is surfaced for display, update_available stays off.
        assert!(!tools[0].update_available);
    }

    fn installed_tool(id: &str, version: &str) -> ToolStatus {
        ToolStatus {
            id: id.to_string(),
            name: id.to_string(),
            category: ToolCategory::AiTool,
            command: id.to_string(),
            path_repair: None,
            version: Some(version.to_string()),
            latest_version: None,
            update_available: false,
            update_command: None,
            install_state: InstallState::Installed,
            config_state: ConfigState::Configured,
            config_path: None,
            install_path: None,
            install_command: None,
            details: None,
            install_kind: None,
            running: false,
        }
    }

    #[test]
    fn npm_outdated_marks_codex_cli_and_claude_code_updates() {
        let mut tools = vec![
            installed_tool("codex", "0.142.2"),
            installed_tool("claude", "2.1.195"),
        ];
        let npm_outdated = HashMap::from([
            (
                "@openai/codex".to_string(),
                NpmOutdatedPackage {
                    latest: "0.142.5".to_string(),
                },
            ),
            (
                "@anthropic-ai/claude-code".to_string(),
                NpmOutdatedPackage {
                    latest: "2.1.199".to_string(),
                },
            ),
        ]);

        apply_package_update_status(&mut tools, &mut [], &npm_outdated, &HashMap::new());

        assert_eq!(tools[0].latest_version.as_deref(), Some("0.142.5"));
        assert!(tools[0].update_available);
        assert_eq!(tools[1].latest_version.as_deref(), Some("2.1.199"));
        assert!(tools[1].update_available);
    }

    #[test]
    fn hermes_version_output_marks_update_available() {
        assert!(hermes_version_reports_update_available(
            "Hermes Agent v0.16.0\nUpdate available: 2161 commits behind -- run 'hermes update'"
        ));
        assert!(!hermes_version_reports_update_available(
            "Hermes Agent v0.16.0"
        ));
    }

    #[test]
    fn normalizes_msix_four_part_claude_desktop_version_to_three_part() {
        // Get-AppxPackage reports 4-part versions; the release feed is 3-part.
        assert_eq!(
            normalized_claude_desktop_version("1.14271.0.0").as_str(),
            "1.14271.0"
        );
        // A genuine 4-part non-zero build segment is preserved.
        assert_eq!(
            normalized_claude_desktop_version("1.14271.0.5").as_str(),
            "1.14271.0.5"
        );
        // Already-3-part versions are unchanged.
        assert_eq!(
            normalized_claude_desktop_version("1.14271.0").as_str(),
            "1.14271.0"
        );
    }

    #[test]
    fn normalizes_packaged_claude_desktop_version_label() {
        assert_eq!(
            normalized_version_label("v1.14271.0-2").as_deref(),
            Some("1.14271.0")
        );
        assert_eq!(
            normalized_version_label("1.14271.0").as_deref(),
            Some("1.14271.0")
        );
        assert_eq!(normalized_version_label("latest"), None);
    }

    #[test]
    fn detector_update_routes_do_not_use_homebrew_commands() {
        let source = include_str!("detector.rs");
        let brew = ["br", "ew"].concat();

        assert!(!source.contains(&format!("{brew} outdated")));
        assert!(!source.contains(&format!("{brew} upgrade")));
        assert!(source.contains("https://nodejs.org/dist/index.json"));
        assert!(source.contains("https://bun.sh/install"));
        assert!(source.contains("https://hermes-agent.nousresearch.com/install.sh"));
        assert!(source.contains("xcode-select --install"));
    }
}
