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
