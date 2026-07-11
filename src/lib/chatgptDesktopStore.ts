import { get, writable } from "svelte/store";
import {
  detectChatGPTDesktopInstallKinds,
  inspectChatGPTDesktop,
  installChatGPTDesktop,
  loadCachedChatGPTDesktopStates,
  loadCachedDetection,
  launchChatGPTDesktop,
  listenChatGPTDesktopProgress,
  planChatGPTDesktopUpdate,
  stageChatGPTDesktopUpdate,
  uninstallChatGPTDesktop,
  updateChatGPTDesktopSettings
} from "./api";
import {
  applyChatGPTDesktopBrandingFromDetection,
  applyChatGPTDesktopBrandingFromInstalled,
  setChatGPTDesktopGeneration
} from "./chatgptDesktopBranding";
import type {
  ChatGPTDesktopInstallKinds,
  ChatGPTDesktopOperationResult,
  ChatGPTDesktopProgress,
  ChatGPTDesktopSettings,
  ChatGPTDesktopStageReport,
  ChatGPTDesktopState,
  ChatGPTDesktopStateCache
} from "../types";
import type { TranslationKey } from "./i18n";
import { REFRESH_CACHE_TTL_MS, readRefreshTimestamp, refreshTimestampFresh, writeRefreshTimestamp } from "./refreshCache";

export type ChatGPTDesktopInstallKind = "msix" | "portable";

export type ChatGPTDesktopNoticeMessage =
  | string
  | { key: TranslationKey; values?: Record<string, string | number> };

interface ChatGPTDesktopKindViewState {
  state: ChatGPTDesktopState | null;
  planRefreshing: boolean;
  planStale: boolean;
  loading: boolean;
  loaded: boolean;
  busyAction: string | null;
  stageReport: ChatGPTDesktopStageReport | null;
  operationResult: ChatGPTDesktopOperationResult | null;
  progress: ChatGPTDesktopProgress | null;
}

interface ChatGPTDesktopViewState {
  kindViews: Record<ChatGPTDesktopInstallKind, ChatGPTDesktopKindViewState>;
  settingsDraft: ChatGPTDesktopSettings | null;
  settingsSaveStatus: "idle" | "dirty" | "saving" | "saved" | "error";
  loaded: boolean;
  error: string | null;
  success: ChatGPTDesktopNoticeMessage | null;
  confirmUninstall: boolean;
  // Per-kind install detection (MSIX vs portable) for the page tabs.
  installKinds: ChatGPTDesktopInstallKinds | null;
  // Which install-kind tab is selected: "msix" (Windows App) or "portable".
  selectedKind: ChatGPTDesktopInstallKind;
}

const INSTALL_KINDS: ChatGPTDesktopInstallKind[] = ["msix", "portable"];

function emptyKindView(): ChatGPTDesktopKindViewState {
  return {
    state: null,
    planRefreshing: false,
    planStale: false,
    loading: false,
    loaded: false,
    busyAction: null,
    stageReport: null,
    operationResult: null,
    progress: null
  };
}

const initialState: ChatGPTDesktopViewState = {
  kindViews: {
    msix: emptyKindView(),
    portable: emptyKindView()
  },
  // Pre-seeded with backend defaults so the launch options section renders and
  // is editable before the first scan completes. applyState replaces this with
  // the real settings once loaded, while preserving any pre-scan edits to the
  // launch-option fields.
  settingsDraft: defaultChatGPTDesktopSettings(),
  settingsSaveStatus: "idle",
  loaded: false,
  error: null,
  success: null,
  confirmUninstall: false,
  installKinds: null,
  selectedKind: "msix"
};

export const chatgptDesktopView = writable<ChatGPTDesktopViewState>(initialState);

let loadPromise: Promise<void> | null = null;
let lastNavigationRefreshAt = Math.max(readRefreshTimestamp("chatgptDesktop"), readRefreshTimestamp("detection"));
const NAVIGATION_REFRESH_TTL_MS = REFRESH_CACHE_TTL_MS;
let progressListenerStarted = false;
let settingsSaveTimer: ReturnType<typeof window.setTimeout> | null = null;
let settingsSaveInFlight = false;
let settingsSaveRevision = 0;
let lastSavedSettingsKey: string | null = null;
let lastSavedSettings: ChatGPTDesktopSettings | null = null;

