import { get, writable } from "svelte/store";
import {
  inspectClaudeDesktopPage,
  installTool,
  launchClaudeDesktop,
  listenClaudeDesktopLocalizationProgress,
  listenToolInstallProgress,
  loadCachedDetection,
  openClaudeDesktopPath,
  uninstallTool,
  updateTool
} from "./api";
import type {
  ClaudeDesktopInstallKinds,
  ClaudeDesktopLocalizationProgress,
  ClaudeDesktopPendingLaunch,
  ClaudeDesktopPlan,
  CodexClientCapability,
  DetectionSnapshot,
  ToolInstallPlan,
  ToolInstallProgress,
  ToolInstallResult,
  ToolStatus
} from "../types";
import { REFRESH_CACHE_TTL_MS, readRefreshTimestamp, refreshTimestampFresh, writeRefreshTimestamp } from "./refreshCache";

export const CLAUDE_DESKTOP_TOOL_ID = "claude-desktop";

export type ClaudeDesktopInstallKind = "msix" | "exe";

type ClaudeDesktopBusyAction = "install" | "update" | "uninstall";

interface ClaudeDesktopKindViewState {
  status: ToolStatus | null;
  installPlan: ToolInstallPlan | null;
  updatePlan: ToolInstallPlan | null;
  plan: ClaudeDesktopPlan | null;
  planRefreshing: boolean;
  loading: boolean;
  loaded: boolean;
  busyAction: ClaudeDesktopBusyAction | null;
  result: ToolInstallResult | null;
  progress: ToolInstallProgress | null;
  progressLogs: ToolInstallProgress[];
}

interface ClaudeDesktopViewState {
  snapshot: DetectionSnapshot | null;
  kindViews: Record<ClaudeDesktopInstallKind, ClaudeDesktopKindViewState>;
  loaded: boolean;
  error: string | null;
  success: string | null;
  confirmUninstall: boolean;
  // Persisted launch option (survives page switches and app restarts): whether
  // the Claude Desktop launch button applies runtime Chinese localization.
  // Stored in the view store rather than component-local state so it is not
  // reset to its default when the page unmounts/remounts on navigation.
  localizeLaunch: boolean;
  localizationProgress: ClaudeDesktopLocalizationProgress | null;
  // Per-kind install detection (MSIX vs native .exe) for the page tabs.
  installKinds: ClaudeDesktopInstallKinds | null;
  // Which install-kind tab is selected: "msix" (Windows App) or "exe".
  selectedKind: ClaudeDesktopInstallKind;
  // Local MSIX-runtime capability checks for the Windows App tab.
  capabilities: CodexClientCapability[];
  pendingLaunchAfterRestart: ClaudeDesktopPendingLaunch | null;
}

const INSTALL_KINDS: ClaudeDesktopInstallKind[] = ["msix", "exe"];
const LOCALIZE_LAUNCH_STORAGE_KEY = "codestudio-lite-claude-localize-launch";
const LOCALIZE_LAUNCH_INITIALIZED_KEY = "codestudio-lite-claude-localize-launch-initialized";
const PLAN_CACHE_KEY = "codestudio-lite:claude-desktop-plan";

function readPersistedLocalizeLaunch(): boolean {
  if (typeof localStorage === "undefined") {
    return false;
  }
  const stored = localStorage.getItem(LOCALIZE_LAUNCH_STORAGE_KEY);
  if (stored !== null) {
    return stored === "1";
  }
  // First launch: auto-enable localization when the system language is
  // Chinese (zh-CN or zh-TW). A separate "initialized" flag ensures this
  // only runs once; once the user manually toggles the option (on or off) the
  // persisted value takes precedence on all subsequent launches.
  if (localStorage.getItem(LOCALIZE_LAUNCH_INITIALIZED_KEY) !== "1") {
    const lang = navigator.language || "";
    if (lang.toLowerCase().startsWith("zh")) {
      localStorage.setItem(LOCALIZE_LAUNCH_STORAGE_KEY, "1");
      localStorage.setItem(LOCALIZE_LAUNCH_INITIALIZED_KEY, "1");
      return true;
    }
    localStorage.setItem(LOCALIZE_LAUNCH_STORAGE_KEY, "0");
    localStorage.setItem(LOCALIZE_LAUNCH_INITIALIZED_KEY, "1");
    return false;
  }
  return false;
}

