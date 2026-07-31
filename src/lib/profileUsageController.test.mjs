import assert from "node:assert/strict";
import test from "node:test";
import { get } from "svelte/store";
import { createProfileUsageController } from "../../.tmp-tests/lib/profiles/profileUsageController.js";

const profile = (id) => ({
  id,
  app: "codex",
  name: id,
  mode: "config",
  provider: "custom",
  protocol: "openai-chat-completions",
  model: "",
  reviewModel: "",
  modelMappings: [],
  baseUrl: `https://${id}.example.com`,
  authRef: null,
  remark: "",
  icon: "",
  isBuiltin: false,
  usageEnabled: false,
  createdAt: "2026-07-31T00:00:00Z",
  updatedAt: "2026-07-31T00:00:00Z",
  lastTestStatus: "pending",
  sortOrder: 0
});

const deferred = () => {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

const scheduler = () => ({
  setInterval: () => 1,
  clearInterval: () => {}
});

test("usage controller loads a profile into a ready form", async () => {
  const api = {
    load: async () => ({
      profileId: "alpha",
      config: null,
      defaultCode: "default-code",
      lastResult: null
    })
  };
  const controller = createProfileUsageController({ api, scheduler: scheduler() });

  await controller.open(profile("alpha"));

  const state = get(controller);
  assert.equal(state.status, "ready");
  assert.equal(state.profile.id, "alpha");
  assert.equal(state.form.baseUrl, "https://alpha.example.com");
  assert.equal(state.form.code, "default-code");
});

test("usage controller ignores a stale load after switching profiles", async () => {
  const alpha = deferred();
  const api = {
    load: (id) => (id === "alpha" ? alpha.promise : Promise.resolve({
      profileId: "beta",
      config: null,
      defaultCode: "beta-code",
      lastResult: null
    }))
  };
  const controller = createProfileUsageController({ api, scheduler: scheduler() });

  const first = controller.open(profile("alpha"));
  await controller.open(profile("beta"));
  alpha.resolve({ profileId: "alpha", config: null, defaultCode: "alpha-code", lastResult: null });
  await first;

  assert.equal(get(controller).profile.id, "beta");
  assert.equal(get(controller).form.code, "beta-code");
});
