import { get, writable } from "svelte/store";
import {
  inspectCodexClient,
  installCodexClient,
  loadCachedCodexClientState,
  launchCodexClient,
  listenCodexClientProgress,
  planCodexClientUpdate,
  stageCodexClientUpdate,
  uninstallCodexClient,
  updateCodexClientSettings
} from "./api";
import type {
  CodexClientOperationResult,
  CodexClientProgress,
  CodexClientSettings,
  CodexClientStageReport,
  CodexClientState
} from "../types";
import type { TranslationKey } from "./i18n";

export type CodexClientNoticeMessage =
  | string
  | { key: TranslationKey; values?: Record<string, string | number> };

interface CodexClientViewState {
  state: CodexClientState | null;
  settingsDraft: CodexClientSettings | null;
  settingsSaveStatus: "idle" | "dirty" | "saving" | "saved" | "error";
  loading: boolean;
  loaded: boolean;
  busyAction: string | null;
  error: string | null;
  success: CodexClientNoticeMessage | null;
  stageReport: CodexClientStageReport | null;
  operationResult: CodexClientOperationResult | null;
  progress: CodexClientProgress | null;
  confirmUninstall: boolean;
}

const initialState: CodexClientViewState = {
  state: null,
  // Pre-seeded with backend defaults so the launch options section renders and
  // is editable before the first scan completes. applyState replaces this with
  // the real settings once loaded, while preserving any pre-scan edits to the
  // launch-option fields.
  settingsDraft: defaultCodexClientSettings(),
  settingsSaveStatus: "idle",
  loading: false,
  loaded: false,
  busyAction: null,
  error: null,
  success: null,
  stageReport: null,
  operationResult: null,
  progress: null,
  confirmUninstall: false
};

export const codexClientView = writable<CodexClientViewState>(initialState);

let loadPromise: Promise<void> | null = null;
let progressListenerStarted = false;
let settingsSaveTimer: ReturnType<typeof window.setTimeout> | null = null;
let settingsSaveInFlight = false;
let settingsSaveRevision = 0;
let lastSavedSettingsKey: string | null = null;

const SETTINGS_SAVE_DEBOUNCE_MS = 650;

// Defaults mirror the backend's CodexClientSettings::default() so a pre-scan
// draft looks like what the scan would return. Used only to render the launch
// options before the first scan completes; applyState preserves any edits the
// user made to these fields during that window.
function defaultCodexClientSettings(): CodexClientSettings {
  return {
    source: "mirror",
    customUrl: "",
    autoCheck: true,
    askBefore: true,
    signedOnly: true,
    windowsInstallMode: "msix",
    installRoot: "",
    keepUserDataOnUninstall: true,
    syncHistoryOnLaunch: false,
    patchForcePluginUnlock: false
  };
}

function patch(next: Partial<CodexClientViewState>) {
  codexClientView.update((current) => ({ ...current, ...next }));
}

function settingsKey(settings: CodexClientSettings) {
  return JSON.stringify({
    source: settings.source,
    customUrl: settings.customUrl,
    autoCheck: settings.autoCheck,
    askBefore: settings.askBefore,
    signedOnly: settings.signedOnly,
    windowsInstallMode: settings.windowsInstallMode,
    installRoot: settings.installRoot,
    keepUserDataOnUninstall: settings.keepUserDataOnUninstall,
    syncHistoryOnLaunch: settings.syncHistoryOnLaunch,
    patchForcePluginUnlock: settings.patchForcePluginUnlock
  });
}

function applyState(state: CodexClientState) {
  const current = get(codexClientView);
  const draft = current.settingsDraft;
  // If the user edited a launch option before the scan completed, keep their
  // choice instead of clobbering it with the scanned value. Only the two
  // launch-option toggles are preserved this way; other settings always reflect
  // the authoritative scanned state.
  const preserveLaunchOptions = !current.loaded && Boolean(draft);
  const mergedSettings: CodexClientSettings = preserveLaunchOptions && draft
    ? {
        ...state.settings,
        syncHistoryOnLaunch: draft.syncHistoryOnLaunch,
        patchForcePluginUnlock: draft.patchForcePluginUnlock
      }
    : state.settings;
  lastSavedSettingsKey = settingsKey(mergedSettings);
  patch({
    state,
    settingsDraft: { ...mergedSettings },
    loaded: true,
    settingsSaveStatus: "idle"
  });
}

