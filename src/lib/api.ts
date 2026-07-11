import { invoke } from "@tauri-apps/api/core";
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

const isTauri = () => Boolean(window.__TAURI_INTERNALS__);
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
  if (isTauri()) {
    return invoke("ensure_app_dirs");
  }
  return mockProfiles();
}

type DetectEnvironmentOptions = {
  waitForUpdates?: boolean;
};

export async function detectEnvironment(options: DetectEnvironmentOptions = {}): Promise<DetectionSnapshot> {
  if (isTauri()) {
    return invoke("detect_environment", { request: options });
  }
  const snapshot = mockDetection();
  writeMockDetectionCache(snapshot);
  return snapshot;
}

export async function loadCachedDetection(): Promise<DetectionSnapshot | null> {
  if (isTauri()) {
    return invoke("load_cached_detection");
  }
  return readMockDetectionCache();
}

export async function detectEnvironmentFresh(): Promise<DetectionSnapshot> {
  if (isTauri()) {
    return invoke("detect_environment_fresh");
  }
  const snapshot = mockDetection();
  writeMockDetectionCache(snapshot);
  return snapshot;
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
  return mockBackups;
}

export async function restoreBackup(request: RestoreBackupRequest): Promise<RestoreBackupResult> {
  if (isTauri()) {
    return invoke("restore_backup", { request });
  }

  const backup = mockBackups.find((item) => item.id === request.backupId);
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
  mockBackupSnapshots[safetyBackup.id] = cloneMockActiveProfilesByMode();
  mockBackups = [safetyBackup, ...mockBackups];
  mockActiveProfilesByMode = cloneActiveProfilesByMode(mockBackupSnapshots[backup.id] ?? emptyActiveProfilesByMode());
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
  if (isTauri()) {
    return invoke("test_profile_connection", { request });
  }

  const snapshot = mockDetection();
  const tool = snapshot.tools.find((item) => item.id === request.app);
  const checks: TestProfileConnectionResult["checks"] = [];
  const protocol = normalizeMockProtocol(request.protocol);

  if (tool) {
    checks.push({
      id: "tool-install",
      label: "Target tool",
      status: tool.installState === "installed" ? "ok" : "warning",
      detail: tool.version
        ? `${tool.name} is installed: ${tool.version}`
        : `${tool.name} is missing${tool.installCommand ? `. Suggested command: ${tool.installCommand}` : "."}`
    });
    checks.push({
      id: "tool-config",
      label: "Existing tool config",
      status: tool.configState === "configured" ? "ok" : "info",
      detail: tool.configPath
        ? `${formatConfigState(tool.configState)} at ${tool.configPath}`
        : "No config path is known for this tool."
    });
  } else {
    checks.push({
      id: "tool-install",
      label: "Target tool",
      status: "error",
      detail: `Tool '${request.app}' is not in the registry.`
    });
  }

  checks.push(validateBaseUrlCheckForProvider(request.provider, request.baseUrl));
  checks.push({
    id: "protocol",
    label: "Protocol",
    status: "ok",
    detail: `Selected upstream API protocol: ${mockProtocolLabel(protocol)}.`
  });
  checks.push({
    id: "model",
    label: "Model",
    status: request.model.trim() ? "ok" : "info",
    detail: request.model.trim() || "Model is not specified."
  });
  checks.push({
    id: "credential",
    label: "Credential",
    status: credentialStatus(request.provider, request.secretProvided),
    detail: request.apiKey?.trim()
      ? "Provider API key is ready to be stored in the system keychain when this profile is saved."
      : credentialDetail(request.provider, request.secretProvided)
  });
  checks.push({
    id: "network",
    label: "Provider ping",
    status: "info",
    detail: "Network provider checks are not sent yet."
  });

  const status = aggregateStatus(checks.map((check) => check.status));
  mockActivity = [
    {
      id: `mock-test-${Date.now()}`,
      level: status,
      message: `Ran profile connection checks for ${request.app}/${request.provider}.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return {
    generatedAt: new Date().toISOString(),
    status,
    checks
  };
}

export async function listProfileModels(
  request: ListProfileModelsRequest
): Promise<ListProfileModelsResult> {
  if (isTauri()) {
    return invoke("list_profile_models", { request });
  }

  const protocol = normalizeMockProtocol(request.protocol);
  validateBaseUrlForProviderOrThrow(request.provider, request.baseUrl);
  if (providerRequiresApiKey(request.provider) && !request.apiKey?.trim() && !request.profileId) {
    throw new Error("Provider API key is required to fetch models.");
  }

  return {
    generatedAt: new Date().toISOString(),
    provider: request.provider.trim(),
    protocol,
    baseUrl: request.baseUrl.trim(),
    models: mockProfileModels(protocol)
  };
}

export async function saveProfileDraft(request: SaveProfileDraftRequest): Promise<ProfileDraft> {
  if (isTauri()) {
    return invoke("save_profile_draft", { request });
  }

  const app = canonicalProfileApp(request.app);
  const mode = normalizeMockProfileMode(request.provider, request.mode);
  ensureCustomOfficialProfileAllowed(app, request.provider, mode);
  if (providerRequiresApiKey(request.provider) && !request.secretProvided) {
    throw new Error("Provider API key is required for non-official providers.");
  }
  validateBaseUrlForProviderOrThrow(request.provider, request.baseUrl);

  const protocol = normalizeMockProtocol(request.protocol);
  ensureMockProfileProtocolSupported(app, mode, request.provider, protocol);
  const profileId = uniqueMockProfileId(slugify(request.name));
  const now = new Date().toISOString();
  const profile: ProfileDraft = {
    id: profileId,
    name: request.name.trim(),
    icon: normalizeMockProfileIcon(request.icon),
    remark: normalizeMockProfileRemark(request.remark),
    app,
    isBuiltin: false,
    mode,
    provider: request.provider,
    protocol,
    model: request.model.trim(),
    modelMappings: normalizeMockProfileModelMappings(app, request.modelMappings),
    baseUrl: request.baseUrl.trim(),
    authRef: providerIsOfficial(request.provider) ? null : request.secretProvided ? `keychain:codestudio-lite/${profileId}/api_key` : null,
    createdAt: now,
    updatedAt: now,
    lastTestStatus: "pending",
    usageEnabled: false,
    sortOrder: nextMockProfileSortOrder(app, mode)
  };
  mockProfileDrafts = [...mockProfileDrafts, profile];
  mockActivity = [
    {
      id: `mock-profile-${Date.now()}`,
      level: "ok",
      message: `Saved profile draft '${profile.name}' for ${profile.app}/${profile.provider}.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return profile;
}

export async function startCodexOAuthLogin(): Promise<StartCodexOAuthLoginResult> {
  if (isTauri()) {
    return invoke("start_codex_oauth_login");
  }
  await openExternalUrl("https://developers.openai.com/codex/auth");
  return {
    started: true,
    command: null,
    message: "Opened the official Codex authorization page."
  };
}

export async function updateProfileDraft(request: UpdateProfileDraftRequest): Promise<ProfileDraft> {
  if (isTauri()) {
    return invoke("update_profile_draft", { request });
  }

  if (isBuiltinOfficialProfileId(request.profileId)) {
    throw new Error("Built-in official profiles cannot be modified.");
  }
  const index = mockProfileDrafts.findIndex((draft) => draft.id === request.profileId);
  if (index === -1) {
    throw new Error(`Profile '${request.profileId}' does not exist`);
  }
  if (!request.name.trim()) {
    throw new Error("Profile Name is required");
  }
  const existing = mockProfileDrafts[index];
  const mode = normalizeMockProfileMode(request.provider, request.mode ?? existing.mode);
  const protocol = normalizeMockProtocol(request.protocol ?? existing.protocol);
  const app = canonicalProfileApp(existing.app);
  ensureCustomOfficialProfileAllowed(app, request.provider, mode);
  ensureMockProfileProtocolSupported(app, mode, request.provider, protocol);
  validateBaseUrlForProviderOrThrow(request.provider, request.baseUrl);
  if (providerRequiresApiKey(request.provider) && !existing.authRef && !request.apiKey?.trim()) {
    throw new Error("Provider API key is required for non-official providers.");
  }

  const hasNewSecret = Boolean(request.apiKey?.trim());
  const updated: ProfileDraft = {
    ...existing,
    name: request.name.trim(),
    icon: normalizeMockProfileIcon(request.icon),
    remark: normalizeMockProfileRemark(request.remark),
    app,
    mode,
    provider: request.provider.trim(),
    protocol,
    model: request.model.trim(),
    modelMappings: normalizeMockProfileModelMappings(app, request.modelMappings ?? existing.modelMappings),
    baseUrl: request.baseUrl.trim(),
    authRef: providerIsOfficial(request.provider) ? null : hasNewSecret ? existing.authRef ?? `keychain:codestudio-lite/${existing.id}/api_key` : existing.authRef,
    updatedAt: new Date().toISOString(),
    lastTestStatus: "pending",
    usageEnabled: mockUsageScripts.get(existing.id)?.enabled ?? existing.usageEnabled
  };
  mockProfileDrafts = [
    ...mockProfileDrafts.slice(0, index),
    updated,
    ...mockProfileDrafts.slice(index + 1)
  ];
  cleanMockActiveProfilesByMode();
  mockActivity = [
    {
      id: `mock-profile-update-${Date.now()}`,
      level: "ok",
      message: `Updated profile draft '${updated.name}' for ${updated.app}/${updated.provider}.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return updated;
}

export async function duplicateProfileDraft(request: DuplicateProfileDraftRequest): Promise<ProfileDraft> {
  if (isTauri()) {
    return invoke("duplicate_profile_draft", { request });
  }

  const existing = mockAllProfiles().find((draft) => draft.id === request.profileId);
  if (!existing) {
    throw new Error(`Profile '${request.profileId}' does not exist`);
  }
  if (existing.isBuiltin) {
    throw new Error("Built-in official profiles cannot be duplicated.");
  }

  const profileId = uniqueMockProfileId(slugify(existing.name));
  const now = new Date().toISOString();
  const duplicated: ProfileDraft = {
    ...existing,
    id: profileId,
    isBuiltin: false,
    authRef: existing.authRef ? `keychain:codestudio-lite/${profileId}/api_key` : null,
    createdAt: now,
    updatedAt: now,
    sortOrder: nextMockProfileSortOrder(canonicalProfileApp(existing.app), existing.mode)
  };

  mockProfileDrafts = [...mockProfileDrafts, duplicated];
  mockActivity = [
    {
      id: `mock-profile-duplicate-${Date.now()}`,
      level: "ok",
      message: `Duplicated profile draft '${existing.name}' as '${duplicated.name}'.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return duplicated;
}

export async function deleteProfileDraft(request: DeleteProfileDraftRequest): Promise<ProfileSummary> {
  if (isTauri()) {
    return invoke("delete_profile_draft", { request });
  }

  const existing = mockAllProfiles().find((draft) => draft.id === request.profileId);
  if (!existing) {
    throw new Error(`Profile '${request.profileId}' does not exist`);
  }
  if (existing.isBuiltin || isBuiltinOfficialProfileId(existing.id)) {
    throw new Error("Built-in official profiles cannot be deleted.");
  }

  mockProfileDrafts = mockProfileDrafts.filter((draft) => draft.id !== request.profileId);
  for (const [app, profileId] of Object.entries(mockActiveProfilesByMode.config)) {
    if (profileId === request.profileId) {
      const canonicalApp = canonicalProfileApp(app);
      delete mockActiveProfilesByMode.config[app];
      mockActiveProfilesByMode.config[canonicalApp] = builtinOfficialProfileId(canonicalApp);
    }
  }
  for (const [app, profileId] of Object.entries(mockActiveProfilesByMode.gateway)) {
    if (profileId === request.profileId) {
      delete mockActiveProfilesByMode.gateway[app];
    }
  }
  cleanMockActiveProfilesByMode();
  mockActivity = [
    {
      id: `mock-profile-delete-${Date.now()}`,
      level: "ok",
      message: `Deleted profile draft '${existing.name}' for ${existing.app}/${existing.provider}.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return mockProfiles();
}

export async function reorderProfileDrafts(request: ReorderProfileDraftsRequest): Promise<ProfileSummary> {
  if (isTauri()) {
    return invoke("reorder_profile_drafts", { request });
  }

  const app = canonicalProfileApp(request.app);
  const profiles = mockAllProfiles().filter(
    (profile) => canonicalProfileApp(profile.app) === app && profile.mode === request.mode
  );
  const expectedIds = new Set(profiles.map((profile) => profile.id));
  const requestedIds = new Set(request.profileIds);
  if (expectedIds.size !== requestedIds.size || [...expectedIds].some((profileId) => !requestedIds.has(profileId))) {
    throw new Error("Profile order must include every profile in this tool category.");
  }

  mockProfileOrder[mockProfileOrderKey(app, request.mode)] = [...request.profileIds];
  const orderById = new Map(request.profileIds.map((profileId, index) => [profileId, index]));
  mockProfileDrafts = mockProfileDrafts.map((profile) => {
    const order = orderById.get(profile.id);
    return order === undefined ? profile : { ...profile, sortOrder: order };
  });
  return mockProfiles();
}

export async function loadUsageScriptState(profileId: string): Promise<UsageScriptState> {
  if (isTauri()) {
    return invoke("load_usage_script_state", { profileId });
  }
  return mockUsageScriptState(profileId);
}

export async function saveUsageScript(request: UsageScriptSaveRequest): Promise<UsageScriptState> {
  if (isTauri()) {
    return invoke("save_usage_script", { request });
  }
  const profile = mockAllProfiles().find((draft) => draft.id === request.profileId);
  if (!profile) {
    throw new Error(`Profile '${request.profileId}' does not exist`);
  }
  const config = mockUsageConfigFromRequest(request, mockUsageScripts.get(request.profileId));
  mockUsageScripts.set(request.profileId, config);
  return mockUsageScriptState(request.profileId);
}

export async function testUsageScript(request: UsageScriptSaveRequest): Promise<UsageQueryResult> {
  if (isTauri()) {
    return invoke("test_usage_script", { request });
  }
  const profile = mockAllProfiles().find((draft) => draft.id === request.profileId);
  if (profile && isCodexOfficialProfile(profile)) {
    throw new Error("Codex official OAuth usage can be queried directly; no custom script test is needed.");
  }
  return mockUsageResult(request.profileId, "test");
}

export async function queryProfileUsage(profileId: string): Promise<UsageQueryResult> {
  if (isTauri()) {
    return invoke("query_profile_usage", { profileId });
  }
  const config = mockUsageScripts.get(profileId);
  if (!config) {
    throw new Error("Usage query is not configured for this profile.");
  }
  if (!config.enabled) {
    throw new Error("Usage query is disabled for this profile.");
  }
  const result = mockUsageResult(profileId, "query");
  mockUsageResults.set(profileId, result);
  return result;
}

export async function deleteUsageScript(profileId: string): Promise<UsageScriptState> {
  if (isTauri()) {
    return invoke("delete_usage_script", { profileId });
  }
  mockUsageScripts.delete(profileId);
  mockUsageResults.delete(profileId);
  return mockUsageScriptState(profileId);
}

export async function previewProfileWrite(
  request: PreviewProfileWriteRequest
): Promise<PreviewProfileWriteResult> {
  if (isTauri()) {
    return invoke("preview_profile_write", { request });
  }

  const app = canonicalProfileApp(request.app);
  const profileId = uniqueMockProfileId(slugify(request.name));
  const profilePath = "~/.codestudio-lite/app_state.sqlite";
  const tool = mockDetection().tools.find((item) => item.id === app);
  const targetToolPath = tool?.configPath ?? mockToolConfigPath(app);
  const warnings: string[] = [];

  if (!request.name.trim()) {
    throw new Error("Profile Name is required");
  }
  const mode = normalizeMockProfileMode(request.provider, request.mode);
  ensureCustomOfficialProfileAllowed(app, request.provider, mode);
  if (providerRequiresApiKey(request.provider) && !request.secretProvided) {
    throw new Error("Provider API key is required for non-official providers.");
  }
  validateBaseUrlForProviderOrThrow(request.provider, request.baseUrl);
  const protocol = normalizeMockProtocol(request.protocol);
  ensureMockProfileProtocolSupported(app, mode, request.provider, protocol);
  const icon = normalizeMockProfileIcon(request.icon);

  if (profileId !== slugify(request.name)) {
    warnings.push(`Profile id '${slugify(request.name)}' already exists, so this draft will use '${profileId}'.`);
  }
  if (!tool) {
    warnings.push(`Tool '${app}' is not in the preview registry.`);
  }
  const generatedAt = new Date().toISOString();
  const remark = normalizeMockProfileRemark(request.remark);
  const profileContent = mockProfileSqlPreviewContent({
    id: profileId,
    name: request.name.trim(),
    icon,
    remark,
    app,
    mode,
    provider: request.provider.trim(),
    protocol,
    model: request.model.trim(),
    modelMappings: normalizeMockProfileModelMappings(app, request.modelMappings),
    baseUrl: request.baseUrl.trim(),
    authRef: providerIsOfficial(request.provider) ? null : request.secretProvided ? `keychain:codestudio-lite/${profileId}/api_key` : null,
    timestamp: generatedAt,
    secretStatus: providerIsOfficial(request.provider) ? "oauth" : request.secretProvided ? "pending_keychain" : "missing"
  });
  return {
    generatedAt,
    profileId,
    profilePath,
    targetToolPath,
    backupRequired: false,
    warnings,
    items: [
      {
        label: "Profile row",
        path: profilePath,
        action: "create",
        backupRequired: false,
        detail: `Save Profile Draft stores normalized metadata in SQLite for ${mockProtocolLabel(protocol)}/${request.provider} and excludes API keys.`,
        content: profileContent
      },
      {
        label: "Active tool profile pointer",
        path: profilePath,
        action: "not_modified",
        backupRequired: false,
        detail: "Saving a draft does not switch the active profile.",
        content: null
      },
      {
        label: `${tool?.name ?? "Target tool"} config`,
        path: targetToolPath,
        action: "future_confirmation_required",
        backupRequired: Boolean(targetToolPath),
        detail: "Client config is not modified when saving a Provider Profile. Client Bootstrap remains a separate confirmation flow.",
        content: null
      },
      {
        label: "Credential",
        path: null,
        action: request.secretProvided ? "pending_keychain" : "missing",
        backupRequired: false,
        detail: credentialDetail(request.provider, request.secretProvided),
        content: null
      }
    ]
  };
}

export async function previewProfileApply(
  request: PreviewProfileApplyRequest
): Promise<PreviewProfileApplyResult> {
  if (isTauri()) {
    return invoke("preview_profile_apply", { request });
  }

  const profile = mockAllProfiles().find((draft) => draft.id === request.profileId);
  if (!profile) {
    throw new Error(`Profile '${request.profileId}' does not exist`);
  }

  const isCodexTool = isCodexFamilyApp(profile.app);
  const tool = mockDetection().tools.find((item) => item.id === profile.app);
  const nativeConfigPath =
    mockNativeConfigPath(profile.app, profile.mode, profile.provider) ?? tool?.configPath ?? mockToolConfigPath(profile.app);
  const appliedPath = "~/.codestudio-lite/app_state.sqlite";
  const canApply = Boolean(tool) || isCodexTool;
  const configNativeDiff = mockNativeConfigPreview(profile, nativeConfigPath, "config");
  const gatewayNativeDiff = mockNativeConfigPreview(profile, nativeConfigPath, "gateway");
  const nativeDiff = profile.mode === "config" ? configNativeDiff : gatewayNativeDiff;
  const modePreviews = mockModePreviews(profile, configNativeDiff, gatewayNativeDiff);

  return {
    generatedAt: new Date().toISOString(),
    profileId: profile.id,
    profileName: profile.name,
    app: profile.app,
    provider: profile.provider,
    canApply,
    nativeDiff,
    modePreviews,
    warnings: canApply ? [] : [`Tool '${profile.app}' is not in the preview registry.`],
    envConflicts: canonicalProfileApp(profile.app) === "claude" ? mockClaudeEnvConflicts : [],
    items: [
      {
        label: "Active tool profile pointer",
        path: appliedPath,
        action: "update",
        backupRequired: false,
        detail: `Sets the SQLite active profile pointer for '${profile.app}' to '${profile.id}' before refreshing detection.`
      },
      {
        label: `${tool?.name ?? "Target tool"} native config`,
        path: nativeConfigPath,
        action: nativeDiff ? "create_or_update" : "not_modified",
        backupRequired: Boolean(nativeDiff),
        detail: nativeDiff
          ? "Selected profile type writes this client config; detailed file changes are shown below."
          : "This profile does not require a native client config write."
      },
      {
        label: "Credential",
        path: null,
        action: "not_written",
        backupRequired: false,
        detail: "CodeStudio Lite profile metadata never stores plaintext API keys. Config profiles may write the selected Provider key into the target client's native config."
      }
    ]
  };
}

export async function applyProfile(request: ApplyProfileRequest): Promise<ApplyProfileResult> {
  if (isTauri()) {
    return invoke("apply_profile", { request });
  }

  const profile = mockAllProfiles().find((draft) => draft.id === request.profileId);
  if (!profile) {
    throw new Error(`Profile '${request.profileId}' does not exist`);
  }
  if (mockProfileIsActive(profile)) {
    throw new Error("Profile is already active for this tool and profile category.");
  }
  const preview = await previewProfileApply(request);
  if (!preview.canApply) {
    throw new Error(`Profile '${request.profileId}' cannot be applied yet.`);
  }
  const mode = profile.mode;
  if (request.restartAfterApply && mode !== "config") {
    throw new Error("Apply and restart is only available for Config profiles.");
  }
  const syncClaudeVsCode =
    Boolean(request.syncClaudeVsCode) && mode === "config" && canonicalProfileApp(profile.app) === "claude";
  const selectedModePreview = preview.modePreviews.find((item) => item.mode === mode);
  if (!selectedModePreview?.supported) {
    throw new Error(selectedModePreview?.blockedReason ?? `${mode} is not supported for this profile.`);
  }
  if (request.restartAfterApply && !selectedModePreview.writesNativeConfig) {
    throw new Error("Apply and restart requires a native client config write for this profile.");
  }
  const backupId = new Date().toISOString().replaceAll(":", "-");
  const appliedPath = "~/.codestudio-lite/app_state.sqlite";
  const nativePath = selectedModePreview.writesNativeConfig ? selectedModePreview.nativeDiff?.path ?? null : null;
  const restartMessage = request.restartAfterApply ? mockRestartMessageForProfile(profile, syncClaudeVsCode) : null;
  mockBackupSnapshots[backupId] = cloneMockActiveProfilesByMode();
  mockActiveProfilesByMode = setMockActiveProfileForMode(mode, profile);
  const backup: BackupManifest = {
    id: backupId,
    reason: "apply-profile",
    profile: profile.id,
    changedFiles: [
      ...(nativePath ? [nativePath] : []),
      ...(syncClaudeVsCode ? ["~/.claude/config.json"] : [])
    ],
    createdAt: new Date().toISOString()
  };
  mockBackups = [backup, ...mockBackups];
  mockActivity = [
    {
      id: `mock-apply-${Date.now()}`,
      level: "ok",
      message: mode === "gateway"
        ? `Applied profile '${profile.name}' for ${profile.app}/${profile.provider} in Gateway profile.`
        : `Applied profile '${profile.name}' for ${profile.app}/${profile.provider} through direct client config profile.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return {
    summary: mockProfiles(),
    mode,
    backup,
    appliedPath,
    verified: true,
    nativePath,
    nativeVerified: Boolean(nativePath),
    restartRequested: Boolean(request.restartAfterApply),
    restartPerformed: false,
    restartMessage,
    gatewayStatus: null,
    envConflicts: canonicalProfileApp(profile.app) === "claude" ? mockClaudeEnvConflicts : []
  };
}

let mockActiveProfilesByMode: ActiveProfilesByMode = emptyActiveProfilesByMode();

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

let mockBackups: BackupManifest[] = [];

let mockBackupSnapshots: Record<string, ActiveProfilesByMode> = {};

let mockProfileDrafts: ProfileDraft[] = [];
let mockProfileOrder: Record<string, string[]> = {};

const builtinOfficialProfileDefinitions = [
  ["codex", "Codex Official", PROTOCOL_OPENAI_RESPONSES],
  ["claude-desktop", "Claude Desktop Official", PROTOCOL_ANTHROPIC_MESSAGES],
  ["claude", "Claude Code Official", PROTOCOL_ANTHROPIC_MESSAGES],
  ["gemini", "Gemini CLI Official", PROTOCOL_GOOGLE_GEMINI],
  ["gemini-code-assist", "Gemini Code Assist Official", PROTOCOL_GOOGLE_GEMINI],
  ["opencode", "OpenCode Official", PROTOCOL_OPENAI_CHAT_COMPLETIONS],
  ["openclaw", "OpenClaw Official", PROTOCOL_OPENAI_CHAT_COMPLETIONS],
  ["hermes", "Hermes Official", PROTOCOL_OPENAI_CHAT_COMPLETIONS]
] as const;

function builtinOfficialProfileId(app: string): string {
  return `builtin-official-${canonicalProfileApp(app)}`;
}

function isBuiltinOfficialProfileId(profileId: string): boolean {
  return profileId.startsWith("builtin-official-");
}

function builtinOfficialProfiles(): ProfileDraft[] {
  return builtinOfficialProfileDefinitions.map(([app, name, protocol]) => ({
    id: builtinOfficialProfileId(app),
    name,
    icon: null,
    remark: null,
    app,
    isBuiltin: true,
    mode: "config",
    provider: "official",
    protocol,
    model: "",
    modelMappings: [],
    baseUrl: "",
    authRef: null,
    createdAt: null,
    updatedAt: null,
    lastTestStatus: "builtin",
    usageEnabled: false,
    sortOrder: 0
  }));
}

function normalizeMockProfileIcon(value?: string | null): string | null {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) {
    return null;
  }
  if (trimmed.startsWith("data:image/")) {
    if (trimmed.length > 512 * 1024) {
      throw new Error("Profile icon image is too large.");
    }
    return trimmed;
  }
  if ([...trimmed].length > 4) {
    throw new Error("Profile icon text cannot be longer than 4 characters.");
  }
  return trimmed;
}

function normalizeMockProfileRemark(value?: string | null): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed.length > 0 ? trimmed : null;
}

function normalizeMockProfileModelMappings(
  app: string,
  mappings?: ProfileModelMapping[] | null
): ProfileModelMapping[] {
  if (canonicalProfileApp(app) !== "claude") {
    return [];
  }
  const normalized: ProfileModelMapping[] = [];
  const aliases = new Set<string>();
  for (const mapping of mappings ?? []) {
    const alias = mapping.alias.trim();
    const model = mapping.model.trim();
    const description = mapping.description?.trim() || null;
    if (!alias && !model && !description) {
      continue;
    }
    if (!alias || !model) {
      throw new Error("Claude Code model mappings require both alias and target model.");
    }
    const aliasKey = alias.toLowerCase();
    if (aliases.has(aliasKey)) {
      throw new Error(`Claude Code model mapping alias '${alias}' is duplicated.`);
    }
    aliases.add(aliasKey);
    normalized.push({
      alias,
      model,
      supports1m: Boolean(mapping.supports1m),
      description
    });
  }
  return normalized;
}

function mockAllProfiles(): ProfileDraft[] {
  return applyMockProfileOrder(
    [...builtinOfficialProfiles(), ...mockProfileDrafts].map((profile) => ({
      ...profile,
      usageEnabled: mockUsageScripts.get(profile.id)?.enabled ?? profile.usageEnabled
    }))
  ).sort(compareMockProfiles);
}

function nextMockProfileSortOrder(app: string, mode: ProviderApplyMode): number {
  const matching = mockAllProfiles().filter(
    (profile) => canonicalProfileApp(profile.app) === app && profile.mode === mode
  );
  return matching.reduce((max, profile) => Math.max(max, profile.sortOrder), -1) + 1;
}

function mockProfileOrderKey(app: string, mode: ProviderApplyMode): string {
  return `${canonicalProfileApp(app)}:${mode}`;
}

function applyMockProfileOrder(profiles: ProfileDraft[]): ProfileDraft[] {
  const nextProfiles = profiles.map((profile) => ({ ...profile }));
  const groups = new Set(
    nextProfiles.map((profile) => mockProfileOrderKey(profile.app, profile.mode))
  );
  for (const groupKey of groups) {
    const storedOrder = mockProfileOrder[groupKey];
    if (!storedOrder?.length) {
      continue;
    }
    const orderById = new Map(storedOrder.map((profileId, index) => [profileId, index]));
    let nextUnorderedIndex = storedOrder.length;
    const groupProfiles = nextProfiles
      .filter((profile) => mockProfileOrderKey(profile.app, profile.mode) === groupKey)
      .sort(compareMockProfiles);
    for (const profile of groupProfiles) {
      const storedIndex = orderById.get(profile.id);
      profile.sortOrder = storedIndex ?? nextUnorderedIndex;
      if (storedIndex === undefined) {
        nextUnorderedIndex += 1;
      }
    }
  }
  return nextProfiles;
}

function compareMockProfiles(left: ProfileDraft, right: ProfileDraft): number {
  const appCompare = canonicalProfileApp(left.app).localeCompare(canonicalProfileApp(right.app));
  if (appCompare !== 0) {
    return appCompare;
  }
  const modeCompare = left.mode.localeCompare(right.mode);
  if (modeCompare !== 0) {
    return modeCompare;
  }
  const orderCompare = left.sortOrder - right.sortOrder;
  if (orderCompare !== 0) {
    return orderCompare;
  }
  return left.name.localeCompare(right.name);
}

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

const mockUsageScripts = new Map<string, UsageScriptConfig>();
const mockUsageResults = new Map<string, UsageQueryResult>();

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

function mockUsageScriptState(profileId: string): UsageScriptState {
  const config = mockUsageScripts.get(profileId) ?? null;
  return {
    profileId,
    config,
    lastResult: mockUsageResults.get(profileId) ?? null,
    defaultCode: mockDefaultUsageScript(config?.templateType ?? "general")
  };
}

function mockUsageConfigFromRequest(
  request: UsageScriptSaveRequest,
  existing: UsageScriptConfig | undefined
): UsageScriptConfig {
  const now = new Date().toISOString();
  const profile = mockAllProfiles().find((draft) => draft.id === request.profileId);
  const codexOfficial = profile ? isCodexOfficialProfile(profile) : false;
  return {
    profileId: request.profileId,
    enabled: request.enabled,
    templateType: codexOfficial ? "general" : request.templateType,
    code: codexOfficial ? "" : request.code.trim() || mockDefaultUsageScript(request.templateType),
    apiKey: codexOfficial
      ? null
      : request.apiKey?.trim()
        ? `keychain:codestudio-lite/${request.profileId}/usage_api_key`
        : null,
    baseUrl: codexOfficial ? null : request.baseUrl?.trim() || null,
    accessToken: codexOfficial
      ? null
      : request.accessToken?.trim()
        ? `keychain:codestudio-lite/${request.profileId}/usage_access_token`
        : null,
    userId: codexOfficial ? null : request.userId?.trim() || null,
    timeoutSeconds: request.timeoutSeconds ?? existing?.timeoutSeconds ?? 10,
    autoQueryIntervalMinutes: request.autoQueryIntervalMinutes ?? existing?.autoQueryIntervalMinutes ?? 0,
    updatedAt: now
  };
}

function mockUsageResult(profileId: string, source: string): UsageQueryResult {
  const profile = mockAllProfiles().find((draft) => draft.id === profileId);
  if (!profile) {
    throw new Error(`Profile '${profileId}' does not exist`);
  }
  if (isCodexOfficialProfile(profile)) {
    return {
      success: true,
      data: [
        {
          isValid: true,
          planName: "Codex 5h limit (pro)",
          remaining: 58,
          used: 42,
          total: 100,
          unit: "%",
          extra: "Window: 5h / Reset: 1h"
        },
        {
          isValid: true,
          planName: "Codex weekly limit (pro)",
          remaining: 93,
          used: 7,
          total: 100,
          unit: "%",
          extra: "Window: 7d"
        },
        {
          isValid: true,
          planName: "Lifetime tokens (pro)",
          remaining: null,
          used: 123456,
          total: null,
          unit: "tokens",
          extra: "Mock official OAuth usage"
        }
      ],
      error: null,
      queriedAt: new Date().toISOString(),
      source: "codex_official_oauth"
    };
  }
  return {
    success: true,
    data: [
      {
        isValid: true,
        planName: profile.provider === "newapi" ? "Default" : "API Balance",
        remaining: 18.42,
        used: 6.58,
        total: 25,
        unit: "USD",
        extra: "Mock query result"
      }
    ],
    error: null,
    queriedAt: new Date().toISOString(),
    source
  };
}

function isCodexOfficialProfile(profile: ProfileDraft): boolean {
  return isCodexFamilyApp(profile.app) && providerIsOfficial(profile.provider);
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
  return cloneActiveProfilesByMode(mockActiveProfilesByMode);
}

function cleanMockActiveProfilesByMode(): void {
  const next = emptyActiveProfilesByMode();
  const modes: ProviderApplyMode[] = ["config", "gateway"];
  for (const mode of modes) {
    for (const [app, profileId] of Object.entries(mockActiveProfilesByMode[mode])) {
      const canonicalApp = canonicalProfileApp(app);
      const profile = mockAllProfiles().find(
        (draft) => draft.id === profileId && canonicalProfileApp(draft.app) === canonicalApp && draft.mode === mode
      );
      if (profile) {
        next[mode][canonicalApp] = profile.id;
      }
    }
  }
  mockActiveProfilesByMode = next;
}

function setMockActiveProfileForMode(mode: ProviderApplyMode, profile: ProfileDraft): ActiveProfilesByMode {
  const app = canonicalProfileApp(profile.app);
  return {
    ...cloneMockActiveProfilesByMode(),
    [mode]: {
      ...mockActiveProfilesByMode[mode],
      [app]: profile.id
    }
  };
}

function mockProfileIsActive(profile: ProfileDraft): boolean {
  const app = canonicalProfileApp(profile.app);
  const activeProfiles = mockActiveProfilesByMode[profile.mode];
  const activeProfileId = activeProfiles[app] ?? (app === "codex"
    ? activeProfiles["chatgpt-desktop"]
      ?? activeProfiles["codex-app"]
      ?? activeProfiles["codex-client"]
      ?? activeProfiles["codex-desktop"]
    : undefined);
  return activeProfileId === profile.id;
}

function mockDefaultActiveProfile(): ProfileDraft | null {
  const activeProfiles = mockActiveProfilesByMode.gateway;
  const preferredApps = ["codex", "claude-desktop", "claude", "gemini", "gemini-code-assist", "opencode", "openclaw", "hermes"];
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

function mockNativeConfigPath(app: string, mode: ProviderApplyMode, provider: string): string | null {
  const canonicalApp = canonicalProfileApp(app);
  if (mode === "gateway") {
    if (isCodexFamilyApp(canonicalApp)) {
      return "~/.codex/config.toml";
    }
    if (canonicalApp === "claude-desktop") {
      return mockClaudeDesktopProfilePath();
    }
    if (canonicalApp === "claude") {
      return "~/.claude/settings.json";
    }
    if (canonicalApp === "gemini") {
      return "~/.gemini/.env";
    }
    if (canonicalApp === "opencode") {
      return "~/.config/opencode/opencode.json";
    }
    if (canonicalApp === "openclaw") {
      return "~/.openclaw/openclaw.json";
    }
    if (canonicalApp === "hermes") {
      return "~/.hermes/config.yaml";
    }
    return null;
  }
  if (canonicalApp === "claude-desktop") {
    return mockClaudeDesktopProfilePath();
  }
  if (providerIsOfficial(provider) && !isCodexFamilyApp(canonicalApp)) {
    return null;
  }
  if (isCodexFamilyApp(canonicalApp)) {
    return "~/.codex/config.toml";
  }
  if (canonicalApp === "claude") {
    return "~/.claude/settings.json";
  }
  if (canonicalApp === "gemini") {
    return "~/.gemini/.env";
  }
  if (canonicalApp === "gemini-code-assist") {
    return "~/AppData/Roaming/Code/User/settings.json";
  }
  if (canonicalApp === "opencode") {
    return "~/.config/opencode/opencode.json";
  }
  if (canonicalApp === "openclaw") {
    return "~/.openclaw/openclaw.json";
  }
  if (canonicalApp === "hermes") {
    return "~/.hermes/config.yaml";
  }
  return null;
}

function mockClaudeDesktopProfilePath(): string {
  return `~/AppData/Local/Claude-3p/configLibrary/${CLAUDE_DESKTOP_PROFILE_ID}.json`;
}

function mockClaudeDesktopGatewayBaseUrl(): string {
  return "http://127.0.0.1:43112/tools/claude-desktop";
}

function mockNativeConfigPreview(
  profile: ProfileDraft,
  nativeConfigPath: string | null,
  mode: ProviderApplyMode
): PreviewProfileApplyResult["nativeDiff"] {
  if (!isCodexFamilyApp(profile.app)) {
    return withMockNativeContent(mockNonCodexNativeConfigPreview(profile, nativeConfigPath, mode));
  }

  if (mode === "config") {
    const wireApi = mockCodexWireApi(profile.protocol);
    if (!wireApi) {
      return null;
    }
    if (profile.provider === "official") {
      return withMockNativeContent({
        tool: "codex",
        path: nativeConfigPath ?? "~/.codex/config.toml",
        status: "preview",
        writeEnabled: true,
        changes: [
          {
            key: "model_provider",
            action: "update",
            before: "custom",
            after: "openai",
            detail: "Selects Codex's official OpenAI provider."
          },
          {
            key: "model",
            action: profile.model ? "update" : "remove",
            before: "gpt-5-codex",
            after: profile.model || null,
            detail: profile.model ? "Sets Codex to the selected official model." : "Official provider can use Codex's own model default."
          },
          {
            key: "model_providers.openai.base_url",
            action: "remove",
            before: "https://example.invalid/v1",
            after: null,
            detail: "Removes any custom OpenAI base URL override for the official provider."
          },
          {
            key: "model_providers.openai.requires_openai_auth",
            action: "add",
            before: null,
            after: "true",
            detail: "Uses Codex OAuth/OpenAI auth for the official provider."
          }
        ],
        warnings: [
          "Official provider uses the target client's own login.",
          "No Provider API key or model override is required."
        ]
      });
    }

    const providerId = "custom";
    const directChanges: NativeConfigDiffLine[] = [
      {
        key: "model_provider",
        action: "update",
        before: "custom",
        after: providerId,
        detail: "Selects the direct provider entry managed by CodeStudio Lite."
      },
      {
        key: `model_providers.${providerId}.wire_api`,
        action: "add",
        before: null,
        after: wireApi,
        detail: "Uses Codex's selected provider wire API."
      },
      {
        key: `model_providers.${providerId}.base_url`,
        action: "add",
        before: null,
        after: profile.baseUrl,
        detail: "Points Codex directly at the upstream Provider Base URL."
      },
      {
        key: `model_providers.${providerId}.requires_openai_auth`,
        action: "add",
        before: null,
        after: "true",
        detail: "Uses Codex OAuth/OpenAI auth for this direct upstream entry."
      }
    ];
    if (profile.model) {
      directChanges.push({
        key: "model",
        action: "update",
        before: "gpt-5-codex",
        after: profile.model,
        detail: "Sets Codex to the selected upstream model."
      });
    } else {
      directChanges.push({
        key: "model",
        action: "remove",
        before: "gpt-5-codex",
        after: null,
        detail: "Removes the model override when the profile has no selected model."
      });
    }
    return withMockNativeContent({
      tool: "codex",
      path: nativeConfigPath ?? "~/.codex/config.toml",
      status: "preview",
      writeEnabled: true,
      changes: directChanges,
      warnings: [
        "Config profiles write Codex's provider entry directly to the selected upstream Provider.",
        "Changing Codex config usually requires restarting Codex or opening a new Codex session."
      ]
    });
  }

  const gatewayBaseUrl = mockGatewayBaseUrlForTool(profile.app);
  const gatewayModel = profile.model || "default";
  return withMockNativeContent({
    tool: "codex",
    path: nativeConfigPath ?? "~/.codex/config.toml",
    status: "preview",
    writeEnabled: true,
    changes: [
      {
        key: "model_provider",
        action: "update",
        before: "custom",
        after: "custom",
        detail: "Selects the CodeStudio Lite localhost provider."
      },
      {
        key: "model",
        action: "update",
        before: "gpt-5-codex",
        after: gatewayModel,
        detail: "Sets Codex to the virtual model name resolved by the Local Gateway."
      },
      {
        key: "model_providers.custom.base_url",
        action: "add",
        before: null,
        after: gatewayBaseUrl,
        detail: "Points Codex at the tool-scoped CodeStudio Lite Local Gateway."
      },
      {
        key: "model_providers.custom.requires_openai_auth",
        action: "add",
        before: null,
        after: "false",
        detail: "Disables Codex official OpenAI auth for the Local Gateway provider entry."
      }
    ],
    warnings: [
      "Gateway profiles are a one-time relay injection target, not a direct Provider switch.",
      "Switching profiles later changes only the Gateway active profile for this tool.",
      "Codex official login is still required for the desktop app; the Local Gateway only takes over model requests.",
      "If Codex is already running, restart Codex or open a new Codex session after bootstrap so it reloads config.toml."
    ]
  });
}

function withMockNativeContent(
  preview: PreviewProfileApplyResult["nativeDiff"]
): PreviewProfileApplyResult["nativeDiff"] {
  if (!preview || preview.content) {
    return preview;
  }
  const lines = [
    `# ${preview.tool}`,
    `# ${preview.path}`,
    ...preview.changes
      .map((change) =>
        change.after === null
          ? `# remove ${change.key}`
          : `${change.key} = ${JSON.stringify(change.after)}`
      )
  ];
  return {
    ...preview,
    content: lines.join("\n")
  };
}

function mockCodexWireApi(protocol: string): string | null {
  const normalized = normalizeMockProtocol(protocol);
  if (normalized === PROTOCOL_OPENAI_RESPONSES) {
    return "responses";
  }
  if (normalized === PROTOCOL_OPENAI_CHAT_COMPLETIONS) {
    return "chat";
  }
  return null;
}

function mockNonCodexNativeConfigPreview(
  profile: ProfileDraft,
  nativeConfigPath: string | null,
  mode: ProviderApplyMode
): PreviewProfileApplyResult["nativeDiff"] {
  if (canonicalProfileApp(profile.app) === "claude-desktop") {
    return mockClaudeDesktopNativeConfigPreview(profile, nativeConfigPath, mode);
  }

  if (mode === "gateway") {
    return mockNonCodexGatewayNativeConfigPreview(profile, nativeConfigPath);
  }

  if (mode !== "config") {
    return null;
  }

  const app = canonicalProfileApp(profile.app);
  if (!mockConfigProtocolSupported(profile)) {
    return null;
  }
    const providerId = "custom";
  const secret = profile.authRef ? "keychain:****" : "(missing keychain secret)";
  const model = profile.model.trim();
  const path =
    mockNativeConfigPath(app, mode, profile.provider) ??
    nativeConfigPath ??
    mockToolConfigPath(app) ??
    "~/.codestudio-lite/native-config";

  if (providerIsOfficial(profile.provider)) {
    return mockNonCodexOfficialNativeConfigPreview(app, path);
  }

  if (app === "claude") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "env.ANTHROPIC_BASE_URL",
          action: "update",
          before: "https://api.anthropic.com",
          after: profile.baseUrl,
          detail: "Points Claude Code at the selected upstream Provider Base URL."
        },
        {
          key: "env.ANTHROPIC_AUTH_TOKEN",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key as Claude Code's bearer token."
        },
        {
          key: "model",
          action: model ? "update" : "remove",
          before: "claude-sonnet-4-5",
          after: model || null,
          detail: model ? "Sets Claude Code to the selected upstream model." : "Model is optional; no Claude model override will be written."
        }
      ],
      warnings: [
        "Config profiles write Claude Code user settings under the env section.",
        "The selected endpoint must be Anthropic/Claude-compatible; generic OpenAI-only endpoints need a translator.",
        "Restart Claude Code or open a new session after applying so settings reload."
      ]
    };
  }

  if (app === "gemini") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "GEMINI_API_KEY",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for Gemini CLI."
        },
        {
          key: "GOOGLE_GEMINI_BASE_URL",
          action: "update",
          before: "https://generativelanguage.googleapis.com",
          after: profile.baseUrl,
          detail: "Points Gemini CLI at the selected upstream Provider Base URL."
        },
        {
          key: "GEMINI_MODEL",
          action: model ? "update" : "remove",
          before: "gemini-2.5-pro",
          after: model || null,
          detail: model ? "Sets Gemini CLI to the selected upstream model." : "Model is optional; no Gemini model override will be written."
        }
      ],
      warnings: [
        "Gemini CLI reads API key and base URL from environment variables, so this adapter writes ~/.gemini/.env.",
        "Restart Gemini CLI or open a new terminal session after applying so environment variables reload."
      ]
    };
  }

  if (app === "gemini-code-assist") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "geminicodeassist.geminiApiKey",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for Gemini Code Assist."
        },
        {
          key: "Provider Base URL",
          action: "update",
          before: null,
          after: profile.baseUrl,
          detail: "Gemini Code Assist does not expose a VS Code setting for custom Base URL; this stays in the CodeStudio Lite profile."
        },
        {
          key: "Model",
          action: model ? "update" : "remove",
          before: null,
          after: model || null,
          detail: model ? "Gemini Code Assist does not expose a VS Code setting for model override; this stays in the CodeStudio Lite profile." : "Model is optional and Gemini Code Assist has no model override setting to write."
        }
      ],
      warnings: [
        "Gemini Code Assist stores its API key in VS Code user settings.",
        "The public Gemini Code Assist VS Code setting exposes the API key; Provider Base URL and model are kept in CodeStudio Lite but are not written to the extension config.",
        "Restart VS Code or reload the Gemini Code Assist extension after applying so settings reload."
      ]
    };
  }

  if (app === "opencode") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "$schema",
          action: "add",
          before: null,
          after: "https://opencode.ai/config.json",
          detail: "Keeps OpenCode config aligned with the published schema."
        },
        {
          key: `provider.${providerId}.npm`,
          action: "add",
          before: null,
          after: "@ai-sdk/openai-compatible",
          detail: "Uses OpenCode's OpenAI-compatible provider package."
        },
        {
          key: `provider.${providerId}.options.baseURL`,
          action: "add",
          before: null,
          after: profile.baseUrl,
          detail: "Points OpenCode at the selected upstream Provider Base URL."
        },
        {
          key: `provider.${providerId}.options.apiKey`,
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for OpenCode."
        },
        {
          key: "model",
          action: model ? "update" : "remove",
          before: "openai/gpt-5",
          after: model ? `${providerId}/${model}` : null,
          detail: model ? "Selects the provider/model pair in OpenCode." : "Model is optional; no OpenCode model override will be written."
        }
      ],
      warnings: [
        "OpenCode custom providers are written to opencode.json using the OpenAI-compatible provider package.",
        "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file."
      ]
    };
  }

  if (app === "openclaw") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "models.mode",
          action: "add",
          before: null,
          after: "merge",
          detail: "Merges CodeStudio Lite provider definitions with existing OpenClaw providers."
        },
        {
          key: `models.providers.${providerId}.api`,
          action: "add",
          before: null,
          after: "openai-completions",
          detail: "Uses OpenClaw's OpenAI-compatible API adapter."
        },
        {
          key: `models.providers.${providerId}.baseUrl`,
          action: "add",
          before: null,
          after: profile.baseUrl,
          detail: "Points OpenClaw at the selected upstream Provider Base URL."
        },
        {
          key: `models.providers.${providerId}.apiKey`,
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for OpenClaw."
        },
        {
          key: "agents.defaults.model.primary",
          action: model ? "update" : "unchanged",
          before: "openai/gpt-5",
          after: model ? `${providerId}/${model}` : null,
          detail: model ? "Selects the provider/model pair as OpenClaw's primary default." : "Model is optional; no OpenClaw model override will be written."
        }
      ],
      warnings: [
        "OpenClaw providers are written in models.mode=merge so existing provider definitions can stay available.",
        "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file."
      ]
    };
  }

  if (app === "hermes") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "model.provider",
          action: "add",
          before: null,
          after: "custom",
          detail: "Selects Hermes custom provider mode."
        },
        {
          key: "model.base_url",
          action: "add",
          before: null,
          after: profile.baseUrl,
          detail: "Points Hermes at the selected upstream Provider Base URL."
        },
        {
          key: "model.api_key",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for Hermes."
        },
        {
          key: "model.api_mode",
          action: "add",
          before: null,
          after: "chat_completions",
          detail: "Uses Hermes' OpenAI Chat Completions custom endpoint mode."
        },
        {
          key: "model.default",
          action: model ? "update" : "remove",
          before: "gpt-5",
          after: model || null,
          detail: model ? "Sets Hermes to the selected upstream model." : "Model is optional; no Hermes model override will be written."
        }
      ],
      warnings: [
        "Hermes custom providers are written to ~/.hermes/config.yaml under the model section.",
        "Existing YAML comments are not preserved when CodeStudio Lite writes the file.",
        "Hermes config profiles currently target OpenAI Chat Completions endpoints."
      ]
    };
  }

  return null;
}

