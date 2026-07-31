# Profile Usage State Machine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move profile usage loading, form state, operations, stale-response handling, timers, and dialog rendering out of `Profiles.svelte` into a tested TypeScript controller and focused Svelte dialog.

**Architecture:** `profileUsageController.ts` is a readable state machine with injected API and scheduler adapters. `ProfileUsageDialog.svelte` owns the controller and presentation; `Profiles.svelte` owns only selected-profile open/close integration.

**Tech Stack:** TypeScript, Svelte 5 legacy component syntax, Svelte stores, Node test runner, Panda CSS recipes.

---

### Task 1: Define and Test the Controller State Interface

**Files:**
- Create: `src/lib/profiles/profileUsageController.ts`
- Create: `src/lib/profileUsageController.test.mjs`
- Modify: `tsconfig.tests.json`

- [ ] **Step 1: Add the controller to the test compilation graph**

Add this entry to `tsconfig.tests.json` `include`:

```json
"src/lib/profiles/profileUsageController.ts"
```

- [ ] **Step 2: Write failing open/load and stale-response tests**

Create `src/lib/profileUsageController.test.mjs`:

```js
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
```

- [ ] **Step 3: Run the tests and verify they fail**

Run:

```bash
npx tsc -p tsconfig.tests.json
node --test --test-name-pattern='usage controller' src/lib/profileUsageController.test.mjs
```

Expected: TypeScript compilation fails because `profileUsageController.ts` does not exist.

- [ ] **Step 4: Implement the readable controller shell and load transition**

Create `profileUsageController.ts` with these public types and interface:

```ts
import { writable, type Readable } from "svelte/store";
import type {
  ProfileDraft,
  UsageQueryResult,
  UsageScriptSaveRequest,
  UsageScriptState,
  UsageScriptTemplateType
} from "../../types";

export type ProfileUsageStatus =
  | "closed"
  | "loading"
  | "ready"
  | "saving"
  | "testing"
  | "querying"
  | "deleting";

export interface ProfileUsageForm {
  enabled: boolean;
  templateType: UsageScriptTemplateType;
  code: string;
  apiKey: string;
  baseUrl: string;
  accessToken: string;
  userId: string;
  timeoutSeconds: number;
  autoQueryIntervalMinutes: number;
}

export interface ProfileUsageViewState {
  status: ProfileUsageStatus;
  profile: ProfileDraft | null;
  loaded: UsageScriptState | null;
  form: ProfileUsageForm;
  result: UsageQueryResult | null;
  officialOAuth: boolean;
  error: string | null;
  notice: "saved" | "tested" | "queried" | "deleted" | null;
}

export interface UsageApi {
  load(profileId: string): Promise<UsageScriptState>;
  save(request: UsageScriptSaveRequest): Promise<UsageScriptState>;
  test(request: UsageScriptSaveRequest): Promise<UsageQueryResult>;
  query(profileId: string): Promise<UsageQueryResult>;
  remove(profileId: string): Promise<UsageScriptState>;
}

export interface UsageScheduler {
  setInterval(callback: () => void, milliseconds: number): unknown;
  clearInterval(handle: unknown): void;
}

export interface ProfileUsageController extends Readable<ProfileUsageViewState> {
  open(profile: ProfileDraft): Promise<void>;
  close(): boolean;
  updateForm(patch: Partial<ProfileUsageForm>): void;
  selectTemplate(template: UsageScriptTemplateType): void;
  save(): Promise<void>;
  test(): Promise<void>;
  query(): Promise<void>;
  remove(): Promise<void>;
  dispose(): void;
}
```

Implement `createProfileUsageController` with a private `generation` counter. `open` increments the generation, writes `loading`, awaits `api.load`, and writes `ready` only if both generation and profile id still match. `close` and `dispose` increment the generation. Compute `officialOAuth` from the active profile using the existing canonical tool id and official-provider helpers so the dialog does not duplicate that rule.

- [ ] **Step 5: Run the focused tests and commit**

Run:

```bash
npx tsc -p tsconfig.tests.json
node --test --test-name-pattern='usage controller' src/lib/profileUsageController.test.mjs
```

Expected: both controller tests pass.

```bash
git add tsconfig.tests.json src/lib/profiles/profileUsageController.ts src/lib/profileUsageController.test.mjs
git commit -m "refactor: define profile usage state machine"
```

### Task 2: Implement Operations, Request Mapping, and Timers Test-First

**Files:**
- Modify: `src/lib/profiles/profileUsageController.ts`
- Modify: `src/lib/profileUsageController.test.mjs`

