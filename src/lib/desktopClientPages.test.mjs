import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("app exposes a dedicated Claude Desktop route below Codex Client", () => {
  const app = read("src/App.svelte");

  assert.match(app, /import ClaudeDesktop from "\.\/routes\/ClaudeDesktop\.svelte"/);
  assert.match(app, /type Route = [^;]*"claudeDesktop"/);
  assert.match(
    app,
    /\{ id: "codexClient", labelKey: "app\.nav\.codexClient"[\s\S]*\{ id: "claudeDesktop", labelKey: "app\.nav\.claudeDesktop"/
  );
  assert.match(app, /route === "claudeDesktop"/);
  assert.match(app, /<ClaudeDesktop/);
});

test("desktop client pages are shown only on Windows and macOS", () => {
  const app = read("src/App.svelte");

  assert.match(app, /desktopClientPagesAvailable = \["windows", "macos"\]\.includes\(snapshot\?\.platform \?\? ""\)/);
  assert.match(app, /!\["codexClient", "claudeDesktop"\]\.includes\(item\.id\) \|\| desktopClientPagesAvailable/);
  assert.match(app, /\["codexClient", "claudeDesktop"\]\.includes\(route\) && !desktopClientPagesAvailable/);
  assert.doesNotMatch(app, /codexClientAvailable = snapshot\?\.platform !== "linux"/);
});

test("dashboard desktop client actions run in place instead of navigating to client pages", () => {
  const dashboard = read("src/routes/Dashboard.svelte");

  assert.match(dashboard, /installOrUpdateCodexClient/);
  assert.match(dashboard, /installOrUpdateClaudeDesktop/);
  assert.match(dashboard, /tool\.id === "codex-app"[\s\S]*triggerDesktopClientAction\(tool, mode\)/);
  assert.match(dashboard, /tool\.id === "claude-desktop"[\s\S]*triggerDesktopClientAction\(tool, mode\)/);
  assert.doesNotMatch(dashboard, /if \(tool\.id === "codex-app"\) \{\s*onOpenCodexClient\(\)/);
});

test("Claude Desktop page supports install update and uninstall through the shared tool installer", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");
  const store = read("src/lib/claudeDesktopStore.ts");
  const api = read("src/lib/api.ts");
  const commands = read("src-tauri/src/commands/tool_installer.rs");
  const lib = read("src-tauri/src/lib.rs");

  assert.match(route, /claudeDesktopView/);
  assert.match(route, /installOrUpdateClaudeDesktop/);
  assert.match(route, /removeClaudeDesktop/);
  assert.match(store, /const CLAUDE_DESKTOP_TOOL_ID = "claude-desktop"/);
  assert.match(store, /planToolInstall\(CLAUDE_DESKTOP_TOOL_ID\)/);
  assert.match(store, /planToolUpdate\(CLAUDE_DESKTOP_TOOL_ID\)/);
  assert.match(store, /installTool\(/);
  assert.match(store, /updateTool\(/);
  assert.match(store, /uninstallTool\(/);
  assert.match(api, /export async function uninstallTool/);
  assert.match(commands, /pub async fn uninstall_tool/);
  assert.match(lib, /commands::tool_installer::uninstall_tool/);
});

test("Claude Desktop page launches like a desktop client without the shared tool modal", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");
  const api = read("src/lib/api.ts");
  const commandsMod = read("src-tauri/src/commands/mod.rs");
  const commands = read("src-tauri/src/commands/claude_desktop.rs");
  const lib = read("src-tauri/src/lib.rs");
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");

  assert.match(route, /launchClaudeDesktop\(\{ localize: localizeClaudeLaunch \}\)/);
  assert.match(route, /claudeDesktop\.launchOptionsTitle/);
  assert.match(route, /claudeDesktop\.localizeLaunch/);
  assert.match(api, /export async function launchClaudeDesktop/);
  assert.match(commandsMod, /pub mod claude_desktop;/);
  assert.match(commands, /pub fn launch_claude_desktop/);
  assert.match(lib, /commands::claude_desktop::launch_claude_desktop/);
  assert.match(patch, /pub fn launch\(localize: bool\)/);
  assert.doesNotMatch(route, /openClaudeLaunch/);
  assert.doesNotMatch(route, /launchOpen/);
  assert.doesNotMatch(route, /selectedLaunchProfileId/);
  assert.doesNotMatch(route, /selectedLaunchShellId/);
  assert.doesNotMatch(route, /planToolLaunch/);
  assert.doesNotMatch(route, /startInstallTerminal/);
  assert.doesNotMatch(route, /listenInstallTerminalOutput/);
  assert.doesNotMatch(route, /modal-panel wide-modal/);
});

test("Claude Desktop Windows launch uses native app activation instead of fire-and-forget PowerShell scripts", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");

  assert.match(patch, /package::launch_first_msix_package_with_args/);
  assert.match(patch, /launch_windows_claude_desktop\(localize\)/);
  assert.match(patch, /find_windows_claude_exe\(\)/);
  const launchFunction = patch.slice(patch.indexOf("pub fn launch(localize: bool)"), patch.indexOf("pub fn base_launch_command"));
  assert.doesNotMatch(launchFunction, /hidden_command\("powershell\.exe"\)[\s\S]*\.spawn\(\)/);
}
);

test("Claude Desktop detection caches Windows MSIX lookups so page plans do not rescan slowly", () => {
  const detector = read("src-tauri/src/core/detector.rs");

  assert.match(detector, /const CLAUDE_DESKTOP_INSTALL_CACHE_TTL: Duration = Duration::from_secs\(30\)/);
  assert.match(detector, /static CLAUDE_DESKTOP_INSTALL_CACHE: OnceLock<Mutex<ClaudeDesktopInstallCache>>/);
  assert.match(detector, /fn cached_claude_desktop_windows_msix_package\(\)/);
  assert.match(detector, /cached_claude_desktop_windows_msix_package\(\)\.map/);
  assert.match(detector, /cache\.detected = None;\s*cache\.checked_at = None;/s);
}
);

test("Claude Desktop launch does not expose console selection", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");

  assert.doesNotMatch(route, /toolLaunch\.console/);
  assert.doesNotMatch(route, /launch-option-grid compact/);
  assert.doesNotMatch(route, /on:click=\{\(\) => \(selectedLaunchShellId = shell\.id\)\}/);
  assert.doesNotMatch(route, /shellId:/);
});

test("Claude Desktop launch can enable localization patching", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");
  const api = read("src/lib/api.ts");
  const command = read("src-tauri/src/commands/claude_desktop.rs");
  const coreMod = read("src-tauri/src/core/mod.rs");
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");

  assert.match(route, /localizeClaudeLaunch/);
  assert.match(route, /claudeDesktop\.localizeLaunch/);
  assert.match(route, /localize: localizeClaudeLaunch/);
  assert.match(api, /invoke\("launch_claude_desktop", \{ localize: request\.localize \}\)/);
  assert.match(command, /claude_desktop_patch::launch\(localize\)/);
  assert.match(coreMod, /pub mod claude_desktop_patch;/);
  assert.match(patch, /TRANSLATION_RUNTIME/);
  assert.match(patch, /Page\.addScriptToEvaluateOnNewDocument/);
});

test("Claude Desktop localization uses native payloads with a small runtime fallback", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");

  assert.match(patch, /write_claude_locale_payloads/);
  assert.match(patch, /include_str!/);
  assert.match(patch, /CLAUDE_SHELL_ZH_LOCALE/);
  assert.match(patch, /CLAUDE_ION_ZH_LOCALE/);
  assert.match(patch, /zh-CN\.json/);
  assert.match(patch, /ion-dist\/i18n\/zh-CN\.json/);
  assert.match(patch, /fetch/);
  assert.match(patch, /TEXT_ZH/);
  assert.match(patch, /MutationObserver/);
  assert.doesNotMatch(patch, /CLAUDE_SHELL_EN_LOCALE_FILE/);
  assert.doesNotMatch(patch, /CLAUDE_ION_EN_LOCALE_RELATIVE_PATH/);
  assert.doesNotMatch(patch, /find_claude_resources_dir/);
  assert.doesNotMatch(patch, /write_merged_locale_payload/);
  assert.doesNotMatch(patch, /TRANSLATION_DICTIONARY/);
  assert.doesNotMatch(patch, /__CLAUDE_ZH_DICTIONARY__/);
  assert.doesNotMatch(patch, /translation-dictionary/);
  assert.doesNotMatch(patch, /data-message-author-role/);
});

