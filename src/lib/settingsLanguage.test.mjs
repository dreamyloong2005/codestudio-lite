import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("settings language options use stable self-labels instead of locale dictionary keys", () => {
  const i18n = read("src/lib/i18n.ts");
  const settings = read("src/routes/Settings.svelte");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  assert.match(i18n, /code:\s*"zh-CN",\s*label:\s*"简体中文"/);
  assert.match(i18n, /code:\s*"zh-TW",\s*label:\s*"繁體中文"/);
  assert.match(i18n, /code:\s*"en-US",\s*label:\s*"English"/);
  assert.doesNotMatch(i18n, /settings\.language\.(zhCN|zhTW|enUS)|labelKey/);
  assert.match(settings, /\{locale\.label\}/);
  assert.doesNotMatch(settings, /\$t\(locale\.labelKey\)/);

  for (const dictionary of [zhCN, zhTW, enUS]) {
    assert.doesNotMatch(dictionary, /"settings\.language\.(zhCN|zhTW|enUS)"/);
  }
});

test("settings page ignores stale initial loads after local edits", () => {
  const settings = read("src/routes/Settings.svelte");

  assert.match(settings, /let settingsEditRevision = 0/);
  assert.match(settings, /const loadRevision = settingsEditRevision/);
  assert.match(settings, /if \(loadRevision !== settingsEditRevision\) \{[\s\S]*return;[\s\S]*\}/);
  assert.match(settings, /settingsEditRevision \+= 1;[\s\S]*language = nextLanguage/);
  assert.match(settings, /settingsEditRevision \+= 1;[\s\S]*theme = nextTheme/);
  assert.doesNotMatch(settings, /preserveCodexOfficialAuth|codexAuthPreservation/);
});

test("Codex official auth preservation is no longer user configurable", () => {
  const settings = read("src/routes/Settings.svelte");
  const profiles = read("src/routes/Profiles.svelte");
  const types = read("src/types.ts");
  const rustTypes = read("src-tauri/src/core/types.rs");
  const storage = read("src-tauri/src/core/storage.rs");
  const api = read("src/lib/api.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  for (const source of [settings, profiles, types, rustTypes, storage, api, zhCN, zhTW, enUS]) {
    assert.doesNotMatch(source, /preserveCodexOfficialAuth|preserve_codex_official_auth|codexAuthPreservation/);
  }

  assert.doesNotMatch(profiles, /codexOAuthConflict/);
});
