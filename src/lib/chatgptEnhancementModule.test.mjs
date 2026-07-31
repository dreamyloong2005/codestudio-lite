import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(path, "utf8");

test("ChatGPT desktop launch delegates enhancement sequencing to one controller", () => {
  const parent = read("src-tauri/src/core/chatgpt_desktop.rs");
  const controller = read("src-tauri/src/core/chatgpt_desktop/enhancement.rs");

  assert.match(parent, /mod enhancement;/);
  assert.match(parent, /enhancement::launch\(settings, \|args\|\s*launch_installed_codex\(installed, args\)\s*\)/);
  assert.match(controller, /pub\(super\) fn launch/);
  assert.match(controller, /struct EnhancementController/);
});

test("Codex enhancement JavaScript is a dedicated validated resource", () => {
  const controller = read("src-tauri/src/core/chatgpt_desktop/enhancement.rs");
  const script = read("src-tauri/src/core/chatgpt_desktop/codex_enhancements.js");

  assert.match(controller, /include_str!\("codex_enhancements\.js"\)/);
  assert.match(controller, /SETTINGS_PLACEHOLDER/);
  assert.match(controller, /MARKETPLACES_PLACEHOLDER/);
  assert.match(controller, /render_script/);
  assert.match(script, /__CODESTUDIO_LITE_SETTINGS__/);
  assert.match(script, /__CODESTUDIO_LITE_PLUGIN_MARKETPLACES__/);
  assert.match(script, /codestudioLiteCodexEnhancementsVersion/);
});
