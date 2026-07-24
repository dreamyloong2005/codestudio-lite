import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  canonicalProfileToolId,
  configProtocolIdsForTool,
  PROFILE_TOOL_CATALOG,
  profileSupportsConfigProtocol,
  profileSupportsModelMappings,
  profileSupportsReviewModel
} from "../../.tmp-tests/lib/profiles/catalog.js";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("profile catalog centralizes aliases and capabilities", () => {
  assert.equal(canonicalProfileToolId("chatgpt-desktop"), "codex");
  assert.equal(canonicalProfileToolId("claude-vscode"), "claude");
  assert.equal(canonicalProfileToolId("antigravity-cli"), "antigravity");
  assert.equal(canonicalProfileToolId("open-code"), "opencode");
  assert.equal(canonicalProfileToolId("pi-coding-agent"), "pi");

  assert.equal(PROFILE_TOOL_CATALOG.length, 9);
  assert.equal(PROFILE_TOOL_CATALOG.some((tool) => tool.id === "antigravity"), false);
  assert.deepEqual(configProtocolIdsForTool("codex"), ["openai-chat-completions", "openai-responses"]);
  assert.equal(profileSupportsConfigProtocol("grok", "anthropic-messages"), true);
  assert.equal(profileSupportsConfigProtocol("codex", "anthropic-messages"), false);
  assert.equal(profileSupportsModelMappings("claude"), true);
  assert.equal(profileSupportsReviewModel("codex"), true);
});

test("tool identity ownership stays in the catalogs", () => {
  const backendCatalog = read("src-tauri/src/core/tool_catalog.rs");
  const backendConsumers = [
    read("src-tauri/src/core/profile.rs"),
    read("src-tauri/src/core/tool_launch.rs"),
    read("src-tauri/src/core/env_health.rs"),
    read("src-tauri/src/core/gateway.rs")
  ];
  const frontendConsumers = [
    read("src/lib/api.ts"),
    read("src/routes/Profiles.svelte"),
    read("src/routes/SetupWizard.svelte")
  ];

  assert.match(backendCatalog, /pub fn canonical_tool_id/);
  for (const source of backendConsumers) {
    assert.doesNotMatch(source, /fn canonical_(?:profile_app|tool_id)\s*\(/);
  }
  for (const source of frontendConsumers) {
    assert.doesNotMatch(source, /function canonicalProfileToolId\s*\(/);
  }
});
