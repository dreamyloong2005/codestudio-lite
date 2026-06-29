import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

const recipeBlock = (source, recipeName, nextRecipeName) => {
  const start = source.indexOf(`${recipeName}: {`);
  const end = source.indexOf(`${nextRecipeName}: {`, start);
  assert.ok(start >= 0, `${recipeName} should exist in panda.config.ts`);
  assert.ok(end > start, `${recipeName} should be followed by ${nextRecipeName}`);
  return source.slice(start, end);
};

test("Panda CSS infrastructure is wired for Svelte", () => {
  const packageJson = read("package.json");
  const pandaConfig = read("panda.config.ts");
  const postcssConfig = read("postcss.config.cjs");
  const main = read("src/main.ts");
  const gitignore = read(".gitignore");

  assert.match(packageJson, /"@pandacss\/dev"/);
  assert.match(packageJson, /"panda:codegen"/);
  assert.match(packageJson, /"prepare":\s*"panda codegen"/);
  assert.match(pandaConfig, /include:\s*\[/);
  assert.match(pandaConfig, /src\/\*\*\/\*\.\{svelte,ts\}/);
  assert.match(pandaConfig, /outdir:\s*"styled-system"/);
  assert.match(postcssConfig, /@pandacss\/dev\/postcss/);
  assert.match(main, /import "\.\/panda\.css";/);
  assert.match(gitignore, /styled-system\//);
});

test("DismissibleNotice uses Panda recipes instead of legacy notice classes", () => {
  const component = read("src/components/DismissibleNotice.svelte");
  const styles = read("src/styles.css");

  assert.match(component, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(component, /import \{ noticeRecipe \} from "\.\.\/\.\.\/styled-system\/recipes";/);
  assert.match(component, /noticeRecipe\(\{ tone \}\)/);
  assert.doesNotMatch(component, /class=\{`notice inline-\$\{tone\}`\}/);
  assert.doesNotMatch(styles, /\.notice\s*\{/);
  assert.doesNotMatch(styles, /\.notice-dismiss\s*\{/);
});

test("shared status and secret input components use Panda styling", () => {
  const statusPill = read("src/components/StatusPill.svelte");
  const secretInput = read("src/components/SecretInput.svelte");
  const styles = read("src/styles.css");

  assert.match(statusPill, /import \{ statusPillRecipe \} from "\.\.\/\.\.\/styled-system\/recipes";/);
  assert.match(statusPill, /statusPillRecipe\(\{ tone \}\)/);
  assert.doesNotMatch(statusPill, /class=\{`pill \$\{tone\}`\}/);

  assert.match(secretInput, /import \{ iconButtonRecipe, secretInputRecipe \} from "\.\.\/\.\.\/styled-system\/recipes";/);
  assert.match(secretInput, /secretInputRecipe\(\)/);
  assert.match(secretInput, /iconButtonRecipe\(\)/);
  assert.doesNotMatch(secretInput, /toggleButtonClass/);
  assert.doesNotMatch(secretInput, /class="secret-input"/);
  assert.doesNotMatch(styles, /\.secret-input\s*\{/);
});

test("ProblemList owns its row and list styling through Panda", () => {
  const component = read("src/components/ProblemList.svelte");
  const styles = read("src/styles.css");

  assert.match(component, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(component, /problemListRecipe/);
  assert.match(component, /problemRowRecipe/);
  assert.match(component, /problemListRecipe\(\)/);
  assert.match(component, /problemRowRecipe\(\)/);
  assert.doesNotMatch(component, /class="problem-list"/);
  assert.doesNotMatch(component, /class="problem-row"/);
  assert.doesNotMatch(styles, /\.problem-list\s*,/);
  assert.doesNotMatch(styles, /\.problem-row\s*,/);
  assert.doesNotMatch(styles, /\.problem-row\s+h3/);
  assert.doesNotMatch(styles, /\.problem-row\s+p/);
});

test("ToolStatusCard no longer depends on legacy tool-card classes", () => {
  const component = read("src/components/ToolStatusCard.svelte");

  assert.match(component, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(component, /import \{[^}]*actionButtonRecipe[^}]*spinRecipe[^}]*toolCardRecipe[^}]*toolMainRecipe[^}]*toolStateRecipe[^}]*toolActionRecipe[^}]*\} from "\.\.\/\.\.\/styled-system\/recipes";/);
  assert.match(component, /toolCardRecipe\(\)/);
  assert.match(component, /toolMainRecipe\(\)/);
  assert.match(component, /toolStateRecipe\(\)/);
  assert.match(component, /toolActionRecipe\(\)/);
  assert.match(component, /actionButtonRecipe\(\{ compact: true \}\)/);
  assert.doesNotMatch(component, /actionButtonClass/);
  assert.doesNotMatch(component, /class="tool-card"/);
  assert.doesNotMatch(component, /class="tool-main"/);
  assert.doesNotMatch(component, /class="tool-copy"/);
  assert.doesNotMatch(component, /class="tool-path"/);
  assert.doesNotMatch(component, /class="tool-state"/);
  assert.doesNotMatch(component, /class="tool-action"/);
});

test("ToolIcon owns icon tone and size styling through Panda", () => {
  const component = read("src/components/ToolIcon.svelte");
  const styles = read("src/styles.css");
  const pandaConfig = read("panda.config.ts");
  const toolIcon = recipeBlock(pandaConfig, "toolIconRecipe", "problemListRecipe");

  assert.match(component, /import \{ toolIconRecipe \} from "\.\.\/\.\.\/styled-system\/recipes";/);
  assert.match(component, /toolIconRecipe\(\{\s*variant,\s*tone: iconTone\s*\}\)/s);
  assert.match(component, /data-tool-icon-variant=\{variant\}/);
  assert.match(component, /data-tool-icon-tone=\{iconTone\}/);
  assert.match(component, /"codex-vscode": \{ src: "\/tool-icons\/codex-vscode\.svg", tone: "vscode" \}/);
  assert.match(component, /"claude-vscode": \{ src: "\/tool-icons\/claude-vscode\.svg", tone: "vscode" \}/);
  assert.match(component, /"gemini-code-assist": \{ src: "\/tool-icons\/gemini-code-assist\.svg", tone: "vscode" \}/);
  assert.match(component, /data-tool-icon-fallback-text/);
  assert.doesNotMatch(component, /class:tool-icon-/);
  assert.doesNotMatch(component, /class=\{`tool-icon/);
  assert.doesNotMatch(styles, /\.tool-icon/);
  assert.doesNotMatch(pandaConfig, /"[^"]*\.tool-icon/);
  assert.match(toolIcon, /codex:\s*\{[\s\S]*?background: "#111111"[\s\S]*?\}/);
  assert.match(toolIcon, /"&\[data-tool-icon-tone='codex'\]": \{[\s\S]*?background: "#111111"[\s\S]*?\}/);
  assert.doesNotMatch(toolIcon, /"&\[data-tool-icon-tone='codex-app'\]": \{[\s\S]*?background: "#111111"/);
  assert.match(toolIcon, /"codex-app": \{[\s\S]*?background: "#fff"[\s\S]*?\}/);
  assert.doesNotMatch(toolIcon, /"codex-app": \{[\s\S]*?background: "#111111"/);
  assert.match(toolIcon, /vscode:\s*\{[\s\S]*?background: "#007ACC"/);
  assert.match(toolIcon, /"&\[data-tool-icon-tone='vscode'\] img": \{[\s\S]*?width: "24px"[\s\S]*?height: "24px"/);
  assert.match(toolIcon, /choice:\s*\{[\s\S]*?"&\[data-tool-icon-tone='vscode'\] img": \{[\s\S]*?width: "18px"[\s\S]*?height: "18px"/);
  assert.doesNotMatch(toolIcon, /data-tool-icon-tone='vscode'][\s\S]*?width: "30px"/);
});

test("unowned legacy tool and profile globals are removed", () => {
  const styles = read("src/styles.css");
  const productionSurfaces = [
    "src/App.svelte",
    "src/components/ToolStatusCard.svelte",
    "src/routes/Dashboard.svelte",
    "src/routes/CodexClient.svelte",
    "src/routes/ClaudeDesktop.svelte",
    "src/routes/Profiles.svelte",
    "src/routes/SetupWizard.svelte",
    "src/routes/Gateway.svelte",
    "src/routes/Settings.svelte",
    "src/routes/TerminalPanel.svelte"
  ].map((file) => read(file)).join("\n");

  assert.match(read("panda.config.ts"), /appErrorBannerRecipe/);
  assert.match(read("panda.config.ts"), /wizardWideFieldRecipe/);
  assert.match(read("src/App.svelte"), /appErrorBannerRecipe\(\)/);
  assert.match(read("src/routes/SetupWizard.svelte"), /wizardWideFieldRecipe\(\)/);

  for (const selector of [
    "tool-grid",
    "tool-copy",
    "tool-card",
    "tool-main",
    "tool-path",
    "tool-state",
    "tool-action",
    "provider-summary",
    "provider-mode-grid",
    "backup-row",
    "test-panel",
    "tool-check-list",
    "check-summary",
    "check-row",
    "launch-options-grid",
    "codex-oauth-panel",
    "oauth-grid",
    "oauth-card",
    "oauth-toggle",
    "profile-choice-grid",
    "profile-choice-item",
    "oauth-confirm-panel",
    "oauth-diff",
    "quiet"
  ]) {
    assert.doesNotMatch(styles, new RegExp(`\\.${selector}\\b`));
  }

  assert.doesNotMatch(productionSurfaces, /class="error-banner"/);
  assert.doesNotMatch(productionSurfaces, /class="wide-field"/);
});

test("shared panel components use Panda panel and button recipes", () => {
  const activityLog = read("src/components/ActivityLog.svelte");
  const problemList = read("src/components/ProblemList.svelte");

  assert.match(activityLog, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(activityLog, /import \{ activityListRecipe, activityRowRecipe, emptyRowRecipe, panelRecipe, sectionHeadingRecipe \} from "\.\.\/\.\.\/styled-system\/recipes";/);
  assert.match(activityLog, /panelRecipe\(\)/);
  assert.match(activityLog, /sectionHeadingRecipe\(\{ compact: true \}\)/);
  assert.match(activityLog, /activityListRecipe\(\)/);
  assert.match(activityLog, /activityRowRecipe\(\)/);
  assert.match(activityLog, /emptyRowRecipe\(\)/);
  assert.doesNotMatch(activityLog, /class="panel-band"/);
  assert.doesNotMatch(activityLog, /class="section-heading compact"/);
  assert.doesNotMatch(activityLog, /class="activity-list"/);
  assert.doesNotMatch(activityLog, /class="activity-row"/);
  assert.doesNotMatch(activityLog, /class="empty-row"/);

  assert.match(problemList, /import \{[^}]*actionButtonRecipe[^}]*emptyRowRecipe[^}]*panelRecipe[^}]*sectionHeadingRecipe[^}]*\} from "\.\.\/\.\.\/styled-system\/recipes";/s);
  assert.match(problemList, /panelRecipe\(\)/);
  assert.match(problemList, /sectionHeadingRecipe\(\)/);
  assert.match(problemList, /actionButtonRecipe\(\{ tone: "primary" \}\)/);
  assert.match(problemList, /emptyRowRecipe\(\)/);
  assert.doesNotMatch(problemList, /class="panel-band"/);
  assert.doesNotMatch(problemList, /class="section-heading"/);
  assert.doesNotMatch(problemList, /class="primary-button"/);
  assert.doesNotMatch(problemList, /class="empty-row"/);
});

test("shared button typography stays compact after the Panda migration", () => {
  const pandaConfig = read("panda.config.ts");
  const appNav = recipeBlock(pandaConfig, "appNavRecipe", "appNavButtonRecipe");
  const appNavButton = recipeBlock(pandaConfig, "appNavButtonRecipe", "appNavLabelRecipe");
  const appNavLabel = recipeBlock(pandaConfig, "appNavLabelRecipe", "appNavUpdateDotRecipe");
  const actionButton = recipeBlock(pandaConfig, "actionButtonRecipe", "iconButtonRecipe");
  const iconButton = recipeBlock(pandaConfig, "iconButtonRecipe", "emptyRowRecipe");
  const dashboardCardActions = recipeBlock(pandaConfig, "dashboardCardActionsRecipe", "dashboardOverflowRecipe");
  const dashboardOverflow = recipeBlock(pandaConfig, "dashboardOverflowRecipe", "dashboardEnvConflictRecipe");
  const desktopTabs = recipeBlock(pandaConfig, "desktopClientTabsRecipe", "desktopClientMetricsRecipe");
  const gatewaySegmented = recipeBlock(pandaConfig, "gatewaySegmentedRecipe", "gatewayInlineErrorRecipe");
  const profileToolTabs = recipeBlock(pandaConfig, "profileToolTabsRecipe", "profileToolSectionRecipe");
  const profileUsageTemplateRow = recipeBlock(pandaConfig, "profileUsageTemplateRowRecipe", "profileUsageCodeFieldRecipe");
  const wizardChoiceButton = recipeBlock(pandaConfig, "wizardChoiceButtonRecipe", "wizardModeChoiceRecipe");

  assert.match(appNav, /gap:\s*"8px"/);
  assert.match(appNavButton, /minHeight:\s*"42px"/);
  assert.match(appNavButton, /padding:\s*"0 12px"/);
  assert.doesNotMatch(appNavButton, /translateX/);
  assert.match(appNavLabel, /fontSize:\s*"14px"/);
  assert.match(actionButton, /minHeight:\s*"32px"/);
  assert.match(actionButton, /padding:\s*"0 10px"/);
  assert.match(actionButton, /fontSize:\s*"11px"/);
  assert.match(actionButton, /"&\[data-refresh-button='true'\]": \{[\s\S]*?fontSize:\s*"12px"/);
  assert.match(actionButton, /"&\[data-refresh-button='true'\]": \{[\s\S]*?"& svg": \{[\s\S]*?width:\s*"15px"/);
  assert.match(actionButton, /compact:\s*\{[\s\S]*?minHeight:\s*"30px"/);
  assert.match(actionButton, /compact:\s*\{[\s\S]*?padding:\s*"0 9px"/);
  assert.match(actionButton, /compact:\s*\{[\s\S]*?fontSize:\s*"10.5px"/);
  assert.match(iconButton, /width:\s*"32px"/);
  assert.match(iconButton, /minHeight:\s*"32px"/);
  assert.match(iconButton, /compact:\s*\{[\s\S]*?height:\s*"28px"/);
  assert.match(iconButton, /compact:\s*\{[\s\S]*?width:\s*"28px"/);
  assert.match(dashboardCardActions, /"& > button, & \[data-dashboard-overflow-menu\] button": \{[\s\S]*?minHeight:\s*"28px"/);
  assert.match(dashboardCardActions, /"& > button, & \[data-dashboard-overflow-menu\] button": \{[\s\S]*?padding:\s*"0 8px"/);
  assert.match(dashboardCardActions, /"& > button, & \[data-dashboard-overflow-menu\] button": \{[\s\S]*?fontSize:\s*"10.5px"/);
  assert.match(dashboardOverflow, /"& > summary": \{[\s\S]*?width:\s*"32px"/);
  assert.match(dashboardOverflow, /"& > summary": \{[\s\S]*?height:\s*"28px"/);
  assert.match(desktopTabs, /"& button": \{[\s\S]*?minHeight:\s*"34px"/);
  assert.match(desktopTabs, /"& button": \{[\s\S]*?padding:\s*"0 10px"/);
  assert.match(desktopTabs, /"& button": \{[\s\S]*?fontSize:\s*"10.5px"/);
  assert.match(gatewaySegmented, /"& button": \{[\s\S]*?minHeight:\s*"30px"/);
  assert.match(gatewaySegmented, /"& button": \{[\s\S]*?fontSize:\s*"10.5px"/);
  assert.match(profileToolTabs, /"& > button": \{[\s\S]*?minWidth:\s*"136px"/);
  assert.match(profileToolTabs, /"& > button": \{[\s\S]*?minHeight:\s*"46px"/);
  assert.match(profileToolTabs, /"& > button": \{[\s\S]*?padding:\s*"7px 9px"/);
  assert.match(profileToolTabs, /"& strong": \{[\s\S]*?fontSize:\s*"11px"/);
  assert.match(profileUsageTemplateRow, /"& button": \{[\s\S]*?minHeight:\s*"30px"/);
  assert.match(profileUsageTemplateRow, /"& button": \{[\s\S]*?padding:\s*"0 10px"/);
  assert.match(profileUsageTemplateRow, /"& button": \{[\s\S]*?fontSize:\s*"11px"/);
  assert.match(wizardChoiceButton, /minHeight:\s*"44px"/);
  assert.match(wizardChoiceButton, /fontSize:\s*"11px"/);
  assert.match(wizardChoiceButton, /kind:\s*\{[\s\S]*?tool:\s*\{[\s\S]*?minHeight:\s*"78px"/);
  assert.match(wizardChoiceButton, /kind:\s*\{[\s\S]*?tool:\s*\{[\s\S]*?padding:\s*"12px 10px 10px"/);
});

test("non-button typography is unchanged by compact button sizing", () => {
  const pandaConfig = read("panda.config.ts");
  const notice = recipeBlock(pandaConfig, "noticeRecipe", "statusPillRecipe");
  const toolIcon = recipeBlock(pandaConfig, "toolIconRecipe", "problemListRecipe");
  const dashboardCommandBox = recipeBlock(pandaConfig, "dashboardCommandBoxRecipe", "dashboardCommandListRecipe");
  const dashboardCommandList = recipeBlock(pandaConfig, "dashboardCommandListRecipe", "dashboardInfoGridRecipe");
  const dashboardInfoGrid = recipeBlock(pandaConfig, "dashboardInfoGridRecipe", "dashboardPreviewListRecipe");
  const dashboardPreviewList = recipeBlock(pandaConfig, "dashboardPreviewListRecipe", "dashboardLogRecipe");

  assert.match(notice, /fontSize:\s*"13px"/);
  assert.match(toolIcon, /"\& \[data-tool-icon-fallback-text\]": \{[\s\S]*?fontSize:\s*"11px"/);
  assert.match(dashboardCommandBox, /"& span": \{[\s\S]*?fontSize:\s*"12px"/);
  assert.match(dashboardCommandList, /"& span": \{[\s\S]*?fontSize:\s*"12px"/);
  assert.match(dashboardInfoGrid, /"& > span": \{[\s\S]*?fontSize:\s*"12px"/);
  assert.match(dashboardInfoGrid, /"& strong": \{[\s\S]*?fontSize:\s*"13px"/);
  assert.match(dashboardPreviewList, /"& span": \{[\s\S]*?fontSize:\s*"13px"/);
});

test("Dashboard main cards use Panda recipes for route surfaces", () => {
  const dashboard = read("src/routes/Dashboard.svelte");

  assert.match(dashboard, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(dashboard, /dashboardGridRecipe/);
  assert.match(dashboard, /dashboardCardRecipe/);
  assert.match(dashboard, /dashboardCardMainRecipe/);
  assert.match(dashboard, /dashboardCardStateRecipe/);
  assert.match(dashboard, /dashboardCardActionsRecipe/);
  assert.match(dashboard, /dashboardOverflowRecipe/);
  assert.match(dashboard, /dashboardEnvConflictRecipe/);
  assert.match(dashboard, /panelRecipe\(\)/);
  assert.match(dashboard, /sectionHeadingRecipe\(\)/);
  assert.match(dashboard, /actionButtonRecipe\(\{ compact: true \}\)/);
  assert.match(dashboard, /iconButtonRecipe\(\{ compact: true \}\)/);
  assert.match(dashboard, /emptyRowRecipe\(\)/);
  assert.match(dashboard, /data-dashboard-env-conflict-chips/);

  const mainMarkup = dashboard.split("{#if pendingInstallTool}")[0];
  assert.doesNotMatch(mainMarkup, /class="inline-error env-conflict-banner"/);
  assert.doesNotMatch(mainMarkup, /class="conflict-chip-list"/);
  assert.doesNotMatch(mainMarkup, /class="panel-band"/);
  assert.doesNotMatch(mainMarkup, /class="section-heading"/);
  assert.doesNotMatch(mainMarkup, /class="system-grid/);
  assert.doesNotMatch(mainMarkup, /class="system-card/);
  assert.doesNotMatch(mainMarkup, /class="system-main"/);
  assert.doesNotMatch(mainMarkup, /class="system-copy"/);
  assert.doesNotMatch(mainMarkup, /class="tool-path"/);
  assert.doesNotMatch(mainMarkup, /class="system-card-state/);
  assert.doesNotMatch(mainMarkup, /class="client-card-actions"/);
  assert.doesNotMatch(mainMarkup, /class="secondary-button"/);
  assert.doesNotMatch(mainMarkup, /class="icon-button card-overflow-toggle"/);
  assert.doesNotMatch(mainMarkup, /class="empty-row"/);
});

test("Dashboard install and launch modals use Panda recipes", () => {
  const dashboard = read("src/routes/Dashboard.svelte");
  const pandaConfig = read("panda.config.ts");
  const modalMarkup = dashboard.split("{#if pendingInstallTool}")[1];

  assert.match(dashboard, /dashboardModalBackdropRecipe/);
  assert.match(dashboard, /dashboardModalPanelRecipe/);
  assert.match(dashboard, /dashboardModalBodyRecipe/);
  assert.match(dashboard, /dashboardModalActionsRecipe/);
  assert.match(dashboard, /dashboardProgressRecipe/);
  assert.match(dashboard, /dashboardCommandBoxRecipe/);
  assert.match(dashboard, /dashboardTerminalCardRecipe/);
  assert.match(dashboard, /dashboardLogRecipe/);
  assert.match(dashboard, /dashboardLaunchOptionRecipe/);
  assert.match(dashboard, /data-selected=\{!selectedLaunchProfileId\}/);
  assert.match(dashboard, /data-selected=\{selectedLaunchProfileId === profile\.id\}/);
  assert.match(dashboard, /data-selected=\{selectedLaunchShellId === shell\.id\}/);
  assert.match(dashboard, /data-selected=\{launchMode === "embedded"\}/);
  assert.match(dashboard, /data-selected=\{launchMode === "external"\}/);
  assert.match(pandaConfig, /"&\[data-selected='true'\]": \{[\s\S]*?borderColor: "color-mix\(in srgb, var\(--accent\) 48%, transparent\)"/);
  assert.match(dashboard, /noticeRecipe\(\{ tone: installResult\.success \? "success" : "error" \}\)/);
  assert.match(dashboard, /actionButtonRecipe\(\{ tone: "primary" \}\)/);

  assert.doesNotMatch(modalMarkup, /class="modal-backdrop"/);
  assert.doesNotMatch(modalMarkup, /class="modal-panel wide-modal"/);
  assert.doesNotMatch(modalMarkup, /class="modal-body"/);
  assert.doesNotMatch(modalMarkup, /class="modal-actions"/);
  assert.doesNotMatch(modalMarkup, /class="install-progress"/);
  assert.doesNotMatch(modalMarkup, /class="install-command-box"/);
  assert.doesNotMatch(modalMarkup, /class="install-command-list"/);
  assert.doesNotMatch(modalMarkup, /class="install-meta"/);
  assert.doesNotMatch(modalMarkup, /class="install-result-grid"/);
  assert.doesNotMatch(modalMarkup, /class="install-log/);
  assert.doesNotMatch(modalMarkup, /class="install-terminal/);
  assert.doesNotMatch(modalMarkup, /class="launch-section/);
  assert.doesNotMatch(modalMarkup, /class="launch-option/);
  assert.doesNotMatch(modalMarkup, /class="preview-list"/);
  assert.doesNotMatch(modalMarkup, /class="primary-button"/);
  assert.doesNotMatch(modalMarkup, /class="secondary-button"/);
  assert.doesNotMatch(modalMarkup, /class="icon-button"/);
  assert.doesNotMatch(modalMarkup, /class="inline-error"/);
  assert.doesNotMatch(modalMarkup, /class="inline-success"/);
});

test("Codex Client main route surfaces use Panda recipes", () => {
  const route = read("src/routes/CodexClient.svelte");
  const mainMarkup = route.split("{#if confirmUninstall}")[0];

  assert.match(route, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(route, /desktopClientMetricsRecipe/);
  assert.match(route, /desktopClientProgressRecipe/);
  assert.match(route, /desktopClientTabsRecipe/);
  assert.match(route, /nativeToggleRecipe/);
  assert.match(route, /doctorListRecipe/);
  assert.match(route, /doctorRowRecipe/);
  assert.match(route, /panelRecipe\(\)/);
  assert.match(route, /sectionHeadingRecipe\(\)/);
  assert.match(route, /actionButtonRecipe\(\{ tone: "primary" \}\)/);
  assert.match(route, /emptyRowRecipe\(\)/);

  assert.doesNotMatch(mainMarkup, /class="top-actions"/);
  assert.doesNotMatch(mainMarkup, /class="primary-button"/);
  assert.doesNotMatch(mainMarkup, /class="secondary-button"/);
  assert.doesNotMatch(mainMarkup, /class="install-kind-tabs"/);
  assert.doesNotMatch(mainMarkup, /class="panel-band"/);
  assert.doesNotMatch(mainMarkup, /class="section-heading"/);
  assert.doesNotMatch(mainMarkup, /class="settings-list codex-client-settings/);
  assert.doesNotMatch(mainMarkup, /class="native-write-toggle/);
  assert.doesNotMatch(mainMarkup, /class="gateway-metrics codex-client-metrics"/);
  assert.doesNotMatch(mainMarkup, /class="gateway-actions codex-client-actions"/);
  assert.doesNotMatch(mainMarkup, /class="install-progress"/);
  assert.doesNotMatch(mainMarkup, /class="progress-copy"/);
  assert.doesNotMatch(mainMarkup, /class="progress-track/);
  assert.doesNotMatch(mainMarkup, /class="progress-fill"/);
  assert.doesNotMatch(mainMarkup, /class="progress-meta"/);
  assert.doesNotMatch(mainMarkup, /class="preview-list codex-client-list"/);
  assert.doesNotMatch(mainMarkup, /class="doctor-list"/);
  assert.doesNotMatch(mainMarkup, /class="doctor-row"/);
  assert.doesNotMatch(mainMarkup, /class="empty-row"/);
});

test("Codex Client uninstall modal uses Panda recipes", () => {
  const route = read("src/routes/CodexClient.svelte");
  const modalMarkup = route.split("{#if confirmUninstall}")[1];
  const styles = read("src/styles.css");

  assert.match(route, /desktopClientModalBackdropRecipe/);
  assert.match(route, /desktopClientModalPanelRecipe/);
  assert.match(route, /desktopClientModalBodyRecipe/);
  assert.match(route, /desktopClientModalActionsRecipe/);
  assert.match(route, /desktopClientPreviewListRecipe/);
  assert.match(route, /actionButtonRecipe\(\{ tone: "primary" \}\)/);

  assert.doesNotMatch(modalMarkup, /class="modal-backdrop"/);
  assert.doesNotMatch(modalMarkup, /class="modal-panel"/);
  assert.doesNotMatch(modalMarkup, /class="modal-body"/);
  assert.doesNotMatch(modalMarkup, /class="modal-actions"/);
  assert.doesNotMatch(modalMarkup, /class="preview-list"/);
  assert.doesNotMatch(modalMarkup, /class="primary-button"/);
  assert.doesNotMatch(modalMarkup, /class="secondary-button"/);

  assert.doesNotMatch(styles, /\.modal-backdrop\b/);
  assert.doesNotMatch(styles, /\.modal-panel\b/);
  assert.doesNotMatch(styles, /\.modal-body\b/);
  assert.doesNotMatch(styles, /\.modal-actions\b/);
});

test("Claude Desktop route surfaces use Panda desktop client recipes", () => {
  const route = read("src/routes/ClaudeDesktop.svelte");

  assert.match(route, /import \{ css \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.doesNotMatch(route, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(route, /desktopClientMetricsRecipe/);
  assert.match(route, /desktopClientProgressRecipe/);
  assert.match(route, /desktopClientTabsRecipe/);
  assert.match(route, /desktopClientLogRecipe/);
  assert.match(route, /desktopClientPreviewListRecipe/);
  assert.match(route, /desktopClientModalBackdropRecipe/);
  assert.match(route, /nativeToggleRecipe/);
  assert.match(route, /doctorListRecipe/);
  assert.match(route, /doctorRowRecipe/);
  assert.match(route, /panelRecipe\(\)/);
  assert.match(route, /sectionHeadingRecipe\(\)/);
  assert.match(route, /actionButtonRecipe\(\{ tone: "primary" \}\)/);
  assert.match(route, /emptyRowRecipe\(\)/);

  assert.doesNotMatch(route, /class="top-actions"/);
  assert.doesNotMatch(route, /class="primary-button"/);
  assert.doesNotMatch(route, /class="secondary-button"/);
  assert.doesNotMatch(route, /class="install-kind-tabs"/);
  assert.doesNotMatch(route, /class="panel-band"/);
  assert.doesNotMatch(route, /class="section-heading"/);
  assert.doesNotMatch(route, /class="settings-list codex-client-settings/);
  assert.doesNotMatch(route, /class="native-write-toggle/);
  assert.doesNotMatch(route, /class="gateway-metrics codex-client-metrics"/);
  assert.doesNotMatch(route, /class="gateway-actions codex-client-actions"/);
  assert.doesNotMatch(route, /class="install-progress"/);
  assert.doesNotMatch(route, /class="progress-copy"/);
  assert.doesNotMatch(route, /class="progress-track/);
  assert.doesNotMatch(route, /class="progress-fill"/);
  assert.doesNotMatch(route, /class="progress-meta"/);
  assert.doesNotMatch(route, /class="preview-list codex-client-list"/);
  assert.doesNotMatch(route, /class="preview-list"/);
  assert.doesNotMatch(route, /class="doctor-list"/);
  assert.doesNotMatch(route, /class="doctor-row"/);
  assert.doesNotMatch(route, /class="install-log/);
  assert.doesNotMatch(route, /class="modal-backdrop"/);
  assert.doesNotMatch(route, /class="modal-panel"/);
  assert.doesNotMatch(route, /class="modal-body"/);
  assert.doesNotMatch(route, /class="modal-actions"/);
  assert.doesNotMatch(route, /class="empty-row"/);
});

test("desktop client recipes keep panel content comfortably inset and controls consistent", () => {
  const pandaConfig = read("panda.config.ts");
  const codexRoute = read("src/routes/CodexClient.svelte");
  const claudeRoute = read("src/routes/ClaudeDesktop.svelte");

  const tabs = recipeBlock(pandaConfig, "desktopClientTabsRecipe", "desktopClientMetricsRecipe");
  const metrics = recipeBlock(pandaConfig, "desktopClientMetricsRecipe", "desktopClientActionsRecipe");
  const actions = recipeBlock(pandaConfig, "desktopClientActionsRecipe", "desktopClientProgressRecipe");
  const progress = recipeBlock(pandaConfig, "desktopClientProgressRecipe", "desktopClientPreviewListRecipe");
  const preview = recipeBlock(pandaConfig, "desktopClientPreviewListRecipe", "desktopClientSettingsListRecipe");
  const settings = recipeBlock(pandaConfig, "desktopClientSettingsListRecipe", "desktopClientModalBackdropRecipe");
  const log = recipeBlock(pandaConfig, "desktopClientLogRecipe", "desktopClientLogViewportRecipe");
  const doctorList = recipeBlock(pandaConfig, "doctorListRecipe", "doctorRowRecipe");

  assert.match(tabs, /gap:\s*"var\(--space-sm\)"/);
  assert.match(tabs, /minHeight:\s*"34px"/);
  assert.match(tabs, /padding:\s*"0 10px"/);

  assert.match(metrics, /gap:\s*"var\(--space-md\)"/);
  assert.match(metrics, /padding:\s*"var\(--space-lg\)"/);
  assert.match(metrics, /"& > div": \{[\s\S]*?padding:\s*"var\(--space-md\)"/);

  assert.match(actions, /gap:\s*"var\(--space-sm\)"/);
  assert.match(actions, /padding:\s*"0 var\(--space-lg\) var\(--space-lg\)"/);
  assert.match(actions, /"& button": \{[\s\S]*?minHeight:\s*"33px"/);

  assert.match(progress, /margin:\s*"0 var\(--space-lg\) var\(--space-lg\)"/);
  assert.doesNotMatch(progress, /var\(--space-sm\).*2px/);

  assert.match(preview, /gap:\s*"var\(--space-md\)"/);
  assert.match(preview, /padding:\s*"var\(--space-lg\)"/);
  assert.match(preview, /"& > div": \{[\s\S]*?padding:\s*"var\(--space-md\)"/);

  assert.match(settings, /gap:\s*"var\(--space-md\)"/);
  assert.match(settings, /padding:\s*"var\(--space-lg\)"/);
  assert.match(settings, /minHeight:\s*"40px"/);

  assert.match(log, /margin:\s*"var\(--space-lg\)"/);
  assert.match(log, /padding:\s*"var\(--space-md\)"/);
  assert.match(doctorList, /gap:\s*"var\(--space-md\)"/);
  assert.match(doctorList, /padding:\s*"var\(--space-lg\)"/);

  for (const route of [codexRoute, claudeRoute]) {
    assert.match(route, /routeStackRecipe\(\{ width: "desktopClient" \}\)/);
    assert.match(route, /desktopClientSettingsListRecipe/);
    assert.match(route, /desktopClientMetricsRecipe\(\)/);
    assert.match(route, /desktopClientActionsRecipe\(\)/);
    assert.match(route, /desktopClientProgressRecipe\(\)/);
    assert.match(route, /desktopClientPreviewListRecipe\(\)/);
  }
});

test("migrated desktop client globals are removed from the legacy stylesheet", () => {
  const styles = read("src/styles.css");

  assert.doesNotMatch(styles, /\.install-kind-tabs/);
  assert.doesNotMatch(styles, /\.install-progress/);
  assert.doesNotMatch(styles, /\.progress-copy/);
  assert.doesNotMatch(styles, /\.progress-track/);
  assert.doesNotMatch(styles, /\.progress-fill/);
  assert.doesNotMatch(styles, /\.doctor-list/);
  assert.doesNotMatch(styles, /\.doctor-row/);
  assert.doesNotMatch(styles, /\.install-log/);
  assert.doesNotMatch(styles, /\.live-install-log/);
  assert.doesNotMatch(styles, /\.install-log-viewport/);
  assert.doesNotMatch(styles, /\.install-log-stage/);
  assert.doesNotMatch(styles, /\.native-diff\b/);
  assert.doesNotMatch(styles, /\.native-diff-heading/);
  assert.doesNotMatch(styles, /\.native-write-toggle/);
});

test("legacy inline notice globals are removed", () => {
  const styles = read("src/styles.css");
  const productionRoutes = [
    "src/routes/Dashboard.svelte",
    "src/routes/CodexClient.svelte",
    "src/routes/ClaudeDesktop.svelte",
    "src/routes/Profiles.svelte",
    "src/routes/SetupWizard.svelte",
    "src/routes/Gateway.svelte",
    "src/routes/Settings.svelte",
    "src/routes/TerminalPanel.svelte"
  ].map((file) => read(file)).join("\n");

  assert.doesNotMatch(productionRoutes, /class="inline-error/);
  assert.doesNotMatch(productionRoutes, /class="inline-success/);
  assert.doesNotMatch(productionRoutes, /class="error-banner/);
  assert.doesNotMatch(productionRoutes, /class="env-conflict-banner/);
  assert.doesNotMatch(productionRoutes, /class="conflict-chip-list/);
  assert.doesNotMatch(styles, /\.inline-error/);
  assert.doesNotMatch(styles, /\.inline-success/);
  assert.doesNotMatch(styles, /\.error-banner/);
  assert.doesNotMatch(styles, /\.env-conflict-banner/);
  assert.doesNotMatch(styles, /\.conflict-chip-list/);
});

test("unused preview-list global compatibility styles are removed", () => {
  const styles = read("src/styles.css");
  const productionRoutes = [
    "src/routes/Dashboard.svelte",
    "src/routes/CodexClient.svelte",
    "src/routes/ClaudeDesktop.svelte",
    "src/routes/Profiles.svelte",
    "src/routes/SetupWizard.svelte",
    "src/routes/Gateway.svelte",
    "src/routes/Settings.svelte",
    "src/routes/TerminalPanel.svelte"
  ].map((file) => read(file)).join("\n");

  assert.doesNotMatch(productionRoutes, /class="preview-list/);
  assert.doesNotMatch(styles, /\.preview-list/);
});

test("shared utility globals for spinning icons are removed and visible eyebrows are not rendered", () => {
  const styles = read("src/styles.css");
  const pandaConfig = read("panda.config.ts");
  const productionSurfaces = [
    "src/components/ToolStatusCard.svelte",
    "src/routes/Dashboard.svelte",
    "src/routes/CodexClient.svelte",
    "src/routes/ClaudeDesktop.svelte",
    "src/routes/Profiles.svelte",
    "src/routes/SetupWizard.svelte",
    "src/routes/Gateway.svelte",
    "src/routes/Settings.svelte"
  ].map((file) => read(file)).join("\n");

  assert.doesNotMatch(pandaConfig, /eyebrowRecipe/);
  assert.match(pandaConfig, /spinRecipe/);
  assert.doesNotMatch(productionSurfaces, /eyebrowRecipe/);
  assert.match(productionSurfaces, /spinRecipe\(\)/);
  assert.doesNotMatch(productionSurfaces, /class="eyebrow"/);
  assert.doesNotMatch(productionSurfaces, /class="spin"/);
  assert.doesNotMatch(productionSurfaces, /\? "spin" : ""/);
  assert.doesNotMatch(styles, /\.eyebrow\b/);
  assert.doesNotMatch(styles, /\.spin\b/);
});

test("Profiles modal surfaces use Panda recipes", () => {
  const route = read("src/routes/Profiles.svelte");
  const styles = read("src/styles.css");
  const modalMarkup = route.split("{#if pendingUsageProfile}")[1];

  assert.match(route, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(route, /profileDiffPanelRecipe/);
  assert.match(route, /profileDiffHeadingRecipe/);
  assert.match(route, /profileDiffRowRecipe/);
  assert.match(route, /profileInlineNoticeRecipe/);
  assert.match(route, /profileUsageOfficialPanelRecipe/);
  assert.match(route, /desktopClientModalBackdropRecipe/);
  assert.match(route, /desktopClientModalPanelRecipe/);
  assert.match(route, /desktopClientPreviewListRecipe/);
  assert.match(route, /nativeToggleRecipe/);
  assert.match(route, /actionButtonRecipe/);
  assert.match(route, /emptyRowRecipe/);

  assert.doesNotMatch(modalMarkup, /class="modal-backdrop"/);
  assert.doesNotMatch(modalMarkup, /class="modal-panel/);
  assert.doesNotMatch(modalMarkup, /class="modal-body"/);
  assert.doesNotMatch(modalMarkup, /class="modal-actions"/);
  assert.doesNotMatch(modalMarkup, /class="preview-list/);
  assert.doesNotMatch(modalMarkup, /class="native-write-toggle/);
  assert.doesNotMatch(modalMarkup, /class="native-diff/);
  assert.doesNotMatch(modalMarkup, /class="native-diff-heading"/);
  assert.doesNotMatch(modalMarkup, /class="native-diff-row"/);
  assert.doesNotMatch(modalMarkup, /class="usage-official-panel"/);
  assert.doesNotMatch(modalMarkup, /class="inline-error"/);
  assert.doesNotMatch(modalMarkup, /class="inline-success"/);

  assert.doesNotMatch(styles, /\.modal-panel\.wide-modal/);
  assert.doesNotMatch(styles, /\.modal-panel\.usage-modal/);
  assert.doesNotMatch(styles, /\.native-diff-list/);
  assert.doesNotMatch(styles, /\.native-diff-row/);
  assert.doesNotMatch(styles, /\.usage-toggle/);
  assert.doesNotMatch(styles, /\.usage-official-panel/);
  assert.doesNotMatch(styles, /\.usage-result-panel/);
  assert.doesNotMatch(styles, /\.compact-conflict-list/);
  assert.doesNotMatch(styles, /\.env-conflict-panel/);
  assert.doesNotMatch(styles, /\.auth-conflict-panel/);
});

test("Profiles tool switcher surfaces use Panda recipes", () => {
  const route = read("src/routes/Profiles.svelte");
  const styles = read("src/styles.css");
  const mainMarkup = route.split("{#if pendingUsageProfile}")[0];

  assert.match(route, /profileModeLayoutRecipe/);
  assert.match(route, /profileToolSwitcherRecipe/);
  assert.match(route, /profileToolTabsRecipe/);

  assert.doesNotMatch(mainMarkup, /class="profile-mode-layout"/);
  assert.doesNotMatch(mainMarkup, /class="panel-band profile-tool-switcher"/);
  assert.doesNotMatch(mainMarkup, /class="profile-tool-tabs"/);
  assert.doesNotMatch(mainMarkup, /class:selected=\{selectedToolId === group\.id\}/);
  assert.match(mainMarkup, /data-selected=\{selectedToolId === group\.id\}/);

  assert.doesNotMatch(styles, /\.profile-mode-layout/);
  assert.doesNotMatch(styles, /\.profile-tool-switcher/);
  assert.doesNotMatch(styles, /\.profile-tool-tabs/);
});

test("Profiles main card list uses Panda recipes", () => {
  const route = read("src/routes/Profiles.svelte");
  const styles = read("src/styles.css");
  const mainMarkup = route.split("{#if pendingUsageProfile}")[0];

  assert.match(route, /profileToolSectionRecipe/);
  assert.match(route, /profileGridRecipe/);
  assert.match(route, /profileSortableRowRecipe/);
  assert.match(route, /profileCardRecipe/);
  assert.match(route, /profileCardMainRecipe/);
  assert.match(route, /profileDragHandleRecipe/);
  assert.match(route, /profileAvatarRecipe/);
  assert.match(route, /profileIdentityRecipe/);
  assert.match(route, /profileCardStatusRecipe/);
  assert.match(route, /profileCardActionsRecipe/);
  assert.match(route, /data-profile-card/);
  assert.match(route, /data-sortable-active=\{sortableActiveId === profile\.id\}/);
  assert.match(route, /data-active=\{isActive\}/);
  assert.match(route, /data-builtin=\{profile\.isBuiltin\}/);
  assert.match(route, /data-drag-active=\{sortableActiveId === profile\.id\}/);
  assert.match(route, /querySelector\("\[data-profile-card\]"\)/);
  assert.match(
    route,
    /<div class=\{profileAvatarRecipe\(\)\} data-profile-avatar aria-hidden="true">[\s\S]*?<ToolIcon toolId=\{profile\.app\} label=\{profileDisplayName\(profile\)\} variant="heading" \/>/
  );

  const pandaConfig = read("panda.config.ts");
  const avatarRecipe = recipeBlock(pandaConfig, "profileAvatarRecipe", "profileIdentityRecipe");
  assert.match(avatarRecipe, /"& \[data-tool-icon-variant\]": \{[\s\S]*?display:\s*"grid"/);
  assert.match(avatarRecipe, /"& \[data-tool-icon-variant\]": \{[\s\S]*?flex:\s*"0 0 100%"/);
  assert.match(avatarRecipe, /"& \[data-tool-icon-variant\]": \{[\s\S]*?minWidth:\s*"100%"/);
  assert.match(avatarRecipe, /"& \[data-tool-icon-variant\]": \{[\s\S]*?minHeight:\s*"100%"/);
  assert.match(avatarRecipe, /"& \[data-tool-icon-variant\]": \{[\s\S]*?placeItems:\s*"center"/);
  assert.match(avatarRecipe, /"&:has\(\[data-tool-icon-tone='hermes'\]\)": \{[\s\S]*?overflow:\s*"visible"/);
  assert.match(avatarRecipe, /"& \[data-tool-icon-tone='hermes'\] img": \{[\s\S]*?width:\s*"36px"[\s\S]*?height:\s*"36px"/);

  assert.doesNotMatch(mainMarkup, /class="panel-band profile-tool-section"/);
  assert.doesNotMatch(mainMarkup, /class="profile-grid"/);
  assert.doesNotMatch(mainMarkup, /class="profile-sortable-row"/);
  assert.doesNotMatch(mainMarkup, /class="profile-card compact-profile-card"/);
  assert.doesNotMatch(mainMarkup, /class="profile-card-main"/);
  assert.doesNotMatch(mainMarkup, /class="profile-drag-handle"/);
  assert.doesNotMatch(mainMarkup, /class="profile-avatar"/);
  assert.doesNotMatch(mainMarkup, /class="profile-identity"/);
  assert.doesNotMatch(mainMarkup, /class="profile-card-status"/);
  assert.doesNotMatch(mainMarkup, /class="card-actions"/);
  assert.doesNotMatch(mainMarkup, /class:active-profile/);
  assert.doesNotMatch(mainMarkup, /class:builtin-profile/);
  assert.doesNotMatch(mainMarkup, /class:sortable-active-card/);
  assert.doesNotMatch(mainMarkup, /class:sortable-active-row/);
  assert.doesNotMatch(mainMarkup, /class="primary-button"/);
  assert.doesNotMatch(mainMarkup, /class="icon-button/);

  assert.doesNotMatch(styles, /\.profile-grid/);
  assert.doesNotMatch(styles, /\.profile-tool-section/);
  assert.doesNotMatch(styles, /\.profile-sortable-row/);
  assert.doesNotMatch(styles, /\.compact-profile-card/);
  assert.doesNotMatch(styles, /\.profile-card(?!-)/);
  assert.doesNotMatch(styles, /\.profile-card-main/);
  assert.doesNotMatch(styles, /\.profile-drag-handle/);
  assert.doesNotMatch(styles, /\.profile-card-status/);
  assert.doesNotMatch(styles, /\.sortable-active-card/);
  assert.doesNotMatch(styles, /\.sortable-active-row/);
});

test("Profiles edit and usage forms use Panda recipes", () => {
  const route = read("src/routes/Profiles.svelte");
  const styles = read("src/styles.css");
  const modalMarkup = route.split("{#if pendingUsageProfile}")[1];

  assert.match(route, /profileEmbeddedStackRecipe/);
  assert.match(route, /profileFormGridRecipe/);
  assert.match(route, /profileFieldErrorRecipe/);
  assert.match(route, /profileIconEditorRecipe/);
  assert.match(route, /profileIconActionsRecipe/);
  assert.match(route, /profileUsageTemplateRowRecipe/);
  assert.match(route, /profileUsageCodeFieldRecipe/);
  assert.match(route, /profileUsageResultGridRecipe/);
  assert.match(route, /profileUsageResultCardRecipe/);
  assert.match(route, /profileWriteContentPreviewRecipe/);
  assert.match(route, /data-selected=\{usageForm\.templateType === option\.id\}/);
  assert.match(route, /data-invalid=\{item\.isValid === false\}/);
  assert.match(route, /data-usage-balance/);

  assert.doesNotMatch(route, /"embedded-profile-stack"/);
  assert.doesNotMatch(modalMarkup, /class="form-grid edit-profile-form/);
  assert.doesNotMatch(modalMarkup, /class="profile-icon-editor"/);
  assert.doesNotMatch(modalMarkup, /class="field-error"/);
  assert.doesNotMatch(modalMarkup, /class="profile-icon-actions"/);
  assert.doesNotMatch(modalMarkup, /class="usage-template-row"/);
  assert.doesNotMatch(modalMarkup, /class:selected=\{usageForm\.templateType === option\.id\}/);
  assert.doesNotMatch(modalMarkup, /class="usage-code-field"/);
  assert.doesNotMatch(modalMarkup, /class="usage-result-grid"/);
  assert.doesNotMatch(modalMarkup, /class="usage-result-card"/);
  assert.doesNotMatch(modalMarkup, /class:invalid-usage/);
  assert.doesNotMatch(modalMarkup, /class="usage-balance-value"/);
  assert.doesNotMatch(modalMarkup, /class="write-content-preview"/);

  assert.doesNotMatch(styles, /\.embedded-profile-stack/);
  assert.doesNotMatch(styles, /\.form-grid/);
  assert.doesNotMatch(styles, /\.field-error/);
  assert.doesNotMatch(styles, /\.edit-profile-form/);
  assert.doesNotMatch(styles, /\.profile-icon-editor/);
  assert.doesNotMatch(styles, /\.profile-icon-actions/);
  assert.doesNotMatch(styles, /\.edit-mode-field/);
  assert.doesNotMatch(styles, /\.edit-mode-toggle/);
  assert.doesNotMatch(styles, /\.usage-template-row/);
  assert.doesNotMatch(styles, /\.usage-form/);
  assert.doesNotMatch(styles, /\.usage-code-field/);
  assert.doesNotMatch(styles, /\.usage-result-grid/);
  assert.doesNotMatch(styles, /\.usage-result-card/);
  assert.doesNotMatch(styles, /\.usage-balance-value/);
  assert.doesNotMatch(styles, /\.write-content-preview/);
});

test("Setup Wizard route surfaces use Panda recipes", () => {
  const route = read("src/routes/SetupWizard.svelte");
  const styles = read("src/styles.css");

  assert.match(route, /routeStackRecipe\(\{ width: "full" \}\)/);
  assert.match(route, /topStripRecipe\(\)/);
  assert.match(route, /wizardActionsRecipe/);
  assert.match(route, /wizardStepperRecipe/);
  assert.match(route, /wizardStepItemRecipe/);
  assert.match(route, /wizardPanelRecipe/);
  assert.match(route, /wizardStepContentRecipe/);
  assert.match(route, /wizardChoiceGridRecipe/);
  assert.match(route, /wizardChoiceButtonRecipe/);
  assert.match(route, /wizardModeChoiceRecipe/);
  assert.match(route, /wizardFormGridRecipe/);
  assert.match(route, /wizardInlineNoticeRecipe/);
  assert.match(route, /wizardCodexAuthCardRecipe/);
  assert.match(route, /wizardSecurityNoteRecipe/);
  assert.match(route, /wizardPreviewBoxRecipe/);
  assert.match(route, /wizardPreviewHeadingRecipe/);
  assert.match(route, /wizardWritePreviewListRecipe/);
  assert.match(route, /wizardWritePreviewRowRecipe/);
  assert.match(route, /wizardWritePreviewMetaRecipe/);
  assert.match(route, /wizardWriteContentPreviewRecipe/);
  assert.match(route, /wizardPreviewWarningsRecipe/);
  assert.match(route, /data-step-state=\{index === currentStep \? "active" : index < currentStep \? "done" : "idle"\}/);
  assert.match(route, /data-selected=\{selectedTool === tool\.id\}/);
  assert.match(route, /data-selected=\{profileMode === "config"\}/);
  assert.match(route, /data-selected=\{codexOAuthConfig\}/);

  assert.doesNotMatch(route, /class="route-stack wizard-route"/);
  assert.doesNotMatch(route, /class="wizard-actions"/);
  assert.doesNotMatch(route, /class="stepper"/);
  assert.doesNotMatch(route, /class="stepper-item"/);
  assert.doesNotMatch(route, /class:active=\{index === currentStep\}/);
  assert.doesNotMatch(route, /class:done=\{index < currentStep\}/);
  assert.doesNotMatch(route, /class="panel-band wizard-panel"/);
  assert.doesNotMatch(route, /class="wizard-step-content"/);
  assert.doesNotMatch(route, /class="field-grid choices wizard-tool-choices"/);
  assert.doesNotMatch(route, /class="field-grid choices compact-choices"/);
  assert.doesNotMatch(route, /class:selected/);
  assert.doesNotMatch(route, /class="form-grid"/);
  assert.doesNotMatch(route, /class="field-error"/);
  assert.doesNotMatch(route, /class="codex-auth-card"/);
  assert.doesNotMatch(route, /class="button-row"/);
  assert.doesNotMatch(route, /class="security-note"/);
  assert.doesNotMatch(route, /class="preview-box"/);
  assert.doesNotMatch(route, /class="preview-heading"/);
  assert.doesNotMatch(route, /class="preview-list write-preview-list"/);
  assert.doesNotMatch(route, /class="write-preview-row"/);
  assert.doesNotMatch(route, /class="write-preview-meta"/);
  assert.doesNotMatch(route, /class="write-content-preview"/);
  assert.doesNotMatch(route, /class="preview-warnings"/);
  assert.doesNotMatch(route, /class="inline-error"/);
  assert.doesNotMatch(route, /class="inline-success"/);
  assert.doesNotMatch(route, /class="primary-button"/);
  assert.doesNotMatch(route, /class="secondary-button"/);

  assert.doesNotMatch(styles, /\.stepper/);
  assert.doesNotMatch(styles, /\.stepper-item/);
  assert.doesNotMatch(styles, /\.wizard-panel/);
  assert.doesNotMatch(styles, /\.wizard-step-content/);
  assert.doesNotMatch(styles, /\.wizard-tool-choices/);
  assert.doesNotMatch(styles, /\.wizard-mode-choice/);
  assert.doesNotMatch(styles, /\.codex-auth-card/);
  assert.doesNotMatch(styles, /\.preview-box/);
  assert.doesNotMatch(styles, /\.write-preview-list/);
  assert.doesNotMatch(styles, /\.write-preview-row/);
  assert.doesNotMatch(styles, /\.write-preview-meta/);
  assert.doesNotMatch(styles, /\.preview-warnings/);
});

test("Gateway route surfaces use Panda recipes", () => {
  const route = read("src/routes/Gateway.svelte");
  const styles = read("src/styles.css");

  assert.match(route, /import \{ css, cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(route, /routeStackRecipe\(\{ width: "full" \}\)/);
  assert.match(route, /topStripRecipe\(\)/);
  assert.match(route, /gatewayHeroRecipe/);
  assert.match(route, /topActionsRecipe\(\)/);
  assert.match(route, /gatewayPanelRecipe/);
  assert.match(route, /gatewayMetricsRecipe/);
  assert.match(route, /gatewaySettingRowRecipe/);
  assert.match(route, /gatewaySegmentedRecipe/);
  assert.match(route, /gatewayInlineErrorRecipe/);
  assert.match(route, /gatewayRequestPanelRecipe/);
  assert.match(route, /gatewayRequestListRecipe/);
  assert.match(route, /gatewayRequestRowRecipe/);
  assert.match(route, /panelRecipe\(\)/);
  assert.match(route, /sectionHeadingRecipe\(\{ compact: true \}\)/);
  assert.match(route, /actionButtonRecipe\(\{ tone: "primary" \}\)/);
  assert.match(route, /emptyRowRecipe\(\)/);
  assert.match(route, /data-selected=\{privacyFilterMode === mode\.value\}/);
  assert.match(route, /data-privacy-action=\{entry\.privacyFilterAction\}/);

  assert.doesNotMatch(route, /class="route-stack gateway-route"/);
  assert.doesNotMatch(route, /class=\{`top-strip gateway-hero \$\{gatewayTone\}`\}/);
  assert.doesNotMatch(route, /class="gateway-actions"/);
  assert.doesNotMatch(route, /class="primary-button"/);
  assert.doesNotMatch(route, /class="secondary-button"/);
  assert.doesNotMatch(route, /class="panel-band gateway-panel"/);
  assert.doesNotMatch(route, /class="gateway-metrics"/);
  assert.doesNotMatch(route, /class="gateway-setting-row"/);
  assert.doesNotMatch(route, /class="gateway-segmented"/);
  assert.doesNotMatch(route, /class:selected/);
  assert.doesNotMatch(route, /class="sidebar-gateway-error"/);
  assert.doesNotMatch(route, /class="panel-band gateway-request-panel"/);
  assert.doesNotMatch(route, /class="section-heading compact-heading"/);
  assert.doesNotMatch(route, /class="empty-state"/);
  assert.doesNotMatch(route, /class="gateway-request-list"/);
  assert.doesNotMatch(route, /class=\{`gateway-request-row privacy-\$\{entry\.privacyFilterAction\}`\}/);
  assert.doesNotMatch(route, /class="gateway-request-cell"/);
  assert.doesNotMatch(route, /class="gateway-request-time gateway-request-cell"/);

  assert.doesNotMatch(styles, /\.gateway-panel/);
  assert.doesNotMatch(styles, /\.gateway-route/);
  assert.doesNotMatch(styles, /\.gateway-hero/);
  assert.doesNotMatch(styles, /\.gateway-actions/);
  assert.doesNotMatch(styles, /\.gateway-metrics/);
  assert.doesNotMatch(styles, /\.gateway-setting-row/);
  assert.doesNotMatch(styles, /\.gateway-segmented/);
  assert.doesNotMatch(styles, /\.gateway-request-panel/);
  assert.doesNotMatch(styles, /\.gateway-request-list/);
  assert.doesNotMatch(styles, /\.gateway-request-row/);
  assert.doesNotMatch(styles, /\.gateway-request-time/);
  assert.doesNotMatch(styles, /\.sidebar-gateway-error/);
});

test("Settings route surfaces use Panda recipes", () => {
  const route = read("src/routes/Settings.svelte");
  const styles = read("src/styles.css");

  assert.match(route, /import \{ cx \} from "\.\.\/\.\.\/styled-system\/css";/);
  assert.match(route, /settingsListRecipe/);
  assert.match(route, /settingsRowRecipe/);
  assert.match(route, /settingsRowValueRecipe/);
  assert.match(route, /settingsAboutPanelRecipe/);
  assert.match(route, /settingsAboutContentRecipe/);
  assert.match(route, /settingsAboutSummaryRecipe/);
  assert.match(route, /settingsAboutMarkRecipe/);
  assert.match(route, /settingsAboutTitleRecipe/);
  assert.match(route, /settingsAboutUpdateRecipe/);
  assert.match(route, /settingsUpdatePillRecipe/);
  assert.match(route, /profileInlineNoticeRecipe\(\{ tone: "error" \}\)/);
  assert.match(route, /panelRecipe\(\)/);
  assert.match(route, /sectionHeadingRecipe\(\{ compact: true \}\)/);
  assert.match(route, /actionButtonRecipe\(\)/);

  assert.doesNotMatch(route, /class="inline-error"/);
  assert.doesNotMatch(route, /class="panel-band settings-list"/);
  assert.doesNotMatch(route, /class="settings-row"/);
  assert.doesNotMatch(route, /class="settings-row settings-toggle-row"/);
  assert.doesNotMatch(route, /class="settings-row-value"/);
  assert.doesNotMatch(route, /class="panel-band about-panel"/);
  assert.doesNotMatch(route, /class="section-heading compact"/);
  assert.doesNotMatch(route, /class="about-content"/);
  assert.doesNotMatch(route, /class="about-summary"/);
  assert.doesNotMatch(route, /class="brand-mark about-mark"/);
  assert.doesNotMatch(route, /class="about-title"/);
  assert.doesNotMatch(route, /class="about-update"/);
  assert.doesNotMatch(route, /class=\{`pill \$\{updateStatusTone\}`\}/);
  assert.doesNotMatch(route, /class="secondary-button"/);
  assert.doesNotMatch(route, /class="settings-row about-row"/);

  assert.doesNotMatch(styles, /\.settings-list/);
  assert.doesNotMatch(styles, /\.settings-row/);
  assert.doesNotMatch(styles, /\.settings-row-value/);
  assert.doesNotMatch(styles, /\.settings-toggle-row/);
  assert.doesNotMatch(styles, /\.about-panel/);
  assert.doesNotMatch(styles, /\.about-content/);
  assert.doesNotMatch(styles, /\.about-summary/);
  assert.doesNotMatch(styles, /\.about-title/);
  assert.doesNotMatch(styles, /\.about-mark/);
  assert.doesNotMatch(styles, /\.about-row/);
  assert.doesNotMatch(styles, /\.about-update/);
  assert.doesNotMatch(styles, /\.pill\b/);
});

test("Terminal Panel route surfaces use Panda recipes", () => {
  const route = read("src/routes/TerminalPanel.svelte");

  assert.match(route, /import \{ actionButtonRecipe, terminalPanelActionsRecipe, terminalPanelFrameRecipe, terminalPanelHeaderRecipe, terminalPanelRecipe, terminalPanelStatusRecipe, terminalPanelTitleRecipe \} from "\.\.\/\.\.\/styled-system\/recipes";/);
  assert.match(route, /terminalPanelRecipe\(\)/);
  assert.match(route, /terminalPanelHeaderRecipe\(\)/);
  assert.match(route, /terminalPanelTitleRecipe\(\)/);
  assert.match(route, /terminalPanelStatusRecipe\(\{ tone: terminalStatusTone \}\)/);
  assert.match(route, /terminalPanelActionsRecipe\(\)/);
  assert.match(route, /terminalPanelFrameRecipe\(\)/);
  assert.match(route, /actionButtonRecipe\(\)/);
  assert.match(route, /let terminalStatusTone: TerminalStatusTone = "idle";/);
  assert.match(route, /data-terminal-frame/);

  assert.doesNotMatch(route, /class="terminal-panel"/);
  assert.doesNotMatch(route, /class="terminal-panel-header"/);
  assert.doesNotMatch(route, /class="terminal-panel-title"/);
  assert.doesNotMatch(route, /class="terminal-panel-status"/);
  assert.doesNotMatch(route, /class="terminal-error"/);
  assert.doesNotMatch(route, /class="terminal-running"/);
  assert.doesNotMatch(route, /class="terminal-exited"/);
  assert.doesNotMatch(route, /class="terminal-idle"/);
  assert.doesNotMatch(route, /class="terminal-panel-actions"/);
  assert.doesNotMatch(route, /class="secondary-button"/);
  assert.doesNotMatch(route, /class="terminal-panel-frame"/);
  assert.doesNotMatch(route, /<style>/);
});

test("App shell and navigation use Panda recipes", () => {
  const app = read("src/App.svelte");
  const styles = read("src/styles.css");

  assert.match(app, /import \{ appBrandMarkRecipe, appBrandRecipe, appErrorBannerRecipe, appNavButtonRecipe, appNavLabelRecipe, appNavRecipe, appNavUpdateDotRecipe, appRouteTransitionRecipe, appShellRecipe, appSidebarRecipe, appWorkspaceRecipe \} from "\.\.\/styled-system\/recipes";/);
  assert.match(app, /appShellRecipe\(\)/);
  assert.match(app, /appSidebarRecipe\(\)/);
  assert.match(app, /appBrandRecipe\(\)/);
  assert.match(app, /appBrandMarkRecipe\(\)/);
  assert.match(app, /appNavRecipe\(\)/);
  assert.doesNotMatch(app, /appNavSectionTitleRecipe/);
  assert.doesNotMatch(app, />Workspace</);
  assert.match(app, /appNavButtonRecipe\(\)/);
  assert.match(app, /data-active=\{route === item\.id\}/);
  assert.match(app, /appNavLabelRecipe\(\)/);
  assert.match(app, /appNavUpdateDotRecipe\(\)/);
  assert.match(app, /appWorkspaceRecipe\(\)/);
  assert.match(app, /appErrorBannerRecipe\(\)/);
  assert.match(app, /appRouteTransitionRecipe\(\)/);

  assert.doesNotMatch(app, /class="app-shell"/);
  assert.doesNotMatch(app, /class="sidebar"/);
  assert.doesNotMatch(app, /class="brand"/);
  assert.doesNotMatch(app, /class="brand-mark"/);
  assert.doesNotMatch(app, /class="sidebar-nav"/);
  assert.doesNotMatch(app, /class="nav-section-title"/);
  assert.doesNotMatch(app, /class:active=\{route === item\.id\}/);
  assert.doesNotMatch(app, /class="nav-item-label"/);
  assert.doesNotMatch(app, /class="nav-update-dot"/);
  assert.doesNotMatch(app, /class="workspace"/);
  assert.doesNotMatch(app, /class="error-banner"/);
  assert.doesNotMatch(app, /class="route-transition"/);

  assert.doesNotMatch(styles, /\.app-shell/);
  assert.doesNotMatch(styles, /\.sidebar\b/);
  assert.doesNotMatch(styles, /\.brand\b/);
  assert.doesNotMatch(styles, /\.brand-mark/);
  assert.doesNotMatch(styles, /\.brand-logo/);
  assert.doesNotMatch(styles, /\.sidebar-nav/);
  assert.doesNotMatch(styles, /\.nav-section-title/);
  assert.doesNotMatch(styles, /nav button/);
  assert.doesNotMatch(styles, /\.nav-item-label/);
  assert.doesNotMatch(styles, /\.nav-update-dot/);
  assert.doesNotMatch(styles, /\.workspace/);
  assert.doesNotMatch(styles, /\.route-transition/);
});

test("route shell primitives use Panda recipes", () => {
  const routeFiles = [
    "src/routes/Dashboard.svelte",
    "src/routes/CodexClient.svelte",
    "src/routes/ClaudeDesktop.svelte",
    "src/routes/Settings.svelte",
    "src/routes/Profiles.svelte",
    "src/routes/SetupWizard.svelte",
    "src/routes/Gateway.svelte"
  ];
  const routes = routeFiles.map((file) => read(file)).join("\n");
  const styles = read("src/styles.css");

  for (const file of routeFiles) {
    const route = read(file);
    assert.match(route, /routeStackRecipe/);
    assert.match(route, /topStripRecipe/);
  }

  assert.match(routes, /routeStackRecipe\(\{ width: "desktopClient" \}\)/);
  assert.match(routes, /routeStackRecipe\(\{ width: "full" \}\)/);
  assert.match(routes, /topStripRecipe\(\{ compact: true \}\)/);
  assert.match(routes, /topActionsRecipe\(\)/);
  assert.match(routes, /statusStripRecipe\(\)/);
  assert.match(routes, /panelRecipe\(\)/);

  assert.doesNotMatch(routes, /class="route-stack/);
  assert.doesNotMatch(routes, /"route-stack"/);
  assert.doesNotMatch(routes, /class="top-strip/);
  assert.doesNotMatch(routes, /"top-strip"/);
  assert.doesNotMatch(routes, /class="compact-top-strip"/);
  assert.doesNotMatch(routes, /class="panel-band"/);
  assert.doesNotMatch(routes, /class="top-actions"/);
  assert.doesNotMatch(routes, /class="status-strip"/);
  assert.doesNotMatch(routes, /class="primary-button"/);
  assert.doesNotMatch(routes, /class="secondary-button"/);

  assert.doesNotMatch(styles, /\.route-stack/);
  assert.doesNotMatch(styles, /\.top-strip/);
  assert.doesNotMatch(styles, /\.compact-top-strip/);
  assert.doesNotMatch(styles, /\.panel-band/);
  assert.doesNotMatch(styles, /\.top-actions/);
  assert.doesNotMatch(styles, /\.status-strip/);
  assert.doesNotMatch(styles, /\.primary-button/);
  assert.doesNotMatch(styles, /\.secondary-button/);
  assert.doesNotMatch(styles, /\.icon-button/);
  assert.doesNotMatch(styles, /\.section-heading/);
});
