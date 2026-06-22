import { get, writable } from "svelte/store";
import {
  detectEnvironmentFresh,
  installTool,
  launchClaudeDesktop,
  listenToolInstallProgress,
  loadCachedDetection,
  planToolInstall,
  planToolUpdate,
  uninstallTool,
  updateTool
} from "./api";
import type {
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
}

const LOCALIZE_LAUNCH_STORAGE_KEY = "codestudio-lite-claude-localize-launch";

function readPersistedLocalizeLaunch(): boolean {
  if (typeof localStorage === "undefined") {
    return false;
  }
  return localStorage.getItem(LOCALIZE_LAUNCH_STORAGE_KEY) === "1";
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
  localizeLaunch: readPersistedLocalizeLaunch()
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
      patch({ snapshot: cached, status, loaded: true });
    }
  } catch {
    // Cache read failures are non-fatal: the async re-scan will populate.
  }
}

export async function refreshClaudeDesktop() {
  startClaudeDesktopProgressListener();
  if (get(claudeDesktopView).busyAction) {
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
    patch({
      snapshot,
      status,
      installPlan,
      updatePlan,
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
    await refreshClaudeDesktop();
    // MSIX package registration (winget install) can lag slightly behind the
    // installer process exiting. If the install/update action succeeded but
    // the immediate re-detect still does not see Claude installed, retry once
    // after a short delay so the page reflects the new install without the
    // user having to click refresh.
    if (action === "install" || action === "update") {
      const stillMissing = get(claudeDesktopView).status?.installState !== "installed";
      if (stillMissing) {
        await new Promise((resolve) => setTimeout(resolve, 2000));
        await refreshClaudeDesktop();
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
  await refreshClaudeDesktop();
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
  return runAction("uninstall", () =>
    uninstallTool({
      toolId: CLAUDE_DESKTOP_TOOL_ID,
      confirm: true
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