function emptyKindView(): ClaudeDesktopKindViewState {
  return {
    status: null,
    installPlan: null,
    updatePlan: null,
    plan: null,
    planRefreshing: false,
    loading: false,
    loaded: false,
    busyAction: null,
    result: null,
    progress: null,
    progressLogs: []
  };
}

const initialState: ClaudeDesktopViewState = {
  snapshot: null,
  kindViews: {
    msix: emptyKindView(),
    exe: emptyKindView()
  },
  loaded: false,
  error: null,
  success: null,
  confirmUninstall: false,
  localizeLaunch: readPersistedLocalizeLaunch(),
  localizationProgress: null,
  installKinds: null,
  selectedKind: "msix",
  capabilities: [],
  pendingLaunchAfterRestart: null
};

export const claudeDesktopView = writable<ClaudeDesktopViewState>(initialState);

let loadPromise: Promise<void> | null = null;
let lastNavigationRefreshAt = Math.max(readRefreshTimestamp("claudeDesktop"), readRefreshTimestamp("detection"));
const NAVIGATION_REFRESH_TTL_MS = REFRESH_CACHE_TTL_MS;
let progressListenerStarted = false;
let localizationProgressListenerStarted = false;
let progressLogKeys = new Set<string>();

function patch(next: Partial<ClaudeDesktopViewState>) {
  claudeDesktopView.update((current) => ({ ...current, ...next }));
}

function patchKind(
  kind: ClaudeDesktopInstallKind,
  next: Partial<ClaudeDesktopKindViewState>
) {
  claudeDesktopView.update((current) => ({
    ...current,
    kindViews: {
      ...current.kindViews,
      [kind]: {
        ...current.kindViews[kind],
        ...next
      }
    }
  }));
}

function selectedKindView(view = get(claudeDesktopView)) {
  return view.kindViews[view.selectedKind];
}

function cachedClaudeDesktopPlan(): ClaudeDesktopPlan | null {
  if (typeof localStorage === "undefined") {
    return null;
  }
  try {
    const raw = localStorage.getItem(PLAN_CACHE_KEY);
    return raw ? JSON.parse(raw) as ClaudeDesktopPlan : null;
  } catch {
    return null;
  }
}

function storeClaudeDesktopPlan(plan: ClaudeDesktopPlan) {
  if (typeof localStorage === "undefined") {
    return;
  }
  localStorage.setItem(PLAN_CACHE_KEY, JSON.stringify(plan));
}

function applyClaudeDesktopPlan(plan: ClaudeDesktopPlan | null) {
  patchKind("msix", { plan });
}

function claudeDesktopExeInstallDetected(installKinds: ClaudeDesktopInstallKinds | null): boolean {
  return Boolean(installKinds?.exe?.installed);
}

export function claudeDesktopVisibleInstallKinds(
  view = get(claudeDesktopView)
): ClaudeDesktopInstallKind[] {
  return claudeDesktopExeInstallDetected(view.installKinds) ? ["msix", "exe"] : ["msix"];
}

function hasBusyAction(view = get(claudeDesktopView)) {
  return INSTALL_KINDS.some((kind) => Boolean(view.kindViews[kind].busyAction));
}

function hasLoadingView(view = get(claudeDesktopView)) {
  return INSTALL_KINDS.some((kind) => view.kindViews[kind].loading);
}

function normalizeInstallKind(value: string | null | undefined): ClaudeDesktopInstallKind {
  return value === "exe" ? "exe" : "msix";
}

function findClaudeDesktop(snapshot: DetectionSnapshot | null) {
  return snapshot?.tools.find((tool) => tool.id === CLAUDE_DESKTOP_TOOL_ID) ?? null;
}

