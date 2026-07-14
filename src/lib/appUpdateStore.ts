import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { get, writable } from "svelte/store";
import { applicationUpdateTarget, installApplicationUpdate } from "./api";
import { APP_UPDATER_ENABLED, APP_VERSION } from "./appInfo";

type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "installing"
  | "upToDate"
  | "unconfigured"
  | "error";

export interface AppUpdateState {
  status: UpdateStatus;
  updateAvailable: boolean;
  installable: boolean;
  currentVersion: string;
  latestVersion: string | null;
  releaseName: string | null;
  releaseUrl: string | null;
  publishedAt: string | null;
  checkedAt: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: string | null;
}

interface ReleaseInfo {
  version: string;
  name: string | null;
  url: string | null;
  publishedAt: string | null;
  installable: boolean;
}

interface ReleaseLookup {
  release: ReleaseInfo | null;
  emptyStatus: "upToDate";
}

interface InstallerArtifact {
  url: string;
  signature: string;
  filename: string;
}

interface AppUpdateProgress {
  phase: "downloading" | "verifying" | "installing";
  downloadedBytes: number;
  totalBytes: number | null;
}

const initialState: AppUpdateState = {
  status: "idle",
  updateAvailable: false,
  installable: false,
  currentVersion: APP_VERSION,
  latestVersion: null,
  releaseName: null,
  releaseUrl: null,
  publishedAt: null,
  checkedAt: null,
  downloadedBytes: 0,
  totalBytes: null,
  error: null
};

export const appUpdateState = writable<AppUpdateState>(initialState);

let inFlight: Promise<AppUpdateState> | null = null;
let installInFlight: Promise<AppUpdateState> | null = null;
let pendingUpdate: Update | null = null;
let pendingUpdateTarget: string | null = null;

export async function checkForAppUpdate(force = false): Promise<AppUpdateState> {
  const current = get(appUpdateState);
  if (!isTauri() || !APP_UPDATER_ENABLED) {
    pendingUpdate = null;
    pendingUpdateTarget = null;
    const unconfiguredState: AppUpdateState = {
      ...initialState,
      status: "unconfigured",
      checkedAt: new Date().toISOString()
    };
    appUpdateState.set(unconfiguredState);
    return unconfiguredState;
  }
  if (!force && current.status === "checking" && inFlight) {
    return inFlight;
  }

  appUpdateState.set({
    ...current,
    status: "checking",
    downloadedBytes: 0,
    totalBytes: null,
    error: null
  });

  inFlight = fetchTauriRelease()
    .then(({ release, emptyStatus }) => {
      const checkedAt = new Date().toISOString();
      if (!release) {
        const nextState: AppUpdateState = {
          ...initialState,
          status: emptyStatus,
          checkedAt
        };
        appUpdateState.set(nextState);
        return nextState;
      }

      const latestVersion = normalizeVersionLabel(release.version);
      const updateAvailable = compareVersions(latestVersion, APP_VERSION) > 0;
      const nextState: AppUpdateState = {
        status: updateAvailable ? "available" : "upToDate",
        updateAvailable,
        installable: updateAvailable && release.installable,
        currentVersion: APP_VERSION,
        latestVersion,
        releaseName: release.name ?? latestVersion,
        releaseUrl: release.url,
        publishedAt: release.publishedAt,
        checkedAt,
        downloadedBytes: 0,
        totalBytes: null,
        error: null
      };
      appUpdateState.set(nextState);
      return nextState;
    })
    .catch((err) => {
      pendingUpdate = null;
      pendingUpdateTarget = null;
      const nextState: AppUpdateState = {
        ...initialState,
        status: "error",
        checkedAt: new Date().toISOString(),
        error: err instanceof Error ? err.message : String(err)
      };
      appUpdateState.set(nextState);
      return nextState;
    })
    .finally(() => {
      inFlight = null;
    });

  return inFlight;
}

export function installAppUpdate(): Promise<AppUpdateState> {
  if (installInFlight) {
    return installInFlight;
  }
  installInFlight = performAppUpdateInstall().finally(() => {
    installInFlight = null;
  });
  return installInFlight;
}

async function performAppUpdateInstall(): Promise<AppUpdateState> {
  const update = pendingUpdate;
  const updateRawJson = pendingUpdate && pendingUpdate.rawJson;
  const updateTarget = pendingUpdateTarget;
  const current = get(appUpdateState);
  if (!update || !updateRawJson || !updateTarget || !current.installable) {
    const unavailableState: AppUpdateState = {
      ...current,
      status: "error",
      error: "No signed application update is ready to install."
    };
    appUpdateState.set(unavailableState);
    return unavailableState;
  }

  let downloadedBytes = 0;
  appUpdateState.set({
    ...current,
    status: "downloading",
    downloadedBytes: 0,
    totalBytes: null,
    error: null
  });

  try {
    const installerArtifact = installerArtifactForTarget(updateRawJson, updateTarget);
    if (installerArtifact) {
      const unlisten = await listen<AppUpdateProgress>("app-update-progress", (event) => {
        const progress = event.payload;
        appUpdateState.update((state) => ({
          ...state,
          status: progress.phase === "installing" ? "installing" : "downloading",
          downloadedBytes: progress.downloadedBytes,
          totalBytes: progress.totalBytes
        }));
      });
      try {
        await installApplicationUpdate({
          version: update.version,
          ...installerArtifact
        });
      } finally {
        unlisten();
      }
      return get(appUpdateState);
    }

    await update.downloadAndInstall((event: DownloadEvent) => {
      const state = get(appUpdateState);
      if (event.event === "Started") {
        appUpdateState.set({
          ...state,
          downloadedBytes: 0,
          totalBytes: event.data.contentLength ?? null
        });
        return;
      }
      if (event.event === "Progress") {
        downloadedBytes += event.data.chunkLength;
        appUpdateState.set({
          ...state,
          downloadedBytes
        });
        return;
      }
      appUpdateState.set({
        ...state,
        downloadedBytes: state.totalBytes ?? downloadedBytes
      });
    });

    appUpdateState.update((state) => ({ ...state, status: "installing" }));
    return get(appUpdateState);
  } catch (err) {
    const failedState: AppUpdateState = {
      ...get(appUpdateState),
      status: "error",
      error: err instanceof Error ? err.message : String(err)
    };
    appUpdateState.set(failedState);
    return failedState;
  }
}

