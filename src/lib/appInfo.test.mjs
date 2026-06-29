import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("app version is injected from package metadata", () => {
  const appInfo = read("src/lib/appInfo.ts");
  const viteConfig = read("vite.config.ts");

  assert.match(viteConfig, /__APP_VERSION__:\s*JSON\.stringify\(packageJson\.version\)/);
  assert.match(appInfo, /APP_VERSION\s*=\s*__APP_VERSION__/);
  assert.match(appInfo, /APP_VERSION_LABEL\s*=\s*`v\$\{APP_VERSION\}`/);
  assert.doesNotMatch(appInfo, /APP_VERSION\s*=\s*"1\.0\.0"/);
  assert.doesNotMatch(appInfo, /APP_VERSION_LABEL\s*=\s*"v1\.0\.0"/);
});
