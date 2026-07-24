import { runtimeAdapter } from "./api/runtime";
import { tauriProfileAdapter } from "./api/tauri/profiles";
import { createBrowserMockState } from "./api/browserMock/state";
import { createBrowserProfileStore } from "./api/browserMock/profileStore";
import { createBrowserUsage } from "./api/browserMock/usage";
import { createBrowserProfiles } from "./api/browserMock/profiles";
import { createBrowserProfileWritePreview } from "./api/browserMock/profileWritePreview";
import { createBrowserProfileApply } from "./api/browserMock/profileApply";
import { browserProfileAdapter } from "./api/browserMock";
import {
  mockModePreviews as browserMockModePreviews,
  mockNativeConfigPath as browserMockNativeConfigPath,
  mockNativeConfigPreview as browserMockNativeConfigPreview
} from "./api/browserMock/nativePreview";
import {
  browserCredentialDetail as credentialDetail,
  browserCredentialStatus as credentialStatus,
  browserProfileModels as mockProfileModels,
  browserProtocolLabel as mockProtocolLabel,
  ensureBrowserProfileProtocolSupported as ensureMockProfileProtocolSupported,
  ensureCustomOfficialProfileAllowed,
  normalizeBrowserProfileMode as normalizeMockProfileMode,
  normalizeBrowserProtocol as normalizeMockProtocol,
  providerIsOfficial,
  providerRequiresApiKey,
  validateBrowserBaseUrl as validateBaseUrlCheck,
  validateBrowserBaseUrlForProvider as validateBaseUrlCheckForProvider,
  validateBrowserBaseUrlForProviderOrThrow as validateBaseUrlForProviderOrThrow
} from "./api/browserMock/profilePolicy";
import {
  canonicalProfileToolId as canonicalProfileApp,
  profileSupportsConfigProtocol
} from "./profiles/catalog";
import { normalizeChatGPTDesktopDetectionSnapshot } from "./chatgptDesktopBranding";
import type {
  ActivityEvent,
  ActiveProfilesByMode,
  AppSettings,
  ApplyProfileRequest,
  ApplyProfileResult,
  BackupManifest,
  ClaudeDesktopLocalizationProgress,
  ClaudeDesktopLaunchRequest,
  ClaudeDesktopPendingLaunch,
  ClaudeDesktopInstallKinds,
  ClaudeDesktopPlan,
  ClaudeDesktopPageState,
  ClearEnvironmentVariablesRequest,
  ClearEnvironmentVariablesResult,
  ChatGPTDesktopInstallKinds,
  DesktopClientCapability,
  ChatGPTDesktopInstallRequest,
  ChatGPTDesktopOperationResult,
  ChatGPTDesktopProgress,
  ChatGPTDesktopSettings,
  ChatGPTDesktopStageReport,
  ChatGPTDesktopState,
  ChatGPTDesktopStateCache,
  ChatGPTDesktopUninstallRequest,
  PlanChatGPTDesktopUpdateRequest,
  StageChatGPTDesktopUpdateRequest,
  DeleteProfileDraftRequest,
  DetectionSnapshot,
  DoctorReport,
  DuplicateProfileDraftRequest,
  GatewayControlResult,
  GatewayRequestLogEntry,
  GatewayStatus,
  InstallTerminalInputRequest,
  InstallTerminalOutput,
  InstallTerminalResizeRequest,
  ListProfileModelsRequest,
  ListProfileModelsResult,
  NativeConfigDiffLine,
  PreviewProfileApplyRequest,
  PreviewProfileApplyResult,
  PreviewProfileWriteRequest,
  PreviewProfileWriteResult,
  ProfileDraft,
  ProfileModelMapping,
  ProviderApplyMode,
  RepairToolPathRequest,
  RepairToolPathResult,
  ReorderProfileDraftsRequest,
  ProfileSummary,
  RestoreBackupRequest,
  RestoreBackupResult,
  SaveProfileDraftRequest,
  StartCodexOAuthLoginResult,
  ExternalToolLaunchResult,
  StartInstallTerminalRequest,
  StartInstallTerminalResult,
  StopInstallTerminalRequest,
  TestProfileConnectionRequest,
  TestProfileConnectionResult,
  ToolInstallPlan,
  ToolInstallProgress,
  ToolInstallRequest,
  ToolInstallResult,
  ToolUninstallRequest,
  ToolLaunchPlan,
  UpdateChatGPTDesktopSettingsRequest,
  ToolStatus,
  UpdateAppSettingsRequest,
  UpdateGatewaySettingsRequest,
  UpdateProfileDraftRequest,
  UsageQueryResult,
  UsageScriptConfig,
  UsageScriptSaveRequest,
  UsageScriptState,
  UsageScriptTemplateType
} from "../types";

const runtime = runtimeAdapter();
const tauriProfiles = tauriProfileAdapter(runtime);
const browserState = createBrowserMockState();
const browserProfileStore = createBrowserProfileStore(browserState);
const builtinOfficialProfileId = browserProfileStore.builtinId;
const isBuiltinOfficialProfileId = browserProfileStore.isBuiltinId;
const mockAllProfiles = browserProfileStore.all;
const nextMockProfileSortOrder = browserProfileStore.nextSortOrder;
const mockProfileOrderKey = browserProfileStore.orderKey;
const normalizeMockProfileIcon = browserProfileStore.normalizeIcon;
const normalizeMockProfileRemark = browserProfileStore.normalizeRemark;
const normalizeMockProfileReviewModel = browserProfileStore.normalizeReviewModel;
const normalizeMockProfileModelMappings = browserProfileStore.normalizeModelMappings;
const browserUsage = createBrowserUsage(browserState, {
  allProfiles: browserProfileStore.all,
  defaultScript: mockDefaultUsageScript
});
const browserProfiles = createBrowserProfiles(browserState, {
  summary: mockProfiles,
  detection: mockDetection,
  recordActivity(level, message) {
    mockActivity = [{ id: `mock-profile-${Date.now()}`, level, message, createdAt: new Date().toISOString() }, ...mockActivity];
  }
});
const browserProfileWritePreview = createBrowserProfileWritePreview(browserState, {
  detection: mockDetection,
  toolConfigPath: mockToolConfigPath,
  gatewayWillAutoActivate: browserProfiles.gatewayWillAutoActivate
});
const browserProfileApply = createBrowserProfileApply(browserState, {
  detection: mockDetection,
  toolConfigPath: mockToolConfigPath,
  nativeConfigPath: (profile) => browserMockNativeConfigPath(profile.app, profile.mode, profile.provider),
  nativePreview: browserMockNativeConfigPreview,
  modePreviews: browserMockModePreviews,
  envConflicts: () => mockClaudeEnvConflicts,
  summary: mockProfiles,
  restartMessage: mockRestartMessageForProfile,
  recordActivity(message) {
    mockActivity = [{ id: `mock-apply-${Date.now()}`, level: "ok", message, createdAt: new Date().toISOString() }, ...mockActivity];
  }
});
const browserProfileApi = browserProfileAdapter(browserState, {
  summary: async () => mockProfiles(),
  testConnection: browserProfiles.testConnection,
  listModels: browserProfiles.listModels,
  save: browserProfiles.save,
  async startCodexOAuthLogin() {
    await openExternalUrl("https://developers.openai.com/codex/auth");
    return { started: true, command: null, message: "Opened the official Codex authorization page." };
  },
  update: browserProfiles.update,
  duplicate: browserProfiles.duplicate,
  delete: browserProfiles.delete,
  reorder: browserProfiles.reorder,
  loadUsage: browserUsage.load,
  saveUsage: browserUsage.save,
  testUsage: browserUsage.test,
  queryUsage: browserUsage.query,
  deleteUsage: browserUsage.delete,
  previewWrite: browserProfileWritePreview,
  previewApply: browserProfileApply.preview,
  apply: browserProfileApply.apply
});
const profileApi = runtime.kind === "tauri" ? tauriProfiles : browserProfileApi;
const isTauri = () => runtime.kind === "tauri";
const invoke = <T>(command: string, args?: Record<string, unknown>) => runtime.invoke<T>(command, args);
const chatgptDesktopProgressListeners = new Set<(progress: ChatGPTDesktopProgress) => void>();
const toolInstallProgressListeners = new Set<(progress: ToolInstallProgress) => void>();
const claudeDesktopLocalizationProgressListeners = new Set<(progress: ClaudeDesktopLocalizationProgress) => void>();
const installTerminalOutputListeners = new Set<(output: InstallTerminalOutput) => void>();
const PROTOCOL_OPENAI_CHAT_COMPLETIONS = "openai-chat-completions";
const PROTOCOL_OPENAI_RESPONSES = "openai-responses";
const PROTOCOL_ANTHROPIC_MESSAGES = "anthropic-messages";
const PROTOCOL_GOOGLE_GEMINI = "google-gemini";
const CLAUDE_DESKTOP_PROFILE_ID = "00000000-0000-4000-8000-000000157210";
const CLAUDE_DESKTOP_DEFAULT_ROUTE_ID = "claude-sonnet-4-6";
const CLAUDE_DESKTOP_DEFAULT_ROUTES = [
  "claude-sonnet-4-6",
  "claude-opus-4-8",
  "claude-haiku-4-5",
  "claude-fable-5"
];
const MOCK_DETECTION_CACHE_KEY = "codestudio-lite:detection-cache";
const MOCK_LANGUAGE_KEY = "codestudio-lite-language";
const mockCodexAuthStatus = {
  available: true,
  method: "chat_gpt" as const,
  storage: "auth_json" as const,
  path: "~/.codex/auth.json",
  detail: "Codex ChatGPT/OAuth login cache detected in auth.json."
};

export async function ensureAppDirs(): Promise<ProfileSummary> {
  return profileApi.ensureAppDirs();
}

export async function loadProfileSummary(): Promise<ProfileSummary> {
  return profileApi.loadSummary();
}

type DetectEnvironmentOptions = {
  waitForUpdates?: boolean;
};

export async function detectEnvironment(options: DetectEnvironmentOptions = {}): Promise<DetectionSnapshot> {
  if (isTauri()) {
    const snapshot = await invoke<DetectionSnapshot>("detect_environment", { request: options });
    return normalizeChatGPTDesktopDetectionSnapshot(snapshot);
  }
  const snapshot = mockDetection();
  writeMockDetectionCache(snapshot);
  return normalizeChatGPTDesktopDetectionSnapshot(snapshot);
}

export async function loadCachedDetection(): Promise<DetectionSnapshot | null> {
  const snapshot = isTauri()
    ? await invoke<DetectionSnapshot | null>("load_cached_detection")
    : readMockDetectionCache();
  return snapshot ? normalizeChatGPTDesktopDetectionSnapshot(snapshot) : null;
}

