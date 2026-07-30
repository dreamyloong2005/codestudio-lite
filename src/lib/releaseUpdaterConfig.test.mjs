import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const readJson = (path) =>
  JSON.parse(readFileSync(new URL(`../../${path}`, import.meta.url), "utf8").replace(/^\uFEFF/, ""));

test("the base Tauri release config always initializes the updater plugin", () => {
  const tauriConfig = readJson("src-tauri/tauri.conf.json");
  const updaterConfig = tauriConfig.plugins?.updater;

  assert.equal(typeof updaterConfig, "object");
  assert.ok(Array.isArray(updaterConfig.endpoints));
  assert.ok(updaterConfig.endpoints.length > 0);
  assert.ok(updaterConfig.endpoints.every((endpoint) => endpoint.startsWith("https://")));
  assert.equal(typeof updaterConfig.pubkey, "string");
  assert.ok(updaterConfig.pubkey.length > 0);
});
