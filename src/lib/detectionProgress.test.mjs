import assert from "node:assert/strict";
import test from "node:test";

import { mergeDetectionProgressSnapshot } from "../../.tmp-tests/lib/detectionProgress.js";

const status = (id, version) => ({ id, version });

test("detection progress preserves pending cached cards and replaces completed cards in place", () => {
  const current = {
    tools: [status("codex", "old"), status("claude", "old")],
    system: [status("git", "old")],
    problems: [{ id: "cached-problem" }],
    claudeInstallKinds: { msix: true },
    chatgptDesktopInstallKinds: { exe: true }
  };
  const partial = {
    tools: [status("claude", "new"), status("pi", "new")],
    system: [],
    problems: [],
    claudeInstallKinds: null,
    chatgptDesktopInstallKinds: null
  };

  const merged = mergeDetectionProgressSnapshot(current, partial);

  assert.deepEqual(merged.tools, [status("codex", "old"), status("claude", "new"), status("pi", "new")]);
  assert.deepEqual(merged.system, current.system);
  assert.equal(merged.problems, current.problems);
  assert.equal(merged.claudeInstallKinds, current.claudeInstallKinds);
  assert.equal(merged.chatgptDesktopInstallKinds, current.chatgptDesktopInstallKinds);
});

test("detection progress uses the partial snapshot directly before a cache exists", () => {
  const partial = { tools: [status("codex", "new")], system: [], problems: [] };
  assert.equal(mergeDetectionProgressSnapshot(null, partial), partial);
});
