use crate::core::app_paths::{app_paths, ensure_dirs};
#[cfg(target_os = "windows")]
use crate::core::detector::{
    claude_desktop_windows_cached_stale_msix_manifest,
    claude_desktop_windows_known_stale_msix_manifest, claude_desktop_windows_package_identities,
    claude_desktop_windows_stale_msix_manifest,
};
#[cfg(target_os = "windows")]
use crate::core::platform::package;
use crate::core::platform::{hidden_command, powershell_exe};
#[cfg(not(target_os = "macos"))]
use crate::core::process_control;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::core::profile;
use crate::core::types::{
    ClaudeDesktopLocalizationProgress, ClaudeDesktopPendingLaunch, InstallTerminalOutput,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
#[cfg(target_os = "macos")]
use std::ffi::{c_void, CString};
use std::fs;
#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tungstenite::{connect, Message};

const CLAUDE_NODE_INSPECT_PORT: u16 = 9229;
const CLAUDE_ZH_INJECTION_RETRY_COUNT: usize = 30;
const CLAUDE_ZH_INJECTION_RETRY_MS: u64 = 500;
const CLAUDE_ZH_BACKGROUND_INJECTION_WAIT_TIMEOUT: Duration = Duration::from_secs(600);
const CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT: u32 = 5;
const CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_MS: u64 = 1_500;
#[cfg(target_os = "macos")]
const MACOS_MAIN_PROCESS_DEBUGGER_WAIT_TIMEOUT: Duration = Duration::from_secs(90);
#[cfg(target_os = "macos")]
const MACOS_MAIN_PROCESS_DEBUGGER_RETRY_MS: u64 = 1_000;
#[cfg(target_os = "windows")]
const WINDOWS_MAIN_PROCESS_DEBUGGER_SCRIPT_TIMEOUT: Duration = Duration::from_secs(30);
const INSTALL_TERMINAL_OUTPUT_EVENT: &str = "install-terminal://output";
#[cfg(target_os = "macos")]
static MACOS_ACCESSIBILITY_PROMPT_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
const MACOS_AX_MAX_CHILDREN_PER_NODE: isize = 80;
/// Per-message read timeout for CDP eval round-trips over the Node inspector.
/// Guards against a stalled inspector response hanging the injection thread
/// forever (the read loop otherwise blocks indefinitely).
const CLAUDE_INSPECTOR_EVAL_TIMEOUT: Duration = Duration::from_secs(15);
const CLAUDE_SHELL_ZH_LOCALE_FILE: &str = "zh-CN.json";
const CLAUDE_ION_ZH_LOCALE_RELATIVE_PATH: &str = "ion-dist/i18n/zh-CN.json";
const CLAUDE_LOCALIZED_LAUNCH_MARKER: &str = "localized-launch.flag";
#[cfg(target_os = "macos")]
const CLAUDE_MACOS_ACCESSIBILITY_PENDING_LAUNCH_MARKER: &str =
    "pending-localized-launch-after-accessibility-grant.json";
const CLAUDE_SHELL_ZH_LOCALE: &str = include_str!("../../resources/claude-desktop/i18n/zh-CN.json");
const CLAUDE_ION_ZH_LOCALE: &str =
    include_str!("../../resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json");
const CLAUDE_ION_DYNAMIC_ZH_LOCALE_RELATIVE_PATH: &str = "ion-dist/i18n/dynamic/zh-CN.json";
const CLAUDE_ION_DYNAMIC_ZH_LOCALE: &str =
    include_str!("../../resources/claude-desktop/i18n/ion-dist/i18n/dynamic/zh-CN.json");

pub fn launch(localize: bool) -> Result<(), String> {
    launch_with_app(localize, None)
}

pub fn launch_with_app(localize: bool, app: Option<tauri::AppHandle>) -> Result<(), String> {
    if !cfg!(any(target_os = "windows", target_os = "macos")) {
        return Err("Claude Desktop launch is only supported on Windows and macOS.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        launch_windows_claude_desktop(localize, app)?;
    }

    #[cfg(target_os = "macos")]
    {
        let _ = app;
        if localize {
            launch_macos_claude_desktop_localized()?;
        } else {
            launch_macos_claude_desktop_plain_restart()?;
        }
    }

    Ok(())
}

pub fn take_pending_claude_desktop_launch_after_restart(
) -> Result<Option<ClaudeDesktopPendingLaunch>, String> {
    #[cfg(target_os = "macos")]
    {
        return take_macos_accessibility_pending_launch_marker();
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

pub fn restart_claude_desktop_after_accessibility_grant(
    app: tauri::AppHandle,
    localize: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        write_macos_accessibility_pending_launch_marker(localize)?;
        append_macos_debugger_log(format!(
            "User confirmed Accessibility grant; restarting CodeStudio Lite to resume Claude launch. {}",
            macos_accessibility_identity_summary()
        ));
        app.request_restart();
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        let _ = localize;
        Ok(())
    }
}

pub fn base_launch_command(tool_id: &str, fallback: &str) -> String {
    if tool_id == "claude-desktop" && cfg!(target_os = "windows") {
        return ensure_patch_files()
            .map(|patch_dir| powershell_file_command(&patch_dir.join("launch-claude.ps1")))
            .unwrap_or_else(|_| fallback.to_string());
    }
    fallback.to_string()
}

pub fn patched_launch_command(
    tool_id: &str,
    command: &str,
    localize: bool,
) -> Result<String, String> {
    if !localize || tool_id != "claude-desktop" {
        return Ok(command.to_string());
    }
    if !cfg!(any(target_os = "windows", target_os = "macos")) {
        return Err(
            "Claude Desktop localization launch is only supported on Windows and macOS."
                .to_string(),
        );
    }
    if cfg!(target_os = "windows") {
        let patch_dir = ensure_patch_files()?;
        Ok(powershell_file_command(
            &patch_dir.join("launch-claude-zh.ps1"),
        ))
    } else {
        let patch_dir = ensure_patch_files()?;
        write_localized_launch_marker()?;
        Ok(sh_file_command(
            &patch_dir.join("launch-claude-macos-zh.sh"),
        ))
    }
}

/// Prepare Claude Desktop localization launch support without modifying the
/// installed Claude app. Both Windows and macOS use Claude's official
/// Developer Mode / "Enable Main Process Debugger" route, then inject at
/// runtime through the opened Node inspector.
pub fn ensure_localization_patch() -> Result<(), String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        ensure_patch_files()?;
        return ensure_claude_desktop_developer_mode();
    }

    #[allow(unreachable_code)]
    Err("Claude Desktop localization is only supported on Windows and macOS.".to_string())
}

pub fn ensure_localized_launch_prerequisites() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        ensure_macos_accessibility_trusted_for_localized_launch()?;
    }
    Ok(())
}

pub fn spawn_localization_injector(app: tauri::AppHandle, session_id: String) {
    thread::spawn(move || {
        let _manual_debugger_activation_fallback = "manualDebuggerActivationFallback";
        if cfg!(target_os = "windows") {
            emit_terminal(
                &app,
                &session_id,
                "[claude-zh] requesting Claude main process debugger; manual enable is still accepted while waiting...\r\n",
            );
        } else if cfg!(target_os = "macos") {
            emit_terminal(
                &app,
                &session_id,
                "[claude-zh] ensuring Claude main process debugger is enabled...\r\n",
            );
        } else {
            emit_terminal(
                &app,
                &session_id,
                "[claude-zh] waiting for Claude DevTools endpoint...\r\n",
            );
        }
        match retry_localization_after_background_debugger_request() {
            Ok(count) => emit_terminal(
                &app,
                &session_id,
                &format!("[claude-zh] injected {count} page(s).\r\n"),
            ),
            Err(err) => emit_terminal(
                &app,
                &session_id,
                &format!("[claude-zh] injection failed: {err}\r\n"),
            ),
        }
    });
}

pub fn spawn_silent_localization_injector() {
    spawn_silent_localization_injector_with_app(None);
}

pub fn spawn_silent_localization_injector_with_app(app: Option<tauri::AppHandle>) {
    thread::spawn(move || {
        run_background_localization_retry_loop(app);
    });
}

fn emit_localization_progress(
    app: Option<&tauri::AppHandle>,
    phase: &str,
    message: &str,
    attempt: u32,
    max_attempts: u32,
    done: bool,
    success: bool,
    attached: Option<usize>,
    error: Option<String>,
) {
    let Some(app) = app else {
        return;
    };
    let _ = app.emit(
        "claude-desktop://localization-progress",
        ClaudeDesktopLocalizationProgress {
            phase: phase.to_string(),
            message: message.to_string(),
            attempt,
            max_attempts,
            done,
            success,
            attached,
            error,
        },
    );
}

fn run_background_localization_retry_loop(app: Option<tauri::AppHandle>) {
    let _manual_debugger_activation_fallback = "manualDebuggerActivationFallback";
    let mut last_error = "Claude Desktop localization did not start.".to_string();
    for attempt in 1..=CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT {
        emit_localization_progress(
            app.as_ref(),
            "debugger",
            "claudeDesktop.localizationDebugger",
            attempt,
            CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT,
            false,
            false,
            None,
            None,
        );

        let debugger_result = request_claude_main_process_debugger_for_background_retry();

        match debugger_result {
            Ok(()) => {
                emit_localization_progress(
                    app.as_ref(),
                    "injecting",
                    "claudeDesktop.localizationInjecting",
                    attempt,
                    CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT,
                    false,
                    false,
                    None,
                    None,
                );
                match retry_inject_localization_until(CLAUDE_ZH_BACKGROUND_INJECTION_WAIT_TIMEOUT) {
                    Ok(attached) => {
                        emit_localization_progress(
                            app.as_ref(),
                            "done",
                            "claudeDesktop.localizationDone",
                            attempt,
                            CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT,
                            true,
                            true,
                            Some(attached),
                            None,
                        );
                        return;
                    }
                    Err(err) => {
                        last_error = err;
                    }
                }
            }
            Err(err) => {
                last_error = err;
            }
        }

        if attempt < CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT {
            thread::sleep(Duration::from_millis(
                CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_MS,
            ));
        }
    }

    emit_localization_progress(
        app.as_ref(),
        "failed",
        "claudeDesktop.localizationFailed",
        CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT,
        CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT,
        true,
        false,
        None,
        Some(last_error),
    );
}

fn retry_localization_after_background_debugger_request() -> Result<usize, String> {
    #[cfg(target_os = "windows")]
    {
        let _manual_debugger_activation_fallback = "manualDebuggerActivationFallback";
        return run_background_localization_retry_loop_for_terminal();
    }

    #[cfg(target_os = "macos")]
    {
        enable_macos_claude_main_process_debugger()?;
        return retry_inject_localization();
    }

    #[allow(unreachable_code)]
    retry_inject_localization()
}

#[cfg(target_os = "windows")]
fn run_background_localization_retry_loop_for_terminal() -> Result<usize, String> {
    let mut last_error = "Claude Desktop localization did not start.".to_string();
    for attempt in 1..=CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT {
        match request_claude_main_process_debugger_for_background_retry() {
            Ok(()) => {
                match retry_inject_localization_until(CLAUDE_ZH_BACKGROUND_INJECTION_WAIT_TIMEOUT) {
                    Ok(attached) => return Ok(attached),
                    Err(err) => last_error = err,
                }
            }
            Err(err) => last_error = err,
        }

        if attempt < CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT {
            thread::sleep(Duration::from_millis(
                CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_MS,
            ));
        }
    }
    Err(last_error)
}

fn request_claude_main_process_debugger_for_background_retry() -> Result<(), String> {
    if claude_node_inspector_available() {
        return Ok(());
    }
    request_claude_main_process_debugger_once()?;
    if wait_for_claude_node_inspector() {
        Ok(())
    } else {
        Err(
            "Claude main process debugger request finished, but no Node inspector endpoint opened yet."
                .to_string(),
        )
    }
}

fn request_claude_main_process_debugger_once() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return request_windows_claude_main_process_debugger_once();
    }

    #[cfg(target_os = "macos")]
    {
        return request_macos_claude_main_process_debugger_once();
    }

    #[allow(unreachable_code)]
    Ok(())
}

fn ensure_patch_files() -> Result<PathBuf, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    let patch_dir = paths.config_dir.join("claude-desktop-patch");
    fs::create_dir_all(&patch_dir).map_err(|err| err.to_string())?;
    write_claude_locale_payloads(&patch_dir)?;
    write_if_changed(
        &patch_dir.join("translation-runtime.js"),
        TRANSLATION_RUNTIME,
    )?;
    write_if_changed(
        &patch_dir.join("launch-claude.ps1"),
        &windows_launch_script(false),
    )?;
    write_if_changed(
        &patch_dir.join("launch-claude-zh.ps1"),
        &windows_launch_script(true),
    )?;
    if cfg!(target_os = "macos") {
        write_if_changed(
            &patch_dir.join("launch-claude-macos-zh.sh"),
            &macos_localized_launch_script(),
        )?;
    }
    Ok(patch_dir)
}

fn localized_launch_marker_path() -> Result<PathBuf, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    Ok(paths
        .config_dir
        .join("claude-desktop-patch")
        .join(CLAUDE_LOCALIZED_LAUNCH_MARKER))
}

#[cfg(target_os = "macos")]
fn macos_accessibility_pending_launch_marker_path() -> Result<PathBuf, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    Ok(paths
        .config_dir
        .join("claude-desktop-patch")
        .join(CLAUDE_MACOS_ACCESSIBILITY_PENDING_LAUNCH_MARKER))
}

#[cfg(target_os = "macos")]
fn write_macos_accessibility_pending_launch_marker(localize: bool) -> Result<(), String> {
    let path = macos_accessibility_pending_launch_marker_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create {}: {err}", parent.display()))?;
    }
    let pending = ClaudeDesktopPendingLaunch {
        action: "launch".to_string(),
        localize,
        requested_at: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    };
    let content = serde_json::to_string_pretty(&pending)
        .map_err(|err| format!("Failed to serialize Claude Desktop pending launch: {err}"))?;
    fs::write(&path, content).map_err(|err| format!("Failed to write {}: {err}", path.display()))
}

#[cfg(target_os = "macos")]
fn take_macos_accessibility_pending_launch_marker(
) -> Result<Option<ClaudeDesktopPendingLaunch>, String> {
    let path = macos_accessibility_pending_launch_marker_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read {}: {err}", path.display()))?;
    let pending = serde_json::from_str::<ClaudeDesktopPendingLaunch>(&content)
        .map_err(|err| format!("Failed to parse {}: {err}", path.display()))?;
    if let Err(err) = fs::remove_file(&path) {
        append_macos_debugger_log(format!(
            "WARN: failed to remove Accessibility pending launch marker {}: {err}",
            path.display()
        ));
    };
    Ok(Some(pending))
}

fn write_localized_launch_marker() -> Result<(), String> {
    let path = localized_launch_marker_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create {}: {err}", parent.display()))?;
    }
    fs::write(&path, "zh-CN").map_err(|err| format!("Failed to write {}: {err}", path.display()))
}

fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    if fs::read_to_string(path)
        .map(|current| current == content)
        .unwrap_or(false)
    {
        return Ok(());
    }
    fs::write(path, content).map_err(|err| format!("Failed to write {}: {err}", path.display()))
}

fn powershell_file_command(path: &Path) -> String {
    let powershell = powershell_exe();
    format!(
        "{} -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File {}",
        windows_shell_quote(&powershell.to_string_lossy()),
        windows_shell_quote(&path.to_string_lossy())
    )
}

fn sh_file_command(path: &Path) -> String {
    format!("sh {}", sh_single_quote(&path.to_string_lossy()))
}

#[cfg(target_os = "windows")]
fn launch_windows_claude_desktop(
    localize: bool,
    app: Option<tauri::AppHandle>,
) -> Result<Option<u32>, String> {
    let args = claude_launch_args(localize);
    close_existing_claude_for_localized_launch()?;
    if localize {
        ensure_patch_files()?;
        ensure_claude_desktop_developer_mode()?;
        write_localized_launch_marker()?;
        if package::detect_first_msix_package(claude_desktop_windows_package_identities()).is_some()
        {
            launch_windows_claude_msix(&args)?;
        } else if let Some(exe) = find_windows_claude_exe() {
            launch_windows_claude_exe(exe, &args)?;
        } else {
            launch_windows_claude_msix(&args)?;
        }
        emit_localization_progress(
            app.as_ref(),
            "launching",
            "claudeDesktop.localizationLaunching",
            0,
            CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT,
            false,
            false,
            None,
            None,
        );
        spawn_silent_localization_injector_with_app(app);
        return Ok(None);
    }

    if package::detect_first_msix_package(claude_desktop_windows_package_identities()).is_some() {
        return launch_windows_claude_msix(&args);
    }

    if let Some(exe) = find_windows_claude_exe() {
        return launch_windows_claude_exe(exe, &args);
    }

    launch_windows_claude_msix(&args)
}

#[cfg(target_os = "windows")]
fn launch_windows_claude_msix(args: &[String]) -> Result<Option<u32>, String> {
    repair_claude_msix_registration()?;
    package::launch_first_msix_package_with_args(claude_desktop_windows_package_identities(), args)
        .map(|pid| (pid > 0).then_some(pid))
        .map_err(|err| format!("Failed to launch Claude Desktop MSIX package: {err}"))
}

#[cfg(target_os = "windows")]
fn repair_claude_msix_registration() -> Result<(), String> {
    if package::detect_first_msix_package(claude_desktop_windows_package_identities()).is_some() {
        return Ok(());
    }
    let Some(manifest) = claude_desktop_windows_stale_msix_manifest()
        .or_else(claude_desktop_windows_cached_stale_msix_manifest)
        .or_else(claude_desktop_windows_known_stale_msix_manifest)
    else {
        return Ok(());
    };
    package::register_msix_manifest(&manifest)
        .map_err(|err| format!("Failed to repair Claude Desktop MSIX registration: {err}"))
}

#[cfg(target_os = "windows")]
fn launch_windows_claude_exe(exe: PathBuf, args: &[String]) -> Result<Option<u32>, String> {
    let mut command = hidden_command(&exe);
    command.args(args);
    if let Some(parent) = exe.parent() {
        command.current_dir(parent);
    }
    command
        .spawn()
        .map(|child| Some(child.id()))
        .map_err(|err| format!("Failed to launch Claude Desktop executable: {err}"))
}

#[cfg(not(target_os = "macos"))]
fn close_existing_claude_for_localized_launch() -> Result<(), String> {
    process_control::close_processes("Claude Desktop", &["Claude"], &[], None, 3)
        .map(|_| ())
        .map_err(|err| {
            err.replace(
                "the update was not continued",
                "localized launch was not continued",
            )
        })
}

