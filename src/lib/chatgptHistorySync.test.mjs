import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const read = (path) => fs.readFileSync(path, "utf8");

test("ChatGPT Desktop exposes complete history sync management", () => {
  const commands = read("src-tauri/src/commands/chatgpt_desktop.rs");
  const lib = read("src-tauri/src/lib.rs");
  const api = read("src/lib/api.ts");
  const store = read("src/lib/chatgptDesktopStore.ts");
  const page = read("src/routes/ChatGPTDesktop.svelte");

  for (const command of [
    "load_chatgpt_history_sync_targets",
    "sync_chatgpt_history_now",
    "preview_chatgpt_session_index_cleanup",
    "apply_chatgpt_session_index_cleanup"
  ]) {
    assert.match(commands, new RegExp(`fn ${command}`));
    assert.match(lib, new RegExp(`commands::chatgpt_desktop::${command}`));
    assert.match(api, new RegExp(`invoke\\(\"${command}\"`));
  }

  assert.match(store, /historySyncTargets/);
  assert.match(store, /historySyncResult/);
  assert.match(store, /sessionIndexCleanupPreview/);
  assert.match(page, /data-history-sync-management/);
  assert.match(page, /encryptedContentWarning/);
  assert.match(page, /sessionIndexCleanupPreview/);
});

test("automatic history sync is non-blocking but records failures", () => {
  const core = read("src-tauri/src/core/chatgpt_desktop.rs");
  const body = core.split("fn sync_history_if_enabled")[1]?.split("\n}")[0] ?? "";

  assert.doesNotMatch(body, /run_default_provider_sync\(\)\?/);
  assert.match(body, /ProviderSyncStatus::Skipped/);
  assert.match(body, /Severity::Warning/);
});