function applyInstallResult(result: CodexClientOperationResult) {
  const current = get(codexClientView);
  const state = current.state;
  if (!state || !result.installed) {
    return;
  }

  const nextPlan = state.plan
    ? {
        ...state.plan,
        upToDate: true,
        currentVersion: result.installed.version,
        stagedPath: result.stage?.stagedPath ?? null
      }
    : state.plan;

  patch({
    state: {
      ...state,
      generatedAt: new Date().toISOString(),
      installed: result.installed,
      installClass: "managed",
      plan: nextPlan
    },
    stageReport: result.stage,
    settingsDraft: { ...state.settings }
  });
}

function progressSeed(message: string, total?: number | null, stepTotal?: number | null): CodexClientProgress {
  return {
    phase: "preparing",
    message,
    downloaded: null,
    total: total ?? null,
    percent: null,
    step: 1,
    stepTotal: stepTotal ?? null
  };
}

export function startCodexClientProgressListener() {
  if (progressListenerStarted) {
    return;
  }
  progressListenerStarted = true;
  listenCodexClientProgress((progress) => {
    patch({ progress });
  }).catch((err) => {
    progressListenerStarted = false;
    patch({ error: err instanceof Error ? err.message : String(err) });
  });
}

// Hydrate the Codex client view from the on-disk state cache so the page can
// render a prior session plan instantly, before a fresh network re-fetch
// completes. Mirrors the Claude Desktop page hydrateClaudeDesktopFromCache.
// Marks the view as loaded so a subsequent navigation does not re-block; the
// async re-scan still runs and supersedes this with live data.
async function hydrateCodexClientFromCache(): Promise<boolean> {
  try {
    const cached = await loadCachedCodexClientState();
    if (cached) {
      // Pre-mark loaded so applyState uses the cached settings verbatim
      // instead of preserving the default-seeded draft launch options.
      patch({ loaded: true });
      applyState(cached);
      return true;
    }
  } catch {
    // Cache read failures are non-fatal: the async re-fetch will populate.
  }
  return false;
}

export async function ensureCodexClientLoaded() {
  startCodexClientProgressListener();
  const snapshot = get(codexClientView);
  if (snapshot.loaded || snapshot.loading || snapshot.busyAction) {
    return;
  }
  // Hydrate from the on-disk cache first so the page renders instantly with
  // a prior session plan, then kick off an async re-fetch to stay current.
  const hydrated = await hydrateCodexClientFromCache();
  if (!loadPromise && !get(codexClientView).loading && !get(codexClientView).busyAction) {
    if (hydrated) {
      // We already have a full cached state; only re-fetch the network plan
      // when auto-check is enabled, and skip the plan-less local inspect so
      // the cached plan stays visible until the network fetch supersedes it.
      const settings = get(codexClientView).settingsDraft;
      if (settings?.autoCheck) {
        loadPromise = refreshCodexClient(true).finally(() => {
          loadPromise = null;
        });
      }
    } else {
      loadPromise = refreshCodexClient(false).finally(() => {
        loadPromise = null;
      });
    }
  }
}

export async function refreshCodexClient(withNetwork = true, force = false) {
  startCodexClientProgressListener();
  if (get(codexClientView).busyAction && !force) {
    return;
  }
  patch({ loading: true, error: null });
  try {
    let nextState = withNetwork ? await planCodexClientUpdate() : await inspectCodexClient();
    applyState(nextState);
    if (!withNetwork && nextState.settings.autoCheck) {
      nextState = await planCodexClientUpdate();
      applyState(nextState);
    }
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
  } finally {
    patch({ loading: false });
  }
}

async function runAction<T>(
  name: string,
  action: () => Promise<T>,
  onSuccess?: (value: T) => void | Promise<void>
) {
  startCodexClientProgressListener();
  patch({ busyAction: name, error: null, success: null });
  try {
    const result = await action();
    await onSuccess?.(result);
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
  } finally {
    patch({ busyAction: null });
  }
}

export function updateCodexClientDraft(patchValue: Partial<CodexClientSettings>) {
  let nextDraft: CodexClientSettings | null = null;
  codexClientView.update((current) => {
    if (!current.settingsDraft) {
      return current;
    }
    nextDraft = { ...current.settingsDraft, ...patchValue };
    const unchanged = lastSavedSettingsKey === settingsKey(nextDraft);
    return {
      ...current,
      settingsDraft: nextDraft,
      settingsSaveStatus: unchanged ? "saved" : "dirty"
    };
  });
  if (nextDraft) {
    settingsSaveRevision += 1;
    // Only auto-save once the real settings are loaded; before that, the draft
    // is seeded from defaults and saving it could overwrite the user's real
    // backend settings (source/customUrl/etc.) with those defaults.
    if (get(codexClientView).loaded) {
      scheduleSettingsAutoSave();
    }
  }
}

