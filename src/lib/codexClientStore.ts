import { get, writable } from "svelte/store";
import {
  inspectCodexClient,
  installCodexClient,
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

interface CodexClientViewState {
  state: CodexClientState | null;
  settingsDraft: CodexClientSettings | null;
  settingsSaveStatus: "idle" | "dirty" | "saving" | "saved" | "error";
  loading: boolean;
  loaded: boolean;
  busyAction: string | null;
  error: string | null;
  success: string | null;
  stageReport: CodexClientStageReport | null;
  operationResult: CodexClientOperationResult | null;
  progress: CodexClientProgress | null;
  confirmUninstall: boolean;
}

const initialState: CodexClientViewState = {
  state: null,
  settingsDraft: null,
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
    keepUserDataOnUninstall: settings.keepUserDataOnUninstall
  });
}

function applyState(state: CodexClientState) {
  lastSavedSettingsKey = settingsKey(state.settings);
  patch({
    state,
    settingsDraft: { ...state.settings },
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

export async function ensureCodexClientLoaded() {
  startCodexClientProgressListener();
  const snapshot = get(codexClientView);
  if (snapshot.loaded || snapshot.loading || snapshot.busyAction) {
    return;
  }
  if (!loadPromise) {
    loadPromise = refreshCodexClient(false).finally(() => {
      loadPromise = null;
    });
  }
  await loadPromise;
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
    scheduleSettingsAutoSave();
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
      "正在准备暂存 Codex 客户端安装包...",
      snapshot.state?.plan?.downloadSize ?? snapshot.state?.release?.contentLength,
      4
    )
  });
  await runAction("stage", stageCodexClientUpdate, (report) => {
    patch({
      stageReport: report,
      success: "安装包已暂存并校验。"
    });
  });
}

export async function installOrUpdateCodexClient() {
  const plan = get(codexClientView).state?.plan ?? null;
  patch({
    progress: progressSeed("正在准备安装 Codex 客户端...", plan?.downloadSize, 7)
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
        success: result.message
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
        success: result.message
      });
      await refreshCodexClient(true, true);
    }
  );
}

export async function launchManagedCodexClient() {
  await runAction("launch", launchCodexClient, () => {
    patch({ success: "已请求启动 Codex 客户端。" });
  });
}
