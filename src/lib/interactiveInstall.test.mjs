import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("Hermes installs through an embedded interactive terminal", () => {
  const cargo = read("src-tauri/Cargo.toml");
  const packageJson = read("package.json");
  const types = read("src/types.ts");
  const api = read("src/lib/api.ts");
  const dashboard = read("src/routes/Dashboard.svelte");
  const toolInstaller = read("src-tauri/src/core/tool_installer.rs");
  const terminalCommand = read("src-tauri/src/commands/install_terminal.rs");
  const lib = read("src-tauri/src/lib.rs");

  assert.match(cargo, /portable-pty/);
  assert.match(packageJson, /"@xterm\/xterm"/);
  assert.match(types, /interactive:\s*boolean/);
  assert.match(types, /interface InstallTerminalOutput/);
  assert.match(api, /startInstallTerminal/);
  assert.match(api, /writeInstallTerminal/);
  assert.match(api, /listenInstallTerminalOutput/);
  assert.match(dashboard, /Terminal/);
  assert.match(dashboard, /openInteractiveInstall/);
  assert.match(dashboard, /terminal\.write/);
  assert.match(dashboard, /terminal\.dispose/);
  assert.match(toolInstaller, /InstallAction::InteractiveShellScript/);
  assert.match(toolInstaller, /interactive:\s*action_interactive/);
  assert.match(terminalCommand, /portable_pty::native_pty_system/);
  assert.match(terminalCommand, /install-terminal:\/\/output/);
  assert.match(lib, /commands::install_terminal::start_install_terminal/);
  assert.doesNotMatch(api, /because the target installer may ask for input/);
  assert.doesNotMatch(dashboard, /因为目标安装器可能需要输入/);
  assert.doesNotMatch(toolInstaller, /because the target installer may ask for input/);
});
