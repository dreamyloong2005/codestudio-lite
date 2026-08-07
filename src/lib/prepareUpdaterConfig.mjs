import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const rootDir = resolve(import.meta.dirname, "../..");
const generatedConfigPath = resolve(rootDir, "src-tauri/tauri.updater.generated.conf.json");

const result = spawnSync(process.execPath, ["scripts/prepare-updater-config.mjs"], {
  cwd: rootDir,
  stdio: "inherit",
  env: process.env
});
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const config = JSON.parse(readFileSync(generatedConfigPath, "utf8"));
const updater = config.plugins?.updater;
if (!updater || !Array.isArray(updater.endpoints)) {
  throw new Error("Generated updater configuration has no endpoints.");
}
updater.endpoints = updater.endpoints.map((endpoint) =>
  endpoint.includes("?")
    ? `${endpoint}&r={{current_version}}`
    : `${endpoint}?r={{current_version}}`
);
writeFileSync(generatedConfigPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");
console.log("Updater endpoints configured with a per-check cache-busting query.");
