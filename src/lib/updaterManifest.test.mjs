import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("updater manifest maps each macOS architecture to its own DMG", () => {
  const root = mkdtempSync(join(tmpdir(), "codestudio-updater-"));
  try {
    const windows = join(root, "CodeStudio-Lite-1.4.2-Windows-x64-setup.exe");
    const macosArm64 = join(root, "CodeStudio-Lite-1.4.2-macOS-arm64.dmg");
    const macosX64 = join(root, "CodeStudio-Lite-1.4.2-macOS-x64.dmg");
    const linux = join(root, "CodeStudio-Lite-1.4.2-Linux-x64.AppImage");
    const output = join(root, "latest.json");
    writeFileSync(windows, "windows");
    writeFileSync(`${windows}.sig`, "windows-signature\n");
    writeFileSync(macosArm64, "macos-arm64");
    writeFileSync(`${macosArm64}.sig`, "macos-arm64-signature\n");
    writeFileSync(macosX64, "macos-x64");
    writeFileSync(`${macosX64}.sig`, "macos-x64-signature\n");
    writeFileSync(linux, "linux");
    writeFileSync(`${linux}.sig`, "linux-signature\n");

    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../../scripts/generate-update-manifest.mjs", import.meta.url)),
        "--version", "1.4.2",
        "--base-url", "https://download.codestudio.build/",
        "--pub-date", "2026-07-14T12:00:00Z",
        "--windows-installer", windows,
        "--macos-arm64-dmg", macosArm64,
        "--macos-x64-dmg", macosX64,
        "--linux-appimage", linux,
        "--linux-platform", "linux-x86_64",
        "--output", output
      ],
      { encoding: "utf8" }
    );

    assert.equal(result.status, 0, result.stderr);
    const manifest = JSON.parse(readFileSync(output, "utf8"));
    assert.equal(manifest.version, "1.4.2");
    assert.equal(manifest.platforms["windows-x86_64"].signature, "windows-signature");
    assert.equal(manifest.platforms["windows-x86_64"].url, "https://download.codestudio.build/releases/1.4.2/CodeStudio-Lite-1.4.2-Windows-x64-setup.exe");
    const windowsFilename = new URL(manifest.platforms["windows-x86_64"].url).pathname.split("/").at(-1);
    assert.doesNotMatch(windowsFilename, /%20|_/);
    assert.notDeepEqual(manifest.platforms["darwin-aarch64"], manifest.platforms["darwin-x86_64"]);
    assert.equal(manifest.platforms["darwin-aarch64"].signature, "macos-arm64-signature");
    assert.equal(manifest.platforms["darwin-aarch64"].url, "https://download.codestudio.build/releases/1.4.2/CodeStudio-Lite-1.4.2-macOS-arm64.dmg");
    assert.equal(manifest.platforms["darwin-x86_64"].signature, "macos-x64-signature");
    assert.equal(manifest.platforms["darwin-x86_64"].url, "https://download.codestudio.build/releases/1.4.2/CodeStudio-Lite-1.4.2-macOS-x64.dmg");
    assert.equal(manifest.platforms["linux-x86_64"].signature, "linux-signature");
    assert.equal(manifest.platforms["linux-x86_64"].url, "https://download.codestudio.build/releases/1.4.2/CodeStudio-Lite-1.4.2-Linux-x64.AppImage");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("updater manifest requires both macOS architecture packages", () => {
  const root = mkdtempSync(join(tmpdir(), "codestudio-updater-macos-pair-"));
  try {
    const macosArm64 = join(root, "CodeStudio-Lite-1.4.2-macOS-arm64.dmg");
    writeFileSync(macosArm64, "macos-arm64");
    writeFileSync(`${macosArm64}.sig`, "signature\n");
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../../scripts/generate-update-manifest.mjs", import.meta.url)),
        "--version", "1.4.2",
        "--base-url", "https://download.codestudio.build",
        "--macos-arm64-dmg", macosArm64,
        "--output", join(root, "latest.json")
      ],
      { encoding: "utf8" }
    );

    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Provide both --macos-arm64-dmg and --macos-x64-dmg/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("R2 publisher uploads both macOS architecture packages", () => {
  const root = mkdtempSync(join(tmpdir(), "codestudio-publisher-macos-"));
  try {
    const macosArm64 = join(root, "CodeStudio-Lite-1.4.2-macOS-arm64.dmg");
    const macosX64 = join(root, "CodeStudio-Lite-1.4.2-macOS-x64.dmg");
    const manifest = join(root, "latest.json");
    for (const artifact of [macosArm64, macosX64]) {
      writeFileSync(artifact, artifact);
      writeFileSync(`${artifact}.sig`, "signature\n");
    }
    writeFileSync(manifest, "{}\n");
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../../scripts/publish-update-r2.mjs", import.meta.url)),
        "--account-id", "dry-run-account",
        "--bucket", "dry-run-bucket",
        "--version", "1.4.2",
        "--macos-arm64-dmg", macosArm64,
        "--macos-x64-dmg", macosX64,
        "--manifest", manifest,
        "--dry-run"
      ],
      { encoding: "utf8" }
    );

    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /CodeStudio-Lite-1\.4\.2-macOS-arm64\.dmg/);
    assert.match(result.stdout, /CodeStudio-Lite-1\.4\.2-macOS-x64\.dmg/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("R2 publisher stores platform-qualified artifacts directly under the version", () => {
  const root = mkdtempSync(join(tmpdir(), "codestudio-publisher-flat-"));
  try {
    const windows = join(root, "CodeStudio-Lite-1.4.2-Windows-x64-setup.exe");
    const manifest = join(root, "latest.json");
    writeFileSync(windows, "windows");
    writeFileSync(`${windows}.sig`, "signature\n");
    writeFileSync(manifest, "{}\n");
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../../scripts/publish-update-r2.mjs", import.meta.url)),
        "--account-id", "dry-run-account",
        "--bucket", "dry-run-bucket",
        "--version", "1.4.2",
        "--windows-installer", windows,
        "--manifest", manifest,
        "--dry-run"
      ],
      { encoding: "utf8" }
    );

    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /releases\/1\.4\.2\/CodeStudio-Lite-1\.4\.2-Windows-x64-setup\.exe/);
    assert.doesNotMatch(result.stdout, /releases\/1\.4\.2\/windows-x86_64/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("updater manifest rejects a kebab-case artifact with the wrong OS token", () => {
  const root = mkdtempSync(join(tmpdir(), "codestudio-updater-platform-"));
  try {
    const windows = join(root, "CodeStudio-Lite-1.4.2-x64-setup.exe");
    writeFileSync(windows, "windows");
    writeFileSync(`${windows}.sig`, "signature\n");
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../../scripts/generate-update-manifest.mjs", import.meta.url)),
        "--version", "1.4.2",
        "--base-url", "https://download.codestudio.build",
        "--windows-installer", windows,
        "--output", join(root, "latest.json")
      ],
      { encoding: "utf8" }
    );

    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /must include Windows between version and architecture/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("R2 publisher rejects a platform-mismatched artifact before upload", () => {
  const root = mkdtempSync(join(tmpdir(), "codestudio-publisher-platform-"));
  try {
    const windows = join(root, "CodeStudio-Lite-1.4.2-macOS-x64-setup.exe");
    const manifest = join(root, "latest.json");
    writeFileSync(windows, "windows");
    writeFileSync(`${windows}.sig`, "signature\n");
    writeFileSync(manifest, "{}\n");
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../../scripts/publish-update-r2.mjs", import.meta.url)),
        "--account-id", "dry-run-account",
        "--bucket", "dry-run-bucket",
        "--version", "1.4.2",
        "--windows-installer", windows,
        "--manifest", manifest,
        "--dry-run"
      ],
      { encoding: "utf8" }
    );

    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /must include Windows between version and architecture/);
    assert.doesNotMatch(result.stdout, /stable\/latest\.json/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("updater manifest rejects artifact names containing spaces or underscores", () => {
  const root = mkdtempSync(join(tmpdir(), "codestudio-updater-invalid-"));
  try {
    const windows = join(root, "CodeStudio Lite_1.4.2_x64.exe");
    writeFileSync(windows, "windows");
    writeFileSync(`${windows}.sig`, "signature\n");
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../../scripts/generate-update-manifest.mjs", import.meta.url)),
        "--version", "1.4.2",
        "--base-url", "https://download.codestudio.build",
        "--windows-installer", windows,
        "--output", join(root, "latest.json")
      ],
      { encoding: "utf8" }
    );

    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /must use kebab-case/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