- [ ] **Step 1: Add failing operation and mutual-exclusion tests**

Add tests that use a call-recording `UsageApi` and assert:

```js
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
```

Define `completeApi`, `stateWithConfig`, and `successfulResult` in the test file with valid values for every `UsageApi` method; do not use partial production objects.

- [ ] **Step 2: Run and verify the new tests fail**

Run:

```bash
npx tsc -p tsconfig.tests.json
node --test --test-name-pattern='usage controller' src/lib/profileUsageController.test.mjs
```

Expected: FAIL because operation methods are not implemented.

- [ ] **Step 3: Implement form mapping and all remote operations**

Add private pure helpers:

```ts
const normalizeBaseUrl = (value: string) => value.trim().replace(/\/+$/, "");

const requestFrom = (profile: ProfileDraft, form: ProfileUsageForm): UsageScriptSaveRequest => ({
  profileId: profile.id,
  enabled: form.enabled,
  templateType: form.templateType,
  code: form.code,
  apiKey: form.apiKey.trim() ? form.apiKey : null,
  baseUrl: form.baseUrl.trim() ? normalizeBaseUrl(form.baseUrl) : null,
  accessToken: form.accessToken.trim() ? form.accessToken : null,
  userId: form.userId.trim() ? form.userId : null,
  timeoutSeconds: Number(form.timeoutSeconds),
  autoQueryIntervalMinutes: Number(form.autoQueryIntervalMinutes)
});
```

Implement `save`, `test`, `query`, and `remove` through one internal operation runner that:

1. returns immediately unless current status is `ready`;
2. captures generation and profile id;
3. writes the operation status and clears error/notice;
4. awaits exactly one API call;
5. applies the response only if generation and profile still match;
6. returns to `ready`, preserving the last valid form/result on error.

Move the five existing template bodies from `Profiles.svelte:590-687` verbatim into a pure `codeForTemplate` helper used by `selectTemplate`.

- [ ] **Step 4: Add and verify deterministic scheduler tests**

Add a fake scheduler that records callbacks and cleared handles. Test that an enabled configuration with a positive interval creates one timer, changing profiles clears it, and `dispose()` clears the final timer and prevents its callback from querying.

Run:

```bash
npx tsc -p tsconfig.tests.json
node --test --test-name-pattern='usage controller' src/lib/profileUsageController.test.mjs
```

Expected: all controller tests pass, including stale responses, operations, mutual exclusion, and timer disposal.

- [ ] **Step 5: Commit controller behavior**

```bash
git add src/lib/profiles/profileUsageController.ts src/lib/profileUsageController.test.mjs
git commit -m "refactor: isolate profile usage workflow state"
```

### Task 3: Extract the Profile Usage Dialog

**Files:**
- Create: `src/components/profiles/ProfileUsageDialog.svelte`
- Modify: `src/routes/Profiles.svelte:1-90, 130-190, 420-850, 1818-1998`
- Modify: `src/lib/pandaMigration.test.mjs`

- [ ] **Step 1: Write failing dialog ownership assertions**

Update the relevant Panda test setup:

```js
const route = read("src/routes/Profiles.svelte");
const usageDialog = read("src/components/profiles/ProfileUsageDialog.svelte");
const profileSurfaces = `${route}\n${usageDialog}`;
```

Add assertions:

```js
assert.match(route, /import ProfileUsageDialog from "\.\.\/components\/profiles\/ProfileUsageDialog\.svelte"/);
assert.match(route, /<ProfileUsageDialog/);
assert.doesNotMatch(route, /function handleUsageSave\(/);
assert.doesNotMatch(route, /function configureUsageAutoQuery\(/);
assert.match(usageDialog, /createProfileUsageController/);
assert.match(usageDialog, /profileUsageTemplateRowRecipe/);
assert.match(usageDialog, /data-usage-balance/);
```

Change existing usage-only recipe/markup assertions from `route` to `profileSurfaces` or `usageDialog`. Keep edit/apply/delete modal assertions pointed at `route`.

- [ ] **Step 2: Run the focused tests and verify they fail**

Run:

```bash
node --test --test-name-pattern='Profiles modal|Profiles edit and usage' src/lib/pandaMigration.test.mjs
```

Expected: FAIL because `ProfileUsageDialog.svelte` does not exist.

- [ ] **Step 3: Build the dialog on the controller interface**

Create `ProfileUsageDialog.svelte` with this small parent-facing interface:

