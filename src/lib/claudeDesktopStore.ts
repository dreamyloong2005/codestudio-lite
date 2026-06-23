import { get, writable } from "svelte/store";
import {
  detectClaudeCapabilities,
  detectEnvironmentFresh,
  installTool,
  launchClaudeDesktop,
  listenToolInstallProgress,
  loadCachedDetection,
  openClaudeDesktopPath,
  planToolInstall,
  planToolUpdate,
  uninstallTool,
  updateTool
} from "./api";
import type {
  ClaudeDesktopInstallKinds,
  CodexClientCapability,
  DetectionSnapshot,
  ToolInstallPlan,
  ToolInstallProgress,
  ToolInstallResult,
  ToolStatus
} from "../types";

export const CLAUDE_DESKTOP_TOOL_ID = "claude-desktop";

interface ClaudeDesktopViewState {
  snapshot: DetectionSnapshot | null;
  status: ToolStatus | null;
  installPlan: ToolInstallPlan | null;
  updatePlan: ToolInstallPlan | null;
  loading: boolean;
  loaded: boolean;
  busyAction: "install" | "update" | "uninstall" | null;
  error: string | null;
  success: string | null;
  result: ToolInstallResult | null;
  progressLogs: ToolInstallProgress[];
  confirmUninstall: boolean;
  // Persisted launch option (survives page switches and app restarts): whether
  // the Claude Desktop launch button applies the in-place Chinese-localization
  // patch. Stored in the view store rather than component-local state so it is
  // not reset to its default when the page unmounts/remounts on navigation.
  localizeLaunch: boolean;
  // Per-kind install detection (MSIX vs native .exe) for the page tabs.
  installKinds: ClaudeDesktopInstallKinds | null;
  // Which install-kind tab is selected: "msix" (Windows App) or "exe".
  selectedKind: "msix" | "exe";
  // Local MSIX-runtime capability checks for the Windows App tab.
  capabilities: CodexClientCapability[];
}

const LOCALIZE_LAUNCH_STORAGE_KEY = "codestudio-lite-claude-localize-launch";

const LOCALIZE_LAUNCH_INITIALIZED_KEY = "codestudio-lite-claude-localize-launch-initialized";

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
  // only runs once — once the user manually toggles the option (on or
  // off) the persisted value takes precedence on all subsequent launches.
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

const initialState: ClaudeDesktopViewState = {
  snapshot: null,
  status: null,
  installPlan: null,
  updatePlan: null,
  loading: false,
  loaded: false,
  busyAction: null,
  error: null,
  success: null,
  result: null,
  progressLogs: [],
  confirmUninstall: false,
  localizeLaunch: readPersistedLocalizeLaunch(),
  installKinds: null,
  selectedKind: "msix",
  capabilities: []
};

export const claudeDesktopView = writable<ClaudeDesktopViewState>(initialState);

let loadPromise: Promise<void> | null = null;
let progressListenerStarted = false;
let progressLogKeys = new Set<string>();

function patch(next: Partial<ClaudeDesktopViewState>) {
  claudeDesktopView.update((current) => ({ ...current, ...next }));
}

function findClaudeDesktop(snapshot: DetectionSnapshot | null) {
  return snapshot?.tools.find((tool) => tool.id === CLAUDE_DESKTOP_TOOL_ID) ?? null;
}

