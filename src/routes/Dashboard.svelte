<script lang="ts">
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import AppIcon from "../components/AppIcon.svelte";
  import DismissibleNotice from "../components/DismissibleNotice.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import {
    getClaudeDesktopLocalizeLaunch,
    installOrUpdateClaudeDesktop,
    launchClaudeDesktopFromDashboard,
    refreshClaudeDesktop
  } from "../lib/claudeDesktopStore";
  import {
    installOrUpdateCodexClient,
    launchManagedCodexClient,
    refreshCodexClient
  } from "../lib/codexClientStore";
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
  import { afterUpdate, onDestroy, tick } from "svelte";

  export let snapshot: DetectionSnapshot | null = null;
  export let onRefresh: (options?: { quiet?: boolean; scheduleFollowup?: boolean }) => void | Promise<void> = () => {};
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
  let selectedLaunchProfileId: string | null = null;
  let selectedLaunchShellId: string | null = null;
  let launchWorkingDirectory = "";
  let launchMode: LaunchMode = "embedded";
  let launchTerminalElement: HTMLDivElement | null = null;
  let launchTerminal: Terminal | null = null;
  let launchTerminalSessionId: string | null = null;
  let launchTerminalRunning = false;
  let launchTerminalExitCode: number | null = null;
  let unlistenInstallTerminalOutput: (() => void) | null = null;
  let unlistenInstallProgress: (() => void) | null = null;
  let refreshing = false;
  let clearingClaudeEnv = false;
  const vscodePluginToolIds = new Set(["codex-vscode", "claude-vscode", "gemini-code-assist"]);

  function clientSortRank(tool: ToolStatus) {
    return tool.id === "codex-app" ? 0 : 1;
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

  function isLaunchingTool(tool: ToolStatus) {
    return planningLaunchToolId === tool.id || launchingToolId === tool.id;
  }

  function isToolActionBusy(tool: ToolStatus) {
    return isInstallingTool(tool) || isUpdatingTool(tool) || isRepairingTool(tool) || isLaunchingTool(tool);
  }

  function isManagedDesktopClient(tool: ToolStatus) {
    return tool.id === "codex-app" || tool.id === "claude-desktop";
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

  onDestroy(() => {
    unlistenInstallProgress?.();
    unlistenInstallTerminalOutput?.();
    void disposeInstallTerminal(true);
    void disposeLaunchTerminal(true);
  });

  function desktopClientRouteForTool(toolId: string): string | null {
    if (toolId === "codex-app") return "codexClient";
    if (toolId === "claude-desktop") return "claudeDesktop";
    return null;
  }

  $: connectedClients = [
    ...(snapshot?.tools.filter((tool) => {
      if (tool.category !== "ai_tool") {
        return false;
      }
      return !isVscodePluginTool(tool) || hasVsCodeHost(snapshot?.tools ?? []);
    }) ?? [])
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
    try {
      const result = tool.id === "codex-app"
        ? await installOrUpdateCodexClient()
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

  async function refreshDashboard() {
    if (refreshing) {
      return;
    }
    refreshing = true;
    toolActionMessage = null;
    toolActionError = null;
    try {
      await Promise.resolve(onRefresh({ quiet: false, scheduleFollowup: true }));
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
    launchingToolId = tool.id;
    toolActionError = null;
    const launchPromise = tool.id === "codex-app"
      ? launchManagedCodexClient()
      : launchClaudeDesktopFromDashboard();
    launchPromise.then(() => {
      void Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false })).catch(() => {});
    }).catch((err) => {
      toolActionError = err instanceof Error ? err.message : String(err);
    }).finally(() => {
      launchingToolId = null;
    });
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
    launchMode = "embedded";
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
    launchMode = "embedded";
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

<div class="route-stack">
  <section class="top-strip">
    <div>
      <span class="eyebrow">{$t("dashboard.eyebrow")}</span>
      <h1>{$t("dashboard.title")}</h1>
      <p>{$t("dashboard.subtitle")}</p>
    </div>
    <div class="top-actions">
      <button class="secondary-button" type="button" disabled={refreshing} on:click={refreshDashboard}>
        <AppIcon name={refreshing ? "loading" : "refresh"} size={16} class={refreshing ? "spin" : ""} />
        {$t(refreshing ? "common.refreshing" : "common.refresh")}
      </button>
    </div>
  </section>

  <section class="panel-band">
    {#if toolActionMessage}
      <DismissibleNotice tone="success" message={toolActionMessage} on:dismiss={() => (toolActionMessage = null)} />
    {/if}
    {#if toolActionError}
      <DismissibleNotice tone="error" message={toolActionError} on:dismiss={() => (toolActionError = null)} />
    {/if}
    {#if envConflicts.length > 0}
      <div class="inline-error env-conflict-banner">
        <div>
          <strong>{$t("envConflict.title")}</strong>
          <span>{$t("envConflict.dashboardDescription", { count: envConflicts.length })}</span>
          <div class="conflict-chip-list">
            {#each envConflicts as conflict}
              <code>{conflict.scope}:{conflict.variable}={conflict.currentValuePreview}</code>
            {/each}
          </div>
        </div>
        <button class="secondary-button" disabled={clearingClaudeEnv} on:click={clearClaudeEnvConflicts}>
          {#if clearingClaudeEnv}
            <AppIcon name="loading" size={16} class="spin" />
          {:else}
            <AppIcon name="repair" size={16} />
          {/if}
          {$t("envConflict.clearAction")}
        </button>
      </div>
    {/if}

    <div class="section-heading">
      <div>
        <h2>{$t("dashboard.connectedClients")}</h2>
        <p>{snapshot ? $t("dashboard.toolsTracked", { count: connectedClients.length }) : $t("app.state.scanning")}</p>
      </div>
    </div>
    <div class="system-grid client-grid">
      {#each connectedClients as tool}
        <article
          class="system-card client-card"
          class:clickable-card={desktopClientRouteForTool(tool.id)}
          role={desktopClientRouteForTool(tool.id) ? "button" : undefined}
          on:click={(event) => { if (event.target !== event.currentTarget) return; const r = desktopClientRouteForTool(tool.id); if (r) onNavigateToClient(tool.id); }}
          on:keydown={(event) => { if (event.key === "Enter" || event.key === " ") { const r = desktopClientRouteForTool(tool.id); if (r) { event.preventDefault(); onNavigateToClient(tool.id); } } }}
        >
          <div class="system-main">
            <ToolIcon toolId={tool.id} label={tool.name} category={tool.category} />
            <div class="system-copy">
              <h3>{tool.name}</h3>
              <p>{tool.version ?? tool.details ?? tool.command}</p>
              {#if resolvedToolDetail(tool)}
                <span class="tool-path">{resolvedToolDetail(tool)}</span>
              {/if}
              {#if tool.updateAvailable && tool.latestVersion}
                <span class="tool-path">{$t("tool.latestVersion", { version: tool.latestVersion })}</span>
              {/if}
              {#if tool.pathRepair}
                <span class="tool-path">{tool.pathRepair.message}</span>
              {/if}
            </div>
          </div>

          <div class="system-card-state client-card-state">
            <StatusPill
              status={tool.installState}
              label={$t(`status.${tool.installState}` as Parameters<typeof $t>[0])}
            />
          </div>
          <div class="client-card-actions">
            {#if tool.installState === "missing"}
              {#if tool.pathRepair}
                <button
                  class="secondary-button"
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if isRepairingTool(tool)}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {isRepairingTool(tool) ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              <button
                class="secondary-button"
                title={$t("tool.installCommand", { name: tool.name })}
                disabled={isToolActionBusy(tool)}
                on:click={() => openInstallPlan(tool)}
              >
                {#if isInstallingTool(tool)}
                  <AppIcon name="loading" size={16} class="spin" />
                {:else}
                  <AppIcon name="install" size={16} />
                {/if}
                {isInstallingTool(tool) ? $t("tool.installing") : $t("common.install")}
              </button>
            {:else}
              {#if tool.updateAvailable}
                <button
                  class="secondary-button"
                  title={$t("tool.updateCommand", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => openToolActionPlan(tool, "update")}
                >
                  {#if isUpdatingTool(tool)}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="update" size={16} />
                  {/if}
                  {isUpdatingTool(tool) ? $t("tool.updating") : $t("common.update")}
                </button>
              {/if}
              {#if tool.pathRepair}
                <button
                  class="secondary-button"
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if isRepairingTool(tool)}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {isRepairingTool(tool) ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              {#if canShowToolLaunch(tool)}
                <button
                  class="secondary-button"
                  title={$t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restartTitle" : "toolLaunch.actionTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => openToolLaunch(tool)}
                >
                  {#if isLaunchingTool(tool)}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="play" size={16} />
                  {/if}
                  {isLaunchingTool(tool) ? $t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restarting" : "toolLaunch.starting") : $t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restart" : "toolLaunch.action")}
                </button>
              {/if}
              <button
                class="secondary-button"
                title={$t("tool.createConfig", { name: tool.name })}
                disabled={isToolActionBusy(tool)}
                on:click={() => onConfigureTool(tool)}
              >
                <AppIcon name="settings" size={16} />
                {$t("common.createConfig")}
              </button>
            {/if}
          </div>
        </article>
      {:else}
        <div class="empty-row">{$t("dashboard.noClientSnapshot")}</div>
      {/each}
    </div>
  </section>

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("dashboard.system")}</h2>
        <p>{$t("dashboard.systemDeps")}</p>
      </div>
    </div>
    <div class="system-grid">
      {#each snapshot?.system ?? [] as tool}
        <article class="system-card">
          <div class="system-main">
            <ToolIcon toolId={tool.id} label={tool.name} category={tool.category} />
            <div class="system-copy">
              <h3>{tool.name}</h3>
              <p>{tool.version ?? tool.details ?? tool.command}</p>
              {#if tool.updateAvailable && tool.latestVersion}
                <span class="tool-path">{$t("tool.latestVersion", { version: tool.latestVersion })}</span>
              {/if}
            </div>
          </div>

          <div class="system-card-state">
            <StatusPill
              status={tool.installState}
              label={$t(`status.${tool.installState}` as Parameters<typeof $t>[0])}
            />
          </div>
          <div class="client-card-actions">
            {#if tool.installState === "missing"}
              {#if tool.pathRepair}
                <button
                  class="secondary-button"
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              <button
                class="secondary-button"
                title={$t("tool.installCommand", { name: tool.name })}
                disabled={isToolActionBusy(tool)}
                on:click={() => openInstallPlan(tool)}
              >
                {#if isInstallingTool(tool)}
                  <AppIcon name="loading" size={16} class="spin" />
                {:else}
                  <AppIcon name="install" size={16} />
                {/if}
                {isInstallingTool(tool) ? $t("tool.installing") : $t("common.install")}
              </button>
            {:else if tool.updateAvailable}
              <button
                class="secondary-button"
                title={$t("tool.updateCommand", { name: tool.name })}
                disabled={isToolActionBusy(tool)}
                on:click={() => openToolActionPlan(tool, "update")}
              >
                {#if updatingToolId === tool.id}
                  <AppIcon name="loading" size={16} class="spin" />
                {:else}
                  <AppIcon name="update" size={16} />
                {/if}
                {updatingToolId === tool.id ? $t("tool.updating") : $t("common.update")}
              </button>
              {#if tool.pathRepair}
                <button
                  class="secondary-button"
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              {#if canShowToolLaunch(tool)}
                <button
                  class="secondary-button"
                  title={$t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restartTitle" : "toolLaunch.actionTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => openToolLaunch(tool)}
                >
                  {#if isLaunchingTool(tool)}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="play" size={16} />
                  {/if}
                  {isLaunchingTool(tool) ? $t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restarting" : "toolLaunch.starting") : $t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restart" : "toolLaunch.action")}
                </button>
              {/if}
            {:else}
              {#if tool.pathRepair}
                <button
                  class="secondary-button"
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              {#if canShowToolLaunch(tool)}
                <button
                  class="secondary-button"
                  title={$t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restartTitle" : "toolLaunch.actionTitle", { name: tool.name })}
                  disabled={isToolActionBusy(tool)}
                  on:click={() => openToolLaunch(tool)}
                >
                  {#if isLaunchingTool(tool)}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="play" size={16} />
                  {/if}
                  {isLaunchingTool(tool) ? $t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restarting" : "toolLaunch.starting") : $t(isManagedDesktopClient(tool) && tool.running ? "toolLaunch.restart" : "toolLaunch.action")}
                </button>
              {/if}
            {/if}
          </div>
        </article>
      {:else}
        <div class="empty-row">{$t("dashboard.noSystemSnapshot")}</div>
      {/each}
    </div>
  </section>

</div>

{#if pendingInstallTool}
  <div class="modal-backdrop" role="presentation">
    <div class="modal-panel wide-modal" role="dialog" aria-modal="true" aria-labelledby="tool-install-title">
      <div class="modal-body">
        <div>
        <span class="eyebrow">{$t("toolInstall.eyebrow")}</span>
        <h2 id="tool-install-title">
          {$t(installMode === "update" ? "toolInstall.updateTitle" : "toolInstall.title", { name: pendingInstallTool.name })}
        </h2>
        <p>{$t("toolInstall.description")}</p>
      </div>

      {#if planningToolId}
        <div class="install-progress" aria-live="polite">
          <div class="progress-copy">
            <strong>{$t("toolInstall.planning")}</strong>
            <span>{pendingInstallTool.name}</span>
          </div>
          <div class="progress-track indeterminate">
            <span class="progress-fill"></span>
          </div>
        </div>
      {/if}

      {#if installPlan}
        <div class="install-command-box">
          <div>
            <strong>{$t("toolInstall.command")}</strong>
            <span>{$t("toolInstall.manager", { manager: installPlan.manager })}</span>
          </div>
          <div class="install-command-list">
            {#each installPlan.commands as command}
              <div>
                <span>{command.stage === "prerequisite" ? $t("toolInstall.stage.prerequisite") : installMode === "update" ? $t("common.update") : $t("toolInstall.stage.target")}</span>
                <code>{command.command}</code>
              </div>
            {/each}
          </div>
          <button class="icon-button" title={$t("toolInstall.copyCommand")} on:click={copyInstallCommand}>
            <AppIcon name="copy" size={18} />
          </button>
        </div>

        <div class="install-meta">
          <span>{installPlan.requiresAdmin ? $t("toolInstall.adminMayPrompt") : $t("toolInstall.userScope")}</span>
        </div>

        {#if installPlan.prerequisites.length > 0}
          <div class="preview-list">
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
          <div class="inline-error">{installPlan.blocker}</div>
        {/if}

        {#if installMode !== "update" && installPlan.steps.length > 0}
          <div class="preview-list">
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
        <div class="install-progress" aria-live="polite">
          <div class="progress-copy">
            <strong>{$t(installMode === "update" ? "tool.updating" : "tool.installing")}</strong>
            <span>{installPlan?.command}</span>
          </div>
          <div class="progress-track indeterminate">
            <span class="progress-fill"></span>
          </div>
        </div>
      {/if}

      {#if installPlan?.interactive}
        <div class="install-terminal-card">
          <div class="install-terminal-header">
            <strong>{$t("toolInstall.terminalTitle")}</strong>
            {#if installTerminalRunning}
              <span>{$t("common.running")}</span>
            {:else if installTerminalExitCode !== null}
              <span>{$t("toolInstall.terminalExit", { code: installTerminalExitCode })}</span>
            {:else}
              <span>{$t("toolInstall.terminalReady")}</span>
            {/if}
          </div>
          <div class="install-terminal-frame" bind:this={installTerminalElement}></div>
        </div>
      {/if}

      {#if !installPlan?.interactive && (installingToolId || updatingToolId || hasLiveInstallLogs)}
        <div class="install-log live-install-log">
          <strong>{$t("toolInstall.consoleOutput")}</strong>
          <div class="install-log-viewport" bind:this={installLogViewport}>
            {#each liveInstallLogGroups as group (group.key)}
              <div class="install-log-stage">
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
              <div class="install-log-stage">
                <span>{$t("common.loading")}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      {#if installResult}
        <div class={installResult.success ? "inline-success" : "inline-error"}>
          {localizedInstallResultMessage(installResult)}
        </div>
        <div class="install-result-grid">
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
          <div class="preview-list">
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
          <div class="install-log">
            <strong>{$t("toolInstall.consoleOutput")}</strong>
            {#each installResult.stageResults as stage}
              {#if stage.stdoutTail || stage.stderrTail}
                <div class="install-log-stage">
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
        <div class="inline-error">{installError}</div>
      {/if}

      </div>

      <div class="modal-actions">
        <button class="secondary-button" on:click={closeInstallPlan} disabled={Boolean(installingToolId)}>
          {$t(installResult ? "common.close" : "common.cancel")}
        </button>
        {#if installTerminalRunning}
          <button class="secondary-button" type="button" on:click={stopInteractiveInstall}>
            <AppIcon name="stop" size={16} />
            {$t("common.stop")}
          </button>
        {/if}
        {#if installPlan && !installResult}
          <button
            class="primary-button"
            on:click={confirmInstallAction}
            disabled={!installPlan.canInstall || Boolean(installingToolId)}
          >
            {#if installingToolId}
              <AppIcon name="loading" size={16} class="spin" />
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
  <div class="modal-backdrop" role="presentation">
    <div class="modal-panel wide-modal" role="dialog" aria-modal="true" aria-labelledby="tool-launch-title">
      <div class="modal-body">
        <div>
          <span class="eyebrow">{$t("toolLaunch.eyebrow")}</span>
          <h2 id="tool-launch-title">{$t("toolLaunch.title", { name: pendingLaunchTool.name })}</h2>
          <p>{$t("toolLaunch.description")}</p>
        </div>

        {#if planningLaunchToolId}
          <div class="install-progress" aria-live="polite">
            <div class="progress-copy">
              <strong>{$t("toolLaunch.planning")}</strong>
              <span>{pendingLaunchTool.name}</span>
            </div>
            <div class="progress-track indeterminate">
              <span class="progress-fill"></span>
            </div>
          </div>
        {/if}

        {#if launchPlan}
          <div class="install-command-box">
            <div>
              <strong>{$t("toolLaunch.command")}</strong>
              <span>{launchPlan.toolName}</span>
            </div>
            <code>{launchPlan.command}</code>
          </div>

          <section class="launch-section">
            <div class="launch-section-heading">
              <strong>{$t("toolLaunch.configSource")}</strong>
              {#if selectedLaunchProfile()}
                <span>{$t("toolLaunch.temporaryProfile")}</span>
              {:else}
                <span>{$t("toolLaunch.globalConfigDescription")}</span>
              {/if}
            </div>
            <div class="launch-option-grid">
              <button
                type="button"
                class:selected={!selectedLaunchProfileId}
                class="launch-option"
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
                  class:selected={selectedLaunchProfileId === profile.id}
                  class="launch-option"
                  aria-pressed={selectedLaunchProfileId === profile.id}
                  disabled={launchTerminalRunning}
                  on:click={() => (selectedLaunchProfileId = profile.id)}
                >
                  <strong>{profile.name}</strong>
                  <span>{profile.provider === "official" ? $t("toolLaunch.officialProfile") : profile.baseUrl || profile.provider}</span>
                </button>
              {:else}
                <div class="launch-empty-option">{$t("toolLaunch.noProfiles")}</div>
              {/each}
            </div>
          </section>

          <section class="launch-section">
            <div class="launch-section-heading">
              <strong>{$t("toolLaunch.console")}</strong>
              <span>{launchPlan.shells.find((shell) => shell.id === selectedLaunchShellId)?.command ?? $t("common.none")}</span>
            </div>
            <div class="launch-option-grid compact">
              {#each launchPlan.shells as shell}
                <button
                  type="button"
                  class:selected={selectedLaunchShellId === shell.id}
                  class="launch-option"
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

          <section class="launch-section">
            <div class="launch-section-heading">
              <strong>{$t("toolLaunch.launchMode")}</strong>
              <span>{$t(launchMode === "embedded" ? "toolLaunch.embeddedDescription" : "toolLaunch.externalDescription")}</span>
            </div>
            <div class="launch-option-grid compact">
              <button
                type="button"
                class:selected={launchMode === "embedded"}
                class="launch-option"
                aria-pressed={launchMode === "embedded"}
                disabled={launchTerminalRunning || Boolean(launchingToolId)}
                on:click={() => (launchMode = "embedded")}
              >
                <strong>{$t("toolLaunch.embedded")}</strong>
                <span>{$t("toolLaunch.embeddedHint")}</span>
              </button>
              <button
                type="button"
                class:selected={launchMode === "external"}
                class="launch-option"
                aria-pressed={launchMode === "external"}
                disabled={launchTerminalRunning || Boolean(launchingToolId)}
                on:click={() => (launchMode = "external")}
              >
                <strong>{$t("toolLaunch.external")}</strong>
                <span>{$t("toolLaunch.externalHint")}</span>
              </button>
            </div>
          </section>

          <section class="launch-section">
            <label class="launch-directory-field">
              <span>{$t("toolLaunch.workingDirectory")}</span>
              <input
                bind:value={launchWorkingDirectory}
                disabled={launchTerminalRunning}
                placeholder={$t("toolLaunch.workingDirectoryPlaceholder")}
              />
            </label>
          </section>

          {#if launchPlan.blocker}
            <div class="inline-error">{launchPlan.blocker}</div>
          {/if}
        {/if}

        {#if launchingToolId || launchTerminalSessionId || launchTerminalExitCode !== null}
          <div class="install-terminal-card">
            <div class="install-terminal-header">
              <strong>{$t("toolLaunch.terminalTitle")}</strong>
              {#if launchTerminalRunning}
                <span>{$t("common.running")}</span>
              {:else if launchTerminalExitCode !== null}
                <span>{$t("toolLaunch.terminalExit", { code: launchTerminalExitCode })}</span>
              {:else}
                <span>{$t("toolLaunch.terminalReady")}</span>
              {/if}
            </div>
            <div class="install-terminal-frame" bind:this={launchTerminalElement}></div>
          </div>
        {/if}

        {#if launchError}
          <div class="inline-error">{launchError}</div>
        {/if}
      </div>

      <div class="modal-actions">
        <button class="secondary-button" on:click={closeToolLaunch} disabled={launchTerminalRunning}>
          {$t(launchTerminalExitCode !== null ? "common.close" : "common.cancel")}
        </button>
        {#if launchTerminalRunning}
          <button class="secondary-button" type="button" on:click={stopToolLaunch}>
            <AppIcon name="stop" size={16} />
            {$t("common.stop")}
          </button>
        {/if}
        {#if launchPlan}
          <button
            class="primary-button"
            on:click={startToolLaunch}
            disabled={!launchPlan.canLaunch || !selectedLaunchShellId || launchTerminalRunning || Boolean(launchingToolId)}
          >
            {#if launchingToolId}
              <AppIcon name="loading" size={16} class="spin" />
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
