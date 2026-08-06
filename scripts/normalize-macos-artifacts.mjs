import { existsSync, renameSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { macosArtifactPaths } from "./macos-artifact-contract.mjs";

export function normalizeMacosArtifacts({ bundleRoot, version, target }) {
  const paths = macosArtifactPaths(bundleRoot, version, target);

  requireAbsent(paths.canonical.dmg, "canonical DMG");
  requireAbsent(paths.canonical.archive, "canonical updater archive");
  requireAbsent(paths.canonical.archiveSignature, "canonical updater archive signature");
  requireFile(paths.raw.dmg, "raw DMG");
  requireFile(paths.raw.archive, "raw updater archive");
  requireFile(paths.raw.archiveSignature, "raw updater archive signature");

  renameSync(paths.raw.dmg, paths.canonical.dmg);
  renameSync(paths.raw.archive, paths.canonical.archive);
  renameSync(paths.raw.archiveSignature, paths.canonical.archiveSignature);

  return paths;
}

function requireFile(path, label) {
  if (!existsSync(path)) {
    throw new Error(`Missing ${label}: ${path}`);
  }
}

function requireAbsent(path, label) {
  if (existsSync(path)) {
    throw new Error(`${label} already exists; remove only the current-version artifact before rebuilding: ${path}`);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [target, version, bundleRoot] = process.argv.slice(2);
  if (!target || !version || !bundleRoot) {
    throw new Error("Usage: normalize-macos-artifacts.mjs <target> <version> <bundle-root>");
  }
  normalizeMacosArtifacts({ target, version, bundleRoot });
  console.log(`macOS release artifacts normalized with base name: ${macosArtifactPaths(bundleRoot, version, target).canonicalBase}`);
}
