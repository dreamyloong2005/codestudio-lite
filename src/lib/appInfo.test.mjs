import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("app version is injected from package metadata", () => {
  const appInfo = read("src/lib/appInfo.ts");
  const viteConfig = read("vite.config.ts");

  assert.match(viteConfig, /__APP_VERSION__:\s*JSON\.stringify\(packageJson\.version\)/);
  assert.match(viteConfig, /__APP_UPDATER_ENABLED__:\s*JSON\.stringify\(updaterEnabled\)/);
  assert.match(appInfo, /APP_VERSION\s*=\s*__APP_VERSION__/);
  assert.match(appInfo, /APP_UPDATER_ENABLED\s*=\s*__APP_UPDATER_ENABLED__/);
  assert.match(appInfo, /APP_VERSION_LABEL\s*=\s*`v\$\{APP_VERSION\}`/);
  assert.doesNotMatch(appInfo, /APP_VERSION\s*=\s*"1\.0\.0"/);
  assert.doesNotMatch(appInfo, /APP_VERSION_LABEL\s*=\s*"v1\.0\.0"/);
});

test("release manifests share the global application version", () => {
  const packageJson = JSON.parse(read("package.json"));
  const packageLock = JSON.parse(read("package-lock.json"));
  const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));
  const cargoToml = read("src-tauri/Cargo.toml");
  const cargoLock = read("src-tauri/Cargo.lock");
  const cargoManifestVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const cargoLockVersion = cargoLock.match(/\[\[package\]\]\s+name = "codestudio-lite"\s+version = "([^"]+)"/)?.[1];

  assert.equal(packageJson.version, "1.5.1");
  assert.equal(packageLock.version, packageJson.version);
  assert.equal(packageLock.packages[""].version, packageJson.version);
  assert.equal(tauriConfig.version, packageJson.version);
  assert.equal(cargoManifestVersion, packageJson.version);
  assert.equal(cargoLockVersion, packageJson.version);
});