#[cfg(target_os = "macos")]
fn close_existing_claude_for_localized_launch() -> Result<(), String> {
    let pids = macos_claude_process_ids()?;
    if pids.is_empty() {
        return Ok(());
    }

    for pid in &pids {
        let _ = hidden_command("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
    }
    wait_for_macos_pids_to_exit(&pids, Duration::from_secs(3));

    let remaining_after_term = pids
        .iter()
        .copied()
        .filter(|pid| macos_pid_alive(*pid))
        .collect::<Vec<_>>();
    for pid in &remaining_after_term {
        hidden_command("kill")
            .args(["-KILL", &pid.to_string()])
            .output()
            .map_err(|err| format!("Failed to force-close Claude Desktop: {err}"))?;
    }
    wait_for_macos_pids_to_exit(&remaining_after_term, Duration::from_millis(500));

    if pids.iter().any(|pid| macos_pid_alive(*pid)) {
        return Err(
            "Claude Desktop is still running; localized launch was not continued.".to_string(),
        );
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn wait_for_macos_pids_to_exit(pids: &[u32], timeout: Duration) {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if pids.iter().all(|pid| !macos_pid_alive(*pid)) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(target_os = "macos")]
fn macos_pid_alive(pid: u32) -> bool {
    hidden_command("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(any(target_os = "windows", test))]
fn claude_launch_args(_localize: bool) -> Vec<String> {
    Vec::new()
}

#[cfg(target_os = "macos")]
fn launch_macos_claude_desktop_localized() -> Result<(), String> {
    ensure_patch_files()?;
    ensure_claude_desktop_developer_mode()?;
    ensure_macos_accessibility_trusted_for_localized_launch()?;
    write_localized_launch_marker()?;
    close_existing_claude_for_localized_launch()?;
    hidden_command("open")
        .args(["-a", "Claude"])
        .status()
        .map_err(|err| format!("Failed to launch Claude Desktop: {err}"))
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err("Failed to launch Claude Desktop.".to_string())
            }
        })?;
    spawn_silent_localization_injector();
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn ensure_claude_desktop_developer_mode() -> Result<(), String> {
    profile::ensure_claude_desktop_developer_mode()
        .map(|_| ())
        .map_err(|err| format!("Failed to enable Claude Desktop developer mode: {err}"))
}

#[cfg(target_os = "windows")]
fn request_windows_claude_main_process_debugger_once() -> Result<(), String> {
    if claude_node_inspector_available() {
        return Ok(());
    }

    let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class CslClaudeWin32 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr extraData);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr SetActiveWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr SetFocus(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
'@

[CslClaudeWin32]::SetProcessDPIAware() | Out-Null
$WM_CLOSE = 0x0010
$SW_SHOW = 5
$SW_RESTORE = 9
$HWND_TOPMOST = [IntPtr](-1)
$HWND_NOTOPMOST = [IntPtr](-2)
$SWP_NOSIZE = 0x0001
$SWP_NOMOVE = 0x0002
$SWP_SHOWWINDOW = 0x0040
$script:claudeDebuggerLogPath = $null
try {
  $patchRoot = Join-Path $env:USERPROFILE '.codestudio-lite\claude-desktop-patch'
  New-Item -ItemType Directory -Force -Path $patchRoot | Out-Null
  $script:claudeDebuggerLogPath = Join-Path $patchRoot 'windows-main-debugger.log'
} catch {}

function Write-ClaudeDebuggerLog([string]$message) {
  if (-not $script:claudeDebuggerLogPath) { return }
  try {
    $timestamp = [DateTime]::Now.ToString('yyyy-MM-dd HH:mm:ss.fff')
    Add-Content -LiteralPath $script:claudeDebuggerLogPath -Encoding UTF8 -Value "[$timestamp] $message"
  } catch {}
}

function Format-ClaudeElementForLog($element) {
  if (-not $element) { return '<null>' }
  try {
    $patterns = [string]::Join(',', @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName }))
    return "name=[$($element.Current.Name)] control=[$($element.Current.ControlType.ProgrammaticName)] class=[$($element.Current.ClassName)] patterns=[$patterns]"
  } catch {
    return '<stale element>'
  }
}

function Wait-ClaudeCondition([int]$attempts, [int]$delayMs, [scriptblock]$condition) {
  for ($attempt = 0; $attempt -lt $attempts; $attempt++) {
    $result = & $condition
    if ($result) { return $result }
    Start-Sleep -Milliseconds $delayMs
  }
  return $null
}

function Start-ClaudeWindowsApp {
  Write-ClaudeDebuggerLog 'Starting or activating Claude Windows app.'
  $pkgNames = @('Claude', 'Anthropic.Claude')
  $pkg = Get-AppxPackage -ErrorAction SilentlyContinue |
    Where-Object { $pkgNames -contains $_.Name -or $_.PackageFullName -match 'Claude' } |
    Sort-Object -Property Version -Descending |
    Select-Object -First 1
  if (-not $pkg) {
    Write-ClaudeDebuggerLog 'No registered Claude AppX package found for activation.'
    return
  }

  $bang = [char]33
  $packagePrefix = $pkg.PackageFamilyName + $bang
  $app = Get-StartApps |
    Where-Object { $_.AppID.StartsWith($packagePrefix) -or $_.Name -eq 'Claude' } |
    Select-Object -First 1
  $appId = if ($app) { $app.AppID } else { $packagePrefix + 'Claude' }
  $target = 'shell:AppsFolder\' + $appId
  Write-ClaudeDebuggerLog "Activating Claude app identity [$appId]."
  Start-Process -FilePath $target
}

function Get-ClaudeProcessMap {
  $map = @{}
  foreach ($proc in (Get-Process -Name 'claude' -ErrorAction SilentlyContinue)) {
    try { $map[[int]$proc.Id] = $proc } catch {}
  }
  $map
}

function Build-ClaudeWindowCandidate([IntPtr]$hWnd, $proc) {
  if (-not $proc -or $hWnd -eq [IntPtr]::Zero) { return $null }

  $titleBuilder = New-Object System.Text.StringBuilder 512
  [CslClaudeWin32]::GetWindowText($hWnd, $titleBuilder, $titleBuilder.Capacity) | Out-Null
  $classBuilder = New-Object System.Text.StringBuilder 256
  [CslClaudeWin32]::GetClassName($hWnd, $classBuilder, $classBuilder.Capacity) | Out-Null
  $rect = New-Object CslClaudeWin32+RECT
  [CslClaudeWin32]::GetWindowRect($hWnd, [ref]$rect) | Out-Null

  $width = $rect.Right - $rect.Left
  $height = $rect.Bottom - $rect.Top
  $isIconic = [CslClaudeWin32]::IsIconic($hWnd)
  if (-not $isIconic -and ($width -lt 320 -or $height -lt 240)) { return $null }

  $title = $titleBuilder.ToString()
  $isInspectorPrompt = Test-ClaudeInspectorPromptCandidate $title $width $height
  $titleScore = if ($title -match 'Claude|chat') { 8 } elseif ($title.Length -gt 0) { 2 } else { 0 }

  $path = ''
  try { $path = $proc.Path } catch { $path = '' }
  if ($path -and $path.IndexOf('Claude', [System.StringComparison]::OrdinalIgnoreCase) -lt 0) {
    return $null
  }

  $className = $classBuilder.ToString()
  if ($className -ne 'Chrome_WidgetWin_1') { return $null }

  [pscustomobject]@{
    Hwnd = $hWnd
    ProcessId = [int]$proc.Id
    Title = $title
    ClassName = $className
    Visible = [CslClaudeWin32]::IsWindowVisible($hWnd)
    Iconic = $isIconic
    IsInspectorPrompt = $isInspectorPrompt
    TitleScore = $titleScore
    Width = $width
    Height = $height
    Area = $width * $height
  }
}

function Get-ClaudeMainWindowFromProcessHandles($claudeProcesses) {
  $candidates = @()
  foreach ($proc in $claudeProcesses.Values) {
    try {
      $hWnd = [IntPtr]$proc.MainWindowHandle
      if ($hWnd -eq [IntPtr]::Zero) { continue }
      $candidate = Build-ClaudeWindowCandidate $hWnd $proc
      if ($candidate -and -not $candidate.IsInspectorPrompt) { $candidates += $candidate }
    } catch {}
  }
  $selected = $candidates |
    Sort-Object -Property @{ Expression = { if ($_.Visible) { 1 } else { 0 } }; Descending = $true },
                          @{ Expression = { $_.TitleScore }; Descending = $true },
                          @{ Expression = { $_.Area }; Descending = $true },
                          @{ Expression = { $_.ProcessId }; Descending = $true } |
    Select-Object -First 1
  if ($selected) {
    Write-ClaudeDebuggerLog "Selected Claude window from process handle hwnd=[$($selected.Hwnd)] pid=[$($selected.ProcessId)] title=[$($selected.Title)] size=[$($selected.Width)x$($selected.Height)]."
  }
  $selected
}

function Get-ClaudeMainWindow {
  $windows = New-Object System.Collections.Generic.List[object]
  $claudeProcesses = Get-ClaudeProcessMap
  if ($claudeProcesses.Count -eq 0) { return $null }
  $candidate = Get-ClaudeMainWindowFromProcessHandles $claudeProcesses
  if ($candidate) { return $candidate }
  [CslClaudeWin32]::EnumWindows({
    param([IntPtr]$hWnd, [IntPtr]$extraData)
    $processId = [uint32]0
    [CslClaudeWin32]::GetWindowThreadProcessId($hWnd, [ref]$processId) | Out-Null
    $proc = $claudeProcesses[[int]$processId]
    if (-not $proc) { return $true }
    $candidate = Build-ClaudeWindowCandidate $hWnd $proc
    if ($candidate) { $windows.Add($candidate) | Out-Null }
    return $true
  }, [IntPtr]::Zero) | Out-Null

  $mainWindows = @($windows | Where-Object { -not $_.IsInspectorPrompt })
  if ($mainWindows.Count -eq 0) { $mainWindows = @($windows) }
  $selected = $mainWindows |
    Sort-Object -Property @{ Expression = { if ($_.Visible) { 1 } else { 0 } }; Descending = $true },
                          @{ Expression = { $_.TitleScore }; Descending = $true },
                          @{ Expression = { $_.Area }; Descending = $true },
                          @{ Expression = { $_.ProcessId }; Descending = $true } |
    Select-Object -First 1
  if ($selected) {
    Write-ClaudeDebuggerLog "Selected Claude window hwnd=[$($selected.Hwnd)] pid=[$($selected.ProcessId)] title=[$($selected.Title)] size=[$($selected.Width)x$($selected.Height)]."
  }
  $selected
}

function Activate-ClaudeMainWindow($window) {
  if (-not $window -or $window.Hwnd -eq [IntPtr]::Zero) { return $window }

  try {
    $isIconic = [CslClaudeWin32]::IsIconic($window.Hwnd)
    if ($isIconic) {
      Write-ClaudeDebuggerLog "Restoring minimized Claude window hwnd=[$($window.Hwnd)]."
      [CslClaudeWin32]::ShowWindow($window.Hwnd, $SW_RESTORE) | Out-Null
    } else {
      [CslClaudeWin32]::ShowWindow($window.Hwnd, $SW_SHOW) | Out-Null
    }

    $flags = [uint32]($SWP_NOMOVE -bor $SWP_NOSIZE -bor $SWP_SHOWWINDOW)
    [CslClaudeWin32]::SetWindowPos($window.Hwnd, $HWND_TOPMOST, 0, 0, 0, 0, $flags) | Out-Null
    [CslClaudeWin32]::SetWindowPos($window.Hwnd, $HWND_NOTOPMOST, 0, 0, 0, 0, $flags) | Out-Null

    $targetPid = [uint32]0
    $targetThread = [CslClaudeWin32]::GetWindowThreadProcessId($window.Hwnd, [ref]$targetPid)
    $currentThread = [CslClaudeWin32]::GetCurrentThreadId()
    $foreground = [CslClaudeWin32]::GetForegroundWindow()
    $foregroundPid = [uint32]0
    $foregroundThread = if ($foreground -ne [IntPtr]::Zero) {
      [CslClaudeWin32]::GetWindowThreadProcessId($foreground, [ref]$foregroundPid)
    } else {
      [uint32]0
    }

    $attachedTarget = $false
    $attachedForeground = $false
    try {
      if ($targetThread -ne 0 -and $targetThread -ne $currentThread) {
        $attachedTarget = [CslClaudeWin32]::AttachThreadInput($currentThread, $targetThread, $true)
      }
      if ($foregroundThread -ne 0 -and
          $foregroundThread -ne $currentThread -and
          $foregroundThread -ne $targetThread) {
        $attachedForeground = [CslClaudeWin32]::AttachThreadInput($currentThread, $foregroundThread, $true)
      }
      [CslClaudeWin32]::BringWindowToTop($window.Hwnd) | Out-Null
      [CslClaudeWin32]::SetActiveWindow($window.Hwnd) | Out-Null
      [CslClaudeWin32]::SetFocus($window.Hwnd) | Out-Null
      [CslClaudeWin32]::SetForegroundWindow($window.Hwnd) | Out-Null
    } finally {
      if ($attachedForeground) {
        [CslClaudeWin32]::AttachThreadInput($currentThread, $foregroundThread, $false) | Out-Null
      }
      if ($attachedTarget) {
        [CslClaudeWin32]::AttachThreadInput($currentThread, $targetThread, $false) | Out-Null
      }
    }

    $proc = Get-Process -Id ([int]$window.ProcessId) -ErrorAction SilentlyContinue
    $refreshed = Build-ClaudeWindowCandidate $window.Hwnd $proc
    if ($refreshed) { $window = $refreshed }

    Wait-ClaudeCondition 20 100 {
      try {
        if ([CslClaudeWin32]::IsIconic($window.Hwnd)) { return $null }
        $root = [System.Windows.Automation.AutomationElement]::FromHandle($window.Hwnd)
        if (-not $root) { return $null }
        $rect = $root.Current.BoundingRectangle
        if ($root.Current.IsOffscreen -or $rect.IsEmpty -or $rect.Width -lt 320 -or $rect.Height -lt 240) {
          return $null
        }
        return $true
      } catch {
        return $null
      }
    } | Out-Null

    Write-ClaudeDebuggerLog "Activated Claude window hwnd=[$($window.Hwnd)] iconic=[$([CslClaudeWin32]::IsIconic($window.Hwnd))]."
  } catch {
    Write-ClaudeDebuggerLog "Ignoring Claude window activation failure: $($_.Exception.Message)"
  }

  $window
}

function Test-ClaudeInspectorPromptCandidate([string]$title, [int]$width, [int]$height) {
  return $title -match 'Inspector|Debugger|DevTools|Main Process|调试|偵錯|检查|檢查' -or
    ($title.Length -eq 0 -and
      $width -ge 480 -and $width -le 1200 -and
      $height -ge 360 -and $height -le 900)
}

function Test-ClaudeInspectorWindowClass([string]$className) {
  return $className -like 'Chrome_WidgetWin_*' -or $className -eq '#32770'
}

function Invoke-Element($element) {
  $invokePattern = $null
  if ($element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$invokePattern)) {
    try {
      $invokePattern.Invoke()
      return $true
    } catch {}
  }

  $expandPattern = $null
  if ($element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$expandPattern)) {
    try {
      if ($expandPattern.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Expanded) {
        $expandPattern.Expand()
      }
      return $true
    } catch {}
  }

  $togglePattern = $null
  if ($element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$togglePattern)) {
    try {
      $togglePattern.Toggle()
      return $true
    } catch {}
  }

  return $false
}

function Invoke-ClaudeElementDefaultAction($element) {
  $legacyAutomationPattern = $null
  try {
    $legacyAutomationPattern = $element.GetSupportedPatterns() |
      Where-Object { $_.ProgrammaticName -eq 'LegacyIAccessiblePatternIdentifiers.Pattern' } |
      Select-Object -First 1
  } catch {
    $legacyAutomationPattern = $null
  }
  if (-not $legacyAutomationPattern) { return $false }

  $legacyPattern = $null
  if ($element.TryGetCurrentPattern($legacyAutomationPattern, [ref]$legacyPattern)) {
    try {
      $legacyPattern.DoDefaultAction()
      return $true
    } catch {}
  }
  return $false
}

function Get-ClaudeAutomationRoots($window) {
  $roots = New-Object System.Collections.Generic.List[object]
  [CslClaudeWin32]::EnumWindows({
    param([IntPtr]$hWnd, [IntPtr]$extraData)
    $processId = [uint32]0
    [CslClaudeWin32]::GetWindowThreadProcessId($hWnd, [ref]$processId) | Out-Null
    if ([int]$processId -ne [int]$window.ProcessId) { return $true }

    $classBuilder = New-Object System.Text.StringBuilder 256
    [CslClaudeWin32]::GetClassName($hWnd, $classBuilder, $classBuilder.Capacity) | Out-Null
    $className = $classBuilder.ToString()
    if ($className -notlike 'Chrome_WidgetWin_*') { return $true }

    try {
      $root = [System.Windows.Automation.AutomationElement]::FromHandle($hWnd)
      if ($root) {
        $roots.Add([pscustomobject]@{
          Hwnd = $hWnd
          Root = $root
          IsMainWindow = $hWnd -eq $window.Hwnd
        }) | Out-Null
      }
    } catch {}
    return $true
  }, [IntPtr]::Zero) | Out-Null

  $roots |
    Sort-Object -Property @{ Expression = { if ($_.IsMainWindow) { 1 } else { 0 } }; Descending = $true }
}

function Move-ClaudeMenuPopupsOffscreen($window) {
  $moved = 0
  [CslClaudeWin32]::EnumWindows({
    param([IntPtr]$hWnd, [IntPtr]$extraData)
    if ($hWnd -eq $window.Hwnd) { return $true }

    $processId = [uint32]0
    [CslClaudeWin32]::GetWindowThreadProcessId($hWnd, [ref]$processId) | Out-Null
    if ([int]$processId -ne [int]$window.ProcessId) { return $true }

    $classBuilder = New-Object System.Text.StringBuilder 256
    [CslClaudeWin32]::GetClassName($hWnd, $classBuilder, $classBuilder.Capacity) | Out-Null
    $className = $classBuilder.ToString()
    if ($className -notlike 'Chrome_WidgetWin_*') { return $true }

    $rect = New-Object CslClaudeWin32+RECT
    [CslClaudeWin32]::GetWindowRect($hWnd, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -lt 80 -or $height -lt 40) { return $true }
    if ($width -ge ($window.Width - 80) -and $height -ge ($window.Height - 80)) { return $true }

    try {
      $root = [System.Windows.Automation.AutomationElement]::FromHandle($hWnd)
      if (-not $root) { return $true }
      $hasMenuItem = $root.FindFirst(
        [System.Windows.Automation.TreeScope]::Subtree,
        (New-Object System.Windows.Automation.PropertyCondition(
          [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
          [System.Windows.Automation.ControlType]::MenuItem
        ))
      )
      $hasCheckBox = $root.FindFirst(
        [System.Windows.Automation.TreeScope]::Subtree,
        (New-Object System.Windows.Automation.PropertyCondition(
          [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
          [System.Windows.Automation.ControlType]::CheckBox
        ))
      )
      if (-not $hasMenuItem -and -not $hasCheckBox) { return $true }
    } catch {
      return $true
    }

    [CslClaudeWin32]::SetWindowPos($hWnd, [IntPtr]::Zero, -32000, -32000, $width, $height, 0x0014) | Out-Null
    $moved += 1
    return $true
  }, [IntPtr]::Zero) | Out-Null
  if ($moved -gt 0) {
    Write-ClaudeDebuggerLog "Moved $moved Claude menu popup window(s) offscreen."
  }
  $moved
}

function Find-ClaudeMenuElement([string[]]$names, $window, [bool]$preferToggle, [bool]$popupOnly) {
  $best = $null
  $bestScore = -1
  foreach ($rootInfo in (Get-ClaudeAutomationRoots $window)) {
    if ($popupOnly -and $rootInfo.IsMainWindow) { continue }
    $root = $rootInfo.Root
    foreach ($name in $names) {
      $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $name
      )
      $matches = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)
      foreach ($element in $matches) {
        $patterns = @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
        $className = $element.Current.ClassName
        $controlType = $element.Current.ControlType.ProgrammaticName
        $score = 0
        if ($className -eq 'MenuItemView') { $score += 4 }
        if ($controlType -eq 'ControlType.MenuItem') { $score += 4 }
        if ($controlType -eq 'ControlType.CheckBox') { $score += 4 }
        if ($patterns -contains 'ExpandCollapsePatternIdentifiers.Pattern') { $score += 3 }
        if ($patterns -contains 'TogglePatternIdentifiers.Pattern') { $score += 4 }
        if ($patterns -contains 'ValuePatternIdentifiers.Pattern') { $score += 1 }
        if (-not $rootInfo.IsMainWindow) { $score += 2 }
        if ($preferToggle -and $patterns -notcontains 'TogglePatternIdentifiers.Pattern') { continue }
        if ($preferToggle -and $controlType -ne 'ControlType.CheckBox') { continue }
        if ($score -gt $bestScore) {
          $bestScore = $score
          $best = $element
        }
      }
    }
  }
  $best
}

function Find-ClaudeDeveloperMenuElement([string[]]$names, $window) {
  $best = $null
  $bestScore = -1
  foreach ($rootInfo in (Get-ClaudeAutomationRoots $window)) {
    if ($rootInfo.IsMainWindow) { continue }
    $root = $rootInfo.Root
    foreach ($name in $names) {
      $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $name
      )
      $matches = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)
      foreach ($element in $matches) {
        $patterns = @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
        $className = $element.Current.ClassName
        $controlType = $element.Current.ControlType.ProgrammaticName
        if ($patterns -notcontains 'ExpandCollapsePatternIdentifiers.Pattern') { continue }

        $score = 0
        if ($controlType -eq 'ControlType.MenuItem') { $score += 8 }
        if ($className -eq 'MenuItemView') { $score += 8 }
        if ($className -eq 'SubmenuButton') { $score -= 4 }
        if ($patterns -contains 'ScrollItemPatternIdentifiers.Pattern') { $score += 1 }
        if ($score -gt $bestScore) {
          $bestScore = $score
          $best = $element
        }
      }
    }
  }
  if ($best) { Write-ClaudeDebuggerLog ("Selected Developer candidate: " + (Format-ClaudeElementForLog $best)) }
  $best
}

function Find-ClaudeDebuggerToggleElement([string[]]$names, $window) {
  $best = $null
  $bestScore = -1
  foreach ($rootInfo in (Get-ClaudeAutomationRoots $window)) {
    if ($rootInfo.IsMainWindow) { continue }
    $root = $rootInfo.Root
    foreach ($name in $names) {
      $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $name
      )
      $matches = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)
      foreach ($element in $matches) {
        $patterns = @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
        $className = $element.Current.ClassName
        $controlType = $element.Current.ControlType.ProgrammaticName
        if ($patterns -notcontains 'TogglePatternIdentifiers.Pattern') { continue }
        if ($controlType -ne 'ControlType.CheckBox') { continue }

        $score = 0
        if ($className -eq 'MenuItemView') { $score += 8 }
        if ($patterns -contains 'ValuePatternIdentifiers.Pattern') { $score += 1 }
        if ($score -gt $bestScore) {
          $bestScore = $score
          $best = $element
        }
      }
    }
  }
  if ($best) { Write-ClaudeDebuggerLog ("Selected debugger toggle candidate: " + (Format-ClaudeElementForLog $best)) }
  $best
}

function Find-ClaudeMenuItems($window) {
  $conditions = @(
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ClassNameProperty,
      'MenuItemView'
    )),
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::MenuItem
    )),
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::CheckBox
    ))
  )
  $seen = @{}
  $items = New-Object System.Collections.Generic.List[object]
  foreach ($rootInfo in (Get-ClaudeAutomationRoots $window)) {
    if ($rootInfo.IsMainWindow) { continue }
    $root = $rootInfo.Root
    foreach ($condition in $conditions) {
      $matches = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)
      foreach ($element in $matches) {
        $className = $element.Current.ClassName
        if ($className -ne 'MenuItemView') { continue }
        try {
          $runtimeId = [string]::Join('.', $element.GetRuntimeId())
        } catch {
          $runtimeId = "$($element.Current.Name)|$($element.Current.ControlType.ProgrammaticName)"
        }
        if ($seen.ContainsKey($runtimeId)) { continue }
        $seen[$runtimeId] = $true
        $items.Add([pscustomobject]@{
          Element = $element
          ClassName = $className
          ControlType = $element.Current.ControlType.ProgrammaticName
          Patterns = @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
        }) | Out-Null
      }
    }
  }
  $items
}

function Test-ClaudeMenuPopupOpen($window, [string[]]$developerNames) {
  if (Find-ClaudeDeveloperMenuElement $developerNames $window) { return $true }
  if (Find-ClaudeDeveloperMenuByStructure $window) { return $true }
  return $false
}

function Find-ClaudeDeveloperMenuByStructure($window) {
  $expandable = @(Find-ClaudeMenuItems $window | Where-Object {
    $_.ControlType -eq 'ControlType.MenuItem' -and
    $_.Patterns -contains 'ExpandCollapsePatternIdentifiers.Pattern'
  })
  if ($expandable.Count -lt 4) { return $null }
  $selected = $expandable[3].Element
  Write-ClaudeDebuggerLog ("Selected structural Developer candidate: " + (Format-ClaudeElementForLog $selected))
  $selected
}

function Find-ClaudeDebuggerToggleByStructure($window) {
  $toggles = @(Find-ClaudeMenuItems $window | Where-Object {
    $_.ClassName -eq 'MenuItemView' -and
    $_.ControlType -eq 'ControlType.CheckBox' -and
    $_.Patterns -contains 'TogglePatternIdentifiers.Pattern'
  })
  if ($toggles.Count -eq 0) { return $null }
  $selected = $toggles[0].Element
  Write-ClaudeDebuggerLog ("Selected structural debugger toggle candidate: " + (Format-ClaudeElementForLog $selected))
  $selected
}

function Find-ClaudeMenuButton($window) {
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($window.Hwnd)
  if (-not $root) { return $null }
  $names = @('Menu', '菜单')
  $best = $null
  $bestScore = -1
  foreach ($name in $names) {
    $condition = New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::NameProperty,
      $name
    )
    $matches = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)
    foreach ($element in $matches) {
      $patterns = @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
      $className = $element.Current.ClassName
      $controlType = $element.Current.ControlType.ProgrammaticName
      $score = 0
      if ($controlType -eq 'ControlType.Button') { $score += 6 }
      if ($className -match 'Button|Menu') { $score += 4 }
      if ($patterns -contains 'InvokePatternIdentifiers.Pattern') { $score += 4 }
      if ($patterns -contains 'TogglePatternIdentifiers.Pattern') { $score += 2 }
      if ($score -gt $bestScore) {
        $bestScore = $score
        $best = $element
      }
    }
  }
  $best
}