export function installerArtifactForTarget(
  rawJson: Record<string, unknown>,
  target: string
): InstallerArtifact | null {
  if (!target.startsWith("windows-") && !target.startsWith("darwin-")) {
    return null;
  }
  const platforms = rawJson.platforms;
  if (!platforms || typeof platforms !== "object" || Array.isArray(platforms)) {
    throw new Error("The updater manifest does not contain platform installers.");
  }
  const entry = Reflect.get(platforms, target);
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
    throw new Error("No signed installer is available for this platform.");
  }
  const url = Reflect.get(entry, "url");
  const signature = Reflect.get(entry, "signature");
  if (typeof url !== "string" || typeof signature !== "string") {
    throw new Error("The updater manifest contains an invalid installer entry.");
  }
  const parsedUrl = new URL(url);
  const filename = parsedUrl.pathname.split("/").at(-1);
  if (!filename) {
    throw new Error("The updater installer URL has no filename.");
  }
  return { url, signature, filename };
}

async function fetchTauriRelease(): Promise<ReleaseLookup> {
  const target = await applicationUpdateTarget();
  pendingUpdate = await check({ timeout: 8000, target });
  pendingUpdateTarget = pendingUpdate ? target : null;
  if (!pendingUpdate) {
    return { release: null, emptyStatus: "upToDate" };
  }

  return {
    emptyStatus: "upToDate",
    release: {
      version: pendingUpdate.version,
      name: pendingUpdate.version,
      url: null,
      publishedAt: pendingUpdate.date ?? null,
      installable: true
    }
  };
}

function normalizeVersionLabel(value: string): string {
  const normalized = value
    .trim()
    .replace(/^codestudio-lite[@\s_-]*/i, "")
    .replace(/^code\s*studio\s*lite[@\s_-]*/i, "")
    .replace(/^v/i, "")
    .replace(/\s+/g, "-");
  const versionMatch = normalized.match(/\d+(?:\.\d+){0,2}(?:[-+][0-9A-Za-z.-]+)?/);
  return versionMatch?.[0] ?? "";
}

interface ParsedVersion {
  main: number[];
  prerelease: string[];
}

function parseVersion(version: string): ParsedVersion {
  const normalized = normalizeVersionLabel(version).split("+", 1)[0];
  const prereleaseIndex = normalized.indexOf("-");
  const mainPart = prereleaseIndex >= 0 ? normalized.slice(0, prereleaseIndex) : normalized;
  const prereleasePart = prereleaseIndex >= 0 ? normalized.slice(prereleaseIndex + 1) : "";
  const main = mainPart
    .split(".")
    .slice(0, 3)
    .map((part) => Number.parseInt(part, 10))
    .map((part) => (Number.isFinite(part) ? part : 0));
  while (main.length < 3) {
    main.push(0);
  }

  return {
    main,
    prerelease: prereleasePart ? prereleasePart.split(".") : []
  };
}

function compareVersions(leftVersion: string, rightVersion: string): number {
  const left = parseVersion(leftVersion);
  const right = parseVersion(rightVersion);
  for (let index = 0; index < 3; index += 1) {
    const diff = left.main[index] - right.main[index];
    if (diff !== 0) {
      return diff;
    }
  }

  if (left.prerelease.length === 0 && right.prerelease.length === 0) {
    return 0;
  }
  if (left.prerelease.length === 0) {
    return 1;
  }
  if (right.prerelease.length === 0) {
    return -1;
  }

  const maxLength = Math.max(left.prerelease.length, right.prerelease.length);
  for (let index = 0; index < maxLength; index += 1) {
    const leftPart = left.prerelease[index];
    const rightPart = right.prerelease[index];
    if (leftPart === undefined) {
      return -1;
    }
    if (rightPart === undefined) {
      return 1;
    }
    if (leftPart === rightPart) {
      continue;
    }

    const leftNumber = Number.parseInt(leftPart, 10);
    const rightNumber = Number.parseInt(rightPart, 10);
    const leftNumeric = String(leftNumber) === leftPart;
    const rightNumeric = String(rightNumber) === rightPart;
    if (leftNumeric && rightNumeric) {
      return leftNumber - rightNumber;
    }
    if (leftNumeric) {
      return -1;
    }
    if (rightNumeric) {
      return 1;
    }
    return leftPart.localeCompare(rightPart);
  }

  return 0;
}