function mockNonCodexOfficialNativeConfigPreview(
  app: string,
  path: string
): PreviewProfileApplyResult["nativeDiff"] {
  const base = {
    tool: app,
    path,
    status: "preview",
    writeEnabled: true
  };

  if (app === "claude") {
    return {
      ...base,
      changes: [
        {
          key: "env.ANTHROPIC_BASE_URL",
          action: "remove",
          before: "https://api.example.test/v1",
          after: null,
          detail: "Restores Claude Code to the client's own official endpoint."
        },
        {
          key: "env.ANTHROPIC_AUTH_TOKEN",
          action: "remove",
          before: "<redacted>",
          after: null,
          detail: "Removes the CodeStudio Lite managed API token from Claude settings."
        },
        {
          key: "model",
          action: "remove",
          before: "claude-sonnet-4-5",
          after: null,
          detail: "Removes the CodeStudio Lite managed model override."
        },
        {
          key: "env.ANTHROPIC_MODEL",
          action: "remove",
          before: "claude-sonnet-4-5",
          after: null,
          detail: "Removes the CodeStudio Lite managed model environment override."
        }
      ],
      warnings: [
        "Official provider restores Claude Code to its own login.",
        "CodeStudio Lite removes managed API or Gateway fields from Claude settings."
      ]
    };
  }

  if (app === "gemini") {
    return {
      ...base,
      changes: [
        {
          key: "GEMINI_API_KEY",
          action: "remove",
          before: "<redacted>",
          after: null,
          detail: "Removes the CodeStudio Lite managed Gemini API key."
        },
        {
          key: "GOOGLE_GEMINI_BASE_URL",
          action: "remove",
          before: "https://api.example.test/v1",
          after: null,
          detail: "Restores Gemini CLI to the client's own official endpoint."
        },
        {
          key: "GEMINI_MODEL",
          action: "remove",
          before: "gemini-2.5-pro",
          after: null,
          detail: "Removes the CodeStudio Lite managed model override."
        }
      ],
      warnings: [
        "Official provider restores Gemini CLI to its own login.",
        "CodeStudio Lite removes managed API or Gateway values from ~/.gemini/.env."
      ]
    };
  }

  if (app === "gemini-code-assist") {
    return {
      ...base,
      changes: [
        {
          key: "geminicodeassist.geminiApiKey",
          action: "remove",
          before: "<redacted>",
          after: null,
          detail: "Removes the CodeStudio Lite managed Gemini Code Assist API key."
        }
      ],
      warnings: [
        "Official provider restores Gemini Code Assist to its own login.",
        "CodeStudio Lite removes the managed API key setting from VS Code user settings."
      ]
    };
  }

  if (app === "opencode") {
    return {
      ...base,
      changes: [
        {
          key: "provider.custom",
          action: "remove",
          before: "managed provider entries",
          after: null,
          detail: "Removes CodeStudio Lite managed OpenCode provider entries."
        },
        {
          key: "model",
          action: "remove",
          before: "custom/default",
          after: null,
          detail: "Removes the active model only when it points to a CodeStudio Lite managed provider."
        }
      ],
      warnings: ["Official provider removes CodeStudio Lite managed OpenCode provider entries."]
    };
  }

  if (app === "openclaw") {
    return {
      ...base,
      changes: [
        {
          key: "models.providers.custom",
          action: "remove",
          before: "managed provider entries",
          after: null,
          detail: "Removes CodeStudio Lite managed OpenClaw provider entries."
        },
        {
          key: "agents.defaults.model.primary",
          action: "remove",
          before: "custom/default",
          after: null,
          detail: "Removes the primary model only when it points to a CodeStudio Lite managed provider."
        }
      ],
      warnings: ["Official provider removes CodeStudio Lite managed OpenClaw provider entries."]
    };
  }

  if (app === "hermes") {
    return {
      ...base,
      changes: [
        {
          key: "model.provider",
          action: "remove",
          before: "custom",
          after: null,
          detail: "Restores Hermes away from the CodeStudio Lite managed custom provider mode."
        },
        {
          key: "model.base_url",
          action: "remove",
          before: "https://api.example.test/v1",
          after: null,
          detail: "Removes the CodeStudio Lite managed Base URL."
        },
        {
          key: "model.api_key",
          action: "remove",
          before: "<redacted>",
          after: null,
          detail: "Removes the CodeStudio Lite managed API key."
        },
        {
          key: "model.api_mode",
          action: "remove",
          before: "chat_completions",
          after: null,
          detail: "Removes the CodeStudio Lite managed API mode."
        },
        {
          key: "model.default",
          action: "remove",
          before: "gpt-5",
          after: null,
          detail: "Removes the CodeStudio Lite managed model override."
        }
      ],
      warnings: ["Official provider removes CodeStudio Lite managed Hermes custom endpoint fields."]
    };
  }

  return null;
}