function Open-ClaudeMenu($window, [string[]]$developerNames) {
  $window = Activate-ClaudeMainWindow $window
  if (Test-ClaudeMenuPopupOpen $window $developerNames) {
    Write-ClaudeDebuggerLog 'Claude menu popup already appears to be open.'
    return $true
  }

  $menuButton = Find-ClaudeMenuButton $window
  if (-not $menuButton) {
    Write-ClaudeDebuggerLog 'Claude in-window menu button was not found.'
    return $false
  }
  Write-ClaudeDebuggerLog ("Selected menu button: " + (Format-ClaudeElementForLog $menuButton))

  for ($attempt = 0; $attempt -lt 3; $attempt++) {
    Write-ClaudeDebuggerLog "Invoking Claude menu button attempt $($attempt + 1)."
    if (-not (Invoke-Element $menuButton)) {
      Write-ClaudeDebuggerLog 'Claude menu button did not expose an invokable UIA pattern.'
      return $false
    }
    if (Wait-ClaudeCondition 16 40 { if (Test-ClaudeMenuPopupOpen $window $developerNames) { $true } else { $null } }) {
      Write-ClaudeDebuggerLog 'Claude menu popup opened.'
      return $true
    }
  }

  Write-ClaudeDebuggerLog 'Claude menu popup did not expose Developer after menu button attempts.'
  return $false
}

function Test-ClaudeElementStillVisible($element) {
  if (-not $element) { return $false }
  try {
    $null = $element.Current.ControlType
    if ($element.Current.IsOffscreen) { return $false }
    $rect = $element.Current.BoundingRectangle
    return -not $rect.IsEmpty -and $rect.Width -gt 0 -and $rect.Height -gt 0
  } catch {
    return $false
  }
}

function Get-ClaudeElementRect($element) {
  try { return $element.Current.BoundingRectangle } catch { return $null }
}

function Test-ClaudeRectInside($inner, $outer, [int]$tolerance) {
  if (-not $inner -or -not $outer -or $inner.IsEmpty -or $outer.IsEmpty) { return $false }
  return $inner.Left -ge ($outer.Left - $tolerance) -and
    $inner.Top -ge ($outer.Top - $tolerance) -and
    $inner.Right -le ($outer.Right + $tolerance) -and
    $inner.Bottom -le ($outer.Bottom + $tolerance)
}

function Test-ClaudeModalCloseButtonName([string]$name) {
  if (-not $name) { return $false }
  return $name -match '^(Close|Dismiss|Not now|No thanks|Maybe later|Got it|OK)$' -or
    $name -match '^(关闭|關閉|稍后|稍後|取消|跳过|跳過|知道了?|好的)$'
}

function Find-ClaudeModalCloseButton($modal) {
  $modalRect = Get-ClaudeElementRect $modal
  if (-not $modalRect -or $modalRect.IsEmpty) { return $null }

  $condition = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Button
  )
  $matches = $modal.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)
  $best = $null
  $bestScore = -1
  foreach ($element in $matches) {
    $patterns = @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
    if ($patterns -notcontains 'InvokePatternIdentifiers.Pattern' -and
        $patterns -notcontains 'LegacyIAccessiblePatternIdentifiers.Pattern') { continue }

    $className = ''
    $name = ''
    try { $className = $element.Current.ClassName } catch { $className = '' }
    try { $name = $element.Current.Name } catch { $name = '' }
    if ($className -eq 'WinCaptionButton') { continue }

    $rect = Get-ClaudeElementRect $element
    if (-not (Test-ClaudeRectInside $rect $modalRect 4)) { continue }
    if ($rect.Width -lt 12 -or $rect.Height -lt 12) { continue }

    $score = 0
    if (Test-ClaudeModalCloseButtonName $name) { $score += 18 }
    if ($name.Length -eq 0 -and $rect.Width -le 80 -and $rect.Height -le 80) { $score += 6 }
    if ($rect.Top -le ($modalRect.Top + 120) -and $rect.Right -ge ($modalRect.Right - 160)) { $score += 10 }
    if ($className -match 'close|icon|ghost|square|aspect-square|rounded') { $score += 4 }
    if ($patterns -contains 'LegacyIAccessiblePatternIdentifiers.Pattern') { $score += 2 }
    if ($score -lt 8) { continue }
    if ($score -gt $bestScore) {
      $bestScore = $score
      $best = $element
    }
  }
  $best
}

function Find-ClaudeBlockingWebModal($root) {
  $rootRect = Get-ClaudeElementRect $root
  if (-not $rootRect -or $rootRect.IsEmpty) { return $null }

  $conditions = @(
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::Window
    )),
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::Pane
    )),
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::Group
    ))
  )

  $best = $null
  $bestScore = -1
  foreach ($condition in $conditions) {
    $matches = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)
    foreach ($element in $matches) {
      if ($element -eq $root) { continue }
      $controlType = ''
      $className = ''
      $frameworkId = ''
      try { $controlType = $element.Current.ControlType.ProgrammaticName } catch { $controlType = '' }
      try { $className = $element.Current.ClassName } catch { $className = '' }
      try { $frameworkId = $element.Current.FrameworkId } catch { $frameworkId = '' }
      if ($frameworkId -ne 'Chrome') { continue }
      if ($className -eq 'WinCaptionButton') { continue }
      if ($className -eq 'MenuItemView') { continue }

      $rect = Get-ClaudeElementRect $element
      if (-not (Test-ClaudeRectInside $rect $rootRect 8)) { continue }
      if ($rect.Width -lt 260 -or $rect.Height -lt 160) { continue }
      if ($rect.Width -ge ($rootRect.Width - 20) -and $rect.Height -ge ($rootRect.Height - 20)) { continue }

      $button = Find-ClaudeModalCloseButton $element
      if (-not $button) { continue }

      $score = 0
      if ($controlType -eq 'ControlType.Window') { $score += 24 }
      if ($element.Current.IsKeyboardFocusable) { $score += 6 }
      if ($className -match 'fixed|modal|dialog|popover|rounded|shadow|z-') { $score += 4 }
      $area = [double]$rect.Width * [double]$rect.Height
      $rootArea = [double]$rootRect.Width * [double]$rootRect.Height
      if ($rootArea -gt 0) {
        $ratio = $area / $rootArea
        if ($ratio -ge 0.05 -and $ratio -le 0.80) { $score += 4 }
      }
      if ($score -gt $bestScore) {
        $bestScore = $score
        $best = $element
      }
    }
  }
  $best
}

function Get-ClaudeBlockingWebCloseButtonScore($button, $rootRect) {
  if (-not $button -or -not $rootRect -or $rootRect.IsEmpty) { return -1 }

  $controlType = ''
  $className = ''
  $frameworkId = ''
  $name = ''
  try { $controlType = $button.Current.ControlType.ProgrammaticName } catch { return -1 }
  try { $className = $button.Current.ClassName } catch { $className = '' }
  try { $frameworkId = $button.Current.FrameworkId } catch { $frameworkId = '' }
  try { $name = $button.Current.Name } catch { $name = '' }
  if ($controlType -ne 'ControlType.Button') { return -1 }
  if ($frameworkId -ne 'Chrome') { return -1 }
  if ($className -eq 'WinCaptionButton') { return -1 }
  try { if ($button.Current.IsOffscreen) { return -1 } } catch {}

  $patterns = @($button.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
  if ($patterns -notcontains 'InvokePatternIdentifiers.Pattern' -and
      $patterns -notcontains 'LegacyIAccessiblePatternIdentifiers.Pattern') { return -1 }

  $rect = Get-ClaudeElementRect $button
  if (-not (Test-ClaudeRectInside $rect $rootRect 8)) { return -1 }
  if ($rect.Width -lt 12 -or $rect.Height -lt 12) { return -1 }

  $smallSquare = $rect.Width -le 100 -and $rect.Height -le 100
  $rightSide = $rect.Left -ge ($rootRect.Left + ($rootRect.Width * 0.50))
  $upperContent = $rect.Top -ge ($rootRect.Top + 56) -and
    $rect.Top -le ($rootRect.Top + ($rootRect.Height * 0.60))
  $looksLikeNamedClose = $name -match '^(Close|关闭|關閉)$'
  $looksLikeDismissAction = Test-ClaudeModalCloseButtonName $name

  if (-not $looksLikeDismissAction -and $name.Length -gt 0) { return -1 }
  if ($name.Length -eq 0 -and -not ($smallSquare -and $rightSide -and $upperContent)) { return -1 }

  $score = 0
  if ($looksLikeNamedClose) {
    $score += 24
  } elseif ($looksLikeDismissAction) {
    $score += 14
  }
  if ($smallSquare) { $score += 8 }
  if ($rightSide) { $score += 5 }
  if ($upperContent) { $score += 5 }
  if ($className -match 'close|icon|ghost|square|aspect-square|rounded|w-control') { $score += 4 }
  if ($patterns -contains 'LegacyIAccessiblePatternIdentifiers.Pattern') { $score += 2 }
  $score
}

function Test-ClaudeBlockingWebCloseButton($button, $rootRect) {
  (Get-ClaudeBlockingWebCloseButtonScore $button $rootRect) -ge 20
}

function Find-ClaudeBlockingWebCloseButton($root, $window) {
  if (-not $root) { return $null }
  $menuButton = Find-ClaudeMenuButton $window
  if ($menuButton) { return $null }

  $rootRect = Get-ClaudeElementRect $root
  if (-not $rootRect -or $rootRect.IsEmpty) { return $null }

  $condition = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Button
  )
  $matches = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)
  $best = $null
  $bestScore = -1
  foreach ($button in $matches) {
    if (-not (Test-ClaudeBlockingWebCloseButton $button $rootRect)) { continue }
    $score = Get-ClaudeBlockingWebCloseButtonScore $button $rootRect
    if ($score -gt $bestScore) {
      $bestScore = $score
      $best = $button
    }
  }
  if ($best) { Write-ClaudeDebuggerLog ("Selected structural web close button: " + (Format-ClaudeElementForLog $best)) }
  $best
}

function Find-ClaudeCloseButton($root) {
  # Direct full-subtree search for any dismiss button that is not a window
  # caption button. Used while the Menu button is hidden by a popup: the popup
  # may not be identifiable as a discrete modal container, so searching the
  # whole tree for a dismissable button is more reliable than first locating
  # the modal then its close button.
  #
  # Claude desktop localizes the accessible names of its popup buttons, so the
  # match covers every language Claude ships (en, zh-CN, zh-TW, fr, de, es,
  # pt-BR, ja, ko, it, hi, id). Explicit "close" verbs score higher than softer
  # dismissals ("not now"/"later"/etc.) since closing the popup outright is the
  # goal.
  if (-not $root) { return $null }
  $condition = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Button
  )
  # Localized close verbs (preferred) and softer dismissals (fallback), across
  # every Claude-supported locale.
  $closeVerbs = '^(Close|关闭|關閉|Fermer|Schließen|Schliessen|Cerrar|Fechar|閉じる|닫기|Chiudi|बंद करें|Tutup)$'
  $dismissPhrases = '^(Not now|Dismiss|No thanks|Maybe later|Got it|OK|Later|暂不|以后再说|以后再說|不用了|知道了|好的|稍后|稍後|以后|以後|Pas maintenant|Plus tard|Non merci|Peut-être plus tard|J''ai compris|Nicht jetzt|Später|Spater|Nein danke|Vielleicht später|Vielleicht spater|Verstanden|Ahora no|Más tarde|Mas tarde|No gracias|Tal vez más tarde|Tal vez mas tarde|Entendido|Agora não|Agora nao|Mais tarde|Não obrigado|Nao obrigado|Talvez mais tarde|Entendi|後で|今はしない|いいえ|あとで|나중에|아니요|알겠습니다|Non ora|Più tardi|Piu tardi|No grazie|Magari più tardi|Magari piu tardi|Ho capito|अभी नहीं|बाद में|नहीं धन्यवाद|Jangan sekarang|Nanti|Tidak terima kasih)$'
  $best = $null
  $bestScore = -1
  foreach ($button in $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, $condition)) {
    $name = ''
    $className = ''
    try { $name = $button.Current.Name } catch { $name = '' }
    try { $className = $button.Current.ClassName } catch { $className = '' }
    if ($className -eq 'WinCaptionButton') { continue }
    $isCloseVerb = $name -match $closeVerbs
    if (-not $isCloseVerb -and $name -notmatch $dismissPhrases) { continue }
    $patterns = @($button.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
    if ($patterns -notcontains 'InvokePatternIdentifiers.Pattern' -and
        $patterns -notcontains 'LegacyIAccessiblePatternIdentifiers.Pattern') { continue }
    try { if ($button.Current.IsOffscreen) { continue } } catch {}
    $rect = $button.Current.BoundingRectangle
    if ($rect.Width -lt 12 -or $rect.Height -lt 12) { continue }
    # Prefer an explicit close verb over softer dismissals (Not now/etc.).
    $score = 0
    if ($isCloseVerb) { $score += 20 } else { $score += 10 }
    if ($className -match 'close|icon|ghost|square|aspect-square|rounded') { $score += 4 }
    if ($score -gt $bestScore) {
      $bestScore = $score
      $best = $button
    }
  }
  $best
}

function Close-ClaudeBlockingWebModals($window) {
  $closed = 0
  try {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($window.Hwnd)
    if (-not $root) { return 0 }

    for ($attempt = 0; $attempt -lt 3; $attempt++) {
      if (Find-ClaudeMenuButton $window) { break }
      $modal = Find-ClaudeBlockingWebModal $root
      if (-not $modal) { break }
      # The popup's Close button may not have rendered yet when the script
      # runs right after the window is activated. Wait for it to appear before
      # giving up — otherwise Close-ClaudeBlockingWebModals exits without ever
      # invoking the dismiss button, and the main flow's Menu-button gate then
      # loops forever without dismissing the popup.
      $button = Wait-ClaudeCondition 40 50 { Find-ClaudeModalCloseButton $modal }
      if (-not $button) {
        Write-ClaudeDebuggerLog 'Claude blocking web modal had no close button after waiting.'
        break
      }
      Write-ClaudeDebuggerLog ("Closing Claude blocking web modal: modal=" + (Format-ClaudeElementForLog $modal) + " button=" + (Format-ClaudeElementForLog $button))
      $invoked = Invoke-Element $button
      # Gauge success by the in-window Menu button reappearing (it is hidden
      # while a blocking popup covers the toolbar) rather than by the modal
      # element's visibility, which stays stale after the popup is dismissed.
      if ($invoked -and (Wait-ClaudeCondition 8 50 { if (Find-ClaudeMenuButton $window) { $true } else { $null } })) { $closed += 1; continue }
      $invoked = Invoke-ClaudeElementDefaultAction $button
      if (-not $invoked) { break }
      if (Wait-ClaudeCondition 8 50 { if (Find-ClaudeMenuButton $window) { $true } else { $null } }) { $closed += 1; continue }
      break
    }

    for ($attempt = 0; $attempt -lt 2; $attempt++) {
      if (Find-ClaudeMenuButton $window) { break }
      $button = Wait-ClaudeCondition 40 50 { Find-ClaudeBlockingWebCloseButton $root $window }
      if (-not $button) { break }
      Write-ClaudeDebuggerLog ("Closing Claude blocking web content via close button: " + (Format-ClaudeElementForLog $button))
      $invoked = Invoke-Element $button
      if ($invoked -and (Wait-ClaudeCondition 16 50 { if (Find-ClaudeMenuButton $window) { $true } else { $null } })) { $closed += 1; continue }
      $invoked = Invoke-ClaudeElementDefaultAction $button
      if (-not $invoked) { break }
      if (Wait-ClaudeCondition 16 50 { if (Find-ClaudeMenuButton $window) { $true } else { $null } }) { $closed += 1; continue }
      break
    }
  } catch {
    Write-ClaudeDebuggerLog "Ignoring Claude blocking web modal cleanup failure: $($_.Exception.Message)"
  }
  $closed
}

function Close-ClaudeInspectorPromptWindows($window) {
  $closed = 0
  [CslClaudeWin32]::EnumWindows({
    param([IntPtr]$hWnd, [IntPtr]$extraData)
    if ($hWnd -eq $window.Hwnd) { return $true }

    $processId = [uint32]0
    [CslClaudeWin32]::GetWindowThreadProcessId($hWnd, [ref]$processId) | Out-Null
    if ([int]$processId -ne [int]$window.ProcessId) { return $true }

    $classBuilder = New-Object System.Text.StringBuilder 256
    [CslClaudeWin32]::GetClassName($hWnd, $classBuilder, $classBuilder.Capacity) | Out-Null
    $className = $classBuilder.ToString()
    if (-not (Test-ClaudeInspectorWindowClass $className)) { return $true }

    $titleBuilder = New-Object System.Text.StringBuilder 512
    [CslClaudeWin32]::GetWindowText($hWnd, $titleBuilder, $titleBuilder.Capacity) | Out-Null
    $title = $titleBuilder.ToString()
    $rect = New-Object CslClaudeWin32+RECT
    [CslClaudeWin32]::GetWindowRect($hWnd, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top

    $looksLikeInspectorPrompt = (Test-ClaudeInspectorPromptCandidate $title $width $height) -and
      ($width -lt ($window.Width - 80) -or $height -lt ($window.Height - 80))
    if (-not $looksLikeInspectorPrompt) { return $true }

    try {
      $element = [System.Windows.Automation.AutomationElement]::FromHandle($hWnd)
      $windowPattern = $null
      if ($element -and $element.TryGetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern, [ref]$windowPattern)) {
        $windowPattern.Close()
      }
    } catch {}
    [CslClaudeWin32]::PostMessage($hWnd, $WM_CLOSE, [UIntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $closed += 1
    return $true
  }, [IntPtr]::Zero) | Out-Null
  $closed
}

function Wait-CloseClaudeInspectorPromptWindows($window, [int]$attempts = 10) {
  $closed = 0
  for ($attempt = 0; $attempt -lt $attempts; $attempt++) {
    $closed += Close-ClaudeInspectorPromptWindows $window
    if ($closed -gt 0) {
      Start-Sleep -Milliseconds 40
      $closed += Close-ClaudeInspectorPromptWindows $window
      break
    }
    Start-Sleep -Milliseconds 40
  }
  $closed
}

function Start-ClaudeInspectorPromptCleanupJob($window, [int]$durationMs) {
  $processId = [int]$window.ProcessId
  $mainHwnd = [IntPtr]$window.Hwnd
  $mainWidth = [int]$window.Width
  $mainHeight = [int]$window.Height
  Start-Job -ScriptBlock {
    param([int]$processId, [IntPtr]$mainHwnd, [int]$mainWidth, [int]$mainHeight, [int]$durationMs)
    Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class CslClaudePromptCleanupWin32 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr extraData);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, UIntPtr wParam, IntPtr lParam);
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
'@
    $deadline = [DateTime]::UtcNow.AddMilliseconds($durationMs)
    $WM_CLOSE = 0x0010
    while ([DateTime]::UtcNow -lt $deadline) {
      [CslClaudePromptCleanupWin32]::EnumWindows({
        param([IntPtr]$hWnd, [IntPtr]$extraData)
        if ($hWnd -eq $mainHwnd) { return $true }
        $windowProcessId = [uint32]0
        [CslClaudePromptCleanupWin32]::GetWindowThreadProcessId($hWnd, [ref]$windowProcessId) | Out-Null
        if ([int]$windowProcessId -ne $processId) { return $true }

        $classBuilder = New-Object System.Text.StringBuilder 256
        [CslClaudePromptCleanupWin32]::GetClassName($hWnd, $classBuilder, $classBuilder.Capacity) | Out-Null
        $className = $classBuilder.ToString()
        if ($className -notlike 'Chrome_WidgetWin_*' -and $className -ne '#32770') { return $true }

        $titleBuilder = New-Object System.Text.StringBuilder 512
        [CslClaudePromptCleanupWin32]::GetWindowText($hWnd, $titleBuilder, $titleBuilder.Capacity) | Out-Null
        $title = $titleBuilder.ToString()
        $rect = New-Object CslClaudePromptCleanupWin32+RECT
        [CslClaudePromptCleanupWin32]::GetWindowRect($hWnd, [ref]$rect) | Out-Null
        $width = $rect.Right - $rect.Left
        $height = $rect.Bottom - $rect.Top
        $looksLikePrompt =
          $title -match 'Inspector|Debugger|DevTools|Main Process|调试|偵錯|检查|檢查' -or
          ($title.Length -eq 0 -and $width -ge 480 -and $width -le 1200 -and $height -ge 360 -and $height -le 900)
        $insideMainWindow = $width -ge ($mainWidth - 80) -and $height -ge ($mainHeight - 80)
        if ($looksLikePrompt -and -not $insideMainWindow) {
          [CslClaudePromptCleanupWin32]::PostMessage($hWnd, $WM_CLOSE, [UIntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        }
        return $true
      }, [IntPtr]::Zero) | Out-Null
      Start-Sleep -Milliseconds 80
    }
  } -ArgumentList $processId, $mainHwnd, $mainWidth, $mainHeight, $durationMs | Out-Null
}

function Invoke-DebuggerConfirmation($window) {
  $names = @(
    'Continue', 'Allow', 'Open',
    '继续', '允许', '繼續', '開く',
    'Continuer', 'Fortfahren', 'Continuar', 'Permitir', 'Lanjutkan'
  )
  $button = Find-ClaudeMenuElement $names $window $false $true
  if (-not $button) { return $false }
  Write-ClaudeDebuggerLog ("Invoking debugger confirmation: " + (Format-ClaudeElementForLog $button))
  Invoke-Element $button | Out-Null
  return $true
}

Write-ClaudeDebuggerLog 'Windows Main Process Debugger automation started.'

$window = $null
$window = Get-ClaudeMainWindow
if ($window) {
  Write-ClaudeDebuggerLog 'Using existing Claude window before app activation.'
} else {
  Start-ClaudeWindowsApp
  $window = Wait-ClaudeCondition 8 40 {
    $candidate = Get-ClaudeMainWindow
    if ($candidate) { $candidate } else { $null }
  }
  if (-not $window) {
    Start-ClaudeWindowsApp
    $window = Wait-ClaudeCondition 50 100 {
      $candidate = Get-ClaudeMainWindow
      if ($candidate) { $candidate } else { $null }
    }
  }
}
if (-not $window) {
  Write-ClaudeDebuggerLog 'Claude main window was not found after launch.'
  throw 'Claude main window was not found after launch.'
}

$window = Activate-ClaudeMainWindow $window
Wait-CloseClaudeInspectorPromptWindows $window 2 | Out-Null
$developerNames = @('Developer', '开发者', '開發者')
$debuggerNames = @(
  'Enable Main Process Debugger',
  'Main Process Debugger',
  '启用主进程调试器',
  '啟用主進程偵錯器'
)

# Claude repaints its window asynchronously after activation. The in-window
# Menu button takes a moment to enter the UIA tree, and when a popup (e.g. the
# upgrade plan banner) is shown the Menu button stays hidden until the popup is
# dismissed. Driving menu automation before the Menu button is visible makes
# Open-ClaudeMenu fail and the whole debugger enablement throws out.
#
# Single poll loop: keep checking for the Menu button (signal #1 — when it
# appears the window is ready and unobstructed, so stop and open the menu).
# While the Menu button is still missing, look for any dismiss button
# (Close/Not now, signal #2) across the whole tree and invoke it to clear the
# popup. Once the Menu button appears, the dismiss search stops too.
$root = [System.Windows.Automation.AutomationElement]::FromHandle($window.Hwnd)
$menuReady = $false
for ($phase = 0; $phase -lt 4; $phase++) {
  if (Find-ClaudeMenuButton $window) {
    $menuReady = $true
    Write-ClaudeDebuggerLog 'Claude in-window menu button appeared; proceeding to menu automation.'
    break
  }
  Write-ClaudeDebuggerLog "Menu button not visible yet (phase $($phase + 1)); scanning for dismiss button."
  $closeButton = Find-ClaudeCloseButton $root
  if ($closeButton) {
    Write-ClaudeDebuggerLog ("Invoking dismiss button: " + (Format-ClaudeElementForLog $closeButton))
    $invoked = Invoke-Element $closeButton
    if (-not $invoked) { $invoked = Invoke-ClaudeElementDefaultAction $closeButton }
    if ($invoked) {
      # Give the popup a moment to dismiss before the next Menu check.
      Wait-ClaudeCondition 8 50 { if (Find-ClaudeMenuButton $window) { $true } else { $null } } | Out-Null
    }
  } else {
    # No dismiss button found this round; briefly keep polling for the Menu
    # button before the next full scan so the loop stays responsive.
    if (Wait-ClaudeCondition 10 50 { if (Find-ClaudeMenuButton $window) { $true } else { $null } }) {
      $menuReady = $true
      Write-ClaudeDebuggerLog 'Claude in-window menu button appeared; proceeding to menu automation.'
      break
    }
  }
}
if (-not $menuReady) {
  Write-ClaudeDebuggerLog 'Claude in-window menu button did not appear after popup cleanup.'
}

$developer = $null
if (-not (Open-ClaudeMenu $window $developerNames)) {
  throw 'Claude in-window menu could not be opened through UI Automation.'
}
Move-ClaudeMenuPopupsOffscreen $window | Out-Null
$developer = Find-ClaudeDeveloperMenuElement $developerNames $window
if (-not $developer) {
  $developer = Find-ClaudeDeveloperMenuByStructure $window
}
if (-not $developer) {
  Write-ClaudeDebuggerLog 'Claude Developer menu was not found after opening menu popup.'
  throw 'Claude Developer menu was not found.'
}
Write-ClaudeDebuggerLog ("Invoking Developer menu: " + (Format-ClaudeElementForLog $developer))
if (-not (Invoke-Element $developer)) {
  Write-ClaudeDebuggerLog ("Developer menu could not be opened: " + (Format-ClaudeElementForLog $developer))
  throw 'Claude Developer menu could not be opened through UI Automation.'
}
Move-ClaudeMenuPopupsOffscreen $window | Out-Null

$debuggerItem = Find-ClaudeDebuggerToggleElement $debuggerNames $window
if (-not $debuggerItem) {
  $debuggerItem = Find-ClaudeDebuggerToggleByStructure $window
}
if (-not $debuggerItem) {
  Write-ClaudeDebuggerLog 'Claude Developer > Enable Main Process Debugger menu item was not found.'
  throw 'Claude Developer > Enable Main Process Debugger menu item was not found.'
}
Write-ClaudeDebuggerLog ("Using debugger toggle: " + (Format-ClaudeElementForLog $debuggerItem))

$valuePattern = $null
$null = $debuggerItem.TryGetCurrentPattern(
  [System.Windows.Automation.ValuePattern]::Pattern,
  [ref]$valuePattern
)

$togglePattern = $null
if ($debuggerItem.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$togglePattern)) {
  if ($togglePattern.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On) {
    Write-ClaudeDebuggerLog 'Toggling Claude Main Process Debugger on.'
    $togglePattern.Toggle()
    Start-ClaudeInspectorPromptCleanupJob $window 4500
  } else {
    Write-ClaudeDebuggerLog 'Claude Main Process Debugger toggle already appears on.'
    Start-ClaudeInspectorPromptCleanupJob $window 2000
  }
} else {
  Write-ClaudeDebuggerLog 'Claude Main Process Debugger menu item did not expose TogglePattern.'
  throw 'Claude Main Process Debugger menu item does not expose TogglePattern.'
}

for ($attempt = 0; $attempt -lt 3; $attempt++) {
  Wait-CloseClaudeInspectorPromptWindows $window 1 | Out-Null
  if (-not (Invoke-DebuggerConfirmation $window)) { break }
}
Start-ClaudeInspectorPromptCleanupJob $window 2000
Write-ClaudeDebuggerLog 'Windows Main Process Debugger automation completed.'
"#;

    run_windows_debugger_powershell_with_timeout(
        script,
        WINDOWS_MAIN_PROCESS_DEBUGGER_SCRIPT_TIMEOUT,
    )
    .map(|_| ())
    .map_err(|err| {
        // The PowerShell automation writes its own progress log, but a parse
        // error or early crash happens before that log is ever written, leaving
        // no trace. Mirror the failure (incl. PowerShell stderr) to a separate
        // file so we can diagnose why the debugger never came up.
        if let Ok(paths) = app_paths() {
            let log_path = paths
                .config_dir
                .join("claude-desktop-patch")
                .join("windows-main-debugger-error.log");
            let _ = fs::write(
                &log_path,
                format!(
                    "[{}] {}\n",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    err
                ),
            );
        }
        format!("Failed to request Claude main process debugger on Windows: {err}")
    })
}

