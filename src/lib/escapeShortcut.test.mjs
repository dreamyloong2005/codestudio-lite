import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const profiles = fs.readFileSync("src/routes/Profiles.svelte", "utf8");
const dashboard = fs.readFileSync("src/routes/Dashboard.svelte", "utf8");
const terminal = fs.readFileSync("src/routes/TerminalPanel.svelte", "utf8");

test("small-window close and back actions support Escape", () => {
  assert.match(profiles, /<svelte:window on:keydown=\{handleModalEscape\}/);
  assert.match(dashboard, /<svelte:window on:keydown=\{handleDialogEscape\}/);
  assert.match(dashboard, /void closeInstallPlan\(\)/);
  assert.match(dashboard, /void closeToolLaunch\(\)/);
  assert.match(terminal, /<svelte:window on:keydown=\{handleEscape\}/);
  assert.match(terminal, /handleBack\(\)/);
});

test("small-window primary actions support Enter without stealing field input", () => {
  assert.match(profiles, /<svelte:window[^>]*handleModalEnter/);
  assert.match(profiles, /void handleUsageSave\(\)/);
  assert.match(profiles, /void handleEditSave\(\)/);
  assert.match(profiles, /void handleDeleteConfirm\(\)/);
  assert.match(profiles, /void handleApplyWithOptions\(pendingApply\.id\)/);
  assert.match(dashboard, /<svelte:window[^>]*handleDialogEnter/);
  assert.match(dashboard, /void confirmInstallAction\(\)/);
  assert.match(dashboard, /void startToolLaunch\(\)/);
  assert.match(profiles, /\["INPUT", "TEXTAREA", "SELECT", "BUTTON"\]/);
  assert.match(dashboard, /\["INPUT", "TEXTAREA", "SELECT", "BUTTON"\]/);
});