test("Claude Desktop localized Windows launch uses runtime Node inspector attach instead of auth-gated CDP", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");
  const productionPatch = patch.slice(0, patch.indexOf("#[cfg(test)]"));

  // The cross-process `process._debugProcess` / SIGUSR1 signal path is gone:
  // the packaged Claude main process does not register the libuv signal
  // mapping on Windows, and the Electron fuse disables `--inspect`. Instead
  // the installed app.asar is patched in place so its entry shim opens the
  // Node inspector itself (same path as the in-app Developer menu), then the
  // app is activated by MSIX identity.
  assert.doesNotMatch(productionPatch, /process\._debugProcess/);
  assert.doesNotMatch(productionPatch, /kill"\)\s*\.args\(\["-USR1", &pid\]\)/);
  assert.doesNotMatch(productionPatch, /trigger_node_inspector/);
  assert.doesNotMatch(productionPatch, /--inspect/);
  assert.doesNotMatch(productionPatch, /--remote-debugging-port/);
  assert.match(patch, /apply_localization_patch/);
  assert.match(patch, /activate_localized_claude_msix/);
  assert.match(patch, /build_inspector_shim/);
  assert.match(patch, /CLAUDE_INSPECTOR_OPEN_PORT/);
  assert.match(patch, /fuse_integrity_offset/);
  assert.match(patch, /read_node_inspector_targets/);
  assert.match(patch, /node_inspector_identity_is_claude/);
  assert.match(patch, /build_main_process_injection_source/);
  const launchFunction = patch.slice(
    patch.indexOf("fn launch_windows_claude_desktop"),
    patch.indexOf("fn claude_launch_args")
  );
  assert.match(launchFunction, /if localize \{/);
  assert.match(launchFunction, /close_existing_claude_for_localized_launch\(\)\?/);
  assert.match(launchFunction, /apply_localization_patch\(\)\?/);
  assert.match(launchFunction, /activate_localized_claude\(\)\?/);
  // Non-localized launch still falls back to MSIX/exe activation.
  assert.match(launchFunction, /launch_windows_claude_msix\(&args\)/);
  assert.match(launchFunction, /find_windows_claude_exe\(\)/);
  assert.match(launchFunction, /launch_windows_claude_exe\(exe, &args\)/);
});

test("Claude Desktop Windows launch scripts stay clean and avoid debug argv", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");

  const scriptFunction = patch.slice(
    patch.indexOf("fn windows_launch_script"),
    patch.indexOf("fn inject_localization")
  );
  // Both localized and non-localized scripts activate by MSIX app identity
  // (shell:AppsFolder) and never pass debug arguments. The localized launch
  // no longer Start-Process'es the raw InstallLocation exe directly, because
  // that would lose MSIX user-data redirection; the in-place asar patch
  // makes the main process open the inspector itself.
  assert.match(scriptFunction, /shell:AppsFolder\\/);
  assert.match(scriptFunction, /PackageFamilyName/);
  assert.match(scriptFunction, /Add-AppxPackage -Register/);
  assert.match(scriptFunction, /WindowsApps/);
  assert.match(scriptFunction, /app identity activation is required/);
  assert.doesNotMatch(scriptFunction, /Join-Path \$pkg\.InstallLocation 'app\\Claude\.exe'/);
  assert.doesNotMatch(scriptFunction, /--inspect/);
  assert.doesNotMatch(scriptFunction, /--remote-debugging-port/);
});

