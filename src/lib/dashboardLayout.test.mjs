import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("dashboard cards keep installed status and three actions on one compact row", () => {
  const css = read("src/styles.css");

  assert.match(css, /\.system-card\s*\{[^}]*grid-template-columns:\s*minmax\(86px,\s*auto\)\s+minmax\(220px,\s*1fr\)/s);
  assert.match(css, /\.system-card-state\s*\{[^}]*min-width:\s*86px/s);
  assert.match(css, /\.system-card-state\s*\.pill\s*\{[^}]*white-space:\s*nowrap/s);
  assert.match(css, /\.client-card-actions\s*\{[^}]*flex-flow:\s*row nowrap/s);
  assert.match(css, /\.client-card-actions\s*\{[^}]*gap:\s*6px/s);
  assert.match(css, /\.client-card-actions\s*\.secondary-button\s*\{[^}]*min-height:\s*30px/s);
  assert.match(css, /\.client-card-actions\s*\.secondary-button\s*\{[^}]*padding:\s*0 8px/s);
  assert.match(css, /\.client-card-actions\s*\.secondary-button\s*\{[^}]*font-size:\s*12px/s);
});

const sliceBetween = (source, startNeedle, endNeedle, label) => {
  const start = source.indexOf(startNeedle);
  assert.notEqual(start, -1, `${label} start marker should exist`);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  assert.notEqual(end, -1, `${label} end marker should exist`);
  return source.slice(start, end);
};

const assertBefore = (source, earlierNeedle, laterNeedle, label) => {
  const earlier = source.indexOf(earlierNeedle);
  const later = source.indexOf(laterNeedle);

  assert.notEqual(earlier, -1, `${label} earlier marker should exist`);
  assert.notEqual(later, -1, `${label} later marker should exist`);
  assert.ok(earlier < later, `${label} should place the update action first`);
};

const actionStartBefore = (source, handler) => {
  const handlerIndex = source.indexOf(handler);
  assert.notEqual(handlerIndex, -1, `${handler} should exist`);
  const start = source.lastIndexOf("<button", handlerIndex);
  assert.notEqual(start, -1, `${handler} should be inside a button`);
  return start;
};

test("dashboard hides launch actions for VS Code plugin tools", () => {
  const svelte = read("src/routes/Dashboard.svelte");

  assert.match(
    svelte,
    /const vscodePluginToolIds = new Set\(\["codex-vscode", "claude-vscode", "gemini-code-assist"\]\);/
  );
  assert.match(svelte, /function canShowToolLaunch\(tool: ToolStatus\)\s*\{\s*return !isVscodePluginTool\(tool\);\s*\}/s);

  const launchHandlers = [...svelte.matchAll(/on:click=\{\(\) => openToolLaunch\(tool\)\}/g)];
  assert.ok(launchHandlers.length >= 1, "dashboard should still expose launch actions for non-plugin tools");

  for (const handler of launchHandlers) {
    const handlerIndex = handler.index ?? -1;
    const buttonStart = svelte.lastIndexOf("<button", handlerIndex);
    const guardStart = svelte.lastIndexOf("{#if canShowToolLaunch(tool)}", handlerIndex);

    assert.notEqual(buttonStart, -1, "launch handler should be inside a button");
    assert.ok(guardStart !== -1 && guardStart < buttonStart, "launch button should be gated by canShowToolLaunch");
  }
});

test("dashboard refresh button follows external detection refresh state", () => {
  const svelte = read("src/routes/Dashboard.svelte");

  assert.match(svelte, /export let refreshingExternally = false/);
  assert.match(svelte, /\$:\s*refreshBusy = refreshing \|\| refreshingExternally/);
  assert.match(svelte, /disabled=\{refreshBusy\}/);
  assert.match(svelte, /name=\{refreshBusy \? "loading" : "refresh"\}/);
  assert.match(svelte, /class=\{refreshBusy \? "spin" : ""\}/);
  assert.match(svelte, /\$t\(refreshBusy \? "common\.refreshing" : "common\.refresh"\)/);
  assert.match(svelte, /onRefresh\(\{ quiet: false, scheduleFollowup: true, showRefreshIndicator: true \}\)/);
});

