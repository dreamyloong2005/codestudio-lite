import assert from "node:assert/strict";
import test from "node:test";
import { profileDisplayName } from "../../.tmp-tests/lib/profiles/presentation.js";

function profile(overrides = {}) {
  return {
    id: "custom-codex-profile",
    name: "codex 的配置",
    icon: null,
    remark: null,
    app: "codex",
    isBuiltin: false,
    mode: "config",
    provider: "compatible",
    protocol: "openai-responses",
    model: "gpt-5.5",
    reviewModel: null,
    modelMappings: [],
    baseUrl: "https://example.test/v1",
    authRef: "keychain:test/custom-codex-profile/api_key",
    createdAt: "2026-07-17T00:00:00Z",
    updatedAt: "2026-07-17T00:00:00Z",
    lastTestStatus: "pending",
    usageEnabled: false,
    sortOrder: 1,
    ...overrides
  };
}

test("custom profile display names preserve a leading tool name", () => {
  assert.equal(profileDisplayName(profile()), "codex 的配置");
  assert.equal(
    profileDisplayName(profile({ app: "claude", name: "Claude Code work" })),
    "Claude Code work"
  );
});

test("built-in official profiles still use their localized display name", () => {
  assert.equal(
    profileDisplayName(
      profile({ id: "official-codex", name: "Codex Official", isBuiltin: true, provider: "official" }),
      "Codex 官方"
    ),
    "Codex 官方"
  );
});