export async function detectEnvironmentFresh(): Promise<DetectionSnapshot> {
  if (isTauri()) {
    const snapshot = await invoke<DetectionSnapshot>("detect_environment_fresh");
    return normalizeChatGPTDesktopDetectionSnapshot(snapshot);
  }
  const snapshot = mockDetection();
  writeMockDetectionCache(snapshot);
  return normalizeChatGPTDesktopDetectionSnapshot(snapshot);
}
/// Per-kind install detection for the Claude Desktop page tabs (MSIX vs
/// native .exe). Resolves both kinds independently so a user with both
/// installed can manage each via its own tab.
export async function detectClaudeInstallKinds(): Promise<ClaudeDesktopInstallKinds> {
  if (isTauri()) {
    return invoke("detect_claude_install_kinds");
  }
  return {
    msix: { installed: false, version: null, path: null },
    exe: { installed: false, version: null, path: null }
  };
}
/// Local MSIX-runtime capability check for the Claude Desktop page, mirroring
/// the ChatGPT desktop capability panel.
export async function detectClaudeCapabilities(): Promise<DesktopClientCapability[]> {
  if (isTauri()) {
    return invoke("detect_claude_capabilities");
  }
  return [];
}

/// Per-kind install detection for the ChatGPT desktop client page tabs (MSIX
/// vs portable). Resolves both kinds independently.
export async function detectChatGPTDesktopInstallKinds(): Promise<ChatGPTDesktopInstallKinds> {
  if (isTauri()) {
    return invoke("detect_chatgpt_desktop_install_kinds");
  }
  return {
    msix: { installed: false, version: null, path: null },
    portable: { installed: false, version: null, path: null }
  };
}

export async function planToolInstall(toolId: string): Promise<ToolInstallPlan> {
  if (isTauri()) {
    return invoke("plan_tool_install", { toolId });
  }
  return mockToolInstallPlan(toolId);
}

export async function planToolUpdate(toolId: string): Promise<ToolInstallPlan> {
  if (isTauri()) {
    return invoke("plan_tool_update", { toolId });
  }
  return mockToolUpdatePlan(toolId);
}

export async function planToolLaunch(toolId: string): Promise<ToolLaunchPlan> {
  if (isTauri()) {
    return invoke("plan_tool_launch", { toolId });
  }
  return mockToolLaunchPlan(toolId);
}

export async function planClaudeDesktopUpdate(): Promise<ClaudeDesktopPlan> {
  if (isTauri()) {
    return invoke("plan_claude_desktop_update");
  }
  return mockClaudeDesktopPlan();
}

export async function inspectClaudeDesktopPage(force = false): Promise<ClaudeDesktopPageState> {
  if (isTauri()) {
    return invoke("inspect_claude_desktop_page", { force });
  }
  const snapshot = mockDetection();
  return {
    snapshot,
    installPlan: mockToolInstallPlan("claude-desktop"),
    updatePlan: mockToolUpdatePlan("claude-desktop"),
    plan: mockClaudeDesktopPlan(),
    capabilities: []
  };
}