function fallbackClaudeStatus(
  base: ToolStatus | null,
  kind: ClaudeDesktopInstallKind
): ToolStatus {
  return {
    id: CLAUDE_DESKTOP_TOOL_ID,
    name: "Claude Desktop",
    category: "ai_tool",
    command: base?.command ?? "Claude",
    pathRepair: base?.pathRepair ?? null,
    version: null,
    latestVersion: base?.latestVersion ?? null,
    updateAvailable: false,
    updateCommand: base?.updateCommand ?? null,
    installState: "missing",
    configState: base?.configState ?? "unknown",
    configPath: base?.configPath ?? null,
    installPath: null,
    installCommand: base?.installCommand ?? null,
    details: null,
    installKind: kind,
    running: base?.running ?? false
  };
}

function versionDiffers(version: string | null | undefined, latest: string | null | undefined) {
  return Boolean(version && latest && version !== latest);
}

function statusForInstallKind(
  base: ToolStatus | null,
  kind: ClaudeDesktopInstallKind,
  installKinds: ClaudeDesktopInstallKinds | null
): ToolStatus | null {
  const info = installKinds?.[kind] ?? null;
  if (!base && !info) {
    return null;
  }
  if (!installKinds && base && normalizeInstallKind(base.installKind) === kind) {
    return { ...base, installKind: kind };
  }

  const status = fallbackClaudeStatus(base, kind);
  if (!info?.installed) {
    return status;
  }

  const version = info.version ?? (base?.installKind === kind ? base.version : null);
  const installPath = info.path ?? (base?.installKind === kind ? base.installPath : null);
  return {
    ...status,
    version,
    installPath,
    installState: "installed",
    details: base?.installKind === kind
      ? base.details
      : installPath
        ? `Resolved: ${installPath} (${kind})`
        : null,
    updateAvailable: base?.installKind === kind
      ? base.updateAvailable
      : versionDiffers(version, base?.latestVersion)
  };
}

function applyKindStatusesFromSnapshot(
  snapshot: DetectionSnapshot,
  installPlan?: ToolInstallPlan | null,
  updatePlan?: ToolInstallPlan | null,
  capabilities?: CodexClientCapability[]
) {
  const baseStatus = findClaudeDesktop(snapshot);
  const isWindows = snapshot.platform === "windows";
  const installKinds = isWindows ? (snapshot.claudeInstallKinds ?? null) : null;
  claudeDesktopView.update((current) => ({
    ...current,
    snapshot,
    installKinds,
    selectedKind: isWindows ? current.selectedKind : "msix",
    capabilities: capabilities ?? current.capabilities,
    loaded: true,
    kindViews: {
      msix: {
        ...current.kindViews.msix,
        status: statusForInstallKind(baseStatus, "msix", installKinds),
        installPlan: installPlan === undefined ? current.kindViews.msix.installPlan : installPlan,
        updatePlan: updatePlan === undefined ? current.kindViews.msix.updatePlan : updatePlan,
        loaded: true
      },
      exe: {
        ...current.kindViews.exe,
        status: statusForInstallKind(baseStatus, "exe", installKinds),
        installPlan: null,
        updatePlan: null,
        busyAction: isWindows ? current.kindViews.exe.busyAction : null,
        loaded: true
      }
    }
  }));
}

function progressKey(progress: ToolInstallProgress) {
  return [
    progress.installKind ?? "",
    progress.rootToolId,
    progress.toolId,
    progress.stage,
    progress.command,
    progress.stream,
    progress.done ? "done" : "chunk",
    progress.exitCode ?? "",
    progress.chunk
  ].join("\u001f");
}

function progressInstallKind(progress: ToolInstallProgress): ClaudeDesktopInstallKind {
  if (progress.installKind === "exe" || progress.installKind === "msix") {
    return progress.installKind;
  }
  const view = get(claudeDesktopView);
  return INSTALL_KINDS.find((kind) => Boolean(view.kindViews[kind].busyAction)) ?? view.selectedKind;
}

