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
  assert.match(detector, /if supports_chatgpt_desktop\(\)/);
  assert.match(detector, /fn supports_chatgpt_desktop_for_platform\(platform: &str\) -> bool/);
  assert.match(detector, /linux_platform_does_not_track_chatgpt_desktop/);
  assert.match(detector, /fn supports_claude_desktop_client_for_platform\(platform: &str\) -> bool/);
  assert.match(detector, /linux_platform_does_not_track_claude_desktop_client/);
  assert.match(detector, /tool\.id != "claude-desktop" \|\| supports_claude_desktop_client\(\)/);
  assert.match(app, /desktopClientPagesAvailable/);
  assert.match(app, /desktopClientPagesAvailable = \["windows", "macos"\]\.includes\(snapshot\?\.platform \?\? ""\)/);
  assert.match(app, /!\["chatgptDesktop", "claudeDesktop"\]\.includes\(item\.id\) \|\| desktopClientPagesAvailable/);
  assert.match(app, /function desktopClientRouteAllowed\(currentRoute: Route\)/);
  assert.match(app, /\["chatgptDesktop", "claudeDesktop"\]\.includes\(route\) && !desktopClientRouteAllowed\(route\)/);
  assert.match(dashboard, /isChatGPTDesktopToolId\(tool\.id\)/);
});

test("Linux install routes use native shell-friendly installers", () => {
  const registry = read("src-tauri/src/core/tool_registry.rs");
  const installer = read("src-tauri/src/core/tool_installer.rs");
  const detector = read("src-tauri/src/core/detector.rs");

  assert.match(registry, /const HERMES_UNIX_INSTALL_COMMAND: &str =\s*"curl -fsSL https:\/\/hermes-agent\.nousresearch\.com\/install\.sh \| bash"/);
  assert.match(registry, /Some\(HERMES_UNIX_INSTALL_COMMAND\)/);
  assert.match(registry, /Some\("curl -fsSL https:\/\/deb\.nodesource\.com\/setup_lts\.x \| sudo -E bash - && sudo apt-get install -y nodejs"\)/);
  assert.match(installer, /InstallAction::ShellScript/);
  assert.match(installer, /"Hermes official install script"/);
  assert.match(installer, /"curl -fsSL https:\/\/hermes-agent\.nousresearch\.com\/install\.sh \| bash"/);
  assert.match(detector, /"hermes" => Some\("hermes update"\.to_string\(\)\)/);
});
