import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("tool install dialog exposes command output for each stage", () => {
  const dashboard = read("src/routes/Dashboard.svelte");
  const en = read("src/lib/locales/en-US.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");

  assert.match(dashboard, /toolInstall\.consoleOutput/);
  assert.match(dashboard, /listenToolInstallProgress/);
  assert.match(dashboard, /installProgressLogs/);
  assert.match(dashboard, /bind:this=\{installLogViewport\}/);
  assert.match(dashboard, /stage\.stdoutTail/);
  assert.match(dashboard, /stage\.stderrTail/);
  assert.match(dashboard, /installResult = result/);
  assert.match(read("src/types.ts"), /stdoutTail: string/);
  assert.match(read("src/types.ts"), /stderrTail: string/);
  assert.match(read("src/lib/api.ts"), /tool-install:\/\/progress/);
  assert.match(read("src/types.ts"), /interface ToolInstallProgress/);
  assert.match(read("src/types.ts"), /rootToolId: string/);
  assert.match(en, /"toolInstall\.consoleOutput"/);
  assert.match(zhCN, /"toolInstall\.consoleOutput"/);
  assert.match(zhTW, /"toolInstall\.consoleOutput"/);
});

test("tool install dialog does not show advisory install warnings", () => {
  const dashboard = read("src/routes/Dashboard.svelte");
  const api = read("src/lib/api.ts");
  const installer = read("src-tauri/src/core/tool_installer.rs");

  assert.doesNotMatch(dashboard, /installWarningKeys/);
  assert.doesNotMatch(dashboard, /installWarningLabel/);
  assert.doesNotMatch(dashboard, /installPlan\.warnings/);
  assert.doesNotMatch(dashboard, /installResult\.notes/);

  for (const source of [api, installer]) {
    assert.doesNotMatch(source, /Global npm installs write/);
    assert.doesNotMatch(source, /Some winget packages may trigger/);
    assert.doesNotMatch(source, /Homebrew installs into/);
    assert.doesNotMatch(source, /This plan opens an embedded terminal/);
    assert.doesNotMatch(source, /official PowerShell install script/);
    assert.doesNotMatch(source, /VS Code profile; VS Code may need/);
  }
});