#[cfg(target_os = "windows")]
fn run_windows_debugger_powershell_with_timeout(
    script: &str,
    timeout: Duration,
) -> Result<String, String> {
    let script = format!(
        r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
{script}
"#
    );
    // The debugger automation script is large (tens of KB). Passing it as a
    // `-Command` argument overflows the Windows command-line length limit
    // (32767 chars, os error 206 "filename or extension too long"), so
    // PowerShell never starts. Write it to a temp .ps1 file (UTF-8 with BOM so
    // Windows PowerShell 5.1 decodes the embedded CJK menu names correctly) and
    // invoke with -File instead.
    let temp_dir = env::temp_dir();
    let script_path = temp_dir.join("codestudio-claude-debugger.ps1");
    let mut bytes = Vec::with_capacity(script.len() + 3);
    // UTF-8 BOM so PowerShell 5.1 reads the file as UTF-8, not system ANSI.
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(script.as_bytes());
    fs::write(&script_path, &bytes)
        .map_err(|err| format!("Failed to write PowerShell script to temp file: {err}"))?;

    let mut child = hidden_command(powershell_exe())
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.to_string_lossy(),
        ])
        .spawn()
        .map_err(|err| {
            let _ = fs::remove_file(&script_path);
            format!("Failed to start PowerShell: {err}")
        })?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let output = child.wait_with_output().map_err(|err| {
                    let _ = fs::remove_file(&script_path);
                    format!("Failed to read PowerShell output: {err}")
                })?;
                if !output.status.success() {
                    let _ = fs::remove_file(&script_path);
                    return Err(format!(
                        "PowerShell execution failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
                let _ = fs::remove_file(&script_path);
                return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = fs::remove_file(&script_path);
                    return Err(format!(
                        "PowerShell debugger automation timed out after {} seconds; waiting for manual Main Process Debugger activation.",
                        timeout.as_secs()
                    ));
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&script_path);
                return Err(format!(
                    "Failed to poll PowerShell debugger automation: {err}"
                ));
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn ensure_macos_accessibility_trusted_for_localized_launch() -> Result<(), String> {
    append_macos_debugger_log(format!(
        "Accessibility preflight started; {}",
        macos_accessibility_identity_summary()
    ));
    if macos_accessibility_is_trusted_raw() {
        append_macos_debugger_log("Accessibility preflight check: AXIsProcessTrusted=true");
        return Ok(());
    }
    append_macos_debugger_log("Accessibility preflight check: AXIsProcessTrusted=false");
    if request_macos_accessibility_prompt("localized launch preflight") {
        append_macos_debugger_log(
            "Accessibility preflight prompt returned trusted; restart required before launching Claude",
        );
        return Err(macos_accessibility_restart_required_error());
    }

    Err(macos_accessibility_not_trusted_error())
}

#[cfg(target_os = "macos")]
fn enable_macos_claude_main_process_debugger() -> Result<(), String> {
    let started = Instant::now();
    let mut last_error = "Claude Node inspector endpoint was not available.".to_string();
    let mut request_count = 0usize;
    while started.elapsed() < MACOS_MAIN_PROCESS_DEBUGGER_WAIT_TIMEOUT {
        if claude_node_inspector_available() {
            return Ok(());
        }

        request_count += 1;
        match request_macos_claude_main_process_debugger_once() {
            Ok(()) => {
                if wait_for_claude_node_inspector() {
                    return Ok(());
                }
                last_error = format!(
                    "Claude main process debugger menu request #{request_count} completed, but no Claude Node inspector endpoint opened yet."
                );
            }
            Err(err) => {
                if err.contains("ACCESSIBILITY_NOT_TRUSTED") {
                    return Err(format!(
                        "{err} After granting Accessibility access, quit and reopen CodeStudio Lite if macOS still reports it as not trusted, then retry localized launch."
                    ));
                }
                last_error = err;
            }
        }
        thread::sleep(Duration::from_millis(MACOS_MAIN_PROCESS_DEBUGGER_RETRY_MS));
    }

    Err(format!(
        "Timed out waiting for Claude main process debugger. Grant CodeStudio Lite Accessibility permission in System Settings, keep Claude open, then try again. Last error: {last_error}. Debug log: {}",
        macos_debugger_log_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "~/.codestudio-lite/claude-desktop-patch/macos-main-debugger.log".to_string())
    ))
}

#[cfg(target_os = "macos")]
fn request_macos_claude_main_process_debugger_once() -> Result<(), String> {
    match request_macos_claude_main_process_debugger_native() {
        Ok(()) => {
            append_macos_debugger_log("native Accessibility request succeeded");
            Ok(())
        }
        Err(err) => {
            append_macos_debugger_log(format!("native Accessibility request failed: {err}"));
            Err(err)
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_debugger_log_path() -> Option<PathBuf> {
    app_paths().ok().map(|paths| {
        paths
            .config_dir
            .join("claude-desktop-patch")
            .join("macos-main-debugger.log")
    })
}

#[cfg(target_os = "macos")]
fn append_macos_debugger_log(message: impl AsRef<str>) {
    let Some(path) = macos_debugger_log_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(
            file,
            "[{}] {}",
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            message.as_ref()
        );
    }
}

#[cfg(target_os = "macos")]
type CFTypeRef = *const c_void;
#[cfg(target_os = "macos")]
type CFStringRef = *const c_void;
#[cfg(target_os = "macos")]
type CFArrayRef = *const c_void;
#[cfg(target_os = "macos")]
type CFDictionaryRef = *const c_void;
#[cfg(target_os = "macos")]
type AXUIElementRef = *const c_void;

#[cfg(target_os = "macos")]
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
#[cfg(target_os = "macos")]
const AX_ERROR_SUCCESS: i32 = 0;

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: CFTypeRef;
    fn CFStringCreateWithCString(
        alloc: CFTypeRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetCount(array: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: isize) -> CFTypeRef;
    fn CFDictionaryCreate(
        allocator: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        num_values: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFDictionaryRef;
}

#[cfg(target_os = "macos")]
struct OwnedCf(CFTypeRef);

#[cfg(target_os = "macos")]
impl OwnedCf {
    fn new(value: CFTypeRef) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }

    fn as_ptr(&self) -> CFTypeRef {
        self.0
    }
}

#[cfg(target_os = "macos")]
impl Drop for OwnedCf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

#[cfg(target_os = "macos")]
fn request_macos_claude_main_process_debugger_native() -> Result<(), String> {
    if claude_node_inspector_available() {
        return Ok(());
    }
    if !macos_accessibility_trusted_or_prompt() {
        return Err(macos_accessibility_not_trusted_error());
    }

    let pids = macos_claude_process_ids()?;
    if pids.is_empty() {
        return Err("Claude process was not found after launch.".to_string());
    }

    let mut errors = Vec::new();
    for pid in pids {
        if claude_node_inspector_available() {
            return Ok(());
        }
        match request_macos_claude_main_process_debugger_native_for_pid(pid) {
            Ok(()) => return Ok(()),
            Err(err) => errors.push(format!("pid {pid}: {err}")),
        }
    }
    Err(format!(
        "Native Accessibility click did not enable Claude main process debugger. {}",
        errors.join(" | ")
    ))
}

#[cfg(target_os = "macos")]
fn macos_accessibility_is_trusted_raw() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(target_os = "macos")]
fn macos_accessibility_not_trusted_error() -> String {
    let log_path = macos_debugger_log_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| {
            "~/.codestudio-lite/claude-desktop-patch/macos-main-debugger.log".to_string()
        });

    format!(
        "ACCESSIBILITY_NOT_TRUSTED: CodeStudio Lite is not trusted for macOS Accessibility yet. Enable the exact running CodeStudio Lite app in System Settings > Privacy & Security > Accessibility, then retry the localized launch. {}. If it is already enabled, remove the old CodeStudio Lite entry from Accessibility, add this exact app again, then quit and reopen CodeStudio Lite. Debug log: {log_path}",
        macos_accessibility_identity_summary()
    )
}

#[cfg(target_os = "macos")]
fn macos_accessibility_restart_required_error() -> String {
    let log_path = macos_debugger_log_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| {
            "~/.codestudio-lite/claude-desktop-patch/macos-main-debugger.log".to_string()
        });

    format!(
        "ACCESSIBILITY_NOT_TRUSTED: CodeStudio Lite Accessibility access was just granted, but macOS requires restarting the app before automation is reliable. Confirm the restart in CodeStudio Lite to resume this Claude Desktop launch. {}. Debug log: {log_path}",
        macos_accessibility_identity_summary()
    )
}

#[cfg(target_os = "macos")]
fn macos_accessibility_identity_summary() -> String {
    let current_exe = env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|err| format!("unavailable ({err})"));
    let app_bundle = env::current_exe()
        .ok()
        .and_then(|path| macos_app_bundle_for_executable(&path))
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "unavailable".to_string());
    format!("Current app bundle: {app_bundle}. Current executable: {current_exe}")
}