function mockClaudeDesktopNativeConfigPreview(
  profile: ProfileDraft,
  nativeConfigPath: string | null,
  mode: ProviderApplyMode
): PreviewProfileApplyResult["nativeDiff"] {
  if (mode === "config" && !providerIsOfficial(profile.provider) && !mockConfigProtocolSupported(profile)) {
    return null;
  }

  const path = nativeConfigPath ?? mockClaudeDesktopProfilePath();
  const secret = profile.authRef ? "keychain:****" : "(missing keychain secret)";
  const commonWarnings = [
    "Also updates ~/AppData/Local/Claude/claude_desktop_config.json.",
    "Also updates ~/AppData/Local/Claude-3p/claude_desktop_config.json and configLibrary/_meta.json.",
    "Restart Claude Desktop after applying so it reloads the config library."
  ];

  if (mode === "config" && providerIsOfficial(profile.provider)) {
    return {
      tool: "claude-desktop",
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "deploymentMode",
          action: "update",
          before: "3p",
          after: "1p",
          detail: "Restores Claude Desktop to first-party official mode in both config files."
        },
        {
          key: "configLibrary/_meta.appliedId",
          action: "remove",
          before: CLAUDE_DESKTOP_PROFILE_ID,
          after: null,
          detail: "Removes the CodeStudio Lite profile from Claude Desktop's 3P config library."
        },
        {
          key: `${CLAUDE_DESKTOP_PROFILE_ID}.json`,
          action: "remove",
          before: "CodeStudio Lite 3P profile",
          after: null,
          detail: "Deletes the managed CodeStudio Lite Claude Desktop 3P profile file."
        }
      ],
      warnings: [
        "Claude Desktop official mode restores deploymentMode=1p and removes the CodeStudio Lite 3P profile entry.",
        "No Provider API key or model override is required.",
        ...commonWarnings
      ]
    };
  }

  if (mode === "config") {
    const modelSpecs = mockClaudeDesktopDirectModelSpecs(profile);
    return {
      tool: "claude-desktop",
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "developer_settings.allowDevTools",
          action: "update",
          before: "false",
          after: "true",
          detail: "Enables Claude Desktop developer mode before applying the managed 3P profile."
        },
        {
          key: "deploymentMode",
          action: "update",
          before: "1p",
          after: "3p",
          detail: "Switches Claude Desktop to third-party provider mode in both config files."
        },
        {
          key: "inferenceProvider",
          action: "update",
          before: "official",
          after: "gateway",
          detail: "Uses Claude Desktop's built-in 3P inference gateway provider."
        },
        {
          key: "inferenceGatewayBaseUrl",
          action: "update",
          before: "https://api.anthropic.com",
          after: profile.baseUrl,
          detail: "Points Claude Desktop directly at the selected Anthropic-compatible Provider Base URL."
        },
        {
          key: "inferenceGatewayApiKey",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key in Claude Desktop's 3P profile."
        },
        {
          key: "inferenceModels",
          action: modelSpecs.length ? "update" : "remove",
          before: "[]",
          after: modelSpecs.length ? JSON.stringify(modelSpecs) : null,
          detail: modelSpecs.length
            ? "Exposes the selected Claude-safe model in Claude Desktop's model menu."
            : "Model is optional; no Claude Desktop model menu override will be written."
        }
      ],
      warnings: [
        "Claude Desktop config profile writes the 3P profile system used by Claude Desktop.",
        "CodeStudio Lite enables Claude Desktop developer mode before writing the 3P profile if it is not already enabled.",
        "The selected endpoint must be Anthropic Messages compatible; generic OpenAI-only endpoints need Gateway profiles.",
        ...commonWarnings
      ]
    };
  }

  const modelSpecs = mockClaudeDesktopGatewayModelSpecs(profile);
  return {
    tool: "claude-desktop",
    path,
    status: "preview",
    writeEnabled: true,
    changes: [
      {
        key: "developer_settings.allowDevTools",
        action: "update",
        before: "false",
        after: "true",
        detail: "Enables Claude Desktop developer mode before applying the managed Gateway profile."
      },
      {
        key: "deploymentMode",
        action: "update",
        before: "1p",
        after: "3p",
        detail: "Switches Claude Desktop to third-party provider mode in both config files."
      },
      {
        key: "inferenceProvider",
        action: "update",
        before: "official",
        after: "gateway",
        detail: "Uses Claude Desktop's built-in 3P inference gateway provider."
      },
      {
        key: "inferenceGatewayBaseUrl",
        action: "update",
        before: "https://api.anthropic.com",
        after: mockClaudeDesktopGatewayBaseUrl(),
        detail: "Points Claude Desktop at the tool-scoped CodeStudio Lite Local Gateway."
      },
      {
        key: "inferenceGatewayApiKey",
        action: "add",
        before: null,
        after: "codestudio-local-****7f3a2c",
        detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
      },
      {
        key: "inferenceModels",
        action: "update",
        before: "[]",
        after: JSON.stringify(modelSpecs),
        detail: "Exposes Claude Desktop-safe route IDs while the Gateway resolves the real upstream model."
      }
    ],
    warnings: [
      "Claude Desktop gateway profile writes the 3P profile to the tool-scoped CodeStudio Lite Local Gateway URL.",
      "CodeStudio Lite enables Claude Desktop developer mode before writing the Gateway profile if it is not already enabled.",
      "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.",
      ...commonWarnings
    ]
  };
}