function progressSeed(
  installKind: ClaudeDesktopInstallKind,
  message: string,
  total?: number | null,
  stepTotal?: number | null
): ToolInstallProgress {
  return {
    rootToolId: CLAUDE_DESKTOP_TOOL_ID,
    toolId: CLAUDE_DESKTOP_TOOL_ID,
    toolName: "Claude Desktop",
    stage: "target",
    command: "",
    installKind,
    phase: "preparing",
    message,
    downloaded: null,
    total: total ?? null,
    percent: null,
    step: 1,
    stepTotal: stepTotal ?? null,
    stream: "status",
    chunk: "",
    done: false,
    exitCode: null
  };
}

function progressMessageForEvent(progress: ToolInstallProgress, phase: string): string {
  if (progress.message) {
    return progress.message;
  }
  if (phase === "downloading") {
    return "claudeDesktop.progressDownloading";
  }
  if (phase === "installing") {
    return "claudeDesktop.progressInstalling";
  }
  if (phase === "done") {
    return "claudeDesktop.progressDone";
  }
  if (progress.chunk.trim()) {
    return progress.chunk.trim();
  }
  return "claudeDesktop.progressWorking";
}

function progressPhaseFromEvent(progress: ToolInstallProgress): string {
  if (progress.phase) {
    return progress.phase;
  }
  if (progress.done) {
    return "done";
  }
  if (progress.downloaded !== undefined && progress.downloaded !== null) {
    return "downloading";
  }
  if (progress.stage === "target" || progress.stage === "update") {
    return "installing";
  }
  return progress.stage || "preparing";
}

function progressFromInstallEvent(progress: ToolInstallProgress): ToolInstallProgress {
  const phase = progressPhaseFromEvent(progress);
  const done = progress.done || phase === "done";
  const percent = progress.percent ?? (done && progress.exitCode === 0 ? 100 : null);
  return {
    ...progress,
    phase,
    message: progressMessageForEvent(progress, phase),
    downloaded: progress.downloaded ?? null,
    total: progress.total ?? null,
    percent,
    step: progress.step ?? null,
    stepTotal: progress.stepTotal ?? null
  };
}

function pushProgress(progress: ToolInstallProgress) {
  if (progress.rootToolId !== CLAUDE_DESKTOP_TOOL_ID) {
    return;
  }
  const kind = progressInstallKind(progress);
  const key = progressKey(progress);
  if (progressLogKeys.has(key)) {
    return;
  }
  progressLogKeys.add(key);
  patchKind(kind, {
    progressLogs: [...get(claudeDesktopView).kindViews[kind].progressLogs, progress].slice(-240),
    progress: progressFromInstallEvent(progress)
  });
}

function clearProgress(kind: ClaudeDesktopInstallKind) {
  progressLogKeys = new Set<string>();
  patchKind(kind, { progress: null, progressLogs: [] });
}

export function startClaudeDesktopProgressListener() {
  if (progressListenerStarted) {
    return;
  }
  progressListenerStarted = true;
  listenToolInstallProgress(pushProgress).catch((err) => {
    progressListenerStarted = false;
    patch({ error: err instanceof Error ? err.message : String(err) });
  });
}

export function startClaudeDesktopLocalizationProgressListener() {
  if (localizationProgressListenerStarted) {
    return;
  }
  localizationProgressListenerStarted = true;
  listenClaudeDesktopLocalizationProgress((progress) => {
    patch({
      localizationProgress: progress,
      error: progress.phase === "failed" ? progress.error ?? progress.message : get(claudeDesktopView).error,
      success: progress.phase === "done" ? "claudeDesktop.localizationDone" : get(claudeDesktopView).success
    });
  }).catch((err) => {
    localizationProgressListenerStarted = false;
    patch({ error: err instanceof Error ? err.message : String(err) });
  });
}

