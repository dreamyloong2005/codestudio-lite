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