function mockClaudeDesktopDirectModelSpecs(profile: ProfileDraft): unknown[] {
  const model = profile.model.trim();
  if (!model || !mockClaudeDesktopSafeModelId(model)) {
    return [];
  }
  return [model];
}

function mockClaudeDesktopGatewayModelSpecs(profile: ProfileDraft): unknown[] {
  const model = profile.model.trim();
  if (!model) {
    return CLAUDE_DESKTOP_DEFAULT_ROUTES.map((name) => ({ name, supports1m: true }));
  }
  if (mockClaudeDesktopSafeModelId(model)) {
    return [{ name: model, supports1m: true }];
  }
  return [{ name: CLAUDE_DESKTOP_DEFAULT_ROUTE_ID, labelOverride: model, supports1m: true }];
}

function mockClaudeDesktopSafeModelId(model: string): boolean {
  const normalized = model.trim().toLowerCase();
  if (normalized.includes("[1m]")) {
    return false;
  }
  const routeTail = normalized.startsWith("anthropic/claude-")
    ? normalized.slice("anthropic/claude-".length)
    : normalized.startsWith("claude-")
      ? normalized.slice("claude-".length)
      : "";
  return ["sonnet-", "opus-", "haiku-", "fable-"].some((prefix) => routeTail.startsWith(prefix) && routeTail.length > prefix.length);
}

