import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("Linux detection and UI do not expose desktop client panels", () => {
  const rustTypes = read("src-tauri/src/core/types.rs");
  const detector = read("src-tauri/src/core/detector.rs");
  const app = read("src/App.svelte");
  const dashboard = read("src/routes/Dashboard.svelte");

  assert.match(rustTypes, /pub platform: String/);
  assert.match(detector, /platform: current_platform_label\(\)/);
 assert.match(detector, /if supports_codex_desktop_client\(\)/);
 assert.match(detector, /fn supports_codex_desktop_client_for_platform\(platform: &str\) -> bool/);
 assert.match(detector, /linux_platform_does_not_track_codex_desktop_client/);
  assert.match(detector, /fn supports_claude_desktop_client_for_platform\(platform: &str\) -> bool/);
  assert.match(detector, /linux_platform_does_not_track_claude_desktop_client/);
  assert.match(detector, /tool\.id != "claude-desktop" \|\| supports_claude_desktop_client\(\)/);
 assert.match(app, /desktopClientPagesAvailable/);
  assert.match(app, /desktopClientPagesAvailable = \["windows", "macos"\]\.includes\(snapshot\?\.platform \?\? ""\)/);
  assert.match(app, /!\["codexClient", "claudeDesktop"\]\.includes\(item\.id\) \|\| desktopClientPagesAvailable/);
  assert.match(app, /\["codexClient", "claudeDesktop"\]\.includes\(route\) && !desktopClientPagesAvailable/);
  assert.match(dashboard, /tool\.id === "codex-app"/);
});

test("Linux install routes use native shell-friendly installers", () => {
  const registry = read("src-tauri/src/core/tool_registry.rs");
  const installer = read("src-tauri/src/core/tool_installer.rs");
  const detector = read("src-tauri/src/core/detector.rs");

  assert.match(registry, /Some\("curl -fsSL https:\/\/hermes-agent\.nousresearch\.com\/install\.sh \| bash"\)/);
  assert.match(registry, /Some\("curl -fsSL https:\/\/deb\.nodesource\.com\/setup_lts\.x \| sudo -E bash - && sudo apt-get install -y nodejs"\)/);
  assert.match(installer, /InstallAction::ShellScript/);
  assert.match(installer, /"Hermes official install script"/);
  assert.match(installer, /"curl -fsSL https:\/\/hermes-agent\.nousresearch\.com\/install\.sh \| bash"/);
  assert.match(detector, /"hermes" if cfg!\(target_os = "linux"\)/);
  assert.match(detector, /"bash -lc 'curl -fsSL https:\/\/hermes-agent\.nousresearch\.com\/install\.sh \| bash'"/);
});