#[cfg(target_os = "macos")]
fn macos_app_bundle_for_executable(executable: &Path) -> Option<PathBuf> {
    for ancestor in executable.ancestors() {
        if ancestor
            .extension()
            .is_some_and(|extension| extension == "app")
        {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn macos_accessibility_trusted_or_prompt() -> bool {
    if macos_accessibility_is_trusted_raw() {
        append_macos_debugger_log(
            "Accessibility debugger check: AXIsProcessTrusted=true before prompt",
        );
        return true;
    }

    append_macos_debugger_log(
        "Accessibility debugger check: AXIsProcessTrusted=false before prompt",
    );
    let prompt_result = request_macos_accessibility_prompt("debugger menu request");
    let trusted_after_prompt = macos_accessibility_is_trusted_raw();
    append_macos_debugger_log(format!(
        "Accessibility debugger check after prompt: AXIsProcessTrusted={trusted_after_prompt}"
    ));
    prompt_result || trusted_after_prompt
}

#[cfg(target_os = "macos")]
fn request_macos_accessibility_prompt(reason: &str) -> bool {
    if MACOS_ACCESSIBILITY_PROMPT_REQUESTED.swap(true, Ordering::SeqCst) {
        append_macos_debugger_log(format!(
            "Accessibility prompt already requested; reason={reason}"
        ));
        return false;
    }

    append_macos_debugger_log(format!(
        "requesting CodeStudio Lite Accessibility permission prompt from macOS; reason={reason}; {}",
        macos_accessibility_identity_summary()
    ));
    let prompt_result = macos_request_accessibility_prompt_raw();
    append_macos_debugger_log(format!(
        "AXIsProcessTrustedWithOptions(prompt=true) returned {prompt_result}; reason={reason}"
    ));
    prompt_result
}

#[cfg(target_os = "macos")]
fn macos_request_accessibility_prompt_raw() -> bool {
    let keys = [unsafe { kAXTrustedCheckOptionPrompt as CFTypeRef }];
    let values = [unsafe { kCFBooleanTrue }];
    let options = unsafe {
        CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if options.is_null() {
        return false;
    }
    let trusted = unsafe { AXIsProcessTrustedWithOptions(options) != 0 };
    unsafe { CFRelease(options) };
    trusted
}

#[cfg(target_os = "macos")]
fn request_macos_claude_main_process_debugger_native_for_pid(pid: u32) -> Result<(), String> {
    let app = unsafe { AXUIElementCreateApplication(pid as i32) };
    let app = OwnedCf::new(app)
        .ok_or_else(|| format!("AXUIElementCreateApplication({pid}) returned null"))?;
    ax_set_frontmost(app.as_ptr());

    let mut observed_titles = Vec::new();
    for attempt in 1..=20 {
        if claude_node_inspector_available() {
            return Ok(());
        }
        if click_macos_claude_debugger_confirmation(app.as_ptr()) {
            append_macos_debugger_log(format!(
                "native Accessibility accepted existing confirmation for pid {pid}"
            ));
        }
        match click_macos_claude_main_process_debugger_menu(app.as_ptr(), &mut observed_titles) {
            Ok(true) => {
                append_macos_debugger_log(format!(
                    "native Accessibility clicked Claude debugger menu for pid {pid} on attempt {attempt}"
                ));
                for _ in 0..30 {
                    if click_macos_claude_debugger_confirmation(app.as_ptr()) {
                        append_macos_debugger_log(format!(
                            "native Accessibility accepted Claude debugger confirmation for pid {pid}"
                        ));
                    }
                    if claude_node_inspector_available() {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                return Ok(());
            }
            Ok(false) => {
                thread::sleep(Duration::from_millis(250));
            }
            Err(err) => {
                return Err(err);
            }
        }
    }

    observed_titles.sort();
    observed_titles.dedup();
    if observed_titles.len() > 30 {
        observed_titles.truncate(30);
    }
    Err(format!(
        "Enable Main Process Debugger menu item was not found. Observed menu titles: {}",
        observed_titles.join(", ")
    ))
}

#[cfg(target_os = "macos")]
fn macos_claude_process_ids() -> Result<Vec<u32>, String> {
    let output = hidden_command("pgrep")
        .args(["-x", "Claude"])
        .output()
        .map_err(|err| format!("Failed to find Claude process: {err}"))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }

    let mut preferred = Vec::new();
    let mut fallback = Vec::new();
    for pid in String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
    {
        let command = macos_process_command_for_pid(pid);
        if command
            .as_deref()
            .map(|value| value.contains("Claude.app/Contents/MacOS/Claude"))
            .unwrap_or(false)
        {
            preferred.push(pid);
        } else {
            fallback.push(pid);
        }
    }
    preferred.extend(fallback);
    Ok(preferred)
}

#[cfg(target_os = "macos")]
fn macos_process_command_for_pid(pid: u32) -> Option<String> {
    let pid = pid.to_string();
    let output = hidden_command("ps")
        .args(["-p", &pid, "-o", "command="])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn click_macos_claude_main_process_debugger_menu(
    app: AXUIElementRef,
    observed_titles: &mut Vec<String>,
) -> Result<bool, String> {
    let Some(menu_bar) = ax_copy_attribute(app, "AXMenuBar")? else {
        return Err("Claude AX menu bar was not available.".to_string());
    };
    let children = ax_children(menu_bar.as_ptr());
    for child in &children {
        if let Some(title) = ax_title(child.as_ptr() as AXUIElementRef) {
            if !title.is_empty() {
                observed_titles.push(title.clone());
            }
            if macos_developer_menu_title_matches(&title) {
                ax_press(child.as_ptr() as AXUIElementRef)?;
                thread::sleep(Duration::from_millis(150));
                if ax_find_and_press_debugger_menu_item(
                    child.as_ptr() as AXUIElementRef,
                    6,
                    observed_titles,
                )? {
                    return Ok(true);
                }
            }
        }
    }

    for child in children {
        ax_press(child.as_ptr() as AXUIElementRef)?;
        thread::sleep(Duration::from_millis(80));
        if ax_find_and_press_debugger_menu_item(
            child.as_ptr() as AXUIElementRef,
            6,
            observed_titles,
        )? {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(target_os = "macos")]
fn click_macos_claude_debugger_confirmation(app: AXUIElementRef) -> bool {
    ax_find_and_press_matching(app, 6, &mut Vec::new(), |title| {
        macos_debugger_confirmation_title_matches(title)
    })
    .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn ax_find_and_press_debugger_menu_item(
    element: AXUIElementRef,
    depth: usize,
    observed_titles: &mut Vec<String>,
) -> Result<bool, String> {
    ax_find_and_press_matching(element, depth, observed_titles, |title| {
        macos_main_process_debugger_menu_title_matches(title)
    })
}

#[cfg(target_os = "macos")]
fn ax_find_and_press_matching(
    element: AXUIElementRef,
    depth: usize,
    observed_titles: &mut Vec<String>,
    matches: impl Copy + Fn(&str) -> bool,
) -> Result<bool, String> {
    if depth == 0 || element.is_null() {
        return Ok(false);
    }
    if let Some(title) = ax_title(element) {
        if !title.is_empty() {
            observed_titles.push(title.clone());
        }
        if matches(&title) {
            ax_press(element)?;
            return Ok(true);
        }
    }
    for child in ax_children(element) {
        if ax_find_and_press_matching(
            child.as_ptr() as AXUIElementRef,
            depth - 1,
            observed_titles,
            matches,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(any(target_os = "macos", test))]
fn macos_developer_menu_title_matches(title: &str) -> bool {
    let normalized = normalized_menu_title(title);
    if normalized.is_empty() {
        return false;
    }

    normalized_title_equals_any(
        &normalized,
        &[
            "Developer",
            "开发者",
            "開發者",
            "Entwickler",
            "Desarrollador",
            "Développeur",
            "Developpeur",
            "डेवलपर",
            "Pengembang",
            "Sviluppatore",
            "開発",
            "開発者",
            "개발자",
            "Desenvolvedor",
        ],
    )
}

#[cfg(any(target_os = "macos", test))]
fn macos_main_process_debugger_menu_title_matches(title: &str) -> bool {
    let normalized = normalized_menu_title(title);
    if normalized.is_empty() {
        return false;
    }

    normalized_title_contains_any(&normalized, macos_main_process_debugger_menu_titles())
}

#[cfg(any(target_os = "macos", test))]
fn macos_debugger_confirmation_title_matches(title: &str) -> bool {
    let normalized = normalized_menu_title(title);
    if normalized.is_empty() {
        return false;
    }
    if normalized_title_contains_any(&normalized, macos_main_process_debugger_menu_titles()) {
        return true;
    }

    normalized_title_equals_any(
        &normalized,
        &[
            "Enable",
            "启用",
            "啟用",
            "Continue",
            "继续",
            "繼續",
            "Allow",
            "允许",
            "允許",
            "Open",
            "打开",
            "打開",
            "OK",
            "好",
            "确定",
            "確認",
            "Activer",
            "Continuer",
            "Autoriser",
            "Ouvrir",
            "Aktivieren",
            "Fortfahren",
            "Erlauben",
            "Öffnen",
            "Offnen",
            "Activar",
            "Habilitar",
            "Continuar",
            "Permitir",
            "Abrir",
            "Aceptar",
            "Ativar",
            "Aceitar",
            "Abilita",
            "Attiva",
            "Continua",
            "Consenti",
            "Apri",
            "有効にする",
            "続ける",
            "許可",
            "開く",
            "はい",
            "활성화",
            "계속",
            "허용",
            "열기",
            "확인",
            "सक्षम करें",
            "जारी रखें",
            "अनुमति दें",
            "खोलें",
            "ठीक",
            "Aktifkan",
            "Lanjutkan",
            "Izinkan",
            "Buka",
        ],
    )
}

#[cfg(any(target_os = "macos", test))]
fn macos_main_process_debugger_menu_titles() -> &'static [&'static str] {
    const TITLES: &[&str] = &[
        "Enable Main Process Debugger",
        "Main Process Debugger",
        "启用主进程调试器",
        "主进程调试器",
        "啟用主進程偵錯器",
        "主進程偵錯器",
        "啟用主行程偵錯器",
        "主行程偵錯器",
        "啟用主程序偵錯器",
        "主程序偵錯器",
        "Activer le débogueur du processus principal",
        "Débogueur du processus principal",
        "Activer le debogueur du processus principal",
        "Debogueur du processus principal",
        "Hauptprozess-Debugger aktivieren",
        "Hauptprozess-Debugger",
        "Depurador del proceso principal",
        "Activar depurador del proceso principal",
        "Habilitar depurador del proceso principal",
        "Depurador do processo principal",
        "Ativar depurador do processo principal",
        "Debugger processo principale",
        "Abilita debugger processo principale",
        "Attiva debugger processo principale",
        "メインプロセスデバッガーを有効にする",
        "メインプロセスデバッガー",
        "メインプロセスデバッガ",
        "메인 프로세스 디버거 활성화",
        "메인 프로세스 디버거",
        "मुख्य प्रक्रिया डिबगर सक्षम करें",
        "मुख्य प्रक्रिया डिबगर",
        "Aktifkan debugger proses utama",
        "Debugger proses utama",
    ];
    TITLES
}

#[cfg(any(target_os = "macos", test))]
fn normalized_title_contains_any(normalized_title: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| {
        let normalized_candidate = normalized_menu_title(candidate);
        !normalized_candidate.is_empty() && normalized_title.contains(&normalized_candidate)
    })
}

#[cfg(any(target_os = "macos", test))]
fn normalized_title_equals_any(normalized_title: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| {
        let normalized_candidate = normalized_menu_title(candidate);
        !normalized_candidate.is_empty() && normalized_title == normalized_candidate
    })
}

#[cfg(any(target_os = "macos", test))]
fn normalized_menu_title(title: &str) -> String {
    title
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| ch.is_alphanumeric() || is_cjk_char(*ch))
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn is_cjk_char(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}

#[cfg(target_os = "macos")]
fn ax_copy_attribute(element: AXUIElementRef, attribute: &str) -> Result<Option<OwnedCf>, String> {
    if element.is_null() {
        return Ok(None);
    }
    with_cf_string(attribute, |attribute_ref| {
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe { AXUIElementCopyAttributeValue(element, attribute_ref, &mut value) };
        if error == AX_ERROR_SUCCESS {
            Ok(OwnedCf::new(value))
        } else {
            Ok(None)
        }
    })
}

#[cfg(target_os = "macos")]
fn ax_children(element: AXUIElementRef) -> Vec<OwnedCf> {
    let Ok(Some(children)) = ax_copy_attribute(element, "AXChildren") else {
        return Vec::new();
    };
    if !cf_is_array(children.as_ptr()) {
        return Vec::new();
    }
    let count = unsafe { CFArrayGetCount(children.as_ptr() as CFArrayRef) }
        .min(MACOS_AX_MAX_CHILDREN_PER_NODE);
    let mut result = Vec::new();
    for index in 0..count {
        let child = unsafe { CFArrayGetValueAtIndex(children.as_ptr() as CFArrayRef, index) };
        if !child.is_null() {
            if let Some(retained_child) = unsafe { OwnedCf::new(CFRetain(child)) } {
                result.push(retained_child);
            }
        }
    }
    result
}

#[cfg(target_os = "macos")]
fn ax_title(element: AXUIElementRef) -> Option<String> {
    let Ok(Some(title)) = ax_copy_attribute(element, "AXTitle") else {
        return None;
    };
    cf_string_to_string(title.as_ptr())
}

#[cfg(target_os = "macos")]
fn ax_set_frontmost(element: AXUIElementRef) {
    with_cf_string("AXFrontmost", |attribute| {
        let _ = unsafe { AXUIElementSetAttributeValue(element, attribute, kCFBooleanTrue) };
    });
}

#[cfg(target_os = "macos")]
fn ax_press(element: AXUIElementRef) -> Result<(), String> {
    with_cf_string("AXPress", |action| {
        let error = unsafe { AXUIElementPerformAction(element, action) };
        if error == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("AXPress failed with error {error}"))
        }
    })
}

#[cfg(target_os = "macos")]
fn with_cf_string<T>(value: &str, f: impl FnOnce(CFStringRef) -> T) -> T {
    let c_string = CString::new(value).expect("AX constant should not contain NUL");
    let cf_string = unsafe {
        CFStringCreateWithCString(
            std::ptr::null(),
            c_string.as_ptr(),
            K_CF_STRING_ENCODING_UTF8,
        )
    };
    let result = f(cf_string);
    if !cf_string.is_null() {
        unsafe { CFRelease(cf_string) };
    }
    result
}

#[cfg(target_os = "macos")]
fn cf_is_array(value: CFTypeRef) -> bool {
    !value.is_null() && unsafe { CFGetTypeID(value) == CFArrayGetTypeID() }
}

#[cfg(target_os = "macos")]
fn cf_is_string(value: CFTypeRef) -> bool {
    !value.is_null() && unsafe { CFGetTypeID(value) == CFStringGetTypeID() }
}

#[cfg(target_os = "macos")]
fn cf_string_to_string(value: CFTypeRef) -> Option<String> {
    if !cf_is_string(value) {
        return None;
    }
    let mut buffer = vec![0 as c_char; 4096];
    let ok = unsafe {
        CFStringGetCString(
            value as CFStringRef,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        )
    };
    if ok == 0 {
        return None;
    }
    let bytes = buffer
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    String::from_utf8(bytes).ok()
}

fn macos_localized_launch_script() -> String {
    r#"#!/bin/sh
set -eu
mkdir -p "$HOME/.codestudio-lite/claude-desktop-patch"
printf 'zh-CN' > "$HOME/.codestudio-lite/claude-desktop-patch/__CLAUDE_LOCALIZED_LAUNCH_MARKER__"
if /usr/bin/pgrep -x Claude >/dev/null 2>&1; then
  /usr/bin/pkill -TERM -x Claude >/dev/null 2>&1 || true
fi
for _ in 1 2 3 4 5 6 7 8 9 10; do
  /usr/bin/pgrep -x Claude >/dev/null 2>&1 || break
  /bin/sleep 0.25
done
if /usr/bin/pgrep -x Claude >/dev/null 2>&1; then
  /usr/bin/pkill -KILL -x Claude >/dev/null 2>&1 || true
  /bin/sleep 0.5
fi
/usr/bin/open -a Claude
/bin/sleep 2
deadline=$(( $(/bin/date +%s) + 90 ))
debugger_attempts=0
claude_debugger_open() {
  port=__CLAUDE_NODE_INSPECT_PORT__
  /usr/bin/curl -fsS --max-time 1 "http://127.0.0.1:${port}/json" 2>/dev/null | /usr/bin/grep -E '"webSocketDebuggerUrl"[[:space:]]*:[[:space:]]*"ws://127\.0\.0\.1:' >/dev/null || return 1
  pids=$(/usr/sbin/lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true)
  for pid in $pids; do
    args=$(/bin/ps -p "$pid" -o args= 2>/dev/null || true)
    case "$args" in
      *"Claude.app/Contents/MacOS/Claude"*) return 0 ;;
    esac
  done
  return 1
}
while ! claude_debugger_open; do
  if [ "$(/bin/date +%s)" -ge "$deadline" ]; then
    echo "[claude-zh] Timed out waiting for Claude main process debugger. Grant CodeStudio Lite Accessibility permission in System Settings, then retry." >&2
    exit 1
  fi
  debugger_attempts=$((debugger_attempts + 1))
  echo "[claude-zh] Waiting for CodeStudio Lite to enable Claude main process debugger via Accessibility (#$debugger_attempts)..." >&2
  for _ in 1 2 3 4 5; do
    claude_debugger_open && break 2
    /bin/sleep 1
  done
done
echo "[claude-zh] Claude main process debugger is ready." >&2
"#
    .replace(
        "__CLAUDE_LOCALIZED_LAUNCH_MARKER__",
        CLAUDE_LOCALIZED_LAUNCH_MARKER,
    )
    .replace(
        "__CLAUDE_NODE_INSPECT_PORT__",
        &CLAUDE_NODE_INSPECT_PORT.to_string(),
    )
}

#[cfg(target_os = "macos")]
fn launch_macos_claude_desktop_plain_restart() -> Result<(), String> {
    hidden_command("sh")
        .arg("-c")
        .arg(macos_plain_launch_script())
        .spawn()
        .map_err(|err| format!("Failed to restart Claude Desktop: {err}"))
        .map(|_| ())
}

#[cfg(any(target_os = "macos", test))]
fn macos_plain_launch_script() -> String {
    r#"set -eu
if /usr/bin/pgrep -x Claude >/dev/null 2>&1; then
  /usr/bin/pkill -TERM -x Claude >/dev/null 2>&1 || true
fi
for _ in 1 2 3 4 5 6 7 8 9 10; do
  /usr/bin/pgrep -x Claude >/dev/null 2>&1 || break
  /bin/sleep 0.25
done
if /usr/bin/pgrep -x Claude >/dev/null 2>&1; then
  /usr/bin/pkill -KILL -x Claude >/dev/null 2>&1 || true
  /bin/sleep 0.5
fi
if /usr/bin/pgrep -x Claude >/dev/null 2>&1; then
  echo "Claude Desktop is still running; restart was not continued." >&2
  exit 1
fi
/usr/bin/open -a Claude
"#
    .to_string()
}

fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "windows")]
fn find_windows_claude_exe() -> Option<PathBuf> {
    windows_claude_exe_candidates()
        .into_iter()
        .find(|path| path.is_file())
}

#[cfg(target_os = "windows")]
fn windows_claude_exe_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        push_windows_claude_local_candidates(&mut candidates, Path::new(&local_app_data));
    }
    if let Ok(paths) = app_paths() {
        push_windows_claude_local_candidates(
            &mut candidates,
            &paths.home_dir.join("AppData").join("Local"),
        );
    }
    if let Some(program_files) = env::var_os("ProgramFiles") {
        push_windows_claude_program_files_candidates(&mut candidates, Path::new(&program_files));
    }
    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        push_windows_claude_program_files_candidates(
            &mut candidates,
            Path::new(&program_files_x86),
        );
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

#[cfg(target_os = "windows")]
fn push_windows_claude_local_candidates(candidates: &mut Vec<PathBuf>, root: &Path) {
    candidates.push(root.join("Programs").join("Claude").join("Claude.exe"));
    candidates.push(root.join("Claude").join("Claude.exe"));
    // Native electron-builder/NSIS installer (winget's Anthropic.Claude on a
    // clean VM). Match the detector's candidate set so localized launch
    // resolves the same install detection finds.
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

#[cfg(target_os = "windows")]
fn push_windows_claude_program_files_candidates(candidates: &mut Vec<PathBuf>, root: &Path) {
    candidates.push(root.join("Claude").join("Claude.exe"));
    candidates.push(root.join("Anthropic").join("Claude").join("Claude.exe"));
}

fn windows_shell_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn windows_launch_script(localize: bool) -> String {
    let args = "$argsList = @()";
    let localized_marker = if localize {
        r#"$markerDir = Join-Path $HOME '.codestudio-lite\claude-desktop-patch'
New-Item -ItemType Directory -Force -Path $markerDir | Out-Null
Set-Content -LiteralPath (Join-Path $markerDir 'localized-launch.flag') -Value 'zh-CN' -Encoding ASCII
"#
    } else {
        ""
    };
    // Claude 已 fuse 掉 inspect 启动参数；Windows 本地化启动只正常
    // 激活官方 App，主进程调试器由后续菜单自动化开启。
    let msix_launch = r#"
  $target = 'shell:AppsFolder\' + $pkg.PackageFamilyName + '!' + $appId
  Start-Process -FilePath $target
"#;
    format!(
        r#"$ErrorActionPreference = 'Stop'
$pkgNames = @('Claude', 'Anthropic.Claude')
{localized_marker}$pkg = Get-AppxPackage | Where-Object {{ $pkgNames -contains $_.Name -or $_.PackageFullName -match 'Claude' }} | Sort-Object -Property Version -Descending | Select-Object -First 1
if (-not $pkg -and $env:ProgramFiles) {{
  $manifest = Get-ChildItem -Path (Join-Path $env:ProgramFiles 'WindowsApps\Claude_*_x64__pzs8sxrjxfjjc\AppxManifest.xml') -ErrorAction SilentlyContinue |
    Sort-Object -Property LastWriteTime -Descending |
    Select-Object -First 1
  if ($manifest) {{
    Add-AppxPackage -Register $manifest.FullName -DisableDevelopmentMode -ForceApplicationShutdown -ErrorAction Stop
    $pkg = Get-AppxPackage | Where-Object {{ $pkgNames -contains $_.Name -or $_.PackageFullName -match 'Claude' }} | Sort-Object -Property Version -Descending | Select-Object -First 1
  }}
}}
if ($pkg) {{
  $app = (Get-AppxPackageManifest $pkg).Package.Applications.Application
  if ($app -is [array]) {{ $app = $app[0] }}
  $appId = [string]$app.Id
  if (-not $appId) {{ $appId = 'App' }}
  {args}
  {msix_launch}
  exit 0
}}
$cmd = Get-Command Claude -ErrorAction SilentlyContinue
if (-not $cmd -or -not $cmd.Source -or -not (Test-Path -LiteralPath $cmd.Source)) {{
  throw 'Claude Desktop executable was not found.'
}}
$exe = [string]$cmd.Source
if ($exe.IndexOf('\WindowsApps\', [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {{
  throw 'Claude Desktop MSIX package was found only as a raw WindowsApps executable; app identity activation is required.'
}}
{args}
Start-Process -FilePath $exe -ArgumentList $argsList
"#,
        localized_marker = localized_marker,
        args = args,
        msix_launch = msix_launch,
    )
}

fn write_claude_locale_payloads(patch_dir: &Path) -> Result<(), String> {
    write_locale_payload(
        &patch_dir.join(CLAUDE_SHELL_ZH_LOCALE_FILE),
        CLAUDE_SHELL_ZH_LOCALE,
    )?;
    write_locale_payload(
        &patch_dir.join(Path::new(CLAUDE_ION_ZH_LOCALE_RELATIVE_PATH)),
        CLAUDE_ION_ZH_LOCALE,
    )?;
    write_locale_payload(
        &patch_dir.join(Path::new(CLAUDE_ION_DYNAMIC_ZH_LOCALE_RELATIVE_PATH)),
        CLAUDE_ION_DYNAMIC_ZH_LOCALE,
    )?;
    Ok(())
}

fn write_locale_payload(target_path: &Path, content: &str) -> Result<(), String> {
    serde_json::from_str::<Value>(content).map_err(|err| {
        format!(
            "Bundled Claude locale {} is invalid: {err}",
            target_path.display()
        )
    })?;
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create {}: {err}", parent.display()))?;
    }
    write_if_changed(target_path, content)
}

fn inject_localization() -> Result<usize, String> {
    // The inspector is opened through Claude's official "Developer -> Enable
    // Main Process Debugger" route; once it is available, scan and inject.
    ensure_patch_files()?;
    inject_localization_via_node_inspector()
}

fn inject_localization_via_node_inspector() -> Result<usize, String> {
    let patch_dir = ensure_patch_files()?;
    let source = build_main_process_injection_source(&patch_dir);
    let targets = read_node_inspector_targets()?;
    let mut injected = 0;
    for target in targets {
        let Some(ws_url) = target
            .get("webSocketDebuggerUrl")
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        if !is_claude_node_inspector(ws_url)? {
            continue;
        }
        injected += evaluate_node_inspector_expression(ws_url, &source)?;
    }
    Ok(injected)
}

fn build_main_process_injection_source(patch_dir: &Path) -> String {
    build_main_process_injection_source_for_paths(
        &patch_dir.join("translation-runtime.js"),
        &patch_dir.join(CLAUDE_SHELL_ZH_LOCALE_FILE),
        &patch_dir.join(Path::new(CLAUDE_ION_ZH_LOCALE_RELATIVE_PATH)),
        &patch_dir.join(Path::new(CLAUDE_ION_DYNAMIC_ZH_LOCALE_RELATIVE_PATH)),
    )
}

fn build_main_process_injection_source_for_paths(
    runtime_path: &Path,
    shell_locale_path: &Path,
    ion_locale_path: &Path,
    dynamic_locale_path: &Path,
) -> String {
    let runtime_path = serde_json::to_string(&runtime_path.to_string_lossy()).unwrap();
    let shell_locale_path = serde_json::to_string(&shell_locale_path.to_string_lossy()).unwrap();
    let ion_locale_path = serde_json::to_string(&ion_locale_path.to_string_lossy()).unwrap();
    let dynamic_locale_path =
        serde_json::to_string(&dynamic_locale_path.to_string_lossy()).unwrap();
    let injection_signature = serde_json::to_string(&main_process_injection_signature()).unwrap();
    format!(
        r##"(async () => {{
  const CSL_INJECTION_VERSION = 9;
  const CSL_INJECTION_SIGNATURE = {injection_signature};
  if (globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__?.version === CSL_INJECTION_VERSION &&
      globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__?.injectionSignature === CSL_INJECTION_SIGNATURE) {{
    const summary = await globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__.refresh();
    return {{ ok: true, reused: true, ...summary }};
  }}
  if (globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__) {{
    try {{ globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__.dispose?.(); }} catch (_) {{}}
  }}

  const requireFromMain = process.getBuiltinModule("module").createRequire(process.execPath);
  const fs = requireFromMain("fs");
  const electron = requireFromMain("electron");
  const BrowserWindow = electron.BrowserWindow;
  const webContents = electron.webContents;
  const app = electron.app;
  const runtime = fs.readFileSync({runtime_path}, "utf8");
  const shellLocale = fs.readFileSync({shell_locale_path}, "utf8");
  const ionLocale = fs.readFileSync({ion_locale_path}, "utf8");
  const dynamicLocale = fs.readFileSync({dynamic_locale_path}, "utf8");
  const attached = new Set();
  const localizedLaunchMarkerPath = () => {{
    try {{ return requireFromMain("path").join(requireFromMain("os").homedir(), ".codestudio-lite", "claude-desktop-patch", "localized-launch.flag"); }} catch (_) {{ return ""; }}
  }};
  const consumeLocalizedLaunchMarker = () => {{
    try {{
      const marker = localizedLaunchMarkerPath();
      if (!marker) return false;
      let text = "";
      try {{ text = fs.readFileSync(marker, "utf8"); }} catch (_) {{ return false; }}
      try {{ fs.unlinkSync(marker); }} catch (_) {{}}
      return String(text || "").trim() === "zh-CN";
    }} catch (_) {{
      return false;
    }}
  }};
  let localizedLaunchDefaultZh = consumeLocalizedLaunchMarker();
  let currentLocale = localizedLaunchDefaultZh ? "zh-CN" : "en-US";
  const CSL_WANTED_LOCALE_KEY = "__cslWantedLocale";
  const localeChangeListeners = [];
  const fireLocaleChange = (loc) => {{
    for (const listener of localeChangeListeners) {{
      try {{ listener(loc); }} catch (_) {{}}
    }}
  }};
  const setCurrentLocale = (loc) => {{
    if (typeof loc !== "string" || !loc || loc === currentLocale) return;
    currentLocale = loc;
    fireLocaleChange(loc);
  }};
  const zhActive = () => currentLocale === "zh-CN";
  const runtimeLaunchPrefix = () => "var __CSL_LL=" + (currentLocale === "zh-CN" ? "!0" : "!1") + ";if(__CSL_LL&&!sessionStorage.getItem('__CSL_LL_DONE'))try{{localStorage.setItem('__cslWantedLocale','zh-CN');localStorage.setItem('spa:locale','zh-CN');document.documentElement&&document.documentElement.setAttribute('lang','zh-CN');sessionStorage.setItem('__CSL_LL_DONE','1')}}catch(e){{}};";
  const patterns = [
    {{ urlPattern: "*ion-dist/i18n/zh-CN.json*" }},
    {{ urlPattern: "*ion-dist/i18n/en-US.json*" }},
    {{ urlPattern: "*/i18n/zh-CN.json*" }},
    {{ urlPattern: "*/i18n/en-US.json*" }},
    {{ urlPattern: "*/zh-CN.json*" }}
  ];

  const localePayloadForUrl = (url) => {{
    const normalized = String(url || "").replaceAll("\\", "/");
    const bare = normalized.split("?")[0].split("#")[0].toLowerCase();
    const isZh = bare.endsWith("/zh-cn.json");
    const isEn = bare.endsWith("/en-us.json");
    const localLike = bare.startsWith("app://") || bare.startsWith("file://");
    if (!isZh && !(currentLocale === "zh-CN" && isEn && localLike)) return null;
    if (bare.includes("/dynamic/")) return dynamicLocale;
    if (bare.includes("/ion-dist/i18n/") || bare.includes("/i18n/")) return ionLocale;
    return shellLocale;
  }};

  const attach = async (contents) => {{
    if (!contents || contents.isDestroyed?.()) return false;
    if (attached.has(contents)) return true;
    const url = contents.getURL?.() ?? "";
    if (!url || (!url.startsWith("http://") && !url.startsWith("https://") && !url.startsWith("app://") && !url.startsWith("file://"))) return false;
    const previousVersion = contents.__cslZhAttachedVersion || (contents.__cslZhAttached ? 1 : 0);
    const previousInjectionSignature = contents.__cslZhAttachedInjectionSignature || "";
    let debuggerWasAttached = false;
    try {{ debuggerWasAttached = contents.debugger.isAttached(); }} catch (_) {{}}
    if ((previousVersion !== CSL_INJECTION_VERSION || previousInjectionSignature !== CSL_INJECTION_SIGNATURE) &&
        (previousVersion || previousInjectionSignature || debuggerWasAttached)) {{
      try {{ contents.debugger.removeAllListeners("message"); }} catch (_) {{}}
      try {{ if (contents.debugger.isAttached()) contents.debugger.detach(); }} catch (_) {{}}
      try {{ contents.__cslZhAttached = false; contents.__cslZhAttachedVersion = 0; contents.__cslZhAttachedInjectionSignature = ""; }} catch (_) {{}}
    }}
    try {{
      if (!contents.debugger.isAttached()) {{
        contents.debugger.attach("1.3");
      }}
    }} catch (_) {{
      return false;
    }}
    contents.debugger.on("message", (_event, method, params) => {{
      if (method !== "Fetch.requestPaused") return;
      const requestId = params?.requestId;
      if (!requestId) return;
      const url = params?.request?.url;
      const payload = localePayloadForUrl(url);
      if (payload) {{
        contents.debugger.sendCommand("Fetch.fulfillRequest", {{
          requestId,
          responseCode: 200,
          responseHeaders: [
            {{ name: "Content-Type", value: "application/json; charset=utf-8" }},
            {{ name: "Cache-Control", value: "no-store" }},
            {{ name: "Access-Control-Allow-Origin", value: "*" }}
          ],
          body: Buffer.from(payload, "utf8").toString("base64")
        }}).catch(() => {{}});
      }} else {{
        contents.debugger.sendCommand("Fetch.continueRequest", {{ requestId }}).catch(() => {{}});
      }}
    }});
    try {{
      await contents.debugger.sendCommand("Page.enable", {{}});
      await contents.debugger.sendCommand("Fetch.enable", {{ patterns }});
      await contents.debugger.sendCommand("Page.addScriptToEvaluateOnNewDocument", {{ source: runtimeLaunchPrefix() + runtime }});
      // Reload so the runtime registered via addScriptToEvaluateOnNewDocument
      // runs before the page's own scripts and rewrites locale fetches. Do
      // not `await contents.executeJavaScript(runtime)` here: the reload that
      // follows would unload the page and leave that promise pending forever,
      // stalling the whole async injection (which then blocks the Rust
      // inspector read loop with no timeout). addScriptToEvaluateOnNewDocument
      // is the durable path that survives the reload.
      const withTimeout = (promise, ms) => Promise.race([
        promise,
        new Promise((resolve) => setTimeout(() => resolve(undefined), ms)),
      ]);
      await withTimeout(contents.debugger.sendCommand("Page.reload", {{ ignoreCache: true }}), 2000);
      contents.__cslZhAttached = true;
      contents.__cslZhAttachedVersion = CSL_INJECTION_VERSION;
      contents.__cslZhAttachedInjectionSignature = CSL_INJECTION_SIGNATURE;
      attached.add(contents);
      return true;
    }} catch (_) {{
      return false;
    }}
  }};

  const allContents = () => {{
    const fromWebContents = typeof webContents?.getAllWebContents === "function"
      ? webContents.getAllWebContents()
      : [];
    const fromWindows = BrowserWindow.getAllWindows().map((window) => window.webContents);
    return Array.from(new Set([...fromWindows, ...fromWebContents]));
  }};

  const safeLocaleForLocalWindow = (loc) => {{
    if (typeof loc !== "string" || !loc) return "en-US";
    if (loc === "zh-CN") return loc;
    try {{
      const path = requireFromMain("path");
      if (fs.existsSync(path.join(process.resourcesPath, loc + ".json"))) return loc;
      if (fs.existsSync(path.join(process.resourcesPath, "ion-dist", "i18n", loc + ".json"))) return loc;
    }} catch (_) {{}}
    return "en-US";
  }};

  const isSyncableUrl = (lower) =>
    lower.startsWith("http://") ||
    lower.startsWith("https://") ||
    lower.startsWith("app://") ||
    lower.startsWith("file://") ||
    lower.startsWith("about:blank") ||
    lower.startsWith("devtools://");

  const localLocalePage = (lower) =>
    lower.startsWith("app://") ||
    lower.includes("/settings") ||
    lower.includes("setup") ||
    lower.includes("third-party") ||
    lower.includes("inference") ||
    lower.includes("developer") ||
    lower.includes("about_window");

  const localWindowHotSwitchSync = true;
  const devToolsWindowTitleSync = true;
  const devToolsPage = (lower) => lower.startsWith("devtools://") || lower.startsWith("chrome-devtools://");
  const aboutClaudeWindowFallback = true;
  const aboutClaudeTitle = (target) => target === "zh-CN" ? "\u5173\u4e8eClaude" : "About Claude";
  const aboutClaudePage = (lower) => lower.includes("about_window");
  const aboutClaudeTitleActive = (title) => {{
    const t = String(title || "").trim();
    return t === "About Claude" || t === "\u5173\u4e8eClaude" || t === "\u5173\u4e8e Claude";
  }};
  const localTitleForUrl = (lower, target) => {{
    if (aboutClaudePage(lower)) return aboutClaudeTitle(target);
    if (lower.includes("setup-desktop-3p")) return target === "zh-CN" ? "\u914d\u7f6e\u7b2c\u4e09\u65b9\u0041\u0050\u0049" : "Configure Third-Party Inference\u2026";
    if (devToolsPage(lower)) return target === "zh-CN" ? "\u5f00\u53d1\u8005\u5de5\u5177" : "DevTools";
    return "";
  }};
  const localTitleForWindow = (lower, target, currentTitle) => {{
    if (aboutClaudePage(lower) || aboutClaudeTitleActive(currentTitle)) return aboutClaudeTitle(target);
    return localTitleForUrl(lower, target);
  }};
  const applyLocalWindowTitle = (contents, target, lower) => {{
    try {{
      let win = null;
      try {{
        win = BrowserWindow.fromWebContents?.(contents);
      }} catch (_) {{}}
      let currentTitle = "";
      try {{
        if (win && typeof win.getTitle === "function") currentTitle = win.getTitle();
        else if (typeof contents?.getTitle === "function") currentTitle = contents.getTitle();
      }} catch (_) {{}}
      const title = localTitleForWindow(lower, target, currentTitle);
      if (!title) return;
      try {{
        if (win && typeof win.getTitle === "function" && typeof win.setTitle === "function" && win.getTitle() !== title) win.setTitle(title);
      }} catch (_) {{}}
      if (devToolsPage(lower) || aboutClaudePage(lower)) {{
        const quotedTitle = JSON.stringify(title);
        contents.executeJavaScript('try{{if(document.title!==' + quotedTitle + ')document.title=' + quotedTitle + '}}catch(e){{}}', true).catch(() => {{}});
      }}
    }} catch (_) {{}}
  }};

  const syncOneWindowLocale = (contents, target) => {{
    try {{
      if (!contents || contents.isDestroyed?.()) return;
      const url = contents.getURL?.() ?? "";
      const lower = String(url || "").toLowerCase();
      applyLocalWindowTitle(contents, target, lower);
      if (devToolsPage(lower)) return;
      if (!isSyncableUrl(lower)) return;
      const localPage = localLocalePage(lower);
      const localLike = localPage || lower.startsWith("file://") || lower.startsWith("about:blank");
      const remoteClaude = lower.startsWith("https://claude.ai") || lower.startsWith("http://claude.ai");
      if (remoteClaude && !localPage) return;
      const loc = localLike ? safeLocaleForLocalWindow(target) : target;
      const quoted = JSON.stringify(loc);
      const js = 'try{{localStorage.setItem("__cslWantedLocale",' + quoted + ');localStorage.setItem("spa:locale",' + quoted + ');document.documentElement&&document.documentElement.setAttribute("lang",' + quoted + ');window.dispatchEvent(new StorageEvent("storage",{{key:"spa:locale",newValue:' + quoted + '}}));window.dispatchEvent(new CustomEvent("claude-locale-change",{{detail:' + quoted + '}}));true}}catch(e){{false}}';
      contents.executeJavaScript(js, true).catch(() => {{}});
      if (localPage && contents.__cslLocaleReloaded !== loc) {{
        contents.__cslLocaleReloaded = loc;
        setTimeout(() => {{
          try {{
            if (contents.isDestroyed?.()) return;
            if (typeof contents.reloadIgnoringCache === "function") contents.reloadIgnoringCache();
            else contents.reload();
          }} catch (_) {{}}
        }}, 80);
      }}
    }} catch (_) {{}}
  }};

  const syncOpenWindowsLocale = (target) => {{
    try {{
      for (const contents of allContents()) syncOneWindowLocale(contents, target);
    }} catch (_) {{}}
  }};
  localeChangeListeners.push(syncOpenWindowsLocale);
  const syncDevToolsTitleLater = (contents, delay = 80) => {{
    try {{
      setTimeout(() => {{
        try {{
          if (!contents || contents.isDestroyed?.()) return;
          const lower = String(contents.getURL?.() || "").toLowerCase();
          if (devToolsPage(lower)) applyLocalWindowTitle(contents, currentLocale, lower);
        }} catch (_) {{}}
      }}, delay);
    }} catch (_) {{}}
  }};
  localeChangeListeners.push(() => {{
    try {{
      for (const contents of allContents()) syncDevToolsTitleLater(contents, 20);
    }} catch (_) {{}}
  }});

  const macosMenuBarLocalization = true;
  const menuHardcodedZh = {{
    "File": "\u6587\u4ef6",
    "Edit": "\u7f16\u8f91",
    "View": "\u89c6\u56fe",
    "Developer": "\u5f00\u53d1\u8005",
    "Help": "\u5e2e\u52a9",
    "New Chat": "\u65b0\u5efa\u804a\u5929",
    "Open MCP Log File...": "\u6253\u5f00 MCP \u65e5\u5fd7\u6587\u4ef6...",
    "Reload MCP Configuration": "\u91cd\u65b0\u52a0\u8f7d MCP \u914d\u7f6e",
    "Open Hardware Buddy\u2026": "\u6253\u5f00 Hardware Buddy\u2026",
    "Configure Third-Party Inference\u2026": "\u914d\u7f6e\u7b2c\u4e09\u65b9\u63a8\u7406\u2026",
    "Extensions": "\u6269\u5c55",
    "Open App Config File...": "\u6253\u5f00\u5e94\u7528\u914d\u7f6e\u6587\u4ef6...",
    "Open Developer Config File...": "\u6253\u5f00\u5f00\u53d1\u8005\u914d\u7f6e\u6587\u4ef6...",
    "Show Dev Tools": "\u663e\u793a\u5f00\u53d1\u8005\u5de5\u5177",
    "Show All Dev Tools": "\u663e\u793a\u6240\u6709\u5f00\u53d1\u8005\u5de5\u5177",
    "Enable Main Process Debugger": "\u542f\u7528\u4e3b\u8fdb\u7a0b\u8c03\u8bd5\u5668",
    "Record Performance Trace": "\u5f55\u5236\u6027\u80fd\u8ddf\u8e2a",
    "Write Main Process Heap Snapshot": "\u5199\u5165\u4e3b\u8fdb\u7a0b\u5806\u5feb\u7167",
    "Record Memory Trace (auto-stop)": "\u5f55\u5236\u5185\u5b58\u8ddf\u8e2a\uff08\u81ea\u52a8\u505c\u6b62\uff09",
    "Paste and Match Style": "\u7c98\u8d34\u5e76\u5339\u914d\u6837\u5f0f",
    "Zoom In (numpad)": "\u653e\u5927\uff08\u5c0f\u952e\u76d8\uff09",
    "Zoom Out (numpad)": "\u7f29\u5c0f\uff08\u5c0f\u952e\u76d8\uff09",
    "Actual Size (numpad)": "\u5b9e\u9645\u5927\u5c0f\uff08\u5c0f\u952e\u76d8\uff09",
    "Hide Claude": "\u9690\u85cf Claude",
    "Hide Others": "\u9690\u85cf\u5176\u4ed6",
    "Show All": "\u5168\u90e8\u663e\u793a",
    "Services": "\u670d\u52a1",
    "Quit Claude": "\u9000\u51fa Claude",
    "Minimize": "\u6700\u5c0f\u5316",
    "Bring All to Front": "\u5168\u90e8\u7f6e\u4e8e\u9876\u5c42",
    "Enter Full Screen": "\u8fdb\u5165\u5168\u5c4f",
    "Toggle Developer Tools": "\u5207\u6362\u5f00\u53d1\u8005\u5de5\u5177",
    "Force Reload": "\u5f3a\u5236\u91cd\u65b0\u52a0\u8f7d",
    "Check for Updates\u2026": "\u68c0\u67e5\u66f4\u65b0\u2026",
    "Show App": "\u663e\u793a\u5e94\u7528\u754c\u9762",
    "Show Claude": "\u663e\u793a Claude",
    "Open Claude": "\u6253\u5f00 Claude",
    "Quit Claude": "\u9000\u51fa Claude",
    "Quit": "\u9000\u51fa",
    "Settings": "\u8bbe\u7f6e",
    "Preferences": "\u504f\u597d\u8bbe\u7f6e"
  }};
  const installMacosMenuLocalization = () => {{
    try {{
      if (process.platform !== "darwin") return;
      const Menu = electron.Menu;
      if (!Menu || Menu.__cslMenuBarLocalizationInstalled) return;
      let zhHardcodedToEn = {{}};
      for (const key in menuHardcodedZh) zhHardcodedToEn[menuHardcodedZh[key]] = key;
      const menuRoleZh = {{
        about: "\u5173\u4e8eClaude",
        services: "\u670d\u52a1",
        hide: "\u9690\u85cf Claude",
        hideothers: "\u9690\u85cf\u5176\u4ed6",
        unhide: "\u5168\u90e8\u663e\u793a",
        quit: "\u9000\u51fa Claude",
        undo: "\u64a4\u9500",
        redo: "\u91cd\u505a",
        cut: "\u526a\u5207",
        copy: "\u590d\u5236",
        paste: "\u7c98\u8d34",
        pasteandmatchstyle: "\u7c98\u8d34\u5e76\u5339\u914d\u6837\u5f0f",
        delete: "\u5220\u9664",
        selectall: "\u5168\u9009",
        reload: "\u91cd\u65b0\u52a0\u8f7d",
        forcereload: "\u5f3a\u5236\u91cd\u65b0\u52a0\u8f7d",
        toggledevtools: "\u5207\u6362\u5f00\u53d1\u8005\u5de5\u5177",
        resetzoom: "\u5b9e\u9645\u5927\u5c0f",
        zoomin: "\u653e\u5927",
        zoomout: "\u7f29\u5c0f",
        togglefullscreen: "\u8fdb\u5165\u5168\u5c4f",
        minimize: "\u6700\u5c0f\u5316",
        close: "\u5173\u95ed",
        front: "\u5168\u90e8\u7f6e\u4e8e\u9876\u5c42",
        startspeaking: "\u5f00\u59cb\u8bb2\u8bdd",
        stopspeaking: "\u505c\u6b62\u8bb2\u8bdd"
      }};
      let enToZh = {{}};
      let labelToId = {{}};
      let zhValToId = {{}};
      let zhLocaleObj = {{}};
      const rememberCatalog = (catalog) => {{
        try {{
          for (const key in catalog) {{
            const value = catalog[key];
            if (typeof value === "string" && value && !(value in labelToId)) labelToId[value] = key;
          }}
        }} catch (_) {{}}
      }};
      try {{
        const path = requireFromMain("path");
        const enObj = JSON.parse(fs.readFileSync(path.join(process.resourcesPath, "en-US.json"), "utf8"));
        const zhObj = JSON.parse(shellLocale);
        zhLocaleObj = zhObj;
        for (const key in enObj) if (zhObj[key]) enToZh[enObj[key]] = zhObj[key];
        for (const key in zhObj) if (typeof zhObj[key] === "string" && !(zhObj[key] in zhValToId)) zhValToId[zhObj[key]] = key;
        rememberCatalog(enObj);
        rememberCatalog(zhObj);
        try {{
          for (const name of fs.readdirSync(process.resourcesPath)) {{
            if (!/^[a-z]{{2}}(?:-[A-Z0-9]{{2,4}})?\\.json$/.test(name)) continue;
            if (name === "en-US.json" || name === "zh-CN.json") continue;
            try {{ rememberCatalog(JSON.parse(fs.readFileSync(path.join(process.resourcesPath, name), "utf8"))); }} catch (_) {{}}
          }}
        }} catch (_) {{}}
      }} catch (_) {{}}
      const labelMessageId = (label) => {{
        if (typeof label !== "string" || !label) return "";
        return labelToId[label] || zhValToId[label] || "";
      }};
      const roleKey = (item) => {{
        try {{ return String(item?.role || "").replace(/[^a-z0-9]/gi, "").toLowerCase(); }} catch (_) {{ return ""; }}
      }};
      const menuLabelKey = (label) => {{
        try {{ return String(label || "").replace(/&/g, "").replace(/\t.*$/, "").trim(); }} catch (_) {{ return ""; }}
      }};
      const translateDynamicMenuLabel = (label) => {{
        const key = menuLabelKey(label);
        const restartPrefix = "Restart to update to ";
        if (key.startsWith(restartPrefix)) return "\u91cd\u65b0\u542f\u52a8\u4ee5\u66f4\u65b0\u5230 " + key.slice(restartPrefix.length);
        return "";
      }};
      const loadLocaleCatalog = (target) => {{
        const idToVal = {{}};
        try {{
          if (!target || target === "zh-CN") return idToVal;
          const path = requireFromMain("path");
          const tobj = JSON.parse(fs.readFileSync(path.join(process.resourcesPath, target + ".json"), "utf8"));
          rememberCatalog(tobj);
          for (const key in tobj) if (tobj[key]) idToVal[key] = tobj[key];
        }} catch (_) {{}}
        return idToVal;
      }};
      const translateLabel = (label, id, role) => {{
        if (typeof label !== "string" || !label) return label;
        if (role && menuRoleZh[role]) return menuRoleZh[role];
        const dynamic = translateDynamicMenuLabel(label);
        if (dynamic) return dynamic;
        if (id && enToZh[label]) return enToZh[label];
        if (id && zhLocaleObj[id]) return zhLocaleObj[id];
        if (menuHardcodedZh[label]) return menuHardcodedZh[label];
        if (enToZh[label]) return enToZh[label];
        return label;
      }};
      const translateMenuItems = (menu) => {{
        if (!menu || !menu.items) return menu;
        if (!zhActive()) return menu;
        for (const item of menu.items) {{
          try {{
            if (typeof item.label === "string") {{
              if (item.__cslOrig === undefined) item.__cslOrig = item.label;
              if (item.__cslMessageId === undefined) item.__cslMessageId = labelMessageId(item.__cslOrig) || labelMessageId(item.label);
              item.label = translateLabel(item.__cslOrig, item.__cslMessageId, roleKey(item));
            }}
            if (item.submenu) translateMenuItems(item.submenu);
          }} catch (_) {{}}
        }}
        return menu;
      }};
      const relabelMenuItems = (menu, target, idToVal) => {{
        if (!menu || !menu.items) return;
        for (const item of menu.items) {{
          try {{
            const orig = typeof item.__cslOrig === "string" ? item.__cslOrig : (typeof item.label === "string" ? item.label : "");
            if (typeof item.label === "string" && item.__cslOrig === undefined) item.__cslOrig = orig;
            if (typeof item.label === "string" && item.__cslMessageId === undefined) item.__cslMessageId = labelMessageId(orig) || labelMessageId(item.label);
            if (orig) {{
              if (target === "zh-CN") item.label = translateLabel(orig, item.__cslMessageId, roleKey(item));
              else {{
                const id = item.__cslMessageId || labelMessageId(orig);
                item.label = id && idToVal[id] ? idToVal[id] : (zhHardcodedToEn[orig] || orig);
              }}
            }}
            if (item.submenu) relabelMenuItems(item.submenu, target, idToVal);
          }} catch (_) {{}}
        }}
      }};
      const origSetAppMenu = Menu.setApplicationMenu.bind(Menu);
      Menu.__cslOrigSetApplicationMenu = origSetAppMenu;
      Menu.__cslMenuBarLocalizationInstalled = true;
      Menu.setApplicationMenu = (menu) => {{
        try {{
          if (menu && menu.items) {{
            relabelMenuItems(menu, currentLocale, loadLocaleCatalog(currentLocale));
            translateMenuItems(menu);
            Menu.__cslLastApplicationMenu = menu;
          }}
        }} catch (_) {{}}
        return origSetAppMenu(menu);
      }};
      const retranslateMenuBar = (target) => {{
        try {{
          const menu = Menu.__cslLastApplicationMenu || (typeof Menu.getApplicationMenu === "function" ? Menu.getApplicationMenu() : null);
          if (!menu || !menu.items) return;
          const idToVal = loadLocaleCatalog(target);
          relabelMenuItems(menu, target, idToVal);
          origSetAppMenu(menu);
          Menu.__cslLastApplicationMenu = menu;
        }} catch (_) {{}}
      }};
      localeChangeListeners.push(retranslateMenuBar);
      const currentMenu = typeof Menu.getApplicationMenu === "function" ? Menu.getApplicationMenu() : null;
      if (currentMenu) Menu.setApplicationMenu(currentMenu);
    }} catch (_) {{}}
  }};
  installMacosMenuLocalization();

  const installWindowsMenuPopupLocalization = () => {{
    try {{
      const windowsMenuPopupLocalization = true;
      const windowsTrayMenuLocalization = true;
      if (process.platform === "win32") {{
        const Menu = electron.Menu;
        const Tray = electron.Tray;
        if (!Menu && !Tray) return;
        const zhHardcodedToEn = {{}};
        for (const key in menuHardcodedZh) zhHardcodedToEn[menuHardcodedZh[key]] = key;
        const menuRoleZh = {{
          undo: "\u64a4\u9500",
          redo: "\u91cd\u505a",
          cut: "\u526a\u5207",
          copy: "\u590d\u5236",
          paste: "\u7c98\u8d34",
          pasteandmatchstyle: "\u7c98\u8d34\u5e76\u5339\u914d\u6837\u5f0f",
          delete: "\u5220\u9664",
          selectall: "\u5168\u9009",
          reload: "\u91cd\u65b0\u52a0\u8f7d",
          forcereload: "\u5f3a\u5236\u91cd\u65b0\u52a0\u8f7d",
          toggledevtools: "\u5207\u6362\u5f00\u53d1\u8005\u5de5\u5177",
          resetzoom: "\u5b9e\u9645\u5927\u5c0f",
          zoomin: "\u653e\u5927",
          zoomout: "\u7f29\u5c0f",
          togglefullscreen: "\u8fdb\u5165\u5168\u5c4f",
          minimize: "\u6700\u5c0f\u5316",
          close: "\u5173\u95ed",
          quit: "\u9000\u51fa Claude",
          settings: "\u8bbe\u7f6e"
        }};
        let enToZh = {{}};
        let labelToId = {{}};
        let zhValToId = {{}};
        let zhLocaleObj = {{}};
        const rememberCatalog = (catalog) => {{
          try {{
            for (const key in catalog) {{
              const value = catalog[key];
              if (typeof value === "string" && value && !(value in labelToId)) labelToId[value] = key;
            }}
          }} catch (_) {{}}
        }};
        try {{
          const path = requireFromMain("path");
          const enObj = JSON.parse(fs.readFileSync(path.join(process.resourcesPath, "en-US.json"), "utf8"));
          const zhObj = JSON.parse(shellLocale);
          zhLocaleObj = zhObj;
          for (const key in enObj) if (zhObj[key]) enToZh[enObj[key]] = zhObj[key];
          for (const key in zhObj) if (typeof zhObj[key] === "string" && !(zhObj[key] in zhValToId)) zhValToId[zhObj[key]] = key;
          rememberCatalog(enObj);
          rememberCatalog(zhObj);
          try {{
            for (const name of fs.readdirSync(process.resourcesPath)) {{
              if (!/^[a-z]{{2}}(?:-[A-Z0-9]{{2,4}})?\\.json$/.test(name)) continue;
              if (name === "en-US.json" || name === "zh-CN.json") continue;
              try {{ rememberCatalog(JSON.parse(fs.readFileSync(path.join(process.resourcesPath, name), "utf8"))); }} catch (_) {{}}
            }}
          }} catch (_) {{}}
        }} catch (_) {{}}
        const roleKey = (item) => {{
          try {{ return String(item?.role || "").replace(/[^a-z0-9]/gi, "").toLowerCase(); }} catch (_) {{ return ""; }}
        }};
        const labelKey = (label) => {{
          try {{ return String(label || "").replace(/&/g, "").replace(/\t.*$/, "").trim(); }} catch (_) {{ return ""; }}
        }};
        const labelMessageId = (label) => {{
          if (typeof label !== "string" || !label) return "";
          return labelToId[label] || zhValToId[label] || labelToId[labelKey(label)] || "";
        }};
        const loadLocaleCatalog = (target) => {{
          const idToVal = {{}};
          try {{
            if (!target || target === "zh-CN") return idToVal;
            const path = requireFromMain("path");
            const tobj = JSON.parse(fs.readFileSync(path.join(process.resourcesPath, target + ".json"), "utf8"));
            rememberCatalog(tobj);
            for (const key in tobj) if (tobj[key]) idToVal[key] = tobj[key];
          }} catch (_) {{}}
          return idToVal;
        }};
        const translateDynamicMenuLabel = (label) => {{
          const key = labelKey(label);
          const restartPrefix = "Restart to update to ";
          if (key.startsWith(restartPrefix)) return "\u91cd\u65b0\u542f\u52a8\u4ee5\u66f4\u65b0\u5230 " + key.slice(restartPrefix.length);
          return "";
        }};
        const translateLabel = (label, id, role) => {{
          if (typeof label !== "string" || !label) return label;
          if (role && menuRoleZh[role]) return menuRoleZh[role];
          const dynamic = translateDynamicMenuLabel(label);
          if (dynamic) return dynamic;
          if (id && enToZh[label]) return enToZh[label];
          if (id && zhLocaleObj[id]) return zhLocaleObj[id];
          if (menuHardcodedZh[label]) return menuHardcodedZh[label];
          const key = labelKey(label);
          if (enToZh[label]) return enToZh[label];
          if (menuHardcodedZh[key]) return menuHardcodedZh[key];
          if (enToZh[key]) return enToZh[key];
          return label;
        }};
        const relabelMenuItems = (menu, target, idToVal = {{}}) => {{
          if (!menu || !menu.items) return;
          for (const item of menu.items) {{
            try {{
              if (typeof item.label === "string") {{
                const base = item.__cslOrig === undefined ? item.label : item.__cslOrig;
                if (item.__cslOrig === undefined) item.__cslOrig = zhHardcodedToEn[base] || base;
                const orig = item.__cslOrig;
                if (item.__cslMessageId === undefined) item.__cslMessageId = labelMessageId(orig) || labelMessageId(item.label);
                if (target === "zh-CN") item.label = translateLabel(orig, item.__cslMessageId, roleKey(item));
                else {{
                  const id = item.__cslMessageId || labelMessageId(orig);
                  item.label = id && idToVal[id] ? idToVal[id] : (zhHardcodedToEn[orig] || orig);
                }}
              }}
              if (item.submenu) relabelMenuItems(item.submenu, target, idToVal);
            }} catch (_) {{}}
          }}
        }};
        const translateMenuItems = (menu) => {{
          if (!menu || !menu.items || !zhActive()) return menu;
          relabelMenuItems(menu, "zh-CN", {{}});
          return menu;
        }};
        const localizeMenuForCurrentLocale = (menu) => {{
          try {{
            if (menu && menu.items) {{
              relabelMenuItems(menu, currentLocale, loadLocaleCatalog(currentLocale));
              translateMenuItems(menu);
            }}
          }} catch (_) {{}}
          return menu;
        }};
        if (Menu) {{
          Menu.__cslLocalizeMenuForCurrentLocale = localizeMenuForCurrentLocale;
          Menu.__cslMenuPopupLocalizationInstalled = true;
          const origBuildFromTemplate = typeof Menu.buildFromTemplate === "function"
            ? (Menu.__cslOrigBuildFromTemplate || Menu.buildFromTemplate.bind(Menu))
            : null;
          if (origBuildFromTemplate) {{
            Menu.__cslOrigBuildFromTemplate = origBuildFromTemplate;
            Menu.buildFromTemplate = (template) => {{
              const menu = origBuildFromTemplate(template);
              return Menu.__cslLocalizeMenuForCurrentLocale?.(menu) || menu;
            }};
          }}
          if (typeof Menu.setApplicationMenu === "function") {{
            const origSetApplicationMenu = Menu.__cslOrigSetApplicationMenuForPopup || Menu.setApplicationMenu.bind(Menu);
            Menu.__cslOrigSetApplicationMenuForPopup = origSetApplicationMenu;
            Menu.setApplicationMenu = (menu) => {{
              try {{ Menu.__cslLocalizeMenuForCurrentLocale?.(menu); }} catch (_) {{}}
              return origSetApplicationMenu(menu);
            }};
          }}
          if (Menu.prototype && typeof Menu.prototype.popup === "function") {{
            const origPopup = Menu.__cslOrigPopup || Menu.prototype.popup;
            Menu.__cslOrigPopup = origPopup;
            Menu.prototype.popup = function (...args) {{
              try {{ Menu.__cslLocalizeMenuForCurrentLocale?.(this); }} catch (_) {{}}
              return origPopup.call(this, ...args);
            }};
          }}
          localeChangeListeners.push(() => {{
            try {{
              const menu = typeof Menu.getApplicationMenu === "function" ? Menu.getApplicationMenu() : null;
              if (menu) Menu.__cslLocalizeMenuForCurrentLocale?.(menu);
            }} catch (_) {{}}
          }});
          const currentMenu = typeof Menu.getApplicationMenu === "function" ? Menu.getApplicationMenu() : null;
          if (currentMenu) Menu.__cslLocalizeMenuForCurrentLocale?.(currentMenu);
        }}
        if (Tray?.prototype && typeof Tray.prototype.setContextMenu === "function") {{
          const knownTrayMenus = Tray.__cslKnownTrayMenus || new Set();
          Tray.__cslKnownTrayMenus = knownTrayMenus;
          const localizeTrayMenuForCurrentLocale = (menu) => {{
            try {{
              if (menu) knownTrayMenus.add(menu);
              return localizeMenuForCurrentLocale(menu);
            }} catch (_) {{}}
            return menu;
          }};
          const retranslateTrayMenus = () => {{
            try {{
              for (const menu of Array.from(knownTrayMenus)) localizeTrayMenuForCurrentLocale(menu);
            }} catch (_) {{}}
          }};
          const origSetContextMenu = Tray.prototype.__cslOrigSetContextMenu || Tray.prototype.setContextMenu;
          Tray.prototype.__cslOrigSetContextMenu = origSetContextMenu;
          Tray.__cslTrayMenuLocalizationInstalled = true;
          Tray.__cslLocalizeTrayMenuForCurrentLocale = localizeTrayMenuForCurrentLocale;
          Tray.prototype.__cslTrayMenuLocalizationInstalled = true;
          Tray.prototype.setContextMenu = function (menu) {{
            try {{ Tray.__cslLocalizeTrayMenuForCurrentLocale?.(menu); }} catch (_) {{}}
            return origSetContextMenu.call(this, menu);
          }};
          localeChangeListeners.push(retranslateTrayMenus);
          retranslateTrayMenus();
        }}
      }}
    }} catch (_) {{}}
  }};
  installWindowsMenuPopupLocalization();

  const pollLocale = async () => {{
    try {{
      const contents = allContents();
      let fallback = "";
      for (const item of contents) {{
        try {{
          const url = item.getURL?.() ?? "";
          const lower = String(url || "").toLowerCase();
          if (!isSyncableUrl(lower)) continue;
          const loc = await item.executeJavaScript('localStorage.getItem("__cslWantedLocale")||localStorage.getItem("spa:locale")', true);
          if (typeof loc !== "string" || !loc) continue;
          if (lower.startsWith("https://claude.ai") || lower.startsWith("http://claude.ai")) {{
            setCurrentLocale(loc);
            return;
          }}
          if (!fallback) fallback = loc;
        }} catch (_) {{}}
      }}
      if (fallback) setCurrentLocale(fallback);
    }} catch (_) {{}}
  }};

  const refresh = async () => {{
    if (!localizedLaunchDefaultZh && consumeLocalizedLaunchMarker()) {{
      localizedLaunchDefaultZh = true;
      setCurrentLocale("zh-CN");
    }}
    const contents = allContents();
	    const results = await Promise.all(contents.map((item) => attach(item).catch(() => false)));
	    syncOpenWindowsLocale(currentLocale);
	    fireLocaleChange(currentLocale);
	    return {{
      attached: results.filter(Boolean).length,
      contents: contents.length,
      windows: BrowserWindow.getAllWindows().length
    }};
  }};

  app.on("browser-window-created", (_event, window) => {{
    setTimeout(() => {{
      try {{ syncOneWindowLocale(window.webContents, currentLocale); }} catch (_) {{}}
      attach(window.webContents).catch(() => {{}});
    }}, 50);
    try {{
      const contents = window?.webContents;
      if (contents && !contents.__cslDevToolsTitleSyncInstalled) {{
        contents.__cslDevToolsTitleSyncInstalled = true;
        contents.on?.("page-title-updated", () => syncDevToolsTitleLater(contents, 20));
        contents.on?.("did-finish-load", () => syncDevToolsTitleLater(contents, 20));
        contents.on?.("devtools-opened", () => {{
          try {{
            const devContents = contents.devToolsWebContents;
            if (devContents) {{
              syncDevToolsTitleLater(devContents, 60);
              devContents.on?.("page-title-updated", () => syncDevToolsTitleLater(devContents, 20));
              devContents.on?.("did-finish-load", () => syncDevToolsTitleLater(devContents, 20));
            }}
          }} catch (_) {{}}
        }});
      }}
    }} catch (_) {{}}
  }});
  const timer = setInterval(refresh, 2000);
  timer.unref?.();
  const localeTimer = setInterval(pollLocale, 1000);
  localeTimer.unref?.();
  const dispose = () => {{
    try {{ clearInterval(timer); }} catch (_) {{}}
    try {{ clearInterval(localeTimer); }} catch (_) {{}}
  }};
  globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__ = {{ version: CSL_INJECTION_VERSION, injectionSignature: CSL_INJECTION_SIGNATURE, refresh, dispose }};
  const summary = await refresh();
  return {{ ok: true, reused: false, ...summary }};
}})()"##
    )
}