function mockNonCodexGatewayNativeConfigPreview(
  profile: ProfileDraft,
  nativeConfigPath: string | null
): PreviewProfileApplyResult["nativeDiff"] {
  const app = canonicalProfileApp(profile.app);
  const path =
    mockNativeConfigPath(app, "gateway", profile.provider) ??
    nativeConfigPath ??
    mockToolConfigPath(app) ??
    "~/.codestudio-lite/native-config";
  const gatewayBaseUrl = mockGatewayBaseUrlForTool(app);
  const providerId = "custom";
  const providerName = "CodeStudio Lite Local Gateway";
  const localToken = "codestudio-local-****7f3a2c";
  const localModel = profile.model || "default";
  const modelRef = `${providerId}/${localModel}`;
  const commonWarnings = [
    "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.",
    "Real upstream Provider API keys stay in the system keychain and are used by the local gateway."
  ];

  if (app === "claude") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "env.ANTHROPIC_BASE_URL",
          action: "update",
          before: "https://api.anthropic.com",
          after: gatewayBaseUrl,
          detail: "Points Claude Code at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: "env.ANTHROPIC_AUTH_TOKEN",
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "model",
          action: "update",
          before: "claude-sonnet-4-5",
          after: localModel,
          detail: "Sets Claude Code to the virtual model name resolved by the Local Gateway."
        },
        {
          key: "env.ANTHROPIC_MODEL",
          action: "update",
          before: "claude-sonnet-4-5",
          after: localModel,
          detail: "Keeps the local gateway virtual model available to Claude Code environment consumers."
        }
      ],
      warnings: [
        "Gateway profiles write Claude Code settings to the tool-scoped local gateway URL.",
        "Restart Claude Code or open a new session after applying so settings reload.",
        ...commonWarnings
      ]
    };
  }

  if (app === "gemini") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "GEMINI_API_KEY",
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "GOOGLE_GEMINI_BASE_URL",
          action: "update",
          before: "https://generativelanguage.googleapis.com",
          after: gatewayBaseUrl,
          detail: "Points Gemini CLI at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: "GEMINI_MODEL",
          action: "update",
          before: "gemini-2.5-pro",
          after: localModel,
          detail: "Sets Gemini CLI to the virtual model name resolved by the Local Gateway."
        }
      ],
      warnings: [
        "Gateway profiles write Gemini CLI environment values to the tool-scoped local gateway URL.",
        "Restart Gemini CLI or open a new terminal session after applying so environment variables reload.",
        ...commonWarnings
      ]
    };
  }

  if (app === "opencode") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: `provider.${providerId}.npm`,
          action: "add",
          before: null,
          after: "@ai-sdk/openai-compatible",
          detail: "Uses OpenCode's OpenAI-compatible provider package."
        },
        {
          key: `provider.${providerId}.name`,
          action: "add",
          before: null,
          after: providerName,
          detail: "Adds a readable provider label for the Local Gateway."
        },
        {
          key: `provider.${providerId}.options.baseURL`,
          action: "add",
          before: null,
          after: gatewayBaseUrl,
          detail: "Points OpenCode at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: `provider.${providerId}.options.apiKey`,
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "model",
          action: "update",
          before: "openai/gpt-5",
          after: modelRef,
          detail: "Selects the local gateway provider/model pair in OpenCode."
        },
        {
          key: `provider.${providerId}.models.${localModel}.name`,
          action: "add",
          before: null,
          after: localModel,
          detail: "Registers the local gateway virtual model under the managed provider."
        }
      ],
      warnings: [
        "Gateway profiles write OpenCode's provider entry to the tool-scoped local gateway URL.",
        "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file.",
        ...commonWarnings
      ]
    };
  }

  if (app === "openclaw") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "models.mode",
          action: "add",
          before: null,
          after: "merge",
          detail: "Merges CodeStudio Lite provider definitions with existing OpenClaw providers."
        },
        {
          key: `models.providers.${providerId}.api`,
          action: "add",
          before: null,
          after: "openai-completions",
          detail: "Uses OpenClaw's OpenAI-compatible API adapter."
        },
        {
          key: `models.providers.${providerId}.baseUrl`,
          action: "add",
          before: null,
          after: gatewayBaseUrl,
          detail: "Points OpenClaw at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: `models.providers.${providerId}.apiKey`,
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "agents.defaults.model.primary",
          action: "update",
          before: "openai/gpt-5",
          after: modelRef,
          detail: "Selects the local gateway provider/model pair as OpenClaw's primary default."
        }
      ],
      warnings: [
        "Gateway profiles write OpenClaw's provider entry to the tool-scoped local gateway URL.",
        "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file.",
        ...commonWarnings
      ]
    };
  }

  if (app === "hermes") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "model.provider",
          action: "add",
          before: null,
          after: "custom",
          detail: "Selects Hermes custom provider mode."
        },
        {
          key: "model.base_url",
          action: "add",
          before: null,
          after: gatewayBaseUrl,
          detail: "Points Hermes at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: "model.api_key",
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "model.api_mode",
          action: "add",
          before: null,
          after: "chat_completions",
          detail: "Uses Hermes' OpenAI Chat Completions custom endpoint mode."
        },
        {
          key: "model.default",
          action: "update",
          before: "gpt-5",
          after: localModel,
          detail: "Sets Hermes to the virtual model name resolved by the Local Gateway."
        }
      ],
      warnings: [
        "Gateway profiles write Hermes custom provider settings to the tool-scoped local gateway URL.",
        "Existing YAML comments are not preserved when CodeStudio Lite writes the file.",
        ...commonWarnings
      ]
    };
  }

  return null;
}

