import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { macosArtifactPaths, targetMetadata } from "../../scripts/macos-artifact-contract.mjs";

const rootDir = fileURLToPath(new URL("../..", import.meta.url));

const readJson = (path) =>
  JSON.parse(readFileSync(new URL(`../../${path}`, import.meta.url), "utf8").replace(/^\uFEFF/, ""));

test("Panda CSS release pipeline uses the maintained 1.x dev dependency", () => {
  const packageJson = readJson("package.json");

  assert.equal(packageJson.devDependencies?.["@pandacss/dev"], "^1.12.0");
  assert.notEqual(packageJson.devDependencies?.["@pandacss/dev"], "^0.31.0");
  assert.equal(packageJson.devDependencies?.postcss, "^8.5.18");
  assert.equal(packageJson.overrides?.postcss, "$postcss");
});

test("macOS target metadata maps Rust targets to release labels", () => {
  assert.deepEqual(targetMetadata("aarch64-apple-darwin"), {
    architecture: "arm64",
    tauriArchitecture: "aarch64",
    executableArchitecture: "arm64"
  });
  assert.deepEqual(targetMetadata("x86_64-apple-darwin"), {
    architecture: "x64",
    tauriArchitecture: "x64",
    executableArchitecture: "x86_64"
  });
});

test("macOS artifact paths use exact raw and canonical names", () => {
  const paths = macosArtifactPaths("/tmp/bundle", "1.5.2", "aarch64-apple-darwin");

  assert.equal(paths.raw.dmg, "/tmp/bundle/dmg/CodeStudio Lite_1.5.2_aarch64.dmg");
  assert.equal(paths.canonical.dmg, "/tmp/bundle/dmg/CodeStudio-Lite-1.5.2-macOS-arm64.dmg");
  assert.equal(
    paths.canonical.archiveSignature,
    "/tmp/bundle/macos/CodeStudio-Lite-1.5.2-macOS-arm64.app.tar.gz.sig"
  );
});

test("unsupported macOS targets fail before creating paths", () => {
  assert.throws(() => targetMetadata("darwin-universal"), /Unsupported macOS target/);
});

test("updater config can disable the repeated frontend build hook", () => {
  const tempDir = mkdtempSync(join(tmpdir(), "codestudio-updater-config-"));
  try {
    const outputPath = join(tempDir, "config.json");
    const result = spawnSync(
      process.execPath,
      ["scripts/prepare-updater-config.mjs", "--output", outputPath, "--skip-before-build"],
      { cwd: rootDir, encoding: "utf8" }
    );

    assert.equal(result.status, 0, result.stderr);
    const config = JSON.parse(readFileSync(outputPath, "utf8"));
    assert.equal(config.build.beforeBuildCommand, "");
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
});

test("updater config forwards an optional Apple signing identity", () => {
  const tempDir = mkdtempSync(join(tmpdir(), "codestudio-apple-config-"));
  try {
    const outputPath = join(tempDir, "config.json");
    const result = spawnSync(
      process.execPath,
      ["scripts/prepare-updater-config.mjs", "--output", outputPath],
      {
        cwd: rootDir,
        encoding: "utf8",
        env: { ...process.env, APPLE_SIGNING_IDENTITY: "Developer ID Application: Example" }
      }
    );

    assert.equal(result.status, 0, result.stderr);
    const config = JSON.parse(readFileSync(outputPath, "utf8"));
    assert.equal(config.bundle.macOS.signingIdentity, "Developer ID Application: Example");
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
});
