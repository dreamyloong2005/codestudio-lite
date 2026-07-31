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

const successfulResult = () => ({
  success: true,
  data: [],
  error: null,
  queriedAt: "2026-07-31T00:01:00Z",
  source: "script"
});

const stateWithConfig = (request) => ({
  profileId: request.profileId,
  config: {
    profileId: request.profileId,
    enabled: request.enabled,
    templateType: request.templateType ?? "general",
    code: request.code ?? "",
    apiKey: request.apiKey ?? null,
    baseUrl: request.baseUrl ?? null,
    accessToken: request.accessToken ?? null,
    userId: request.userId ?? null,
    timeoutSeconds: request.timeoutSeconds ?? 10,
    autoQueryIntervalMinutes: request.autoQueryIntervalMinutes ?? 0,
    updatedAt: "2026-07-31T00:01:00Z"
  },
  defaultCode: "default-code",
  lastResult: null
});

const completeApi = (overrides = {}) => ({
  load: async (profileId) => ({ profileId, config: null, defaultCode: "default-code", lastResult: null }),
  save: async (request) => stateWithConfig(request),
  test: async () => successfulResult(),
  query: async () => successfulResult(),
  remove: async (profileId) => ({ profileId, config: null, defaultCode: "default-code", lastResult: null }),
  ...overrides
});

const recordingScheduler = () => {
  let nextHandle = 1;
  const callbacks = new Map();
  const cleared = [];
  return {
    callbacks,
    cleared,
    adapter: {
      setInterval(callback) {
        const handle = nextHandle++;
        callbacks.set(handle, callback);
        return handle;
      },
      clearInterval(handle) {
        cleared.push(handle);
        callbacks.delete(handle);
      }
    }
  };
};

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

test("usage controller normalizes and saves the current form", async () => {
  const requests = [];
  const api = completeApi({
    save: async (request) => {
      requests.push(request);
      return stateWithConfig(request);
    }
  });
  const controller = createProfileUsageController({ api, scheduler: scheduler() });
  await controller.open(profile("alpha"));
  controller.updateForm({ enabled: true, baseUrl: "https://alpha.example.com/", code: "script" });

  await controller.save();

  assert.equal(requests.length, 1);
  assert.equal(requests[0].baseUrl, "https://alpha.example.com");
  assert.equal(get(controller).status, "ready");
  assert.equal(get(controller).notice, "saved");
});

test("usage controller refuses a second remote operation while busy", async () => {
  const save = deferred();
  let queryCalls = 0;
  const api = completeApi({
    save: () => save.promise,
    query: async () => {
      queryCalls += 1;
      return successfulResult();
    }
  });
  const controller = createProfileUsageController({ api, scheduler: scheduler() });
  await controller.open(profile("alpha"));
  controller.updateForm({ enabled: true, code: "script" });

  const saving = controller.save();
  await controller.query();
  assert.equal(queryCalls, 0);
  save.resolve(stateWithConfig({ profileId: "alpha", enabled: true, code: "script" }));
  await saving;
});

test("usage controller replaces and disposes auto-query timers", async () => {
  let queryCalls = 0;
  const timers = recordingScheduler();
  const api = completeApi({
    load: async (profileId) => stateWithConfig({
      profileId,
      enabled: true,
      code: "script",
      autoQueryIntervalMinutes: 1
    }),
    query: async () => {
      queryCalls += 1;
      return successfulResult();
    }
  });
  const controller = createProfileUsageController({ api, scheduler: timers.adapter });

  await controller.open(profile("alpha"));
  const alphaCallback = timers.callbacks.get(1);
  assert.equal(timers.callbacks.size, 1);
  await controller.open(profile("beta"));
  const betaCallback = timers.callbacks.get(2);
  assert.deepEqual(timers.cleared, [1]);
  assert.equal(timers.callbacks.size, 1);

  controller.dispose();
  assert.deepEqual(timers.cleared, [1, 2]);
  alphaCallback();
  betaCallback();
  await Promise.resolve();
  assert.equal(queryCalls, 0);
});
