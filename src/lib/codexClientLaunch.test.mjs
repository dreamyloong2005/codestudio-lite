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
  assert.match(commands, /pub fn launch_codex_client\(\)/);
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

test("Codex client hides stale update plan while settings are being replanned", () => {
  const route = read("src/routes/CodexClient.svelte");
  const store = read("src/lib/codexClientStore.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  assert.match(store, /planRefreshing:\s*boolean/);
  assert.match(store, /planStale:\s*boolean/);
  assert.match(store, /function planAffectingSettingsChanged/);
  assert.match(store, /planAffectingSettingsChanged\(lastSavedSettings,\s*nextDraft\)/);
  assert.match(route, /planUnavailable\s*=\s*planRefreshing\s*\|\|\s*view\.planStale/);
  assert.match(route, /effectivePlan\s*=\s*planUnavailable\s*\?\s*null\s*:\s*plan/);
  assert.match(route, /effectiveRelease\s*=\s*planUnavailable\s*\?\s*null\s*:\s*release/);
  assert.match(route, /\{#if planUnavailable\}/);
  assert.match(route, /codexClient\.planStale/);

  for (const dictionary of [zhCN, zhTW, enUS]) {
    assert.match(dictionary, /"codexClient\.planRefreshing"/);
    assert.match(dictionary, /"codexClient\.planStale"/);
  }
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