function progressKey(progress: ToolInstallProgress) {
  return [
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

function pushProgress(progress: ToolInstallProgress) {
  if (progress.rootToolId !== CLAUDE_DESKTOP_TOOL_ID) {
    return;
  }
  const key = progressKey(progress);
  if (progressLogKeys.has(key)) {
    return;
  }
  progressLogKeys.add(key);
  claudeDesktopView.update((current) => ({
    ...current,
    progressLogs: [...current.progressLogs, progress].slice(-240)
  }));
}

function clearProgress() {
  progressLogKeys = new Set<string>();
  patch({ progressLogs: [] });
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

export async function ensureClaudeDesktopLoaded() {
  startClaudeDesktopProgressListener();
  const snapshot = get(claudeDesktopView);
  if (snapshot.loaded || snapshot.loading || snapshot.busyAction) {
    return;
  }
  // Before the first in-memory scan lands, hydrate the view from the on-disk
  // detection cache so the page renders immediately with a prior scan's
  // results instead of blocking on a fresh detect. The cache is then
  // superseded by an async re-scan (below) so the data stays current without
  // making the user wait to see the page.
  if (!snapshot.loaded) {
    await hydrateClaudeDesktopFromCache();
  }
  // Kick off a background re-scan but do not block navigation on it: the page
  // already shows the cached state, and the fresh result updates the store
  // (and the reactive UI) when it arrives.
  if (!loadPromise && !get(claudeDesktopView).loading && !get(claudeDesktopView).busyAction) {
    loadPromise = refreshClaudeDesktop()
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
      const status = findClaudeDesktop(cached);
      patch({
        snapshot: cached,
        status,
        // Restore the per-kind install detection from the cached snapshot so
        // the tabs render instantly before the async re-scan completes.
        installKinds: cached.claudeInstallKinds ?? null,
        loaded: true
      });
    }
  } catch {
    // Cache read failures are non-fatal: the async re-scan will populate.
  }
}

export async function refreshClaudeDesktop(force = false) {
  startClaudeDesktopProgressListener();
  if (get(claudeDesktopView).busyAction && !force) {
    return;
  }
  patch({ loading: true, error: null });
  try {
    // Use the fresh (cache-invalidating) detection so a manual refresh or a
    // post-install re-detect always re-resolves from scratch instead of
    // serving a stale in-process install cache (e.g. MSIX Get-AppxPackage
    // result held for 30s).
    const snapshot = await detectEnvironmentFresh();
    const status = findClaudeDesktop(snapshot);
    const [installPlan, updatePlan] = await Promise.all([
      planToolInstall(CLAUDE_DESKTOP_TOOL_ID).catch(() => null),
      planToolUpdate(CLAUDE_DESKTOP_TOOL_ID).catch(() => null)
    ]);
    const capabilities = await detectClaudeCapabilities().catch(() => []);
    patch({
      snapshot,
      status,
      installPlan,
      updatePlan,
      installKinds: snapshot.claudeInstallKinds ?? null,
      capabilities,
      loaded: true
    });
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
  } finally {
    patch({ loading: false });
  }
}

async function runAction(
  action: "install" | "update" | "uninstall",
  runner: () => Promise<ToolInstallResult>
) {
  startClaudeDesktopProgressListener();
  clearProgress();
  patch({ busyAction: action, error: null, success: null, result: null });
  try {
    const result = await runner();
    patch({
      result,
      success: result.message,
      status: result.currentStatus ?? get(claudeDesktopView).status
    });
    await refreshClaudeDesktop(true);
    // MSIX package registration (winget install) can lag slightly behind the
    // installer process exiting. If the install/update action succeeded but
    // the immediate re-detect still does not see Claude installed, retry once
    // after a short delay so the page reflects the new install without the
    // user having to click refresh.
    if (action === "install" || action === "update") {
      const stillMissing = get(claudeDesktopView).status?.installState !== "installed";
      if (stillMissing) {
        await new Promise((resolve) => setTimeout(resolve, 2000));
        await refreshClaudeDesktop(true);
      }
    }
    return result;
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
    return null;
  } finally {
    patch({ busyAction: null });
  }
}

export async function launchClaudeDesktopFromDashboard() {
  await launchClaudeDesktop({ localize: getClaudeDesktopLocalizeLaunch() });
  await new Promise((resolve) => setTimeout(resolve, 2500));
  await refreshClaudeDesktop(true);
}

export async function installOrUpdateClaudeDesktop(mode?: "install" | "update") {
  const state = get(claudeDesktopView);
  const shouldUpdate = mode === "update" || (mode !== "install" && Boolean(state.status?.updateAvailable));
  return runAction(shouldUpdate ? "update" : "install", () =>
    shouldUpdate
      ? updateTool({ toolId: CLAUDE_DESKTOP_TOOL_ID, confirm: true })
      : installTool({
          toolId: CLAUDE_DESKTOP_TOOL_ID,
          confirm: true,
          installPrerequisites: state.installPlan?.requiresPrerequisites ?? true
        })
  );
}

export async function removeClaudeDesktop() {
  const view = get(claudeDesktopView);
  // Uninstall the install kind the user is currently viewing on the page
  // tab. Fall back to the detected kind if the selected tab has no install.
  let installKind = view.selectedKind;
  if (installKind === "exe" && !view.installKinds?.exe?.installed) {
    installKind = "msix";
  }
  return runAction("uninstall", () =>
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
export function setClaudeDesktopSelectedKind(kind: "msix" | "exe") {
  patch({ selectedKind: kind });
}

export async function openClaudeDesktopStagingPath() {
  try {
    await openClaudeDesktopPath("staging");
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
  }
}