const SETTINGS_SAVE_DEBOUNCE_MS = 650;

// Defaults mirror the backend's ChatGPTDesktopSettings::default() so a pre-scan
// draft looks like what the scan would return. Used only to render the launch
// options before the first scan completes; applyState preserves any edits the
// user made to these fields during that window.
function defaultChatGPTDesktopSettings(): ChatGPTDesktopSettings {
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
    pluginMarketplaceUnlockOnLaunch: true,
    pluginAutoExpandOnLaunch: true,
    modelWhitelistUnlockOnLaunch: true,
    serviceTierControlsOnLaunch: false,
    officialRemotePluginCacheOnLaunch: true,
    computerUseGuardOnLaunch: false
  };
}

function patch(next: Partial<ChatGPTDesktopViewState>) {
  chatgptDesktopView.update((current) => ({ ...current, ...next }));
}

function patchKind(
  kind: ChatGPTDesktopInstallKind,
  next: Partial<ChatGPTDesktopKindViewState>
) {
  chatgptDesktopView.update((current) => ({
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

function patchAllKinds(
  mapper: (view: ChatGPTDesktopKindViewState) => ChatGPTDesktopKindViewState
) {
  chatgptDesktopView.update((current) => ({
    ...current,
    kindViews: {
      msix: mapper(current.kindViews.msix),
      portable: mapper(current.kindViews.portable)
    }
  }));
}

function selectedKindView(view = get(chatgptDesktopView)) {
  return view.kindViews[view.selectedKind];
}

function hasBusyAction(view = get(chatgptDesktopView)) {
  return INSTALL_KINDS.some((kind) => Boolean(view.kindViews[kind].busyAction));
}

function hasLoadingView(view = get(chatgptDesktopView)) {
  return INSTALL_KINDS.some((kind) => view.kindViews[kind].loading);
}

function normalizeInstallKind(value: string | null | undefined): ChatGPTDesktopInstallKind {
  return value === "portable" ? "portable" : "msix";
}

function stateInstallKind(state: ChatGPTDesktopState): ChatGPTDesktopInstallKind {
  return normalizeInstallKind(state.installKind);
}

function cachedStateEntries(
  cache: ChatGPTDesktopStateCache | null | undefined
): Array<[ChatGPTDesktopInstallKind, ChatGPTDesktopState]> {
  const byKind = new Map<ChatGPTDesktopInstallKind, ChatGPTDesktopState>();
  for (const kind of INSTALL_KINDS) {
    const state = cache?.[kind];
    if (state) {
      byKind.set(stateInstallKind(state), state);
    }
  }
  return INSTALL_KINDS.flatMap((kind) => {
    const state = byKind.get(kind);
    return state ? [[kind, state] as [ChatGPTDesktopInstallKind, ChatGPTDesktopState]] : [];
  });
}

function settingsKey(settings: ChatGPTDesktopSettings) {
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
    pluginMarketplaceUnlockOnLaunch: settings.pluginMarketplaceUnlockOnLaunch,
    pluginAutoExpandOnLaunch: settings.pluginAutoExpandOnLaunch,
    modelWhitelistUnlockOnLaunch: settings.modelWhitelistUnlockOnLaunch,
    serviceTierControlsOnLaunch: settings.serviceTierControlsOnLaunch,
    officialRemotePluginCacheOnLaunch: settings.officialRemotePluginCacheOnLaunch,
    computerUseGuardOnLaunch: settings.computerUseGuardOnLaunch
  });
}

function planAffectingSettingsChanged(
  before: ChatGPTDesktopSettings,
  after: ChatGPTDesktopSettings
) {
  return before.source !== after.source
    || before.customUrl !== after.customUrl
    || before.windowsInstallMode !== after.windowsInstallMode
    || before.installRoot !== after.installRoot;
}

function mergeScannedSettings(
  stateSettings: ChatGPTDesktopSettings,
  current: ChatGPTDesktopViewState
) {
  const draft = current.settingsDraft;
  const preserveLaunchOptions = !current.loaded && Boolean(draft);
  return {
    ...stateSettings,
    syncHistoryOnLaunch: preserveLaunchOptions && draft
      ? draft.syncHistoryOnLaunch
      : stateSettings.syncHistoryOnLaunch,
    pluginMarketplaceUnlockOnLaunch: preserveLaunchOptions && draft
      ? draft.pluginMarketplaceUnlockOnLaunch
      : stateSettings.pluginMarketplaceUnlockOnLaunch,
    pluginAutoExpandOnLaunch: preserveLaunchOptions && draft
      ? draft.pluginAutoExpandOnLaunch
      : stateSettings.pluginAutoExpandOnLaunch,
    modelWhitelistUnlockOnLaunch: preserveLaunchOptions && draft
      ? draft.modelWhitelistUnlockOnLaunch
      : stateSettings.modelWhitelistUnlockOnLaunch,
    serviceTierControlsOnLaunch: preserveLaunchOptions && draft
      ? draft.serviceTierControlsOnLaunch
      : stateSettings.serviceTierControlsOnLaunch,
    officialRemotePluginCacheOnLaunch: preserveLaunchOptions && draft
      ? draft.officialRemotePluginCacheOnLaunch
      : stateSettings.officialRemotePluginCacheOnLaunch,
    computerUseGuardOnLaunch: preserveLaunchOptions && draft
      ? draft.computerUseGuardOnLaunch
      : stateSettings.computerUseGuardOnLaunch
  };
}

type ApplyStateOptions = {
  preserveDraft?: boolean;
};

function applyState(
  state: ChatGPTDesktopState,
  kind: ChatGPTDesktopInstallKind = stateInstallKind(state),
  options: ApplyStateOptions = {}
) {
  applyChatGPTDesktopBrandingFromInstalled(state.installed);
  const current = get(chatgptDesktopView);
  const mergedSettings = mergeScannedSettings(state.settings, current);
  const preserveDraft = Boolean(options.preserveDraft && current.settingsDraft);
  if (!preserveDraft) {
    lastSavedSettingsKey = settingsKey(mergedSettings);
    lastSavedSettings = { ...mergedSettings };
  }
  chatgptDesktopView.update((existing) => ({
    ...existing,
    settingsDraft: preserveDraft && existing.settingsDraft ? existing.settingsDraft : { ...mergedSettings },
    loaded: true,
    settingsSaveStatus: preserveDraft ? existing.settingsSaveStatus : "idle",
    kindViews: {
      ...existing.kindViews,
      [kind]: {
        ...existing.kindViews[kind],
        state,
        loaded: true,
        loading: false,
        planRefreshing: false,
        planStale: preserveDraft ? existing.kindViews[kind].planStale : false
      }
    }
  }));
}

function applyInstallResult(result: ChatGPTDesktopOperationResult) {
  applyChatGPTDesktopBrandingFromInstalled(result.installed);
  const kind = normalizeInstallKind(result.installKind);
  const current = get(chatgptDesktopView);
  const state = current.kindViews[kind].state;
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

  patchKind(kind, {
    state: {
      ...state,
      generatedAt: new Date().toISOString(),
      installed: result.installed,
      installClass: "managed",
      plan: nextPlan
    },
    stageReport: result.stage
  });
}

function progressSeed(
  installKind: ChatGPTDesktopInstallKind,
  message: string,
  total?: number | null,
  stepTotal?: number | null
): ChatGPTDesktopProgress {
  return {
    installKind,
    phase: "preparing",
    message,
    downloaded: null,
    total: total ?? null,
    percent: null,
    step: 1,
    stepTotal: stepTotal ?? null
  };
}

export function startChatGPTDesktopProgressListener() {
  if (progressListenerStarted) {
    return;
  }
  progressListenerStarted = true;
  listenChatGPTDesktopProgress((progress) => {
    patchKind(progress.installKind, { progress });
  }).catch((err) => {
    progressListenerStarted = false;
    patch({ error: err instanceof Error ? err.message : String(err) });
  });
}

// Hydrate the ChatGPT desktop view from the on-disk state cache so the page can
// render a prior session plan instantly, before a fresh network re-fetch
// completes. Mirrors the Claude Desktop page hydrateClaudeDesktopFromCache.
// Marks the view as loaded so a subsequent navigation does not re-block; the
// async re-scan still runs and supersedes this with live data.
async function hydrateChatGPTDesktopFromCache(): Promise<boolean> {
  try {
    const cached = await loadCachedChatGPTDesktopStates();
    const entries = cachedStateEntries(cached);
    if (entries.length > 0) {
      const selectedKind = get(chatgptDesktopView).selectedKind;
      const preferredKind = entries.find(([kind, state]) => kind === selectedKind && Boolean(state.plan))?.[0]
        ?? entries.find(([, state]) => Boolean(state.plan))?.[0]
        ?? entries.find(([kind]) => kind === selectedKind)?.[0]
        ?? entries[0][0];
      const orderedEntries = [...entries].sort(([left], [right]) => {
        if (left === preferredKind) {
          return 1;
        }
        if (right === preferredKind) {
          return -1;
        }
        return 0;
      });
      // Pre-mark loaded so applyState uses the cached settings verbatim
      // instead of preserving the default-seeded draft launch options.
      patch({ loaded: true, selectedKind: preferredKind });
      for (const [kind, state] of orderedEntries) {
        applyState(state, kind);
      }
      // Restore per-kind install detection from the cached detection snapshot
      // so the tabs render instantly before the async re-scan completes.
      try {
        const det = await loadCachedDetection();
        if (det) {
          applyChatGPTDesktopBrandingFromDetection(det);
        }
        if (det?.chatgptDesktopInstallKinds) {
          patch({ installKinds: det.chatgptDesktopInstallKinds });
        }
      } catch {
        // Non-fatal: the async re-scan will populate installKinds.
      }
      return true;
    }
  } catch {
    // Cache read failures are non-fatal: the async re-fetch will populate.
  }
  return false;
}

export async function ensureChatGPTDesktopLoaded() {
  startChatGPTDesktopProgressListener();
  const snapshot = get(chatgptDesktopView);
  if (hasBusyAction(snapshot)) {
    return;
  }
  const hydrated = snapshot.loaded ? true : await hydrateChatGPTDesktopFromCache();
  lastNavigationRefreshAt = Math.max(readRefreshTimestamp("chatgptDesktop"), readRefreshTimestamp("detection"));
  const stale = !refreshTimestampFresh("chatgptDesktop", NAVIGATION_REFRESH_TTL_MS) && !refreshTimestampFresh("detection", NAVIGATION_REFRESH_TTL_MS);
  if (!loadPromise && !hasLoadingView() && !hasBusyAction()) {
    if (hydrated) {
      const settings = get(chatgptDesktopView).settingsDraft;
      if (settings?.autoCheck && stale) {
        loadPromise = refreshChatGPTDesktop(true).finally(() => {
          loadPromise = null;
        });
      }
    } else {
      loadPromise = refreshChatGPTDesktop(false).finally(() => {
        loadPromise = null;
      });
    }
  }
}

export async function refreshChatGPTDesktop(
  withNetwork = true,
  force = false,
  installKind: ChatGPTDesktopInstallKind = get(chatgptDesktopView).selectedKind
) {
  startChatGPTDesktopProgressListener();
  if (get(chatgptDesktopView).kindViews[installKind].busyAction && !force) {
    return;
  }
  const refreshSettingsRevision = settingsSaveRevision;
  patchKind(installKind, { loading: true, planRefreshing: withNetwork });
  patch({ error: null });
  try {
    let nextState = withNetwork
      ? await planChatGPTDesktopUpdate({ installKind })
      : await inspectChatGPTDesktop();
    const preserveDraft = () => settingsSaveRevision !== refreshSettingsRevision;
    applyState(nextState, withNetwork ? installKind : stateInstallKind(nextState), { preserveDraft: preserveDraft() });
    if (!withNetwork && nextState.settings.autoCheck) {
      patchKind(installKind, { planRefreshing: true });
      nextState = await planChatGPTDesktopUpdate({ installKind });
      applyState(nextState, installKind, { preserveDraft: preserveDraft() });
    }
    const installKinds = await detectChatGPTDesktopInstallKinds().catch(() => null);
    lastNavigationRefreshAt = Date.now();
    writeRefreshTimestamp("chatgptDesktop", lastNavigationRefreshAt);
    patch({ installKinds });
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
  } finally {
    patchKind(installKind, { loading: false, planRefreshing: false });
  }
}

async function runAction<T>(
  kind: ChatGPTDesktopInstallKind,
  name: string,
  action: () => Promise<T>,
  onSuccess?: (value: T) => void | Promise<void>
) {
  startChatGPTDesktopProgressListener();
  patchKind(kind, { busyAction: name });
  patch({ error: null, success: null });
  try {
    const result = await action();
    await onSuccess?.(result);
    return result;
  } catch (err) {
    patch({ error: err instanceof Error ? err.message : String(err) });
    return null;
  } finally {
    patchKind(kind, { busyAction: null });
  }
}

export function updateChatGPTDesktopDraft(patchValue: Partial<ChatGPTDesktopSettings>) {
  let nextDraft: ChatGPTDesktopSettings | null = null;
  let planDirty = false;
  chatgptDesktopView.update((current) => {
    if (!current.settingsDraft) {
      return current;
    }
    nextDraft = { ...current.settingsDraft, ...patchValue };
    const unchanged = lastSavedSettingsKey === settingsKey(nextDraft);
    planDirty = current.loaded
      && !unchanged
      && (lastSavedSettings
        ? planAffectingSettingsChanged(lastSavedSettings, nextDraft)
        : INSTALL_KINDS.some((kind) => Boolean(current.kindViews[kind].state?.plan)));
    return {
      ...current,
      settingsDraft: nextDraft,
      kindViews: {
        msix: {
          ...current.kindViews.msix,
          planRefreshing: current.kindViews.msix.loading ? current.kindViews.msix.planRefreshing : planDirty,
          planStale: planDirty,
          stageReport: planDirty ? null : current.kindViews.msix.stageReport,
          operationResult: planDirty ? null : current.kindViews.msix.operationResult
        },
        portable: {
          ...current.kindViews.portable,
          planRefreshing: current.kindViews.portable.loading ? current.kindViews.portable.planRefreshing : planDirty,
          planStale: planDirty,
          stageReport: planDirty ? null : current.kindViews.portable.stageReport,
          operationResult: planDirty ? null : current.kindViews.portable.operationResult
        }
      },
      settingsSaveStatus: unchanged ? "saved" : "dirty"
    };
  });
  if (nextDraft) {
    settingsSaveRevision += 1;
    // Only auto-save once the real settings are loaded; before that, the draft
    // is seeded from defaults and saving it could overwrite the user's real
    // backend settings (source/customUrl/etc.) with those defaults.
    if (get(chatgptDesktopView).loaded) {
      scheduleSettingsAutoSave();
    }
  }
}

export function setChatGPTDesktopConfirmUninstall(confirmUninstall: boolean) {
  patch({ confirmUninstall });
}

/// Select the install-kind tab ("msix" or "portable") on the ChatGPT desktop page.
export function setChatGPTDesktopSelectedKind(kind: ChatGPTDesktopInstallKind) {
  patch({ selectedKind: kind });
  const view = get(chatgptDesktopView);
  const kindView = view.kindViews[kind];
  if ((!kindView.loaded || kindView.planStale) && !kindView.loading && !kindView.busyAction && view.loaded) {
    void refreshChatGPTDesktop(true, false, kind);
  }
}

function scheduleSettingsAutoSave() {
  if (settingsSaveTimer !== null) {
    window.clearTimeout(settingsSaveTimer);
  }
  settingsSaveTimer = window.setTimeout(() => {
    settingsSaveTimer = null;
    void flushChatGPTDesktopSettingsDraft();
  }, SETTINGS_SAVE_DEBOUNCE_MS);
}

async function flushChatGPTDesktopSettingsDraft() {
  if (settingsSaveInFlight) {
    scheduleSettingsAutoSave();
    return;
  }

  const snapshot = get(chatgptDesktopView);
  const draft = snapshot.settingsDraft;
  if (!draft) {
    return;
  }
  if (hasBusyAction(snapshot)) {
    scheduleSettingsAutoSave();
    return;
  }

  const revision = settingsSaveRevision;
  const draftKey = settingsKey(draft);
  const planNeedsRefresh = lastSavedSettings
    ? planAffectingSettingsChanged(lastSavedSettings, draft)
    : INSTALL_KINDS.some((kind) => Boolean(snapshot.kindViews[kind].state?.plan));
  if (draftKey === lastSavedSettingsKey) {
    patchAllKinds((view) => ({
      ...view,
      planRefreshing: false,
      planStale: false
    }));
    patch({ settingsSaveStatus: "saved" });
    return;
  }

  settingsSaveInFlight = true;
  patch({
    settingsSaveStatus: "saving",
    error: null
  });
  patchAllKinds((view) => ({
    ...view,
    planRefreshing: planNeedsRefresh,
    planStale: planNeedsRefresh
  }));
  try {
    const settings = await updateChatGPTDesktopSettings(draft);
    lastSavedSettingsKey = settingsKey(settings);
    lastSavedSettings = { ...settings };
    const installKind = get(chatgptDesktopView).selectedKind;
    const nextState = await planChatGPTDesktopUpdate({ installKind });
    if (settingsSaveRevision === revision) {
      applyState(nextState, installKind);
      patch({ settingsSaveStatus: "saved" });
    } else {
      scheduleSettingsAutoSave();
    }
  } catch (err) {
    patch({
      settingsSaveStatus: "error",
      error: err instanceof Error ? err.message : String(err)
    });
    patchAllKinds((view) => ({
      ...view,
      planRefreshing: false
    }));
  } finally {
    settingsSaveInFlight = false;
    const current = get(chatgptDesktopView);
    if (
      current.settingsDraft &&
      settingsKey(current.settingsDraft) !== lastSavedSettingsKey &&
      current.settingsSaveStatus !== "error"
    ) {
      scheduleSettingsAutoSave();
    }
  }
}

export async function stageChatGPTDesktopPackage() {
  const snapshot = get(chatgptDesktopView);
  const installKind = snapshot.selectedKind;
  const kindView = selectedKindView(snapshot);
  patchKind(installKind, {
    progress: progressSeed(
      installKind,
      "chatgptDesktop.progressStagePreparing",
      kindView.state?.plan?.downloadSize ?? kindView.state?.release?.contentLength,
      4
    )
  });
  await runAction(
    installKind,
    "stage",
    () => stageChatGPTDesktopUpdate({ installKind }),
    (report) => {
      patchKind(report.installKind, { stageReport: report });
      patch({ success: { key: "chatgptDesktop.stageComplete" } });
    }
  );
}

export async function installOrUpdateChatGPTDesktop() {
  const snapshot = get(chatgptDesktopView);
  const installKind = snapshot.selectedKind;
  const kindView = selectedKindView(snapshot);
  const plan = kindView.state?.plan ?? null;
  patchKind(installKind, {
    progress: progressSeed(installKind, "chatgptDesktop.progressInstallPreparing", plan?.downloadSize, 7)
  });
  return runAction(
    installKind,
    "install",
    () => installChatGPTDesktop({
      confirm: true,
      expectedCurrentVersion: plan?.currentVersion ?? null,
      expectedLatestVersion: plan?.latestVersion ?? null,
      expectedRoute: plan?.route ?? null,
      installKind
    }),
    async (result) => {
      applyInstallResult(result);
      patchKind(result.installKind, { operationResult: result });
      patch({
        success: result.installed
          ? { key: "chatgptDesktop.ready", values: { version: result.installed.version } }
          : result.message
      });
      window.setTimeout(() => {
        void refreshChatGPTDesktop(true, true, result.installKind);
      }, 0);
    }
  );
}

export async function removeChatGPTDesktop() {
  const snapshot = get(chatgptDesktopView);
  const draft = snapshot.settingsDraft;
  const installKind = snapshot.selectedKind;
  return runAction(
    installKind,
    "uninstall",
    () => uninstallChatGPTDesktop({
      confirm: true,
      purgeUserData: !(draft?.keepUserDataOnUninstall ?? true),
      installKind
    }),
    async (result) => {
      patchKind(result.installKind, {
        operationResult: result,
        state: result.installed
          ? get(chatgptDesktopView).kindViews[result.installKind].state
          : get(chatgptDesktopView).kindViews[result.installKind].state
            ? {
                ...get(chatgptDesktopView).kindViews[result.installKind].state!,
                installed: null,
                installClass: "none"
              }
            : null
      });
      patch({
        confirmUninstall: false,
        success: { key: "chatgptDesktop.uninstallComplete" }
      });
      const remainingInstalled = Object.values(get(chatgptDesktopView).kindViews)
        .some((view) => Boolean(view.state?.installed));
      if (!remainingInstalled) {
        setChatGPTDesktopGeneration("current");
      }
      await refreshChatGPTDesktop(true, true, result.installKind);
    }
  );
}

export async function launchManagedChatGPTDesktop() {
  const installKind = get(chatgptDesktopView).selectedKind;
  await runAction(installKind, "launch", launchChatGPTDesktop, async () => {
    patch({ success: { key: "chatgptDesktop.launchRequested" } });
  });
}