export async function installTool(request: ToolInstallRequest): Promise<ToolInstallResult> {
  if (isTauri()) {
    return invoke("install_tool", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  const plan = mockToolInstallPlan(request.toolId);
  if (!plan.canInstall) {
    return {
      success: plan.alreadyInstalled,
      toolId: plan.toolId,
      toolName: plan.toolName,
      action: plan.alreadyInstalled ? "already-installed" : "blocked",
      message: plan.blocker ?? "Install plan is blocked.",
      command: plan.command,
      exitCode: null,
      stdoutTail: "",
      stderrTail: "",
      currentStatus: mockFindToolStatus(plan.toolId),
      stageResults: [],
      notes: []
    };
  }
  if (plan.requiresPrerequisites && !request.installPrerequisites) {
    return {
      success: false,
      toolId: plan.toolId,
      toolName: plan.toolName,
      action: "prerequisites-required",
      message: "This tool requires prerequisites. Allow prerequisite installation before continuing.",
      command: plan.command,
      exitCode: null,
      stdoutTail: "",
      stderrTail: "",
      currentStatus: mockFindToolStatus(plan.toolId),
      stageResults: [],
      notes: []
    };
  }

  await new Promise((resolve) => window.setTimeout(resolve, 240));
  const stageResults: ToolInstallResult["stageResults"] = [];
  for (const prerequisite of plan.prerequisites) {
    if (prerequisite.installed) {
      continue;
    }
    await emitMockToolInstallProgress({
      rootToolId: request.toolId,
      toolId: prerequisite.toolId,
      toolName: prerequisite.toolName,
      stage: "prerequisite",
      command: prerequisite.command,
      installKind: request.installKind
    });
    markMockToolUpdated(prerequisite.toolId);
    if (prerequisite.toolId === "node") {
      markMockToolUpdated("npm");
    }
    stageResults.push({
      toolId: prerequisite.toolId,
      toolName: prerequisite.toolName,
      stage: "prerequisite",
      command: prerequisite.command,
      success: true,
      exitCode: 0,
      stdoutTail: `browser-dev mock: ${prerequisite.command}`,
      stderrTail: "",
      message: `${prerequisite.toolName} prerequisite installed.`
    });
  }
  await emitMockToolInstallProgress({
    rootToolId: request.toolId,
    toolId: plan.toolId,
    toolName: plan.toolName,
    stage: "target",
    command: plan.commands.find((command) => command.stage === "target")?.command ?? plan.command,
    installKind: request.installKind
  });
  markMockToolUpdated(request.toolId);
  if (request.toolId === "node") {
    markMockToolUpdated("npm");
  }
  const currentStatus = mockFindToolStatus(request.toolId);
  writeMockDetectionCache(mockDetection());
  const success = currentStatus?.installState === "installed";
  stageResults.push({
    toolId: plan.toolId,
    toolName: plan.toolName,
    stage: "target",
    command: plan.commands.find((command) => command.stage === "target")?.command ?? plan.command,
    success,
    exitCode: 0,
    stdoutTail: `browser-dev mock: ${plan.commands.find((command) => command.stage === "target")?.command ?? plan.command}`,
    stderrTail: "",
    message: success
      ? `${plan.toolName} installed and verified.`
      : `${plan.toolName} install command finished, but verification still did not confirm it is available.`
  });
  mockActivity = [
    {
      id: `mock-tool-install-${request.toolId}-${Date.now()}`,
      level: success ? "ok" : "warning",
      message: success
        ? `Installed ${plan.toolName} through ${plan.manager}.`
        : `Install command completed but ${plan.toolName} was not verified.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return {
    success,
    toolId: plan.toolId,
    toolName: plan.toolName,
    action: plan.manager,
    message: success
      ? `${plan.toolName} installed and verified.`
      : `${plan.toolName} install command finished, but verification still did not confirm it is available.`,
    command: plan.command,
    exitCode: 0,
    stdoutTail: stageResults.map((stage) => stage.stdoutTail).filter(Boolean).join("\n"),
    stderrTail: "",
    currentStatus,
    stageResults,
    notes: []
  };
}

export async function updateTool(request: ToolInstallRequest): Promise<ToolInstallResult> {
  if (isTauri()) {
    return invoke("update_tool", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  const status = mockFindToolStatus(request.toolId);
  if (!status || status.installState !== "installed") {
    return {
      success: false,
      toolId: request.toolId,
      toolName: status?.name ?? request.toolId,
      action: "blocked",
      message: `${status?.name ?? request.toolId} is not installed and cannot be updated.`,
      command: status?.updateCommand ?? "",
      exitCode: null,
      stdoutTail: "",
      stderrTail: "",
      currentStatus: status,
      stageResults: [],
      notes: []
    };
  }
  if (!status.updateAvailable || !status.updateCommand) {
    return {
      success: false,
      toolId: status.id,
      toolName: status.name,
      action: "blocked",
      message: `${status.name} has no detected update right now.`,
      command: status.updateCommand ?? "",
      exitCode: null,
      stdoutTail: "",
      stderrTail: "",
      currentStatus: status,
      stageResults: [],
      notes: []
    };
  }

  await emitMockToolInstallProgress({
    rootToolId: request.toolId,
    toolId: status.id,
    toolName: status.name,
    stage: "update",
    command: status.updateCommand,
    installKind: request.installKind
  });
  markMockToolUpdated(request.toolId);
  const currentStatus = mockFindToolStatus(request.toolId);
  writeMockDetectionCache(mockDetection());
  const message = `${status.name} update command completed and verified.`;
  mockActivity = [
    {
      id: `mock-tool-update-${request.toolId}-${Date.now()}`,
      level: "ok",
      message: `Updated ${status.name}.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return {
    success: true,
    toolId: status.id,
    toolName: status.name,
    action: "update",
    message,
    command: status.updateCommand,
    exitCode: 0,
    stdoutTail: `browser-dev mock: ${status.updateCommand}`,
    stderrTail: "",
    currentStatus,
    stageResults: [
      {
        toolId: status.id,
        toolName: status.name,
        stage: "update",
        command: status.updateCommand,
        success: true,
        exitCode: 0,
        stdoutTail: `browser-dev mock: ${status.updateCommand}`,
        stderrTail: "",
        message
      }
    ],
    notes: []
  };
}

export async function uninstallTool(request: ToolUninstallRequest): Promise<ToolInstallResult> {
  if (isTauri()) {
    return invoke("uninstall_tool", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  const status = mockFindToolStatus(request.toolId);
  if (!status || status.installState !== "installed") {
    return {
      success: false,
      toolId: request.toolId,
      toolName: status?.name ?? request.toolId,
      action: "blocked",
      message: `${status?.name ?? request.toolId} is not installed and cannot be uninstalled.`,
      command: "",
      exitCode: null,
      stdoutTail: "",
      stderrTail: "",
      currentStatus: status,
      stageResults: [],
      notes: []
    };
  }

  const command = mockToolUninstallCommand(status.id);
  await emitMockToolInstallProgress({
    rootToolId: request.toolId,
    toolId: status.id,
    toolName: status.name,
    stage: "uninstall",
    command,
    installKind: request.installKind
  });
  mockInstalledToolIds.delete(request.toolId);
  const currentStatus = mockFindToolStatus(request.toolId);
  writeMockDetectionCache(mockDetection());
  const message = `${status.name} uninstalled.`;
  mockActivity = [
    {
      id: `mock-tool-uninstall-${request.toolId}-${Date.now()}`,
      level: "ok",
      message,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return {
    success: currentStatus?.installState !== "installed",
    toolId: status.id,
    toolName: status.name,
    action: "uninstall",
    message,
    command,
    exitCode: 0,
    stdoutTail: `browser-dev mock: ${command}`,
    stderrTail: "",
    currentStatus,
    stageResults: [
      {
        toolId: status.id,
        toolName: status.name,
        stage: "uninstall",
        command,
        success: currentStatus?.installState !== "installed",
        exitCode: 0,
        stdoutTail: `browser-dev mock: ${command}`,
        stderrTail: "",
        message
      }
    ],
    notes: []
  };
}

export async function listenToolInstallProgress(
  handler: (progress: ToolInstallProgress) => void
): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<ToolInstallProgress>("tool-install://progress", (event) => handler(event.payload));
  }
  toolInstallProgressListeners.add(handler);
  return () => toolInstallProgressListeners.delete(handler);
}

export async function startInstallTerminal(
  request: StartInstallTerminalRequest
): Promise<StartInstallTerminalResult> {
  if (isTauri()) {
    return invoke("start_install_terminal", { request });
  }
  const sessionId = `mock-install-terminal-${Date.now()}`;
  const commandText = [
    request.keepOpen ? "[keep-open]" : "[run-once]",
    request.command,
    request.workingDirectory ? `[cwd:${request.workingDirectory}]` : null,
    request.profileId ? `[profile:${request.profileId}]` : null
  ]
    .filter(Boolean)
    .join(" ");
  void simulateInstallTerminalOutput(
    sessionId,
    commandText
  );
  return {
    sessionId,
    toolId: request.toolId,
    command: request.command,
    started: true
  };
}

export async function launchToolExternal(
  request: StartInstallTerminalRequest
): Promise<ExternalToolLaunchResult> {
  if (isTauri()) {
    return invoke("launch_tool_external", { request });
  }
  return {
    started: true,
    toolId: request.toolId,
    command: request.command
  };
}

export async function writeInstallTerminal(request: InstallTerminalInputRequest): Promise<void> {
  if (isTauri()) {
    return invoke("write_install_terminal", { request });
  }
  installTerminalOutputListeners.forEach((listener) =>
    listener({
      sessionId: request.sessionId,
      stream: "output",
      data: request.data,
      done: false,
      exitCode: null
    })
  );
}

export async function resizeInstallTerminal(request: InstallTerminalResizeRequest): Promise<void> {
  if (isTauri()) {
    return invoke("resize_install_terminal", { request });
  }
}

export async function stopInstallTerminal(request: StopInstallTerminalRequest): Promise<void> {
  if (isTauri()) {
    return invoke("stop_install_terminal", { request });
  }
  installTerminalOutputListeners.forEach((listener) =>
    listener({
      sessionId: request.sessionId,
      stream: "status",
      data: "",
      done: true,
      exitCode: null
    })
  );
}

export async function listenInstallTerminalOutput(
  handler: (output: InstallTerminalOutput) => void
): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<InstallTerminalOutput>("install-terminal://output", (event) => handler(event.payload));
  }
  installTerminalOutputListeners.add(handler);
  return () => installTerminalOutputListeners.delete(handler);
}

export async function repairToolPath(request: RepairToolPathRequest): Promise<RepairToolPathResult> {
  if (isTauri()) {
    return invoke("repair_tool_path", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  const status = mockFindToolStatus(request.toolId);
  if (!status?.pathRepair) {
    return {
      success: false,
      toolId: request.toolId,
      toolName: status?.name ?? request.toolId,
      addedPath: null,
      message: "No repairable PATH candidate is available.",
      currentStatus: status,
      notes: []
    };
  }
  mockInstalledToolIds.add(request.toolId);
  const currentStatus = mockFindToolStatus(request.toolId);
  writeMockDetectionCache(mockDetection());
  return {
    success: true,
    toolId: request.toolId,
    toolName: currentStatus?.name ?? status.name,
    addedPath: status.pathRepair.directory,
    message: `Added ${status.pathRepair.directory} to the user PATH.`,
    currentStatus,
    notes: []
  };
}

export async function clearClaudeEnvironmentVariables(
  request: ClearEnvironmentVariablesRequest
): Promise<ClearEnvironmentVariablesResult> {
  if (isTauri()) {
    return invoke("clear_claude_environment_variables", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  const cleared = request.variables.length > 0 ? request.variables : mockClaudeEnvConflicts.map((item) => item.variable);
  mockClaudeEnvConflicts = mockClaudeEnvConflicts.filter((item) => !cleared.includes(item.variable));
  writeMockDetectionCache(mockDetection());
  return {
    success: mockClaudeEnvConflicts.length === 0,
    toolId: "claude",
    cleared,
    skipped: [],
    message: "Claude global environment variables were cleared.",
    conflicts: mockClaudeEnvConflicts
  };
}

export async function runDoctor(): Promise<DoctorReport> {
  if (isTauri()) {
    return invoke("run_doctor");
  }
  const snapshot = mockDetection();
  return {
    generatedAt: snapshot.generatedAt,
    problems: snapshot.problems,
    checks: [
      {
        id: "config-dir",
        group: "Config Files",
        label: "CodeStudio Lite directory",
        status: "ok",
        detail: snapshot.appConfigDir
      },
      {
        id: "keychain",
        group: "Security",
        label: "System keychain",
        status: "ok" as const,
        detail: "Provider API keys are stored through the desktop system keychain in Tauri mode."
      },
      ...snapshot.tools.map((tool) => ({
        id: `tool-${tool.id}`,
        group: "AI Coding Tools",
        label: tool.name,
        status: tool.installState === "installed" ? ("ok" as const) : ("warning" as const),
        detail: tool.version ?? tool.details ?? "Missing"
      }))
    ]
  };
}

export async function loadAppSettings(): Promise<AppSettings> {
  if (isTauri()) {
    return invoke("load_app_settings");
  }
  return mockSettings;
}

export async function updateAppSettings(request: UpdateAppSettingsRequest): Promise<AppSettings> {
  if (isTauri()) {
    return invoke("update_app_settings", { request });
  }

  mockSettings = {
    ...mockSettings,
    theme: request.theme ?? mockSettings.theme,
    language: request.language ?? mockSettings.language
  };
  mockActivity = [
    {
      id: `mock-settings-${Date.now()}`,
      level: "info",
      message: `Updated application settings: language=${mockSettings.language}, theme=${mockSettings.theme}.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];
  return mockSettings;
}

export interface InstallApplicationUpdateRequest {
  version: string;
  url: string;
  signature: string;
  filename: string;
}

export async function applicationUpdateTarget(): Promise<string> {
  if (isTauri()) {
    return invoke("application_update_target");
  }
  return "browser-unknown";
}

export async function installApplicationUpdate(
  request: InstallApplicationUpdateRequest
): Promise<void> {
  if (isTauri()) {
    return invoke("install_application_update", { request });
  }
}

export async function loadGatewayStatus(): Promise<GatewayStatus> {
  if (isTauri()) {
    return invoke("load_gateway_status");
  }
  return mockGatewayStatus();
}

export async function startGateway(): Promise<GatewayControlResult> {
  if (isTauri()) {
    return invoke("start_gateway");
  }
  mockGatewayRunning = true;
  mockGatewayStartedAt = new Date().toISOString();
  return { status: mockGatewayStatus() };
}

export async function stopGateway(): Promise<GatewayControlResult> {
  if (isTauri()) {
    return invoke("stop_gateway");
  }
  mockGatewayRunning = false;
  mockGatewayStartedAt = null;
  return { status: mockGatewayStatus() };
}

export async function restartGateway(): Promise<GatewayControlResult> {
  if (isTauri()) {
    return invoke("restart_gateway");
  }
  mockGatewayRunning = true;
  mockGatewayStartedAt = new Date().toISOString();
  return { status: mockGatewayStatus() };
}

export async function updateGatewaySettings(
  request: UpdateGatewaySettingsRequest
): Promise<GatewayControlResult> {
  if (isTauri()) {
    return invoke("update_gateway_settings", { request });
  }
  mockGatewayPrivacyFilterMode = request.privacyFilterMode ?? mockGatewayPrivacyFilterMode;
  return { status: mockGatewayStatus() };
}

export async function loadActivityLog(): Promise<ActivityEvent[]> {
  if (isTauri()) {
    return invoke("load_activity_log");
  }
  return mockActivity;
}

export async function loadGatewayRequestLog(): Promise<GatewayRequestLogEntry[]> {
  if (isTauri()) {
    return invoke("load_gateway_request_log");
  }
  return mockGatewayRequests;
}

export async function openExternalUrl(url: string): Promise<void> {
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function loadBackups(): Promise<BackupManifest[]> {
  if (isTauri()) {
    return invoke("list_backups");
  }
  return browserState.backups;
}

export async function restoreBackup(request: RestoreBackupRequest): Promise<RestoreBackupResult> {
  if (isTauri()) {
    return invoke("restore_backup", { request });
  }

  const backup = browserState.backups.find((item) => item.id === request.backupId);
  if (!backup) {
    throw new Error(`Backup '${request.backupId}' does not exist`);
  }

  const safetyBackup: BackupManifest = {
    id: new Date().toISOString().replaceAll(":", "-"),
    reason: "restore-current",
    profile: mockDefaultActiveProfileId(),
    changedFiles: ["~/.codestudio-lite/app_state.sqlite"],
    createdAt: new Date().toISOString()
  };
  browserState.backupSnapshots[safetyBackup.id] = cloneMockActiveProfilesByMode();
  browserState.backups = [safetyBackup, ...browserState.backups];
  browserState.activeProfilesByMode = cloneActiveProfilesByMode(browserState.backupSnapshots[backup.id] ?? emptyActiveProfilesByMode());
  mockActivity = [
    {
      id: `mock-restore-${Date.now()}`,
      level: "ok",
      message: `Restored backup '${backup.id}'.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return {
    restored: backup,
    safetyBackup
  };
}

export async function inspectChatGPTDesktop(): Promise<ChatGPTDesktopState> {
  if (isTauri()) {
    return invoke("inspect_chatgpt_desktop");
  }
  return mockChatGPTDesktopState(false);
}

export async function loadCachedChatGPTDesktopState(): Promise<ChatGPTDesktopState | null> {
  if (isTauri()) {
    return invoke("load_cached_chatgpt_desktop_state");
  }
  return null;
}

export async function loadCachedChatGPTDesktopStates(): Promise<ChatGPTDesktopStateCache> {
  if (isTauri()) {
    return invoke("load_cached_chatgpt_desktop_states");
  }
  return {};
}

export async function planChatGPTDesktopUpdate(
  request: PlanChatGPTDesktopUpdateRequest = {}
): Promise<ChatGPTDesktopState> {
  if (isTauri()) {
    return invoke("plan_chatgpt_desktop_update", { request });
  }
  return mockChatGPTDesktopState(true, undefined, request.installKind);
}

export async function stageChatGPTDesktopUpdate(
  request: StageChatGPTDesktopUpdateRequest
): Promise<ChatGPTDesktopStageReport> {
  if (isTauri()) {
    return invoke("stage_chatgpt_desktop_update", { request });
  }
  const installKind = mockChatGPTDesktopInstallKind(request.installKind);
  await simulateChatGPTDesktopProgress([
    { installKind, phase: "preparing", message: "chatgptDesktop.progressStageReading", downloaded: null, total: null, percent: null, step: 1, stepTotal: 4 },
    { installKind, phase: "downloading", message: "chatgptDesktop.progressDownloading", downloaded: 46000000, total: 552187367, percent: 8.3, step: 2, stepTotal: 4 },
    { installKind, phase: "downloading", message: "chatgptDesktop.progressDownloading", downloaded: 178000000, total: 552187367, percent: 32.2, step: 2, stepTotal: 4 },
    { installKind, phase: "downloading", message: "chatgptDesktop.progressDownloading", downloaded: 394000000, total: 552187367, percent: 71.3, step: 2, stepTotal: 4 },
    { installKind, phase: "verifying", message: "chatgptDesktop.progressVerifying", downloaded: null, total: null, percent: null, step: 3, stepTotal: 4 },
    { installKind, phase: "done", message: "chatgptDesktop.progressStageDone", downloaded: 552187367, total: 552187367, percent: 100, step: 4, stepTotal: 4 }
  ]);
  return mockChatGPTDesktopStageReport(installKind);
}

export async function installChatGPTDesktop(
  request: ChatGPTDesktopInstallRequest
): Promise<ChatGPTDesktopOperationResult> {
  if (isTauri()) {
    return invoke("install_chatgpt_desktop", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  const installKind = mockChatGPTDesktopInstallKind(request.installKind);
  await simulateChatGPTDesktopProgress([
    { installKind, phase: "preparing", message: "chatgptDesktop.progressInstallConfirming", downloaded: null, total: null, percent: null, step: 1, stepTotal: 7 },
    { installKind, phase: "downloading", message: "chatgptDesktop.progressDownloading", downloaded: 220000000, total: 552187367, percent: 39.8, step: 2, stepTotal: 7 },
    { installKind, phase: "downloading", message: "chatgptDesktop.progressDownloading", downloaded: 552187367, total: 552187367, percent: 100, step: 2, stepTotal: 7 },
    { installKind, phase: "verifying", message: "chatgptDesktop.progressVerifying", downloaded: null, total: null, percent: null, step: 3, stepTotal: 7 },
    { installKind, phase: "extracting", message: "chatgptDesktop.progressExtractingMsix", downloaded: 38, total: 120, percent: 31.7, step: 4, stepTotal: 7 },
    { installKind, phase: "copying", message: "chatgptDesktop.progressCopyingPortable", downloaded: null, total: null, percent: null, step: 5, stepTotal: 7 },
    { installKind, phase: "writing", message: "chatgptDesktop.progressWritingInstall", downloaded: null, total: null, percent: null, step: 6, stepTotal: 7 },
    { installKind, phase: "finalizing", message: "chatgptDesktop.progressFinalizingInstall", downloaded: null, total: null, percent: null, step: 6, stepTotal: 7 },
    { installKind, phase: "done", message: "chatgptDesktop.progressInstallDone", downloaded: 1, total: 1, percent: 100, step: 7, stepTotal: 7 }
  ]);
  mockChatGPTDesktopInstalled = {
    path: installKind === "portable"
      ? mockChatGPTDesktopSettings.installRoot
      : "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.609.4994.0_x64__2p2nqsd0c76g0",
    version: "26.609.4994.0",
    arch: "x64",
    source: installKind === "portable" ? "portable" : "msix",
    generation: "current",
    packageFamilyName: installKind === "portable"
      ? null
      : "OpenAI.Codex_2p2nqsd0c76g0",
    installedAt: new Date().toISOString()
  };
  return {
    installKind,
    success: true,
    action: mockChatGPTDesktopInstalled.source === "portable" ? "portable-fallback" : "msix-sideload",
    message: `ChatGPT Desktop is ready: ${mockChatGPTDesktopInstalled.version}`,
    installed: mockChatGPTDesktopInstalled,
    stage: mockChatGPTDesktopStageReport(installKind),
    notes: ["browser-dev mock: install path is simulated."]
  };
}

export async function uninstallChatGPTDesktop(
  request: ChatGPTDesktopUninstallRequest
): Promise<ChatGPTDesktopOperationResult> {
  if (isTauri()) {
    return invoke("uninstall_chatgpt_desktop", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  const installKind = mockChatGPTDesktopInstallKind(request.installKind);
  mockChatGPTDesktopInstalled = null;
  return {
    installKind,
    success: true,
    action: "remove-portable",
    message: "ChatGPT Desktop uninstalled.",
    installed: null,
    stage: null,
    notes: [request.purgeUserData ? "Deleted ~/.codex user data." : "Kept ~/.codex user data."]
  };
}

export async function launchChatGPTDesktop(): Promise<void> {
  if (isTauri()) {
    return invoke("launch_chatgpt_desktop");
  }
}

export async function launchClaudeDesktop(request: ClaudeDesktopLaunchRequest = {}): Promise<void> {
  if (isTauri()) {
    return invoke("launch_claude_desktop", { localize: request.localize });
  }
}

export async function listenClaudeDesktopLocalizationProgress(
  handler: (progress: ClaudeDesktopLocalizationProgress) => void
): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<ClaudeDesktopLocalizationProgress>("claude-desktop://localization-progress", (event) =>
      handler(event.payload)
    );
  }
  claudeDesktopLocalizationProgressListeners.add(handler);
  return () => claudeDesktopLocalizationProgressListeners.delete(handler);
}

export async function takePendingClaudeDesktopLaunchAfterRestart(): Promise<ClaudeDesktopPendingLaunch | null> {
  if (isTauri()) {
    return invoke("take_pending_claude_desktop_launch_after_restart");
  }
  return null;
}

export async function restartClaudeDesktopAfterAccessibilityGrant(
  request: ClaudeDesktopLaunchRequest = {}
): Promise<void> {
  if (isTauri()) {
    return invoke("restart_claude_desktop_after_accessibility_grant", { localize: request.localize });
  }
}

export async function openClaudeDesktopPath(kind: "staging"): Promise<void> {
  if (isTauri()) {
    return invoke("open_claude_desktop_path", { kind });
  }
}

export async function updateChatGPTDesktopSettings(
  request: UpdateChatGPTDesktopSettingsRequest
): Promise<ChatGPTDesktopSettings> {
  if (isTauri()) {
    return invoke("update_chatgpt_desktop_settings", { request });
  }
  mockChatGPTDesktopSettings = {
    ...mockChatGPTDesktopSettings,
    source: "mirror",
    customUrl: "",
    autoCheck: request.autoCheck ?? mockChatGPTDesktopSettings.autoCheck,
    askBefore: request.askBefore ?? mockChatGPTDesktopSettings.askBefore,
    windowsInstallMode: request.windowsInstallMode ?? mockChatGPTDesktopSettings.windowsInstallMode,
    installRoot: request.installRoot ?? mockChatGPTDesktopSettings.installRoot,
    keepUserDataOnUninstall: request.keepUserDataOnUninstall ?? mockChatGPTDesktopSettings.keepUserDataOnUninstall,
    syncHistoryOnLaunch: request.syncHistoryOnLaunch ?? mockChatGPTDesktopSettings.syncHistoryOnLaunch,
    pluginMarketplaceUnlockOnLaunch: request.pluginMarketplaceUnlockOnLaunch ?? mockChatGPTDesktopSettings.pluginMarketplaceUnlockOnLaunch,
    pluginAutoExpandOnLaunch: request.pluginAutoExpandOnLaunch ?? mockChatGPTDesktopSettings.pluginAutoExpandOnLaunch,
    modelWhitelistUnlockOnLaunch: request.modelWhitelistUnlockOnLaunch ?? mockChatGPTDesktopSettings.modelWhitelistUnlockOnLaunch,
    serviceTierControlsOnLaunch: request.serviceTierControlsOnLaunch ?? mockChatGPTDesktopSettings.serviceTierControlsOnLaunch,
    officialRemotePluginCacheOnLaunch: request.officialRemotePluginCacheOnLaunch ?? mockChatGPTDesktopSettings.officialRemotePluginCacheOnLaunch,
    computerUseGuardOnLaunch: request.computerUseGuardOnLaunch ?? mockChatGPTDesktopSettings.computerUseGuardOnLaunch,
    signedOnly: true
  };
  return mockChatGPTDesktopSettings;
}

export async function openChatGPTDesktopPath(kind: "install" | "staging" | "config"): Promise<void> {
  if (isTauri()) {
    return invoke("open_chatgpt_desktop_path", { kind });
  }
}

export async function listenChatGPTDesktopProgress(
  handler: (progress: ChatGPTDesktopProgress) => void
): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<ChatGPTDesktopProgress>("chatgpt-desktop://progress", (event) => handler(event.payload));
  }
  chatgptDesktopProgressListeners.add(handler);
  return () => chatgptDesktopProgressListeners.delete(handler);
}

export async function testProfileConnection(
  request: TestProfileConnectionRequest
): Promise<TestProfileConnectionResult> {
  return profileApi.testConnection(request);
}

export async function listProfileModels(
  request: ListProfileModelsRequest
): Promise<ListProfileModelsResult> {
  return profileApi.listModels(request);
}

export async function saveProfileDraft(request: SaveProfileDraftRequest): Promise<ProfileDraft> {
  return profileApi.save(request);
}

export async function startCodexOAuthLogin(): Promise<StartCodexOAuthLoginResult> {
  return profileApi.startCodexOAuthLogin();
}

export async function updateProfileDraft(request: UpdateProfileDraftRequest): Promise<ProfileDraft> {
  return profileApi.update(request);
}

export async function duplicateProfileDraft(request: DuplicateProfileDraftRequest): Promise<ProfileDraft> {
  return profileApi.duplicate(request);
}

export async function deleteProfileDraft(request: DeleteProfileDraftRequest): Promise<ProfileSummary> {
  return profileApi.delete(request);
}

export async function reorderProfileDrafts(request: ReorderProfileDraftsRequest): Promise<ProfileSummary> {
  return profileApi.reorder(request);
}

export async function loadUsageScriptState(profileId: string): Promise<UsageScriptState> {
  return profileApi.loadUsage(profileId);
}

export async function saveUsageScript(request: UsageScriptSaveRequest): Promise<UsageScriptState> {
  return profileApi.saveUsage(request);
}

export async function testUsageScript(request: UsageScriptSaveRequest): Promise<UsageQueryResult> {
  return profileApi.testUsage(request);
}

export async function queryProfileUsage(profileId: string): Promise<UsageQueryResult> {
  return profileApi.queryUsage(profileId);
}

export async function deleteUsageScript(profileId: string): Promise<UsageScriptState> {
  return profileApi.deleteUsage(profileId);
}

export async function previewProfileWrite(
  request: PreviewProfileWriteRequest
): Promise<PreviewProfileWriteResult> {
  return profileApi.previewWrite(request);
}

export async function previewProfileApply(
  request: PreviewProfileApplyRequest
): Promise<PreviewProfileApplyResult> {
  return profileApi.previewApply(request);
}

export async function applyProfile(request: ApplyProfileRequest): Promise<ApplyProfileResult> {
  return profileApi.apply(request);
}


function normalizeMockLocale(value: string | null | undefined): AppSettings["language"] | null {
  const normalized = value?.trim().replaceAll("_", "-").toLowerCase();
  if (!normalized) {
    return null;
  }
  if (
    normalized.startsWith("zh-hant") ||
    normalized.startsWith("zh-tw") ||
    normalized.startsWith("zh-hk") ||
    normalized.startsWith("zh-mo")
  ) {
    return "zh-TW";
  }
  if (normalized.startsWith("zh")) {
    return "zh-CN";
  }
  if (normalized.startsWith("en")) {
    return "en-US";
  }
  return null;
}

function initialMockLanguage(): AppSettings["language"] {
  const stored = typeof localStorage === "undefined" ? null : normalizeMockLocale(localStorage.getItem(MOCK_LANGUAGE_KEY));
  if (stored) {
    return stored;
  }
  const detected = typeof navigator === "undefined"
    ? null
    : (navigator.languages ?? [navigator.language]).map(normalizeMockLocale).find(Boolean) ?? null;
  const language = detected ?? "en-US";
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(MOCK_LANGUAGE_KEY, language);
  }
  return language;
}

let mockSettings: AppSettings = {
  theme: "system",
  language: initialMockLanguage(),
  backupBeforeWrite: true,
  redactSecrets: true,
  confirmInstallCommands: true,
  confirmConfigWrites: true
};

let mockGatewayRunning = false;

let mockGatewayStartedAt: string | null = null;

let mockGatewayPrivacyFilterMode: GatewayStatus["privacyFilterMode"] = "off";

let mockChatGPTDesktopSettings: ChatGPTDesktopSettings = {
  source: "mirror",
  customUrl: "",
  autoCheck: true,
  askBefore: true,
  signedOnly: true,
  windowsInstallMode: "msix",
  installRoot: "C:\\Users\\you\\AppData\\Local\\Programs\\Codex",
  keepUserDataOnUninstall: true,
  syncHistoryOnLaunch: false,
  pluginMarketplaceUnlockOnLaunch: true,
  pluginAutoExpandOnLaunch: true,
  modelWhitelistUnlockOnLaunch: true,
  serviceTierControlsOnLaunch: false,
  officialRemotePluginCacheOnLaunch: true,
  computerUseGuardOnLaunch: false
};

let mockChatGPTDesktopInstalled: ChatGPTDesktopState["installed"] = null;

let mockInstalledToolIds = new Set<string>(["codex", "claude", "gemini", "node", "git", "npm"]);
let mockUpdatedToolIds = new Set<string>();
const mockVsCodeAvailable = false;
const mockVsCodePluginToolIds = new Set(["codex-vscode", "claude-vscode", "gemini-code-assist"]);
let mockClaudeEnvConflicts = [
  {
    toolId: "claude",
    toolName: "Claude Code",
    variable: "ANTHROPIC_BASE_URL",
    currentValuePreview: "https://old-claude.example/v1",
    expectedValuePreview: "https://api.anthropic.com",
    scope: "user",
    severity: "warning" as const,
    message: "ANTHROPIC_BASE_URL affects Claude API connections and does not match the current CodeStudio configuration."
  }
];
const mockInitialToolVersions: Record<string, string> = {
  codex: "codex-cli 0.120.0",
  "codex-vscode": "openai.chatgpt@1.0.0",
  "claude-desktop": "installed",
  claude: "2.1.126",
  "claude-vscode": "anthropic.claude-code@1.0.0",
  gemini: "0.4.1",
  "gemini-code-assist": "google.geminicodeassist@1.0.0",
  opencode: "1.17.4",
  openclaw: "2026.6.6",
  hermes: "0.12.0",
  grok: "0.2.93",
  pi: "0.80.6",
  node: "v24.13.0",
  git: "2.51.0",
  npm: "11.6.2",
  pnpm: "10.24.0",
  bun: "1.3.4"
};
let mockToolVersions: Record<string, string> = { ...mockInitialToolVersions };

const mockToolUpdates: Record<string, { latestVersion: string; command: string; installedVersion?: string }> = {
  codex: {
    latestVersion: "0.121.0",
    installedVersion: "codex-cli 0.121.0",
    command: "npm install -g @openai/codex@latest"
  },
  "codex-vscode": {
    latestVersion: "1.1.0",
    installedVersion: "openai.chatgpt@1.1.0",
    command: "code --install-extension openai.chatgpt --force"
  },
  claude: {
    latestVersion: "2.1.130",
    installedVersion: "2.1.130",
    command: "npm install -g @anthropic-ai/claude-code@latest"
  },
  "claude-desktop": {
    latestVersion: "latest",
    installedVersion: "latest",
    command: "winget upgrade --id Anthropic.Claude --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
  },
  "claude-vscode": {
    latestVersion: "1.1.0",
    installedVersion: "anthropic.claude-code@1.1.0",
    command: "code --install-extension anthropic.claude-code --force"
  },
  gemini: {
    latestVersion: "0.4.2",
    installedVersion: "0.4.2",
    command: "npm install -g @google/gemini-cli@latest"
  },
  "gemini-code-assist": {
    latestVersion: "latest",
    installedVersion: "latest",
    command: "code --install-extension Google.geminicodeassist --force"
  },
  opencode: {
    latestVersion: "1.18.0",
    installedVersion: "1.18.0",
    command: "npm install -g opencode-ai@latest"
  },
  openclaw: {
    latestVersion: "2026.6.8",
    installedVersion: "2026.6.8",
    command: "npm install -g openclaw@latest"
  },
  hermes: {
    latestVersion: "latest",
    installedVersion: "latest",
    command: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm https://hermes-agent.nousresearch.com/install.ps1)\""
  },
  grok: {
    latestVersion: "latest",
    installedVersion: "latest",
    command: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://x.ai/cli/install.ps1 | iex\""
  },
  pi: {
    latestVersion: "0.80.6",
    installedVersion: "0.80.6",
    command: "npm install -g --ignore-scripts @earendil-works/pi-coding-agent@latest"
  },
  node: {
    latestVersion: "latest",
    installedVersion: "latest",
    command: "winget upgrade --id OpenJS.NodeJS.LTS --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
  },
  git: {
    latestVersion: "latest",
    installedVersion: "latest",
    command: "winget upgrade --id Git.Git --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
  },
  npm: {
    latestVersion: "11.7.0",
    installedVersion: "11.7.0",
    command: "npm install -g npm@latest"
  },
  pnpm: {
    latestVersion: "10.25.0",
    installedVersion: "10.25.0",
    command: "npm install -g pnpm@latest"
  },
  bun: {
    latestVersion: "latest",
    installedVersion: "latest",
    command: "winget upgrade --id Oven-sh.Bun --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
  }
};


let mockActivity: ActivityEvent[] = [
  {
    id: "mock-start",
    level: "info",
    message: "Opened CodeStudio Lite in browser preview mode.",
    createdAt: new Date().toISOString()
  },
  {
    id: "mock-detect",
    level: "ok",
    message: "Loaded sample detection snapshot.",
    createdAt: new Date().toISOString()
  }
];

let mockGatewayRequests: GatewayRequestLogEntry[] = [
  {
    id: "mock-request-1",
    timestamp: new Date().toISOString(),
    client: "Codex CLI",
    method: "POST",
    path: "/v1/chat/completions",
    provider: "official",
    model: null,
    status: 200,
    latencyMs: 12,
    errorSummary: null,
    privacyFilterMode: "redact",
    privacyFilterHitCount: 2,
    privacyFilterAction: "redacted"
  }
];

function mockDefaultUsageScript(templateType: UsageScriptTemplateType) {
  if (templateType === "newapi") {
    return `({
  request: {
    url: "{{baseUrl}}/api/user/self",
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      "Authorization": "Bearer {{accessToken}}",
      "User-Agent": "codestudio-lite/1.0",
      "New-Api-User": "{{userId}}"
    }
  },
  extractor: function(response) {
    if (response.success && response.data) {
      return {
        planName: response.data.group || "Default",
        remaining: response.data.quota / 500000,
        used: response.data.used_quota / 500000,
        total: (response.data.quota + response.data.used_quota) / 500000,
        unit: "USD"
      };
    }
    return { isValid: false, invalidMessage: response.message || "Query failed" };
  }
})`;
  }
  if (templateType === "token_plan") {
    return `({
  request: {
    url: "{{baseUrl}}/api/user/self",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    var data = response.data || response;
    var total = data.total || data.quota || data.entitlement || 0;
    var used = data.used || data.used_quota || 0;
    return {
      planName: data.plan || data.plan_name || data.group || "Token plan",
      remaining: data.remaining !== undefined ? data.remaining : Math.max(total - used, 0),
      used: used,
      total: total,
      unit: data.unit || "tokens"
    };
  }
})`;
  }
  if (templateType === "balance") {
    return `({
  request: {
    url: "{{baseUrl}}/dashboard/billing/credit_grants",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    var total = response.total_granted || response.total_available || response.balance || 0;
    var used = response.total_used || 0;
    return {
      remaining: response.total_available !== undefined ? response.total_available : Math.max(total - used, 0),
      used: used,
      total: total,
      unit: "USD"
    };
  }
})`;
  }
  return `({
  request: {
    url: "{{baseUrl}}/user/balance",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    return {
      isValid: response.is_active !== false,
      remaining: response.balance,
      unit: "USD"
    };
  }
})`;
}

function mockTool(overrides: Partial<ToolStatus> & Pick<ToolStatus, "id" | "name" | "command">): ToolStatus {
  return {
    category: "ai_tool",
    version: null,
    pathRepair: null,
    latestVersion: null,
    updateAvailable: false,
    updateCommand: null,
    installState: "missing",
    configState: "unknown",
    configPath: null,
    installPath: null,
    installCommand: null,
    details: null,
    running: false,
    ...overrides
  };
}

function mockToolUpdateFields(toolId: string): Pick<ToolStatus, "latestVersion" | "updateAvailable" | "updateCommand"> {
  const update = mockToolUpdates[toolId];
  if (!update) {
    return { latestVersion: null, updateAvailable: false, updateCommand: null };
  }
  const installed = toolId === "chatgpt-desktop" ? Boolean(mockChatGPTDesktopInstalled) : mockInstalledToolIds.has(toolId);
  return {
    latestVersion: installed && !mockUpdatedToolIds.has(toolId) ? update.latestVersion : null,
    updateAvailable: installed && !mockUpdatedToolIds.has(toolId),
    updateCommand: update.command
  };
}

function mockToolVersion(toolId: string): string | null {
  return mockInstalledToolIds.has(toolId) ? (mockToolVersions[toolId] ?? "installed") : null;
}

function markMockToolUpdated(toolId: string) {
  mockInstalledToolIds.add(toolId);
  mockUpdatedToolIds.add(toolId);
  mockToolVersions[toolId] =
    mockToolUpdates[toolId]?.installedVersion ??
    mockToolUpdates[toolId]?.latestVersion ??
    mockToolVersions[toolId] ??
    mockInitialToolVersions[toolId] ??
    "installed";
}

function mockVisibleTools(tools: ToolStatus[]) {
  return mockVsCodeAvailable
    ? tools
    : tools.filter((tool) => !mockVsCodePluginToolIds.has(tool.id));
}

function readMockDetectionCache(): DetectionSnapshot | null {
  try {
    const cached = window.localStorage.getItem(MOCK_DETECTION_CACHE_KEY);
    if (!cached) {
      return null;
    }
    const snapshot = JSON.parse(cached) as DetectionSnapshot;
    if (!snapshot.codexAuth) {
      return null;
    }
    return {
      ...snapshot,
      source: "cached",
      platform: snapshot.platform ?? "windows",
      envConflicts: snapshot.envConflicts ?? [],
      chatgptDesktopProductGeneration: snapshot.chatgptDesktopProductGeneration ?? "current"
    };
  } catch {
    return null;
  }
}

function writeMockDetectionCache(snapshot: DetectionSnapshot) {
  try {
    window.localStorage.setItem(
      MOCK_DETECTION_CACHE_KEY,
      JSON.stringify({
        ...snapshot,
        source: "preview"
      })
    );
  } catch {
    // Browser preview can still run without localStorage.
  }
}

function mockDetection(): DetectionSnapshot {
  const generatedAt = new Date().toISOString();
  const appConfigDir = "~/.codestudio-lite";
  const tools: ToolStatus[] = [
    mockTool({
      id: "codex",
      name: "Codex CLI",
      command: "codex",
      version: mockToolVersion("codex"),
      installState: mockInstalledToolIds.has("codex") ? "installed" : "missing",
      configState: "configured",
      configPath: "~/.codex/config.toml",
      installCommand: "npm install -g @openai/codex",
      details: mockInstalledToolIds.has("codex") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("codex")
    }),
    mockTool({
      id: "codex-vscode",
      name: "Codex VS Code",
      command: "code",
      version: mockToolVersion("codex-vscode"),
      installState: mockInstalledToolIds.has("codex-vscode") ? "installed" : "missing",
      configState: mockInstalledToolIds.has("codex-vscode") ? "configured" : "unknown",
      configPath: "~/.codex/config.toml",
      installCommand: "code --install-extension openai.chatgpt",
      details: mockInstalledToolIds.has("codex-vscode") ? "Ready" : "Extension not found",
      ...mockToolUpdateFields("codex-vscode")
    }),
    mockTool({
      id: "claude-desktop",
      name: "Claude Desktop",
      command: "Claude",
      version: mockToolVersion("claude-desktop"),
      installState: mockInstalledToolIds.has("claude-desktop") ? "installed" : "missing",
      configState: mockInstalledToolIds.has("claude-desktop") ? "configured" : "unknown",
      configPath: "~/AppData/Roaming/Claude",
      installCommand: "winget install --id Anthropic.Claude --exact",
      details: mockInstalledToolIds.has("claude-desktop") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("claude-desktop")
    }),
    mockTool({
      id: "claude",
      name: "Claude Code",
      command: "claude",
      version: mockToolVersion("claude"),
      installState: mockInstalledToolIds.has("claude") ? "installed" : "missing",
      configState: "configured",
      configPath: "~/.claude",
      installCommand: "npm install -g @anthropic-ai/claude-code",
      details: mockInstalledToolIds.has("claude") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("claude")
    }),
    mockTool({
      id: "claude-vscode",
      name: "Claude VS Code",
      command: "code",
      version: mockToolVersion("claude-vscode"),
      installState: mockInstalledToolIds.has("claude-vscode") ? "installed" : "missing",
      configState: "unknown",
      installCommand: "code --install-extension anthropic.claude-code",
      details: mockInstalledToolIds.has("claude-vscode") ? "Ready" : "Extension not found",
      ...mockToolUpdateFields("claude-vscode")
    }),
    mockTool({
      id: "gemini",
      name: "Gemini CLI",
      command: "gemini",
      version: mockToolVersion("gemini"),
      installState: mockInstalledToolIds.has("gemini") ? "installed" : "missing",
      configState: "unconfigured",
      configPath: "~/.gemini",
      installCommand: "npm install -g @google/gemini-cli",
      details: mockInstalledToolIds.has("gemini") ? "Profile needed" : "Command not found",
      ...mockToolUpdateFields("gemini")
    }),
    mockTool({
      id: "gemini-code-assist",
      name: "Gemini Code Assist",
      command: "code",
      version: mockToolVersion("gemini-code-assist"),
      installState: mockInstalledToolIds.has("gemini-code-assist") ? "installed" : "missing",
      configState: "unknown",
      installCommand: "code --install-extension Google.geminicodeassist",
      details: mockInstalledToolIds.has("gemini-code-assist") ? "Ready" : "Extension not found",
      ...mockToolUpdateFields("gemini-code-assist")
    }),
    mockTool({
      id: "opencode",
      name: "OpenCode",
      command: "opencode",
      version: mockToolVersion("opencode"),
      installState: mockInstalledToolIds.has("opencode") ? "installed" : "missing",
      configState: "unconfigured",
      installCommand: "npm install -g opencode-ai",
      details: mockInstalledToolIds.has("opencode") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("opencode")
    }),
    mockTool({
      id: "openclaw",
      name: "OpenClaw",
      command: "openclaw",
      version: mockToolVersion("openclaw"),
      installState: mockInstalledToolIds.has("openclaw") ? "installed" : "missing",
      configState: "unconfigured",
      installCommand: "npm install -g openclaw",
      details: mockInstalledToolIds.has("openclaw") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("openclaw")
    }),
    mockTool({
      id: "hermes",
      name: "Hermes",
      command: "hermes",
      version: mockToolVersion("hermes"),
      installState: mockInstalledToolIds.has("hermes") ? "installed" : "missing",
      configState: mockInstalledToolIds.has("hermes") ? "configured" : "unconfigured",
      configPath: "~/.hermes/config.yaml",
      installCommand:
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm https://hermes-agent.nousresearch.com/install.ps1)\"",
      details: mockInstalledToolIds.has("hermes") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("hermes")
    }),
    mockTool({
      id: "grok",
      name: "Grok",
      command: "grok",
      version: mockToolVersion("grok"),
      installState: mockInstalledToolIds.has("grok") ? "installed" : "missing",
      configState: mockInstalledToolIds.has("grok") ? "configured" : "unconfigured",
      configPath: "~/.grok/config.toml",
      installCommand:
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://x.ai/cli/install.ps1 | iex\"",
      details: mockInstalledToolIds.has("grok") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("grok")
    }),
    mockTool({
      id: "pi",
      name: "Pi Agent",
      command: "pi",
      version: mockToolVersion("pi"),
      installState: mockInstalledToolIds.has("pi") ? "installed" : "missing",
      configState: mockInstalledToolIds.has("pi") ? "configured" : "unconfigured",
      configPath: "~/.pi/agent/models.json",
      installCommand: "npm install -g --ignore-scripts @earendil-works/pi-coding-agent",
      details: mockInstalledToolIds.has("pi") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("pi")
    }),
    mockTool({
      id: "chatgpt-desktop",
      name: "ChatGPT Desktop",
      command: "Codex.exe",
      version: mockChatGPTDesktopInstalled?.version ?? null,
      installState: mockChatGPTDesktopInstalled ? "installed" : "missing",
      configState: "configured",
      configPath: "~/.codex",
      installCommand: "Install or update from the ChatGPT Desktop page",
      details: mockChatGPTDesktopInstalled
        ? `${mockChatGPTDesktopInstalled.source} / ${mockChatGPTDesktopInstalled.path}`
        : "Official ChatGPT desktop was not detected",
      ...mockToolUpdateFields("chatgpt-desktop")
    })
  ];

  const system: ToolStatus[] = [
    mockTool({
      id: "node",
      name: "Node.js",
      category: "system",
      command: "node",
      version: mockToolVersion("node"),
      installState: mockInstalledToolIds.has("node") ? "installed" : "missing",
      configState: "not_applicable",
      installCommand: "winget install --id OpenJS.NodeJS.LTS --exact",
      details: mockInstalledToolIds.has("node") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("node")
    }),
    mockTool({
      id: "git",
      name: "Git",
      category: "system",
      command: "git",
      version: mockToolVersion("git"),
      installState: mockInstalledToolIds.has("git") ? "installed" : "missing",
      configState: "not_applicable",
      installCommand: "winget install --id Git.Git --exact",
      details: mockInstalledToolIds.has("git") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("git")
    }),
    mockTool({
      id: "npm",
      name: "npm",
      category: "system",
      command: "npm",
      version: mockToolVersion("npm"),
      installState: mockInstalledToolIds.has("npm") ? "installed" : "missing",
      configState: "not_applicable",
      details: mockInstalledToolIds.has("npm") ? "Ready" : "Provided by Node.js LTS",
      ...mockToolUpdateFields("npm")
    }),
    mockTool({
      id: "pnpm",
      name: "pnpm",
      category: "system",
      command: "pnpm",
      version: mockToolVersion("pnpm"),
      installState: mockInstalledToolIds.has("pnpm") ? "installed" : "missing",
      configState: "not_applicable",
      installCommand: "npm install -g pnpm",
      details: mockInstalledToolIds.has("pnpm") ? "Ready" : "Command not found",
      pathRepair: mockInstalledToolIds.has("pnpm")
        ? null
        : {
            status: "warning",
            candidatePath: "C:\\Users\\you\\AppData\\Roaming\\npm\\pnpm.cmd",
            directory: "C:\\Users\\you\\AppData\\Roaming\\npm",
            message: "Found pnpm.cmd in a common install directory, but the current PATH cannot resolve command pnpm."
          },
      ...mockToolUpdateFields("pnpm")
    }),
    mockTool({
      id: "bun",
      name: "Bun",
      category: "system",
      command: "bun",
      version: mockToolVersion("bun"),
      installState: mockInstalledToolIds.has("bun") ? "installed" : "missing",
      configState: "not_applicable",
      installCommand: "winget install --id Oven-sh.Bun --exact",
      details: mockInstalledToolIds.has("bun") ? "Ready" : "Command not found",
      ...mockToolUpdateFields("bun")
    })
  ];

  return {
    generatedAt,
    source: "preview",
    platform: "windows",
    homeDir: "~",
    appConfigDir,
    activeProfile: mockDefaultActiveProfileId(),
    activeProfileName: mockActiveProfileName(),
    codexAuth: mockCodexAuthStatus,
    chatgptDesktopProductGeneration: mockChatGPTDesktopInstalled?.generation ?? "current",
    tools: mockVisibleTools(tools),
    system,
    envConflicts: mockClaudeEnvConflicts,
    claudeInstallKinds: { msix: { installed: true, version: "1.0.0", path: "C:/Program Files/WindowsApps/Claude" }, exe: { installed: false, version: null, path: null } },
    chatgptDesktopInstallKinds: { msix: { installed: true, version: "0.0.0", path: "C:/Program Files/WindowsApps/Codex" }, portable: { installed: false, version: null, path: null } },
    problems: [
      {
        id: "missing-pnpm",
        severity: "warning",
        title: "pnpm is missing",
        detail: "Some AI coding tools document pnpm-based install flows.",
        actionLabel: "Install"
      },
      {
        id: "codex-bootstrap",
        severity: "info",
        title: "Codex is installed",
        detail: "Bootstrap Codex to the Local Gateway once, then switch providers inside CodeStudio Lite.",
        actionLabel: "Configure"
      }
    ]
  };
}

function mockFindToolStatus(toolId: string): ToolStatus | null {
  const snapshot = mockDetection();
  return [...snapshot.tools, ...snapshot.system].find((tool) => tool.id === toolId) ?? null;
}

function mockToolInstallPlan(toolId: string): ToolInstallPlan {
  const status = mockFindToolStatus(toolId);
  const definitions: Record<
    string,
    { toolName: string; manager: string; command: string; dependency?: string; interactive?: boolean }
  > = {
    codex: { toolName: "Codex CLI", manager: "npm", command: "npm install -g @openai/codex" },
    "codex-vscode": {
      toolName: "Codex VS Code",
      manager: "vscode",
      command: "code --install-extension openai.chatgpt"
    },
    claude: { toolName: "Claude Code", manager: "npm", command: "npm install -g @anthropic-ai/claude-code" },
    "claude-desktop": {
      toolName: "Claude Desktop",
      manager: "winget",
      command: "winget install --id Anthropic.Claude --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
    },
    "claude-vscode": {
      toolName: "Claude VS Code",
      manager: "vscode",
      command: "code --install-extension anthropic.claude-code"
    },
    gemini: { toolName: "Gemini CLI", manager: "npm", command: "npm install -g @google/gemini-cli" },
    "gemini-code-assist": {
      toolName: "Gemini Code Assist",
      manager: "vscode",
      command: "code --install-extension Google.geminicodeassist"
    },
    opencode: { toolName: "OpenCode", manager: "npm", command: "npm install -g opencode-ai" },
    openclaw: { toolName: "OpenClaw", manager: "npm", command: "npm install -g openclaw" },
    hermes: {
      toolName: "Hermes",
      manager: "terminal",
      command: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm https://hermes-agent.nousresearch.com/install.ps1)\"",
      interactive: true
    },
    grok: {
      toolName: "Grok",
      manager: "terminal",
      command: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://x.ai/cli/install.ps1 | iex\"",
      interactive: true
    },
    pi: {
      toolName: "Pi Agent",
      manager: "npm",
      command: "npm install -g --ignore-scripts @earendil-works/pi-coding-agent"
    },
    node: {
      toolName: "Node.js",
      manager: "winget",
      command: "winget install --id OpenJS.NodeJS.LTS --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
    },
    git: {
      toolName: "Git",
      manager: "winget",
      command: "winget install --id Git.Git --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
    },
    npm: {
      toolName: "npm",
      manager: "winget",
      command: "winget install --id OpenJS.NodeJS.LTS --exact --accept-source-agreements --accept-package-agreements --disable-interactivity",
      dependency: "Node.js LTS"
    },
    pnpm: { toolName: "pnpm", manager: "npm", command: "npm install -g pnpm" },
    bun: {
      toolName: "Bun",
      manager: "winget",
      command: "winget install --id Oven-sh.Bun --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
    }
  };
  const definition = definitions[toolId];
  if (!definition) {
    throw new Error(`Tool '${toolId}' is not allowed for installation.`);
  }
  const alreadyInstalled = status?.installState === "installed";
  const missingDependency = definition.manager === "npm" && !mockInstalledToolIds.has("npm");
  const blocker = alreadyInstalled
    ? `${definition.toolName} is already installed.`
    : null;
  const prerequisites: ToolInstallPlan["prerequisites"] = missingDependency
    ? [
        {
          toolId: "node",
          toolName: "Node.js LTS",
          manager: "winget",
          command: "winget install --id OpenJS.NodeJS.LTS --exact --accept-source-agreements --accept-package-agreements --disable-interactivity",
          installed: mockInstalledToolIds.has("node"),
          canInstall: true,
          reason: "The target tool requires npm; npm is provided by Node.js LTS."
        }
      ]
    : [];
  const commands: ToolInstallPlan["commands"] = [
    ...prerequisites
      .filter((prerequisite) => !prerequisite.installed)
      .map((prerequisite) => ({
        toolId: prerequisite.toolId,
        toolName: prerequisite.toolName,
        stage: "prerequisite",
        manager: prerequisite.manager,
        command: prerequisite.command,
        requiresAdmin: true,
        interactive: false
      })),
    {
      toolId,
      toolName: definition.toolName,
      stage: "target",
      manager: definition.manager,
      command: definition.command,
      requiresAdmin: definition.manager === "winget",
      interactive: Boolean(definition.interactive)
    }
  ];
  const canInstall = !alreadyInstalled && !blocker;

  return {
    toolId,
    toolName: definition.toolName,
    manager: definition.manager,
    command: commands.map((item) => item.command).join(" && "),
    interactive: Boolean(definition.interactive),
    commands,
    prerequisites,
    requiresPrerequisites: prerequisites.some((prerequisite) => !prerequisite.installed),
    canInstall,
    alreadyInstalled,
    requiresAdmin: definition.manager === "winget" || prerequisites.some((prerequisite) => !prerequisite.installed),
    steps: buildMockInstallSteps(definition, status?.command ?? toolId),
    warnings: [],
    blocker
  };
}

function mockClaudeDesktopPlan(): ClaudeDesktopPlan {
  return {
    downloadUrl: "https://claude.ai/api/desktop/win32/x64/msix/latest/redirect",
    sha256: "Pending download verification",
    installLocation: "Windows App package registration"
  };
}

function mockToolLaunchPlan(toolId: string): ToolLaunchPlan {
  const status = mockFindToolStatus(toolId);
  if (!status) {
    throw new Error(`Tool '${toolId}' is not supported for launch.`);
  }
  const canonicalApp = canonicalProfileApp(toolId);
  const command =
    canonicalApp === "claude-desktop"
      ? "Claude"
      : ["codex-vscode", "claude-vscode", "gemini-code-assist"].includes(toolId)
        ? "code"
        : status.command;
  return {
    toolId: canonicalApp,
    toolName: status.name,
    command,
    canLaunch: status.installState === "installed",
    blocker: status.installState === "installed" ? null : `${status.name} cannot be found.`,
    shells: [
      {
        id: "cmd",
        label: "Command Prompt",
        command: "cmd.exe",
        available: true,
        default: true
      },
      {
        id: "powershell",
        label: "Windows PowerShell 5",
        command: "powershell.exe",
        available: true,
        default: false
      },
      {
        id: "pwsh",
        label: "PowerShell 7",
        command: "pwsh.exe",
        available: false,
        default: false
      }
    ],
    profiles: mockAllProfiles()
      .filter((profile) => canonicalProfileApp(profile.app) === canonicalApp && profile.mode === "config")
      .map((profile) => ({
        id: profile.id,
        name: profile.name,
        mode: profile.mode,
        provider: profile.provider,
        baseUrl: profile.baseUrl,
        isBuiltin: profile.isBuiltin
      }))
  };
}

function mockToolUpdatePlan(toolId: string): ToolInstallPlan {
  const status = mockFindToolStatus(toolId);
  const update = mockToolUpdates[toolId];
  if (!status) {
    throw new Error(`Tool '${toolId}' is not allowed for updates.`);
  }

  const manager = mockUpdateManager(status.updateCommand ?? update?.command ?? "");
  const command = status.updateCommand ?? update?.command ?? "";
  const installed = status.installState === "installed";
  const supported = Boolean(command);
  const canInstall = installed && supported && status.updateAvailable;
  const blocker = !installed
    ? `${status.name} is not installed and cannot be updated.`
    : !supported
      ? `${status.name} does not have a built-in update action.`
      : !status.updateAvailable
        ? `${status.name} has no detected update right now.`
        : null;

  return {
    toolId,
    toolName: status.name,
    manager,
    command,
    interactive: false,
    commands: [
      {
        toolId,
        toolName: status.name,
        stage: "update",
        manager,
        command,
        requiresAdmin: manager === "winget",
        interactive: false
      }
    ],
    prerequisites: [],
    requiresPrerequisites: false,
    canInstall,
    alreadyInstalled: installed,
    requiresAdmin: manager === "winget",
    steps: [
      { label: "Check installed app", detail: `Confirm ${status.name} is installed before updating.` },
      { label: "Run update command", detail: command ? `Run ${command}.` : "No update command is available." },
      { label: "Verify version", detail: "Refresh detection after the update command finishes." }
    ],
    warnings: [],
    blocker
  };
}

function mockToolUninstallCommand(toolId: string) {
  if (toolId === "claude-desktop") {
    return "winget uninstall --id Anthropic.Claude --exact";
  }
  if (toolId === "claude-vscode") {
    return "code --uninstall-extension anthropic.claude-code";
  }
  if (toolId === "codex-vscode") {
    return "code --uninstall-extension openai.chatgpt";
  }
  if (toolId === "gemini-code-assist") {
    return "code --uninstall-extension Google.geminicodeassist";
  }
  return `uninstall ${toolId}`;
}

function mockUpdateManager(command: string): string {
  if (command.startsWith("npm ")) {
    return "npm";
  }
  if (command.startsWith("winget ")) {
    return "winget";
  }
  if (command.startsWith("code ")) {
    return "vscode";
  }
  if (command.startsWith("powershell ")) {
    return "powershell";
  }
  return command ? "shell" : "manual";
}

function buildMockInstallSteps(
  definition: { toolName: string; manager: string; command: string; dependency?: string },
  commandName: string
): ToolInstallPlan["steps"] {
  if (definition.dependency) {
    return [
      {
        label: "Install upstream dependency",
        detail: `${definition.toolName} does not have a standalone installer.`
      }
    ];
  }
  if (definition.manager === "winget") {
    return [
      { label: "Check winget", detail: "Windows App Installer / winget must be available." },
      { label: "Install package", detail: `Run ${definition.command}.` },
      { label: "Verify command", detail: `After installation, run ${commandName} --version and refresh the dashboard.` }
    ];
  }
  if (definition.manager === "powershell") {
    return [
      { label: "Check PowerShell", detail: "Local PowerShell must be available." },
      { label: "Run official install script", detail: `Run ${definition.command}.` },
      { label: "Verify command", detail: `After installation, run ${commandName} --version and refresh the dashboard.` }
    ];
  }
  if (definition.manager === "vscode") {
    return [
      { label: "Check VS Code CLI", detail: "The local code command must be available." },
      { label: "Install VS Code extension", detail: `Run ${definition.command}.` },
      { label: "Verify extension", detail: "After installation, run code --list-extensions --show-versions and refresh the dashboard." }
    ];
  }
  const steps = [
    { label: "Check npm", detail: "Local npm must be available; npm usually ships with Node.js LTS." },
    { label: "Install global package", detail: `Run ${definition.command}.` },
    { label: "Verify command", detail: `After installation, run ${commandName} --version and refresh the dashboard.` }
  ];
  if (!mockInstalledToolIds.has("npm")) {
    steps.unshift({
      label: "Install prerequisite",
      detail: "npm is not available; if allowed, Node.js LTS will be installed through winget first."
    });
  }
  return steps;
}

function mockChatGPTDesktopInstallKind(
  value?: "msix" | "portable" | null
): "msix" | "portable" {
  return value === "portable" ? "portable" : "msix";
}

function mockChatGPTDesktopState(
  includeNetwork: boolean,
  installClass = mockChatGPTDesktopInstalled ? "managed" : "none",
  installKind?: "msix" | "portable" | null
): ChatGPTDesktopState {
  const kind = mockChatGPTDesktopInstallKind(installKind);
  const settings = {
    ...mockChatGPTDesktopSettings,
    windowsInstallMode: kind
  };
  const installed = mockChatGPTDesktopInstalled?.source === kind ? mockChatGPTDesktopInstalled : null;
  const release = {
    version: "26.609.4994.0",
    packageMoniker: "OpenAI.Codex_26.609.4994.0_x64__2p2nqsd0c76g0",
    architecture: "x64",
    packageKind: "msix",
    packageSource: "mirror",
    contentLength: 552187367,
    etag: '"giWhSb09BDY0sx9m3oF6LaBIWvo="',
    packageIdentity: "OpenAI.Codex",
    packageUrl: "https://codexapp.agentsmirror.com/latest/win",
    checksumsUrl: "https://codexapp.agentsmirror.com/latest/checksums",
    manifestUrl: "https://codexapp.agentsmirror.com/latest/manifest",
    sha256: "547618a744149221078a27febdfff65c924b46ff85ab2fe1595180e128be8d85",
    macosArm64Version: "26.609.41114",
    macosX64Version: "26.609.41114"
  };
  const upToDate = installed?.version === release.version;
  return {
    installKind: kind,
    generatedAt: new Date().toISOString(),
    platform: "windows",
    settings,
    installed,
    installClass: installed ? installClass : "none",
    release: includeNetwork ? release : null,
    plan: includeNetwork
      ? {
          upToDate,
          currentVersion: installed?.version ?? null,
          latestVersion: release.version,
          route: kind === "portable" ? "portable-fallback" : "msix-sideload",
          packageUrl: release.packageUrl,
          downloadSize: release.contentLength,
          sha256: release.sha256,
          stagedPath: null,
          installRoot: settings.installRoot,
          warnings: kind === "portable"
            ? ["The current plan will install the portable build and register Start menu and uninstall entries."]
            : [],
          capabilities: [
            {
              id: "add-appx",
              label: "Add-AppxPackage",
              status: "ok",
              detail: "MSIX install command is available."
            },
            {
              id: "msix-runtime",
              label: "MSIX runtime",
              status: "ok",
              detail: "Windows PackageManager can be activated."
            }
          ]
        }
      : null,
    stagingDir: "~/.codestudio-lite/downloads/chatgpt-desktop",
    notes: [
      "ChatGPT Desktop management covers install, update, uninstall, launch, and mirror-source flows.",
      "The ChatGPT Desktop installer content is not modified; downloads are SHA-256 verified before installation."
    ],
    running: false
  };
}

function mockChatGPTDesktopStageReport(
  installKind: "msix" | "portable" = "msix"
): ChatGPTDesktopStageReport {
  return {
    installKind,
    upToDate: false,
    stagedPath: "~/.codestudio-lite/downloads/chatgpt-desktop/OpenAI.Codex_26.609.4994.0_x64__2p2nqsd0c76g0.Msix",
    packageMoniker: "OpenAI.Codex_26.609.4994.0_x64__2p2nqsd0c76g0",
    downloadSize: 552187367,
    sha256: "547618a744149221078a27febdfff65c924b46ff85ab2fe1595180e128be8d85",
    hashVerified: true,
    route: installKind === "portable" ? "portable-fallback" : "msix-sideload",
    notes: ["Installer downloaded and passed SHA-256 verification."]
  };
}

async function simulateChatGPTDesktopProgress(steps: ChatGPTDesktopProgress[]) {
  for (const step of steps) {
    chatgptDesktopProgressListeners.forEach((listener) => listener(step));
    await new Promise((resolve) => window.setTimeout(resolve, 160));
  }
}

async function emitMockToolInstallProgress(stage: {
  rootToolId: string;
  toolId: string;
  toolName: string;
  stage: string;
  command: string;
  installKind?: "msix" | "exe" | null;
}) {
  const lines = [
    `> ${stage.command}`,
    `Resolving ${stage.toolName} package...`,
    `Running ${stage.stage} command...`
  ];
  for (const line of lines) {
    toolInstallProgressListeners.forEach((listener) =>
      listener({
        ...stage,
        stream: "stdout",
        chunk: `${line}\n`,
        done: false,
        exitCode: null
      })
    );
    await new Promise((resolve) => window.setTimeout(resolve, 140));
  }
  toolInstallProgressListeners.forEach((listener) =>
    listener({
      ...stage,
      stream: "status",
      chunk: "",
      done: true,
      exitCode: 0
    })
  );
}

async function simulateInstallTerminalOutput(sessionId: string, _command: string) {
  const lines = [
    `CodeStudio Lite interactive installer\r\n`,
    "Mock installer completed.\r\n"
  ];
  for (const line of lines) {
    installTerminalOutputListeners.forEach((listener) =>
      listener({
        sessionId,
        stream: "output",
        data: line,
        done: false,
        exitCode: null
      })
    );
    await new Promise((resolve) => window.setTimeout(resolve, 180));
  }
  markMockToolUpdated("hermes");
  writeMockDetectionCache(mockDetection());
  installTerminalOutputListeners.forEach((listener) =>
    listener({
      sessionId,
      stream: "status",
      data: "",
      done: true,
      exitCode: 0
    })
  );
}

function mockProfiles(): ProfileSummary {
  return {
    configDir: "~/.codestudio-lite",
    activeProfile: mockDefaultActiveProfileId(),
    activeProfileName: mockActiveProfileName(),
    activeProfilesByMode: cloneMockActiveProfilesByMode(),
    codexAuth: mockCodexAuthStatus,
    drafts: mockAllProfiles()
  };
}

function mockGatewayStatus(): GatewayStatus {
  const activeProfile = mockDefaultActiveProfile();
  return {
    running: mockGatewayRunning,
    host: "127.0.0.1",
    port: 43112,
    baseUrl: "http://127.0.0.1:43112/v1",
    healthUrl: "http://127.0.0.1:43112/health",
    authEnabled: true,
    tokenPreview: "codestudio-local-****7f3a2c",
    privacyFilterMode: mockGatewayPrivacyFilterMode,
    activeProfileId: activeProfile?.id ?? null,
    activeProfileName: activeProfile?.name ?? null,
    activeModel: activeProfile?.model ?? null,
    startedAt: mockGatewayStartedAt,
    lastError: null
  };
}

function mockActiveProfileName(): string | null {
  return mockDefaultActiveProfile()?.name ?? null;
}

function mockDefaultActiveProfileId(): string | null {
  return mockDefaultActiveProfile()?.id ?? null;
}

function emptyActiveProfilesByMode(): ActiveProfilesByMode {
  return {
    config: {},
    gateway: {}
  };
}

function cloneActiveProfilesByMode(value: ActiveProfilesByMode): ActiveProfilesByMode {
  return {
    config: { ...value.config },
    gateway: { ...value.gateway }
  };
}

function cloneMockActiveProfilesByMode(): ActiveProfilesByMode {
  return cloneActiveProfilesByMode(browserState.activeProfilesByMode);
}

function cleanMockActiveProfilesByMode(): void {
  const next = emptyActiveProfilesByMode();
  const modes: ProviderApplyMode[] = ["config", "gateway"];
  for (const mode of modes) {
    for (const [app, profileId] of Object.entries(browserState.activeProfilesByMode[mode])) {
      const canonicalApp = canonicalProfileApp(app);
      const profile = mockAllProfiles().find(
        (draft) => draft.id === profileId && canonicalProfileApp(draft.app) === canonicalApp && draft.mode === mode
      );
      if (profile) {
        next[mode][canonicalApp] = profile.id;
      }
    }
  }
  browserState.activeProfilesByMode = next;
}

function setMockActiveProfileForMode(mode: ProviderApplyMode, profile: ProfileDraft): ActiveProfilesByMode {
  const app = canonicalProfileApp(profile.app);
  return {
    ...cloneMockActiveProfilesByMode(),
    [mode]: {
      ...browserState.activeProfilesByMode[mode],
      [app]: profile.id
    }
  };
}

function mockProfileIsActive(profile: ProfileDraft): boolean {
  const app = canonicalProfileApp(profile.app);
  const activeProfiles = browserState.activeProfilesByMode[profile.mode];
  const activeProfileId = activeProfiles[app] ?? (app === "codex"
    ? activeProfiles["chatgpt-desktop"]
      ?? activeProfiles["codex-app"]
      ?? activeProfiles["codex-client"]
      ?? activeProfiles["codex-desktop"]
    : undefined);
  return activeProfileId === profile.id;
}

function mockDefaultActiveProfile(): ProfileDraft | null {
  const activeProfiles = browserState.activeProfilesByMode.gateway;
  const preferredApps = ["codex", "claude-desktop", "claude", "gemini", "gemini-code-assist", "opencode", "openclaw", "hermes", "grok", "pi"];
  for (const app of preferredApps) {
    const profileId = activeProfiles[app] ?? (app === "codex"
      ? activeProfiles["chatgpt-desktop"]
        ?? activeProfiles["codex-app"]
        ?? activeProfiles["codex-client"]
        ?? activeProfiles["codex-desktop"]
      : undefined);
    const profile = mockAllProfiles().find((draft) => draft.id === profileId && canonicalProfileApp(draft.app) === app);
    if (profile) {
      return profile;
    }
  }
  return Object.entries(activeProfiles)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([app, profileId]) => mockAllProfiles().find((draft) => draft.id === profileId && canonicalProfileApp(draft.app) === canonicalProfileApp(app)) ?? null)
    .find((profile): profile is ProfileDraft => Boolean(profile)) ?? null;
}

function isCodexFamilyApp(app: string): boolean {
  return canonicalProfileApp(app) === "codex";
}

function mockRestartMessageForProfile(profile: ProfileDraft, syncClaudeVsCode = false): string {
  const labels: Record<string, string> = {
    codex: "ChatGPT Desktop, Codex CLI, or Codex VS Code extension backend",
    "claude-desktop": "Claude Desktop",
    claude: syncClaudeVsCode ? "Claude Code or Claude VS Code extension backend" : "Claude Code",
    gemini: "Gemini CLI",
    "gemini-code-assist": "Gemini Code Assist",
    opencode: "OpenCode",
    openclaw: "OpenClaw",
    hermes: "Hermes",
    grok: "Grok",
    pi: "Pi Agent"
  };
  const app = canonicalProfileApp(profile.app);
  return `${labels[app] ?? profile.app} is not running, so no restart is needed.`;
}

function formatConfigState(state: ToolStatus["configState"]): string {
  if (state === "configured") {
    return "Configured";
  }
  if (state === "unconfigured") {
    return "Not configured";
  }
  if (state === "not_applicable") {
    return "Not applicable";
  }
  return "Unknown";
}

function aggregateStatus(statuses: Array<TestProfileConnectionResult["status"]>): TestProfileConnectionResult["status"] {
  if (statuses.includes("error")) {
    return "error";
  }
  if (statuses.includes("warning")) {
    return "warning";
  }
  return "ok";
}

function mockToolConfigPath(toolId: string): string | null {
  const canonicalToolId = canonicalProfileApp(toolId);
  const paths: Record<string, string> = {
    codex: "~/.codex/config.toml",
    claude: "~/.claude",
    "claude-desktop": `~/AppData/Local/Claude-3p/configLibrary/${CLAUDE_DESKTOP_PROFILE_ID}.json`,
    gemini: "~/.gemini",
    "gemini-code-assist": "~/AppData/Roaming/Code/User/settings.json",
    opencode: "~/.config/opencode",
    openclaw: "~/.openclaw",
    hermes: "~/.hermes/config.yaml",
    grok: "~/.grok/config.toml",
    pi: "~/.pi/agent/models.json"
  };
  return paths[canonicalToolId] ?? null;
}