export async function ensureClaudeDesktopLoaded() {
  startClaudeDesktopProgressListener();
  startClaudeDesktopLocalizationProgressListener();
  const snapshot = get(claudeDesktopView);
  if (hasBusyAction(snapshot)) {
    return;
  }
  hydrateClaudeDesktopPlanFromCache();
  if (!snapshot.loaded) {
    await hydrateClaudeDesktopFromCache();
  }
  const current = get(claudeDesktopView);
  lastNavigationRefreshAt = Math.max(readRefreshTimestamp("claudeDesktop"), readRefreshTimestamp("detection"));
  const stale = !refreshTimestampFresh("claudeDesktop", NAVIGATION_REFRESH_TTL_MS) && !refreshTimestampFresh("detection", NAVIGATION_REFRESH_TTL_MS);
  if (!loadPromise && !hasLoadingView() && !hasBusyAction() && (!current.loaded || stale)) {
    loadPromise = refreshClaudeDesktop(false, get(claudeDesktopView).selectedKind, { forceFresh: false })
      .finally(() => {
        loadPromise = null;
      });
  }
}

/// Hydrate the Claude Desktop view from the on-disk detection cache so the
/// page can render a prior scan's results instantly, before a fresh detect
/// completes. Marks the view as loaded so a subsequent navigation does not
/// re-block; the async re-scan still runs and supersedes this with live data.
async function hydrateClaudeDesktopFromCache() {
  try {
    const cached = await loadCachedDetection();
    if (cached) {
      applyKindStatusesFromSnapshot(cached);
    }
  } catch {
    // Cache read failures are non-fatal: the async re-scan will populate.
  }
}

function hydrateClaudeDesktopPlanFromCache() {
  const plan = cachedClaudeDesktopPlan();
  if (plan) {
    applyClaudeDesktopPlan(plan);
  }
}

export async function refreshClaudeDesktop(
  force = false,
  installKind: ClaudeDesktopInstallKind = get(claudeDesktopView).selectedKind,
  options: { forceFresh?: boolean } = {}
) {
  startClaudeDesktopProgressListener();
  startClaudeDesktopLocalizationProgressListener();
  if (get(claudeDesktopView).kindViews[installKind].busyAction && !force) {
    return;
  }
  patchKind(installKind, { loading: true });
  patch({ error: null });
  try {
    if (installKind === "msix") {
      patchKind("msix", { planRefreshing: true });
    }
    const state = await inspectClaudeDesktopPage(options.forceFresh ?? force);
    if (state.plan) {
      storeClaudeDesktopPlan(state.plan);
      applyClaudeDesktopPlan(state.plan);
    }
    lastNavigationRefreshAt = Date.now();
    writeRefreshTimestamp("claudeDesktop", lastNavigationRefreshAt);
    writeRefreshTimestamp("detection", lastNavigationRefreshAt);
    applyKindStatusesFromSnapshot(
      state.snapshot,
      state.installPlan,
      state.updatePlan,
      state.capabilities
    );
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
  } finally {
    patchKind(installKind, { loading: false, planRefreshing: false });
  }
}

async function runAction(
  kind: ClaudeDesktopInstallKind,
  action: ClaudeDesktopBusyAction,
  runner: () => Promise<ToolInstallResult>,
  initialProgress: ToolInstallProgress | null = null
) {
  startClaudeDesktopProgressListener();
  clearProgress(kind);
  patchKind(kind, { busyAction: action, result: null, progress: initialProgress });
  patch({ error: null, success: null });
  try {
    const result = await runner();
    const currentStatus = result.currentStatus;
    const resultStatusKind = currentStatus?.installKind
      ? normalizeInstallKind(currentStatus.installKind)
      : action === "uninstall" && currentStatus?.installState === "missing"
        ? kind
        : null;
    patchKind(kind, {
      result,
      status: resultStatusKind === kind
        ? currentStatus
        : get(claudeDesktopView).kindViews[kind].status
    });
    patch({ success: result.message });
    await refreshClaudeDesktop(true, kind);
    // MSIX package registration (winget install) can lag slightly behind the
    // installer process exiting. If the install/update action succeeded but
    // the immediate re-detect still does not see the selected install kind,
    // retry once after a short delay so the active page reflects the new
    // install without the user having to click refresh.
    if (action === "install" || action === "update") {
      const stillMissing = get(claudeDesktopView).kindViews[kind].status?.installState !== "installed";
      if (stillMissing) {
        await new Promise((resolve) => setTimeout(resolve, 2000));
        await refreshClaudeDesktop(true, kind);
      }
    }
    return result;
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
    return null;
  } finally {
    patchKind(kind, { busyAction: null });
  }
}