fn main_process_injection_signature() -> String {
    let mut hasher = Sha256::new();
    hasher.update(include_str!("claude_desktop_patch.rs").as_bytes());
    hasher.update(TRANSLATION_RUNTIME.as_bytes());
    hasher.update(CLAUDE_SHELL_ZH_LOCALE.as_bytes());
    hasher.update(CLAUDE_ION_ZH_LOCALE.as_bytes());
    hasher.update(CLAUDE_ION_DYNAMIC_ZH_LOCALE.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}")
}

fn retry_inject_localization() -> Result<usize, String> {
    let timeout = Duration::from_millis(
        (CLAUDE_ZH_INJECTION_RETRY_COUNT as u64).saturating_mul(CLAUDE_ZH_INJECTION_RETRY_MS),
    );
    retry_inject_localization_until(timeout)
}

fn retry_inject_localization_until(timeout: Duration) -> Result<usize, String> {
    let mut last_error: Option<String> = None;
    let started = Instant::now();
    while started.elapsed() < timeout {
        match inject_localization() {
            Ok(count) if count > 0 => return Ok(count),
            Ok(_) => {
                last_error = Some(
                    "Claude Node inspector did not expose a matching Claude target.".to_string(),
                );
            }
            Err(err) => {
                last_error = Some(err);
            }
        }
        thread::sleep(Duration::from_millis(CLAUDE_ZH_INJECTION_RETRY_MS));
    }
    Err(last_error.unwrap_or_else(|| "Claude DevTools endpoint was not available.".to_string()))
}

