import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

test("Antigravity CLI replaces Gemini CLI across the managed tool lifecycle", () => {
  const registry = read("../../src-tauri/src/core/tool_registry.rs");
  const detector = read("../../src-tauri/src/core/detector.rs");
  const installer = read("../../src-tauri/src/core/tool_installer.rs");
  const launcher = read("../../src-tauri/src/core/tool_launch.rs");
  const restart = read("../../src-tauri/src/core/profile/restart.rs");

  assert.match(registry, /id: "antigravity"[\s\S]*name: "Antigravity CLI"[\s\S]*command: "agy"/);
  assert.match(registry, /config_relative_path: Some\("\.gemini\/antigravity-cli"\)/);
  assert.match(installer, /https:\/\/antigravity\.google\/cli\/install\.ps1/);
  assert.match(installer, /https:\/\/antigravity\.google\/cli\/install\.sh/);
  assert.match(launcher, /"antigravity"[\s\S]*"agy"/);

  for (const source of [registry, detector, installer, launcher, restart]) {
    assert.doesNotMatch(source, /@google\/gemini-cli/);
    assert.doesNotMatch(source, /Gemini CLI/);
  }
});

test("Antigravity CLI does not inherit the retired Gemini provider adapter", () => {
  const rustCatalog = read("../../src-tauri/src/core/tool_catalog.rs");
  const webCatalog = read("./profiles/catalog.ts");
  const nativeAdapters = read("../../src-tauri/src/core/profile/native/mod.rs");

  assert.doesNotMatch(rustCatalog, /display_name: "Gemini CLI"/);
  assert.doesNotMatch(rustCatalog, /id: "gemini"/);
  assert.doesNotMatch(webCatalog, /label: "Gemini CLI"/);
  assert.doesNotMatch(webCatalog, /id: "gemini"/);
  assert.doesNotMatch(nativeAdapters, /mod gemini;/);
  assert.equal(
    existsSync(new URL("../../src-tauri/src/core/profile/native/gemini.rs", import.meta.url)),
    false
  );
});

test("Antigravity CLI owns a bundled icon and documentation entry", () => {
  const iconComponent = read("../../src/components/ToolIcon.svelte");
  const readme = read("../../README.md");

  assert.match(iconComponent, /antigravity: \{ src: "\/tool-icons\/antigravity\.(?:svg|png)"/);
  assert.match(readme, /Antigravity CLI/);
  assert.doesNotMatch(readme, /Gemini CLI/);
});