function mockConfigProtocolSupportedFields(app: string, provider: string, value: string): boolean {
  if (providerIsOfficial(provider)) {
    return true;
  }
  let protocol: string;
  try {
    protocol = normalizeMockProtocol(value);
  } catch {
    return false;
  }
  const canonicalApp = canonicalProfileApp(app);
  if (canonicalApp === "codex") {
    return Boolean(mockCodexWireApi(protocol));
  }
  if (canonicalApp === "claude-desktop") {
    return protocol === PROTOCOL_ANTHROPIC_MESSAGES;
  }
  if (canonicalApp === "claude") {
    return protocol === PROTOCOL_ANTHROPIC_MESSAGES;
  }
  if (canonicalApp === "gemini" || canonicalApp === "gemini-code-assist") {
    return protocol === PROTOCOL_GOOGLE_GEMINI;
  }
  if (canonicalApp === "opencode") {
    return protocol === PROTOCOL_OPENAI_CHAT_COMPLETIONS || protocol === PROTOCOL_OPENAI_RESPONSES;
  }
  if (canonicalApp === "openclaw") {
    return protocol === PROTOCOL_OPENAI_CHAT_COMPLETIONS;
  }
  if (canonicalApp === "hermes") {
    return protocol === PROTOCOL_OPENAI_CHAT_COMPLETIONS;
  }
  return false;
}

