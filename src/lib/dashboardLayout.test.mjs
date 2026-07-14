import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8").replace(/\r\n/g, "\n");

test("dashboard cards keep installed status and three actions on one compact row", () => {
  const styles = read("src/styles.css");
  const pandaConfig = read("panda.config.ts");

  assert.doesNotMatch(styles, /\.system-card\s*\{/);
  assert.doesNotMatch(styles, /\.system-card-state\s*\{/);
  assert.doesNotMatch(styles, /\.client-card-actions\s*\{/);
  assert.doesNotMatch(styles, /\.card-action-overflow\s*\{/);
  assert.match(pandaConfig, /dashboardCardRecipe:[\s\S]*?gridTemplateColumns: "minmax\(86px, auto\) minmax\(220px, 1fr\)"/);
  assert.match(pandaConfig, /dashboardCardStateRecipe:[\s\S]*?minWidth: "86px"/);
  assert.match(pandaConfig, /dashboardCardActionsRecipe:[\s\S]*?flexFlow: "row nowrap"/);
  assert.match(pandaConfig, /dashboardCardActionsRecipe:[\s\S]*?gap: "6px"/);
  assert.match(pandaConfig, /actionButtonRecipe:[\s\S]*?compact:[\s\S]*?minHeight: "34px"/);
  assert.match(pandaConfig, /iconButtonRecipe:[\s\S]*?compact:[\s\S]*?minHeight: "30px"/);
});

test("dashboard desktop client cards use a real hit-area button for navigation", () => {
  const svelte = read("src/routes/Dashboard.svelte");
  const pandaConfig = read("panda.config.ts");

  assert.match(
    svelte,
    /function navigateToDesktopClient\(toolId: string\)\s*\{[\s\S]*desktopClientRouteForTool\(toolId\)[\s\S]*onNavigateToClient\(isChatGPTDesktopToolId\(toolId\) \? "chatgpt-desktop" : toolId\)/,
    "dashboard should centralize desktop client card navigation"
  );
  assert.match(
    svelte,
    /const dashboardCardHitAreaClass = css\(\{[\s\S]*?position: "absolute"[\s\S]*?inset: 0[\s\S]*?zIndex: 1[\s\S]*?cursor: "pointer"/,
    "dashboard should use a full-card hit-area button above non-action card content"
  );
  assert.doesNotMatch(
    svelte,
    /event\.target !== event\.currentTarget/,
    "card navigation should not be limited to clicks on the article element itself"
  );

  const connectedArticle = sliceBetween(
    svelte,
    '<article' + "\n" + '          class={dashboardCardRecipe({ clickable: Boolean(desktopClientRouteForTool(tool.id)) })}',
    '<div class={dashboardCardMainRecipe()}>',
    "connected desktop client article"
  );

  assert.doesNotMatch(connectedArticle, /role=\{desktopClientRouteForTool/);
  assert.doesNotMatch(connectedArticle, /tabindex=\{desktopClientRouteForTool/);
  assert.doesNotMatch(connectedArticle.split(">")[0], /on:click=\{/);
  assert.match(
    connectedArticle,
    /\{#if desktopClientRouteForTool\(tool\.id\)\}\s*<button[\s\S]*?class=\{dashboardCardHitAreaClass\}[\s\S]*?type="button"[\s\S]*?aria-label=\{tool\.name\}[\s\S]*?data-dashboard-card-hit-area[\s\S]*?on:click=\{\(\) => navigateToDesktopClient\(tool\.id\)\}[\s\S]*?>\s*<\/button>\s*\{\/if\}/
  );

  const connectedActions = sliceBetween(
    svelte,
    '<div' + "\n" + '            class={dashboardCardActionsRecipe()}',
    "</article>",
    "connected desktop client actions"
  );
  assert.match(connectedActions, /data-dashboard-card-actions/);
  assert.doesNotMatch(connectedActions.split(">")[0], /on:click\|stopPropagation/);
  assert.doesNotMatch(connectedActions.split(">")[0], /on:keydown\|stopPropagation/);
  assert.match(pandaConfig, /dashboardCardActionsRecipe:[\s\S]*?position: "relative"/);
  assert.match(pandaConfig, /dashboardCardActionsRecipe:[\s\S]*?zIndex: 2/);
});

test("dashboard overflow menu visibility stays in the recipe layer", () => {
  const svelte = read("src/routes/Dashboard.svelte");
  const pandaConfig = read("panda.config.ts");

  assert.doesNotMatch(
    svelte,
    /const dashboardOverflowMenuClass = css\(\{[\s\S]*?display: "none"/,
    "overflow menu should not use a utility-layer display:none class that can beat the recipe open selector"
  );
  assert.match(pandaConfig, /dashboardOverflowRecipe:[\s\S]*?"& \[data-dashboard-overflow-menu\]": \{[\s\S]*?display: "none"/);
  assert.match(pandaConfig, /dashboardOverflowRecipe:[\s\S]*?"&\[open\] \[data-dashboard-overflow-menu\]": \{[\s\S]*?display: "grid"/);
  assert.match(pandaConfig, /dashboardOverflowRecipe:[\s\S]*?"& > summary": \{[\s\S]*?cursor: "pointer"/);
  assert.match(svelte, /data-dashboard-overflow-menu/);
  assert.match(svelte, /on:click=\{\(event\) => toggleOverflowDetails/);
});

test("dashboard only folds actions after two direct buttons", () => {
  const svelte = read("src/routes/Dashboard.svelte");

  assert.match(svelte, /const dashboardCardActions: DashboardCardAction\[\] = \["update", "repair", "launch", "configure"\]/);
  assert.match(svelte, /function visibleDashboardActionLimit\(_?tool: ToolStatus\)\s*\{[\s\S]*return 2/);
  assert.match(svelte, /function shouldShowDashboardOverflow\(tool: ToolStatus\)\s*\{[\s\S]*availableDashboardActionCount\(tool\) > visibleDashboardActionLimit\(tool\)/);
  assert.match(svelte, /function isDashboardActionVisible\(tool: ToolStatus, action: DashboardCardAction\)\s*\{[\s\S]*const index = dashboardActionIndex\(tool, action\);[\s\S]*return index >= 0 && index < visibleDashboardActionLimit\(tool\)/);
  assert.match(svelte, /function isDashboardActionOverflowed\(tool: ToolStatus, action: DashboardCardAction\)\s*\{[\s\S]*const index = dashboardActionIndex\(tool, action\);[\s\S]*return index >= 0 && index >= visibleDashboardActionLimit\(tool\)/);

  const connectedSection = sliceBetween(
    svelte,
    '<div class={dashboardGridRecipe({ kind: "client" })}>',
    '<div class={dashboardGridRecipe({ kind: "system" })}>',
    "connected client section"
  );
  const connectedActions = sliceBetween(
    connectedSection,
    '<div' + "\n" + '            class={dashboardCardActionsRecipe()}',
    "</article>",
    "connected client actions"
  );

  assert.match(connectedActions, /isDashboardActionVisible\(tool, "update"\)/);
  assert.match(connectedActions, /isDashboardActionVisible\(tool, "repair"\)/);
  assert.match(connectedActions, /isDashboardActionVisible\(tool, "launch"\)/);
  assert.match(connectedActions, /isDashboardActionVisible\(tool, "configure"\)/);
  assert.match(connectedActions, /\{#if shouldShowDashboardOverflow\(tool\)\}/);
  assert.match(connectedActions, /isDashboardActionOverflowed\(tool, "update"\)/);
  assert.match(connectedActions, /isDashboardActionOverflowed\(tool, "repair"\)/);
  assert.match(connectedActions, /isDashboardActionOverflowed\(tool, "launch"\)/);
  assert.match(connectedActions, /isDashboardActionOverflowed\(tool, "configure"\)/);
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
  assert.match(
    svelte,
    /if \(action === "launch"\) \{[\s\S]*return tool\.installState !== "missing" && canShowToolLaunch\(tool\);[\s\S]*\}/
  );

  const launchHandlers = [...svelte.matchAll(/on:click=\{\(\) => openToolLaunch\(tool\)\}/g)];
  assert.ok(launchHandlers.length >= 1, "dashboard should still expose launch actions for non-plugin tools");

  const launchGuards = [
    '{#if isDashboardActionVisible(tool, "launch")}',
    '{#if isDashboardActionOverflowed(tool, "launch")}',
    "{#if canShowToolLaunch(tool)}"
  ];

  for (const handler of launchHandlers) {
    const handlerIndex = handler.index ?? -1;
    const buttonStart = svelte.lastIndexOf("<button", handlerIndex);
    const guardStart = Math.max(...launchGuards.map((guard) => svelte.lastIndexOf(guard, handlerIndex)));

    assert.notEqual(buttonStart, -1, "launch handler should be inside a button");
    assert.ok(guardStart !== -1 && guardStart < buttonStart, "launch button should be gated by launch availability");
  }
});

test("dashboard refresh button follows external detection refresh state", () => {
  const svelte = read("src/routes/Dashboard.svelte");

  assert.match(svelte, /export let refreshingExternally = false/);
  assert.match(svelte, /\$:\s*refreshBusy = refreshing \|\| refreshingExternally/);
  assert.match(svelte, /data-refresh-button="true"/);
  assert.match(svelte, /disabled=\{refreshBusy\}/);
  assert.match(svelte, /name=\{refreshBusy \? "loading" : "refresh"\}/);
  assert.match(svelte, /size=\{15\}/);
  assert.match(svelte, /class=\{refreshBusy \? spinRecipe\(\) : ""\}/);
  assert.match(svelte, /\$t\(refreshBusy \? "common\.refreshing" : "common\.refresh"\)/);
  assert.match(svelte, /onRefresh\(\{ quiet: false, scheduleFollowup: true, showRefreshIndicator: true \}\)/);
});

test("dashboard foreground refresh waits for update checks while background refresh stays fast", () => {
  const app = read("src/App.svelte");

  assert.match(app, /const waitForUpdates = options\.waitForUpdates \?\? showRefreshIndicator/);
  assert.match(app, /detectEnvironment\(\{ waitForUpdates \}\)/);
  assert.match(
    app,
    /refreshDashboard\(\{ quiet: true, scheduleFollowup: true, showRefreshIndicator: false, waitForUpdates: false \}\)/
  );
  assert.match(
    app,
    /refreshDashboard\(\{ quiet: true, scheduleFollowup: true, showRefreshIndicator: true, waitForUpdates: true \}\)/
  );
});

test("dashboard refresh reuses unchanged detection snapshots to avoid end-of-refresh jank", () => {
  const app = read("src/App.svelte");

  assert.match(app, /function detectionSnapshotUiPayload\(value: DetectionSnapshot\)/);
  assert.match(app, /const \{ generatedAt, source, \.\.\.stableSnapshot \} = value/);
  assert.match(app, /return stableSnapshot/);
  assert.match(app, /function detectionSnapshotUiChanged\(current: DetectionSnapshot \| null, next: DetectionSnapshot\)/);
  assert.match(app, /JSON\.stringify\(detectionSnapshotUiPayload\(current\)\)/);
  assert.match(app, /JSON\.stringify\(detectionSnapshotUiPayload\(next\)\)/);
  assert.match(app, /function applyDetectionSnapshot\(nextSnapshot: DetectionSnapshot\)/);
  assert.match(app, /if \(detectionSnapshotUiChanged\(snapshot, nextSnapshot\)\) \{\s*snapshot = nextSnapshot;\s*\}/);
  assert.doesNotMatch(app, /profileSummary = nextProfileSummary;\s*snapshot = nextSnapshot;\s*gatewayStatus = nextGatewayStatus;/);
});

test("dashboard refresh reuses unchanged profile and gateway state objects", () => {
  const app = read("src/App.svelte");

  assert.match(app, /function profileSummaryUiChanged\(current: ProfileSummary \| null, next: ProfileSummary\)/);
  assert.match(app, /function applyProfileSummary\(nextSummary: ProfileSummary\)/);
  assert.match(app, /if \(profileSummaryUiChanged\(profileSummary, nextSummary\)\) \{\s*profileSummary = nextSummary;\s*\}/);
  assert.match(app, /function gatewayStatusUiChanged\(current: GatewayStatus \| null, next: GatewayStatus \| null\)/);
  assert.match(app, /function applyGatewayStatus\(nextStatus: GatewayStatus \| null\)/);
  assert.match(app, /if \(gatewayStatusUiChanged\(gatewayStatus, nextStatus\)\) \{\s*gatewayStatus = nextStatus;\s*\}/);
  assert.doesNotMatch(app, /profileSummary = nextProfileSummary;/);
  assert.doesNotMatch(app, /gatewayStatus = nextGatewayStatus;/);
  assert.doesNotMatch(app, /gatewayStatus = result\.status;/);
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

test("CLI launch panel defaults to external terminal first", () => {
  const dashboard = read("src/routes/Dashboard.svelte");
  const modeSection = dashboard.slice(
    dashboard.indexOf('{$t("toolLaunch.launchMode")}'),
    dashboard.indexOf('{$t("toolLaunch.workingDirectory")}', dashboard.indexOf('{$t("toolLaunch.launchMode")}') )
  );

  assert.match(dashboard, /let launchMode: LaunchMode = "external"/);
  assert.match(dashboard, /launchMode = "external"/);
  assert.ok(
    modeSection.indexOf('class={dashboardLaunchOptionClass(launchMode === "external")}') < modeSection.indexOf('class={dashboardLaunchOptionClass(launchMode === "embedded")}') ,
    "external launch option should render before embedded"
  );
});
test("dashboard update action is the leftmost action when updates are available", () => {
  const svelte = read("src/routes/Dashboard.svelte");
  const connectedSection = sliceBetween(
    svelte,
    '<div class={dashboardGridRecipe({ kind: "client" })}>',
    '<div class={dashboardGridRecipe({ kind: "system" })}>',
    "connected client section"
  );
  const connectedActions = sliceBetween(
    connectedSection,
    '<div' + "\n" + '            class={dashboardCardActionsRecipe()}',
    "</article>",
    "connected client actions"
  );
  const connectedInstalledStart = connectedActions.indexOf(
    '{#if isDashboardActionVisible(tool, "update")}',
    connectedActions.indexOf("openInstallPlan(tool)")
  );
  assert.notEqual(connectedInstalledStart, -1, "connected installed actions should include an update branch");
  const connectedInstalledActions = connectedActions.slice(connectedInstalledStart);
  const connectedUpdateStart = connectedInstalledActions.indexOf('{#if isDashboardActionVisible(tool, "update")}');
  const connectedRepairStart = connectedInstalledActions.indexOf('{#if isDashboardActionVisible(tool, "repair")}');
  const connectedLaunchStart = connectedInstalledActions.indexOf('{#if isDashboardActionVisible(tool, "launch")}');
  const connectedConfigStart = connectedInstalledActions.indexOf('{#if isDashboardActionVisible(tool, "configure")}');

  assert.ok(connectedUpdateStart < connectedRepairStart, "connected client update action should be before repair");
  assert.ok(connectedUpdateStart < connectedLaunchStart, "connected client update action should be before launch");
  assert.ok(connectedUpdateStart < connectedConfigStart, "connected client update action should be before config");

  const connectedOverflowActions = sliceBetween(
    connectedInstalledActions,
    '<div class={dashboardOverflowMenuClass} data-dashboard-overflow-menu>',
    "</details>",
    "connected client overflow actions"
  );
  assertBefore(
    connectedOverflowActions,
    'isDashboardActionOverflowed(tool, "update")',
    'isDashboardActionOverflowed(tool, "repair")',
    "connected client overflow"
  );
  assertBefore(
    connectedOverflowActions,
    'isDashboardActionOverflowed(tool, "update")',
    'isDashboardActionOverflowed(tool, "launch")',
    "connected client overflow"
  );
  assertBefore(
    connectedOverflowActions,
    'isDashboardActionOverflowed(tool, "update")',
    'isDashboardActionOverflowed(tool, "configure")',
    "connected client overflow"
  );

  const systemSection = sliceBetween(
    svelte,
    '<div class={dashboardGridRecipe({ kind: "system" })}>',
    '<div class={emptyRowRecipe()} data-dashboard-empty>{$t("dashboard.noSystemSnapshot")}</div>',
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
