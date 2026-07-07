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

test("Codex launch options mirror Codex++ plugin and model toggles", () => {
  const route = read("src/routes/CodexClient.svelte");
  const store = read("src/lib/codexClientStore.ts");
  const types = read("src/types.ts");
  const core = read("src-tauri/src/core/codex_client.rs");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  const codexPlusPluginAndModelDefaults = {
    pluginMarketplaceUnlockOnLaunch: true,
    pluginAutoExpandOnLaunch: true,
    modelWhitelistUnlockOnLaunch: true,
    serviceTierControlsOnLaunch: false
  };

  for (const [key, defaultValue] of Object.entries(codexPlusPluginAndModelDefaults)) {
    assert.match(route, new RegExp(`settingsDraft\\.${key}`));
    assert.match(route, new RegExp(`updateCodexClientDraft\\(\\{ ${key}: event\\.currentTarget\\.checked \\}\\)`));
    assert.match(store, new RegExp(`${key}: ${defaultValue}`));
    assert.match(store, new RegExp(`${key}: settings\\.${key}`));
    assert.match(store, new RegExp(`${key}: preserveLaunchOptions && draft[\\s\\S]*\\? draft\\.${key}[\\s\\S]*: stateSettings\\.${key}`));
    assert.match(types, new RegExp(`${key}: boolean;`));
    assert.match(types, new RegExp(`${key}\\?: boolean \\| null;`));
  }

  for (const dictionary of [zhCN, zhTW, enUS]) {
    assert.match(dictionary, /"codexClient\.pluginMarketplaceUnlockOnLaunch"/);
    assert.match(dictionary, /"codexClient\.pluginAutoExpandOnLaunch"/);
    assert.match(dictionary, /"codexClient\.modelWhitelistUnlockOnLaunch"/);
    assert.match(dictionary, /"codexClient\.serviceTierControlsOnLaunch"/);
  }

  assert.doesNotMatch(route, /settingsDraft\.patchForcePluginUnlock/);
  assert.match(core, /pub plugin_marketplace_unlock_on_launch: bool/);
  assert.match(core, /pub plugin_auto_expand_on_launch: bool/);
  assert.match(core, /pub model_whitelist_unlock_on_launch: bool/);
  assert.match(core, /pub service_tier_controls_on_launch: bool/);
});