function mockProfileProtocolSupportedForMode(
  app: string,
  mode: ProviderApplyMode,
  provider: string,
  protocol: string
): boolean {
  if (providerIsOfficial(provider) || mode === "gateway") {
    return true;
  }
  return mockConfigProtocolSupportedFields(app, provider, protocol);
}

function ensureMockProfileProtocolSupported(
  app: string,
  mode: ProviderApplyMode,
  provider: string,
  protocol: string
): void {
  if (mockProfileProtocolSupportedForMode(app, mode, provider, protocol)) {
    return;
  }
  throw new Error(`Config profiles do not support ${mockProtocolLabel(protocol)} for '${canonicalProfileApp(app)}'.`);
}

function mockConfigProtocolSupported(profile: ProfileDraft): boolean {
  return mockConfigProtocolSupportedFields(profile.app, profile.provider, profile.protocol);
}

function mockGatewayBaseUrlForTool(toolId: string): string {
  return `http://127.0.0.1:43112/tools/${canonicalProfileApp(toolId)}/v1`;
}

function requireMockField(label: string, value: unknown): string {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`${label} is required`);
  }
  return value.trim();
}

function requireMockToken(label: string, value: unknown): string {
  const trimmed = requireMockField(label, value);
  const pattern = label === "Provider" ? /^[A-Za-z0-9_.-]+$/ : /^[A-Za-z0-9_-]+$/;
  if (!pattern.test(trimmed)) {
    throw new Error(label === "Provider"
      ? `${label} can only contain letters, numbers, '-', '_' and '.'`
      : `${label} can only contain letters, numbers, '-' and '_'`);
  }
  return trimmed;
}

function mockModePreviews(
  profile: ProfileDraft,
  configNativeDiff: PreviewProfileApplyResult["nativeDiff"],
  gatewayNativeDiff: PreviewProfileApplyResult["nativeDiff"]
): PreviewProfileApplyResult["modePreviews"] {
  const isCodexTool = isCodexFamilyApp(profile.app);
  const isOfficial = providerIsOfficial(profile.provider);
  const officialClientConfig = isOfficial && !isCodexTool;
  const configProtocolSupported = mockConfigProtocolSupported(profile);
  const configSupported = Boolean(configNativeDiff) || officialClientConfig;
  const configBlockedReason = !configProtocolSupported && !isOfficial
    ? `Config profiles do not support ${mockProtocolLabel(profile.protocol)} for '${profile.app}'.`
    : !configSupported && !isOfficial
    ? `Config profile adapter is not implemented for '${profile.app}'.`
    : !profile.authRef && providerRequiresApiKey(profile.provider)
      ? "Config profiles need a stored Provider API key for this Provider."
      : null;
  const gatewayWritesNativeConfig = Boolean(gatewayNativeDiff);
  const gatewaySupported = !isOfficial;

  return [
    {
      mode: "config",
      label: "Client config profile",
      description: "Back up and modify the target client's native provider config directly. This makes the client talk to the selected upstream Provider without CodeStudio Lite in the request path.",
      supported: configSupported && !configBlockedReason,
      recommended: isOfficial && configSupported && !configBlockedReason,
      writesNativeConfig: Boolean(configNativeDiff),
      startsGateway: false,
      blockedReason: configBlockedReason,
      nativeDiff: configNativeDiff,
      warnings: officialClientConfig
        ? [
            "Official provider uses the target client's own login.",
            "No Provider API key or model override is required."
          ]
        : configNativeDiff
        ? [
            "Config profiles write Provider connection details into the client config.",
            "Frequent Provider switching may require the client to reload its own config."
          ]
        : []
    },
    {
      mode: "gateway",
      label: "Gateway profile",
      description: gatewayWritesNativeConfig
        ? "Back up and point the client at the local CodeStudio Gateway once. This apply only switches the active Provider profile; start the Gateway from the sidebar when needed."
        : "Switch the active Provider profile for the local Gateway. This apply does not start the Gateway or modify this tool's native config.",
      supported: gatewaySupported,
      recommended: gatewaySupported,
      writesNativeConfig: gatewayWritesNativeConfig,
      startsGateway: false,
      blockedReason: isOfficial
        ? "Official provider uses the client login directly and does not run through the local gateway."
        : null,
      nativeDiff: gatewayNativeDiff,
      warnings: gatewayWritesNativeConfig
          ? [
            "The client still needs to reload config after the first gateway bootstrap.",
            "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.",
            "Real upstream Provider API keys stay in the system keychain and are used by the local gateway."
          ]
        : [
            `No native gateway bootstrap is written for '${profile.app}'; configure the client to use the Gateway URL manually or wait for a validated adapter.`,
            "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.",
            "Real upstream Provider API keys stay in the system keychain and are used by the local gateway."
          ]
    }
  ];
}

