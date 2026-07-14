import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("startup reconciles native profiles even when the detection cache is fresh", () => {
  const app = read("src/App.svelte");
  const publicApi = read("src/lib/api.ts");
  const profileAdapter = read("src/lib/api/profiles.ts");
  const tauriAdapter = read("src/lib/api/tauri/profiles.ts");

  assert.match(profileAdapter, /loadSummary\(\): Promise<ProfileSummary>/);
  assert.match(tauriAdapter, /loadSummary: \(\) =>\s*runtime\.invoke<ProfileSummary>\("load_profile_summary"\)/);
  assert.match(publicApi, /export async function loadProfileSummary\(\): Promise<ProfileSummary>/);

  const cachedLoad = app.slice(
    app.indexOf("async function loadDashboardWithCache"),
    app.indexOf("async function restorePendingClaudeDesktopLaunch")
  );
  assert.match(cachedLoad, /loadProfileSummary\(\)/);
});

test("dashboard refresh reads the profile summary after native detection sync", () => {
  const app = read("src/App.svelte");
  const refresh = app.slice(
    app.indexOf("async function refreshDashboard"),
    app.indexOf("async function loadDashboardWithCache")
  );

  const detectionIndex = refresh.indexOf("detectEnvironment({ waitForUpdates })");
  const summaryIndex = refresh.indexOf("ensureAppDirs()");
  assert.ok(detectionIndex >= 0, "dashboard refresh should run environment detection");
  assert.ok(summaryIndex > detectionIndex, "profile summary must be read after native detection sync finishes");
});
