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
  assert.match(app, /function desktopClientRouteAllowed\(currentRoute: Route\)/);
  assert.match(app, /desktopClientPagesAvailable \|\| \(currentRoute === "claudeDesktop" && pendingClaudeDesktopRouteRestore\)/);
  assert.match(app, /\["codexClient", "claudeDesktop"\]\.includes\(route\) && !desktopClientRouteAllowed\(route\)/);
  assert.doesNotMatch(app, /codexClientAvailable = snapshot\?\.platform !== "linux"/);
});

test("dashboard desktop client install/update navigates to the client page to show progress", () => {
  const dashboard = read("src/routes/Dashboard.svelte");

  assert.match(dashboard, /installOrUpdateCodexClient/);
  assert.match(dashboard, /installOrUpdateClaudeDesktop/);
  assert.match(dashboard, /tool\.id === "codex-app"[\s\S]*triggerDesktopClientAction\(tool, mode\)/);
  assert.match(dashboard, /tool\.id === "claude-desktop"[\s\S]*triggerDesktopClientAction\(tool, mode\)/);
  assert.doesNotMatch(dashboard, /if \(tool\.id === "codex-app"\) \{\s*onOpenCodexClient\(\)/);
  // install/update must hand off to the dedicated client page so the user can
  // watch the download/progress stream that the page renders from the store.
  assert.match(dashboard, /triggerDesktopClientAction\(tool: ToolStatus, mode: "install" \| "update"\) \{[\s\S]*onNavigateToClient\(tool\.id\)/);
});

test("route switches refresh the active CodeStudio Lite page", () => {
  const app = read("src/App.svelte");

  assert.match(app, /ensureClaudeDesktopLoaded/);
  assert.match(app, /import \{ ensureCodexClientLoaded \} from "\.\/lib\/codexClientStore"/);
  assert.match(app, /let lastRouteRefreshRoute: Route = route/);
  assert.match(app, /route !== lastRouteRefreshRoute[\s\S]*refreshCurrentRouteAfterSwitch\(route\)/);
  assert.match(
    app,
    /currentRoute === "dashboard"[\s\S]*refreshDashboard\(\{ quiet: true, scheduleFollowup: false, showRefreshIndicator: true \}\)/
  );
  assert.match(app, /currentRoute === "codexClient"[\s\S]*ensureCodexClientLoaded\(\)/);
  assert.match(app, /currentRoute === "claudeDesktop"[\s\S]*ensureClaudeDesktopLoaded\(\)/);
  assert.doesNotMatch(app, /currentRoute === "codexClient"[\s\S]*refreshCodexClient\(\)/);
  assert.doesNotMatch(app, /currentRoute === "claudeDesktop"[\s\S]*refreshClaudeDesktop\(\)/);
  assert.match(app, /currentRoute === "profiles" \|\| currentRoute === "gateway"[\s\S]*refreshAfterProfileChange\(\)/);
  assert.match(app, /currentRoute === "settings"[\s\S]*refreshSettings\(\)/);
});

test("desktop client page entry hydrates cache before background refresh", () => {
  const codexStore = read("src/lib/codexClientStore.ts");
  const claudeStore = read("src/lib/claudeDesktopStore.ts");

  const codexEnsure = codexStore.slice(
    codexStore.indexOf("export async function ensureCodexClientLoaded"),
    codexStore.indexOf("export async function refreshCodexClient")
  );
  const claudeEnsure = claudeStore.slice(
    claudeStore.indexOf("export async function ensureClaudeDesktopLoaded"),
    claudeStore.indexOf("/// Hydrate the Claude Desktop view")
  );

  assert.doesNotMatch(codexEnsure, /snapshot\.loaded\s*\|\|\s*snapshot\.loading/);
  assert.match(codexEnsure, /hydrateCodexClientFromCache\(\)/);
  assert.match(codexEnsure, /loadPromise\s*=\s*refreshCodexClient/);
  assert.ok(
    codexEnsure.indexOf("hydrateCodexClientFromCache()") <
      codexEnsure.indexOf("loadPromise = refreshCodexClient"),
    "Codex Client route entry should hydrate cache before starting the background refresh"
  );

  assert.doesNotMatch(claudeEnsure, /snapshot\.loaded\s*\|\|\s*snapshot\.loading/);
  assert.match(claudeEnsure, /hydrateClaudeDesktopFromCache\(\)/);
  assert.match(claudeEnsure, /loadPromise\s*=\s*refreshClaudeDesktop/);
  assert.ok(
    claudeEnsure.indexOf("hydrateClaudeDesktopFromCache()") <
      claudeEnsure.indexOf("loadPromise = refreshClaudeDesktop"),
    "Claude Desktop route entry should hydrate cache before starting the background refresh"
  );
});

test("Claude Desktop page supports install update and uninstall through the shared tool installer", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");
  const store = read("src/lib/claudeDesktopStore.ts");
  const api = read("src/lib/api.ts");
  const commands = read("src-tauri/src/commands/tool_installer.rs");
  const claudeCommands = read("src-tauri/src/commands/claude_desktop.rs");
  const coreInstaller = read("src-tauri/src/core/tool_installer.rs");
  const lib = read("src-tauri/src/lib.rs");

  assert.match(route, /claudeDesktopView/);
  assert.match(route, /installOrUpdateClaudeDesktop/);
  assert.match(route, /removeClaudeDesktop/);
  assert.match(route, /openClaudeDesktopStagingPath/);
  assert.match(route, /claudeDesktop\.openStagingPath/);
  assert.match(store, /const CLAUDE_DESKTOP_TOOL_ID = "claude-desktop"/);
  assert.match(store, /planToolInstall\(CLAUDE_DESKTOP_TOOL_ID\)/);
  assert.match(store, /planToolUpdate\(CLAUDE_DESKTOP_TOOL_ID\)/);
  assert.match(store, /installTool\(/);
  assert.match(store, /updateTool\(/);
  assert.match(store, /uninstallTool\(/);
  assert.match(store, /refreshClaudeDesktop\(true,\s*kind\)/);
  assert.match(store, /openClaudeDesktopPath\("staging"\)/);
  assert.match(api, /export async function uninstallTool/);
  assert.match(api, /export async function openClaudeDesktopPath/);
  assert.match(api, /invoke\("open_claude_desktop_path", \{ kind \}\)/);
  assert.match(commands, /pub async fn uninstall_tool/);
  assert.match(coreInstaller, /pub fn open_claude_desktop_path/);
  assert.match(coreInstaller, /cleanup_claude_desktop_download_cache/);
  assert.match(coreInstaller, /Removed Claude Desktop downloaded installer cache/);
  assert.match(claudeCommands, /pub fn open_claude_desktop_path/);
  assert.match(claudeCommands, /tool_installer::open_claude_desktop_path/);
  assert.match(lib, /commands::tool_installer::uninstall_tool/);
  assert.match(lib, /commands::claude_desktop::open_claude_desktop_path/);
});

test("Claude Desktop page launches like a desktop client without the shared tool modal", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");
  const api = read("src/lib/api.ts");
  const commandsMod = read("src-tauri/src/commands/mod.rs");
  const commands = read("src-tauri/src/commands/claude_desktop.rs");
  const lib = read("src-tauri/src/lib.rs");
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");

  assert.match(route, /launchClaudeWithLocalization\(localizeClaudeLaunch\)/);
  assert.match(route, /launchClaudeDesktop\(\{ localize \}\)/);
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

test("Claude Desktop launch can enable runtime localization", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");
  const api = read("src/lib/api.ts");
  const command = read("src-tauri/src/commands/claude_desktop.rs");
  const coreMod = read("src-tauri/src/core/mod.rs");
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");

  assert.match(route, /localizeClaudeLaunch/);
  assert.match(route, /claudeDesktop\.localizeLaunch/);
  assert.match(route, /launchClaudeWithLocalization\(localizeClaudeLaunch\)/);
  assert.match(route, /localize: accessibilityLaunchLocalize/);
  assert.match(api, /invoke\("launch_claude_desktop", \{ localize: request\.localize \}\)/);
  assert.match(command, /app: tauri::AppHandle/);
  assert.match(command, /claude_desktop_patch::launch_with_app\(localize, Some\(app\)\)/);
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

test("Claude Desktop localized Windows launch uses official debugger runtime injection", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");
  const productionPatch = patch.slice(0, patch.indexOf("mod tests {"));

  // The old destructive patch paths are gone. Windows now matches macOS:
  // launch the official Claude app, open Claude's official main-process
  // debugger route, then inject localization through the Node inspector.
  assert.doesNotMatch(productionPatch, /process\._debugProcess/);
  assert.doesNotMatch(productionPatch, /kill"\)\s*\.args\(\["-USR1", &pid\]\)/);
  assert.doesNotMatch(productionPatch, /trigger_node_inspector/);
  assert.doesNotMatch(productionPatch, /--inspect/);
  assert.doesNotMatch(productionPatch, /--remote-debugging-port/);
  assert.doesNotMatch(productionPatch, new RegExp(["apply", "_localization_patch"].join("")));
  assert.doesNotMatch(productionPatch, new RegExp(["activate", "_localized_claude_msix"].join("")));
  assert.doesNotMatch(productionPatch, new RegExp(["build", "_inspector_shim"].join("")));
  assert.doesNotMatch(productionPatch, new RegExp(["fuse", "_integrity_offset"].join("")));
  assert.doesNotMatch(productionPatch, new RegExp(["Shell", "ExecuteExW"].join("")));
  assert.doesNotMatch(productionPatch, new RegExp(["apply-claude-", "patch\\.ps1"].join("")));
  assert.match(productionPatch, /ensure_windows_claude_main_process_debugger/);
  assert.match(productionPatch, /enable_claude_main_process_debugger/);
  assert.match(productionPatch, /read_node_inspector_targets/);
  assert.match(productionPatch, /node_inspector_identity_is_claude/);
  assert.match(productionPatch, /build_main_process_injection_source/);
  const launchFunction = productionPatch.slice(
    productionPatch.indexOf("fn launch_windows_claude_desktop"),
    productionPatch.indexOf("fn launch_windows_claude_msix")
  );
  assert.match(launchFunction, /if localize \{/);
  assert.match(launchFunction, /close_existing_claude_for_localized_launch\(\)\?/);
  assert.match(launchFunction, /ensure_patch_files\(\)\?/);
  assert.match(launchFunction, /ensure_claude_desktop_developer_mode\(\)\?/);
  assert.match(launchFunction, /write_localized_launch_marker\(\)\?/);
  assert.match(launchFunction, /spawn_silent_localization_injector\(\)/);
  assert.doesNotMatch(launchFunction, /ensure_windows_claude_main_process_debugger\(\)\?/);
  assert.doesNotMatch(launchFunction, /retry_inject_localization\(\)\?/);
  assert.ok(
    launchFunction.indexOf("ensure_patch_files()?") <
      launchFunction.indexOf("write_localized_launch_marker()?"),
    "Windows localized launch should prepare runtime files before writing the zh marker"
  );
  assert.doesNotMatch(launchFunction, new RegExp(["apply", "_localization_patch\\(\\)\\?"].join("")));
  assert.doesNotMatch(launchFunction, new RegExp(["activate", "_localized_claude\\(\\)\\?"].join("")));
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
  // (shell:AppsFolder) and never pass debug arguments. The debugger is opened
  // by the host through Claude's official Developer route after launch.
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

test("Claude Desktop external terminal localization starts the injector", () => {
  const terminalCommand = read("src-tauri/src/commands/install_terminal.rs");
  const startTerminalFunction = terminalCommand.slice(
    terminalCommand.indexOf("fn start_terminal_session"),
    terminalCommand.indexOf("fn normalized_working_directory")
  );
  const launchExternalFunction = terminalCommand.slice(
    terminalCommand.indexOf("pub async fn launch_tool_external"),
    terminalCommand.length
  );

  assert.match(startTerminalFunction, /ensure_localized_launch_prerequisites\(\)\?/);
  assert.match(launchExternalFunction, /ensure_localized_launch_prerequisites\(\)\?/);
  assert.ok(
    launchExternalFunction.indexOf("ensure_localized_launch_prerequisites()?") <
      launchExternalFunction.indexOf("patched_launch_command"),
    "external terminal localized launch should preflight permissions before building the command"
  );
  assert.match(launchExternalFunction, /spawn_external_terminal/);
  assert.match(launchExternalFunction, /localize && request\.tool_id == "claude-desktop"/);
  assert.match(launchExternalFunction, /spawn_silent_localization_injector\(\)/);
});

test("Claude Desktop isolates Windows App and EXE tab operation state", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");
  const store = read("src/lib/claudeDesktopStore.ts");
  const types = read("src/types.ts");
  const coreTypes = read("src-tauri/src/core/types.rs");
  const coreInstaller = read("src-tauri/src/core/tool_installer.rs");

  assert.match(store, /export type ClaudeDesktopInstallKind = "msix" \| "exe"/);
  assert.match(store, /kindViews:\s*Record<ClaudeDesktopInstallKind,\s*ClaudeDesktopKindViewState>/);
  assert.match(store, /function selectedKindView/);
  assert.match(store, /patchKind\(/);
  assert.match(store, /function applyKindStatusesFromSnapshot/);
  assert.match(store, /const isWindows = snapshot\.platform === "windows"/);
  assert.match(store, /const installKinds = isWindows \? \(snapshot\.claudeInstallKinds \?\? null\) : null/);
  assert.match(store, /progressInstallKind\(progress\)/);
  assert.match(store, /patchKind\(kind,\s*\{[\s\S]*progressLogs:/);
  assert.match(store, /progress:\s*ToolInstallProgress \| null/);
  assert.match(store, /progressFromInstallEvent/);
  assert.match(store, /progress:\s*progressFromInstallEvent\(progress\)/);
  assert.match(store, /progressSeed\(installKind,\s*"claudeDesktop.progressInstallPreparing"/);
  assert.match(store, /progressSeed\(installKind,\s*"claudeDesktop.progressUpdatePreparing"/);
  assert.match(store, /uninstallTool\(\{\s*toolId: CLAUDE_DESKTOP_TOOL_ID,\s*confirm: true,\s*installKind/);
  assert.match(store, /function claudeDesktopExeInstallDetected/);
  assert.match(store, /export function claudeDesktopVisibleInstallKinds/);
  assert.match(store, /claudeDesktopExeInstallDetected\(view\.installKinds\) \? \["msix", "exe"\] : \["msix"\]/);
  assert.match(store, /export async function installOrUpdateClaudeDesktopKind/);
  assert.match(store, /state\.kindViews\[installKind\]/);
  assert.match(store, /installKind === "exe"/);
  assert.match(store, /Claude Desktop EXE installation is no longer supported/);
  assert.match(store, /selectedKind:\s*isWindows \? current\.selectedKind : "msix"/);
  assert.doesNotMatch(store, /selectedKind:\s*current\.selectedKind === "exe" && !claudeDesktopExeInstallDetected/);
  assert.match(store, /installPlan:\s*null/);
  assert.match(store, /updatePlan:\s*null/);

  assert.match(route, /claudeDesktopVisibleInstallKinds/);
  assert.match(route, /visibleInstallKinds\s*=\s*claudeDesktopVisibleInstallKinds\(view\)/);
  assert.match(route, /\{#each visibleInstallKinds as kind\}/);
  assert.match(route, /installOrUpdateClaudeDesktopKind\(effectiveSelectedKind,\s*"install"\)/);
  assert.match(route, /installOrUpdateClaudeDesktopKind\(effectiveSelectedKind,\s*"update"\)/);
  assert.match(route, /removeClaudeDesktop\(effectiveSelectedKind\)/);
  assert.match(route, /refreshClaudeDesktop\(false,\s*effectiveSelectedKind\)/);
  assert.doesNotMatch(route, /on:click=\{\(\) => setClaudeDesktopSelectedKind\("exe"\)\}/);
  assert.match(route, /kindView\s*=\s*view\.kindViews\[effectiveSelectedKind\]/);
  assert.match(route, /status\s*=\s*kindView\.status/);
  assert.match(route, /installPlan\s*=\s*kindView\.installPlan/);
  assert.match(route, /updatePlan\s*=\s*kindView\.updatePlan/);
  assert.match(route, /activePlan\s*=\s*installed \? updatePlan : installPlan/);
  assert.match(route, /activePlanAvailable\s*=\s*Boolean\(activePlan\?\.canInstall\)/);
  assert.match(route, /\$t\("codexClient\.planTitle"\)/);
  assert.match(route, /activePlan\.command \|\| \$t\("common\.none"\)/);
  assert.match(route, /activePlan\.requiresAdmin \? \$t\("toolInstall\.adminMayPrompt"\) : \$t\("toolInstall\.userScope"\)/);
  assert.match(route, /\{#each activePlan\.prerequisites as prerequisite\}/);
  assert.match(route, /\{#each activePlan\.commands as command\}/);
  assert.match(route, /\{#each activePlan\.steps as step\}/);
  assert.match(route, /\{#if activePlanBlocker\}/);
  assert.match(route, /busyAction\s*=\s*kindView\.busyAction/);
  assert.match(route, /progress\s*=\s*kindView\.progress/);
  assert.match(route, /progressPercent\s*=\s*progress\?\.percent/);
  assert.match(route, /progressByteLabel\(progress\)/);
  assert.match(route, /class="install-progress"/);
  assert.match(route, /claudeDesktop\.progressBytes/);
  assert.match(route, /liveLogGroups\s*=\s*groupedProgressLogs\(kindView\.progressLogs\)/);
  assert.match(route, /kindView\.loading/);
  assert.doesNotMatch(route, /status\s*=\s*view\.status/);
  assert.doesNotMatch(route, /installPlan\s*=\s*view\.installPlan/);
  assert.doesNotMatch(route, /updatePlan\s*=\s*view\.updatePlan/);
  assert.doesNotMatch(route, /busyAction\s*=\s*view\.busyAction/);
  assert.doesNotMatch(route, /view\.progressLogs/);
  assert.doesNotMatch(route, /view\.loading/);
  assert.doesNotMatch(route, /effectiveSelectedKind = selectedKind === "exe"/);

  assert.match(types, /export interface ToolInstallProgress \{[\s\S]*installKind\?: "msix" \| "exe" \| null;[\s\S]*phase\?: string \| null;[\s\S]*downloaded\?: number \| null;[\s\S]*total\?: number \| null;[\s\S]*percent\?: number \| null;[\s\S]*step\?: number \| null;[\s\S]*stepTotal\?: number \| null;/);
  assert.match(coreTypes, /pub struct ToolInstallProgress \{[\s\S]*pub install_kind: Option<String>,[\s\S]*pub phase: Option<String>,[\s\S]*pub downloaded: Option<u64>,[\s\S]*pub total: Option<u64>,[\s\S]*pub percent: Option<f64>,[\s\S]*pub step: Option<u32>,[\s\S]*pub step_total: Option<u32>,/);
  assert.match(coreInstaller, /install_kind: Option<&'a str>,/);
  assert.match(coreInstaller, /progress_phase: Option<&'a str>,/);
  assert.match(coreInstaller, /install_kind: context\.install_kind\.map\(ToString::to_string\),/);
  assert.match(coreInstaller, /phase: context\.progress_phase\.map\(ToString::to_string\),/);
});

test("Claude Desktop Windows uninstall avoids ambiguous winget removal", () => {
  const installer = read("src-tauri/src/core/tool_installer.rs");
  const packageCore = read("src-tauri/src/core/platform/package.rs");

  assert.match(installer, /run_claude_desktop_windows_uninstall/);
  assert.match(installer, /remove_first_msix_package/);
  assert.match(installer, /remove_claude_msix_payloads/);
  assert.match(installer, /CLAUDE_DESKTOP_WINDOWS_PACKAGE_SUFFIX/);
  assert.match(packageCore, /pub fn remove_first_msix_package/);
  assert.match(packageCore, /pub fn remove_claude_msix_payloads/);
  assert.match(packageCore, /remainingPayloads/);
  assert.match(packageCore, /Test-ClaudePartialPayloadDirectory/);
  assert.match(packageCore, /app\\resources\\cowork-svc\.exe/);
  assert.match(packageCore, /Remove-Item -LiteralPath \$dir\.FullName -Recurse -Force/);
  assert.match(packageCore, /remove_msix_package\(package_name\)/);
  assert.match(installer, /run_claude_desktop_exe_uninstall/);
  assert.match(installer, /InstallLocation/);
  assert.match(installer, /remaining install roots/);
  assert.match(installer, /CoworkVMService/);
  assert.match(installer, /cowork-svc/);
  assert.match(installer, /remove_claude_desktop_windows_background_services/);
  assert.match(installer, /0x8A150016/);
  assert.match(installer, /Multiple packages match/);

  const uninstallFunction = installer.slice(
    installer.indexOf("pub fn uninstall_tool_with_progress"),
    installer.indexOf("pub fn repair_tool_path")
  );
  assert.match(uninstallFunction, /tool_id == "claude-desktop" && cfg!\(target_os = "windows"\)/);
  assert.match(uninstallFunction, /resolve_claude_desktop_windows_uninstall_kind/);
  assert.match(uninstallFunction, /run_claude_desktop_windows_uninstall\(\s*claude_windows_install_kind\.as_deref\(\),\s*Some\(&context\),\s*\)\?/);
  assert.match(uninstallFunction, /claude_desktop_windows_uninstall_verified/);
  assert.match(uninstallFunction, /claude_desktop_windows_registered_msix_installed/);
  assert.doesNotMatch(uninstallFunction, /Anthropic\.Claude/);
});

test("Claude Desktop Windows install uses official MSIX package instead of winget or legacy EXE", () => {
  const installer = read("src-tauri/src/core/tool_installer.rs");
  const detector = read("src-tauri/src/core/detector.rs");

  assert.match(installer, /ClaudeDesktopWindowsMsix/);
  assert.match(installer, /CLAUDE_DESKTOP_WINDOWS_MSIX_URL/);
  assert.match(installer, /claude\.ai\/api\/desktop\/win32\/x64\/msix\/latest\/redirect/);
  assert.match(installer, /Add-AppxPackage -Path/);
  assert.match(installer, /Download and install the latest Claude Desktop MSIX/);
  assert.match(installer, /run_claude_desktop_windows_msix_install/);
  assert.match(installer, /download_url_to_file\(\s*CLAUDE_DESKTOP_WINDOWS_MSIX_URL/);
  assert.match(installer, /emit_install_download_progress/);
  assert.match(installer, /progress_phase: Some\("downloading"\)/);
  assert.match(installer, /remove_stale_claude_desktop_windows_exe_uninstall_entries/);
  assert.match(installer, /AnthropicClaude/);
  assert.match(installer, /InstallLocation/);
  assert.match(installer, /Get-ItemProperty/);
  assert.match(installer, /Remove-Item -LiteralPath \$prop\.PSPath/);

  const claudeDefinition = installer.slice(
    installer.indexOf('"claude-desktop" => {'),
    installer.indexOf('"claude-vscode"', installer.indexOf('"claude-desktop" => {'))
  );
  assert.match(claudeDefinition, /InstallAction::ClaudeDesktopWindowsMsix/);
  assert.doesNotMatch(claudeDefinition, /InstallAction::Winget\("Anthropic\.Claude"\)/);

  const installScript = installer.slice(
    installer.indexOf("const CLAUDE_DESKTOP_WINDOWS_MSIX_INSTALL_SCRIPT"),
    installer.indexOf("const CLAUDE_DESKTOP_WINDOWS_EXE_UNINSTALL_SCRIPT")
  );
  assert.doesNotMatch(installScript, /win32\/x64\/\.latest/);
  assert.doesNotMatch(installScript, /Claude-\$hash\.exe/);
  assert.doesNotMatch(installScript, /Start-Process -FilePath \$target/);
  assert.doesNotMatch(installScript, /Invoke-WebRequest/);

  const updateCommandFunction = detector.slice(
    detector.indexOf("fn update_command_for_tool"),
    detector.indexOf("fn read_npm_global_outdated")
  );
  assert.match(detector, /CLAUDE_DESKTOP_WINDOWS_UPDATE_COMMAND/);
  assert.match(detector, /claude\.ai\/api\/desktop\/win32\/x64\/msix\/latest\/redirect/);
  assert.match(detector, /Add-AppxPackage -Path/);
  assert.match(updateCommandFunction, /CLAUDE_DESKTOP_WINDOWS_UPDATE_COMMAND\.to_string\(\)/);
  assert.doesNotMatch(updateCommandFunction, /winget upgrade --id Anthropic\.Claude/);

  const wingetPackageFunction = detector.slice(
    detector.indexOf("fn winget_package_for_tool"),
    detector.indexOf("fn update_command_for_tool")
  );
  assert.doesNotMatch(
    wingetPackageFunction,
    /"claude-desktop"\s*=>\s*Some\("Anthropic\.Claude"\)/
  );
});

test("macOS app bundle is signed as a bundle for stable Accessibility trust", () => {
  const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));
  const packageJson = JSON.parse(read("package.json"));
  const signScript = read("scripts/sign-macos-bundle.sh");
  const dmgScript = read("scripts/build-macos-dmg.sh");
  const createDmgScript = read("scripts/create-macos-dmg.sh");

  assert.equal(tauriConfig.bundle?.macOS?.signingIdentity, "-");
  assert.equal(tauriConfig.bundle?.macOS?.infoPlist, "Info.plist");
  assert.deepEqual(tauriConfig.bundle?.macOS?.dmg?.windowSize, { width: 660, height: 400 });
  assert.deepEqual(tauriConfig.bundle?.macOS?.dmg?.appPosition, { x: 180, y: 170 });
  assert.deepEqual(tauriConfig.bundle?.macOS?.dmg?.applicationFolderPosition, { x: 480, y: 170 });
  assert.equal(packageJson.scripts?.["tauri:build:dmg"], "scripts/build-macos-dmg.sh");
  assert.equal(packageJson.scripts?.["tauri:sign:macos"], "scripts/sign-macos-bundle.sh");
  assert.match(signScript, /--requirements/);
  assert.match(signScript, /designated => identifier/);
  assert.match(signScript, /com\.codestudio\.lite/);
  assert.match(signScript, /codesign -dr -/);
  assert.match(dmgScript, /scripts\/sign-macos-bundle\.sh/);
  assert.match(dmgScript, /--no-sign/);
  assert.match(dmgScript, /Warn --no-sign flag detected/);
  assert.match(dmgScript, /Warn Skipping signing due to --no-sign flag/);
  assert.match(dmgScript, /create-macos-dmg\.sh/);
  assert.ok(
    dmgScript.indexOf("scripts/sign-macos-bundle.sh") < dmgScript.indexOf("create-macos-dmg.sh")
  );
  assert.match(createDmgScript, /bundle\?\.macOS\?\.dmg/);
  assert.match(createDmgScript, /while IFS=\$'\\t' read -r key value/);
  assert.doesNotMatch(createDmgScript, /eval/);
  assert.match(createDmgScript, /create_tauri_style_dmg/);
  assert.match(createDmgScript, /create_finder_layout_script/);
  assert.match(createDmgScript, /\/usr\/bin\/osascript/);
  assert.match(createDmgScript, /\.DS_STORE/);
  assert.match(createDmgScript, /set position of item "\$escaped_app_name"/);
  assert.match(createDmgScript, /set position of item "Applications"/);
  assert.match(createDmgScript, /DMG_ALLOW_PLAIN_FALLBACK/);
  assert.match(createDmgScript, /does not contain Tauri\/Finder window layout UI/);
  assert.match(createDmgScript, /hdiutil create/);
  assert.match(createDmgScript, /hdiutil makehybrid/);
  assert.match(createDmgScript, /hdiutil verify/);
  assert.doesNotMatch(dmgScript, /tauri build --bundles dmg/);
  assert.doesNotMatch(createDmgScript, /tauri build --bundles dmg/);
});

test("Claude Desktop macOS Accessibility grant resumes through the page after explicit user confirmation", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");
  const store = read("src/lib/claudeDesktopStore.ts");
  const api = read("src/lib/api.ts");
  const types = read("src/types.ts");
  const commands = read("src-tauri/src/commands/claude_desktop.rs");
  const app = read("src/App.svelte");
  const lib = read("src-tauri/src/lib.rs");
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");
  const enUS = read("src/lib/locales/en-US.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");

  assert.match(types, /export interface ClaudeDesktopPendingLaunch/);
  assert.match(api, /takePendingClaudeDesktopLaunchAfterRestart/);
  assert.match(api, /restartClaudeDesktopAfterAccessibilityGrant/);
  assert.match(commands, /take_pending_claude_desktop_launch_after_restart/);
  assert.match(commands, /restart_claude_desktop_after_accessibility_grant/);
  assert.match(lib, /take_pending_claude_desktop_launch_after_restart/);
  assert.match(lib, /restart_claude_desktop_after_accessibility_grant/);
  assert.match(app, /takePendingClaudeDesktopLaunchAfterRestart/);
  assert.match(app, /setClaudeDesktopPendingLaunchAfterRestart/);
  assert.match(app, /pendingClaudeDesktopRouteRestore/);
  assert.match(app, /route\s*=\s*"claudeDesktop"/);
  assert.doesNotMatch(lib, /resume_pending_macos_localized_launch/);

  assert.match(store, /pendingLaunchAfterRestart:\s*ClaudeDesktopPendingLaunch \| null/);
  assert.match(store, /setClaudeDesktopPendingLaunchAfterRestart/);
  assert.match(store, /consumeClaudeDesktopPendingLaunchAfterRestart/);
  assert.match(route, /consumeClaudeDesktopPendingLaunchAfterRestart/);
  assert.match(route, /initializeClaudeDesktopPage/);
  assert.match(route, /await ensureClaudeDesktopLoaded\(\);[\s\S]*await resumePendingLaunchAfterRestart\(\);/);
  assert.match(route, /setClaudeDesktopPendingLaunchAfterRestart/);
  assert.match(route, /function cancelAccessibilityLaunch\(\)/);
  assert.match(route, /setClaudeDesktopPendingLaunchAfterRestart\(null\)/);
  assert.match(route, /on:click=\{cancelAccessibilityLaunch\}/);
  assert.match(route, /claudeDesktop\.accessibilityTitle/);
  assert.match(route, /restartClaudeDesktopAfterAccessibilityGrant/);
  assert.match(route, /ACCESSIBILITY_NOT_TRUSTED/);
  const restartHandler = route.slice(
    route.indexOf("async function restartAfterAccessibilityGrant"),
    route.indexOf("function cancelAccessibilityLaunch")
  );
  assert.match(restartHandler, /restartClaudeDesktopAfterAccessibilityGrant/);
  assert.doesNotMatch(restartHandler, /launchClaudeDesktop|launchClaudeWithLocalization/);

  const launchBody = patch.slice(
    patch.indexOf("fn launch_macos_claude_desktop_localized"),
    patch.indexOf("fn enable_macos_claude_main_process_debugger")
  );
  assert.match(launchBody, /ensure_macos_accessibility_trusted_for_localized_launch\(\)\?/);
  assert.doesNotMatch(launchBody, /schedule_macos_accessibility_restart|allow_accessibility_restart/);
  assert.doesNotMatch(patch, /MACOS_ACCESSIBILITY_PREFLIGHT_TIMEOUT|MACOS_ACCESSIBILITY_PREFLIGHT_RETRY_MS/);
  assert.match(patch, /write_macos_accessibility_pending_launch_marker/);
  assert.match(patch, /take_macos_accessibility_pending_launch_marker/);
  assert.match(patch, /app\.request_restart\(\)/);
  assert.doesNotMatch(patch, /app\.restart\(\)/);
  assert.match(patch, /macos_accessibility_restart_required_error/);
  assert.match(patch, /restart required before launching Claude/);

  for (const dictionary of [enUS, zhCN, zhTW]) {
    assert.match(dictionary, /"claudeDesktop\.accessibilityTitle"/);
    assert.match(dictionary, /"claudeDesktop\.accessibilityRestart"/);
  }
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
  assert.match(detector, /claude_desktop_windows_stale_msix_manifest/);
  const detectWindowsFunction = detector.slice(
    detector.indexOf("fn detect_claude_desktop_windows()"),
    detector.indexOf("fn detect_claude_desktop_windows_localappdata_scan")
  );
  assert.doesNotMatch(detectWindowsFunction, /stale_msix/);
});

test("Claude Desktop stale MSIX repair can use cached detection when WindowsApps cannot be enumerated", () => {
  const patch = read("src-tauri/src/core/claude_desktop_patch.rs");
  const detector = read("src-tauri/src/core/detector.rs");

  assert.match(patch, /claude_desktop_windows_cached_stale_msix_manifest/);
  assert.match(patch, /claude_desktop_windows_known_stale_msix_manifest/);
  assert.match(detector, /claude_desktop_windows_cached_stale_msix_manifest/);
  assert.match(detector, /claude_desktop_windows_known_stale_msix_manifest/);
  assert.match(detector, /storage::load_detection_cache/);
  assert.match(detector, /source_fragment: &str/);
  assert.match(detector, /"appx-stale"/);
  assert.match(detector, /CLAUDE_DESKTOP_WINDOWS_PACKAGE_SUFFIX/);
  assert.match(detector, /AppxManifest\.xml/);
});

test("Claude Desktop stale MSIX residue is not reported as installed", () => {
  const detector = read("src-tauri/src/core/detector.rs");

  assert.match(detector, /claude_desktop_status_from_detection/);
  assert.match(detector, /MSIX\/AppX package files are present but not registered/);
  const detectWindowsFunction = detector.slice(
    detector.indexOf("fn detect_claude_desktop_windows()"),
    detector.indexOf("fn detect_claude_desktop_windows_localappdata_scan")
  );
  assert.doesNotMatch(detectWindowsFunction, /appx-stale/);
  const installKindsFunction = detector.slice(
    detector.indexOf("pub fn claude_desktop_install_kinds()"),
    detector.indexOf("/// Search a LOCALAPPDATA-style root")
  );
  assert.doesNotMatch(installKindsFunction, /stale_msix/);
});
