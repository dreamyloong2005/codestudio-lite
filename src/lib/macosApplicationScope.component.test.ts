// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  DetectionSnapshot,
  MacosApplicationCleanupResult,
  MacosApplicationScopeStatus,
  MacosManagedAppId,
  ToolStatus
} from "../types";
import { enUS } from "./locales/en-US";
import { zhCN } from "./locales/zh-CN";
import { zhTW } from "./locales/zh-TW";
import { setLocale } from "./i18n";

const apiMocks = vi.hoisted(() => ({
  cleanupMacosUserApplication: vi.fn(),
  loadMacosApplicationScopeStatus: vi.fn(),
  takeCodestudioSelfCleanupFailure: vi.fn(),
  loadAppSettings: vi.fn(),
  updateAppSettings: vi.fn(),
  openExternalUrl: vi.fn(),
  listenToolInstallProgress: vi.fn(async () => () => {}),
  listenInstallTerminalOutput: vi.fn(async () => () => {})
}));

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  ...apiMocks
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    loadAddon() {}
    open() {}
    write() {}
    dispose() {}
  }
}));

vi.mock("./appUpdateStore", async () => {
  const { writable } = await import("svelte/store");
  return {
    appUpdateState: writable({
      status: "idle",
      updateAvailable: false,
      installable: false,
      currentVersion: "1.5.2",
      latestVersion: null,
      releaseName: null,
      releaseUrl: null,
      publishedAt: null,
      checkedAt: null,
      downloadedBytes: 0,
      totalBytes: null,
      error: null
    }),
    checkForAppUpdate: vi.fn(),
    installAppUpdate: vi.fn()
  };
});

import Dashboard from "../routes/Dashboard.svelte";
import Settings from "../routes/Settings.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function tool(id: "chatgpt-desktop" | "claude-desktop"): ToolStatus {
  return {
    id,
    name: id === "claude-desktop" ? "Claude Desktop" : "ChatGPT Desktop",
    category: "ai_tool",
    command: id === "claude-desktop" ? "Claude" : "ChatGPT",
    pathRepair: null,
    version: "1.0.0",
    latestVersion: "1.0.0",
    updateAvailable: false,
    updateCommand: null,
    installState: "installed",
    configState: "configured",
    configPath: null,
    installPath: `/Applications/${id === "claude-desktop" ? "Claude" : "ChatGPT"}.app`,
    installCommand: null,
    details: null,
    installKind: null,
    duplicateUserInstall: true,
    running: false
  };
}

function snapshot(status: ToolStatus): DetectionSnapshot {
  return {
    generatedAt: new Date(0).toISOString(),
    source: "live",
    platform: "macos",
    homeDir: "/Users/test",
    appConfigDir: "/Users/test/.codestudio-lite",
    activeProfile: null,
    activeProfileName: null,
    codexAuth: {
      available: false,
      method: "none",
      storage: "none",
      path: null,
      detail: ""
    },
    tools: [status],
    system: [],
    problems: [],
    envConflicts: [],
    chatgptDesktopProductGeneration: "current"
  };
}

function scopeStatus(
  appId: MacosManagedAppId,
  duplicateUserInstall = true
): MacosApplicationScopeStatus {
  const appName =
    appId === "codestudio-lite"
      ? "CodeStudio Lite.app"
      : appId === "claude-desktop"
        ? "Claude.app"
        : "ChatGPT.app";
  return {
    appId,
    systemApp: `/Applications/${appName}`,
    userApps: duplicateUserInstall ? [`/Users/test/Applications/${appName}`] : [],
    preferredApp: `/Applications/${appName}`,
    preferredDestination: `/Applications/${appName}`,
    duplicateUserInstall,
    runningApp: `/Applications/${appName}`,
    runningScope: "system"
  };
}

function cleanupResult(appId: MacosManagedAppId): MacosApplicationCleanupResult {
  return {
    status: scopeStatus(appId, false),
    movedToTrash: [`/Users/test/.Trash/${appId}.app`],
    restartScheduled: false
  };
}

