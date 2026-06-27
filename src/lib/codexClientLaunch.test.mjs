import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("Codex client exposes a single patch-backed launch entrypoint", () => {
  const route = read("src/routes/CodexClient.svelte");
  const store = read("src/lib/codexClientStore.ts");
  const api = read("src/lib/api.ts");
  const commands = read("src-tauri/src/commands/codex_client.rs");
  const lib = read("src-tauri/src/lib.rs");
  const core = read("src-tauri/src/core/codex_client.rs");

  assert.match(route, /launchManagedCodexClient/);
  assert.doesNotMatch(route, /launchManagedCodexClientPatched|patchLaunch|codexClient\.patchLaunch/);
  assert.doesNotMatch(store, /launchManagedCodexClientPatched|launchPatchedCodexClient|patchLaunch/);
  assert.match(api, /invoke\("launch_codex_client"\)/);
  assert.doesNotMatch(api, /launchPatchedCodexClient|launch_codex_client_patched/);
  assert.match(commands, /pub async fn launch_codex_client\(\)/);
  assert.doesNotMatch(commands, /launch_codex_client_patched|launch_patched/);
  assert.doesNotMatch(lib, /launch_codex_client_patched/);
  assert.match(core, /pub fn launch\(\) -> Result<\(\), String> \{[\s\S]*codex_patch_launch_args/);
  assert.doesNotMatch(core, /pub fn launch_patched/);
});

test("Codex plugin force unlock includes modern marketplace request patches", () => {
  const core = read("src-tauri/src/core/codex_client.rs");

  assert.match(core, /Page\.addScriptToEvaluateOnNewDocument/);
  assert.match(core, /allowUnsafeEvalBlockedByCSP/);
  assert.match(core, /function patchPluginMarketplaceRequestParams/);
  assert.match(core, /method === "list-plugins"/);
  assert.match(core, /delete next\.marketplaceKinds/);
  assert.match(core, /function restorePluginMarketplaceName/);
  assert.match(core, /method === "install-plugin"/);
  assert.match(core, /app-server-manager-signals-/);
  assert.match(core, /Array\.prototype\.filter/);
  assert.match(core, /plugin_marketplace_hidden_filter_bypassed/);
});

test("Codex plugin force unlock injection runs after launch without blocking the command", () => {
  const commands = read("src-tauri/src/commands/codex_client.rs");
  const core = read("src-tauri/src/core/codex_client.rs");
  const launchBody = core
    .split("pub fn launch() -> Result<(), String> {")
    .at(1)
    ?.split("pub fn restart()")
    .at(0);

  assert.ok(launchBody, "Codex launch body should be present");
  assert.match(commands, /pub async fn launch_codex_client\(\) -> Result<\(\), String>/);
  assert.match(commands, /spawn_blocking\(\|\| codex_client::launch\(\)\)/);
  assert.doesNotMatch(launchBody, /inject_plugin_unlock\(debug_port\)\?/);
  assert.match(launchBody, /spawn_plugin_unlock_injection\(debug_port\)/);
});

test("Codex client notices are localized and dismiss with an icon", () => {
  const route = read("src/routes/CodexClient.svelte");
  const store = read("src/lib/codexClientStore.ts");
  const notice = read("src/components/DismissibleNotice.svelte");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  for (const source of [store, route]) {
    assert.doesNotMatch(source, /Codex client is ready/);
    assert.doesNotMatch(source, /Codex client launch requested/);
    assert.doesNotMatch(source, /Installer staged and verified/);
    assert.doesNotMatch(source, /Preparing to (stage|install) the Codex client/);
  }

  assert.match(store, /key:\s*"codexClient\.ready"/);
  assert.match(store, /key:\s*"codexClient\.launchRequested"/);
  assert.match(route, /formatNoticeMessage\(success\)/);
  assert.match(notice, /<AppIcon name="close"/);
  assert.doesNotMatch(notice, /\$t\("common\.dismiss"\)/);

  for (const dictionary of [zhCN, zhTW, enUS]) {
    assert.match(dictionary, /"codexClient\.ready"/);
    assert.match(dictionary, /"codexClient\.progressStagePreparing"/);
    assert.match(dictionary, /"codexClient\.progressInstallPreparing"/);
  }
});

test("Codex client keeps cached update plan visible while background refresh runs", () => {
  const route = read("src/routes/CodexClient.svelte");
  const store = read("src/lib/codexClientStore.ts");
  const api = read("src/lib/api.ts");
  const commands = read("src-tauri/src/commands/codex_client.rs");
  const lib = read("src-tauri/src/lib.rs");
  const storage = read("src-tauri/src/core/storage.rs");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  assert.match(store, /planRefreshing:\s*boolean/);
  assert.match(store, /planStale:\s*boolean/);
  assert.match(store, /function planAffectingSettingsChanged/);
  assert.match(store, /planAffectingSettingsChanged\(lastSavedSettings,\s*nextDraft\)/);
  assert.match(route, /planUnavailable\s*=\s*kindView\.planStale/);
  assert.doesNotMatch(route, /planUnavailable\s*=\s*planRefreshing\s*\|\|\s*view\.planStale/);
  assert.match(route, /effectivePlan\s*=\s*planUnavailable\s*\?\s*null\s*:\s*plan/);
  assert.match(route, /effectiveRelease\s*=\s*planUnavailable\s*\?\s*null\s*:\s*release/);
  assert.match(route, /\{effectivePlan\.packageUrl\}/);
  assert.match(route, /\{effectivePlan\.sha256\}/);
  assert.doesNotMatch(route, /planRefreshText\s*=\s*\$t\("codexClient\.planRefreshing"\)/);
  assert.doesNotMatch(route, /planRefreshing && effectivePlan/);
  assert.match(route, /\{#if planUnavailable\}/);
  assert.match(route, /codexClient\.planStale/);
  assert.match(store, /loadCachedCodexClientStates/);
  assert.doesNotMatch(store, /loadCachedCodexClientState,\s*\n/);
  assert.doesNotMatch(store, /await loadCachedCodexClientState\(\)/);
  assert.match(store, /function cachedStateEntries/);
  assert.match(store, /entries\.find\(\(\[kind,\s*state\]\) => kind === selectedKind && Boolean\(state\.plan\)\)/);
  assert.match(store, /patch\(\{ loaded:\s*true,\s*selectedKind:\s*preferredKind \}\)/);
  assert.match(api, /export async function loadCachedCodexClientStates\(\): Promise<CodexClientStateCache>/);
  assert.match(api, /invoke\("load_cached_codex_client_states"\)/);
  assert.match(commands, /pub async fn load_cached_codex_client_states\(\) -> Result<CodexClientStateCache,\s*String>/);
  assert.match(lib, /commands::codex_client::load_cached_codex_client_states/);
  assert.match(storage, /CREATE TABLE IF NOT EXISTS codex_client_state \(\s*install_kind TEXT PRIMARY KEY,/);
  assert.match(storage, /INSERT INTO codex_client_state \(install_kind,\s*generated_at,\s*state_json\)/);

  for (const dictionary of [zhCN, zhTW, enUS]) {
    assert.match(dictionary, /"codexClient\.planRefreshing"/);
    assert.match(dictionary, /"codexClient\.planStale"/);
  }
});

test("Codex client refresh preserves draft edits made while the scan is in flight", () => {
  const store = read("src/lib/codexClientStore.ts");

  assert.match(store, /type ApplyStateOptions = \{[\s\S]*preserveDraft\?: boolean/);
  assert.match(store, /const preserveDraft = Boolean\(options\.preserveDraft && current\.settingsDraft\)/);
  assert.match(store, /if \(!preserveDraft\) \{[\s\S]*lastSavedSettingsKey = settingsKey\(mergedSettings\)/);
  assert.match(store, /settingsDraft:\s*preserveDraft && existing\.settingsDraft\s*\? existing\.settingsDraft\s*:\s*\{ \.\.\.mergedSettings \}/);
  assert.match(store, /settingsSaveStatus:\s*preserveDraft\s*\? existing\.settingsSaveStatus\s*:\s*"idle"/);
  assert.match(store, /planStale:\s*preserveDraft\s*\? existing\.kindViews\[kind\]\.planStale\s*:\s*false/);
  assert.match(store, /const refreshSettingsRevision = settingsSaveRevision/);
  assert.match(store, /const preserveDraft = \(\) => settingsSaveRevision !== refreshSettingsRevision/);
  assert.match(store, /applyState\(nextState,\s*withNetwork \? installKind : stateInstallKind\(nextState\),\s*\{ preserveDraft: preserveDraft\(\) \}\)/);
  assert.match(store, /applyState\(nextState,\s*installKind,\s*\{ preserveDraft: preserveDraft\(\) \}\)/);
});

test("Codex client isolates Windows App and EXE tab operation state", () => {
  const route = read("src/routes/CodexClient.svelte");
  const store = read("src/lib/codexClientStore.ts");
  const api = read("src/lib/api.ts");
  const types = read("src/types.ts");
  const commands = read("src-tauri/src/commands/codex_client.rs");
  const core = read("src-tauri/src/core/codex_client.rs");

  assert.match(store, /export type CodexClientInstallKind = "msix" \| "portable"/);
  assert.match(store, /kindViews:\s*Record<CodexClientInstallKind,\s*CodexClientKindViewState>/);
  assert.match(store, /function selectedKindView/);
  assert.match(store, /patchKind\(/);
  assert.match(store, /listenCodexClientProgress\(\(progress\) => \{\s*patchKind\(progress\.installKind/);
  assert.match(store, /stageCodexClientUpdate\(\{\s*installKind/);
  assert.match(store, /planCodexClientUpdate\(\{\s*installKind/);
  assert.match(store, /operationResult:\s*result/);
  assert.match(route, /kindView\s*=\s*view\.kindViews\[effectiveSelectedKind\]/);
  assert.match(route, /stageReport\s*=\s*kindView\.stageReport/);
  assert.match(route, /operationResult\s*=\s*kindView\.operationResult/);
  assert.match(route, /progress\s*=\s*kindView\.progress/);
  assert.match(route, /busyAction\s*=\s*kindView\.busyAction/);
  assert.match(route, /state\s*=\s*kindView\.state/);
  assert.doesNotMatch(route, /stageReport\s*=\s*view\.stageReport/);
  assert.doesNotMatch(route, /operationResult\s*=\s*view\.operationResult/);
  assert.doesNotMatch(route, /progress\s*=\s*view\.progress/);
  assert.doesNotMatch(route, /busyAction\s*=\s*view\.busyAction/);
  assert.doesNotMatch(route, /state\s*=\s*view\.state/);

  assert.match(types, /export interface PlanCodexClientUpdateRequest \{[\s\S]*installKind\?: "msix" \| "portable" \| null;/);
  assert.match(types, /export interface StageCodexClientUpdateRequest \{[\s\S]*installKind\?: "msix" \| "portable" \| null;/);
  assert.match(types, /export interface CodexClientState \{[\s\S]*installKind: "msix" \| "portable";/);
  assert.match(types, /export interface CodexClientStageReport \{[\s\S]*installKind: "msix" \| "portable";/);
  assert.match(types, /export interface CodexClientProgress \{[\s\S]*installKind: "msix" \| "portable";/);
  assert.match(types, /export interface CodexClientOperationResult \{[\s\S]*installKind: "msix" \| "portable";/);
  assert.match(api, /export async function planCodexClientUpdate\(\s*request: PlanCodexClientUpdateRequest = \{\}/);
  assert.match(api, /invoke\("plan_codex_client_update", \{ request \}\)/);
  assert.match(api, /export async function stageCodexClientUpdate\(\s*request: StageCodexClientUpdateRequest/);
  assert.match(api, /invoke\("stage_codex_client_update", \{ request \}\)/);
  assert.match(commands, /PlanCodexClientUpdateRequest/);
  assert.match(commands, /StageCodexClientUpdateRequest/);
  assert.match(commands, /pub async fn plan_codex_client_update\(\s*request: PlanCodexClientUpdateRequest/);
  assert.match(commands, /pub async fn stage_codex_client_update\(\s*app: tauri::AppHandle,\s*request: StageCodexClientUpdateRequest/);
  assert.match(core, /pub struct PlanCodexClientUpdateRequest/);
  assert.match(core, /pub struct StageCodexClientUpdateRequest/);
  assert.match(core, /fn settings_for_install_kind/);
  assert.match(core, /pub fn plan_update\(request: PlanCodexClientUpdateRequest\)/);
  assert.match(core, /pub fn stage_update_with_progress<F>\(\s*request: StageCodexClientUpdateRequest/);
  assert.match(core, /fn select_install_route\(\s*settings:\s*&CodexClientSettings,\s*installed:\s*Option<&InstalledCodexClient>,\s*\) -> &'static str/);
  assert.doesNotMatch(core, /portable_recommended|Automatically switched to portable installation|progressMsixPortableFallback|progressMsixExecutionPortableFallback/);
});

test("Codex client does not expose a Windows official update-source choice", () => {
  const route = read("src/routes/CodexClient.svelte");
  const core = read("src-tauri/src/core/codex_client.rs");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  assert.match(
    route,
    /\{#if isMacos\}[\s\S]*\{\$t\("codexClient\.source"\)\}[\s\S]*<select[\s\S]*value=\{settingsDraft\.source\}[\s\S]*<option value="official">[\s\S]*\{\/if\}/
  );
  assert.doesNotMatch(route, /windowsOfficial|Microsoft Store installer|get\.microsoft\.com\/installer\/download|winget install Codex/);
  assert.match(core, /"official" if cfg!\(target_os = "macos"\) => "official"/);
  assert.doesNotMatch(core, /"official" if cfg!\(target_os = "windows"\)/);

  for (const dictionary of [zhCN, zhTW, enUS]) {
    assert.doesNotMatch(dictionary, /windowsOfficial/);
    assert.doesNotMatch(dictionary, /继续使用镜像|繼續使用映像|continues to use the mirror/);
  }
  assert.match(zhCN, /Windows 只能使用镜像源/);
  assert.match(zhTW, /Windows 只能使用映像來源/);
  assert.match(enUS, /Windows can only use the mirror source/);
});
