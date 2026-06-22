use crate::core::app_paths::{app_paths, ensure_dirs};
use crate::core::detector::{
    claude_desktop_windows_cached_stale_msix_manifest,
    claude_desktop_windows_known_stale_msix_manifest, claude_desktop_windows_native_install_path,
    claude_desktop_windows_package_identities, claude_desktop_windows_stale_msix_manifest,
};
use crate::core::platform::{hidden_command, package};
use crate::core::process_control;
use crate::core::types::InstallTerminalOutput;
#[cfg(test)]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tauri::Emitter;
use tungstenite::{connect, Message};

const CLAUDE_NODE_INSPECT_PORT: u16 = 9229;
const CLAUDE_NODE_INSPECT_PORT_SCAN_END: u16 = 9300;
const CLAUDE_INSPECTOR_OPEN_PORT: u16 = 9233;
const CLAUDE_INSPECTOR_SHIM_NAME: &str = "_csl_inspector_shim.js";
const CLAUDE_FUSE_MARKER: &[u8] = b"dL7pKGdnNz796PbbjQWNKmHXBZaB9tsX";
const CLAUDE_FUSE_INTEGRITY_INDEX: usize = 4;
const CLAUDE_ZH_INJECTION_RETRY_COUNT: usize = 30;
const CLAUDE_ZH_INJECTION_RETRY_MS: u64 = 500;
const INSTALL_TERMINAL_OUTPUT_EVENT: &str = "install-terminal://output";
/// Per-message read timeout for CDP eval round-trips over the Node inspector.
/// Guards against a stalled inspector response hanging the injection thread
/// forever (the read loop otherwise blocks indefinitely).
const CLAUDE_INSPECTOR_EVAL_TIMEOUT: Duration = Duration::from_secs(15);
const CLAUDE_SHELL_ZH_LOCALE_FILE: &str = "zh-CN.json";
const CLAUDE_ION_ZH_LOCALE_RELATIVE_PATH: &str = "ion-dist/i18n/zh-CN.json";
const CLAUDE_LOCALIZED_LAUNCH_MARKER: &str = "localized-launch.flag";
const CLAUDE_SHELL_ZH_LOCALE: &str = include_str!("../../resources/claude-desktop/i18n/zh-CN.json");
const CLAUDE_ION_ZH_LOCALE: &str =
    include_str!("../../resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json");
const CLAUDE_ION_DYNAMIC_ZH_LOCALE_RELATIVE_PATH: &str = "ion-dist/i18n/dynamic/zh-CN.json";
const CLAUDE_ION_DYNAMIC_ZH_LOCALE: &str =
    include_str!("../../resources/claude-desktop/i18n/ion-dist/i18n/dynamic/zh-CN.json");

pub fn launch(localize: bool) -> Result<(), String> {
    if !cfg!(any(target_os = "windows", target_os = "macos")) {
        return Err("Claude Desktop launch is only supported on Windows and macOS.".to_string());
    }

    if cfg!(target_os = "windows") {
        launch_windows_claude_desktop(localize)?;
    } else {
        let mut command = hidden_command("open");
        command.args(["-a", "Claude"]);
        command
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Failed to launch Claude Desktop: {err}"))?;
    }

    if localize {
        spawn_silent_localization_injector();
    }

    Ok(())
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
        Ok("open -a Claude".to_string())
    }
}

/// Ensure the in-place Claude Desktop localization patch (app.asar entry
/// shim + asar-integrity fuse) is applied. Idempotent: a no-op when the
/// patch is already in place. Performs an elevated write (UAC prompt) only
/// when the installed app is not yet patched or the patch was rolled back
/// by an MSIX update/repair. Call this before launching a localized Claude
/// Desktop so the main process opens the Node inspector itself.
pub fn ensure_localization_patch() -> Result<(), String> {
    apply_localization_patch()
}

pub fn spawn_localization_injector(app: tauri::AppHandle, session_id: String) {
    thread::spawn(move || {
        emit_terminal(
            &app,
            &session_id,
            "[claude-zh] waiting for Claude DevTools endpoint...\r\n",
        );
        match retry_inject_localization() {
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

fn spawn_silent_localization_injector() {
    thread::spawn(move || {
        let _ = retry_inject_localization();
    });
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
    Ok(patch_dir)
}

fn localized_launch_marker_path() -> Result<PathBuf, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    Ok(paths
        .config_dir
        .join("claude-desktop-patch")
        .join(CLAUDE_LOCALIZED_LAUNCH_MARKER))
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
    format!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File {}",
        windows_shell_quote(&path.to_string_lossy())
    )
}

fn launch_windows_claude_desktop(localize: bool) -> Result<Option<u32>, String> {
    let args = claude_launch_args(localize);
    close_existing_claude_for_localized_launch()?;
    if localize {
        // Give Windows time to release file handles after the kill so the
        // elevated Copy-Item does not race with a still-closing Claude.exe.
        std::thread::sleep(std::time::Duration::from_millis(500));
        // The localized launch does not pass debug arguments on argv: the
        // Electron fuse `EnableNodeCliInspectArguments` is disabled and the
        // CDP auth gate exits on `--remote-debugging-port`. Instead we patch
        // the installed app in place so its main process opens the Node
        // inspector itself at runtime (same path as the in-app "Developer ->
        // Enable Main Process Debugger" menu), then activate it by MSIX app
        // identity. The existing inspector-scan injection pipeline picks up
        // the inspector on `CLAUDE_INSPECTOR_OPEN_PORT`.
        apply_localization_patch()?;
        write_localized_launch_marker()?;
        activate_localized_claude()?;
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

fn launch_windows_claude_msix(args: &[String]) -> Result<Option<u32>, String> {
    repair_claude_msix_registration()?;
    package::launch_first_msix_package_with_args(claude_desktop_windows_package_identities(), args)
        .map(|pid| (pid > 0).then_some(pid))
        .map_err(|err| format!("Failed to launch Claude Desktop MSIX package: {err}"))
}

fn repair_claude_msix_registration() -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }
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

fn claude_launch_args(_localize: bool) -> Vec<String> {
    Vec::new()
}

/// How the installed Claude Desktop is packaged on Windows.
enum ClaudeInstallKind {
    /// MSIX/AppX package under WindowsApps, activated by app identity.
    Msix,
    /// Native electron-builder/NSIS `.exe` install (e.g. winget's
    /// `Anthropic.Claude` installer on a clean VM), activated by running
    /// the launcher directly. No MSIX identity is available.
    Exe,
}

/// Resolved on-disk locations of the Claude Desktop app to patch in place,
/// plus how to (re)launch it after patching. Generalizes `claude_patch_paths`
/// across the MSIX and native-exe install layouts.
struct ClaudeInstall {
    kind: ClaudeInstallKind,
    /// The Electron binary whose asar-integrity fuse is flipped. For MSIX
    /// this is `<pkg>/app/Claude.exe`; for a Squirrel `.exe` install it is
    /// `<root>/app-<version>/claude.exe` (the real Electron image), NOT the
    /// tiny `<root>/claude.exe` Squirrel launcher.
    patch_exe: PathBuf,
    /// The executable to run after patching. For MSIX this is unused
    /// (activation is by app identity); for a Squirrel `.exe` install it is
    /// the top-level `<root>/claude.exe` launcher, which selects the newest
    /// `app-<version>/` and starts the patched image.
    launcher_exe: PathBuf,
    asar: PathBuf,
    shell_locale: PathBuf,
    ion_locale: PathBuf,
}

/// Resolve the installed Claude Desktop for in-place localization patching,
/// supporting both the MSIX layout (`<pkg>/app/Claude.exe` +
/// `<pkg>/app/resources/app.asar`) and the native electron-builder `.exe`
/// layout (`<root>/Claude.exe` + `<root>/resources/app.asar`). MSIX is
/// preferred so an existing AppContainer install keeps its identity launch.
fn resolve_claude_install_for_patch() -> Result<ClaudeInstall, String> {
    if !cfg!(target_os = "windows") {
        return Err(
            "Claude Desktop localization patching is only supported on Windows.".to_string(),
        );
    }
    let identities = claude_desktop_windows_package_identities();
    if let Some(installed) = package::detect_first_msix_package(identities) {
        let app_dir = Path::new(&installed.path).join("app");
        let resources = app_dir.join("resources");
        let exe = app_dir.join("Claude.exe");
        return Ok(ClaudeInstall {
            kind: ClaudeInstallKind::Msix,
            patch_exe: exe.clone(),
            launcher_exe: exe,
            asar: resources.join("app.asar"),
            shell_locale: resources.join(CLAUDE_SHELL_ZH_LOCALE_FILE),
            ion_locale: resources.join(CLAUDE_ION_ZH_LOCALE_RELATIVE_PATH),
        });
    }
    // No MSIX package: fall back to the native electron-builder/Squirrel
    // install (winget's `.exe` installer). Its layout is:
    //   <root>/claude.exe            (tiny Squirrel launcher, no Electron fuse)
    //   <root>/app-<version>/claude.exe  (real Electron image + fuse)
    //   <root>/app-<version>/resources/app.asar
    // The fuse must be flipped on the app-<version> image, while activation
    // runs the top-level Squirrel launcher (which forwards to the newest
    // app-<version>/). Prefer the same broad, version-aware scan detection
    // uses (so localization resolves the exact same install + image the page
    // detected, with a real version label); fall back to the explicit
    // candidate list only when the scan misses the install location. The scan
    // returns a path that already passed an is_file() check, so trust it as the
    // patch target rather than re-deriving a (possibly different) path from the
    // install root.
    let (patch_exe, launcher, resources) = match claude_desktop_windows_native_install_path() {
        Some(detected) => resolve_patch_paths_from_detected(&detected)?,
        None => match find_windows_claude_exe() {
            Some(found) => {
                let root = found.parent().map(PathBuf::from).ok_or_else(|| {
                    "Unable to resolve Claude Desktop install directory.".to_string()
                })?;
                resolve_patch_paths_from_launcher(&found, &root)?
            }
            None => {
                return Err(
                    "Claude Desktop was not found; localization requires the installed app."
                        .to_string(),
                );
            }
        },
    };
    // Verify the resolved patch target and asar actually exist on disk before
    // the caller reads them, so a missing file surfaces a clear error instead
    // of a generic "Failed to read".
    if !patch_exe.is_file() {
        return Err(format!(
            "Claude Desktop install was found, but the application image was not found: {}",
            patch_exe.display()
        ));
    }
    if !resources.join("app.asar").is_file() {
        return Err(format!(
            "Claude Desktop install was found, but app.asar was not found: {}",
            resources.join("app.asar").display()
        ));
    }
    Ok(ClaudeInstall {
        kind: ClaudeInstallKind::Exe,
        patch_exe,
        launcher_exe: launcher,
        asar: resources.join("app.asar"),
        shell_locale: resources.join(CLAUDE_SHELL_ZH_LOCALE_FILE),
        ion_locale: resources.join(CLAUDE_ION_ZH_LOCALE_RELATIVE_PATH),
    })
}

/// Resolve patch/launcher/resources paths from a detection result that already
/// passed an is_file() check. The detected path is the Electron image to patch
/// (either `<root>/app-<version>/Claude.exe` or a bare `<root>/Claude.exe`),
/// so it is used directly as `patch_exe`. Resources sit next to the image under
/// `resources/`; the Squirrel launcher is the install root's `claude.exe`.
fn resolve_patch_paths_from_detected(
    detected: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let image_dir = detected
        .parent()
        .ok_or_else(|| "Unable to resolve Claude Desktop install directory.".to_string())?;
    let resources = image_dir.join("resources");
    // Squirrel layout: image is <root>/app-<version>/Claude.exe, launcher is
    // <root>/claude.exe. Bare layout: image is <root>/Claude.exe, launcher is
    // the image itself.
    let launcher = match image_dir.file_name().and_then(|n| n.to_str()) {
        Some(name) if name.starts_with("app-") => image_dir
            .parent()
            .map(|root| root.join("claude.exe"))
            .unwrap_or_else(|| PathBuf::from(detected)),
        _ => PathBuf::from(detected),
    };
    Ok((PathBuf::from(detected), launcher, resources))
}

/// Resolve patch/launcher/resources paths when only a launcher candidate was
/// found (no version-aware scan). Prefer an `app-<version>/Claude.exe` image
/// next to the launcher so the fuse is flipped on the real Electron binary;
/// fall back to the launcher itself when there is no `app-*` directory.
fn resolve_patch_paths_from_launcher(
    launcher: &Path,
    root: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    match find_squirrel_app_version_dir(root).map(PathBuf::from) {
        Some(dir) => Ok((
            dir.join("Claude.exe"),
            launcher.to_path_buf(),
            dir.join("resources"),
        )),
        None => Ok((
            launcher.to_path_buf(),
            launcher.to_path_buf(),
            root.join("resources"),
        )),
    }
}

/// Locate the newest `app-<version>/` directory under a Squirrel install
/// root (winget's electron-builder/NSIS layout). Returns the directory name
/// (e.g. `app-1.14271.0`) of the highest version, or `None` when the install
/// is not Squirrel-shaped (no `app-*` directories).
fn find_squirrel_app_version_dir(root: &Path) -> Option<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return None;
    };
    let mut versions: Vec<(String, Vec<u64>)> = Vec::new();
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
        // Parse dotted numeric version into a comparable vector so
        // app-1.14271.0 outranks app-1.13576.0 even across segment counts.
        let parts: Vec<u64> = version
            .split('.')
            .filter_map(|part| part.parse::<u64>().ok())
            .collect();
        if parts.is_empty() {
            continue;
        }
        versions.push((name.to_string(), parts));
    }
    versions.sort_by(|a, b| b.1.cmp(&a.1));
    versions.first().map(|(name, _)| name.clone())
}

/// Activate the patched Claude Desktop using whichever launch path matches
/// its install kind: MSIX apps are activated by app identity (preserving
/// their AppContainer/identity context and user-data redirection), while
/// native `.exe` installs are launched by running the patched launcher
/// directly (no argv — the in-place asar shim opens the inspector itself).
fn activate_localized_claude() -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("Claude Desktop activation is only supported on Windows.".to_string());
    }
    let install = resolve_claude_install_for_patch()?;
    match install.kind {
        // MSIX apps activate by app identity (preserves AppContainer context
        // and user-data redirection); the patch_exe is not run directly.
        ClaudeInstallKind::Msix => activate_localized_claude_msix(),
        // Squirrel `.exe` installs: run the top-level launcher (not the
        // patched app-<version>/claude.exe image) so Squirrel's version
        // selection still applies and the app starts in its normal context.
        // No argv — the in-place asar shim opens the Node inspector itself.
        ClaudeInstallKind::Exe => launch_windows_claude_exe(install.launcher_exe, &[]).map(|_| ()),
    }
}