export function setCodexClientConfirmUninstall(confirmUninstall: boolean) {
  patch({ confirmUninstall });
}

function scheduleSettingsAutoSave() {
  if (settingsSaveTimer !== null) {
    window.clearTimeout(settingsSaveTimer);
  }
  settingsSaveTimer = window.setTimeout(() => {
    settingsSaveTimer = null;
    void flushCodexClientSettingsDraft();
  }, SETTINGS_SAVE_DEBOUNCE_MS);
}

async function flushCodexClientSettingsDraft() {
  if (settingsSaveInFlight) {
    scheduleSettingsAutoSave();
    return;
  }

  const snapshot = get(codexClientView);
  const draft = snapshot.settingsDraft;
  if (!draft) {
    return;
  }
  if (snapshot.busyAction) {
    scheduleSettingsAutoSave();
    return;
  }

  const revision = settingsSaveRevision;
  const draftKey = settingsKey(draft);
  if (draftKey === lastSavedSettingsKey) {
    patch({ settingsSaveStatus: "saved" });
    return;
  }

  settingsSaveInFlight = true;
  patch({ settingsSaveStatus: "saving", error: null });
  try {
    const settings = await updateCodexClientSettings(draft);
    lastSavedSettingsKey = settingsKey(settings);
    const nextState = await planCodexClientUpdate();
    if (settingsSaveRevision === revision) {
      applyState(nextState);
      patch({ settingsSaveStatus: "saved" });
    } else {
      scheduleSettingsAutoSave();
    }
  } catch (err) {
    patch({
      settingsSaveStatus: "error",
      error: err instanceof Error ? err.message : String(err)
    });
  } finally {
    settingsSaveInFlight = false;
    const current = get(codexClientView);
    if (
      current.settingsDraft &&
      settingsKey(current.settingsDraft) !== lastSavedSettingsKey &&
      current.settingsSaveStatus !== "error"
    ) {
      scheduleSettingsAutoSave();
    }
  }
}

export async function stageCodexClientPackage() {
  const snapshot = get(codexClientView);
  patch({
    progress: progressSeed(
      "codexClient.progressStagePreparing",
      snapshot.state?.plan?.downloadSize ?? snapshot.state?.release?.contentLength,
      4
    )
  });
  await runAction("stage", stageCodexClientUpdate, (report) => {
    patch({
      stageReport: report,
      success: { key: "codexClient.stageComplete" }
    });
  });
}

export async function installOrUpdateCodexClient() {
  const plan = get(codexClientView).state?.plan ?? null;
  patch({
    progress: progressSeed("codexClient.progressInstallPreparing", plan?.downloadSize, 7)
  });
  await runAction(
    "install",
    () => installCodexClient({
      confirm: true,
      expectedCurrentVersion: plan?.currentVersion ?? null,
      expectedLatestVersion: plan?.latestVersion ?? null,
      expectedRoute: plan?.route ?? null
    }),
    async (result) => {
      applyInstallResult(result);
      patch({
        operationResult: result,
        success: result.installed
          ? { key: "codexClient.ready", values: { version: result.installed.version } }
          : result.message
      });
      window.setTimeout(() => {
        void refreshCodexClient(true, true);
      }, 0);
    }
  );
}

export async function removeCodexClient() {
  const draft = get(codexClientView).settingsDraft;
  await runAction(
    "uninstall",
    () => uninstallCodexClient({
      confirm: true,
      purgeUserData: !(draft?.keepUserDataOnUninstall ?? true)
    }),
    async (result) => {
      patch({
        operationResult: result,
        confirmUninstall: false,
        success: { key: "codexClient.uninstallComplete" }
      });
      await refreshCodexClient(true, true);
    }
  );
}

export async function launchManagedCodexClient() {
  await runAction("launch", launchCodexClient, async () => {
    patch({ success: { key: "codexClient.launchRequested" } });
    await new Promise((resolve) => setTimeout(resolve, 2500));
    await refreshCodexClient(false, true);
  });
}