function validateBaseUrlCheck(baseUrl: string): TestProfileConnectionResult["checks"][number] {
  const trimmed = baseUrl.trim();
  if (!/^https?:\/\//i.test(trimmed)) {
    return {
      id: "base-url",
      label: "Base URL",
      status: "error",
      detail: "Base URL must start with http:// or https://"
    };
  }
  try {
    const parsed = new URL(trimmed);
    if (!["http:", "https:"].includes(parsed.protocol)) {
      return {
        id: "base-url",
        label: "Base URL",
        status: "error",
        detail: "Base URL must start with http:// or https://"
      };
    }
    if (!parsed.hostname) {
      return {
        id: "base-url",
        label: "Base URL",
        status: "error",
        detail: "Base URL must include a host."
      };
    }
    return {
      id: "base-url",
      label: "Base URL",
      status: "ok",
      detail: trimmed
    };
  } catch {
    return {
      id: "base-url",
      label: "Base URL",
      status: "error",
      detail: "Base URL is not a valid URL."
    };
  }
}

function validateBaseUrlCheckForProvider(
  provider: string,
  baseUrl: string
): TestProfileConnectionResult["checks"][number] {
  if (provider === "official" && !baseUrl.trim()) {
    return {
      id: "base-url",
      label: "Base URL",
      status: "info",
      detail: "Official provider uses the target client's own login and default endpoint."
    };
  }
  return validateBaseUrlCheck(baseUrl);
}

function validateBaseUrlOrThrow(baseUrl: string): void {
  const check = validateBaseUrlCheck(baseUrl);
  if (check.status === "error") {
    throw new Error(check.detail);
  }
}

function validateBaseUrlForProviderOrThrow(provider: string, baseUrl: string): void {
  if (providerIsOfficial(provider) && !baseUrl.trim()) {
    return;
  }
  validateBaseUrlOrThrow(baseUrl);
}

function normalizeMockProtocol(value?: string | null): string {
  const protocol = (value ?? "").trim();
  if (
    protocol === PROTOCOL_OPENAI_CHAT_COMPLETIONS ||
    protocol === PROTOCOL_OPENAI_RESPONSES ||
    protocol === PROTOCOL_ANTHROPIC_MESSAGES ||
    protocol === PROTOCOL_GOOGLE_GEMINI
  ) {
    return protocol;
  }
  throw new Error("Unsupported Provider API protocol.");
}

function mockProtocolLabel(value?: string | null): string {
  let protocol: string;
  try {
    protocol = normalizeMockProtocol(value);
  } catch {
    return value?.trim() || "Unknown protocol";
  }
  if (protocol === PROTOCOL_OPENAI_RESPONSES) {
    return "OpenAI Responses API";
  }
  if (protocol === PROTOCOL_ANTHROPIC_MESSAGES) {
    return "Claude Messages API";
  }
  if (protocol === PROTOCOL_GOOGLE_GEMINI) {
    return "Gemini API";
  }
  return "OpenAI Chat Completions";
}

function mockProfileModels(protocol: string): ListProfileModelsResult["models"] {
  if (protocol === PROTOCOL_ANTHROPIC_MESSAGES) {
    return [
      { id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", ownedBy: "anthropic", supports1m: true },
      { id: "claude-opus-4-8", name: "Claude Opus 4.8", ownedBy: "anthropic", supports1m: true },
      { id: "claude-haiku-4-5", name: "Claude Haiku 4.5", ownedBy: "anthropic", supports1m: true }
    ];
  }
  if (protocol === PROTOCOL_GOOGLE_GEMINI) {
    return [
      { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro", ownedBy: "google", supports1m: true },
      { id: "gemini-2.5-flash", name: "Gemini 2.5 Flash", ownedBy: "google", supports1m: true }
    ];
  }
  return [
    { id: "gpt-5", name: "GPT-5", ownedBy: "openai", supports1m: false },
    { id: "gpt-5-mini", name: "GPT-5 mini", ownedBy: "openai", supports1m: false },
    { id: "gpt-4.1", name: "GPT-4.1", ownedBy: "openai", supports1m: false }
  ];
}

function mockProfileSqlPreviewContent(input: {
  id: string;
  name: string;
  icon: string | null;
  remark: string | null;
  app: string;
  mode: ProviderApplyMode;
  provider: string;
  protocol: string;
  model: string;
  modelMappings: ProfileModelMapping[];
  baseUrl: string;
  authRef: string | null;
  timestamp: string;
  secretStatus: string;
}): string {
  return JSON.stringify(
    {
      table: "profiles",
      row: {
        id: input.id,
        name: input.name,
        icon: mockProfileIconPreview(input.icon),
        remark: input.remark,
        app: input.app,
        mode: input.mode,
        provider: input.provider,
        protocol: input.protocol,
        model: input.model,
        model_mappings: input.modelMappings,
        base_url: input.baseUrl,
        auth_ref: input.authRef,
        created_at: input.timestamp,
        updated_at: input.timestamp,
        last_test_status: "pending",
        secret_status: input.secretStatus
      },
      secrets: "API keys are stored in the system keychain and never written into SQLite."
    },
    null,
    2
  );
}

function mockProfileIconPreview(icon: string | null): string | null {
  if (!icon) {
    return null;
  }
  return icon.startsWith("data:image/") ? `image data url (${icon.length} bytes)` : icon;
}

function credentialStatus(provider: string, secretProvided: boolean): TestProfileConnectionResult["status"] {
  if (provider === "official") {
    return "info";
  }
  return secretProvided ? "ok" : "error";
}

function credentialDetail(provider: string, secretProvided: boolean): string {
  if (provider === "official") {
    return "Official login flow does not require an API key in this profile draft.";
  }
  return secretProvided
    ? "The Provider API key will be stored in the system keychain when this profile is saved; it is not written to TOML or logs."
    : "Provider API key is required for non-official providers.";
}

function providerIsOfficial(provider: string): boolean {
  return provider.trim() === "official";
}

function providerRequiresApiKey(provider: string): boolean {
  return !providerIsOfficial(provider);
}

function customOfficialProfileAllowed(app: string, provider: string, mode: ProviderApplyMode): boolean {
  return !providerIsOfficial(provider) || (isCodexFamilyApp(app) && mode === "config");
}

function ensureCustomOfficialProfileAllowed(app: string, provider: string, mode: ProviderApplyMode): void {
  if (!customOfficialProfileAllowed(app, provider, mode)) {
    throw new Error("Only Codex OAuth profiles can be saved as custom official profiles.");
  }
}

function defaultMockProfileMode(provider: string): ProviderApplyMode {
  return providerIsOfficial(provider) ? "config" : "gateway";
}

function normalizeMockProfileMode(
  provider: string,
  requested?: ProviderApplyMode | null
): ProviderApplyMode {
  const mode = requested ?? defaultMockProfileMode(provider);
  if (providerIsOfficial(provider) && mode === "gateway") {
    throw new Error("Official provider uses the client login directly and cannot use Gateway profile.");
  }
  return mode;
}

function isCodexFamilyApp(app: string): boolean {
  return canonicalProfileApp(app) === "codex";
}

function canonicalProfileApp(app: string): string {
  const normalized = app.trim().toLowerCase();
  if ([
    "codex",
    "codex-cli",
    "chatgpt-desktop",
    "codex-app",
    "codex-client",
    "codex-desktop",
    "codex-vscode",
    "codex-code-vscode",
    "codex-vs-code"
  ].includes(normalized)) {
    return "codex";
  }
  if (["claude-desktop", "claude-app", "claude-client"].includes(normalized)) {
    return "claude-desktop";
  }
  if (["claude-vscode", "claude-code-vscode", "claude-vs-code"].includes(normalized)) {
    return "claude";
  }
  if (["gemini-vscode", "gemini-code-vscode", "gemini-vs-code"].includes(normalized)) {
    return "gemini-code-assist";
  }
  if (normalized === "hermes-agent") {
    return "hermes";
  }
  return normalized;
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
    hermes: "Hermes"
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

function slugify(value: string): string {
  const slug = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "profile";
}

function uniqueMockProfileId(baseId: string): string {
  const normalized = baseId || "profile";
  for (let index = 0; index < 1000; index += 1) {
    const candidate = index === 0 ? normalized : `${normalized}-${index}`;
    if (!isBuiltinOfficialProfileId(candidate) && !mockProfileDrafts.some((profile) => profile.id === candidate)) {
      return candidate;
    }
  }
  return `${normalized}-${Date.now()}`;
}

function mockToolConfigPath(toolId: string): string | null {
  const canonicalToolId = canonicalProfileApp(toolId);
  const paths: Record<string, string> = {
    codex: "~/.codex/config.toml",
    claude: "~/.claude",
    "claude-desktop": mockClaudeDesktopProfilePath(),
    gemini: "~/.gemini",
    "gemini-code-assist": "~/AppData/Roaming/Code/User/settings.json",
    opencode: "~/.config/opencode",
    openclaw: "~/.openclaw",
    hermes: "~/.hermes/config.yaml"
  };
  return paths[canonicalToolId] ?? null;
}