test("Claude Desktop terminal localization command uses the localized Windows launcher", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");

  const patchedCommandFunction = patch.slice(
    patch.indexOf("pub fn patched_launch_command"),
    patch.indexOf("pub fn spawn_localization_injector")
  );
  const windowsBranch = patchedCommandFunction.slice(
    patchedCommandFunction.indexOf("if cfg!(target_os = \"windows\")"),
    patchedCommandFunction.indexOf("} else {", patchedCommandFunction.indexOf("if cfg!(target_os = \"windows\")"))
  );
  assert.match(windowsBranch, /launch-claude-zh\.ps1/);
  assert.doesNotMatch(windowsBranch, /launch-claude\.ps1"\)\)/);
});

test("Claude Desktop Windows launch repairs stale MSIX registration before activation", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");
  const detector = read("src-tauri/src/core/detector.rs");
  const packageCore = read("src-tauri/src/core/platform/package.rs");

  assert.match(patch, /repair_claude_msix_registration\(\)\?/);
  assert.match(patch, /register_msix_manifest/);
  assert.match(packageCore, /pub fn register_msix_manifest/);
  assert.match(packageCore, /Add-AppxPackage -Register/);
  assert.match(detector, /detect_claude_desktop_windows_registered_msix/);
  assert.match(detector, /detect_claude_desktop_windows_stale_msix/);
  assert.match(detector, /source: "appx-stale"/);
});

test("Claude Desktop stale MSIX repair can use cached detection when WindowsApps cannot be enumerated", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");
  const detector = read("src-tauri/src/core/detector.rs");

  assert.match(patch, /claude_desktop_windows_cached_stale_msix_manifest/);
  assert.match(patch, /claude_desktop_windows_known_stale_msix_manifest/);
  assert.match(detector, /detect_claude_desktop_windows_cached_stale_msix/);
  assert.match(detector, /detect_claude_desktop_windows_known_stale_msix/);
  assert.match(detector, /storage::load_detection_cache/);
  assert.match(detector, /source_fragment: &str/);
  assert.match(detector, /"appx-stale"/);
  assert.match(detector, /CLAUDE_DESKTOP_WINDOWS_PACKAGE_SUFFIX/);
  assert.match(detector, /AppxManifest\.xml/);
});