beforeEach(() => {
  setLocale("en-US");
  apiMocks.loadAppSettings.mockResolvedValue({ language: "en-US", theme: "system" });
  apiMocks.updateAppSettings.mockImplementation(async (request) => ({
    language: request.language ?? "en-US",
    theme: request.theme ?? "system"
  }));
  apiMocks.openExternalUrl.mockResolvedValue(undefined);
  apiMocks.takeCodestudioSelfCleanupFailure.mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe.each([
  ["chatgpt-desktop", "chatgpt-desktop"],
  ["claude-desktop", "claude-desktop"]
] as const)("Dashboard duplicate cleanup for %s", (toolId, appId) => {
  it("renders the warning and sends the closed managed client ID", async () => {
    apiMocks.cleanupMacosUserApplication.mockResolvedValue(cleanupResult(appId));
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    const rendered = render(Dashboard, {
      snapshot: snapshot(tool(toolId)),
      onRefresh
    });

    expect(screen.getByText(/using the copy in \/Applications/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Delete user copy" }));

    await waitFor(() =>
      expect(apiMocks.cleanupMacosUserApplication).toHaveBeenCalledWith(appId)
    );
    await rendered.rerender({ snapshot: snapshot({ ...tool(toolId), duplicateUserInstall: false }) });
    expect(screen.queryByText(/using the copy in \/Applications/)).toBeNull();
  });
});

it("Dashboard disables cleanup while pending, shows success, refreshes, and surfaces rejection", async () => {
  const cleanupCall = deferred<MacosApplicationCleanupResult>();
  const refreshCall = deferred<void>();
  apiMocks.cleanupMacosUserApplication.mockReturnValueOnce(cleanupCall.promise);
  const onRefresh = vi.fn(() => refreshCall.promise);
  let rendered: ReturnType<typeof render<Dashboard>>;
  const onToolStatusUpdated = vi.fn((next: ToolStatus) => {
    void rendered.rerender({ snapshot: snapshot(next), onRefresh, onToolStatusUpdated });
  });
  rendered = render(Dashboard, {
    snapshot: snapshot(tool("chatgpt-desktop")),
    onRefresh,
    onToolStatusUpdated
  });

  const button = screen.getByRole("button", { name: "Delete user copy" });
  await fireEvent.click(button);
  expect((button as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByText("Deleting...")).toBeTruthy();

  cleanupCall.resolve(cleanupResult("chatgpt-desktop"));
  await waitFor(() => expect(screen.getByText(/Deleted the duplicate user copy/)).toBeTruthy());
  expect(onRefresh).toHaveBeenCalledWith({
    quiet: true,
    scheduleFollowup: false,
    showRefreshIndicator: false
  });
  refreshCall.resolve();
  await waitFor(() => expect(screen.queryByText(/using the copy in \/Applications/)).toBeNull());

  cleanup();
  apiMocks.cleanupMacosUserApplication.mockRejectedValueOnce(new Error("cleanup denied"));
  render(Dashboard, { snapshot: snapshot(tool("chatgpt-desktop")) });
  await fireEvent.click(screen.getByRole("button", { name: "Delete user copy" }));
  await waitFor(() => expect(screen.getByText(/cleanup denied/)).toBeTruthy());
});

it("Settings loads CodeStudio scope, disables pending cleanup, and shows refreshed success", async () => {
  const cleanupCall = deferred<MacosApplicationCleanupResult>();
  apiMocks.loadMacosApplicationScopeStatus
    .mockResolvedValueOnce(scopeStatus("codestudio-lite"))
    .mockResolvedValueOnce(scopeStatus("codestudio-lite", false));
  apiMocks.cleanupMacosUserApplication.mockReturnValueOnce(cleanupCall.promise);
  render(Settings);

  await waitFor(() =>
    expect(apiMocks.loadMacosApplicationScopeStatus).toHaveBeenCalledWith("codestudio-lite")
  );
  expect(await screen.findByText(/using the copy in \/Applications/)).toBeTruthy();
  const button = screen.getByRole("button", { name: "Delete user copy" });
  await fireEvent.click(button);
  expect((button as HTMLButtonElement).disabled).toBe(true);
  cleanupCall.resolve(cleanupResult("codestudio-lite"));

  await waitFor(() => {
    expect(apiMocks.cleanupMacosUserApplication).toHaveBeenCalledWith("codestudio-lite");
    expect(screen.getByText(/Deleted the duplicate user copy/)).toBeTruthy();
  });
  expect(screen.queryByText(/using the copy in \/Applications/)).toBeNull();
});

it("Settings renders cleanup rejection", async () => {
  apiMocks.loadMacosApplicationScopeStatus.mockResolvedValue(scopeStatus("codestudio-lite"));
  apiMocks.cleanupMacosUserApplication.mockRejectedValue(new Error("system copy changed"));
  render(Settings);

  await fireEvent.click(
    await screen.findByRole("button", { name: "Delete user copy" })
  );
  await waitFor(() => expect(screen.getByText(/system copy changed/)).toBeTruthy());
});

it("Settings takes and renders the persisted self-cleanup helper failure until dismissed", async () => {
  apiMocks.loadMacosApplicationScopeStatus.mockResolvedValue(scopeStatus("codestudio-lite", false));
  apiMocks.takeCodestudioSelfCleanupFailure.mockResolvedValue({
    message: "System launch failed; restored the user copy.",
    restoredUserApp: "/Users/test/Applications/CodeStudio Lite.app",
    systemApp: "/Applications/CodeStudio Lite.app"
  });
  render(Settings);

  await waitFor(() =>
    expect(apiMocks.takeCodestudioSelfCleanupFailure).toHaveBeenCalledTimes(1)
  );
  expect(
    await screen.findByText(/Previous duplicate cleanup failed.*System launch failed/)
  ).toBeTruthy();
  await fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByText(/Previous duplicate cleanup failed/)).toBeNull();
});

it("all supported locales explicitly say /Applications is used", () => {
  for (const dictionary of [enUS, zhCN, zhTW]) {
    expect(dictionary["applicationScope.duplicateWarning"]).toContain("/Applications");
    expect(dictionary["applicationScope.deleteUserCopy"]).toBeTruthy();
    expect(dictionary["applicationScope.deletingUserCopy"]).toBeTruthy();
    expect(dictionary["applicationScope.cleanupSuccess"]).toContain("/Applications");
    expect(dictionary["applicationScope.cleanupError"]).toContain("{message}");
    expect(dictionary["applicationScope.selfCleanupFailure"]).toContain("{message}");
  }
});
