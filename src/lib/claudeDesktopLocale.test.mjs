import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const localeFiles = [
  "src-tauri/resources/claude-desktop/i18n/zh-CN.json",
  "src-tauri/resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json",
];

const readJson = (path) => JSON.parse(readFileSync(new URL(`../../${path}`, import.meta.url), "utf8"));
const stripUrls = (value) => value.replace(/[a-z][a-z0-9+.-]*:\/\/\S+/gi, "");
const badArtifactTerms = /(?<!复)制品|产物|神器|伪影|\bArtifacts?\b|\bartifacts?\b/;

test('Claude Desktop zh-CN locale translates "Artifact" as "工件"', () => {
  const failures = [];

  for (const file of localeFiles) {
    const locale = readJson(file);
    for (const [key, value] of Object.entries(locale)) {
      if (typeof value !== "string") continue;
      if (badArtifactTerms.test(stripUrls(value))) {
        failures.push(`${file}:${key}: ${value}`);
      }
    }
  }

  assert.deepEqual(failures, []);
});

test('Claude Desktop zh-CN locale uses "凭据" for provider credentials', () => {
  const ion = readJson("src-tauri/resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json");

  assert.equal(ion["w4MKEU2/Va"], "{provider} 凭据");
});
