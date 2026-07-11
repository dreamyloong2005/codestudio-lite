import assert from "node:assert/strict";
import test from "node:test";
import {
  updateGatewayProfileDisplay,
  upsertProfileDraftInSummary
} from "../../.tmp-tests/lib/profileSummary.js";

function profile(overrides = {}) {
  return {
    id: "profile-alpha",
    name: "Alpha",
    icon: null,
    remark: null,
    app: "codex",
    isBuiltin: false,
    mode: "config",
    provider: "compatible",
    protocol: "openai-responses",
    model: "gpt-5.5",
    modelMappings: [],
    baseUrl: "https://example.test/v1",
    authRef: "keychain:test/profile-alpha/api_key",
    createdAt: "2026-07-12T00:00:00Z",
    updatedAt: "2026-07-12T00:00:00Z",
    lastTestStatus: "pending",
    usageEnabled: false,
    sortOrder: 1,
    ...overrides
  };
}

function summary(drafts) {
  return {
    configDir: "~/.codestudio-lite",
    activeProfile: "profile-alpha",
    activeProfileName: "Alpha",
    activeProfilesByMode: {
      config: { codex: "profile-alpha" },
      gateway: {}
    },
    codexAuth: {
      available: true,
      method: "api_key",
      storage: "auth_json",
      path: "~/.codex/auth.json",
      detail: "API key available"
    },
    drafts
  };
}

test("upsertProfileDraftInSummary replaces an edited draft and active name immediately", () => {
  const originalDraft = profile();
  const originalSummary = summary([originalDraft, profile({ id: "profile-beta", name: "Beta" })]);
  const updatedDraft = profile({ name: "Renamed", updatedAt: "2026-07-12T00:01:00Z" });

  const next = upsertProfileDraftInSummary(originalSummary, updatedDraft);

  assert.notEqual(next, originalSummary);
  assert.equal(next.activeProfileName, "Renamed");
  assert.equal(next.drafts[0], updatedDraft);
  assert.equal(next.drafts[1], originalSummary.drafts[1]);
  assert.equal(originalSummary.activeProfileName, "Alpha");
  assert.equal(originalSummary.drafts[0], originalDraft);
});

test("upsertProfileDraftInSummary appends a newly saved draft without changing active metadata", () => {
  const originalSummary = summary([profile()]);
  const created = profile({ id: "profile-new", name: "New Profile", sortOrder: 2 });

  const next = upsertProfileDraftInSummary(originalSummary, created);

  assert.deepEqual(next.drafts.map((draft) => draft.id), ["profile-alpha", "profile-new"]);
  assert.equal(next.activeProfileName, "Alpha");
});

test("updateGatewayProfileDisplay refreshes only the matching active profile name", () => {
  const gateway = {
    running: true,
    host: "127.0.0.1",
    port: 43112,
    baseUrl: "http://127.0.0.1:43112/v1",
    healthUrl: "http://127.0.0.1:43112/health",
    authEnabled: true,
    tokenPreview: "codestudio-local-****",
    privacyFilterMode: "off",
    activeProfileId: "profile-alpha",
    activeProfileName: "Alpha",
    activeModel: "gpt-5.5",
    startedAt: null,
    lastError: null
  };

  const updated = updateGatewayProfileDisplay(gateway, profile({ name: "Renamed" }));
  const unrelated = updateGatewayProfileDisplay(gateway, profile({ id: "profile-beta", name: "Beta" }));

  assert.equal(updated.activeProfileName, "Renamed");
  assert.equal(unrelated, gateway);
});
