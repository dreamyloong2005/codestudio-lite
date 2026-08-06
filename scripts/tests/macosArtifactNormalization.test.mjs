import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

import { macosArtifactPaths } from "../macos-artifact-contract.mjs";
import { normalizeMacosArtifacts } from "../normalize-macos-artifacts.mjs";

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "codestudio-macos-artifacts-"));
  const bundleRoot = join(root, "bundle");
  mkdirSync(join(bundleRoot, "dmg"), { recursive: true });
  mkdirSync(join(bundleRoot, "macos"), { recursive: true });
  return { root, bundleRoot };
}

test("normalization moves only exact current-version files and preserves stale files", () => {
  const { root, bundleRoot } = fixture();
  try {
    const paths = macosArtifactPaths(bundleRoot, "1.5.2", "aarch64-apple-darwin");
    const stale = join(bundleRoot, "dmg", "CodeStudio Lite_1.5.0_aarch64.dmg");
    for (const path of [paths.raw.dmg, paths.raw.archive, paths.raw.archiveSignature, stale]) {
      writeFileSync(path, path);
    }

    normalizeMacosArtifacts({ bundleRoot, version: "1.5.2", target: "aarch64-apple-darwin" });

    assert.equal(existsSync(paths.raw.dmg), false);
    assert.equal(existsSync(paths.raw.archive), false);
    assert.equal(existsSync(paths.raw.archiveSignature), false);
    assert.equal(existsSync(paths.canonical.dmg), true);
    assert.equal(existsSync(paths.canonical.archive), true);
    assert.equal(existsSync(paths.canonical.archiveSignature), true);
    assert.equal(existsSync(stale), true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("normalization rejects a missing current updater signature", () => {
  const { root, bundleRoot } = fixture();
  try {
    const paths = macosArtifactPaths(bundleRoot, "1.5.2", "x86_64-apple-darwin");
    writeFileSync(paths.raw.dmg, "dmg");
    writeFileSync(paths.raw.archive, "archive");

    assert.throws(
      () => normalizeMacosArtifacts({ bundleRoot, version: "1.5.2", target: "x86_64-apple-darwin" }),
      /Missing raw updater archive signature/
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("dual-target release builds frontend assets once and delegates per-target packaging", () => {
  const dualTargetScript = readFileSync(new URL("../build-macos-updater.sh", import.meta.url), "utf8");
  const targetScript = readFileSync(new URL("../build-macos-updater-target.sh", import.meta.url), "utf8");

  assert.equal((dualTargetScript.match(/npm run build/g) ?? []).length, 1);
  assert.match(dualTargetScript, /build-macos-updater-target\.sh/);
  assert.match(dualTargetScript, /--frontend-ready/);
  assert.match(targetScript, /prepare-updater-config\.mjs --skip-before-build/);
  assert.match(targetScript, /verify-macos-artifacts\.sh/);
});