test("tool updates require the same confirmation dialog as installs", () => {
  const dashboard = read("src/routes/Dashboard.svelte");

  assert.match(dashboard, /let installMode:\s*"install"\s*\|\s*"update"\s*=/);
  assert.match(dashboard, /async function openToolActionPlan\(tool: ToolStatus,\s*mode:\s*"install"\s*\|\s*"update"\)/);
  assert.doesNotMatch(dashboard, /on:click=\{\(\) => confirmUpdate\(tool\)\}/);
  assert.match(dashboard, /on:click=\{\(\) => openToolActionPlan\(tool,\s*"update"\)\}/);
  assert.match(dashboard, /installMode === "update"\s*\?\s*planToolUpdate\(planTool\.id\)\s*:\s*planToolInstall\(planTool\.id\)/);
  assert.match(dashboard, /installMode === "update"\s*\?\s*updateTool/);
  assert.match(dashboard, /installMode === "update"\s*\?\s*"toolInstall\.confirmUpdate"/);

  const api = read("src/lib/api.ts");
  assert.match(api, /export async function planToolUpdate\(toolId: string\): Promise<ToolInstallPlan>/);
  assert.match(api, /invoke\("plan_tool_update", \{ toolId \}\)/);
  assert.match(api, /function mockToolUpdatePlan\(toolId: string\): ToolInstallPlan/);

  assert.match(read("src-tauri/src/commands/tool_installer.rs"), /pub async fn plan_tool_update/);
  assert.match(read("src-tauri/src/lib.rs"), /commands::tool_installer::plan_tool_update/);
  assert.match(read("src-tauri/src/core/tool_installer.rs"), /pub fn plan_tool_update\(tool_id: &str\) -> Result<ToolInstallPlan, String>/);
  assert.match(read("src-tauri/src/core/tool_installer.rs"), /fn build_update_plan\(/);

  for (const dictionary of [
    read("src/lib/locales/en-US.ts"),
    read("src/lib/locales/zh-CN.ts"),
    read("src/lib/locales/zh-TW.ts")
  ]) {
    assert.match(dictionary, /"toolInstall\.confirmUpdate"/);
    assert.match(dictionary, /"toolInstall\.updateTitle"/);
  }
});

test("npm install opens the Node.js install plan", () => {
  const dashboard = read("src/routes/Dashboard.svelte");
  const installer = read("src-tauri/src/core/tool_installer.rs");
  const api = read("src/lib/api.ts");

  assert.match(dashboard, /function installPlanToolFor\(tool: ToolStatus\)/);
  assert.match(dashboard, /tool\.id !== "npm"/);
  assert.match(dashboard, /candidate\.id === "node"/);
  assert.match(dashboard, /planToolInstall\(planTool\.id\)/);
  assert.match(installer, /"npm" => InstallAction::ProvidedByTool\("node"\)/);
  assert.match(installer, /provider_command_entry\(definition, &provider_definition\)/);
  assert.match(api, /npm: \{\s*toolName: "npm",\s*manager: "winget",\s*command: "winget install --id OpenJS\.NodeJS\.LTS/s);
});

test("tool update dialog does not render backend English plan tips", () => {
  const dashboard = read("src/routes/Dashboard.svelte");

  assert.match(dashboard, /installMode !== "update" && installPlan\.steps\.length > 0/);
  assert.match(dashboard, /\{#each installPlan\.steps as step\}/);
});

test("tool install and update results are localized in the dashboard", () => {
  const dashboard = read("src/routes/Dashboard.svelte");

  assert.match(dashboard, /function localizedInstallResultMessage\(result: ToolInstallResult\)/);
  assert.match(dashboard, /function localizedInstallStageMessage\(stage: ToolInstallResult\["stageResults"\]\[number\]\)/);
  assert.doesNotMatch(dashboard, /\{installResult\.message\}/);
  assert.doesNotMatch(dashboard, /\{stage\.message\}/);

  for (const dictionary of [
    read("src/lib/locales/en-US.ts"),
    read("src/lib/locales/zh-CN.ts"),
    read("src/lib/locales/zh-TW.ts")
  ]) {
    assert.match(dictionary, /"toolInstall\.result\.installSuccess"/);
    assert.match(dictionary, /"toolInstall\.result\.updateSuccess"/);
    assert.match(dictionary, /"toolInstall\.result\.installVerificationFailed"/);
    assert.match(dictionary, /"toolInstall\.result\.updateVerificationFailed"/);
    assert.match(dictionary, /"toolInstall\.result\.prerequisiteSuccess"/);
    assert.match(dictionary, /"toolInstall\.result\.prerequisitesRequired"/);
  }
});

test("tool console output does not repeat commands or duplicate result tails", () => {
  const dashboard = read("src/routes/Dashboard.svelte");

  assert.doesNotMatch(dashboard, /<code>\{group\.command\}<\/code>/);
  assert.doesNotMatch(dashboard, /<code>\{stage\.command\}<\/code>\s*\{#if stage\.stdoutTail/);
  assert.doesNotMatch(dashboard, /\{#if installResult\.stdoutTail\}/);
  assert.doesNotMatch(dashboard, /\{#if installResult\.stderrTail\}/);
  assert.match(dashboard, /return result\.stageResults\.some\(\(stage\) => stage\.stdoutTail \|\| stage\.stderrTail\);/);
  assert.match(dashboard, /installProgressLogKeys/);
  assert.match(dashboard, /installProgressLogKeys\.has\(key\)/);
  assert.match(dashboard, /installProgressLogKeys\.add\(key\)/);

  const api = read("src/lib/api.ts");
  assert.doesNotMatch(api, /`> \$\{command\}\\r\\n`/);
  assert.doesNotMatch(api, /Hermes installer may ask for confirmation/);
  assert.doesNotMatch(api, /Type responses directly in this terminal/);
});

test("embedded terminal uses FitAddon and only syncs applied xterm dimensions", () => {
  const packageJson = read("package.json");
  const pandaConfig = read("panda.config.ts");
  const terminalPanel = read("src/routes/TerminalPanel.svelte");
  const terminalStore = read("src/lib/terminalSessionStore.ts");

  assert.match(packageJson, /"@xterm\/addon-fit": "\^0\.10\.0"/);
  assert.match(terminalPanel, /import \{ FitAddon \} from "@xterm\/addon-fit";/);
  assert.match(terminalPanel, /term\.loadAddon\(fitAddon\);/);
  assert.match(terminalPanel, /terminalResizeDisposable = term\.onResize\(\(\{ cols, rows \}\) => \{/);
  assert.match(terminalPanel, /resizeSession\(cols, rows\);/);
  assert.match(terminalPanel, /entry\.contentRect\.width/);
  assert.match(terminalPanel, /entry\.contentRect\.height/);
  assert.doesNotMatch(terminalPanel, /_core|_renderService|cellWidth|cellHeight/);
  assert.doesNotMatch(terminalPanel, /BACKEND_RESIZE_SETTLE_MS|pendingBackendResize/);
  assert.match(
    pandaConfig,
    /appRouteTransitionRecipe:\s*\{[\s\S]*?base:\s*\{\s*width: "100%",\s*minWidth: 0,\s*minHeight: 0,\s*height: "100%"/
  );
  assert.match(
    pandaConfig,
    /terminalPanelRecipe:\s*\{[\s\S]*?base:\s*\{\s*display: "grid",\s*gridTemplateRows: "auto minmax\(0, 1fr\)",[\s\S]*?height: "100%",\s*overflow: "hidden"/
  );

  assert.match(terminalStore, /const TERMINAL_RESIZE_FLUSH_DELAY_MS = 80;/);
  assert.match(terminalStore, /let pendingResize:/);
  assert.match(terminalStore, /function schedulePendingResize\(\): void/);
  assert.match(terminalStore, /const resizeKey = `\$\{activeSessionId\}:\$\{cols\}x\$\{rows\}`;/);
  assert.match(terminalStore, /if \(resizeKey === lastResizeKey\) return;/);
});
