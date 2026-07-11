<script lang="ts">
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import AppIcon from "../components/AppIcon.svelte";
  import DismissibleNotice from "../components/DismissibleNotice.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import { css, cx } from "../../styled-system/css";
  import {
    actionButtonRecipe,
    dashboardCardActionsRecipe,
    dashboardCardMainRecipe,
    dashboardCardRecipe,
    dashboardCardStateRecipe,
    dashboardCommandBoxRecipe,
    dashboardCommandListRecipe,
    dashboardDirectoryFieldRecipe,
    dashboardEnvConflictRecipe,
    dashboardGridRecipe,
    dashboardInfoGridRecipe,
    dashboardLaunchEmptyRecipe,
    dashboardLaunchGridRecipe,
    dashboardLaunchHeadingRecipe,
    dashboardLaunchOptionRecipe,
    dashboardLaunchSectionRecipe,
    dashboardLogRecipe,
    dashboardLogStageRecipe,
    dashboardLogViewportRecipe,
    dashboardModalActionsRecipe,
    dashboardModalBackdropRecipe,
    dashboardModalBodyRecipe,
    dashboardModalPanelRecipe,
    dashboardOverflowRecipe,
    dashboardPreviewListRecipe,
    dashboardProgressRecipe,
    dashboardTerminalCardRecipe,
    dashboardTerminalFrameRecipe,
    dashboardTerminalHeaderRecipe,
    emptyRowRecipe,
    iconButtonRecipe,
    noticeRecipe,
    panelRecipe,
    routeStackRecipe,
    sectionHeadingRecipe,
    spinRecipe,
    topActionsRecipe,
    topStripRecipe
  } from "../../styled-system/recipes";
  import {
    getClaudeDesktopLocalizeLaunch,
    installOrUpdateClaudeDesktop,
    launchClaudeDesktopFromDashboard,
    refreshClaudeDesktop
  } from "../lib/claudeDesktopStore";
  import {
    installOrUpdateChatGPTDesktop,
    launchManagedChatGPTDesktop,
    refreshChatGPTDesktop
  } from "../lib/chatgptDesktopStore";
  import {
    applyChatGPTDesktopToolBranding,
    brandChatGPTDesktopText,
    chatgptDesktopGeneration
  } from "../lib/chatgptDesktopBranding";
  import {
    clearClaudeEnvironmentVariables,
    installTool,
    listenInstallTerminalOutput,
    listenToolInstallProgress,
    launchToolExternal,
    planToolLaunch,
    planToolInstall,
    planToolUpdate,
    repairToolPath,
    resizeInstallTerminal,
    startInstallTerminal,
    stopInstallTerminal,
    updateTool,
    writeInstallTerminal
  } from "../lib/api";
  import { t } from "../lib/i18n";
  import { startEmbeddedSession } from "../lib/terminalSessionStore";
  import type {
    DetectionSnapshot,
    InstallTerminalOutput,
    ToolInstallPlan,
    ToolInstallProgress,
    ToolInstallResult,
    LaunchMode,
    ToolLaunchPlan,
    ToolStatus
  } from "../types";
  import { afterUpdate, onDestroy, onMount, tick } from "svelte";

  type DashboardRefreshOptions = { quiet?: boolean; scheduleFollowup?: boolean; showRefreshIndicator?: boolean };
  type DashboardCardAction = "update" | "repair" | "launch" | "configure";
  const dashboardCardActions: DashboardCardAction[] = ["update", "repair", "launch", "configure"];
  const directLaunchFeedbackMs = 700;

  export let snapshot: DetectionSnapshot | null = null;
  export let refreshingExternally = false;
  export let onRefresh: (options?: DashboardRefreshOptions) => void | Promise<void> = () => {};
  export let onToolStatusUpdated: (tool: ToolStatus) => void = () => {};
  export let onConfigureTool: (tool: ToolStatus) => void = () => {};
  export let onOpenTerminal: () => void = () => {};
  export let onNavigateToClient: (toolId: string) => void = () => {};

  let installPlan: ToolInstallPlan | null = null;
  let installResult: ToolInstallResult | null = null;
  let pendingInstallTool: ToolStatus | null = null;
  let installMode: "install" | "update" = "install";
  let installError: string | null = null;
  let toolActionMessage: string | null = null;
  let toolActionError: string | null = null;
  let planningToolId: string | null = null;
  let installingToolId: string | null = null;
  let updatingToolId: string | null = null;
  let repairingToolId: string | null = null;
  let installProgressLogs: ToolInstallProgress[] = [];
  let installProgressLogKeys = new Set<string>();
  let installLogViewport: HTMLDivElement | null = null;
  let installTerminalElement: HTMLDivElement | null = null;
  let terminal: Terminal | null = null;
  let installTerminalSessionId: string | null = null;
  let installTerminalRunning = false;
  let installTerminalExitCode: number | null = null;
  let launchPlan: ToolLaunchPlan | null = null;
  let pendingLaunchTool: ToolStatus | null = null;
  let launchError: string | null = null;
  let planningLaunchToolId: string | null = null;
  let launchingToolId: string | null = null;
  let directLaunchToolIds = new Set<string>();
  let selectedLaunchProfileId: string | null = null;
  let selectedLaunchShellId: string | null = null;
  let launchWorkingDirectory = "";
  let launchMode: LaunchMode = "external";
  let launchTerminalElement: HTMLDivElement | null = null;
  let launchTerminal: Terminal | null = null;
  let launchTerminalSessionId: string | null = null;
  let launchTerminalRunning = false;
  let launchTerminalExitCode: number | null = null;
  let unlistenInstallTerminalOutput: (() => void) | null = null;
  let unlistenInstallProgress: (() => void) | null = null;
  let refreshing = false;
  let clearingClaudeEnv = false;
  $: refreshBusy = refreshing || refreshingExternally;
  // Currently open card-action overflow (<details>) element, if any. Kept as a
  // module-level ref so a single document click listener can close it when the
  // user clicks outside the popover without wiring one listener per card.
  let openOverflowDetails: HTMLDetailsElement | null = null;
  const vscodePluginToolIds = new Set(["codex-vscode", "claude-vscode", "gemini-code-assist"]);

  function clientSortRank(tool: ToolStatus) {
    return tool.id === "chatgpt-desktop" ? 0 : 1;
  }

  function isVscodePluginTool(tool: ToolStatus) {
    return vscodePluginToolIds.has(tool.id);
  }

  function canShowToolLaunch(tool: ToolStatus) {
    return !isVscodePluginTool(tool);
  }

  function hasVsCodeHost(tools: ToolStatus[]) {
    const pluginTools = tools.filter(isVscodePluginTool);
    return (
      pluginTools.length === 0 ||
      pluginTools.some((tool) => tool.installState === "installed" || tool.details !== "Command not found")
    );
  }

  function resolvedToolDetail(tool: ToolStatus) {
    return tool.details?.startsWith("Resolved: ") ? tool.details.replace("Resolved: ", "") : null;
  }

  function isInstallingTool(tool: ToolStatus) {
    const installToolId = installPlanToolFor(tool).id;
    return planningToolId === installToolId || installingToolId === installToolId;
  }

  function isUpdatingTool(tool: ToolStatus) {
    return updatingToolId === tool.id;
  }

  function isRepairingTool(tool: ToolStatus) {
    return repairingToolId === tool.id;
  }

  function isDirectLaunchingTool(tool: ToolStatus, activeDirectLaunchToolIds: Set<string>) {
    return activeDirectLaunchToolIds.has(tool.id);
  }

  function isLaunchingTool(
    tool: ToolStatus,
    activeLaunchingToolId: string | null,
    activeDirectLaunchToolIds: Set<string>
  ) {
    return (
      planningLaunchToolId === tool.id ||
      activeLaunchingToolId === tool.id ||
      isDirectLaunchingTool(tool, activeDirectLaunchToolIds)
    );
  }

  function isToolActionBusy(
    tool: ToolStatus,
    activeLaunchingToolId: string | null,
    activeDirectLaunchToolIds: Set<string>
  ) {
    return (
      isInstallingTool(tool) ||
      isUpdatingTool(tool) ||
      isRepairingTool(tool) ||
      isLaunchingTool(tool, activeLaunchingToolId, activeDirectLaunchToolIds)
    );
  }

  function dashboardActionAvailable(tool: ToolStatus, action: DashboardCardAction) {
    if (action === "update") {
      return tool.installState !== "missing" && Boolean(tool.updateAvailable);
    }
    if (action === "repair") {
      return Boolean(tool.pathRepair);
    }
    if (action === "launch") {
      return tool.installState !== "missing" && canShowToolLaunch(tool);
    }
    return tool.installState !== "missing";
  }

  function dashboardActionIndex(tool: ToolStatus, action: DashboardCardAction) {
    return dashboardCardActions.filter((candidate) => dashboardActionAvailable(tool, candidate)).indexOf(action);
  }

  function availableDashboardActionCount(tool: ToolStatus) {
    return dashboardCardActions.filter((action) => dashboardActionAvailable(tool, action)).length;
  }

  function visibleDashboardActionLimit(_tool: ToolStatus) {
    return 2;
  }

  function shouldShowDashboardOverflow(tool: ToolStatus) {
    return availableDashboardActionCount(tool) > visibleDashboardActionLimit(tool);
  }

  function isDashboardActionVisible(tool: ToolStatus, action: DashboardCardAction) {
    const index = dashboardActionIndex(tool, action);
    return index >= 0 && index < visibleDashboardActionLimit(tool);
  }

  function isDashboardActionOverflowed(tool: ToolStatus, action: DashboardCardAction) {
    const index = dashboardActionIndex(tool, action);
    return index >= 0 && index >= visibleDashboardActionLimit(tool);
  }

  function isManagedDesktopClient(tool: ToolStatus) {
    return tool.id === "chatgpt-desktop" || tool.id === "claude-desktop";
  }

  function installPlanToolFor(tool: ToolStatus) {
    if (tool.id !== "npm") {
      return tool;
    }
    return snapshot?.system.find((candidate) => candidate.id === "node") ?? {
      ...tool,
      id: "node",
      name: "Node.js",
      command: "node"
    };
  }

  function envClearMessage(success: boolean) {
    return $t(success ? "envConflict.clearSuccess" : "envConflict.clearPartial");
  }

  function toolRepairPathMessage(tool: ToolStatus, success: boolean) {
    return $t(success ? "tool.repairPathSuccess" : "tool.repairPathFailed", { name: tool.name });
  }

  function stageLabel(stage: string) {
    if (stage === "prerequisite") {
      return $t("toolInstall.stage.prerequisite");
    }
    if (stage === "update") {
      return $t("common.update");
    }
    return $t("toolInstall.stage.target");
  }

  function hasInstallConsoleOutput(result: ToolInstallResult) {
    return result.stageResults.some((stage) => stage.stdoutTail || stage.stderrTail);
  }

  function messageMentionsVerification(value: string) {
    return value.includes("verification") || value.includes("verified") || value.includes("复检");
  }

  function localizedInstallResultMessage(result: ToolInstallResult) {
    const values = { name: result.toolName };
    if (result.action === "already-installed") {
      return $t("toolInstall.result.alreadyInstalled", values);
    }
    if (result.action === "prerequisites-required") {
      return $t("toolInstall.result.prerequisitesRequired", values);
    }
    if (result.action === "update") {
      if (result.success) {
        return $t("toolInstall.result.updateSuccess", values);
      }
      if (result.exitCode === 0 || messageMentionsVerification(result.message)) {
        return $t("toolInstall.result.updateVerificationFailed", values);
      }
      return $t("toolInstall.result.updateFailed", values);
    }
    if (result.success) {
      return $t("toolInstall.result.installSuccess", values);
    }
    if (result.exitCode === 0 || messageMentionsVerification(result.message)) {
      return $t("toolInstall.result.installVerificationFailed", values);
    }
    return $t("toolInstall.result.installFailed", values);
  }

  function localizedInstallStageMessage(stage: ToolInstallResult["stageResults"][number]) {
    const values = { name: stage.toolName };
    if (stage.stage === "prerequisite") {
      if (stage.success) {
        return $t("toolInstall.result.prerequisiteSuccess", values);
      }
      if (stage.exitCode === 0 || messageMentionsVerification(stage.message)) {
        return $t("toolInstall.result.prerequisiteVerificationFailed", values);
      }
      return $t("toolInstall.result.prerequisiteFailed", values);
    }
    if (stage.stage === "update") {
      if (stage.success) {
        return $t("toolInstall.result.updateSuccess", values);
      }
      if (stage.exitCode === 0 || messageMentionsVerification(stage.message)) {
        return $t("toolInstall.result.updateVerificationFailed", values);
      }
      return $t("toolInstall.result.updateFailed", values);
    }
    if (stage.success) {
      return $t("toolInstall.result.installSuccess", values);
    }
    if (stage.exitCode === 0 || messageMentionsVerification(stage.message)) {
      return $t("toolInstall.result.installVerificationFailed", values);
    }
    return $t("toolInstall.result.installFailed", values);
  }

  function clearInstallProgressLogs() {
    installProgressLogs = [];
    installProgressLogKeys = new Set<string>();
  }

  async function disposeInstallTerminal(stopRunning: boolean) {
    const sessionId = installTerminalSessionId;
    if (stopRunning && sessionId && installTerminalRunning) {
      await stopInstallTerminal({ sessionId }).catch(() => {});
    }
    if (terminal) {
      terminal.dispose();
    }
    terminal = null;
    installTerminalSessionId = null;
    installTerminalRunning = false;
  }

  async function disposeLaunchTerminal(stopRunning: boolean) {
    const sessionId = launchTerminalSessionId;
    if (stopRunning && sessionId && launchTerminalRunning) {
      await stopInstallTerminal({ sessionId }).catch(() => {});
    }
    if (launchTerminal) {
      launchTerminal.dispose();
    }
    launchTerminal = null;
    launchTerminalSessionId = null;
    launchTerminalRunning = false;
  }

  function createDashboardTerminal() {
    return new Terminal({
      convertEol: true,
      cursorBlink: true,
      fontFamily: 'ui-monospace, "SFMono-Regular", Consolas, monospace',
      fontSize: 12,
      rows: 24,
      cols: 100,
      theme: {
        background: "#0f172a",
        foreground: "#e5edf6",
        cursor: "#facc15",
        selectionBackground: "#334155"
      }
    });
  }

  function handleInstallTerminalOutput(output: InstallTerminalOutput) {
    if (!launchTerminalSessionId && launchingToolId && pendingLaunchTool) {
      launchTerminalSessionId = output.sessionId;
    }
    if (launchTerminalSessionId && output.sessionId === launchTerminalSessionId) {
      if (output.data && launchTerminal) {
        launchTerminal.write(output.data);
      }
      if (!output.done) {
        return;
      }
      launchTerminalRunning = false;
      launchingToolId = null;
      launchTerminalExitCode = output.exitCode;
      if (launchTerminal) {
        launchTerminal.write(`\r\n[${$t("toolLaunch.terminalExit", { code: output.exitCode ?? $t("common.none") })}]\r\n`);
      }
      return;
    }

    if (!installTerminalSessionId && installPlan?.interactive && installingToolId) {
      installTerminalSessionId = output.sessionId;
    }
    if (installTerminalSessionId && output.sessionId !== installTerminalSessionId) {
      return;
    }
    if (output.data && terminal) {
      terminal.write(output.data);
    }
    if (!output.done) {
      return;
    }
    installTerminalRunning = false;
    installingToolId = null;
    installTerminalExitCode = output.exitCode;
    if (terminal) {
      terminal.write(`\r\n[${$t("toolInstall.terminalExit", { code: output.exitCode ?? $t("common.none") })}]\r\n`);
    }
    void Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false })).catch(() => {});
  }

  async function openInteractiveInstall() {
    if (!installPlan || installingToolId || installTerminalRunning) {
      return;
    }
    installingToolId = installPlan.toolId;
    installError = null;
    installResult = null;
    installTerminalExitCode = null;
    clearInstallProgressLogs();
    await disposeInstallTerminal(false);
    await tick();

    if (!installTerminalElement) {
      installingToolId = null;
      installError = $t("toolInstall.terminalUnavailable");
      return;
    }

    terminal = createDashboardTerminal();
    terminal.open(installTerminalElement);
    terminal.focus();
    terminal.onData((data: string) => {
      if (!installTerminalSessionId) {
        return;
      }
      void writeInstallTerminal({ sessionId: installTerminalSessionId, data }).catch((err) => {
        installError = err instanceof Error ? err.message : String(err);
      });
    });

    try {
      const result = await startInstallTerminal({
        toolId: installPlan.toolId,
        command: installPlan.command,
        cols: 100,
        rows: 24
      });
      installTerminalSessionId = result.sessionId;
      installTerminalRunning = true;
      await resizeInstallTerminal({ sessionId: result.sessionId, cols: 100, rows: 24 });
    } catch (err) {
      installError = err instanceof Error ? err.message : String(err);
      installingToolId = null;
      await disposeInstallTerminal(false);
    }
  }

  async function stopInteractiveInstall() {
    if (!installTerminalSessionId) {
      return;
    }
    const sessionId = installTerminalSessionId;
    await stopInstallTerminal({ sessionId }).catch((err) => {
      installError = err instanceof Error ? err.message : String(err);
    });
    installTerminalRunning = false;
    installingToolId = null;
    installTerminalExitCode = null;
  }

  function activeInstallToolId() {
    return installingToolId ?? updatingToolId ?? pendingInstallTool?.id ?? null;
  }

  function handleInstallProgress(progress: ToolInstallProgress) {
    const activeToolId = activeInstallToolId();
    if (activeToolId && progress.rootToolId !== activeToolId) {
      return;
    }
    const key = [
      progress.rootToolId,
      progress.toolId,
      progress.stage,
      progress.command,
      progress.stream,
      progress.done ? "done" : "chunk",
      progress.exitCode ?? "",
      progress.chunk
    ].join("\u001f");
    if (installProgressLogKeys.has(key)) {
      return;
    }
    installProgressLogKeys.add(key);
    installProgressLogs = [...installProgressLogs, progress].slice(-240);
  }

  function groupedInstallProgressLogs() {
    const groups: Array<{
      key: string;
      label: string;
      command: string;
      stdout: string;
      stderr: string;
      exitCode: number | null;
      done: boolean;
    }> = [];
    const index = new Map<string, (typeof groups)[number]>();
    for (const item of installProgressLogs) {
      const key = `${item.stage}:${item.toolId}:${item.command}`;
      let group = index.get(key);
      if (!group) {
        group = {
          key,
          label: `${stageLabel(item.stage)} / ${item.toolName}`,
          command: item.command,
          stdout: "",
          stderr: "",
          exitCode: null,
          done: false
        };
        index.set(key, group);
        groups.push(group);
      }
      if (item.stream === "stdout") {
        group.stdout += item.chunk;
      } else if (item.stream === "stderr") {
        group.stderr += item.chunk;
      }
      if (item.done) {
        group.done = true;
        group.exitCode = item.exitCode;
      }
    }
    return groups;
  }

  $: liveInstallLogGroups = groupedInstallProgressLogs();
  $: hasLiveInstallLogs = liveInstallLogGroups.length > 0;

  afterUpdate(() => {
    if (installLogViewport && hasLiveInstallLogs) {
      installLogViewport.scrollTop = installLogViewport.scrollHeight;
    }
  });

  listenToolInstallProgress(handleInstallProgress)
    .then((unlisten) => {
      unlistenInstallProgress = unlisten;
    })
    .catch(() => {});

  listenInstallTerminalOutput(handleInstallTerminalOutput)
    .then((unlisten) => {
      unlistenInstallTerminalOutput = unlisten;
    })
    .catch(() => {});

  // Close any open card-action overflow popover when the user clicks outside it
  // or presses Escape. Registered once at module init and torn down on destroy so
  // every card shares a single listener instead of one popover per card.
  document.addEventListener("click", closeOverflowOnOutsideClick);
  document.addEventListener("keydown", closeOverflowOnEscape);

  const headingCopyClass = css({
    minWidth: 0
  });
  const dashboardCardCopyClass = css({
    minWidth: 0
  });
  const dashboardPathClass = css({
    display: "block",
    marginTop: "4px",
    color: "var(--text-muted)",
    fontFamily: 'ui-monospace, "SFMono-Regular", Consolas, monospace',
    fontSize: "11px",
    lineHeight: "1.35",
    overflowWrap: "anywhere"
  });
  const dashboardCardHitAreaClass = css({
    position: "absolute",
    inset: 0,
    zIndex: 1,
    border: 0,
    borderRadius: "var(--radius)",
    padding: 0,
    background: "transparent",
    cursor: "pointer"
  });
  const dashboardOverflowToggleClass = css({
    fontSize: "18px",
    lineHeight: "1"
  });
  const dashboardOverflowMenuClass = css({
    position: "absolute",
    right: 0,
    top: "calc(100% + 6px)",
    zIndex: 5,
    minWidth: "200px",
    gap: "6px",
    padding: "6px",
    border: "1px solid var(--border-strong)",
    borderRadius: "var(--radius)",
    background: "var(--surface)",
    boxShadow: "0 18px 40px var(--modal-shadow)"
  });
  const dashboardOverflowMenuButtonClass = css({
    justifyContent: "flex-start",
    width: "100%",
    minHeight: "32px",
    whiteSpace: "nowrap"
  });
  const dashboardSingleCommandClass = css({
    gridColumn: "1 / -1"
  });
  const dashboardProgressFillClass = css({
    width: "100%"
  });
  const dashboardLaunchOptionClass = (selected: boolean) => dashboardLaunchOptionRecipe({ selected });

  onDestroy(() => {
    unlistenInstallProgress?.();
    unlistenInstallTerminalOutput?.();
    void disposeInstallTerminal(true);
    void disposeLaunchTerminal(true);
    document.removeEventListener("click", closeOverflowOnOutsideClick);
    document.removeEventListener("keydown", closeOverflowOnEscape);
  });

  function desktopClientRouteForTool(toolId: string): string | null {
    if (toolId === "chatgpt-desktop") return "chatgptDesktop";
    if (toolId === "claude-desktop") return "claudeDesktop";
    return null;
  }

  function navigateToDesktopClient(toolId: string) {
    if (desktopClientRouteForTool(toolId)) {
      onNavigateToClient(toolId);
    }
  }

  function brandDesktopText(value: string) {
    return brandChatGPTDesktopText(value, $chatgptDesktopGeneration);
  }

  $: desktopProductName = $t("app.nav.chatgptDesktop");
  $: connectedClients = [
    ...(snapshot?.tools.filter((tool) => {
      if (tool.category !== "ai_tool") {
        return false;
      }
      return !isVscodePluginTool(tool) || hasVsCodeHost(snapshot?.tools ?? []);
    }).map((tool) => applyChatGPTDesktopToolBranding(tool, desktopProductName)) ?? [])
  ]
    .sort((left, right) => clientSortRank(left) - clientSortRank(right));
  $: envConflicts = snapshot?.envConflicts ?? [];
  async function copyInstallCommand() {
    if (!installPlan?.command) {
      return;
    }
    await navigator.clipboard?.writeText(installPlan.command);
  }

  async function triggerDesktopClientAction(tool: ToolStatus, mode: "install" | "update") {
    pendingInstallTool = null;
    installMode = mode;
    installResult = null;
    installError = null;
    toolActionMessage = null;
    toolActionError = null;
    await disposeInstallTerminal(false);
    clearInstallProgressLogs();
    if (mode === "update") {
      updatingToolId = tool.id;
    } else {
      installingToolId = tool.id;
    }
    // Jump to the dedicated client page so the user can watch the download
    // progress. The stores are global, so the page subscribes to the same
    // install/update stream on mount and renders the progress panel.
    onNavigateToClient(tool.id);
    try {
      const result = tool.id === "chatgpt-desktop"
        ? await installOrUpdateChatGPTDesktop()
        : await installOrUpdateClaudeDesktop(mode);
      if (result && "currentStatus" in result && result.currentStatus) {
        onToolStatusUpdated(result.currentStatus);
      }
      void Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false })).catch(() => {});
    } catch (err) {
      toolActionError = err instanceof Error ? err.message : String(err);
    } finally {
      if (mode === "update") {
        updatingToolId = null;
      } else {
        installingToolId = null;
      }
    }
  }

  async function openToolActionPlan(tool: ToolStatus, mode: "install" | "update") {
    if (isManagedDesktopClient(tool)) {
      await triggerDesktopClientAction(tool, mode);
      return;
    }

    const planTool = mode === "install" ? installPlanToolFor(tool) : tool;
    pendingInstallTool = planTool;
    installMode = mode;
    installPlan = null;
    installResult = null;
    installError = null;
    installTerminalExitCode = null;
    await disposeInstallTerminal(false);
    clearInstallProgressLogs();
    toolActionMessage = null;
    toolActionError = null;
    planningToolId = planTool.id;
    try {
      const plan = installMode === "update" ? planToolUpdate(planTool.id) : planToolInstall(planTool.id);
      installPlan = await plan;
    } catch (err) {
      installError = err instanceof Error ? err.message : String(err);
    } finally {
      planningToolId = null;
    }
  }

  async function openInstallPlan(tool: ToolStatus) {
    await openToolActionPlan(tool, "install");
  }

  async function closeInstallPlan() {
    if (installingToolId) {
      return;
    }
    await disposeInstallTerminal(false);
    pendingInstallTool = null;
    installMode = "install";
    installPlan = null;
    installResult = null;
    installError = null;
    clearInstallProgressLogs();
  }

  async function confirmToolAction() {
    if (!installPlan || installingToolId) {
      return;
    }
    installingToolId = installPlan.toolId;
    if (installMode === "update") {
      updatingToolId = installPlan.toolId;
    }
    installError = null;
    installResult = null;
    clearInstallProgressLogs();
    try {
      const action = installMode === "update" ? updateTool : installTool;
      const result = await action({
        toolId: installPlan.toolId,
        confirm: true,
        installPrerequisites: installPlan.requiresPrerequisites
      });
      installResult = result;
      if (installResult.currentStatus) {
        onToolStatusUpdated(installResult.currentStatus);
      }
      void Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false })).catch(() => {});
    } catch (err) {
      installError = err instanceof Error ? err.message : String(err);
    } finally {
      installingToolId = null;
      if (installMode === "update") {
        updatingToolId = null;
      }
    }
  }

  async function confirmInstallAction() {
    if (installPlan?.interactive) {
      await openInteractiveInstall();
      return;
    }
    await confirmToolAction();
  }

  async function confirmRepairPath(tool: ToolStatus) {
    if (!tool.pathRepair || repairingToolId || installingToolId || planningToolId || updatingToolId) {
      return;
    }

    repairingToolId = tool.id;
    toolActionMessage = null;
    toolActionError = null;
    installError = null;

    try {
      const result = await repairToolPath({
        toolId: tool.id,
        confirm: true
      });
      if (result.success) {
        toolActionMessage = toolRepairPathMessage(tool, true);
      } else {
        toolActionError = toolRepairPathMessage(tool, false);
      }
      if (result.currentStatus) {
        onToolStatusUpdated(result.currentStatus);
      }
      void Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false })).catch(() => {});
    } catch (err) {
      toolActionError = err instanceof Error ? err.message : String(err);
    } finally {
      repairingToolId = null;
    }
  }

  async function clearClaudeEnvConflicts() {
    if (clearingClaudeEnv || envConflicts.length === 0) {
      return;
    }
    clearingClaudeEnv = true;
    toolActionMessage = null;
    toolActionError = null;
    try {
      const result = await clearClaudeEnvironmentVariables({
        toolId: "claude",
        variables: envConflicts.map((conflict) => conflict.variable),
        confirm: true
      });
      if (result.success) {
        toolActionMessage = envClearMessage(true);
      } else {
        toolActionError = envClearMessage(false);
      }
      await Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false }));
    } catch (err) {
      toolActionError = err instanceof Error ? err.message : String(err);
    } finally {
      clearingClaudeEnv = false;
    }
  }

  // Toggle a card-action overflow <details>. Closing the previous popover when a
  // second one opens guarantees only one is open at a time; native <details>
  // would otherwise let several stay open simultaneously.
  function toggleOverflowDetails(event: MouseEvent, details: HTMLDetailsElement) {
    event.stopPropagation();
    const willOpen = !details.open;
    if (openOverflowDetails && openOverflowDetails !== details) {
      openOverflowDetails.open = false;
    }
    openOverflowDetails = willOpen ? details : null;
  }

  function closeOverflowOnOutsideClick(event: MouseEvent) {
    if (!openOverflowDetails) {
      return;
    }
    if (openOverflowDetails.contains(event.target as Node)) {
      return;
    }
    openOverflowDetails.open = false;
    openOverflowDetails = null;
  }

  function closeOverflowOnEscape(event: KeyboardEvent) {
    if (event.key !== "Escape" || !openOverflowDetails) {
      return;
    }
    openOverflowDetails.open = false;
    openOverflowDetails = null;
  }

  async function refreshDashboard() {
    if (refreshBusy) {
      return;
    }
    refreshing = true;
    toolActionMessage = null;
    toolActionError = null;
    try {
      await Promise.resolve(onRefresh({ quiet: false, scheduleFollowup: true, showRefreshIndicator: true }));
    } catch (err) {
      toolActionError = err instanceof Error ? err.message : String(err);
    } finally {
      refreshing = false;
    }
  }

  function defaultLaunchShellId(plan: ToolLaunchPlan) {
    return (
      plan.shells.find((shell) => shell.available && shell.default)?.id ??
      plan.shells.find((shell) => shell.available)?.id ??
      null
    );
  }

  function selectedLaunchProfile() {
    if (!launchPlan || !selectedLaunchProfileId) {
      return null;
    }
    return launchPlan.profiles.find((profile) => profile.id === selectedLaunchProfileId) ?? null;
  }

  function normalizedLaunchWorkingDirectory() {
    return launchWorkingDirectory.trim() || null;
  }

  async function launchDesktopClient(tool: ToolStatus) {
    if (launchingToolId) {
      return;
    }
    const launchStartedAt = Date.now();
    launchingToolId = tool.id;
    directLaunchToolIds = new Set(directLaunchToolIds).add(tool.id);
    toolActionError = null;
    const launchPromise = tool.id === "chatgpt-desktop"
      ? launchManagedChatGPTDesktop()
      : launchClaudeDesktopFromDashboard();
    try {
      await tick();
      await launchPromise;
    } catch (err) {
      toolActionError = err instanceof Error ? err.message : String(err);
    } finally {
      const remainingFeedbackMs = Math.max(0, directLaunchFeedbackMs - (Date.now() - launchStartedAt));
      await new Promise((resolve) => setTimeout(resolve, remainingFeedbackMs));
      const nextDirectLaunchToolIds = new Set(directLaunchToolIds);
      nextDirectLaunchToolIds.delete(tool.id);
      directLaunchToolIds = nextDirectLaunchToolIds;
      launchingToolId = null;
    }
  }

  async function openToolLaunch(tool: ToolStatus) {
    if (!canShowToolLaunch(tool)) {
      return;
    }
    if (isManagedDesktopClient(tool)) {
      await launchDesktopClient(tool);
      return;
    }
    pendingLaunchTool = tool;
    launchPlan = null;
    launchError = null;
    launchTerminalExitCode = null;
    selectedLaunchProfileId = null;
    selectedLaunchShellId = null;
    launchWorkingDirectory = "";
    launchMode = "external";
    await disposeLaunchTerminal(false);
    planningLaunchToolId = tool.id;
    toolActionMessage = null;
    toolActionError = null;
    try {
      const plan = await planToolLaunch(tool.id);
      launchPlan = plan;
      selectedLaunchShellId = defaultLaunchShellId(plan);
    } catch (err) {
      launchError = err instanceof Error ? err.message : String(err);
    } finally {
      planningLaunchToolId = null;
    }
  }

  async function closeToolLaunch() {
    if (launchTerminalRunning) {
      return;
    }
    await disposeLaunchTerminal(false);
    pendingLaunchTool = null;
    launchPlan = null;
    launchError = null;
    launchTerminalExitCode = null;
    selectedLaunchProfileId = null;
    selectedLaunchShellId = null;
    launchWorkingDirectory = "";
    launchMode = "external";
  }

  async function startToolLaunch() {
    if (!launchPlan || !pendingLaunchTool || launchTerminalRunning || launchingToolId) {
      return;
    }
    if (!launchPlan.canLaunch) {
      launchError = launchPlan.blocker ?? $t("toolLaunch.unavailable");
      return;
    }
    if (!selectedLaunchShellId) {
      launchError = $t("toolLaunch.noConsole");
      return;
    }
    if (launchMode === "external") {
      await startExternalLaunch();
    } else {
      await startEmbeddedLaunch();
    }
  }

  async function startExternalLaunch() {
    if (!launchPlan || !pendingLaunchTool) return;
    launchError = null;
    launchingToolId = pendingLaunchTool.id;
    try {
      await launchToolExternal({
        toolId: launchPlan.toolId,
        command: launchPlan.command,
        shellId: selectedLaunchShellId,
        profileId: selectedLaunchProfileId,
        workingDirectory: normalizedLaunchWorkingDirectory()
      });
      closeToolLaunch();
    } catch (err) {
      launchError = err instanceof Error ? err.message : String(err);
    } finally {
      launchingToolId = null;
    }
  }

  async function startEmbeddedLaunch() {
    if (!launchPlan || !pendingLaunchTool) return;
    launchError = null;
    launchTerminalExitCode = null;
    const toolId = launchPlan.toolId;
    const command = launchPlan.command;
    const shellId = selectedLaunchShellId;
    const profileId = selectedLaunchProfileId;
    const workingDirectory = normalizedLaunchWorkingDirectory();
    const toolName = launchPlan.toolName;
    closeToolLaunch();
    // Open the terminal panel immediately — the session starts in the
    // background and output streams in as soon as the PTY is ready.
    onOpenTerminal();
    void startEmbeddedSession(
      {
        toolId,
        command,
        shellId,
        profileId,
        workingDirectory,
        keepOpen: true,
        cols: 100,
        rows: 24
      },
      toolName
    );
  }

  async function stopToolLaunch() {
    if (!launchTerminalSessionId) {
      return;
    }
    const sessionId = launchTerminalSessionId;
    await stopInstallTerminal({ sessionId }).catch((err) => {
      launchError = err instanceof Error ? err.message : String(err);
    });
    launchTerminalRunning = false;
    launchingToolId = null;
    launchTerminalExitCode = null;
  }