```svelte
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    deleteUsageScript,
    loadUsageScriptState,
    queryProfileUsage,
    saveUsageScript,
    testUsageScript
  } from "../../lib/api";
  import { createProfileUsageController } from "../../lib/profiles/profileUsageController";
  import type { ProfileDraft } from "../../types";

  export let profile: ProfileDraft;
  export let formatError: (message: string) => string = (message) => message;
  export let onClose: () => void = () => {};

  const controller = createProfileUsageController({
    api: {
      load: loadUsageScriptState,
      save: saveUsageScript,
      test: testUsageScript,
      query: queryProfileUsage,
      remove: deleteUsageScript
    },
    scheduler: {
      setInterval: (callback, milliseconds) => window.setInterval(callback, milliseconds),
      clearInterval: (handle) => window.clearInterval(handle as number)
    }
  });

  $: state = $controller;
  onMount(() => void controller.open(profile));
  onDestroy(() => controller.dispose());
</script>
```

Move the existing usage-dialog markup from `Profiles.svelte` verbatim, replacing local variables and handlers with `state` and controller commands. Preserve all translation keys, recipes, data attributes, roles, labels, button ordering, disabled rules, and accessible title linkage.

The close button calls `onClose()` only when `controller.close()` returns `true`.

- [ ] **Step 4: Reduce parent ownership**

In `Profiles.svelte`:

- keep `pendingUsageProfile`, `openUsage`, `closeUsage`, `profileCanOpenUsage`, `profileUsesCodexOfficialOAuth`, and the shared `errorLabel` needed by the dialog interface;
- remove usage form/state/result/timer variables, `onDestroy`, API imports used only by usage, and all usage operation/helper functions;
- replace the inline dialog with:

```svelte
{#if pendingUsageProfile}
  <ProfileUsageDialog
    profile={pendingUsageProfile}
    formatError={errorLabel}
    onClose={closeUsage}
  />
{/if}
```

- [ ] **Step 5: Run Svelte and focused structural verification**

Run:

```bash
npm run check
node --test --test-name-pattern='Profiles modal|Profiles edit and usage' src/lib/pandaMigration.test.mjs
```

Expected: Svelte reports zero errors and zero warnings; focused tests pass.

- [ ] **Step 6: Commit the dialog extraction**

```bash
git add src/components/profiles/ProfileUsageDialog.svelte src/routes/Profiles.svelte src/lib/pandaMigration.test.mjs
git commit -m "refactor: extract profile usage dialog"
```

### Task 4: Verify and Deliver the Svelte Tranche

**Files:**
- Modify only failing source-location tests under `src/lib/*.test.mjs`
- Review: `src/routes/Profiles.svelte`
- Review: `src/components/profiles/ProfileUsageDialog.svelte`
- Review: `src/lib/profiles/profileUsageController.ts`

- [ ] **Step 1: Run the complete frontend verification**

Run:

```bash
npm test
npm run build
```

Expected: all existing and new tests pass; Svelte reports zero errors and warnings; Vite production build succeeds.

- [ ] **Step 2: Fix only explicit source-location assumptions**

If a test reads only `Profiles.svelte` for usage behavior, compose these sources:

```js
const usageSources = [
  read("src/routes/Profiles.svelte"),
  read("src/components/profiles/ProfileUsageDialog.svelte"),
  read("src/lib/profiles/profileUsageController.ts")
].join("\n");
```

Point the existing assertion to `usageSources`; do not delete behavior, styling, accessibility, or localization assertions.

- [ ] **Step 3: Verify responsibility movement and diff hygiene**

Run:

```bash
wc -l src/routes/Profiles.svelte src/components/profiles/ProfileUsageDialog.svelte src/lib/profiles/profileUsageController.ts
rg -n 'usageAutoQueryTimer|handleUsageSave|handleUsageTest|handleUsageQuery|handleUsageDelete' src/routes/Profiles.svelte
git diff --check HEAD~3
```

Expected: the `rg` command returns no matches in `Profiles.svelte`; the dialog and controller each have one clear responsibility; diff check passes.

- [ ] **Step 4: Run final repository checks applicable to this tranche**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
git status --short
```

Expected: Rust formatting remains clean and only intended refactor files plus pre-existing unrelated changes are present.

- [ ] **Step 5: Commit any necessary test retargeting**

If Step 2 changed tests:

```bash
git add src/lib/*.test.mjs
git commit -m "test: follow profile usage module seam"
```

Do not stage unrelated release-pipeline or planning files.