test("CLI launch panel accepts and forwards a working directory", () => {
  const dashboard = read("src/routes/Dashboard.svelte");
  const types = read("src/types.ts");
  const api = read("src/lib/api.ts");
  const rustTypes = read("src-tauri/src/core/types.rs");
  const installTerminal = read("src-tauri/src/commands/install_terminal.rs");

  assert.match(types, /workingDirectory\?: string \| null/);
  assert.match(dashboard, /let launchWorkingDirectory = ""/);
  assert.match(dashboard, /bind:value=\{launchWorkingDirectory\}/);
  assert.match(dashboard, /\$t\("toolLaunch\.workingDirectory"\)/);
  assert.match(dashboard, /workingDirectory:\s*normalizedLaunchWorkingDirectory\(\)/);
  assert.match(dashboard, /launchWorkingDirectory = ""/);
  assert.match(api, /request\.workingDirectory \? `\[cwd:\$\{request\.workingDirectory\}\]` : null/);
  assert.match(rustTypes, /pub working_directory: Option<String>/);
  assert.match(installTerminal, /if let Some\(working_directory\) = normalized_working_directory\(&request\.working_directory\)/);
  assert.match(installTerminal, /command\.cwd\(working_directory\)/);
}
);

test("dashboard update action is the leftmost action when updates are available", () => {
  const svelte = read("src/routes/Dashboard.svelte");
  const connectedSection = sliceBetween(
    svelte,
    '<div class="system-grid client-grid">',
    '<section class="panel-band">',
    "connected client section"
  );
  const connectedActions = sliceBetween(
    connectedSection,
    '<div class="client-card-actions">',
    "</article>",
    "connected client actions"
  );
  const connectedInstalledStart = connectedActions.indexOf(
    "{#if tool.updateAvailable}",
    connectedActions.indexOf("openInstallPlan(tool)")
  );
  assert.notEqual(connectedInstalledStart, -1, "connected installed actions should include an update branch");
  const connectedInstalledActions = connectedActions.slice(connectedInstalledStart);
  const connectedUpdateStart = actionStartBefore(connectedInstalledActions, 'openToolActionPlan(tool, "update")');
  const connectedRepairStart = actionStartBefore(connectedInstalledActions, "confirmRepairPath(tool)");
  const connectedLaunchStart = actionStartBefore(connectedInstalledActions, "openToolLaunch(tool)");
  const connectedConfigStart = actionStartBefore(connectedInstalledActions, "onConfigureTool(tool)");

  assert.ok(connectedUpdateStart < connectedRepairStart, "connected client update action should be before repair");
  assert.ok(connectedUpdateStart < connectedLaunchStart, "connected client update action should be before launch");
  assert.ok(connectedUpdateStart < connectedConfigStart, "connected client update action should be before config");

  const systemSection = sliceBetween(
    svelte,
    '<div class="system-grid">',
    '<div class="empty-row">{$t("dashboard.noSystemSnapshot")}</div>',
    "system section"
  );
  const systemUpdateBranchStart = systemSection.indexOf("{:else if tool.updateAvailable}");
  assert.notEqual(systemUpdateBranchStart, -1, "system update branch should exist");
  const systemUpdateBranch = systemSection.slice(systemUpdateBranchStart);
  const systemUpdateStart = actionStartBefore(systemUpdateBranch, 'openToolActionPlan(tool, "update")');
  const systemRepairStart = actionStartBefore(systemUpdateBranch, "confirmRepairPath(tool)");
  const systemLaunchStart = actionStartBefore(systemUpdateBranch, "openToolLaunch(tool)");

  assert.ok(systemUpdateStart < systemRepairStart, "system update action should be before repair");
  assert.ok(systemUpdateStart < systemLaunchStart, "system update action should be before launch");
});