/// Activate the patched Claude Desktop via MSIX app identity (no argv),
/// so it runs in its normal AppContainer/identity context with its real
/// user-data directory.
fn activate_localized_claude_msix() -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("Claude Desktop MSIX activation is only supported on Windows.".to_string());
    }
    let identities = claude_desktop_windows_package_identities();
    let installed = package::detect_first_msix_package(identities)
        .ok_or_else(|| "Claude Desktop MSIX package was not found.".to_string())?;
    let package_name = installed
        .package_family_name
        .as_deref()
        .and_then(|family| {
            identities
                .iter()
                .find(|identity| family.starts_with(&format!("{identity}_")))
                .copied()
        })
        .or_else(|| {
            identities
                .iter()
                .find(|identity| installed.path.contains(**identity))
                .copied()
        })
        .ok_or_else(|| "Unable to resolve Claude Desktop package identity.".to_string())?;
    package::launch_msix_package_with_args(package_name, &[])
        .map(|_| ())
        .map_err(|err| format!("Failed to activate Claude Desktop: {err}"))
}

/// Build the inspector shim source that runs as the asar entry point.
/// It opens the Node V8 inspector on `CLAUDE_INSPECTOR_OPEN_PORT`, then
/// loads the original main module. Errors are written to stderr but never
/// abort the main load, so a failed inspector open still leaves Claude
/// usable.
fn build_inspector_shim(original_main: &str) -> String {
    build_inspector_shim_with_payloads(
        original_main,
        TRANSLATION_RUNTIME,
        CLAUDE_SHELL_ZH_LOCALE,
        CLAUDE_ION_ZH_LOCALE,
        CLAUDE_ION_DYNAMIC_ZH_LOCALE,
    )
}

