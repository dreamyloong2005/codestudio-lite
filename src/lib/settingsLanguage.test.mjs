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
