import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("Codex Computer Use Guard is exposed as a launch option", () => {
  const types = read("src/types.ts");
  const store = read("src/lib/chatgptDesktopStore.ts");
  const api = read("src/lib/api.ts");
  const route = read("src/routes/ChatGPTDesktop.svelte");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  assert.match(types, /computerUseGuardOnLaunch: boolean/);
  assert.match(types, /computerUseGuardOnLaunch\?: boolean \| null/);
  assert.match(store, /computerUseGuardOnLaunch: false/);
  assert.match(store, /computerUseGuardOnLaunch:\s*settings\.computerUseGuardOnLaunch/);
  assert.match(store, /draft\.computerUseGuardOnLaunch/);
  assert.match(api, /computerUseGuardOnLaunch: request\.computerUseGuardOnLaunch \?\?/);
  assert.match(api, /computerUseGuardOnLaunch: false/);
  assert.match(route, /checked=\{settingsDraft\.computerUseGuardOnLaunch\}/);
  assert.match(route, /updateChatGPTDesktopDraft\(\{ computerUseGuardOnLaunch: event\.currentTarget\.checked \}\)/);
  assert.match(route, /chatgptDesktop\.computerUseGuardOnLaunch/);
  assert.match(route, /chatgptDesktop\.computerUseGuardOnLaunchHint/);

  for (const dictionary of [zhCN, zhTW, enUS]) {
    assert.match(dictionary, /"chatgptDesktop\.computerUseGuardOnLaunch"/);
    assert.match(dictionary, /"chatgptDesktop\.computerUseGuardOnLaunchHint"/);
  }
});

test("Codex launch runs Computer Use Guard before and after starting the app", () => {
  const coreMod = read("src-tauri/src/core/mod.rs");
  const core = read("src-tauri/src/core/chatgpt_desktop.rs");
  const launchBody = core
    .split("pub fn launch() -> Result<(), String> {")
    .at(1)
    ?.split("pub fn restart()")
    .at(0);

  assert.ok(launchBody, "Codex launch body should be present");
  assert.match(coreMod, /pub mod computer_use_guard/);
  assert.match(core, /pub computer_use_guard_on_launch: bool/);
  assert.match(core, /pub computer_use_guard_on_launch: Option<bool>/);
  assert.match(core, /computer_use_guard_on_launch: false/);
  assert.match(core, /if let Some\(enabled\) = request\.computer_use_guard_on_launch/);
  assert.match(core, /settings\.computer_use_guard_on_launch = enabled/);
  assert.match(launchBody, /ensure_computer_use_guard_if_enabled\(&settings\)\?/);
  assert.match(launchBody, /launch_installed_codex\(&installed, &args\)\?/);
  assert.match(launchBody, /start_computer_use_guard_watchdog_if_enabled\(&settings\)/);
  assert.ok(
    launchBody.indexOf("ensure_computer_use_guard_if_enabled(&settings)?")
      < launchBody.indexOf("launch_installed_codex(&installed, &args)?"),
    "guard should repair config before Codex starts"
  );
  assert.ok(
    launchBody.indexOf("launch_installed_codex(&installed, &args)?")
      < launchBody.indexOf("start_computer_use_guard_watchdog_if_enabled(&settings)"),
    "watchdog should start after Codex starts"
  );
});
