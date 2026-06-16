import { get, writable } from "svelte/store";
import { APP_VERSION, GITHUB_RELEASES_API_URL } from "./appInfo";

type UpdateStatus = "idle" | "checking" | "available" | "upToDate" | "noRelease" | "error";

export interface AppUpdateState {
  status: UpdateStatus;
  updateAvailable: boolean;
  currentVersion: string;
  latestVersion: string | null;
  releaseName: string | null;
  releaseUrl: string | null;
  publishedAt: string | null;
  checkedAt: string | null;
  error: string | null;
}

interface GitHubRelease {
  tag_name?: string;
  name?: string | null;
  html_url?: string;
  published_at?: string | null;
  draft?: boolean;
}

const initialState: AppUpdateState = {
  status: "idle",
  updateAvailable: false,
  currentVersion: APP_VERSION,
  latestVersion: null,
  releaseName: null,
  releaseUrl: null,
  publishedAt: null,
  checkedAt: null,
  error: null
};

export const appUpdateState = writable<AppUpdateState>(initialState);

let inFlight: Promise<AppUpdateState> | null = null;

export async function checkForAppUpdate(force = false): Promise<AppUpdateState> {
  const current = get(appUpdateState);
  if (!force && current.status === "checking" && inFlight) {
    return inFlight;
  }

  appUpdateState.set({
    ...current,
    status: "checking",
    error: null
  });

  inFlight = fetchLatestRelease()
    .then((release) => {
      const checkedAt = new Date().toISOString();
      if (!release) {
        const nextState: AppUpdateState = {
          ...initialState,
          status: "noRelease",
          checkedAt
        };
        appUpdateState.set(nextState);
        return nextState;
      }

      const latestVersion = normalizeVersionLabel(release.tag_name ?? release.name ?? "");
      const updateAvailable = compareVersions(latestVersion, APP_VERSION) > 0;
      const nextState: AppUpdateState = {
        status: updateAvailable ? "available" : "upToDate",
        updateAvailable,
        currentVersion: APP_VERSION,
        latestVersion,
        releaseName: release.name ?? release.tag_name ?? latestVersion,
        releaseUrl: release.html_url ?? null,
        publishedAt: release.published_at ?? null,
        checkedAt,
        error: null
      };
      appUpdateState.set(nextState);
      return nextState;
    })
    .catch((err) => {
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

async function fetchLatestRelease(): Promise<GitHubRelease | null> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), 8000);
  try {
    const response = await fetch(GITHUB_RELEASES_API_URL, {
      headers: {
        Accept: "application/vnd.github+json"
      },
      signal: controller.signal
    });
    if (!response.ok) {
      throw new Error(`GitHub Releases returned ${response.status}`);
    }

    const releases = (await response.json()) as GitHubRelease[];
    return releases.find((release) => !release.draft && normalizeVersionLabel(release.tag_name ?? release.name ?? "")) ?? null;
  } finally {
    window.clearTimeout(timeout);
  }
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