/// Build the inspector shim source that runs as the asar entry point.
/// It opens the Node V8 inspector on `CLAUDE_INSPECTOR_OPEN_PORT`, then
/// self-injects the Chinese localization into every renderer (attach the
/// webContents debugger, intercept en-US.json/zh-CN.json fetches and fulfill
/// them with the bundled zh-CN payloads, add the runtime script to new
/// documents, and reload), then loads the original main module. The
/// localization is self-contained in the shim so it does not depend on an
/// external injector process or a UI toggle: once the in-place patch is
/// applied, every Claude launch is localized. Errors are written to stderr
/// but never abort the main load, so a failed inspector open or injection
/// still leaves Claude usable (in English).
fn build_inspector_shim_with_payloads(
    original_main: &str,
    runtime_js: &str,
    shell_locale_json: &str,
    ion_locale_json: &str,
    dynamic_locale_json: &str,
) -> String {
    let runtime_literal = serde_json::to_string(runtime_js).unwrap_or_default();
    let shell_literal = serde_json::to_string(shell_locale_json).unwrap_or_default();
    let ion_literal = serde_json::to_string(ion_locale_json).unwrap_or_default();
    let dynamic_literal = serde_json::to_string(dynamic_locale_json).unwrap_or_default();
    let main_literal = serde_json::to_string(original_main).unwrap_or_default();
    // The shim runs as the asar entry point, before Claude's own main
    // module, so it can monkey-patch Electron/Node APIs before Claude
    // registers its handlers. Localization is enforced through several
    // cooperating mechanisms:
    //
    // 1. Renderer Fetch interception: the claude.ai webview (login page +
    //    main UI) loads its locale from https://claude.ai/i18n/en-US.json.
    //    The shim attaches each http(s) webContents' debugger, intercepts
    //    en-US/zh-CN locale fetches and fulfills them with the bundled
    //    zh-CN payloads, then reloads so the page reissues its locale
    //    request through the interceptor. The `/dynamic/` locale file is
    //    a small supplemental catalog and is left to pass through.
    // 2. Native-menu label translation: Claude's main-process i18n already
    //    localizes the app menu and tray menu for every shipped locale via
    //    formatMessage, and the renderer syncs the main-process locale
    //    through window.electronIntl.requestLocaleChange whenever the user
    //    picks a language (which calls sqi -> menu rebuild). The shim hooks
    //    Menu.setApplicationMenu and Tray.setContextMenu and rewrites labels
    //    to Chinese ONLY while zh-CN is the active locale: it detects zh-CN
    //    synchronously by checking whether the first top-level menu label
    //    contains CJK characters (menuIsZh), and polls spa:locale for the
    //    exact locale as a safety net for the tray re-translation. For every
    //    other locale the menu is left exactly as Claude built it; hard-coded
    //    English labels (no message id, e.g. "Enable Main Process Debugger")
    //    stay English, which is the en-US fallback the user expects when no
    //    translation exists. A hard-coded override table + an en->zh map
    //    (built from the on-disk en-US.json + bundled zh-CN) cover zh-CN.
    format!(
        r##"(function () {{
  try {{ require('node:inspector').open({port}); }} catch (e) {{ process.stderr.write('[csl] inspector open failed: ' + (e && e.message) + '\n'); }}
  var RUNTIME = {runtime_literal};
  var SHELL_LOCALE = {shell_literal};
  var ION_LOCALE = {ion_literal};
  var DYNAMIC_LOCALE = {dynamic_literal};
  var MAIN_MODULE = {main_literal};
  function localizedLaunchMarkerPath() {{
    try {{ return require('path').join(require('os').homedir(), ".codestudio-lite", "claude-desktop-patch", "localized-launch.flag"); }} catch (e) {{ return ""; }}
  }}
  function consumeLocalizedLaunchMarker() {{
    try {{
      var marker = localizedLaunchMarkerPath();
      if (!marker) return false;
      var fs = require('fs');
      var text = "";
      try {{ text = fs.readFileSync(marker, "utf8"); }} catch (e) {{ return false; }}
      try {{ fs.unlinkSync(marker); }} catch (e) {{}}
      return String(text || "").trim() === "zh-CN";
    }} catch (e) {{ return false; }}
  }}
  var localizedLaunchDefaultZh = consumeLocalizedLaunchMarker();
  function forceInitialLocale() {{
    try {{
      var electronFL = require('electron');
      var app = electronFL && electronFL.app;
      if (!localizedLaunchDefaultZh || !app) return;
      var zhList = ["zh-CN", "zh-Hans-CN", "en-US"];
      if (typeof app.commandLine === "object" && app.commandLine && typeof app.commandLine.appendSwitch === "function") {{
        try {{ app.commandLine.appendSwitch("lang", "zh-CN"); }} catch (e) {{}}
      }}
      if (typeof app.getLocale === "function") app.getLocale = function () {{ return "zh-CN"; }};
      if (typeof app.getSystemLocale === "function") app.getSystemLocale = function () {{ return "zh-CN"; }};
      if (typeof app.getPreferredSystemLanguages === "function") app.getPreferredSystemLanguages = function () {{ return zhList.slice(); }};
    }} catch (e) {{}}
  }}
  forceInitialLocale();
  var PATTERNS = [
    {{ urlPattern: "*ion-dist/i18n/zh-CN.json*" }},
    {{ urlPattern: "*ion-dist/i18n/en-US.json*" }},
    {{ urlPattern: "*/i18n/zh-CN.json*" }},
    {{ urlPattern: "*/i18n/en-US.json*" }},
    {{ urlPattern: "*/zh-CN.json*" }}
  ];
  function localePayloadForUrl(url) {{
    var bare = String(url || "").split("?")[0].split("#")[0].toLowerCase();
    var isZh = bare.endsWith("/zh-cn.json");
    var isEn = bare.endsWith("/en-us.json");
    var localLike = bare.indexOf("app://") === 0 || bare.indexOf("file://") === 0;
    // Only fulfill zh-CN requests by default; fulfill local en-US catalog
    // requests during a localized launch because local/preload windows can ask
    // DesktopIntl for en-US before renderer scripts can write spa:locale.
    if (!isZh && !(currentLocale === "zh-CN" && isEn && localLike)) return null;
    if (bare.indexOf("/dynamic/") >= 0) return DYNAMIC_LOCALE;
    if (bare.indexOf("/ion-dist/i18n/") >= 0 || bare.indexOf("/i18n/") >= 0) return ION_LOCALE;
    return SHELL_LOCALE;
  }}
  // contents.isDestroyed is a function, not a property; invoking it avoids the
  // truthy-reference bug where attach() always bailed before Fetch interception.
  function isDestroyed(c) {{ try {{ return typeof c.isDestroyed === "function" && c.isDestroyed(); }} catch (e) {{ return false; }} }}
  // (1) Renderer Fetch interception. Only http(s) pages (the claude.ai
  // webview) fetch locale; file:// renderers use window.initialLocale. async
  // attach so Fetch.enable completes before the reload reissues the locale
  // request.
  async function attach(contents) {{
    if (!contents || isDestroyed(contents)) return;
    var url = "";
    try {{ url = contents.getURL(); }} catch (e) {{}}
    // http(s) covers claude.ai (login + main UI); app:// covers the local
    // settings/setup renderers (e.g. setup-desktop-3p) that fetch their own
    // locale catalog from app://localhost/i18n/*.json.
    // Match by protocol prefix, not substring: a devtools:// URL carries
    // "https://" inside its query string (remoteBase=...), which would wrongly
    // pass an indexOf check and let attach() hijack the DevTools window.
    if (!url) return;
    var lower = url.toLowerCase();
    if (lower.indexOf("http://") !== 0 && lower.indexOf("https://") !== 0 && lower.indexOf("app://") !== 0 && lower.indexOf("file://") !== 0) return;
    if (contents.__cslZhAttached) return;
    try {{
      if (!contents.debugger.isAttached()) contents.debugger.attach("1.3");
    }} catch (e) {{ return; }}
    contents.__cslZhAttached = true;
    contents.debugger.on("message", function (_event, method, params) {{
    if (method !== "Fetch.requestPaused") return;
    var requestId = params && params.requestId;
    if (!requestId) return;
    var url = params && params.request && params.request.url;
    // Response-stage interception of JS chunks: patch the locale whitelist
    // (zH) array in the bundled JS so zh-CN is a real array member, not just a
    // prototype-includes false positive. The left-corner language menu maps
    // over this array directly, so it must contain "zh-CN" for Chinese
    // (Simplified) to appear as a selectable option.
    var payload = localePayloadForUrl(url);
    if (payload) {{
      contents.debugger.sendCommand("Fetch.fulfillRequest", {{
        requestId: requestId,
        responseCode: 200,
        responseHeaders: [
          {{ name: "Content-Type", value: "application/json; charset=utf-8" }},
          {{ name: "Cache-Control", value: "no-store" }},
          {{ name: "Access-Control-Allow-Origin", value: "*" }}
        ],
        body: Buffer.from(payload, "utf8").toString("base64")
      }}).catch(function () {{}});
    }} else {{
      contents.debugger.sendCommand("Fetch.continueRequest", {{ requestId: requestId }}).catch(function () {{}});
    }}
  }});
  try {{
    await contents.debugger.sendCommand("Page.enable", {{}});
    await contents.debugger.sendCommand("Fetch.enable", {{ patterns: PATTERNS }});
    await contents.debugger.sendCommand("Page.addScriptToEvaluateOnNewDocument", {{ source: "var __CSL_LL=" + (localizedLaunchDefaultZh ? "!0" : "!1") + ";if(__CSL_LL&&!sessionStorage.getItem('__CSL_LL_DONE'))try{{localStorage.removeItem('spa:locale');sessionStorage.setItem('__CSL_LL_DONE','1')}}catch(e){{}};" + RUNTIME }});
    // The reload is essential: it forces the page to re-request its locale
    // JSON through the Fetch interceptor (which fulfills it with zh-CN) and
    // re-runs the runtime script registered above. Without it the page stays
    // in English because the locale was already fetched before interception.
    // However, reloading while the user has unsent input causes
    // "Your previous message wasn't sent". Guard: probe the page state first.
    // Skip the reload only if the page is already zh-CN (already localized)
    // or fully loaded with unsent user text. In the skip case, inject the
    // runtime directly so locale whitelist + text patching still apply.
    var skipReload = false;
    try {{
      var probe = await contents.executeJavaScript('(function(){{try{{var l=localStorage.getItem("spa:locale");var r=document.readyState;var el=document.querySelector("textarea,[contenteditable]");var t=el?(el.value||el.innerText||"").trim():"";return l+"|"+r+"|"+(t.length>0?1:0)}}catch(e){{return"||||"}}}})()', true);
      var parts = String(probe || "").split("|");
      if (parts[0] === "zh-CN") skipReload = true;
      else if (parts[1] === "complete" && parts[2] === "1") skipReload = true;
    }} catch (e) {{}}
    if (!skipReload) {{ try {{ contents.reload(); }} catch (_) {{}} }}
    else {{ try {{ await contents.executeJavaScript("var __CSL_LL=" + (localizedLaunchDefaultZh ? "!0" : "!1") + ";" + RUNTIME, true); }} catch (e) {{}} }}
  }} catch (e) {{
    process.stderr.write('[csl] localize attach failed: ' + (e && e.message) + '\n');
  }}
  }}
  function attachAll() {{
    try {{
      var electron = require('electron');
      var wc = electron.webContents;
      var all = typeof wc.getAllWebContents === "function" ? wc.getAllWebContents() : [];
      electron.BrowserWindow.getAllWindows().forEach(function (w) {{
        if (w.webContents && all.indexOf(w.webContents) < 0) all.push(w.webContents);
      }});
      all.forEach(function (c) {{ attach(c); }});
    }} catch (e) {{}}
  }}
  // Locale tracking: the renderer syncs the main-process locale through
  // window.electronIntl.requestLocaleChange (which calls sqi -> menu
  // rebuild). The shim learns the active locale by wrapping the
  // requestLocaleChange ipcMain.handle registration (synchronously, before
  // the rebuild calls the menu hook) and by polling the renderer's
  // spa:locale as a safety net. Menu/tray/title translation runs only for
  // zh-CN; every other locale is left as Claude built it.
  var currentLocale = localizedLaunchDefaultZh ? "zh-CN" : "en-US";
  function zhActive() {{ return currentLocale === "zh-CN"; }}
  var localeChangeListeners = [];
  function fireLocaleChange(loc) {{ for (var i = 0; i < localeChangeListeners.length; i++) {{ try {{ localeChangeListeners[i](loc); }} catch (e) {{}} }} }}
  function safeLocaleForLocalWindow(loc) {{
    if (typeof loc !== "string" || !loc) loc = "en-US";
    if (loc === "zh-CN") return loc;
    try {{
      var fsL = require('fs');
      var pathL = require('path');
      if (fsL.existsSync(pathL.join(process.resourcesPath, loc + ".json"))) return loc;
      if (fsL.existsSync(pathL.join(process.resourcesPath, "ion-dist", "i18n", loc + ".json"))) return loc;
    }} catch (e) {{}}
    return "en-US";
  }}
  function isSyncableUrl(lower) {{
    return lower.indexOf("http://") === 0 || lower.indexOf("https://") === 0 || lower.indexOf("app://") === 0 || lower.indexOf("file://") === 0 || lower.indexOf("about:blank") === 0;
  }}
  function localLocalePage(lower) {{
    return lower.indexOf("app://") === 0 || lower.indexOf("/settings") >= 0 || lower.indexOf("setup") >= 0 || lower.indexOf("third-party") >= 0 || lower.indexOf("inference") >= 0 || lower.indexOf("developer") >= 0;
  }}
  function syncOneWindowLocale(contents, target) {{
    try {{
      if (!contents || isDestroyed(contents)) return;
      var url = "";
      try {{ url = contents.getURL(); }} catch (e) {{}}
      var lower = String(url || "").toLowerCase();
      if (!isSyncableUrl(lower)) return;
      var localPage = localLocalePage(lower);
      var localLike = localPage || lower.indexOf("file://") === 0 || lower.indexOf("about:blank") === 0;
      var remoteClaude = lower.indexOf("https://claude.ai") === 0 || lower.indexOf("http://claude.ai") === 0;
      if (remoteClaude && !localPage) return;
      var loc = localLike ? safeLocaleForLocalWindow(target) : target;
      var q = JSON.stringify(loc);
      var js = 'try{{localStorage.setItem("spa:locale",' + q + ');document.documentElement&&document.documentElement.setAttribute("lang",' + q + ');window.dispatchEvent(new StorageEvent("storage",{{key:"spa:locale",newValue:' + q + '}}));window.dispatchEvent(new CustomEvent("claude-locale-change",{{detail:' + q + '}}));true}}catch(e){{false}}';
      contents.executeJavaScript(js, true).catch(function () {{}});
      if (localPage && contents.__cslLocaleReloaded !== loc) {{
        contents.__cslLocaleReloaded = loc;
        setTimeout(function () {{ try {{ if (!isDestroyed(contents)) {{ if (typeof contents.reloadIgnoringCache === "function") contents.reloadIgnoringCache(); else contents.reload(); }} }} catch (e) {{}} }}, 80);
      }}
    }} catch (e) {{}}
  }}
  function syncOpenWindowsLocale(target) {{
    try {{
      var electronSWL = require('electron');
      var all = [];
      try {{ all = electronSWL.webContents.getAllWebContents(); }} catch (e) {{ all = []; }}
      try {{ electronSWL.BrowserWindow.getAllWindows().forEach(function (w) {{ if (w.webContents && all.indexOf(w.webContents) < 0) all.push(w.webContents); }}); }} catch (e) {{}}
      for (var i = 0; i < all.length; i++) syncOneWindowLocale(all[i], target);
    }} catch (e) {{}}
  }}
  localeChangeListeners.push(syncOpenWindowsLocale);
  syncOpenWindowsLocale(currentLocale);
  // Locale detection: Claude's main-process i18n builds the app menu and
  // tray menu via formatMessage in the active locale. When zh-CN is active,
  // the first top-level label ("文件") contains CJK characters; in every
  // other locale it is non-CJK ("File", "Fichier", "Datei", …). The shim
  // detects this synchronously inside the Menu.setApplicationMenu hook —
  // exactly when the menu is set — so currentLocale updates before any
  // label translation runs. This avoids fragile IPC wrapping (Claude
  // registers requestLocaleChange via webContents.ipc, a per-instance
  // IpcMainImpl not exposed on WebContents.prototype). pollLocale reads
  // spa:locale for the exact locale (e.g. fr-FR) as a safety net so the
  // tray re-translation can use the right locale catalog.
  var CJK_RE = /[\u4e00-\u9fff]/;
  function menuIsZh(menu) {{
    try {{ var f = menu && menu.items && menu.items[0] && menu.items[0].label; return typeof f === "string" && CJK_RE.test(f); }} catch (e) {{ return false; }}
  }}
  function updateLocaleFromMenu(menu) {{
    var loc = menuIsZh(menu) ? "zh-CN" : "en-US";
    if (loc !== currentLocale) {{ currentLocale = loc; fireLocaleChange(loc); }}
  }}
  async function pollLocale() {{
    try {{
      var electronPL = require('electron');
      var all = electronPL.webContents.getAllWebContents();
      for (var i = 0; i < all.length; i++) {{
        var wc = all[i];
        var u = "";
        try {{ u = wc.getURL(); }} catch (e) {{}}
        if (u && u.toLowerCase().indexOf("https://claude.ai") === 0) {{
          var loc = await wc.executeJavaScript('localStorage.getItem("spa:locale")', true);
          if (typeof loc === "string" && loc && loc !== currentLocale) {{ currentLocale = loc; fireLocaleChange(loc); }}
          break;
        }}
      }}
    }} catch (e) {{}}
  }}
  setInterval(function () {{ pollLocale(); }}, 2000);
  pollLocale();
  // (2) Native-menu label translation. Build an en->zh map from the on-disk
  // en-US.json and the bundled zh-CN, then hook menu installation to walk
  // and translate labels only while zh-CN is active. A hard-coded override
  // table covers labels with no message id (Developer submenu items).
  try {{
    var fs = require('fs');
    var path = require('path');
    var electron = require('electron');
    var enToZh = {{}};
    try {{
      var enObj = JSON.parse(fs.readFileSync(path.join(process.resourcesPath, "en-US.json"), "utf8"));
      var zhObj = JSON.parse(SHELL_LOCALE);
      for (var k in enObj) {{ if (zhObj[k]) enToZh[enObj[k]] = zhObj[k]; }}
    }} catch (e) {{}}
    var HARDCODED_ZH = {{
      "Enable Main Process Debugger": "\u542f\u7528\u4e3b\u8fdb\u7a0b\u8c03\u8bd5\u5668",
      "Record Performance Trace": "\u5f55\u5236\u6027\u80fd\u8ddf\u8e2a",
      "Write Main Process Heap Snapshot": "\u5199\u5165\u4e3b\u8fdb\u7a0b\u5806\u5feb\u7167",
      "Record Memory Trace (auto-stop)": "\u5f55\u5236\u5185\u5b58\u8ddf\u8e2a\uff08\u81ea\u52a8\u505c\u6b62\uff09",
      "Paste and Match Style": "\u7c98\u8d34\u5e76\u5339\u914d\u6837\u5f0f",
      "Zoom In (numpad)": "\u653e\u5927\uff08\u5c0f\u952e\u76d8\uff09",
      "Zoom Out (numpad)": "\u7f29\u5c0f\uff08\u5c0f\u952e\u76d8\uff09",
      "Actual Size (numpad)": "\u5b9e\u9645\u5927\u5c0f\uff08\u5c0f\u952e\u76d8\uff09"
    }};
    function translateLabel(label) {{
      if (typeof label !== "string" || !label) return label;
      if (HARDCODED_ZH[label]) return HARDCODED_ZH[label];
      if (enToZh[label]) return enToZh[label];
      return label;
    }}
    function translateMenuItems(menu) {{
      if (!menu || !menu.items) return menu;
      updateLocaleFromMenu(menu);
      if (!zhActive()) return menu;
      menu.items.forEach(function (item) {{
        try {{
          if (typeof item.label === "string") {{
            if (item.__cslOrig === undefined) item.__cslOrig = item.label;
            item.label = translateLabel(item.label);
          }}
          if (item.submenu) translateMenuItems(item.submenu);
        }} catch (e) {{}}
      }});
      return menu;
    }}
    var Menu = electron.Menu;
    var origSetAppMenu = Menu.setApplicationMenu.bind(Menu);
    Menu.setApplicationMenu = function (menu) {{ try {{ translateMenuItems(menu); }} catch (e) {{}} return origSetAppMenu(menu); }};
    var Tray = electron.Tray;
    var trayMenu = null;
    var trayRef = null;
    var zhValToId = {{}};
    try {{ for (var zid in zhObj) {{ if (zhObj[zid] && typeof zhObj[zid] === "string" && !(zhObj[zid] in zhValToId)) zhValToId[zhObj[zid]] = zid; }} }} catch (e) {{}}
    function relabelTray(menu, target, idToVal) {{
      if (!menu || !menu.items) return;
      menu.items.forEach(function (item) {{
        try {{
          var orig = item.__cslOrig;
          if (typeof orig === "string" && orig) {{
            if (target === "zh-CN") {{
              if (HARDCODED_ZH[orig]) item.label = HARDCODED_ZH[orig];
              else if (enToZh[orig]) item.label = enToZh[orig];
              else item.label = orig;
            }} else {{
              var rid = zhValToId[orig];
              if (rid && idToVal[rid]) item.label = idToVal[rid];
              else item.label = orig;
            }}
          }}
          if (item.submenu) relabelTray(item.submenu, target, idToVal);
        }} catch (e) {{}}
      }});
    }}
    var origTraySetCtx = null;
    function retranslateTray(target) {{
      try {{
        if (!trayMenu || !trayMenu.items || !origTraySetCtx || !trayRef) return;
        var idToVal = {{}};
        if (target !== "zh-CN") {{
          try {{ var tobj = JSON.parse(fs.readFileSync(path.join(process.resourcesPath, target + ".json"), "utf8")); for (var tid in tobj) {{ if (tobj[tid]) idToVal[tid] = tobj[tid]; }} }} catch (e) {{ return; }}
        }}
        relabelTray(trayMenu, target, idToVal);
        origTraySetCtx.call(trayRef, trayMenu);
      }} catch (e) {{}}
    }}
    if (Tray && Tray.prototype) {{
      origTraySetCtx = Tray.prototype.setContextMenu;
      if (origTraySetCtx) {{
        Tray.prototype.setContextMenu = function (menu) {{
          try {{
            if (menu && menu.items) {{
              (function cap(m) {{ if (!m || !m.items) return; m.items.forEach(function (it) {{ try {{ if (typeof it.label === "string" && it.__cslOrig === undefined) it.__cslOrig = it.label; if (it.submenu) cap(it.submenu); }} catch (e) {{}} }}); }})(menu);
              translateMenuItems(menu);
              trayMenu = menu; trayRef = this;
            }}
          }} catch (e) {{}}
          return origTraySetCtx.call(this, menu);
        }};
      }}
    }}
    localeChangeListeners.push(retranslateTray);
  }} catch (e) {{}}
  try {{
    var electron3 = require('electron');
    var app = electron3.app;
    if (app && typeof app.on === "function") {{
      app.on("browser-window-created", function (_event, window) {{
        setTimeout(function () {{ try {{ syncOneWindowLocale(window.webContents, currentLocale); attach(window.webContents); }} catch (e) {{}} }}, 50);
        try {{
          var wc = window.webContents;
          var SETUP_TITLES = {{ "setup-desktop-3p": "\u914d\u7f6e\u7b2c\u4e09\u65b9\u0041\u0050\u0049", "devtools://devtools": "\u5f00\u53d1\u8005\u5de5\u5177" }};
          var SETUP_TITLES_EN = {{ "setup-desktop-3p": "Configure Third-Party Inference\u2026", "devtools://devtools": "DevTools" }};
          function applySetupTitle() {{ try {{ var u = wc.getURL(); for (var k in SETUP_TITLES) {{ if (u.indexOf(k) >= 0) {{ var want = zhActive() ? SETUP_TITLES[k] : (SETUP_TITLES_EN[k] || SETUP_TITLES[k]); if (window.getTitle() !== want) window.setTitle(want); return; }} }} }} catch (e) {{}} }}
          function applySetupWindowState() {{ try {{ syncOneWindowLocale(wc, currentLocale); }} catch (e) {{}} applySetupTitle(); }}
          wc.on("did-finish-load", applySetupWindowState);
          wc.on("did-navigate", applySetupWindowState);
          applySetupWindowState();
          setInterval(applySetupWindowState, 2000);
        }} catch (e) {{}}
      }});
    }}
    setInterval(attachAll, 2000);
    // DevTools windows are not in BrowserWindow.getAllWindows() and do not
    // trigger browser-window-created, so SETUP_TITLES cannot reach them. Their
    // document.title is hard-coded to "DevTools"; retitle via executeJavaScript
    // (no debugger attach, so no white-screen risk) and poll to keep it.
    function fixDevToolsTitles() {{
      try {{
        var all = electron3.webContents.getAllWebContents();
        for (var i = 0; i < all.length; i++) {{
          try {{
            var c = all[i];
            var u = c.getURL();
            if (u && u.toLowerCase().indexOf("devtools://") === 0) {{
              if (zhActive()) {{
                c.executeJavaScript('try{{if(document.title!==\"\u5f00\u53d1\u8005\u5de5\u5177\")document.title=\"\u5f00\u53d1\u8005\u5de5\u5177\"}}catch(e){{}}', true).catch(function () {{}});
              }} else {{
                c.executeJavaScript('try{{if(document.title!==\"DevTools\")document.title=\"DevTools\"}}catch(e){{}}', true).catch(function () {{}});
              }}
            }}
          }} catch (e) {{}}
        }}
      }} catch (e) {{}}
    }}
    setInterval(fixDevToolsTitles, 2000);
  }} catch (e) {{}}
  try {{ require('./' + MAIN_MODULE); }} catch (e) {{ process.stderr.write('[csl] main load failed: ' + (e && e.message) + '\n'); }}
}})();
"##,
        port = CLAUDE_INSPECTOR_OPEN_PORT,
        runtime_literal = runtime_literal,
        shell_literal = shell_literal,
        ion_literal = ion_literal,
        dynamic_literal = dynamic_literal,
        main_literal = main_literal,
    )
}
/// Build the rewritten package.json: `main` becomes the inspector shim,
/// and the original main is preserved as `originalMain`.
fn build_patched_package_json(original_text: &str, original_main: &str) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(original_text)
        .map_err(|err| format!("Failed to parse Claude package.json: {err}"))?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "Claude package.json is not an object.".to_string())?;
    obj.insert("main".to_string(), Value::from(CLAUDE_INSPECTOR_SHIM_NAME));
    obj.insert("originalMain".to_string(), Value::from(original_main));
    Ok(serde_json::to_string_pretty(&value)
        .map_err(|err| format!("Failed to serialize patched package.json: {err}"))?)
}