</script>

<div class={routeStackRecipe({ width: "full" })}>
  <section class={topStripRecipe()}>
    <div>
      <h1>{$t("dashboard.title")}</h1>
      <p>{$t("dashboard.subtitle")}</p>
    </div>
    <div class={topActionsRecipe()}>
      <button class={actionButtonRecipe()} type="button" data-refresh-button="true" disabled={refreshBusy} on:click={refreshDashboard}>
        <AppIcon name={refreshBusy ? "loading" : "refresh"} size={15} class={refreshBusy ? spinRecipe() : ""} />
        {$t(refreshBusy ? "common.refreshing" : "common.refresh")}
      </button>
    </div>
  </section>

  <section class={panelRecipe()}>
    {#if toolActionMessage}
      <DismissibleNotice tone="success" message={brandDesktopText(toolActionMessage)} on:dismiss={() => (toolActionMessage = null)} />
    {/if}
    {#if toolActionError}
      <DismissibleNotice tone="error" message={brandDesktopText(toolActionError)} on:dismiss={() => (toolActionError = null)} />
    {/if}
    {#if envConflicts.length > 0}
      <div class={dashboardEnvConflictRecipe()}>
        <div>
          <strong>{$t("envConflict.title")}</strong>
          <span>{$t("envConflict.dashboardDescription", { count: envConflicts.length })}</span>
          <div data-dashboard-env-conflict-chips>
            {#each envConflicts as conflict}
              <code>{conflict.scope}:{conflict.variable}={conflict.currentValuePreview}</code>
            {/each}
          </div>
        </div>
        <button class={actionButtonRecipe()} disabled={clearingClaudeEnv} on:click={clearClaudeEnvConflicts}>
          {#if clearingClaudeEnv}
            <AppIcon name="loading" size={16} class={spinRecipe()} />
          {:else}
            <AppIcon name="repair" size={16} />
          {/if}
          {$t("envConflict.clearAction")}
        </button>
      </div>
    {/if}

    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("dashboard.connectedClients")}</h2>
        <p>{snapshot ? $t("dashboard.toolsTracked", { count: connectedClients.length }) : $t("app.state.scanning")}</p>
      </div>
    </div>
    <div class={dashboardGridRecipe({ kind: "client" })}>
      {#each connectedClients as tool}
        <article
          class={dashboardCardRecipe({ clickable: Boolean(desktopClientRouteForTool(tool.id)) })}
        >
          {#if desktopClientRouteForTool(tool.id)}
            <button
              class={dashboardCardHitAreaClass}
              type="button"
              aria-label={tool.name}
              data-dashboard-card-hit-area
              on:click={() => navigateToDesktopClient(tool.id)}
            ></button>
          {/if}
          <div class={dashboardCardMainRecipe()}>
            <ToolIcon toolId={tool.id} label={tool.name} category={tool.category} />
            <div class={dashboardCardCopyClass}>
              <h3>{tool.name}</h3>
              <p>{tool.version ?? tool.details ?? tool.command}</p>
              {#if resolvedToolDetail(tool)}
                <span class={dashboardPathClass}>{resolvedToolDetail(tool)}</span>
              {/if}
              {#if tool.updateAvailable && tool.latestVersion}
                <span class={dashboardPathClass}>{$t("tool.latestVersion", { version: tool.latestVersion })}</span>
              {/if}
              {#if tool.pathRepair}
                <span class={dashboardPathClass}>{tool.pathRepair.message}</span>
              {/if}
            </div>
          </div>

          <div class={dashboardCardStateRecipe()}>
            <StatusPill
              status={tool.installState}
              label={$t(`status.${tool.installState}` as Parameters<typeof $t>[0])}
            />
          </div>
          <div
            class={dashboardCardActionsRecipe()}
            data-dashboard-card-actions
          >
            {#if tool.installState === "missing"}
              {#if tool.pathRepair}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if isRepairingTool(tool)}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {isRepairingTool(tool) ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              <button
                class={actionButtonRecipe({ compact: true })}
                title={$t("tool.installCommand", { name: tool.name })}
                disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                on:click={() => openInstallPlan(tool)}
              >
                {#if isInstallingTool(tool)}
                  <AppIcon name="loading" size={16} class={spinRecipe()} />
                {:else}
                  <AppIcon name="install" size={16} />
                {/if}
                {isInstallingTool(tool) ? $t("tool.installing") : $t("common.install")}
              </button>
            {:else}
              {#if isDashboardActionVisible(tool, "update")}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("tool.updateCommand", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => openToolActionPlan(tool, "update")}
                >
                  {#if isUpdatingTool(tool)}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="update" size={16} />
                  {/if}
                  {isUpdatingTool(tool) ? $t("tool.updating") : $t("common.update")}
                </button>
              {/if}
              {#if isDashboardActionVisible(tool, "repair")}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if isRepairingTool(tool)}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {isRepairingTool(tool) ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              {#if isDashboardActionVisible(tool, "launch")}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("toolLaunch.actionTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => openToolLaunch(tool)}
                >
                  {#if isLaunchingTool(tool, launchingToolId, directLaunchToolIds)}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="play" size={16} />
                  {/if}
                  {isLaunchingTool(tool, launchingToolId, directLaunchToolIds) ? $t("toolLaunch.starting") : $t("toolLaunch.action")}
                </button>
              {/if}
              {#if isDashboardActionVisible(tool, "configure")}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("tool.createConfig", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => onConfigureTool(tool)}
                >
                  <AppIcon name="settings" size={16} />
                  {$t("common.createConfig")}
                </button>
              {/if}
              {#if shouldShowDashboardOverflow(tool)}
                <details class={dashboardOverflowRecipe()} data-dashboard-overflow>
                <summary
                  class={cx(iconButtonRecipe({ compact: true }), dashboardOverflowToggleClass)}
                  title={$t("tool.moreActionsTitle", { name: tool.name })}
                  aria-label={$t("common.moreActions")}
                  on:click={(event) => toggleOverflowDetails(event, (event.currentTarget as Element).closest("details") as HTMLDetailsElement)}
                >⋯</summary>
                <div class={dashboardOverflowMenuClass} data-dashboard-overflow-menu>
                  {#if isDashboardActionOverflowed(tool, "update")}
                    <button
                      class={cx(actionButtonRecipe({ compact: true }), dashboardOverflowMenuButtonClass)}
                      title={$t("tool.updateCommand", { name: tool.name })}
                      disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                      on:click|stopPropagation={() => openToolActionPlan(tool, "update")}
                    >
                      {#if isUpdatingTool(tool)}
                        <AppIcon name="loading" size={16} class={spinRecipe()} />
                      {:else}
                        <AppIcon name="update" size={16} />
                      {/if}
                      {isUpdatingTool(tool) ? $t("tool.updating") : $t("common.update")}
                    </button>
                  {/if}
                  {#if isDashboardActionOverflowed(tool, "repair")}
                    <button
                      class={cx(actionButtonRecipe({ compact: true }), dashboardOverflowMenuButtonClass)}
                      title={$t("tool.repairPathTitle", { name: tool.name })}
                      disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                      on:click|stopPropagation={() => confirmRepairPath(tool)}
                    >
                      {#if isRepairingTool(tool)}
                        <AppIcon name="loading" size={16} class={spinRecipe()} />
                      {:else}
                        <AppIcon name="repair" size={16} />
                      {/if}
                      {isRepairingTool(tool) ? $t("tool.repairingPath") : $t("tool.repairPath")}
                    </button>
                  {/if}
                  {#if isDashboardActionOverflowed(tool, "launch")}
                    <button
                      class={cx(actionButtonRecipe({ compact: true }), dashboardOverflowMenuButtonClass)}
                      title={$t("toolLaunch.actionTitle", { name: tool.name })}
                      disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                      on:click|stopPropagation={() => openToolLaunch(tool)}
                    >
                      {#if isLaunchingTool(tool, launchingToolId, directLaunchToolIds)}
                        <AppIcon name="loading" size={16} class={spinRecipe()} />
                      {:else}
                        <AppIcon name="play" size={16} />
                      {/if}
                      {isLaunchingTool(tool, launchingToolId, directLaunchToolIds) ? $t("toolLaunch.starting") : $t("toolLaunch.action")}
                    </button>
                  {/if}
                  {#if isDashboardActionOverflowed(tool, "configure")}
                    <button
                      class={cx(actionButtonRecipe({ compact: true }), dashboardOverflowMenuButtonClass)}
                      title={$t("tool.createConfig", { name: tool.name })}
                      disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                      on:click|stopPropagation={() => onConfigureTool(tool)}
                    >
                      <AppIcon name="settings" size={16} />
                      {$t("common.createConfig")}
                    </button>
                  {/if}
                </div>
                </details>
              {/if}
            {/if}
          </div>
        </article>
      {:else}
        <div class={emptyRowRecipe()} data-dashboard-empty>{$t("dashboard.noClientSnapshot")}</div>
      {/each}
    </div>
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("dashboard.system")}</h2>
        <p>{$t("dashboard.systemDeps")}</p>
      </div>
    </div>
    <div class={dashboardGridRecipe({ kind: "system" })}>
      {#each snapshot?.system ?? [] as tool}
        <article class={dashboardCardRecipe()}>
          <div class={dashboardCardMainRecipe()}>
            <ToolIcon toolId={tool.id} label={tool.name} category={tool.category} />
            <div class={dashboardCardCopyClass}>
              <h3>{tool.name}</h3>
              <p>{tool.version ?? tool.details ?? tool.command}</p>
              {#if tool.updateAvailable && tool.latestVersion}
                <span class={dashboardPathClass}>{$t("tool.latestVersion", { version: tool.latestVersion })}</span>
              {/if}
            </div>
          </div>

          <div class={dashboardCardStateRecipe()}>
            <StatusPill
              status={tool.installState}
              label={$t(`status.${tool.installState}` as Parameters<typeof $t>[0])}
            />
          </div>
          <div class={dashboardCardActionsRecipe()}>
            {#if tool.installState === "missing"}
              {#if tool.pathRepair}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              <button
                class={actionButtonRecipe({ compact: true })}
                title={$t("tool.installCommand", { name: tool.name })}
                disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                on:click={() => openInstallPlan(tool)}
              >
                {#if isInstallingTool(tool)}
                  <AppIcon name="loading" size={16} class={spinRecipe()} />
                {:else}
                  <AppIcon name="install" size={16} />
                {/if}
                {isInstallingTool(tool) ? $t("tool.installing") : $t("common.install")}
              </button>
            {:else if tool.updateAvailable}
              <button
                class={actionButtonRecipe({ compact: true })}
                title={$t("tool.updateCommand", { name: tool.name })}
                disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                on:click={() => openToolActionPlan(tool, "update")}
              >
                {#if updatingToolId === tool.id}
                  <AppIcon name="loading" size={16} class={spinRecipe()} />
                {:else}
                  <AppIcon name="update" size={16} />
                {/if}
                {updatingToolId === tool.id ? $t("tool.updating") : $t("common.update")}
              </button>
              {#if tool.pathRepair}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              {#if canShowToolLaunch(tool)}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("toolLaunch.actionTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => openToolLaunch(tool)}
                >
                  {#if isLaunchingTool(tool, launchingToolId, directLaunchToolIds)}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="play" size={16} />
                  {/if}
                  {isLaunchingTool(tool, launchingToolId, directLaunchToolIds) ? $t("toolLaunch.starting") : $t("toolLaunch.action")}
                </button>
              {/if}
            {:else}
              {#if tool.pathRepair}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              {#if canShowToolLaunch(tool)}
                <button
                  class={actionButtonRecipe({ compact: true })}
                  title={$t("toolLaunch.actionTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool, launchingToolId, directLaunchToolIds)}
                  on:click={() => openToolLaunch(tool)}
                >
                  {#if isLaunchingTool(tool, launchingToolId, directLaunchToolIds)}
                    <AppIcon name="loading" size={16} class={spinRecipe()} />
                  {:else}
                    <AppIcon name="play" size={16} />
                  {/if}
                  {isLaunchingTool(tool, launchingToolId, directLaunchToolIds) ? $t("toolLaunch.starting") : $t("toolLaunch.action")}
                </button>
              {/if}
            {/if}
          </div>
        </article>
      {:else}
        <div class={emptyRowRecipe()} data-dashboard-empty>{$t("dashboard.noSystemSnapshot")}</div>
      {/each}
    </div>
  </section>

</div>

{#if pendingInstallTool}
  <div class={dashboardModalBackdropRecipe()} role="presentation">
    <div class={dashboardModalPanelRecipe()} role="dialog" aria-modal="true" aria-labelledby="tool-install-title">
      <div class={dashboardModalBodyRecipe()}>
        <div>
        <h2 id="tool-install-title">
          {$t(installMode === "update" ? "toolInstall.updateTitle" : "toolInstall.title", { name: pendingInstallTool.name })}
        </h2>
        <p>{$t("toolInstall.description")}</p>
      </div>

      {#if planningToolId}
        <div class={dashboardProgressRecipe()} aria-live="polite">
          <div data-dashboard-progress-copy>
            <strong>{$t("toolInstall.planning")}</strong>
            <span>{pendingInstallTool.name}</span>
          </div>
          <div data-dashboard-progress-track data-indeterminate="true">
            <span class={dashboardProgressFillClass} data-dashboard-progress-fill></span>
          </div>
        </div>
      {/if}

      {#if installPlan}
        <div class={dashboardCommandBoxRecipe()}>
          <div>
            <strong>{$t("toolInstall.command")}</strong>
            <span>{$t("toolInstall.manager", { manager: installPlan.manager })}</span>
          </div>
          <div class={dashboardCommandListRecipe()}>
            {#each installPlan.commands as command}
              <div>
                <span>{command.stage === "prerequisite" ? $t("toolInstall.stage.prerequisite") : installMode === "update" ? $t("common.update") : $t("toolInstall.stage.target")}</span>
                <code>{command.command}</code>
              </div>
            {/each}
          </div>
          <button class={iconButtonRecipe()} title={$t("toolInstall.copyCommand")} on:click={copyInstallCommand}>
            <AppIcon name="copy" size={18} />
          </button>
        </div>

        <div class={dashboardInfoGridRecipe()}>
          <span>{installPlan.requiresAdmin ? $t("toolInstall.adminMayPrompt") : $t("toolInstall.userScope")}</span>
        </div>

        {#if installPlan.prerequisites.length > 0}
          <div class={dashboardPreviewListRecipe()}>
            {#each installPlan.prerequisites as prerequisite}
              <div>
                <strong>{prerequisite.toolName}</strong>
                <span>{prerequisite.reason}</span>
                <code>{prerequisite.command}</code>
              </div>
            {/each}
          </div>
        {/if}

        {#if installPlan.blocker}
          <div class={noticeRecipe({ tone: "error" })}>{installPlan.blocker}</div>
        {/if}

        {#if installMode !== "update" && installPlan.steps.length > 0}
          <div class={dashboardPreviewListRecipe()}>
            {#each installPlan.steps as step}
              <div>
                <strong>{step.label}</strong>
                <span>{step.detail}</span>
              </div>
            {/each}
          </div>
        {/if}

      {/if}

      {#if installingToolId}
        <div class={dashboardProgressRecipe()} aria-live="polite">
          <div data-dashboard-progress-copy>
            <strong>{$t(installMode === "update" ? "tool.updating" : "tool.installing")}</strong>
            <span>{installPlan?.command}</span>
          </div>
          <div data-dashboard-progress-track data-indeterminate="true">
            <span class={dashboardProgressFillClass} data-dashboard-progress-fill></span>
          </div>
        </div>
      {/if}

      {#if installPlan?.interactive}
        <div class={dashboardTerminalCardRecipe()}>
          <div class={dashboardTerminalHeaderRecipe()}>
            <strong>{$t("toolInstall.terminalTitle")}</strong>
            {#if installTerminalRunning}
              <span>{$t("common.running")}</span>
            {:else if installTerminalExitCode !== null}
              <span>{$t("toolInstall.terminalExit", { code: installTerminalExitCode })}</span>
            {:else}
              <span>{$t("toolInstall.terminalReady")}</span>
            {/if}
          </div>
          <div class={dashboardTerminalFrameRecipe()} bind:this={installTerminalElement}></div>
        </div>
      {/if}

      {#if !installPlan?.interactive && (installingToolId || updatingToolId || hasLiveInstallLogs)}
        <div class={dashboardLogRecipe({ live: true })}>
          <strong>{$t("toolInstall.consoleOutput")}</strong>
          <div class={dashboardLogViewportRecipe()} bind:this={installLogViewport}>
            {#each liveInstallLogGroups as group (group.key)}
              <div class={dashboardLogStageRecipe()}>
                <span>
                  {group.label}
                  {#if group.done}
                    · {$t("toolInstall.exitCode")}: {group.exitCode ?? $t("common.none")}
                  {/if}
                </span>
                {#if group.stdout}
                  <b>{$t("toolInstall.stdout")}</b>
                  <pre>{group.stdout}</pre>
                {/if}
                {#if group.stderr}
                  <b>{$t("toolInstall.stderr")}</b>
                  <pre>{group.stderr}</pre>
                {/if}
              </div>
            {:else}
              <div class={dashboardLogStageRecipe()}>
                <span>{$t("common.loading")}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      {#if installResult}
        <div class={noticeRecipe({ tone: installResult.success ? "success" : "error" })}>
          {localizedInstallResultMessage(installResult)}
        </div>
        <div class={dashboardInfoGridRecipe()}>
          <div>
            <strong>{$t("toolInstall.exitCode")}</strong>
            <span>{installResult.exitCode ?? $t("common.none")}</span>
          </div>
          <div>
            <strong>{$t("common.status")}</strong>
            <span>{installResult.currentStatus?.installState ?? $t("common.unknown")}</span>
          </div>
        </div>
        {#if installResult.stageResults.length > 0}
          <div class={dashboardPreviewListRecipe()}>
            {#each installResult.stageResults as stage}
              <div>
                <strong>{stageLabel(stage.stage)} / {stage.toolName}</strong>
                <span>{localizedInstallStageMessage(stage)}</span>
                <code>{stage.command}</code>
                <span>{$t("toolInstall.exitCode")}: {stage.exitCode ?? $t("common.none")}</span>
              </div>
            {/each}
          </div>
        {/if}
        {#if !hasLiveInstallLogs && hasInstallConsoleOutput(installResult)}
          <div class={dashboardLogRecipe()}>
            <strong>{$t("toolInstall.consoleOutput")}</strong>
            {#each installResult.stageResults as stage}
              {#if stage.stdoutTail || stage.stderrTail}
                <div class={dashboardLogStageRecipe()}>
                  <span>{stageLabel(stage.stage)} / {stage.toolName}</span>
                  {#if stage.stdoutTail}
                    <b>{$t("toolInstall.stdout")}</b>
                    <pre>{stage.stdoutTail}</pre>
                  {/if}
                  {#if stage.stderrTail}
                    <b>{$t("toolInstall.stderr")}</b>
                    <pre>{stage.stderrTail}</pre>
                  {/if}
                </div>
              {/if}
            {/each}
          </div>
        {/if}
      {/if}

      {#if installError}
        <div class={noticeRecipe({ tone: "error" })}>{brandDesktopText(installError)}</div>
      {/if}

      </div>

      <div class={dashboardModalActionsRecipe()}>
        <button class={actionButtonRecipe()} on:click={closeInstallPlan} disabled={Boolean(installingToolId)}>
          {$t(installResult ? "common.close" : "common.cancel")}
        </button>
        {#if installTerminalRunning}
          <button class={actionButtonRecipe()} type="button" on:click={stopInteractiveInstall}>
            <AppIcon name="stop" size={16} />
            {$t("common.stop")}
          </button>
        {/if}
        {#if installPlan && !installResult}
          <button
            class={actionButtonRecipe({ tone: "primary" })}
            on:click={confirmInstallAction}
            disabled={!installPlan.canInstall || Boolean(installingToolId)}
          >
            {#if installingToolId}
              <AppIcon name="loading" size={16} class={spinRecipe()} />
              {$t(installMode === "update" ? "tool.updating" : "tool.installing")}
            {:else}
              <AppIcon name={installMode === "update" ? "update" : "install"} size={16} />
              {$t(
                installPlan.interactive
                  ? "toolInstall.openTerminal"
                  : installMode === "update"
                    ? "toolInstall.confirmUpdate"
                  : installPlan.requiresPrerequisites
                    ? "toolInstall.confirmInstallWithPrerequisites"
                    : "toolInstall.confirmInstall"
              )}
            {/if}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if pendingLaunchTool}
  <div class={dashboardModalBackdropRecipe()} role="presentation">
    <div class={dashboardModalPanelRecipe()} role="dialog" aria-modal="true" aria-labelledby="tool-launch-title">
      <div class={dashboardModalBodyRecipe()}>
        <div>
          <h2 id="tool-launch-title">{$t("toolLaunch.title", { name: pendingLaunchTool.name })}</h2>
          <p>{$t("toolLaunch.description")}</p>
        </div>

        {#if planningLaunchToolId}
          <div class={dashboardProgressRecipe()} aria-live="polite">
            <div data-dashboard-progress-copy>
              <strong>{$t("toolLaunch.planning")}</strong>
              <span>{pendingLaunchTool.name}</span>
            </div>
            <div data-dashboard-progress-track data-indeterminate="true">
              <span class={dashboardProgressFillClass} data-dashboard-progress-fill></span>
            </div>
          </div>
        {/if}

        {#if launchPlan}
          <div class={dashboardCommandBoxRecipe()}>
            <div>
              <strong>{$t("toolLaunch.command")}</strong>
              <span>{launchPlan.toolName}</span>
            </div>
            <code class={dashboardSingleCommandClass}>{launchPlan.command}</code>
          </div>

          <section class={dashboardLaunchSectionRecipe()}>
            <div class={dashboardLaunchHeadingRecipe()}>
              <strong>{$t("toolLaunch.configSource")}</strong>
              {#if selectedLaunchProfile()}
                <span>{$t("toolLaunch.temporaryProfile")}</span>
              {:else}
                <span>{$t("toolLaunch.globalConfigDescription")}</span>
              {/if}
            </div>
            <div class={dashboardLaunchGridRecipe()}>
              <button
                type="button"
                class={dashboardLaunchOptionClass(!selectedLaunchProfileId)}
                data-selected={!selectedLaunchProfileId}
                aria-pressed={!selectedLaunchProfileId}
                disabled={launchTerminalRunning}
                on:click={() => (selectedLaunchProfileId = null)}
              >
                <strong>{$t("toolLaunch.globalConfig")}</strong>
                <span>{$t("toolLaunch.globalConfigDescription")}</span>
              </button>
              {#each launchPlan.profiles as profile}
                <button
                  type="button"
                  class={dashboardLaunchOptionClass(selectedLaunchProfileId === profile.id)}
                  data-selected={selectedLaunchProfileId === profile.id}
                  aria-pressed={selectedLaunchProfileId === profile.id}
                  disabled={launchTerminalRunning}
                  on:click={() => (selectedLaunchProfileId = profile.id)}
                >
                  <strong>{profile.name}</strong>
                  <span>{profile.provider === "official" ? $t("toolLaunch.officialProfile") : profile.baseUrl || profile.provider}</span>
                </button>
              {:else}
                <div class={dashboardLaunchEmptyRecipe()}>{$t("toolLaunch.noProfiles")}</div>
              {/each}
            </div>
          </section>

          <section class={dashboardLaunchSectionRecipe()}>
            <div class={dashboardLaunchHeadingRecipe()}>
              <strong>{$t("toolLaunch.console")}</strong>
              <span>{launchPlan.shells.find((shell) => shell.id === selectedLaunchShellId)?.command ?? $t("common.none")}</span>
            </div>
            <div class={dashboardLaunchGridRecipe({ compact: true })}>
              {#each launchPlan.shells as shell}
                <button
                  type="button"
                  class={dashboardLaunchOptionClass(selectedLaunchShellId === shell.id)}
                  data-selected={selectedLaunchShellId === shell.id}
                  aria-pressed={selectedLaunchShellId === shell.id}
                  disabled={!shell.available || launchTerminalRunning}
                  on:click={() => (selectedLaunchShellId = shell.id)}
                >
                  <strong>{shell.label}</strong>
                  <span>{shell.available ? shell.command : $t("toolLaunch.shellUnavailable")}</span>
                </button>
              {/each}
            </div>
          </section>

          <section class={dashboardLaunchSectionRecipe()}>
            <div class={dashboardLaunchHeadingRecipe()}>
              <strong>{$t("toolLaunch.launchMode")}</strong>
              <span>{$t(launchMode === "embedded" ? "toolLaunch.embeddedDescription" : "toolLaunch.externalDescription")}</span>
            </div>
            <div class={dashboardLaunchGridRecipe({ compact: true })}>
              <button
                type="button"
                class={dashboardLaunchOptionClass(launchMode === "external")}
                data-selected={launchMode === "external"}
                aria-pressed={launchMode === "external"}
                disabled={launchTerminalRunning || Boolean(launchingToolId)}
                on:click={() => (launchMode = "external")}
              >
                <strong>{$t("toolLaunch.external")}</strong>
                <span>{$t("toolLaunch.externalHint")}</span>
              </button>
              <button
                type="button"
                class={dashboardLaunchOptionClass(launchMode === "embedded")}
                data-selected={launchMode === "embedded"}
                aria-pressed={launchMode === "embedded"}
                disabled={launchTerminalRunning || Boolean(launchingToolId)}
                on:click={() => (launchMode = "embedded")}
              >
                <strong>{$t("toolLaunch.embedded")}</strong>
                <span>{$t("toolLaunch.embeddedHint")}</span>
              </button>
            </div>
          </section>

          <section class={dashboardLaunchSectionRecipe()}>
            <label class={dashboardDirectoryFieldRecipe()}>
              <span>{$t("toolLaunch.workingDirectory")}</span>
              <input
                bind:value={launchWorkingDirectory}
                disabled={launchTerminalRunning}
                placeholder={$t("toolLaunch.workingDirectoryPlaceholder")}
              />
            </label>
          </section>

          {#if launchPlan.blocker}
            <div class={noticeRecipe({ tone: "error" })}>{launchPlan.blocker}</div>
          {/if}
        {/if}

        {#if launchingToolId || launchTerminalSessionId || launchTerminalExitCode !== null}
          <div class={dashboardTerminalCardRecipe()}>
            <div class={dashboardTerminalHeaderRecipe()}>
              <strong>{$t("toolLaunch.terminalTitle")}</strong>
              {#if launchTerminalRunning}
                <span>{$t("common.running")}</span>
              {:else if launchTerminalExitCode !== null}
                <span>{$t("toolLaunch.terminalExit", { code: launchTerminalExitCode })}</span>
              {:else}
                <span>{$t("toolLaunch.terminalReady")}</span>
              {/if}
            </div>
            <div class={dashboardTerminalFrameRecipe()} bind:this={launchTerminalElement}></div>
          </div>
        {/if}

        {#if launchError}
          <div class={noticeRecipe({ tone: "error" })}>{brandDesktopText(launchError)}</div>
        {/if}
      </div>

      <div class={dashboardModalActionsRecipe()}>
        <button class={actionButtonRecipe()} on:click={closeToolLaunch} disabled={launchTerminalRunning}>
          {$t(launchTerminalExitCode !== null ? "common.close" : "common.cancel")}
        </button>
        {#if launchTerminalRunning}
          <button class={actionButtonRecipe()} type="button" on:click={stopToolLaunch}>
            <AppIcon name="stop" size={16} />
            {$t("common.stop")}
          </button>
        {/if}
        {#if launchPlan}
          <button
            class={actionButtonRecipe({ tone: "primary" })}
            on:click={startToolLaunch}
            disabled={!launchPlan.canLaunch || !selectedLaunchShellId || launchTerminalRunning || Boolean(launchingToolId)}
          >
            {#if launchingToolId}
              <AppIcon name="loading" size={16} class={spinRecipe()} />
              {$t("toolLaunch.starting")}
            {:else}
              <AppIcon name="play" size={16} />
              {$t("toolLaunch.start")}
            {/if}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
