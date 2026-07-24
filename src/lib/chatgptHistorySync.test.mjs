import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const read = (path) => fs.readFileSync(path, "utf8");

test("ChatGPT Desktop removes manual history management surfaces", () => {
  const commands = read("src-tauri/src/commands/chatgpt_desktop.rs");
  const lib = read("src-tauri/src/lib.rs");
  const api = read("src/lib/api.ts");
  const store = read("src/lib/chatgptDesktopStore.ts");
  const page = read("src/routes/ChatGPTDesktop.svelte");
  const types = read("src/types.ts");

  for (const command of [
    "load_chatgpt_history_sync_targets",
    "sync_chatgpt_history_now",
    "preview_chatgpt_session_index_cleanup",
    "apply_chatgpt_session_index_cleanup"
  ]) {
    assert.doesNotMatch(commands, new RegExp(`fn ${command}`));
    assert.doesNotMatch(lib, new RegExp(`commands::chatgpt_desktop::${command}`));
    assert.doesNotMatch(api, new RegExp(`invoke\\(\"${command}\"`));
  }

  assert.doesNotMatch(store, /historySyncTargets/);
  assert.doesNotMatch(store, /sessionIndexCleanupPreview/);
  assert.doesNotMatch(types, /ProviderSyncTargetList/);
  assert.doesNotMatch(types, /SessionIndexCleanupPreview/);
  assert.doesNotMatch(page, /data-history-sync-management/);
  assert.doesNotMatch(page, /runChatGPTHistorySync/);
  assert.doesNotMatch(page, /previewChatGPTHistoryIndexCleanup/);
  assert.match(page, /settingsDraft\.syncHistoryOnLaunch/);
});

test("automatic history sync is non-blocking but records failures", () => {
  const core = read("src-tauri/src/core/chatgpt_desktop.rs");
  const body = core.split("fn sync_history_if_enabled")[1]?.split("\n}")[0] ?? "";

  assert.doesNotMatch(body, /run_default_provider_sync\(\)\?/);
  assert.match(body, /ProviderSyncStatus::Skipped/);
  assert.match(body, /Severity::Warning/);
});