#[allow(dead_code)]
fn find_running_claude_process_ids(preferred_pid: Option<u32>) -> Vec<u32> {
    if !cfg!(target_os = "windows") {
        return preferred_pid.into_iter().collect();
    }
    let mut pids =
        crate::core::platform::run_powershell(&windows_find_claude_process_script(preferred_pid))
            .ok()
            .map(|output| {
                output
                    .lines()
                    .filter_map(|line| line.trim().parse::<u32>().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
    if let Some(pid) = preferred_pid {
        if !pids.contains(&pid) {
            pids.insert(0, pid);
        }
    }
    pids
}

#[allow(dead_code)]
fn windows_find_claude_process_script(preferred_pid: Option<u32>) -> String {
    let preferred = preferred_pid
        .map(|pid| pid.to_string())
        .unwrap_or_else(|| "$null".to_string());
    format!(
        r#"
$preferred = {preferred}
$visible = @(Get-Process -Name 'claude' -ErrorAction SilentlyContinue |
  Where-Object {{ $_.Path -and $_.Path.IndexOf('Claude', [System.StringComparison]::OrdinalIgnoreCase) -ge 0 }} |
  Sort-Object -Property StartTime)
$ordered = @()
if ($preferred) {{
  $match = $visible | Where-Object {{ [int]$_.Id -eq [int]$preferred }} | Select-Object -First 1
  if ($match) {{ $ordered += $match }}
}}
$ordered += @($visible | Where-Object {{ -not $preferred -or [int]$_.Id -ne [int]$preferred }})
if ($ordered.Count -eq 0) {{
  $ordered = @(Get-Process -Name 'claude' -ErrorAction SilentlyContinue)
}}
$ordered | ForEach-Object {{ [string]$_.Id }}
"#
    )
}

fn read_node_inspector_targets() -> Result<Vec<serde_json::Value>, String> {
    match read_node_inspector_targets_from_port(CLAUDE_NODE_INSPECT_PORT) {
        Ok(targets) if !targets.is_empty() => Ok(targets),
        Ok(_) => Err(format!(
            "Claude Node inspector on port {CLAUDE_NODE_INSPECT_PORT} had no targets."
        )),
        Err(err) => Err(err),
    }
}

fn read_node_inspector_targets_from_port(port: u16) -> Result<Vec<serde_json::Value>, String> {
    reqwest::blocking::get(format!("http://127.0.0.1:{port}/json"))
        .map_err(|err| {
            format!("Failed to read Claude Node inspector targets on port {port}: {err}")
        })?
        .json::<Vec<serde_json::Value>>()
        .map_err(|err| {
            format!("Failed to parse Claude Node inspector targets on port {port}: {err}")
        })
}

fn claude_node_inspector_available() -> bool {
    let Ok(targets) = read_node_inspector_targets() else {
        return false;
    };
    targets
        .iter()
        .filter_map(|target| target.get("webSocketDebuggerUrl").and_then(Value::as_str))
        .any(|ws_url| is_claude_node_inspector(ws_url).unwrap_or(false))
}

fn wait_for_claude_node_inspector() -> bool {
    for _ in 0..20 {
        if claude_node_inspector_available() {
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    false
}

fn evaluate_node_inspector_expression(ws_url: &str, expression: &str) -> Result<usize, String> {
    let (mut socket, _) =
        connect(ws_url).map_err(|err| format!("Failed to connect Claude Node inspector: {err}"))?;
    set_inspector_read_timeout(&mut socket, CLAUDE_INSPECTOR_EVAL_TIMEOUT);
    send_cdp_message(
        &mut socket,
        1,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "awaitPromise": true,
            "returnByValue": true
        }),
        "inject Claude localization through Node inspector",
    )?;
    while let Ok(message) = socket.read() {
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if value.get("id").and_then(Value::as_u64) != Some(1) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!(
                "Claude Node inspector rejected localization: {error}"
            ));
        }
        if let Some(exception) = value
            .get("result")
            .and_then(|result| result.get("exceptionDetails"))
        {
            return Err(format!(
                "Claude Node inspector localization raised an exception: {exception}"
            ));
        }
        let Some(result) = value
            .get("result")
            .and_then(|result| result.get("result"))
            .and_then(|result| result.get("value"))
        else {
            return Err("Claude Node inspector returned no localization result.".to_string());
        };
        if result.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(format!(
                "Claude Node inspector localization did not attach to a renderer: {result}"
            ));
        }
        let attached = result
            .get("attached")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize;
        if attached == 0 {
            return Err(format!(
                "Claude Node inspector localization found no attachable renderer: {result}"
            ));
        }
        return Ok(attached);
    }
    Err("Claude Node inspector closed before confirming localization.".to_string())
}

fn is_claude_node_inspector(ws_url: &str) -> Result<bool, String> {
    let expression = r#"
(() => {
  try {
    const requireFromMain = process.getBuiltinModule("module").createRequire(process.execPath);
    const electron = requireFromMain("electron");
    const app = electron.app;
    const identity = {
      execPath: process.execPath || "",
      argv: process.argv || [],
      appName: app?.getName?.() || "",
      appPath: app?.getAppPath?.() || "",
      userData: app?.getPath?.("userData") || ""
    };
    return JSON.stringify(identity);
  } catch (error) {
    return JSON.stringify({
      execPath: process.execPath || "",
      argv: process.argv || [],
      error: String(error && error.message || error)
    });
  }
})()
"#;
    let value = evaluate_node_inspector_json(ws_url, expression, "identify Claude Node inspector")?;
    Ok(node_inspector_identity_is_claude(&value))
}

fn evaluate_node_inspector_json(
    ws_url: &str,
    expression: &str,
    action: &str,
) -> Result<Value, String> {
    let (mut socket, _) =
        connect(ws_url).map_err(|err| format!("Failed to connect Claude Node inspector: {err}"))?;
    set_inspector_read_timeout(&mut socket, CLAUDE_INSPECTOR_EVAL_TIMEOUT);
    send_cdp_message(
        &mut socket,
        1,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "awaitPromise": false,
            "returnByValue": true
        }),
        action,
    )?;
    while let Ok(message) = socket.read() {
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if value.get("id").and_then(Value::as_u64) != Some(1) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("Claude Node inspector rejected {action}: {error}"));
        }
        if let Some(exception) = value
            .get("result")
            .and_then(|result| result.get("exceptionDetails"))
        {
            return Err(format!(
                "Claude Node inspector raised an exception during {action}: {exception}"
            ));
        }
        let Some(raw) = value
            .get("result")
            .and_then(|result| result.get("result"))
            .and_then(|result| result.get("value"))
            .and_then(Value::as_str)
        else {
            return Err(format!(
                "Claude Node inspector returned no JSON for {action}."
            ));
        };
        return serde_json::from_str(raw)
            .map_err(|err| format!("Failed to parse Claude Node inspector {action} JSON: {err}"));
    }
    Err(format!(
        "Claude Node inspector closed before confirming {action}."
    ))
}

fn node_inspector_identity_is_claude(value: &Value) -> bool {
    // Identify a Node inspector target as Claude by its own identity fields,
    // not by a loose substring scan of the whole JSON. The previous check
    // matched any payload containing "claude" anywhere and then had to
    // blacklist specific third-party programs to avoid false positives; a
    // field-based whitelist is precise and needs no per-app special cases.
    let app_name = value.get("appName").and_then(Value::as_str).unwrap_or("");
    let exec_path = value.get("execPath").and_then(Value::as_str).unwrap_or("");
    let exec_base = exec_path.rsplit(['/', '\\']).next().unwrap_or(exec_path);
    app_name.eq_ignore_ascii_case("Claude")
        || exec_base.eq_ignore_ascii_case("Claude")
        || exec_base.eq_ignore_ascii_case("Claude.exe")
}

/// Apply a read timeout to the underlying TCP stream of an inspector
/// WebSocket so a stalled CDP response cannot block the read loop forever.
fn set_inspector_read_timeout(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    timeout: Duration,
) {
    use std::net::TcpStream;
    let stream = socket.get_ref();
    // The Claude inspector listens on plain ws://127.0.0.1, so the stream is
    // the MaybeTlsStream::Plain variant wrapping a TcpStream.
    let tcp: Option<&TcpStream> = match stream {
        tungstenite::stream::MaybeTlsStream::Plain(tcp) => Some(tcp),
        _ => None,
    };
    if let Some(tcp) = tcp {
        let _ = tcp.set_read_timeout(Some(timeout));
        let _ = tcp.set_write_timeout(Some(timeout));
    }
}