/// Paths inside the installed Claude Desktop app that we patch in place.
struct ClaudePatchPaths {
    exe: PathBuf,
    asar: PathBuf,
    shell_locale: PathBuf,
    ion_locale: PathBuf,
}

/// Locate the fuse byte offset for `EnableEmbeddedAsarIntegrityValidation`
/// (fuse index 4) inside a Claude.exe byte buffer. Returns the absolute
/// offset of the '0'/'1' status byte.
fn fuse_integrity_offset(exe_bytes: &[u8]) -> Option<usize> {
    let marker_start = exe_bytes
        .windows(CLAUDE_FUSE_MARKER.len())
        .position(|window| window == CLAUDE_FUSE_MARKER)?;
    // Wire format after marker: 1 sentinel byte, 1 count byte, then `count`
    // ASCII fuse status bytes. Fuse index 4 is the integrity flag.
    let offset = marker_start + CLAUDE_FUSE_MARKER.len() + 2 + CLAUDE_FUSE_INTEGRITY_INDEX;
    if offset < exe_bytes.len() {
        Some(offset)
    } else {
        None
    }
}

/// True when the installed Claude.exe already has its asar-integrity fuse
/// disabled (byte value '0' / 0x30) at the integrity offset.
fn fuse_integrity_disabled(exe_bytes: &[u8]) -> bool {
    fuse_integrity_offset(exe_bytes)
        .map(|offset| exe_bytes[offset] == b'0')
        .unwrap_or(false)
}

/// True when the installed app.asar already loads our inspector shim as
/// its entry point.
fn asar_already_patched(asar_bytes: &[u8]) -> bool {
    crate::core::asar_archive::read_package_json(asar_bytes)
        .map(|(_text, main)| main == CLAUDE_INSPECTOR_SHIM_NAME)
        .unwrap_or(false)
}

/// True when the on-disk locale file at the given path already matches the
/// bundled zh-CN payload byte-for-byte (so we can skip rewriting it).
fn locale_file_matches(path: &Path, expected: &str) -> bool {
    fs::read_to_string(path)
        .map(|current| current == expected)
        .unwrap_or(false)
}

/// True when the patched asar's inspector shim points MAIN_MODULE at itself.
/// Re-patching an already-patched asar reads the rewritten package.json (whose
/// main is the shim) as the original main, so the shim's MAIN_MODULE becomes the
/// shim filename: require('./_csl_inspector_shim.js') loads the in-progress module
/// (cached empty exports) instead of Claude's real main, no BrowserWindow is
/// created, and Claude appears to hang on launch. Detect this so the asar is
/// rewritten even when asar_already_patched is true.
fn asar_shim_self_references(asar_bytes: &[u8]) -> bool {
    let Ok(shim) =
        crate::core::asar_archive::read_named_file(asar_bytes, CLAUDE_INSPECTOR_SHIM_NAME)
    else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(&shim) else {
        return false;
    };
    // The shim stores MAIN_MODULE as a serde_json string literal, e.g.
    //   var MAIN_MODULE = "_csl_inspector_shim.js";
    // A self-reference is that literal equal to the shim filename.
    let needle = format!(
        "var MAIN_MODULE = {quoted};",
        quoted = serde_json::to_string(CLAUDE_INSPECTOR_SHIM_NAME).unwrap_or_default()
    );
    text.contains(&needle)
}

/// True when the asar contains a packed file at the given slash-separated path.
/// True when the patched asar's inspector shim predates the locale-whitelist
/// redesign. The old shim forces zh-CN and disables English (contains
/// `forceZh` and `installLocalePreference` but lacks `installLocaleWhitelist`);
/// we re-inject the new shim so zh-CN becomes a selectable language instead.
fn asar_shim_needs_update(asar_bytes: &[u8]) -> bool {
    let Ok(shim) =
        crate::core::asar_archive::read_named_file(asar_bytes, CLAUDE_INSPECTOR_SHIM_NAME)
    else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(&shim) else {
        return false;
    };
    // A real shim never embeds its own filename (MAIN_MODULE points at the
    // original .vite main, not the shim), so we must not gate on the shim
    // name being present. read_named_file already confirmed the shim file
    // exists; if it lacks the redesign signatures, re-inject it.
    !text.contains("installLocaleWhitelist")
        || !text.contains("Array.prototype.map")
        || !text.contains("account_profile")
        || !text.contains("withZh")
        || !text.contains("fixLanguageRadio")
        || !text.contains("overrides.json")
        || !text.contains("gated_messages")
        || !text.contains("zhActive")
        || !text.contains("menuIsZh")
        || !text.contains("updateLocaleFromMenu")
        || !text.contains("Currently unavailable")
        || !text.contains("For more complex tasks")
        || !text.contains("For complex tasks")
        || !text.contains("syncOpenWindowsLocale")
        || !text.contains("localizedLaunchDefaultZh")
        || !text.contains("localized-launch.flag")
        || !text.contains("getSystemLocale")
        || !text.contains("ion-dist/i18n/en-US.json")
        || !text.contains("__CSL_LL")
        || !text.contains("__CSL_LL_DONE")
        || !text.contains("Set.prototype")
        || !text.contains("isZhSet")
        || !text.contains("skipReload")
}

fn asar_contains_file(asar_bytes: &[u8], path: &str) -> bool {
    let Ok(header) = crate::core::asar_archive::read_header(asar_bytes) else {
        return false;
    };
    let mut node = &header.tree;
    for part in path.split('/') {
        let Some(child) = node
            .get("files")
            .and_then(Value::as_object)
            .and_then(|f| f.get(part))
        else {
            return false;
        };
        node = child;
    }
    node.get("size").is_some()
}

/// Recover Claude's true original main module from a possibly already-patched
/// asar. When the asar was patched before, package.json main is our shim and the
/// real entry is preserved in originalMain; if that field was also clobbered by
/// repeated re-patching (it holds the shim name too), fall back to probing the
/// asar file tree for Claude's known main entry candidates.
fn recover_original_main(pkg_text: &str, read_main: String, asar_bytes: &[u8]) -> String {
    if read_main != CLAUDE_INSPECTOR_SHIM_NAME {
        return read_main;
    }
    if let Ok(pkg) = serde_json::from_str::<Value>(pkg_text) {
        if let Some(orig) = pkg.get("originalMain").and_then(Value::as_str) {
            if !orig.is_empty() && orig != CLAUDE_INSPECTOR_SHIM_NAME {
                return orig.to_string();
            }
        }
    }
    for candidate in [
        ".vite/build/index.pre.js",
        ".vite/build/index.js",
        "index.js",
        "main.js",
    ] {
        if asar_contains_file(asar_bytes, candidate) {
            return candidate.to_string();
        }
    }
    read_main
}

/// Build the body of the elevated PowerShell script that patches the
/// installed Claude.exe fuse and rewrites app.asar in place. The script
/// takes ownership, grants Administrators write access, writes the patched
/// bytes, then restores the original ACL. `patched_exe` / `patched_asar`
/// are temp paths the host already wrote; the script copies them over the
/// originals from the elevated context.
fn elevated_patch_script(
    exe: &Path,
    asar: &Path,
    patched_exe: &Path,
    patched_asar: &Path,
    shell_locale: &Path,
    patched_shell_locale: &Path,
    ion_locale: &Path,
    patched_ion_locale: &Path,
) -> String {
    let exe_str = ps_single_quote(&exe.to_string_lossy());
    let asar_str = ps_single_quote(&asar.to_string_lossy());
    let patched_exe_str = ps_single_quote(&patched_exe.to_string_lossy());
    let patched_asar_str = ps_single_quote(&patched_asar.to_string_lossy());
    let shell_locale_str = ps_single_quote(&shell_locale.to_string_lossy());
    let patched_shell_locale_str = ps_single_quote(&patched_shell_locale.to_string_lossy());
    let ion_locale_str = ps_single_quote(&ion_locale.to_string_lossy());
    let patched_ion_locale_str = ps_single_quote(&patched_ion_locale.to_string_lossy());
    let ion_locale_dir = ps_single_quote(
        &ion_locale
            .parent()
            .map(|p| p.to_string_lossy())
            .unwrap_or_default(),
    );
    format!(
        r#"$ErrorActionPreference = 'Stop'
$exitReason = ''
function Grant-Write($path) {{
  if (Test-Path $path) {{
    takeown /F $path /A | Out-Null
    icacls $path /grant 'Administrators:F' | Out-Null
  }} else {{
    $parent = Split-Path -Parent $path
    if (Test-Path $parent) {{
      takeown /F $parent /A | Out-Null
      icacls $parent /grant 'Administrators:F' | Out-Null
    }}
  }}
}}
function Restore-Acl($path) {{
  if (Test-Path $path) {{ icacls $path /remove 'Administrators' | Out-Null }}
}}
function Copy-WithRetry($src, $dst, $retries) {{
  for ($i = 0; $i -lt $retries; $i++) {{
    try {{
      Copy-Item -LiteralPath $src -Destination $dst -Force
      return $true
    }} catch {{
      $exitReason = "Copy-Item $src -> $dst failed (attempt $($i+1)/$retries): $($_.Exception.Message)"
      if ($i -lt $retries - 1) {{ Start-Sleep -Milliseconds 500 }}
    }}
  }}
  return $false
}}
try {{
  Grant-Write {exe_str}
  Grant-Write {asar_str}
  Grant-Write {shell_locale_str}
  Grant-Write {ion_locale_dir}
  $ok1 = Copy-WithRetry {patched_exe_str} {exe_str} 5
  $ok2 = Copy-WithRetry {patched_asar_str} {asar_str} 5
  $ok3 = Copy-WithRetry {patched_shell_locale_str} {shell_locale_str} 5
  $ok4 = Copy-WithRetry {patched_ion_locale_str} {ion_locale_str} 5
  if (-not ($ok1 -and $ok2 -and $ok3 -and $ok4)) {{
    Write-Error $exitReason
    exit 2
  }}
  Restore-Acl {asar_str}
  Restore-Acl {exe_str}
  Restore-Acl {shell_locale_str}
  Restore-Acl {ion_locale_dir}
}} catch {{
  Write-Error $_.Exception.Message
  exit 3
}}
"#
    )
}

/// Ensure the localization patch is applied to the installed Claude
/// Desktop: the asar-integrity fuse on Claude.exe is disabled and app.asar
/// is rewritten so its entry point opens the Node inspector. Both edits are
/// in place (zero extra footprint once applied). If the patch is already in
/// place, this is a no-op. Writing the protected WindowsApps files requires
/// elevation, so the actual write happens through an elevated PowerShell
/// script; the patched bytes are prepared in a temp dir first.
fn apply_localization_patch() -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err(
            "Claude Desktop localization patching is only supported on Windows.".to_string(),
        );
    }
    let install = resolve_claude_install_for_patch()?;
    let paths = ClaudePatchPaths {
        exe: install.patch_exe.clone(),
        asar: install.asar.clone(),
        shell_locale: install.shell_locale.clone(),
        ion_locale: install.ion_locale.clone(),
    };

    let exe_bytes =
        fs::read(&paths.exe).map_err(|err| format!("Failed to read Claude.exe: {err}"))?;
    let asar_bytes =
        fs::read(&paths.asar).map_err(|err| format!("Failed to read app.asar: {err}"))?;

    let exe_needs_patch = !fuse_integrity_disabled(&exe_bytes);
    // Re-patching an already-patched asar can self-reference MAIN_MODULE (see
    // asar_shim_self_references); treat that as needing a rewrite too.
    let asar_needs_patch = !asar_already_patched(&asar_bytes)
        || asar_shim_self_references(&asar_bytes)
        || asar_shim_needs_update(&asar_bytes);
    let shell_locale_needs_patch =
        !locale_file_matches(&paths.shell_locale, CLAUDE_SHELL_ZH_LOCALE);
    let ion_locale_needs_patch = !locale_file_matches(&paths.ion_locale, CLAUDE_ION_ZH_LOCALE);
    if !exe_needs_patch && !asar_needs_patch && !shell_locale_needs_patch && !ion_locale_needs_patch
    {
        return Ok(());
    }

    // Prepare patched asar bytes in memory (surgeon-style append: original
    // file content stays at its offsets; a new package.json and the inspector
    // shim are appended).
    let (original_pkg_text, read_main) = crate::core::asar_archive::read_package_json(&asar_bytes)
        .map_err(|err| format!("Failed to read Claude app.asar: {err}"))?;
    // If the asar was already patched, package.json main is our shim; recover
    // Claude's true original main so the shim does not require() itself.
    let original_main = recover_original_main(&original_pkg_text, read_main, &asar_bytes);
    let new_pkg = build_patched_package_json(&original_pkg_text, &original_main)?;
    let shim = build_inspector_shim(&original_main);
    let patched_asar = crate::core::asar_archive::build_patched_asar(
        &asar_bytes,
        new_pkg.as_bytes(),
        CLAUDE_INSPECTOR_SHIM_NAME,
        shim.as_bytes(),
    )
    .map_err(|err| format!("Failed to build patched app.asar: {err}"))?;

    // Prepare patched exe bytes (flip the one fuse byte).
    let mut patched_exe = exe_bytes.clone();
    let fuse_offset = fuse_integrity_offset(&patched_exe).ok_or_else(|| {
        "Claude.exe Electron fuse marker was not found; cannot disable asar integrity.".to_string()
    })?;
    patched_exe[fuse_offset] = b'0';

    // Write the patched blobs and the zh-CN locale files to a temp dir the
    // elevated script can read. The locale files are placed in the resources
    // directory so Claude's built-in locale selection (nqi/oqi) discovers
    // zh-CN and the inspector shim can keep it selected.
    let patch_dir = ensure_patch_files()?;
    let temp_exe = patch_dir.join("Claude.patched.exe");
    let temp_asar = patch_dir.join("app.patched.asar");
    let temp_shell_locale = patch_dir.join(CLAUDE_SHELL_ZH_LOCALE_FILE);
    let temp_ion_locale = patch_dir.join("ion-zh-CN.json");
    fs::write(&temp_exe, &patched_exe)
        .map_err(|err| format!("Failed to write patched Claude.exe: {err}"))?;
    fs::write(&temp_asar, &patched_asar)
        .map_err(|err| format!("Failed to write patched app.asar: {err}"))?;
    fs::write(&temp_shell_locale, CLAUDE_SHELL_ZH_LOCALE)
        .map_err(|err| format!("Failed to write zh-CN shell locale: {err}"))?;
    fs::write(&temp_ion_locale, CLAUDE_ION_ZH_LOCALE)
        .map_err(|err| format!("Failed to write zh-CN ion locale: {err}"))?;

    let script_path = patch_dir.join("apply-claude-patch.ps1");
    write_if_changed(
        &script_path,
        &elevated_patch_script(
            &paths.exe,
            &paths.asar,
            &temp_exe,
            &temp_asar,
            &paths.shell_locale,
            &temp_shell_locale,
            &paths.ion_locale,
            &temp_ion_locale,
        ),
    )?;
    run_elevated_powershell_script(&script_path)?;

    // Re-read the patched files from disk and verify the patch actually
    // landed: the elevated copy is synchronous now, but MSIX-managed
    // WindowsApps files can still roll back or be locked, and a stale read
    // would let the caller activate an unpatched Claude (English, no
    // inspector). This is the last guard before activation.
    let post_exe = fs::read(&paths.exe)
        .map_err(|err| format!("Failed to re-read patched Claude.exe: {err}"))?;
    let post_asar = fs::read(&paths.asar)
        .map_err(|err| format!("Failed to re-read patched app.asar: {err}"))?;
    if !fuse_integrity_disabled(&post_exe) {
        return Err(
            "Claude.exe fuse was not disabled after patching; the elevated write may have failed."
                .to_string(),
        );
    }
    if !asar_already_patched(&post_asar) {
        return Err(
            "Claude app.asar entry was not rewritten after patching; the elevated write may have failed."
                .to_string(),
        );
    }

    // Clean up the temp blobs; the real files are patched in place.
    let _ = fs::remove_file(&temp_exe);
    let _ = fs::remove_file(&temp_asar);
    let _ = fs::remove_file(&temp_shell_locale);
    let _ = fs::remove_file(&temp_ion_locale);
    Ok(())
}