test("Codex launch options include the official remote plugin cache", () => {
  const route = read("src/routes/CodexClient.svelte");
  const store = read("src/lib/codexClientStore.ts");
  const types = read("src/types.ts");
  const api = read("src/lib/api.ts");
  const core = read("src-tauri/src/core/codex_client.rs");
  const moduleIndex = read("src-tauri/src/core/mod.rs");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  assert.match(route, /settingsDraft\.officialRemotePluginCacheOnLaunch/);
  assert.match(route, /updateCodexClientDraft\(\{ officialRemotePluginCacheOnLaunch: event\.currentTarget\.checked \}\)/);
  assert.match(store, /officialRemotePluginCacheOnLaunch: true/);
  assert.match(store, /officialRemotePluginCacheOnLaunch: settings\.officialRemotePluginCacheOnLaunch/);
  assert.match(store, /officialRemotePluginCacheOnLaunch: preserveLaunchOptions && draft[\s\S]*\? draft\.officialRemotePluginCacheOnLaunch[\s\S]*: stateSettings\.officialRemotePluginCacheOnLaunch/);
  assert.match(types, /officialRemotePluginCacheOnLaunch: boolean;/);
  assert.match(types, /officialRemotePluginCacheOnLaunch\?: boolean \| null;/);
  assert.match(api, /officialRemotePluginCacheOnLaunch: request\.officialRemotePluginCacheOnLaunch \?\? mockCodexClientSettings\.officialRemotePluginCacheOnLaunch/);
  assert.match(core, /pub official_remote_plugin_cache_on_launch: bool/);
  assert.match(core, /ensure_official_remote_plugin_cache_if_enabled\(&settings\)/);
  assert.match(moduleIndex, /pub mod codex_plugin_marketplace;/);

  const marketplace = read("src-tauri/src/core/codex_plugin_marketplace.rs");
  assert.match(marketplace, /OPENAI_CURATED_REMOTE_MARKETPLACE: &str = "openai-curated-remote"/);
  assert.match(marketplace, /plugins-remote/);
  assert.match(marketplace, /include_bytes!\(.+openai-curated-remote\.zip/);
  assert.match(marketplace, /ensure_official_remote_plugin_cache/);
  assert.match(marketplace, /source_type"\]\s*=\s*toml_edit::value\("local"\)/);

  for (const dictionary of [zhCN, zhTW, enUS]) {
    assert.match(dictionary, /"codexClient\.officialRemotePluginCacheOnLaunch"/);
    assert.match(dictionary, /"codexClient\.officialRemotePluginCacheOnLaunchHint"/);
  }
});

test("Codex plugin and model injection is gated by individual Codex++ launch options", () => {
  const core = read("src-tauri/src/core/codex_client.rs");

  assert.match(core, /struct CodexEnhancementInjectionSettings/);
  assert.match(core, /plugin_marketplace_unlock:\s*settings\.plugin_marketplace_unlock_on_launch/);
  assert.match(core, /plugin_auto_expand:\s*settings\.plugin_auto_expand_on_launch/);
  assert.match(core, /model_whitelist_unlock:\s*settings\.model_whitelist_unlock_on_launch/);
  assert.match(core, /service_tier_controls:\s*settings\.service_tier_controls_on_launch/);
  assert.match(core, /function codestudioLiteSettings\(\)/);
  assert.match(core, /if \(settings\.pluginMarketplaceUnlock\)/);
  assert.match(core, /if \(settings\.pluginAutoExpand\)/);
  assert.match(core, /if \(settings\.modelWhitelistUnlock\)/);
  assert.match(core, /if \(settings\.serviceTierControls\)/);
  assert.match(core, /function schedulePluginAutoExpand/);
  assert.match(core, /function patchCodexModelWhitelist/);
  assert.match(core, /function installCodexServiceTierDispatcherPatch/);
  assert.doesNotMatch(core, /function unlockInstallButtons/);
  assert.doesNotMatch(core, /强制安装/);
});

test("Codex model whitelist injection reads Codex++ local model catalog files", () => {
  const core = read("src-tauri/src/core/codex_client.rs");

  assert.match(core, /model_catalog_json/);
  assert.match(core, /fn collect_codex_model_catalog_json_models/);
  assert.match(core, /supported_in_api/);
  assert.match(core, /visibility/);
  assert.match(core, /profiles/);
});

test("Codex service tier injection mirrors the latest Codex++ Fast controls", () => {
  const core = read("src-tauri/src/core/codex_client.rs");

  assert.match(core, /const codexThreadServiceTierVersion = "1"/);
  assert.match(core, /const codexThreadServiceTierMaxEntries = 120/);
  assert.match(core, /const codexThreadServiceTierDraftBindWindowMs = 60 \* 1000/);
  assert.match(core, /const codexDefaultServiceTierSetting = \{ key: "default-service-tier", default: null \}/);
  assert.match(core, /function getCodexServiceTierSetting\(\)/);
  assert.match(core, /function readThreadServiceTierState\(\)/);
  assert.match(core, /function setCodexServiceTierControlMode\(mode\)/);
  assert.match(core, /function setCodexThreadServiceTierMode\(mode\)/);
  assert.match(core, /function codexServiceTierOverrideForRequest\(method, params, threadIdHint = ""\)/);
  assert.match(core, /global-standard/);
  assert.match(core, /global-fast/);
  assert.match(core, /custom/);
  assert.match(core, /fastBlocked/);
  assert.doesNotMatch(core, /function codexServiceTierMode\(\)/);
});

test("Codex service tier observer avoids badge self-trigger refresh loops", () => {
  const core = read("src-tauri/src/core/codex_client.rs");

  assert.match(core, /const codestudioLiteCodexEnhancementsVersion = "3"/);
  assert.match(core, /clearInterval\(window\.__codestudioLiteCodexEnhancementsTimer\)/);
  assert.match(core, /window\.__codestudioLiteCodexEnhancementsObserver\.disconnect\?\.\(\)/);
  assert.match(core, /function setCodestudioLiteText\(node, value\)/);
  assert.match(core, /if \(node\.textContent !== next\) node\.textContent = next;/);
  assert.match(core, /function setCodestudioLiteDataset\(node, name, value\)/);
  assert.match(core, /function shouldIgnoreCodestudioLiteMutations\(mutations\)/);
  assert.match(core, /data-codex-service-tier-badge="true"/);
  assert.match(core, /new MutationObserver\(\(mutations\) => scheduleCodestudioLiteRefresh\(mutations\)\)/);
  assert.match(core, /window\.requestAnimationFrame/);
  assert.match(core, /enhancement_refresh_temporarily_throttled/);
  assert.match(core, /attributeFilter: \["disabled", "aria-disabled", "class", "style"\]/);
  assert.doesNotMatch(core, /attributeFilter: \["disabled", "aria-disabled", "data-disabled", "class", "style"\]/);
});

test("Codex model whitelist refresh is not run twice from the main enhancement refresh", () => {
  const core = read("src-tauri/src/core/codex_client.rs");
  const refreshBody = core
    .split("function refresh(mutations = null) {")
    .at(1)
    ?.split("function runCodestudioLiteRefresh")
    .at(0);

  assert.ok(refreshBody, "enhancement refresh body should be present");
  assert.match(refreshBody, /patchCodexModelWhitelist\(mutations\)/);
  assert.doesNotMatch(refreshBody, /refreshCodexModelWhitelistFromScan\(mutations\)/);
});

test("Codex enhancement injection runs after launch without blocking the command", () => {
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
  assert.doesNotMatch(launchBody, /inject_codex_enhancements\(debug_port/);
  assert.match(launchBody, /spawn_codex_enhancement_injection\(debug_port/);
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