fn send_cdp_message(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    id: u64,
    method: &str,
    params: Value,
    action: &str,
) -> Result<(), String> {
    socket
        .send(Message::Text(
            json!({
                "id": id,
                "method": method,
                "params": params
            })
            .to_string()
            .into(),
        ))
        .map_err(|err| format!("Failed to {action}: {err}"))
}

fn emit_terminal(app: &tauri::AppHandle, session_id: &str, data: &str) {
    let _ = app.emit(
        INSTALL_TERMINAL_OUTPUT_EVENT,
        InstallTerminalOutput {
            session_id: session_id.to_string(),
            stream: "output".to_string(),
            data: data.to_string(),
            done: false,
            exit_code: None,
        },
    );
}

const TRANSLATION_RUNTIME: &str = r##"(() => {
  if (globalThis.__CLAUDE_ZH_RUNTIME__) return;
  globalThis.__CLAUDE_ZH_RUNTIME__ = true;

  const debug = localStorage.getItem("claude-zh-debug") === "1";
  const log = (...args) => debug && console.debug("[claude-zh]", ...args);
  const CSL_WANTED_LOCALE_KEY = "__cslWantedLocale";
  const getActiveLocale = () => {
    try {
      var wl = localStorage.getItem(CSL_WANTED_LOCALE_KEY);
      if (wl) return wl;
      var sl = localStorage.getItem("spa:locale");
      if (sl) return sl;
      if (typeof __CSL_LL !== "undefined" && __CSL_LL) return "zh-CN";
      if (/claude\.ai$/i.test(location.hostname) && String(navigator.language || "").toLowerCase().indexOf("zh") === 0) return "zh-CN";
    } catch (_) {}
    return "";
  };
  const zhOn = () => getActiveLocale() === "zh-CN";
  const refreshLocaleUiSoon = () => setTimeout(() => {
    try { if (document.body) walkText(document.body); } catch (_) {}
    try { fixLanguageRadio(); } catch (_) {}
    try { fixTitle(); } catch (_) {}
  }, 0);
  const rememberLocale = (loc) => {
    if (typeof loc !== "string" || !loc) return;
    try {
      localStorage.setItem(CSL_WANTED_LOCALE_KEY, loc);
      localStorage.setItem("spa:locale", loc);
      if (document.documentElement) document.documentElement.setAttribute("lang", loc);
      try { window.dispatchEvent(new StorageEvent("storage", { key: "spa:locale", newValue: loc })); } catch (_) {}
      try { window.dispatchEvent(new CustomEvent("claude-locale-change", { detail: loc })); } catch (_) {}
    } catch (_) {}
    refreshLocaleUiSoon();
  };

  const installLocaleWhitelist = () => {
    try {
      const origInc = Array.prototype.includes;
      const isZh = (a) => {
        try { return a && a.length === 11 && origInc.call(a, "en-US") && origInc.call(a, "id-ID") && !origInc.call(a, "zh-CN") && origInc.call(a, "es-419"); } catch (_) { return false; }
      };
      Array.prototype.includes = function (s, f) {
        if (s === "zh-CN" && isZh(this)) return true;
        return origInc.call(this, s, f);
      };
      const origMap = Array.prototype.map;
      const pMap = function (cb, tA) {
        const r = origMap.call(this, cb, tA);
        if (isZh(this)) { try { r.push(cb.call(tA, "zh-CN", this.length, this)); } catch (_) {} }
        return r;
      };
      const origHas = Map.prototype.has;
      const origGet = Map.prototype.get;
      const isZhM = (m) => {
        try { return m && m.size >= 20 && m.size <= 24 && origHas.call(m, "en-us") && origHas.call(m, "id-id") && !origHas.call(m, "zh-cn") && origHas.call(m, "es-419"); } catch (_) { return false; }
      };
      const pH = function (k) { if (k === "zh-cn" && isZhM(this)) return true; return origHas.call(this, k); };
      const pG = function (k) { if (k === "zh-cn" && isZhM(this)) return "zh-CN"; return origGet.call(this, k); };
      const origSetHas = Set.prototype.has;
      const isZhSet = (s) => { try { return s && s.size >= 9 && s.size <= 11 && origSetHas.call(s, "en-US") && origSetHas.call(s, "id-ID") && !origSetHas.call(s, "zh-CN") && origSetHas.call(s, "es-419"); } catch (_) { return false; } };
      const pSH = function (v) { if (v === "zh-CN" && isZhSet(this)) return true; return origSetHas.call(this, v); };
      const lock = () => {
        try { Object.defineProperty(Array.prototype, "map", { value: pMap, writable: false, configurable: true }); } catch (_) {}
        try { Object.defineProperty(Map.prototype, "has", { value: pH, writable: false, configurable: true }); } catch (_) {}
        try { Object.defineProperty(Map.prototype, "get", { value: pG, writable: false, configurable: true }); } catch (_) {}
        try { Object.defineProperty(Set.prototype, "has", { value: pSH, writable: false, configurable: true }); } catch (_) {}
      };
      lock();
      setInterval(lock, 500);
    } catch (_) {}
  };

  const installLocalePersistence = () => {
    try {
      if (typeof fetch !== "function") return;
      const rf = fetch.bind(globalThis);
      const withZh = (resp) => resp.clone().text().then((text) => {
        try {
          const obj = JSON.parse(text);
          let changed = false;
          if (obj && obj.locale !== "zh-CN") { obj.locale = "zh-CN"; changed = true; }
          if (obj && obj.account && obj.account.locale !== "zh-CN") { obj.account.locale = "zh-CN"; changed = true; }
          if (obj && obj.gated_messages) { delete obj.gated_messages; changed = true; }
          if (!changed) return resp;
          const ct = resp.headers.get("content-type") || "application/json";
          return new Response(JSON.stringify(obj), { status: resp.status, headers: { "content-type": ct } });
        } catch (_) { return resp; }
      }).catch(() => resp);
      const localeRequestBodySync = true;
      const readLocaleBody = (body) => {
        if (!body || (typeof body !== "string" && !(body instanceof String))) return null;
        const text = String(body);
        if (text.indexOf("locale") < 0) return null;
        try {
          const obj = JSON.parse(text);
          if (obj && typeof obj.locale === "string" && obj.locale) return { obj, locale: obj.locale };
        } catch (_) {}
        return null;
      };
      globalThis.fetch = (input, init) => {
        const url = typeof input === "string" ? input : (input && input.url) || "";
        const ap = url.indexOf("/api/account_profile") >= 0;
        let nextInit = init;
        const method = String((init && init.method) || (input && input.method) || "");
        if (init && init.body && /PUT|POST|PATCH/i.test(method)) {
          const parsed = readLocaleBody(init.body);
          if (parsed) {
            rememberLocale(parsed.locale);
            if (parsed.locale === "zh-CN") {
              parsed.obj.locale = "en-US";
              nextInit = Object.assign({}, init, { body: JSON.stringify(parsed.obj) });
            }
          }
        }
        if (url.indexOf("overrides.json") >= 0 && zhOn()) { return rf(input, nextInit).then((resp) => { const ct = (resp.headers.get("content-type") || "").toLowerCase(); if (ct.indexOf("json") < 0) return new Response("{}", { status: 200, headers: { "content-type": "application/json" } }); return resp; }).catch(() => rf(input, nextInit)); }
        if ((!ap && url.indexOf("/bootstrap") < 0 && url.indexOf("/app_start") < 0) || !zhOn()) return rf(input, nextInit);
        return rf(input, nextInit).then((resp) => { if (!resp || !resp.ok) return resp; return withZh(resp); }).catch(() => rf(input, nextInit));
      };
    } catch (_) {}
  };

  installLocaleWhitelist();
  installLocalePersistence();
  const TEXT_ZH = {
    "Pricing Analysis": "\u5b9a\u4ef7\u5206\u6790",
    "Market Research": "\u5e02\u573a\u8c03\u7814",
    "Spreadsheet \u00b7 XLSX": "\u7535\u5b50\u8868\u683c \u00b7 XLSX",
    "Table \u00b7 CSV": "\u8868\u683c \u00b7 CSV",
    "Document \u00b7 PDF": "\u6587\u6863 \u00b7 PDF",
    "Presentation \u00b7 PPTX": "\u6f14\u793a\u6587\u7a3f \u00b7 PPTX",
    "Document \u00b7 DOCX": "\u6587\u6863 \u00b7 DOCX",
    "Cowork": "\u534f\u4f5c",
    "Try Cowork": "\u8bd5\u8bd5 Cowork",
    "Home": "\u9996\u9875",
    "Chat": "\u804a\u5929",
    "Code": "\u4ee3\u7801",
    "Turn on memory": "\u5f00\u542f\u8bb0\u5fc6",
    "Get Pro plan": "\u83b7\u53d6 Pro \u8ba1\u5212",
    "Get started with Claude": "\u5f00\u59cb\u4f7f\u7528 Claude",
    "Upgrade to let Claude take on real tasks for you": "\u5347\u7ea7\uff0c\u8ba9 Claude \u4e3a\u4f60\u5904\u7406\u771f\u6b63\u7684\u4efb\u52a1",
    "Currently unavailable": "\u5f53\u524d\u4e0d\u53ef\u7528",
    "For more complex tasks": "\u66f4\u590d\u6742\u4efb\u52a1",
    "For complex tasks": "\u590d\u6742\u4efb\u52a1",
    "Always uses deep reasoning": "\u59cb\u7ec8\u4f7f\u7528\u6df1\u5ea6\u63a8\u7406",
    "Adaptive": "\u81ea\u9002\u5e94",
    "Extended": "\u6269\u5c55",
    "skill-creator": "\u6280\u80fd\u521b\u5efa\u5668",
    "About Claude": "\u5173\u4e8eClaude",
    "Help": "\u5e2e\u52a9",
    "Get support": "\u83b7\u53d6\u652f\u6301",
    "Copied version to clipboard": "\u7248\u672c\u5df2\u590d\u5236\u5230\u526a\u8d34\u677f",
    "Disable bundled skills and workflows": "\u7981\u7528\u5185\u7f6e\u6280\u80fd\u548c\u5de5\u4f5c\u6d41",
    "Disables Claude Code's bundled skills and workflows (deep-research and similar). Use where they cannot function, for instance when WebFetch is egress-blocked and the gateway does not forward the WebSearch server tool.": "\u7981\u7528 Claude Code \u5185\u7f6e\u6280\u80fd\u548c\u5de5\u4f5c\u6d41\uff08\u5982 deep-research \u7b49\uff09\u3002\u5f53\u8fd9\u4e9b\u529f\u80fd\u65e0\u6cd5\u6b63\u5e38\u4f7f\u7528\u65f6\u542f\u7528\uff0c\u4f8b\u5982 WebFetch \u88ab\u7f51\u7edc\u51fa\u53e3\u963b\u6b62\uff0c\u4e14\u7f51\u5173\u672a\u8f6c\u53d1 WebSearch \u670d\u52a1\u5668\u5de5\u5177\u3002",
  };
  const FULL_ZH = {
    "Create new skills, modify and improve existing skills": "\u521b\u5efa\u65b0\u6280\u80fd\uff0c\u4fee\u6539\u5e76\u6539\u8fdb\u73b0\u6709\u6280\u80fd\uff0c\u5e76\u8861\u91cf\u6280\u80fd\u8868\u73b0\u3002\u5f53\u7528\u6237\u60f3\u8981\u4ece\u96f6\u5f00\u59cb\u521b\u5efa\u6280\u80fd\u3001\u7f16\u8f91\u6216\u4f18\u5316\u73b0\u6709\u6280\u80fd\u3001\u8fd0\u884c\u8bc4\u4f30\u6765\u6d4b\u8bd5\u6280\u80fd\u3001\u901a\u8fc7\u65b9\u5dee\u5206\u6790\u5bf9\u6280\u80fd\u8868\u73b0\u8fdb\u884c\u57fa\u51c6\u6d4b\u8bd5\uff0c\u6216\u4f18\u5316\u6280\u80fd\u63cf\u8ff0\u4ee5\u63d0\u5347\u89e6\u53d1\u51c6\u786e\u6027\u65f6\u4f7f\u7528\u3002",
  };
  const gatewayProviderSubstringFallback = true;
  const codeUiLabelFallback = true;
  const claudeFirstScreenFallback = true;
  const uiOnlyDomFallback = true;
  const reversibleTextFallback = true;
  const SUBSTR_ZH = {
    "GATEWAY": "\u7f51\u5173",
    "Gateway": "\u7f51\u5173",
    "Version ": "\u7248\u672c",
    "is currently unavailable.": "\u5f53\u524d\u4e0d\u53ef\u7528\u3002",
  };
  const FIRST_SCREEN_ZH = {
    "Let's knock something off your list": "\u8ba9\u6211\u4eec\u4ece\u4f60\u7684\u6e05\u5355\u4e0a\u780d\u6389\u4e00\u4ef6\u4e8b",
    "What can I help you with today?": "\u4eca\u5929\u6709\u4ec0\u4e48\u6211\u53ef\u4ee5\u5e2e\u5fd9\u7684\u5417\uff1f",
    "What can I help you with?": "\u6211\u80fd\u5e2e\u4f60\u4ec0\u4e48\uff1f",
    "Good morning": "\u65e9\u4e0a\u597d",
    "Good afternoon": "\u4e0b\u5348\u597d",
    "Good evening": "\u665a\u4e0a\u597d",
    "Evening": "\u665a\u4e0a\u597d",
  };
  const FIRST_SCREEN_PREFIX_ZH = {
    "Good morning, ": "\u65e9\u4e0a\u597d\uff0c",
    "Good afternoon, ": "\u4e0b\u5348\u597d\uff0c",
    "Good evening, ": "\u665a\u4e0a\u597d\uff0c",
    "Evening, ": "\u665a\u4e0a\u597d\uff0c",
  };
  const WEEKDAY_ZH = {
    Monday: "\u5468\u4e00",
    Tuesday: "\u5468\u4e8c",
    Wednesday: "\u5468\u4e09",
    Thursday: "\u5468\u56db",
    Friday: "\u5468\u4e94",
    Saturday: "\u5468\u516d",
    Sunday: "\u5468\u65e5",
  };
  const CSL_ORIG_TEXT = "__cslOrigText";
  const CSL_TRANSLATED_TEXT = "__cslTranslatedText";
  const TEXT_EN = {};
  try { for (var rek in TEXT_ZH) if (TEXT_ZH[rek] && TEXT_EN[TEXT_ZH[rek]] === undefined) TEXT_EN[TEXT_ZH[rek]] = rek; } catch (_) {}
  const genSel = '[data-thinking], [class*="thinking"], [class*="thought"], [class*="markdown"], [class*="prose"], pre, code, [contenteditable]';
  const uiHint = /(nav|menu|sidebar|tab|model|toolbar|button|btn|dropdown|popover|modal)/i;
  const generatedContentTextNode = (node) => {
    try { return !!(node.parentElement?.closest?.(genSel)); } catch (_) { return false; }
  };
  const likelyUiTextNode = (node) => {
    try {
      for (var el = node && node.parentElement, d = 0; el && d < 5; el = el.parentElement, d++) {
        var tag = el.tagName || "";
        if (/^(BUTTON|A|NAV|ASIDE|HEADER)$/.test(tag)) return true;
        var role = el.getAttribute?.("role") || "";
        if (/^(button|menuitem|menuitemradio|tab|option)$/.test(role)) return true;
        if (el.getAttribute?.("aria-label") || el.getAttribute?.("aria-controls") || el.getAttribute?.("aria-current") || el.getAttribute?.("aria-selected")) return true;
        var hint = (el.getAttribute?.("data-testid") || "") + " " + (el.getAttribute?.("class") || "");
        if (uiHint.test(hint)) return true;
      }
    } catch (_) { return false; }
    return false;
  };
  const shouldTranslateDomFallbackTextNode = (node, trimmed) => {
    try {
      if (!trimmed || trimmed.length > 160) return false;
      if (generatedContentTextNode(node)) return false;
      if (translatedFirstScreenTextValue(trimmed)) return true;
      return likelyUiTextNode(node);
    } catch (_) { return false; }
  };
  const clearTextState = (node) => { try { delete node[CSL_ORIG_TEXT]; delete node[CSL_TRANSLATED_TEXT]; } catch (_) {} };
  const restoreTextNode = (node) => {
    try {
      const orig = node[CSL_ORIG_TEXT];
      const translated = node[CSL_TRANSLATED_TEXT];
      if (typeof orig !== "string") {
        if (!likelyUiTextNode(node)) return;
        const v = node.nodeValue || "";
        const trimmed = v.trim();
        const en = TEXT_EN[trimmed];
        if (!en) return;
        const lead = v.length - v.trimStart().length;
        node.nodeValue = v.slice(0, lead) + en + v.slice(lead + trimmed.length);
        return;
      }
      if (node.nodeValue === translated || node.nodeValue === orig) node.nodeValue = orig;
      clearTextState(node);
    } catch (_) {}
  };
  const translatedTextValue = (v) => {
    if (!v) return v;
    const trimmed = v.trim();
    if (!trimmed) return v;
    const lead = v.length - v.trimStart().length;
    const firstScreen = translatedFirstScreenTextValue(v);
    if (firstScreen) return firstScreen;
    var zh = TEXT_ZH[trimmed];
    if (zh) return v.slice(0, lead) + zh + v.slice(lead + trimmed.length);
    for (var fk in FULL_ZH) if (fk.length > 15 && trimmed.indexOf(fk) === 0) return v.slice(0, lead) + FULL_ZH[fk];
    for (var k in TEXT_ZH) if (k.length > 15 && trimmed.indexOf(k) === 0) return v.slice(0, lead) + TEXT_ZH[k] + v.slice(lead + k.length);
    var nv = v;
    for (var sk in SUBSTR_ZH) {
      var pos = nv.indexOf(sk);
      if (pos >= 0) nv = nv.slice(0, pos) + SUBSTR_ZH[sk] + nv.slice(pos + sk.length);
    }
    return nv;
  };
  function translatedFirstScreenTextValue(v) {
    if (!v) return null;
    const trimmed = v.trim();
    if (!trimmed || trimmed.length > 120) return null;
    const lead = v.length - v.trimStart().length;
    const trail = v.slice(lead + trimmed.length);
    const wrap = (text) => v.slice(0, lead) + text + trail;
    const weekday = translatedWeekdayGreetingText(trimmed);
    if (weekday) return wrap(weekday);
    const direct = FIRST_SCREEN_ZH[trimmed];
    if (direct) return wrap(direct);
    for (const prefix in FIRST_SCREEN_PREFIX_ZH) {
      if (trimmed.indexOf(prefix) === 0 && trimmed.length > prefix.length) {
        return wrap(FIRST_SCREEN_PREFIX_ZH[prefix] + trimmed.slice(prefix.length));
      }
    }
    return null;
  }
  function translatedWeekdayGreetingText(trimmed) {
    if (trimmed.indexOf("Happy ") !== 0) return null;
    const rest = trimmed.slice(6);
    for (const day in WEEKDAY_ZH) {
      const zh = WEEKDAY_ZH[day] + "\u5feb\u4e50";
      if (rest === day) return zh;
      const prefix = day + ", ";
      if (rest.indexOf(prefix) === 0 && rest.length > prefix.length) return zh + "\uff0c" + rest.slice(prefix.length);
    }
    return null;
  }
  const translateTextNode = (node) => {
    if (!node || node.nodeType !== 3) return;
    if (generatedContentTextNode(node)) { restoreTextNode(node); return; }
    if (!zhOn()) { restoreTextNode(node); return; }
    let base = typeof node[CSL_ORIG_TEXT] === "string" ? node[CSL_ORIG_TEXT] : node.nodeValue;
    if (typeof node[CSL_ORIG_TEXT] === "string" && node.nodeValue !== node[CSL_TRANSLATED_TEXT] && node.nodeValue !== node[CSL_ORIG_TEXT]) {
      clearTextState(node);
      base = node.nodeValue;
    }
    const trimmed = (base || "").trim();
    if (!shouldTranslateDomFallbackTextNode(node, trimmed)) { restoreTextNode(node); return; }
    const nv = translatedTextValue(base);
    if (!nv || nv === base) { clearTextState(node); return; }
    try { node[CSL_ORIG_TEXT] = base; node[CSL_TRANSLATED_TEXT] = nv; } catch (_) {}
    if (node.nodeValue !== nv) node.nodeValue = nv;
  };
  const walkText = (root) => {
    if (!root) return;
    let walker;
    try { walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null); } catch (_) { return; }
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    for (const n of nodes) translateTextNode(n);
  };
  const fixLanguageRadio = () => {
    try {
      const loc = getActiveLocale();
      if (!loc) return;
      document.querySelectorAll('[role=menuitemradio][lang]').forEach((el) => {
        const want = el.getAttribute("lang") === loc ? "true" : "false";
        if (el.getAttribute("aria-checked") !== want) el.setAttribute("aria-checked", want);
      });
    } catch (_) {}
  };
  const startTextPatch = () => {
    if (!document.body) { setTimeout(startTextPatch, 50); return; }
    walkText(document.body);
    try {
      const obs = new MutationObserver((muts) => {
        let rd = false;
        for (const m of muts) {
          if (m.type === "characterData" && m.target) translateTextNode(m.target);
          else if (m.type === "attributes") rd = true;
          for (const node of m.addedNodes) {
            if (node.nodeType === 3) translateTextNode(node);
            else if (node.nodeType === 1) { walkText(node); rd = true; }
          }
        }
        if (rd) fixLanguageRadio();
      });
      obs.observe(document.body, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["aria-checked"] });
    } catch (_) {}
    setInterval(() => { try { walkText(document.body); } catch (_) {} fixLanguageRadio(); }, 800);
  };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", startTextPatch);
  } else {
    startTextPatch();
  }
  const TITLE_ZH = { "Sign in - Claude": "\u767b\u5f55 - Claude" };
  const TITLE_EN = { "\u767b\u5f55 - Claude": "Sign in - Claude" };
  const fixTitle = () => {
    try {
      if (zhOn()) { if (TITLE_ZH[document.title]) document.title = TITLE_ZH[document.title]; }
      else if (TITLE_EN[document.title]) document.title = TITLE_EN[document.title];
    } catch (_) {}
  };
  fixTitle();
  setInterval(fixTitle, 1500);
})();
"##;

#[cfg(test)]
#[path = "claude_desktop_patch_tests.rs"]
mod claude_desktop_patch_tests;