fn find_windows_claude_exe() -> Option<PathBuf> {
    windows_claude_exe_candidates()
        .into_iter()
        .find(|path| path.is_file())
}

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

fn push_windows_claude_local_candidates(candidates: &mut Vec<PathBuf>, root: &Path) {
    candidates.push(root.join("Programs").join("Claude").join("Claude.exe"));
    candidates.push(root.join("Claude").join("Claude.exe"));
    // Native electron-builder/NSIS installer (winget's Anthropic.Claude on a
    // clean VM). Match the detector's candidate set so localization patching
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

fn push_windows_claude_program_files_candidates(candidates: &mut Vec<PathBuf>, root: &Path) {
    candidates.push(root.join("Claude").join("Claude.exe"));
    candidates.push(root.join("Anthropic").join("Claude").join("Claude.exe"));
}

fn windows_shell_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn ps_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn windows_launch_script(localize: bool) -> String {
    let args = "$argsList = @()".to_string();
    let localized_marker = if localize {
        r#"$markerDir = Join-Path $HOME '.codestudio-lite\claude-desktop-patch'
New-Item -ItemType Directory -Force -Path $markerDir | Out-Null
Set-Content -LiteralPath (Join-Path $markerDir 'localized-launch.flag') -Value 'zh-CN' -Encoding ASCII
"#
    } else {
        ""
    };
    // Both localized and non-localized launches activate the app by MSIX
    // app identity (shell:AppsFolder). The localized launch does not pass
    // debug arguments: the in-place app.asar patch makes the main process
    // open the Node inspector itself at runtime, so identity activation is
    // sufficient and preserves the package's user-data redirection.
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

#[cfg(test)]
fn build_locale_runtime_source() -> &'static str {
    TRANSLATION_RUNTIME
}

fn inject_localization() -> Result<usize, String> {
    // The inspector is opened by the patched app.asar entry shim itself
    // (same code path as the in-app "Developer -> Enable Main Process
    // Debugger" menu), so there is no cross-process signal to send and no
    // Claude PID required: just scan for the now-open inspector and inject.
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
    format!(
        r##"(async () => {{
  if (globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__) {{
    const summary = await globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__.refresh();
    return {{ ok: true, reused: true, ...summary }};
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
    if (!isZh && !(isEn && localLike)) return null;
    if (bare.includes("/dynamic/")) return dynamicLocale;
    if (bare.includes("/ion-dist/i18n/") || bare.includes("/i18n/")) return ionLocale;
    return shellLocale;
  }};

  const attach = async (contents) => {{
    if (!contents || contents.isDestroyed?.() || attached.has(contents)) return false;
    const url = contents.getURL?.() ?? "";
    if (!url || (!url.startsWith("http://") && !url.startsWith("https://") && !url.startsWith("app://") && !url.startsWith("file://"))) return false;
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
      await contents.debugger.sendCommand("Page.addScriptToEvaluateOnNewDocument", {{ source: "if(typeof __CSL_LL==='undefined')var __CSL_LL=!1;" + runtime }});
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

  const refresh = async () => {{
    const contents = allContents();
    const results = await Promise.all(contents.map((item) => attach(item).catch(() => false)));
    return {{
      attached: results.filter(Boolean).length,
      contents: contents.length,
      windows: BrowserWindow.getAllWindows().length
    }};
  }};

  app.on("browser-window-created", (_event, window) => {{
    setTimeout(() => attach(window.webContents).catch(() => {{}}), 50);
  }});
  const timer = setInterval(refresh, 2000);
  timer.unref?.();
  globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__ = {{ refresh }};
  const summary = await refresh();
  return {{ ok: true, reused: false, ...summary }};
}})()"##
    )
}

fn retry_inject_localization() -> Result<usize, String> {
    let mut last_error = "Claude DevTools endpoint was not available.".to_string();
    for _ in 0..CLAUDE_ZH_INJECTION_RETRY_COUNT {
        match inject_localization() {
            Ok(count) if count > 0 => return Ok(count),
            Ok(_) => {
                last_error =
                    "Claude Node inspector did not expose a matching Claude target.".to_string();
            }
            Err(err) => {
                last_error = err;
            }
        }
        thread::sleep(Duration::from_millis(CLAUDE_ZH_INJECTION_RETRY_MS));
    }
    Err(last_error)
}

fn run_elevated_powershell_script(script_path: &Path) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("Elevated PowerShell is only supported on Windows.".to_string());
    }
    run_elevated_powershell_script_windows(script_path)
}