export async function launchClaudeDesktopFromDashboard() {
  startClaudeDesktopLocalizationProgressListener();
  patch({ localizationProgress: null });
  await launchClaudeDesktop({ localize: getClaudeDesktopLocalizeLaunch() });
}

export async function installOrUpdateClaudeDesktop(mode?: "install" | "update") {
  return installOrUpdateClaudeDesktopKind(get(claudeDesktopView).selectedKind, mode);
}

export async function installOrUpdateClaudeDesktopKind(
  installKind: ClaudeDesktopInstallKind,
  mode?: "install" | "update"
) {
  const state = get(claudeDesktopView);
  const kindView = state.kindViews[installKind];
  if (installKind === "exe") {
    patch({ error: "Claude Desktop EXE installation is no longer supported. Use the Windows App tab to install Claude Desktop." });
    return null;
  }
  const shouldUpdate = mode === "update" || (mode !== "install" && Boolean(kindView.status?.updateAvailable));
  const initialProgress = shouldUpdate
    ? progressSeed(installKind, "claudeDesktop.progressUpdatePreparing", null, 3)
    : progressSeed(installKind, "claudeDesktop.progressInstallPreparing", null, 3);
  return runAction(
    installKind,
    shouldUpdate ? "update" : "install",
    () =>
      shouldUpdate
        ? updateTool({ toolId: CLAUDE_DESKTOP_TOOL_ID, confirm: true, installKind })
        : installTool({
            toolId: CLAUDE_DESKTOP_TOOL_ID,
            confirm: true,
            installKind,
            installPrerequisites: kindView.installPlan?.requiresPrerequisites ?? true
          }),
    initialProgress
  );
}

export async function removeClaudeDesktop(installKind: ClaudeDesktopInstallKind = get(claudeDesktopView).selectedKind) {
  return runAction(installKind, "uninstall", () =>
    uninstallTool({
      toolId: CLAUDE_DESKTOP_TOOL_ID,
      confirm: true,
      installKind
    })
  ).finally(() => {
    patch({ confirmUninstall: false });
  });
}

export function setClaudeDesktopConfirmUninstall(confirmUninstall: boolean) {
  patch({ confirmUninstall });
}

/// Toggle the persisted "localize launch" option. Writes to the view store so
/// it survives page switches, and to localStorage so it survives app restarts.
export function getClaudeDesktopLocalizeLaunch(): boolean {
  return get(claudeDesktopView).localizeLaunch;
}

export function setClaudeDesktopLocalizeLaunch(localizeLaunch: boolean) {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(LOCALIZE_LAUNCH_STORAGE_KEY, localizeLaunch ? "1" : "0");
  }
  patch({ localizeLaunch });
}

export function dismissClaudeDesktopError() {
  patch({ error: null });
}

export function dismissClaudeDesktopSuccess() {
  patch({ success: null });
}

/// Select the install-kind tab ("msix" or "exe") on the Claude Desktop page.
export function setClaudeDesktopSelectedKind(kind: ClaudeDesktopInstallKind) {
  patch({ selectedKind: kind });
}

export function setClaudeDesktopPendingLaunchAfterRestart(
  pendingLaunchAfterRestart: ClaudeDesktopPendingLaunch | null
) {
  patch({ pendingLaunchAfterRestart });
}

export function consumeClaudeDesktopPendingLaunchAfterRestart(): ClaudeDesktopPendingLaunch | null {
  const pending = get(claudeDesktopView).pendingLaunchAfterRestart;
  if (pending) {
    patch({ pendingLaunchAfterRestart: null });
  }
  return pending;
}

export async function openClaudeDesktopStagingPath() {
  try {
    await openClaudeDesktopPath("staging");
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
  }
}