#[cfg(windows)]
fn run_elevated_powershell_script_windows(script_path: &Path) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::mem::zeroed;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(once(0)).collect()
    }

    let operation = wide(OsStr::new("runas"));
    let file = wide(OsStr::new("powershell.exe"));
    let args = format!(
        "-NoLogo -NoProfile -ExecutionPolicy Bypass -File {}",
        windows_shell_quote(&script_path.to_string_lossy())
    );
    let params = wide(OsStr::new(&args));

    // Use ShellExecuteExW (not ShellExecuteW) with SEE_MASK_NOCLOSEPROCESS
    // so we receive the elevated process handle and can wait for it to
    // finish. The previous fire-and-forget launch returned before the
    // elevated copy completed, so the caller could activate Claude with
    // the still-unpatched app.asar (no inspector -> no Chinese). Waiting
    // here makes the patch-then-activate ordering reliable.
    let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = operation.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        return Err(format!(
            "ShellExecuteEx runas failed (Win32 error {}).",
            unsafe { windows_sys::Win32::Foundation::GetLastError() }
        ));
    }
    // WaitForSingleObject with INFINITE would block the UI thread if the
    // UAC prompt is left open; poll instead so cancellation stays responsive.
    let handle = info.hProcess;
    if handle.is_null() {
        return Ok(());
    }
    let timeout_ms = 120_000u32;
    let mut waited = 0u32;
    let step = 200u32;
    loop {
        let r = unsafe { WaitForSingleObject(handle, step) };
        if r == WAIT_OBJECT_0 {
            break;
        }
        if r == WAIT_TIMEOUT {
            waited += step;
            if waited >= timeout_ms {
                unsafe { CloseHandle(handle) };
                return Err(
                    "Claude patch elevation timed out (UAC prompt not answered).".to_string(),
                );
            }
            continue;
        }
        unsafe { CloseHandle(handle) };
        return Err(format!(
            "Waiting for elevated patch process failed (status {r})."
        ));
    }
    let mut exit_code: u32 = 1;
    let got = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    unsafe { CloseHandle(handle) };
    if got == 0 {
        return Err("Could not read elevated patch process exit code.".to_string());
    }
    if exit_code != 0 {
        let hint = match exit_code {
            2 => "a protected file copy failed after retries (Claude may still be running or WindowsApps is locked).".to_string(),
            3 => "the elevated PowerShell script threw an error (check takeown/icacls permissions).".to_string(),
            _ => "UAC may have been declined or the elevated process was terminated.".to_string(),
        };
        return Err(format!(
            "Claude in-place patch failed with exit code {exit_code}: {hint}"
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn run_elevated_powershell_script_windows(_script_path: &Path) -> Result<(), String> {
    Err("Elevated PowerShell is only supported on Windows.".to_string())
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
    let mut last_error = "Claude Node inspector endpoint was not available.".to_string();
    let mut all_targets = Vec::new();
    for port in CLAUDE_NODE_INSPECT_PORT..=CLAUDE_NODE_INSPECT_PORT_SCAN_END {
        match read_node_inspector_targets_from_port(port) {
            Ok(targets) if !targets.is_empty() => all_targets.extend(targets),
            Ok(_) => {
                last_error = format!("Claude Node inspector on port {port} had no targets.");
            }
            Err(err) => {
                last_error = err;
            }
        }
    }
    if all_targets.is_empty() {
        Err(last_error)
    } else {
        Ok(all_targets)
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
    app_name.eq_ignore_ascii_case("Claude") || exec_base.eq_ignore_ascii_case("Claude.exe")
}

/// Apply a read timeout to the underlying TCP stream of an inspector
/// WebSocket so a stalled CDP response cannot block the read loop forever.
#[cfg(windows)]
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

#[cfg(not(windows))]
fn set_inspector_read_timeout(
    _socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    _timeout: Duration,
) {
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

#[cfg(test)]
fn cdp_locale_patterns() -> Vec<Value> {
    [
        "*ion-dist/i18n/zh-CN.json*",
        "*ion-dist/i18n/en-US.json*",
        "*/i18n/zh-CN.json*",
        "*/i18n/en-US.json*",
        "*/zh-CN.json*",
    ]
    .into_iter()
    .map(|url_pattern| json!({ "urlPattern": url_pattern }))
    .collect()
}

#[cfg(test)]
fn cdp_locale_response(id: u64, event: &Value) -> Option<Value> {
    let request_id = cdp_request_id(event)?;
    let url = event.get("params")?.get("request")?.get("url")?.as_str()?;
    let payload = locale_payload_for_url_with_locale(url, "zh-CN")?;
    Some(json!({
        "id": id,
        "method": "Fetch.fulfillRequest",
        "params": {
            "requestId": request_id,
            "responseCode": 200,
            "responseHeaders": [
                { "name": "Content-Type", "value": "application/json; charset=utf-8" },
                { "name": "Cache-Control", "value": "no-store" },
                { "name": "Access-Control-Allow-Origin", "value": "*" }
            ],
            "body": BASE64_STANDARD.encode(payload.as_bytes())
        }
    }))
}

#[cfg(test)]
fn cdp_request_id(event: &Value) -> Option<&str> {
    event
        .get("params")?
        .get("requestId")?
        .as_str()
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
fn locale_payload_for_url(url: &str) -> Option<&'static str> {
    locale_payload_for_url_with_locale(url, "en-US")
}

#[cfg(test)]
fn locale_payload_for_url_with_locale(url: &str, current_locale: &str) -> Option<&'static str> {
    let normalized = url.replace('\\', "/");
    let bare = normalized
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_zh = bare.ends_with("/zh-cn.json");
    let is_en = bare.ends_with("/en-us.json");
    let local_like = bare.starts_with("app://") || bare.starts_with("file://");
    if !is_zh && !(current_locale == "zh-CN" && is_en && local_like) {
        return None;
    }
    if bare.contains("/dynamic/") {
        Some(CLAUDE_ION_DYNAMIC_ZH_LOCALE)
    } else if bare.contains("/ion-dist/i18n/") || bare.contains("/i18n/") {
        Some(CLAUDE_ION_ZH_LOCALE)
    } else {
        Some(CLAUDE_SHELL_ZH_LOCALE)
    }
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
  const zhOn = () => { try { var sl = localStorage.getItem("spa:locale"); if (sl) return sl === "zh-CN"; if (__CSL_LL) return !0; return /claude\.ai$/i.test(location.hostname) && String(navigator.language || "").toLowerCase().indexOf("zh") === 0; } catch (_) { return false; } };

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
      globalThis.fetch = (input, init) => {
        const url = typeof input === "string" ? input : (input && input.url) || "";
        const ap = url.indexOf("/api/account_profile") >= 0;
        if (zhOn() && init && init.body && /PUT|POST/i.test(String(init.method || "")) && String(init.body).indexOf("zh-CN") >= 0) {
          try { const obj = JSON.parse(String(init.body)); if (obj && obj.locale === "zh-CN") { obj.locale = "en-US"; return rf(input, Object.assign({}, init, { body: JSON.stringify(obj) })); } } catch (_) {}
        }
        if (url.indexOf("overrides.json") >= 0 && zhOn()) { return rf(input, init).then((resp) => { const ct = (resp.headers.get("content-type") || "").toLowerCase(); if (ct.indexOf("json") < 0) return new Response("{}", { status: 200, headers: { "content-type": "application/json" } }); return resp; }).catch(() => rf(input, init)); }
        if ((!ap && url.indexOf("/bootstrap") < 0 && url.indexOf("/app_start") < 0) || !zhOn()) return rf(input, init);
        return rf(input, init).then((resp) => { if (!resp || !resp.ok) return resp; return withZh(resp); }).catch(() => rf(input, init));
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
    "Currently unavailable": "\u5f53\u524d\u4e0d\u53ef\u7528",
    "For more complex tasks": "\u66f4\u590d\u6742\u4efb\u52a1",
    "For complex tasks": "\u590d\u6742\u4efb\u52a1",
    "Always uses deep reasoning": "\u59cb\u7ec8\u4f7f\u7528\u6df1\u5ea6\u63a8\u7406",
    "Adaptive": "\u81ea\u9002\u5e94",
    "Extended": "\u6269\u5c55",
    "skill-creator": "\u6280\u80fd\u521b\u5efa\u5668",
  };
  const FULL_ZH = {
    "Create new skills, modify and improve existing skills": "\u521b\u5efa\u65b0\u6280\u80fd\uff0c\u4fee\u6539\u5e76\u6539\u8fdb\u73b0\u6709\u6280\u80fd\uff0c\u5e76\u8861\u91cf\u6280\u80fd\u8868\u73b0\u3002\u5f53\u7528\u6237\u60f3\u8981\u4ece\u96f6\u5f00\u59cb\u521b\u5efa\u6280\u80fd\u3001\u7f16\u8f91\u6216\u4f18\u5316\u73b0\u6709\u6280\u80fd\u3001\u8fd0\u884c\u8bc4\u4f30\u6765\u6d4b\u8bd5\u6280\u80fd\u3001\u901a\u8fc7\u65b9\u5dee\u5206\u6790\u5bf9\u6280\u80fd\u8868\u73b0\u8fdb\u884c\u57fa\u51c6\u6d4b\u8bd5\uff0c\u6216\u4f18\u5316\u6280\u80fd\u63cf\u8ff0\u4ee5\u63d0\u5347\u89e6\u53d1\u51c6\u786e\u6027\u65f6\u4f7f\u7528\u3002",
  };
  const translateTextNode = (node) => {
    if (!node || node.nodeType !== 3 || !zhOn()) return;
    // Skip text inside thinking blocks and code/pre elements: these contain
    // Claude's reasoning/output and must not be prefix-translated, or the
    // thinking output may be corrupted and fail to render after completion.
    if (node.parentElement) {
      var el = node.parentElement;
      if (el.closest && el.closest('[data-thinking], [class*="thinking"], [class*="thought"], pre, code, [class*="code"], [contenteditable]')) return;
    }
    const v = node.nodeValue;
    if (!v) return;
    const trimmed = v.trim();
    if (!trimmed) return;
    const lead = v.length - v.trimStart().length;
    var zh = TEXT_ZH[trimmed];
    if (zh) { node.nodeValue = v.slice(0, lead) + zh + v.slice(lead + trimmed.length); return; }
    for (var fk in FULL_ZH) if (fk.length > 15 && trimmed.indexOf(fk) === 0) { node.nodeValue = v.slice(0, lead) + FULL_ZH[fk]; return; }
    for (var k in TEXT_ZH) if (k.length > 15 && trimmed.indexOf(k) === 0) { node.nodeValue = v.slice(0, lead) + TEXT_ZH[k] + v.slice(lead + k.length); return; }
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
      if (localStorage.getItem("spa:locale") !== "zh-CN") return;
      document.querySelectorAll('[role=menuitemradio][lang]').forEach((el) => {
        const want = el.getAttribute("lang") === "zh-CN" ? "true" : "false";
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
  const fixTitle = () => { try { if (!zhOn()) return; if (TITLE_ZH[document.title]) document.title = TITLE_ZH[document.title]; } catch (_) {} };
  fixTitle();
  setInterval(fixTitle, 1500);
})();
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_localized_command_is_unchanged() {
        assert_eq!(
            patched_launch_command("claude-desktop", "Claude", false).unwrap(),
            "Claude"
        );
    }

    #[test]
    fn localized_windows_command_uses_inspector_launcher_without_cdp_auth() {
        let command = patched_launch_command("claude-desktop", "Claude", true).unwrap();
        if cfg!(target_os = "windows") {
            assert!(command.contains("launch-claude-zh.ps1"));
        } else {
            assert_eq!(command, "open -a Claude");
        }
        assert!(!command.contains("--inspect"));
        assert!(!command.contains("remote-debugging-port"));
    }

    #[test]
    fn localized_activation_dispatches_by_install_kind_and_supports_exe() {
        // A winget .exe install has no MSIX identity, so the localized
        // activation must launch the patched launcher directly rather than
        // requiring an MSIX package. The dispatcher resolves the install
        // kind and branches accordingly; MSIX-only errors must not surface
        // for an .exe install.
        let source = include_str!("claude_desktop_patch.rs");
        assert!(source.contains("enum ClaudeInstallKind"));
        assert!(source.contains("ClaudeInstallKind::Exe"));
        assert!(source.contains("launch_windows_claude_exe(install.launcher_exe, &[])"));
        // Squirrel layout: patch the app-<version> image, launch the root launcher.
        assert!(source.contains("find_squirrel_app_version_dir"));
        assert!(source.contains("patch_exe"));
        assert!(source.contains("launcher_exe"));
        // The unified resolver prefers MSIX, then the broad version-aware
        // scan detection uses, then the explicit exe candidate list.
        assert!(source.contains("resolve_claude_install_for_patch"));
        assert!(source.contains("claude_desktop_windows_native_install_path"));
        assert!(source.contains("resolve_patch_paths_from_detected"));
    }

    #[test]
    fn resolve_patch_paths_from_detected_handles_app_version_layout() {
        // Squirrel layout: detected image is <root>/app-<version>/Claude.exe.
        // patch_exe is the image itself, resources next to it, launcher the
        // root claude.exe.
        let detected =
            PathBuf::from(r"C:\Users\A\AppData\Local\AnthropicClaude\app-1.14271.0\Claude.exe");
        let (patch_exe, launcher, resources) =
            resolve_patch_paths_from_detected(&detected).expect("should resolve paths");
        assert_eq!(patch_exe, detected);
        assert_eq!(
            launcher,
            PathBuf::from(r"C:\Users\A\AppData\Local\AnthropicClaude\claude.exe")
        );
        assert_eq!(
            resources,
            PathBuf::from(r"C:\Users\A\AppData\Local\AnthropicClaude\app-1.14271.0\resources")
        );
    }

    #[test]
    fn resolve_patch_paths_from_detected_handles_bare_launcher() {
        // Bare layout: detected is <root>/Claude.exe with no app-<version>
        // parent. patch_exe and launcher are both the image; resources next to it.
        let detected = PathBuf::from(r"C:\Users\A\AppData\Local\Claude\Claude.exe");
        let (patch_exe, launcher, resources) =
            resolve_patch_paths_from_detected(&detected).expect("should resolve paths");
        assert_eq!(patch_exe, detected);
        assert_eq!(launcher, detected);
        assert_eq!(
            resources,
            PathBuf::from(r"C:\Users\A\AppData\Local\Claude\resources")
        );
    }

    #[test]
    fn find_squirrel_app_version_dir_picks_highest_version() {
        let root = std::env::temp_dir().join(format!("cs-squirrel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("app-1.13576.0")).unwrap();
        fs::create_dir_all(root.join("app-1.14271.0")).unwrap();
        fs::create_dir_all(root.join("resources")).unwrap();
        fs::write(root.join("claude.exe"), b"launcher").unwrap();
        let picked = find_squirrel_app_version_dir(&root).expect("should find an app dir");
        assert_eq!(picked, "app-1.14271.0");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_squirrel_app_version_dir_returns_none_for_non_squirrel_layout() {
        let root = std::env::temp_dir().join(format!("cs-nosquirrel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("resources")).unwrap();
        fs::write(root.join("Claude.exe"), b"launcher").unwrap();
        assert_eq!(find_squirrel_app_version_dir(&root), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_launch_script_launches_cleanly_without_debug_args() {
        let script = windows_launch_script(true);
        // Both localized and non-localized scripts activate by MSIX app
        // identity and never pass debug arguments.
        assert!(script.contains("Get-AppxPackage"));
        assert!(script.contains("shell:AppsFolder"));
        assert!(script.contains("Start-Process -FilePath $target"));
        assert!(!script.contains("--inspect"));
        assert!(!script.contains("--remote-debugging-port"));
        assert!(!script.contains("Invoke-CommandInDesktopPackage"));
        assert!(!script.contains("Start-Process -FilePath $exe -WorkingDirectory"));
        assert!(script.contains("app identity activation is required"));
        assert!(script.contains("localized-launch.flag"));
        assert!(script.contains("Set-Content"));
        assert!(script.contains("zh-CN"));
        assert!(!windows_launch_script(false).contains("localized-launch.flag"));
    }

    #[test]
    fn localized_launch_uses_in_place_asar_patch_without_debug_args() {
        // Localized launch never puts debug flags on argv: the fuse
        // `EnableNodeCliInspectArguments` is disabled and the CDP auth gate
        // would exit. Instead the installed app.asar is patched in place so
        // its entry shim opens the Node inspector at runtime.
        assert!(claude_launch_args(true).is_empty());
        assert!(claude_launch_args(false).is_empty());
        let source = include_str!("claude_desktop_patch.rs");
        assert!(source.contains("apply_localization_patch"));
        assert!(source.contains("activate_localized_claude_msix"));
        assert!(source.contains("build_inspector_shim"));
        assert!(source.contains("CLAUDE_INSPECTOR_OPEN_PORT"));
    }

    #[test]
    fn inspector_shim_self_contains_localization_payload_and_reload() {
        // The asar entry shim localizes Claude on its own (no external
        // injector, no UI toggle). Four mechanisms cooperate:
        //   (1) renderer Fetch interception (async attach, http(s) only,
        //       /dynamic/ fulfillment) for the claude.ai webview locale,
        //   (2) native-menu label translation via Menu.setApplicationMenu +
        //       Tray.setContextMenu hooks (en->zh map + hard-coded overrides)
        //       so the tray menu and hard-coded Developer items localize,
        //   (3) zh-CN-only Fetch fulfillment (en-US passes through so English
        //       stays usable), and
        //   (4) a runtime whitelist patch + one-time system-locale default so
        //       zh-CN is a selectable language, not a forced override.
        let shim = build_inspector_shim(".vite/build/index.pre.js");
        assert!(shim.contains("require('node:inspector').open"));
        assert!(shim.contains("CLAUDE_INSPECTOR_OPEN_PORT") || shim.contains("9233"));
        // (1) Fetch interception.
        assert!(shim.contains("localePayloadForUrl"));
        assert!(shim.contains("Fetch.fulfillRequest"));
        assert!(shim.contains("Fetch.enable"));
        assert!(shim.contains("Page.addScriptToEvaluateOnNewDocument"));
        assert!(shim.contains("contents.reload()"));
        // /dynamic/ locale files are fulfilled with the bundled dynamic
        // zh-CN catalog (model/thinking descriptions), not passed through.
        assert!(shim.contains("/dynamic/"));
        assert!(shim.contains("DYNAMIC_LOCALE"));
        // Only http(s) webContents (claude.ai webview) are intercepted.
        assert!(shim.contains("http://"));
        assert!(shim.contains("https://"));
        assert!(shim.contains("async function attach"));
        // app:// renderers (local settings/setup pages) fetch their own locale
        // catalog from app://localhost/i18n/*.json and must be intercepted too.
        assert!(shim.contains(r#"lower.indexOf("app://") !== 0"#));
        assert!(shim.contains(r#"lower.indexOf("file://") !== 0"#));
        assert!(shim.contains("__CSL_LL"));
        // devtools:// URLs carry "https://" in their query string; the filter
        // must match by protocol prefix so DevTools is never hijacked.
        assert!(!shim.contains(r#"url.indexOf("http://") < 0 && url.indexOf("https://")"#));
        assert!(shim.contains("TITLE_ZH"));
        assert!(shim.contains("TEXT_ZH"));
        assert!(shim.contains("translateTextNode"));
        assert!(shim.contains("startTextPatch"));
        assert!(shim.contains("SETUP_TITLES"));
        assert!(shim.contains("fixDevToolsTitles"));
        // isDestroyed is a function, not a property: the old truthy-reference
        // check `contents.isDestroyed` made attach() bail before Fetch enable.
        assert!(shim.contains("function isDestroyed"));
        assert!(shim.contains("isDestroyed(contents)"));
        assert!(!shim.contains("|| contents.isDestroyed) return"));
        assert!(shim.contains("browser-window-created"));
        assert!(shim.contains("setInterval(attachAll"));
        // (2) native-menu translation.
        assert!(shim.contains("translateMenuItems"));
        assert!(shim.contains("Menu.setApplicationMenu"));
        assert!(shim.contains("Tray.prototype.setContextMenu"));
        assert!(shim.contains("HARDCODED_ZH"));
        assert!(shim.contains("Paste and Match Style"));
        assert!(shim.contains("zh-CN.json"));
        // Guards that forced zh-CN and disabled English were removed; the
        // shim only fulfills zh-CN requests and lets en-US pass through.
        assert!(!shim.contains("origRenameSync"));
        assert!(!shim.contains("forceZh"));
        // The shim detects the active locale via CJK character detection on
        // the menu labels (menuIsZh/updateLocaleFromMenu) and spa:locale polling,
        // then only translates to zh when zh-CN is active. No IPC forcing.
        assert!(shim.contains("zhActive"));
        assert!(shim.contains("currentLocale"));
        assert!(shim.contains("menuIsZh"));
        assert!(shim.contains("updateLocaleFromMenu"));
        // Local/preload windows call DesktopIntl.getInitialLocale before
        // document scripts and localStorage synchronization run, so localized
        // launch must make Electron's initial locale zh-CN up front.
        assert!(shim.contains("forceInitialLocale"));
        assert!(shim.contains("localizedLaunchDefaultZh"));
        assert!(shim.contains("localized-launch.flag"));
        assert!(shim.contains("consumeLocalizedLaunchMarker"));
        assert!(shim.contains("getPreferredSystemLanguages"));
        assert!(shim.contains("getSystemLocale"));
        assert!(shim.contains("appendSwitch(\"lang\""));
        assert!(shim.contains("app.getLocale"));
        assert!(shim.contains("ion-dist/i18n/en-US.json"));
        assert!(shim.contains("currentLocale === \"zh-CN\" && isEn && localLike"));
        // Existing local/setup windows must follow language changes too.
        assert!(shim.contains("syncOpenWindowsLocale"));
        assert!(shim.contains("webContents.getAllWebContents"));
        assert!(shim.contains("localStorage.setItem(\"spa:locale\""));
        assert!(shim.contains("__cslLocaleReloaded"));
        assert!(shim.contains("localeChangeListeners.push(syncOpenWindowsLocale"));
        // The zh-CN payloads are embedded so the shim is self-contained.
        assert!(shim.contains("SHELL_LOCALE"));
        assert!(shim.contains("ION_LOCALE"));
        assert!(shim.contains("require('./' + MAIN_MODULE)"));
        assert!(shim.contains(".vite/build/index.pre.js"));
    }

    #[test]
    fn node_inspector_scans_runtime_attach_port_range() {
        assert_eq!(CLAUDE_NODE_INSPECT_PORT, 9229);
        assert!(CLAUDE_NODE_INSPECT_PORT_SCAN_END >= 9300);
        // The patched shim opens the inspector on a dedicated port inside
        // the scan range (avoids 9229, which other Electron apps commonly
        // occupy).
        assert!(CLAUDE_INSPECTOR_OPEN_PORT >= CLAUDE_NODE_INSPECT_PORT);
        assert!(CLAUDE_INSPECTOR_OPEN_PORT <= CLAUDE_NODE_INSPECT_PORT_SCAN_END);
        assert_ne!(CLAUDE_INSPECTOR_OPEN_PORT, 9229);
    }

    #[test]
    fn node_inspector_injection_source_targets_electron_windows() {
        let source = build_main_process_injection_source_for_paths(
            Path::new(r"C:\CodeStudio\translation-runtime.js"),
            Path::new(r"C:\CodeStudio\zh-CN.json"),
            Path::new(r"C:\CodeStudio\ion-dist\i18n\zh-CN.json"),
            Path::new(r"C:\CodeStudio\ion-dist\i18n\dynamic\zh-CN.json"),
        );
        assert!(source.contains("BrowserWindow.getAllWindows"));
        assert!(source.contains("process.getBuiltinModule(\"module\").createRequire"));
        assert!(source.contains("contents.debugger.attach"));
        assert!(source.contains("Fetch.enable"));
        assert!(source.contains("Page.addScriptToEvaluateOnNewDocument"));
        // The runtime is delivered via addScriptToEvaluateOnNewDocument so it
        // survives the reload; executeJavaScript is intentionally NOT awaited
        // before reload (that would leave its promise pending on unload).
        assert!(!source.contains("await contents.executeJavaScript(runtime, true)"));
        assert!(source.contains("Page.reload"));
        assert!(source.contains("withTimeout"));
        assert!(source.contains("__CODESTUDIO_CLAUDE_ZH_MAIN__"));
        assert!(source.contains("translation-runtime.js"));
        assert!(source.contains("localePayloadForUrl"));
        assert!(source.contains("ion-dist/i18n/en-US.json"));
        assert!(source.contains("isEn && localLike"));
        assert!(source.contains("webContents.getAllWebContents"));
    }

    #[test]
    fn node_inspector_injection_waits_for_real_renderer_attach() {
        let source = build_main_process_injection_source_for_paths(
            Path::new(r"C:\CodeStudio\translation-runtime.js"),
            Path::new(r"C:\CodeStudio\zh-CN.json"),
            Path::new(r"C:\CodeStudio\ion-dist\i18n\zh-CN.json"),
            Path::new(r"C:\CodeStudio\ion-dist\i18n\dynamic\zh-CN.json"),
        );

        assert!(source.contains("await globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__.refresh()"));
        assert!(source.contains("const results = await Promise.all"));
        assert!(source.contains("attached.add(contents);"));
        assert!(source.contains("return { ok: true, reused: false, ...summary };"));
        let patch = include_str!("claude_desktop_patch.rs");
        assert!(patch.contains("\"Runtime.evaluate\""));
        assert!(patch.contains("\"awaitPromise\": true"));
    }

    #[test]
    fn node_inspector_injection_reload_is_timeout_guarded() {
        let source = build_main_process_injection_source_for_paths(
            Path::new(r"C:\CodeStudio\translation-runtime.js"),
            Path::new(r"C:\CodeStudio\zh-CN.json"),
            Path::new(r"C:\CodeStudio\ion-dist\i18n\zh-CN.json"),
            Path::new(r"C:\CodeStudio\ion-dist\i18n\dynamic\zh-CN.json"),
        );

        // The reload is wrapped in a timeout so a stalled Page.reload cannot
        // hang the async injection (which would block the inspector read loop).
        assert!(source.contains("Promise.race"));
        assert!(source.contains("Page.reload"));
        // A read timeout guards the CDP eval round-trip on the Rust side too.
        assert!(
            source.contains("CLAUDE_INSPECTOR_EVAL_TIMEOUT") || {
                let patch = include_str!("claude_desktop_patch.rs");
                patch.contains("CLAUDE_INSPECTOR_EVAL_TIMEOUT")
            }
        );
    }

    #[test]
    fn windows_claude_process_lookup_uses_visible_claude_main_processes() {
        let source = windows_find_claude_process_script(Some(1234));

        assert!(source.contains("Get-Process -Name 'claude'"));
        assert!(source.contains("StartTime"));
        assert!(source.contains("Where-Object { $_.Path"));
        assert!(source.contains("Select-Object -First 1"));
        assert!(!source.contains("Get-CimInstance Win32_Process -Filter \"name = 'Claude.exe'\""));
        assert!(!source.contains("CreationDate -Descending"));
    }

    #[test]
    fn windows_claude_process_lookup_returns_all_candidates_for_attach() {
        let source = windows_find_claude_process_script(Some(1234));

        assert!(source.contains("ForEach-Object"));
        assert!(source.contains("[string]$_.Id"));
        assert!(source.contains("$ordered += @($visible"));
        assert!(!source.contains("exit 0"));
    }

    #[test]
    fn inspector_target_lookup_keeps_scanning_after_unrelated_targets() {
        let source = include_str!("claude_desktop_patch.rs");

        assert!(source.contains("read_node_inspector_targets_from_port(port)"));
        assert!(source.contains("all_targets.extend(targets)"));
        assert!(source.contains("Ok(all_targets)"));
    }

    #[test]
    fn windows_inspector_patch_disables_asar_integrity_fuse_in_place() {
        // The fuse marker and integrity index resolve to a byte offset into
        // a Claude.exe image; flipping that byte disables asar integrity.
        let marker = b"dL7pKGdnNz796PbbjQWNKmHXBZaB9tsX";
        let mut image = Vec::from(marker);
        image.push(0x01); // sentinel
        image.push(0x09); // fuse count
        image.extend_from_slice(b"010011011"); // 9 fuse status bytes
        let offset = fuse_integrity_offset(&image).unwrap();
        assert_eq!(image[offset], b'1');
        assert!(!fuse_integrity_disabled(&image));
        image[offset] = b'0';
        assert!(fuse_integrity_disabled(&image));

        // The elevated patch script takes ownership, copies patched blobs
        // over the originals, and restores the ACL — without ever touching
        // argv or CDP debug ports.
        let script = elevated_patch_script(
            Path::new(r"C:\Program Files\WindowsApps\Claude\app\Claude.exe"),
            Path::new(r"C:\Program Files\WindowsApps\Claude\app\resources\app.asar"),
            Path::new(r"C:\CodeStudio\Claude.patched.exe"),
            Path::new(r"C:\CodeStudio\app.patched.asar"),
            Path::new(r"C:\Program Files\WindowsApps\Claude\app\resources\zh-CN.json"),
            Path::new(r"C:\CodeStudio\zh-CN.json"),
            Path::new(
                r"C:\Program Files\WindowsApps\Claude\app\resources\ion-dist\i18n\zh-CN.json",
            ),
            Path::new(r"C:\CodeStudio\ion-zh-CN.json"),
        );
        assert!(script.contains("takeown"));
        assert!(script.contains("icacls"));
        assert!(script.contains("Copy-Item -LiteralPath"));
        assert!(!script.contains("--inspect"));
        assert!(!script.contains("remote-debugging-port"));
        assert!(!script.contains("_debugProcess"));
        assert!(script.contains("zh-CN.json"));
        assert!(script.contains("ion-dist"));
        let source = include_str!("claude_desktop_patch.rs");
        // Elevation waits for the elevated process so the patch is written
        // before Claude is activated; assert the synchronous variant.
        assert!(source.contains("ShellExecuteExW"));
        assert!(source.contains("SEE_MASK_NOCLOSEPROCESS"));
        assert!(source.contains("OsStr::new(\"runas\")"));
        assert!(source.contains("GetExitCodeProcess"));
        assert!(source.contains("WaitForSingleObject"));
    }

    #[test]
    fn node_inspector_identity_rejects_non_claude_electron_apps() {
        let other = json!({
            "execPath": "D:\\OtherApp\\OtherApp.exe",
            "appName": "OtherApp",
            "appPath": "D:\\OtherApp\\resources\\app.asar",
            "userData": "C:\\Users\\dreamyloong\\AppData\\Roaming\\OtherApp"
        });
        let claude = json!({
            "execPath": "C:\\Program Files\\WindowsApps\\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\\app\\Claude.exe",
            "appName": "Claude",
            "appPath": "C:\\Program Files\\WindowsApps\\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\\app\\resources\\app.asar",
            "userData": "C:\\Users\\dreamyloong\\AppData\\Local\\Claude-3p"
        });

        assert!(!node_inspector_identity_is_claude(&other));
        assert!(node_inspector_identity_is_claude(&claude));
    }

    #[test]
    fn unrelated_tool_command_is_unchanged() {
        assert_eq!(
            patched_launch_command("codex", "codex", true).unwrap(),
            "codex"
        );
    }

    #[test]
    fn patch_assets_intercept_native_locale_requests_without_dom_translation() {
        assert!(TRANSLATION_RUNTIME.contains("installLocaleWhitelist"));
        assert!(TRANSLATION_RUNTIME.contains("installLocalePersistence"));
        assert!(TRANSLATION_RUNTIME.contains("account_profile"));
        assert!(TRANSLATION_RUNTIME.contains("withZh"));
        assert!(TRANSLATION_RUNTIME.contains("fixLanguageRadio"));
        assert!(TRANSLATION_RUNTIME.contains("overrides.json"));
        assert!(TRANSLATION_RUNTIME.contains("gated_messages"));
        assert!(TRANSLATION_RUNTIME.contains("fetch"));
        assert!(format!("{:?}", cdp_locale_patterns()).contains("ion-dist/i18n/zh-CN.json"));
        assert!(format!("{:?}", cdp_locale_patterns()).contains("ion-dist/i18n/en-US.json"));
        assert!(cdp_locale_response(
            9,
            &json!({
                "method": "Fetch.requestPaused",
                "params": {
                    "requestId": "intercept-1",
                    "request": { "url": "https://claude.ai/ion-dist/i18n/zh-CN.json" }
                }
            })
        )
        .is_some());
    }

    #[test]
    fn locale_runtime_translates_current_model_badges() {
        assert!(TRANSLATION_RUNTIME.contains("Currently unavailable"));
        assert!(TRANSLATION_RUNTIME.contains("For more complex tasks"));
        assert!(TRANSLATION_RUNTIME.contains("For complex tasks"));
        assert!(!TRANSLATION_RUNTIME.contains("Can think for more complex tasks"));
        assert!(TRANSLATION_RUNTIME.contains("\\u590d\\u6742\\u4efb\\u52a1"));
    }

    #[test]
    fn locale_runtime_source_stays_small() {
        let source = build_locale_runtime_source();
        assert!(source.len() < 10_000);
        assert!(!source.contains("__CLAUDE_ZH_ION_LOCALE__"));
        assert!(!source.contains(CLAUDE_ION_ZH_LOCALE));
    }

    #[test]
    fn locale_payload_selection_matches_shell_and_ion_urls() {
        assert_eq!(
            locale_payload_for_url("https://claude.ai/ion-dist/i18n/zh-CN.json"),
            Some(CLAUDE_ION_ZH_LOCALE)
        );
        assert_eq!(
            locale_payload_for_url("file:///C:/Claude/i18n/zh-CN.json?v=1"),
            Some(CLAUDE_ION_ZH_LOCALE)
        );
        assert_eq!(
            locale_payload_for_url("https://claude.ai/ion-dist/i18n/en-US.json"),
            None
        );
        assert_eq!(locale_payload_for_url("file:///C:/Claude/en-US.json"), None);
        assert_eq!(
            locale_payload_for_url_with_locale("app://localhost/i18n/en-US.json", "zh-CN"),
            Some(CLAUDE_ION_ZH_LOCALE)
        );
        assert_eq!(
            locale_payload_for_url_with_locale("file:///C:/Claude/en-US.json", "zh-CN"),
            Some(CLAUDE_SHELL_ZH_LOCALE)
        );
        assert_eq!(
            locale_payload_for_url_with_locale(
                "https://claude.ai/ion-dist/i18n/en-US.json",
                "zh-CN"
            ),
            None
        );
        assert_eq!(locale_payload_for_url("https://claude.ai/app.js"), None);
    }

    #[test]
    fn bundled_zh_locale_payloads_are_valid_and_substantial() {
        let shell = serde_json::from_str::<Value>(CLAUDE_SHELL_ZH_LOCALE).unwrap();
        let ion = serde_json::from_str::<Value>(CLAUDE_ION_ZH_LOCALE).unwrap();

        assert!(shell.as_object().map(|value| value.len()).unwrap_or(0) > 400);
        assert!(ion.as_object().map(|value| value.len()).unwrap_or(0) > 16_000);
        assert!(CLAUDE_SHELL_ZH_LOCALE.contains("复制"));
        assert!(CLAUDE_ION_ZH_LOCALE.contains("Claude"));
        assert_eq!(
            ion.get("+8nVZyI6SB").and_then(Value::as_str),
            Some("<b>{category}</b> 需要 {count, plural, one {{label}} other {# 个字段}}")
        );
        assert_eq!(
            ion.get("vN8KEpa87z").and_then(Value::as_str),
            Some("完成 {category} 中的 {count, plural, one {# 个必填字段} other {# 个必填字段}}")
        );
        assert_eq!(
            ion.get("2oJ53OuEpZ").and_then(Value::as_str),
            Some("连接器最多可发送 {max, plural, one {# 个请求标头} other {# 个请求标头}}。")
        );
        assert_eq!(
            ion.get("YYeIWoKm4P").and_then(Value::as_str),
            Some("{count, plural, one {# 个字段已更改} other {# 个字段已更改}}")
        );
        for key in ["+8nVZyI6SB", "vN8KEpa87z", "2oJ53OuEpZ", "YYeIWoKm4P"] {
            let value = ion.get(key).and_then(Value::as_str).unwrap_or("");
            assert!(!value.contains("fields"));
            assert!(!value.contains("field changed"));
            assert!(!value.contains("required field"));
            assert!(!value.contains("request header"));
            assert!(!value.contains("，复数"));
            assert!(!value.contains("另一个"));
        }
    }
    #[test]
    fn extract_inspector_shim_to_temp_when_requested() {
        if std::env::var("CSL_EXTRACT_SHIM").is_err() {
            return;
        }
        let shim = build_inspector_shim(".vite/build/index.pre.js");
        let dir = std::env::temp_dir().join("csldiag");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("rustshim.js");
        std::fs::write(&path, &shim).unwrap();
        println!("WROTE_SHIM:{}", path.display());
    }

    #[test]
    fn recover_original_main_passes_through_non_shim_main() {
        assert_eq!(
            recover_original_main("ignored", ".vite/build/index.pre.js".to_string(), &[]),
            ".vite/build/index.pre.js"
        );
    }

    #[test]
    fn recover_original_main_reads_original_main_field() {
        let pkg = r#"{"main":"_csl_inspector_shim.js","originalMain":"app.js"}"#;
        assert_eq!(
            recover_original_main(pkg, CLAUDE_INSPECTOR_SHIM_NAME.to_string(), &[]),
            "app.js"
        );
    }

    #[test]
    fn recover_original_main_probes_tree_when_original_main_clobbered() {
        use crate::core::asar_archive;
        let main_body = b"module.exports = {};".to_vec();
        let pkg = br#"{"name":"claude","main":".vite/build/index.pre.js"}"#.to_vec();
        let asar = asar_archive::build_test_asar_with_files(&[
            (".vite/build/index.pre.js", main_body.as_slice()),
            ("package.json", pkg.as_slice()),
        ]);
        // Deep re-patch: both main and originalMain are the shim name.
        let deep_pkg = format!(
            "{{\"main\":\"{shim}\",\"originalMain\":\"{shim}\"}}",
            shim = CLAUDE_INSPECTOR_SHIM_NAME
        );
        assert_eq!(
            recover_original_main(&deep_pkg, CLAUDE_INSPECTOR_SHIM_NAME.to_string(), &asar),
            ".vite/build/index.pre.js"
        );
    }

    #[test]
    fn repatching_already_patched_asar_does_not_self_reference_main() {
        use crate::core::asar_archive;
        let main_body = b"module.exports = {};".to_vec();
        let pkg = br#"{"name":"claude","main":".vite/build/index.pre.js"}"#.to_vec();
        let asar0 = asar_archive::build_test_asar_with_files(&[
            (".vite/build/index.pre.js", main_body.as_slice()),
            ("package.json", pkg.as_slice()),
        ]);
        // First patch: shim MAIN_MODULE is the real main, not a self-reference.
        let (pkg_text, orig_main) = asar_archive::read_package_json(&asar0).unwrap();
        assert_eq!(orig_main, ".vite/build/index.pre.js");
        let new_pkg = build_patched_package_json(&pkg_text, &orig_main).unwrap();
        let shim = build_inspector_shim(&orig_main);
        let patched = asar_archive::build_patched_asar(
            &asar0,
            new_pkg.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            shim.as_bytes(),
        )
        .unwrap();
        assert!(!asar_shim_self_references(&patched));

        // Simulate re-patching the already-patched asar: read_package_json now
        // returns the shim as main, but recover_original_main must restore the
        // true main from originalMain so the next shim does not require() itself.
        let (pkg_text2, read_main2) = asar_archive::read_package_json(&patched).unwrap();
        assert_eq!(read_main2, CLAUDE_INSPECTOR_SHIM_NAME);
        let recovered = recover_original_main(&pkg_text2, read_main2, &patched);
        assert_eq!(recovered, ".vite/build/index.pre.js");
        let shim2 = build_inspector_shim(&recovered);
        assert!(!shim2.contains("var MAIN_MODULE = \"_csl_inspector_shim.js\""));
    }

    #[test]
    fn asar_shim_self_references_detects_self_reference() {
        use crate::core::asar_archive;
        let main_body = b"module.exports = {};".to_vec();
        let pkg = br#"{"name":"claude","main":".vite/build/index.pre.js"}"#.to_vec();
        let asar0 = asar_archive::build_test_asar_with_files(&[
            (".vite/build/index.pre.js", main_body.as_slice()),
            ("package.json", pkg.as_slice()),
        ]);
        // A shim whose MAIN_MODULE is the shim filename (the re-patch bug).
        let bad_shim = build_inspector_shim(CLAUDE_INSPECTOR_SHIM_NAME);
        let (pkg_text, _) = asar_archive::read_package_json(&asar0).unwrap();
        let np = build_patched_package_json(&pkg_text, CLAUDE_INSPECTOR_SHIM_NAME).unwrap();
        let bad = asar_archive::build_patched_asar(
            &asar0,
            np.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            bad_shim.as_bytes(),
        )
        .unwrap();
        assert!(asar_shim_self_references(&bad));
    }
    #[test]
    fn asar_shim_needs_update_flags_old_forcing_shim() {
        use crate::core::asar_archive;
        let main_body = b"module.exports = {};".to_vec();
        let pkg = br#"{"name":"claude","main":".vite/build/index.pre.js"}"#.to_vec();
        let asar0 = asar_archive::build_test_asar_with_files(&[
            (".vite/build/index.pre.js", main_body.as_slice()),
            ("package.json", pkg.as_slice()),
        ]);
        // The shim built by build_inspector_shim carries installLocaleWhitelist
        // (the redesign), so a freshly-patched asar must NOT be flagged.
        let shim = build_inspector_shim(".vite/build/index.pre.js");
        assert!(shim.contains("installLocaleWhitelist"));
        assert!(!shim.contains("forceZh"));
        let (pkg_text, _) = asar_archive::read_package_json(&asar0).unwrap();
        let np = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let patched = asar_archive::build_patched_asar(
            &asar0,
            np.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            shim.as_bytes(),
        )
        .unwrap();
        assert!(!asar_shim_needs_update(&patched));

        // A legacy shim that forces zh-CN (installLocalePreference + forceZh,
        // no installLocaleWhitelist) must be flagged for re-injection. Real
        // shims never embed their own filename, so the legacy body must not
        // either (regression guard for the self-name gate that used to mask
        // this case).
        let legacy = b"(function(){var forceZh=function(){};installLocalePreference();var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np2 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let old = asar_archive::build_patched_asar(
            &asar0,
            np2.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            legacy.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&old));

        // The real deployed pre-redesign shim carries installLocaleWhitelist
        // (so the old name-only gate passed) but lacks the Array.prototype.map
        // patch that appends zh-CN to the language menu. It must still be
        // flagged so a localized launch re-injects the current shim.
        let partial = b"(function(){installLocaleWhitelist();var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np3 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let stale = asar_archive::build_patched_asar(
            &asar0,
            np3.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            partial.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&stale));

        // The currently deployed shim has installLocaleWhitelist and the
        // Array.prototype.map menu patch (so zh-CN appears in the picker) but
        // predates the account_profile/server-rejection fix: selecting zh-CN
        // PUTs {locale:"zh-CN"} which the server rejects, so the override never
        // lands. It must be flagged so a localized launch re-injects the
        // persistence shim.
        let deployed = b"(function(){installLocaleWhitelist();Array.prototype.map;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np4 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let current = asar_archive::build_patched_asar(
            &asar0,
            np4.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            deployed.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&current));

        // The shim deployed after the PUT-rewrite fix has installLocaleWhitelist,
        // Array.prototype.map, and account_profile (so the PUT body is rewritten
        // and the request succeeds), but it only rewrites bootstrap responses
        // and not the account_profile GET response that drives the language
        // menu radio. Selecting zh-CN succeeds server-side yet the radio never
        // checks. It must be flagged so a localized launch re-injects the
        // withZh shim that also rewrites the GET response.
        let putfix = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np5 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let getfix = asar_archive::build_patched_asar(
            &asar0,
            np5.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            putfix.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&getfix));

        // The shim deployed after the GET-response rewrite has installLocaleWhitelist,
        // Array.prototype.map, account_profile, and withZh (so the GET returns zh-CN
        // and the intl context locale should track it), but it lacks the DOM radio
        // fix: messagesLocale can stay en-US under a gate flag, so IntlProvider locale
        // stays en-US and the zh-CN radio never checks. It must be flagged so a
        // localized launch re-injects the fixLanguageRadio shim.
        let radiofix = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np6 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let noradio = asar_archive::build_patched_asar(
            &asar0,
            np6.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            radiofix.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&noradio));
        // Shim deployed after the fixLanguageRadio DOM patch: it carries all
        // five redesign signatures but predates the overrides.json HTML-fallback
        // fix. /i18n/zh-CN.overrides.json returns the SPA HTML shell, a.json()
        // throws, the i18n query errors, messagesLocale never syncs to zh-CN, so
        // the IntlProvider locale stays the previous language and the zh-CN
        // radio never re-checks after switching away and back. Must be flagged
        // so a localized launch re-injects the overrides-fix shim.
        let preoverrides = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;fixLanguageRadio;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np7 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let noov = asar_archive::build_patched_asar(
            &asar0,
            np7.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            preoverrides.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&noov));

        // Shim deployed after the overrides.json HTML-fallback fix: it carries
        // all six redesign signatures but predates the gated_messages gate fix.
        // The account_profile response includes gated_messages{locale:"en-US"},
        // so the oHt gate s stays false (n===a, xi=false) for zh-CN, m=false,
        // setGatedMessages runs with r=undefined and messagesLocale never
        // re-syncs to zh-CN after switching away and back. The radio then tracks
        // the stale messagesLocale and never checks zh-CN. Must be flagged so a
        // localized launch re-injects the gated_messages-deletion shim.
        let pregm = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;fixLanguageRadio;overrides.json;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np8 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let nogm = asar_archive::build_patched_asar(
            &asar0,
            np8.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            pregm.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&nogm));

        // Shim deployed after the gated_messages fix: it carries all seven
        // redesign signatures but predates the locale-aware menu translation.
        // Its translateMenuItems unconditionally forces en->zh, so switching
        // to any other language leaves the native menu stuck in Chinese (and
        // hard-coded English labels like "Paste and Match Style" are forced to
        // zh even in French/English mode). It must be flagged so a localized
        // launch re-injects the zhActive shim that only translates for zh-CN.
        let prezhactive = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;fixLanguageRadio;overrides.json;gated_messages;zhActive;menuIsZh;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np9 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let nozhactive = asar_archive::build_patched_asar(
            &asar0,
            np9.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            prezhactive.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&nozhactive));

        // The locale-aware menu shim still predates the small DOM fallback for
        // model cards ("currently unavailable", "For more complex tasks"). It
        // must be flagged so users get the new runtime on localized launch.
        let premodeledges = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;fixLanguageRadio;overrides.json;gated_messages;zhActive;menuIsZh;updateLocaleFromMenu;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np10 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let nomodeledges = asar_archive::build_patched_asar(
            &asar0,
            np10.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            premodeledges.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&nomodeledges));

        // The immediately previous shim covered the unavailable badge but used
        // the internal defaultMessage "Can think..." instead of the visible
        // Opus card text "For more complex tasks". It must be replaced too.
        let previsibletask = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;fixLanguageRadio;overrides.json;gated_messages;zhActive;menuIsZh;updateLocaleFromMenu;currently unavailable;Can think for more complex tasks;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np11 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let wrongtask = asar_archive::build_patched_asar(
            &asar0,
            np11.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            previsibletask.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&wrongtask));

        // The next shim covered "For more complex tasks", but Claude's current
        // model menu renders the shorter visible string "For complex tasks".
        let preshorttask = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;fixLanguageRadio;overrides.json;gated_messages;zhActive;menuIsZh;updateLocaleFromMenu;currently unavailable;For more complex tasks;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np12 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let noshorttask = asar_archive::build_patched_asar(
            &asar0,
            np12.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            preshorttask.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&noshorttask));

        // The next shim translated model badges, but did not synchronize
        // already-open setup/local windows when the user changed languages.
        let prelocalsync = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;fixLanguageRadio;overrides.json;gated_messages;zhActive;menuIsZh;updateLocaleFromMenu;currently unavailable;For more complex tasks;For complex tasks;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np13 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let nolocalsync = asar_archive::build_patched_asar(
            &asar0,
            np13.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            prelocalsync.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&nolocalsync));

        // The window-locale sync shim still let local/setup windows bootstrap in
        // English when opened while zh-CN was already active. It lacks the
        // localized-launch marker, initial Electron locale override, and local
        // en-US catalog fallback.
        let preinitiallocale = b"(function(){installLocaleWhitelist();Array.prototype.map;account_profile;withZh;fixLanguageRadio;overrides.json;gated_messages;zhActive;menuIsZh;updateLocaleFromMenu;currently unavailable;For more complex tasks;For complex tasks;syncOpenWindowsLocale;var MAIN_MODULE=\".vite/build/index.pre.js\";})();";
        let np14 = build_patched_package_json(&pkg_text, ".vite/build/index.pre.js").unwrap();
        let noinitiallocale = asar_archive::build_patched_asar(
            &asar0,
            np14.as_bytes(),
            CLAUDE_INSPECTOR_SHIM_NAME,
            preinitiallocale.as_slice(),
        )
        .unwrap();
        assert!(asar_shim_needs_update(&noinitiallocale));
    }
}
