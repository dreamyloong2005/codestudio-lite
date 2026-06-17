import { invoke } from "@tauri-apps/api/core";
import type {
  ActivityEvent,
  ActiveProfilesByMode,
  AppSettings,
  ApplyProfileRequest,
  ApplyProfileResult,
  BackupManifest,
  ClearEnvironmentVariablesRequest,
  ClearEnvironmentVariablesResult,
  CodexClientInstallRequest,
  CodexClientOperationResult,
  CodexClientProgress,
  CodexClientSettings,
  CodexClientStageReport,
  CodexClientState,
  CodexClientUninstallRequest,
  DetectionSnapshot,
  DoctorReport,
  DuplicateProfileDraftRequest,
  ExportProfilesResult,
  GatewayControlResult,
  GatewayRequestLogEntry,
  GatewayStatus,
  ImportProfilesRequest,
  ImportProfilesResult,
  PreviewProfileApplyRequest,
  PreviewProfileApplyResult,
  PreviewProfileWriteRequest,
  PreviewProfileWriteResult,
  ProfileDraft,
  ProviderApplyMode,
  RepairToolPathRequest,
  RepairToolPathResult,
  ProfileSummary,
  RestoreBackupRequest,
  RestoreBackupResult,
  SaveProfileDraftRequest,
  TestProfileConnectionRequest,
  TestProfileConnectionResult,
  ToolInstallPlan,
  ToolInstallRequest,
  ToolInstallResult,
  UpdateCodexClientSettingsRequest,
  ToolStatus,
  UpdateAppSettingsRequest,
  UpdateProfileDraftRequest
} from "../types";

const isTauri = () => Boolean(window.__TAURI_INTERNALS__);
const codexClientProgressListeners = new Set<(progress: CodexClientProgress) => void>();
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

export async function detectEnvironment(): Promise<DetectionSnapshot> {
  if (isTauri()) {
    return invoke("detect_environment");
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

export async function planToolInstall(toolId: string): Promise<ToolInstallPlan> {
  if (isTauri()) {
    return invoke("plan_tool_install", { toolId });
  }
  return mockToolInstallPlan(toolId);
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
      notes: plan.warnings
    };
  }
  if (plan.requiresPrerequisites && !request.installPrerequisites) {
    return {
      success: false,
      toolId: plan.toolId,
      toolName: plan.toolName,
      action: "prerequisites-required",
      message: "安装此工具前需要安装前置依赖，请勾选允许安装前置后再继续。",
      command: plan.command,
      exitCode: null,
      stdoutTail: "",
      stderrTail: "",
      currentStatus: mockFindToolStatus(plan.toolId),
      stageResults: [],
      notes: plan.warnings
    };
  }

  await new Promise((resolve) => window.setTimeout(resolve, 800));
  const stageResults: ToolInstallResult["stageResults"] = [];
  for (const prerequisite of plan.prerequisites) {
    if (prerequisite.installed) {
      continue;
    }
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
      message: `${prerequisite.toolName} 前置依赖安装完成。`
    });
  }
  markMockToolUpdated(request.toolId);
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
      ? `${plan.toolName} 安装完成并通过复检。`
      : `${plan.toolName} 安装命令已结束，但复检仍未确认可用。`
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
      ? `${plan.toolName} 安装完成并通过复检。`
      : `${plan.toolName} 安装命令已结束，但复检仍未确认可用。`,
    command: plan.command,
    exitCode: 0,
    stdoutTail: stageResults.map((stage) => stage.stdoutTail).filter(Boolean).join("\n"),
    stderrTail: "",
    currentStatus,
    stageResults,
    notes: plan.warnings
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
      message: `${status?.name ?? request.toolId} 未安装，无法更新。`,
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
      message: `${status.name} 当前没有检测到可用更新。`,
      command: status.updateCommand ?? "",
      exitCode: null,
      stdoutTail: "",
      stderrTail: "",
      currentStatus: status,
      stageResults: [],
      notes: []
    };
  }

  await new Promise((resolve) => window.setTimeout(resolve, 800));
  markMockToolUpdated(request.toolId);
  const currentStatus = mockFindToolStatus(request.toolId);
  writeMockDetectionCache(mockDetection());
  const message = `${status.name} 更新命令已完成并通过复检。`;
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
      message: "没有可修复的 PATH 候选。",
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
    message: `已把 ${status.pathRepair.directory} 加入用户 PATH。`,
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
    message: "Claude 全局环境变量已清理。",
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
    language: request.language ?? mockSettings.language,
    preserveCodexOfficialAuth:
      request.preserveCodexOfficialAuth ?? mockSettings.preserveCodexOfficialAuth
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
    changedFiles: ["~/.codestudio-lite/config.toml"],
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

export async function inspectCodexClient(): Promise<CodexClientState> {
  if (isTauri()) {
    return invoke("inspect_codex_client");
  }
  return mockCodexClientState(false);
}

export async function planCodexClientUpdate(): Promise<CodexClientState> {
  if (isTauri()) {
    return invoke("plan_codex_client_update");
  }
  return mockCodexClientState(true);
}

export async function stageCodexClientUpdate(): Promise<CodexClientStageReport> {
  if (isTauri()) {
    return invoke("stage_codex_client_update");
  }
  await simulateCodexClientProgress([
    { phase: "preparing", message: "正在读取镜像 manifest 与 checksums...", downloaded: null, total: null, percent: null, step: 1, stepTotal: 4 },
    { phase: "downloading", message: "正在下载安装包...", downloaded: 46000000, total: 552187367, percent: 8.3, step: 2, stepTotal: 4 },
    { phase: "downloading", message: "正在下载安装包...", downloaded: 178000000, total: 552187367, percent: 32.2, step: 2, stepTotal: 4 },
    { phase: "downloading", message: "正在下载安装包...", downloaded: 394000000, total: 552187367, percent: 71.3, step: 2, stepTotal: 4 },
    { phase: "verifying", message: "正在校验安装包 SHA-256...", downloaded: null, total: null, percent: null, step: 3, stepTotal: 4 },
    { phase: "done", message: "安装包已下载并通过 SHA-256 校验。", downloaded: 552187367, total: 552187367, percent: 100, step: 4, stepTotal: 4 }
  ]);
  return mockCodexClientStageReport();
}

export async function installCodexClient(
  request: CodexClientInstallRequest
): Promise<CodexClientOperationResult> {
  if (isTauri()) {
    return invoke("install_codex_client", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  await simulateCodexClientProgress([
    { phase: "preparing", message: "正在确认安装状态与更新计划...", downloaded: null, total: null, percent: null, step: 1, stepTotal: 7 },
    { phase: "downloading", message: "正在下载安装包...", downloaded: 220000000, total: 552187367, percent: 39.8, step: 2, stepTotal: 7 },
    { phase: "downloading", message: "正在下载安装包...", downloaded: 552187367, total: 552187367, percent: 100, step: 2, stepTotal: 7 },
    { phase: "verifying", message: "正在校验安装包 SHA-256...", downloaded: null, total: null, percent: null, step: 3, stepTotal: 7 },
    { phase: "extracting", message: "正在解包 MSIX 安装包...", downloaded: 38, total: 120, percent: 31.7, step: 4, stepTotal: 7 },
    { phase: "copying", message: "正在复制便携版文件...", downloaded: null, total: null, percent: null, step: 5, stepTotal: 7 },
    { phase: "writing", message: "正在写入安装目录...", downloaded: null, total: null, percent: null, step: 6, stepTotal: 7 },
    { phase: "finalizing", message: "正在创建快捷方式与卸载项...", downloaded: null, total: null, percent: null, step: 6, stepTotal: 7 },
    { phase: "done", message: "Codex 客户端安装流程已完成。", downloaded: 1, total: 1, percent: 100, step: 7, stepTotal: 7 }
  ]);
  mockCodexClientInstalled = {
    path: mockCodexClientSettings.windowsInstallMode === "portable"
      ? mockCodexClientSettings.installRoot
      : "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.609.4994.0_x64__2p2nqsd0c76g0",
    version: "26.609.4994.0",
    arch: "x64",
    source: mockCodexClientSettings.windowsInstallMode === "portable" ? "portable" : "msix",
    packageFamilyName: mockCodexClientSettings.windowsInstallMode === "portable"
      ? null
      : "OpenAI.Codex_2p2nqsd0c76g0",
    installedAt: new Date().toISOString()
  };
  return {
    success: true,
    action: mockCodexClientInstalled.source === "portable" ? "portable-fallback" : "msix-sideload",
    message: `Codex 客户端已就绪：${mockCodexClientInstalled.version}`,
    installed: mockCodexClientInstalled,
    stage: mockCodexClientStageReport(),
    notes: ["browser-dev mock: install path is simulated."]
  };
}

export async function uninstallCodexClient(
  request: CodexClientUninstallRequest
): Promise<CodexClientOperationResult> {
  if (isTauri()) {
    return invoke("uninstall_codex_client", { request });
  }
  if (!request.confirm) {
    throw new Error("explicit confirmation is required");
  }
  mockCodexClientInstalled = null;
  return {
    success: true,
    action: "remove-portable",
    message: "Codex 客户端卸载完成。",
    installed: null,
    stage: null,
    notes: [request.purgeUserData ? "已删除 ~/.codex 用户数据。" : "已保留 ~/.codex 用户数据。"]
  };
}

export async function launchCodexClient(): Promise<void> {
  if (isTauri()) {
    return invoke("launch_codex_client");
  }
}

export async function updateCodexClientSettings(
  request: UpdateCodexClientSettingsRequest
): Promise<CodexClientSettings> {
  if (isTauri()) {
    return invoke("update_codex_client_settings", { request });
  }
  mockCodexClientSettings = {
    ...mockCodexClientSettings,
    source: "mirror",
    customUrl: "",
    autoCheck: request.autoCheck ?? mockCodexClientSettings.autoCheck,
    askBefore: request.askBefore ?? mockCodexClientSettings.askBefore,
    windowsInstallMode: request.windowsInstallMode ?? mockCodexClientSettings.windowsInstallMode,
    installRoot: request.installRoot ?? mockCodexClientSettings.installRoot,
    keepUserDataOnUninstall: request.keepUserDataOnUninstall ?? mockCodexClientSettings.keepUserDataOnUninstall,
    signedOnly: true
  };
  return mockCodexClientSettings;
}

export async function openCodexClientPath(kind: "install" | "staging" | "config"): Promise<void> {
  if (isTauri()) {
    return invoke("open_codex_client_path", { kind });
  }
}

export async function listenCodexClientProgress(
  handler: (progress: CodexClientProgress) => void
): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<CodexClientProgress>("codex-client://progress", (event) => handler(event.payload));
  }
  codexClientProgressListeners.add(handler);
  return () => codexClientProgressListeners.delete(handler);
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
    detail: `Network provider checks are not sent yet. Timeout is set to ${request.timeoutSeconds ?? 120}s.`
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

export async function saveProfileDraft(request: SaveProfileDraftRequest): Promise<ProfileDraft> {
  if (isTauri()) {
    return invoke("save_profile_draft", { request });
  }

  if (providerIsOfficial(request.provider)) {
    throw new Error("Official profiles are built in and cannot be saved as custom profiles.");
  }
  if (providerRequiresApiKey(request.provider) && !request.secretProvided) {
    throw new Error("Provider API key is required for non-official providers.");
  }
  validateBaseUrlOrThrow(request.baseUrl);

  const app = canonicalProfileApp(request.app);
  const mode = normalizeMockProfileMode(request.provider, request.mode);
  const protocol = normalizeMockProtocol(request.protocol);
  ensureMockProfileProtocolSupported(app, mode, request.provider, protocol);
  const profileId = uniqueMockProfileId(slugify(request.name));
  const now = new Date().toISOString();
  const profile: ProfileDraft = {
    id: profileId,
    name: request.name.trim(),
    app,
    isBuiltin: false,
    mode,
    provider: request.provider,
    protocol,
    model: request.model.trim(),
    baseUrl: request.baseUrl.trim(),
    authRef: request.secretProvided ? `keychain:codestudio-lite/${profileId}/api_key` : null,
    timeoutSeconds: request.timeoutSeconds ?? 120,
    createdAt: now,
    updatedAt: now,
    lastTestStatus: "pending"
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
  if (providerIsOfficial(request.provider)) {
    throw new Error("Official profiles are built in and cannot be saved as custom profiles.");
  }
  const existing = mockProfileDrafts[index];
  const mode = normalizeMockProfileMode(request.provider, request.mode ?? existing.mode);
  const protocol = normalizeMockProtocol(request.protocol ?? existing.protocol);
  const app = canonicalProfileApp(existing.app);
  ensureMockProfileProtocolSupported(app, mode, request.provider, protocol);
  validateBaseUrlOrThrow(request.baseUrl);
  if (providerRequiresApiKey(request.provider) && !existing.authRef && !request.apiKey?.trim()) {
    throw new Error("Provider API key is required for non-official providers.");
  }

  const hasNewSecret = Boolean(request.apiKey?.trim());
  const updated: ProfileDraft = {
    ...existing,
    name: request.name.trim(),
    app,
    mode,
    provider: request.provider.trim(),
    protocol,
    model: request.model.trim(),
    baseUrl: request.baseUrl.trim(),
    authRef: hasNewSecret ? existing.authRef ?? `keychain:codestudio-lite/${existing.id}/api_key` : existing.authRef,
    timeoutSeconds: request.timeoutSeconds ?? 120,
    updatedAt: new Date().toISOString(),
    lastTestStatus: "pending"
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
    updatedAt: now
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

export async function exportProfiles(): Promise<ExportProfilesResult> {
  if (isTauri()) {
    return invoke("export_profiles");
  }

  const exportedAt = new Date().toISOString();
  return {
    fileName: `codestudio-lite-profiles-${exportedAt.replace(/[:.]/g, "-")}.json`,
    bundle: {
      schemaVersion: 2,
      app: "CodeStudio Lite",
      exportedAt,
      activeProfilesByMode: cloneMockActiveProfilesByMode(),
      profiles: mockProfileDrafts.map((profile) => ({ ...profile, authRef: null })),
      warnings: [
        "Provider API keys are not exported. Imported profiles need their API key saved again before direct config file mode can use them.",
        "Importing profiles does not automatically enable them for any tool."
      ]
    }
  };
}

export async function importProfiles(request: ImportProfilesRequest): Promise<ImportProfilesResult> {
  if (isTauri()) {
    return invoke("import_profiles", { request });
  }

  const importedProfiles = parseMockImportProfiles(request.content);
  const now = new Date().toISOString();
  const imported: ProfileDraft[] = [];
  const skipped: string[] = [];

  importedProfiles.forEach((profile, index) => {
    try {
      const name = requireMockField("Profile Name", profile.name);
      const app = canonicalProfileApp(requireMockToken("Client", profile.app));
      const provider = requireMockToken("Provider", profile.provider);
      if (profile.isBuiltin) {
        throw new Error("Built-in official profiles cannot be imported.");
      }
      if (providerIsOfficial(provider)) {
        throw new Error("Official profiles are built in and cannot be imported.");
      }
      const mode = normalizeMockProfileMode(provider, profile.mode);
      const protocol = normalizeMockProtocol(profile.protocol);
      ensureMockProfileProtocolSupported(app, mode, provider, protocol);
      const baseUrl = requireMockField("Base URL", profile.baseUrl);
      validateBaseUrlOrThrow(baseUrl);
      const preferredId = slugify(profile.id || name);
      if (isBuiltinOfficialProfileId(preferredId)) {
        throw new Error("Built-in official profile IDs are reserved.");
      }
      const id = uniqueMockProfileId(preferredId);
      imported.push({
        id,
        name,
        app,
        isBuiltin: false,
        mode,
        provider,
        protocol,
        model: typeof profile.model === "string" ? profile.model.trim() : "",
        baseUrl,
        authRef: null,
        timeoutSeconds: normalizeMockTimeout(profile.timeoutSeconds),
        createdAt: profile.createdAt || now,
        updatedAt: now,
        lastTestStatus: "pending"
      });
    } catch (err) {
      skipped.push(`${profile.name?.trim() || `profile #${index + 1}`}: ${err instanceof Error ? err.message : String(err)}`);
    }
  });

  mockProfileDrafts = [...mockProfileDrafts, ...imported];
  mockActivity = [
    {
      id: `mock-import-${Date.now()}`,
      level: imported.length > 0 ? "ok" : "warning",
      message: `Imported ${imported.length} profile draft(s); skipped ${skipped.length}.`,
      createdAt: new Date().toISOString()
    },
    ...mockActivity
  ];

  return {
    imported,
    skipped,
    summary: mockProfiles()
  };
}

export async function previewProfileWrite(
  request: PreviewProfileWriteRequest
): Promise<PreviewProfileWriteResult> {
  if (isTauri()) {
    return invoke("preview_profile_write", { request });
  }

  const app = canonicalProfileApp(request.app);
  const profileId = uniqueMockProfileId(slugify(request.name));
  const profilePath = `~/.codestudio-lite/profiles/${profileId}.toml`;
  const tool = mockDetection().tools.find((item) => item.id === app);
  const targetToolPath = tool?.configPath ?? mockToolConfigPath(app);
  const warnings: string[] = [];

  if (!request.name.trim()) {
    throw new Error("Profile Name is required");
  }
  if (providerIsOfficial(request.provider)) {
    throw new Error("Official profiles are built in and cannot be saved as custom profiles.");
  }
  if (providerRequiresApiKey(request.provider) && !request.secretProvided) {
    throw new Error("Provider API key is required for non-official providers.");
  }
  validateBaseUrlOrThrow(request.baseUrl);
  const mode = normalizeMockProfileMode(request.provider, request.mode);
  const protocol = normalizeMockProtocol(request.protocol);
  ensureMockProfileProtocolSupported(app, mode, request.provider, protocol);

  if (profileId !== slugify(request.name)) {
    warnings.push(`Profile id '${slugify(request.name)}' already exists, so this draft will use '${profileId}'.`);
  }
  if (!tool) {
    warnings.push(`Tool '${app}' is not in the preview registry.`);
  }
  const generatedAt = new Date().toISOString();
  const profileContent = mockProfileTomlContent({
    id: profileId,
    name: request.name.trim(),
    app,
    mode,
    provider: request.provider.trim(),
    protocol,
    model: request.model.trim(),
    baseUrl: request.baseUrl.trim(),
    authRef: request.secretProvided ? `keychain:codestudio-lite/${profileId}/api_key` : null,
    timeoutSeconds: normalizeMockTimeout(request.timeoutSeconds),
    timestamp: generatedAt,
    secretStatus: request.secretProvided ? "pending_keychain" : "missing"
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
        label: "Profile draft",
        path: profilePath,
        action: "create",
        backupRequired: false,
        detail: `Save Profile Draft writes normalized metadata for ${mockProtocolLabel(protocol)}/${request.provider} and excludes API keys.`,
        content: profileContent
      },
      {
        label: "Active tool profile pointer",
        path: "~/.codestudio-lite/config.toml",
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
  const appliedPath = `~/.codestudio-lite/applied/${profile.app}-active.toml`;
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
        path: "~/.codestudio-lite/config.toml",
        action: "update",
        backupRequired: true,
        detail: `Sets CodeStudio Lite active profile for '${profile.app}' to '${profile.id}' before refreshing detection.`
      },
      {
        label: "Managed tool binding",
        path: appliedPath,
        action: "create_or_update",
        backupRequired: true,
        detail: `Writes CodeStudio-managed adapter metadata for ${profile.app}/${profile.provider}. API keys are not written.`
      },
      {
        label: `${tool?.name ?? "Target tool"} native config`,
        path: nativeConfigPath,
        action: nativeDiff ? "create_or_update" : "not_modified",
        backupRequired: Boolean(nativeDiff),
        detail: nativeDiff
          ? "Selected mode writes this client config; detailed file changes are shown below."
          : "This profile does not require a native client config write."
      },
      {
        label: "Credential",
        path: null,
        action: "not_written",
        backupRequired: false,
        detail: "CodeStudio Lite profile metadata never stores plaintext API keys. Config file mode may write the selected Provider key into the target client's native config."
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
    throw new Error("Profile is already active for this tool and mode.");
  }
  const preview = await previewProfileApply(request);
  if (!preview.canApply) {
    throw new Error(`Profile '${request.profileId}' cannot be applied yet.`);
  }
  const mode = profile.mode;
  if (request.restartAfterApply && mode !== "config") {
    throw new Error("Apply and restart is only available for Config file mode.");
  }
  const syncClaudeVsCode =
    Boolean(request.syncClaudeVsCode) && mode === "config" && canonicalProfileApp(profile.app) === "claude";
  const selectedModePreview = preview.modePreviews.find((item) => item.mode === mode);
  if (!selectedModePreview?.supported) {
    throw new Error(selectedModePreview?.blockedReason ?? `${mode} mode is not supported for this profile.`);
  }
  if (request.restartAfterApply && !selectedModePreview.writesNativeConfig) {
    throw new Error("Apply and restart requires a native client config write for this profile.");
  }
  const backupId = new Date().toISOString().replaceAll(":", "-");
  const appliedPath = `~/.codestudio-lite/applied/${profile.app}-active.toml`;
  const nativePath = selectedModePreview.writesNativeConfig ? selectedModePreview.nativeDiff?.path ?? null : null;
  const restartMessage = request.restartAfterApply ? mockRestartMessageForProfile(profile) : null;
  mockBackupSnapshots[backupId] = cloneMockActiveProfilesByMode();
  mockActiveProfilesByMode = setMockActiveProfileForMode(mode, profile);
  const backup: BackupManifest = {
    id: backupId,
    reason: "apply-profile",
    profile: profile.id,
    changedFiles: [
      "~/.codestudio-lite/config.toml",
      appliedPath,
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
        ? `Applied profile '${profile.name}' for ${profile.app}/${profile.provider} in Gateway mode.`
        : `Applied profile '${profile.name}' for ${profile.app}/${profile.provider} through direct client config file mode.`,
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

let mockSettings: AppSettings = {
  theme: "system",
  language: "zh-CN",
  backupBeforeWrite: true,
  redactSecrets: true,
  confirmInstallCommands: true,
  confirmConfigWrites: true,
  preserveCodexOfficialAuth: true
};

let mockGatewayRunning = false;

let mockGatewayStartedAt: string | null = null;

let mockCodexClientSettings: CodexClientSettings = {
  source: "mirror",
  customUrl: "",
  autoCheck: true,
  askBefore: true,
  signedOnly: true,
  windowsInstallMode: "msix",
  installRoot: "C:\\Users\\you\\AppData\\Local\\Programs\\Codex",
  keepUserDataOnUninstall: true
};

let mockCodexClientInstalled: CodexClientState["installed"] = null;

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
    message: "ANTHROPIC_BASE_URL 会影响 Claude API 连接，且与当前 CodeStudio 配置不一致。"
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

const builtinOfficialProfileDefinitions = [
  ["codex", "Codex 官方", PROTOCOL_OPENAI_RESPONSES],
  ["claude-desktop", "Claude Desktop 官方", PROTOCOL_ANTHROPIC_MESSAGES],
  ["claude", "Claude Code 官方", PROTOCOL_ANTHROPIC_MESSAGES],
  ["gemini", "Gemini CLI 官方", PROTOCOL_GOOGLE_GEMINI],
  ["gemini-code-assist", "Gemini Code Assist 官方", PROTOCOL_GOOGLE_GEMINI],
  ["opencode", "OpenCode 官方", PROTOCOL_OPENAI_CHAT_COMPLETIONS],
  ["openclaw", "OpenClaw 官方", PROTOCOL_OPENAI_CHAT_COMPLETIONS],
  ["hermes", "Hermes 官方", PROTOCOL_OPENAI_CHAT_COMPLETIONS]
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
    app,
    isBuiltin: true,
    mode: "config",
    provider: "official",
    protocol,
    model: "",
    baseUrl: "",
    authRef: null,
    timeoutSeconds: 120,
    createdAt: null,
    updatedAt: null,
    lastTestStatus: "builtin"
  }));
}

function mockAllProfiles(): ProfileDraft[] {
  return [...builtinOfficialProfiles(), ...mockProfileDrafts];
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
    errorSummary: null
  }
];

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
    installCommand: null,
    details: null,
    ...overrides
  };
}

function mockToolUpdateFields(toolId: string): Pick<ToolStatus, "latestVersion" | "updateAvailable" | "updateCommand"> {
  const update = mockToolUpdates[toolId];
  if (!update) {
    return { latestVersion: null, updateAvailable: false, updateCommand: null };
  }
  const installed = toolId === "codex-app" ? Boolean(mockCodexClientInstalled) : mockInstalledToolIds.has(toolId);
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
      envConflicts: snapshot.envConflicts ?? []
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
      id: "codex-app",
      name: "Codex 客户端",
      command: "Codex.exe",
      version: mockCodexClientInstalled?.version ?? null,
      installState: mockCodexClientInstalled ? "installed" : "missing",
      configState: "configured",
      configPath: "~/.codex",
      installCommand: "在 Codex 客户端页面中安装或更新",
      details: mockCodexClientInstalled
        ? `${mockCodexClientInstalled.source} / ${mockCodexClientInstalled.path}`
        : "未检测到官方 Codex 桌面客户端",
      ...mockToolUpdateFields("codex-app")
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
            message: "已在常见安装目录发现 pnpm.cmd，但当前 PATH 无法直接解析命令 pnpm。"
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
    homeDir: "~",
    appConfigDir,
    activeProfile: mockDefaultActiveProfileId(),
    activeProfileName: mockActiveProfileName(),
    codexAuth: mockCodexAuthStatus,
    tools: mockVisibleTools(tools),
    system,
    envConflicts: mockClaudeEnvConflicts,
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
  const definitions: Record<string, { toolName: string; manager: string; command: string; dependency?: string }> = {
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
      manager: "powershell",
      command: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm https://hermes-agent.nousresearch.com/install.ps1)\""
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
    npm: { toolName: "npm", manager: "dependency", command: "由 Node.js LTS 提供", dependency: "Node.js LTS" },
    pnpm: { toolName: "pnpm", manager: "npm", command: "npm install -g pnpm" },
    bun: {
      toolName: "Bun",
      manager: "winget",
      command: "winget install --id Oven-sh.Bun --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
    }
  };
  const definition = definitions[toolId];
  if (!definition) {
    throw new Error(`工具 '${toolId}' 不在安装白名单中。`);
  }
  const alreadyInstalled = status?.installState === "installed";
  const missingDependency = definition.manager === "npm" && !mockInstalledToolIds.has("npm");
  const blocker = alreadyInstalled
    ? `${definition.toolName} 已安装，无需重复安装。`
    : definition.dependency
      ? `${definition.toolName} 由 ${definition.dependency} 提供，请安装 ${definition.dependency}。`
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
          reason: "目标工具需要 npm；npm 随 Node.js LTS 提供。"
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
        requiresAdmin: true
      })),
    {
      toolId,
      toolName: definition.toolName,
      stage: "target",
      manager: definition.manager,
      command: definition.command,
      requiresAdmin: definition.manager === "winget"
    }
  ];
  const canInstall = !alreadyInstalled && !blocker;

  return {
    toolId,
    toolName: definition.toolName,
    manager: definition.manager,
    command: commands.map((item) => item.command).join(" && "),
    commands,
    prerequisites,
    requiresPrerequisites: prerequisites.some((prerequisite) => !prerequisite.installed),
    canInstall,
    alreadyInstalled,
    requiresAdmin: definition.manager === "winget" || prerequisites.some((prerequisite) => !prerequisite.installed),
    steps: buildMockInstallSteps(definition, status?.command ?? toolId),
    warnings: definition.manager === "winget"
      ? ["部分 winget 包可能触发系统安装权限提示；CodeStudio Lite 不会绕过系统确认。"]
      : definition.manager === "npm"
        ? [
            ...(missingDependency
              ? ["此计划包含前置依赖安装：Node.js LTS 会先安装，随后再安装目标 npm 包。"]
              : []),
            "全局 npm 安装会写入当前用户或当前 npm 前缀目录，完成后可能需要重新打开终端。"
          ]
        : definition.manager === "powershell"
          ? ["此计划会运行目标工具官方发布的 PowerShell 安装脚本；请只在信任该工具来源时确认。"]
        : definition.manager === "vscode"
          ? ["VS Code 扩展会安装到当前用户的 VS Code 配置中，完成后可能需要重启 VS Code。"]
        : [],
    blocker
  };
}

function buildMockInstallSteps(
  definition: { toolName: string; manager: string; command: string; dependency?: string },
  commandName: string
): ToolInstallPlan["steps"] {
  if (definition.dependency) {
    return [
      {
        label: "安装上游依赖",
        detail: `${definition.toolName} 没有独立安装包。`
      }
    ];
  }
  if (definition.manager === "winget") {
    return [
      { label: "检查 winget", detail: "需要 Windows App Installer / winget 可用。" },
      { label: "安装软件包", detail: `执行 ${definition.command}。` },
      { label: "复检命令", detail: `安装后运行 ${commandName} --version 并刷新仪表盘。` }
    ];
  }
  if (definition.manager === "powershell") {
    return [
      { label: "检查 PowerShell", detail: "需要本机 PowerShell 可用。" },
      { label: "运行官方安装脚本", detail: `执行 ${definition.command}。` },
      { label: "复检命令", detail: `安装后运行 ${commandName} --version 并刷新仪表盘。` }
    ];
  }
  if (definition.manager === "vscode") {
    return [
      { label: "检查 VS Code CLI", detail: "需要本机 code 命令可用。" },
      { label: "安装 VS Code 扩展", detail: `执行 ${definition.command}。` },
      { label: "复检扩展", detail: "安装后运行 code --list-extensions --show-versions 并刷新仪表盘。" }
    ];
  }
  const steps = [
    { label: "检查 npm", detail: "需要本机 npm 可用；npm 通常随 Node.js LTS 一起安装。" },
    { label: "安装全局包", detail: `执行 ${definition.command}。` },
    { label: "复检命令", detail: `安装后运行 ${commandName} --version 并刷新仪表盘。` }
  ];
  if (!mockInstalledToolIds.has("npm")) {
    steps.unshift({
      label: "安装前置依赖",
      detail: "检测到 npm 不可用；允许后会先通过 winget 安装 Node.js LTS。"
    });
  }
  return steps;
}

function mockCodexClientState(includeNetwork: boolean, installClass = mockCodexClientInstalled ? "managed" : "none"): CodexClientState {
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
  const upToDate = mockCodexClientInstalled?.version === release.version;
  return {
    generatedAt: new Date().toISOString(),
    platform: "windows",
    settings: mockCodexClientSettings,
    installed: mockCodexClientInstalled,
    installClass,
    release: includeNetwork ? release : null,
    plan: includeNetwork
      ? {
          upToDate,
          currentVersion: mockCodexClientInstalled?.version ?? null,
          latestVersion: release.version,
          route: mockCodexClientSettings.windowsInstallMode === "portable" ? "portable-fallback" : "msix-sideload",
          packageUrl: release.packageUrl,
          downloadSize: release.contentLength,
          sha256: release.sha256,
          stagedPath: null,
          installRoot: mockCodexClientSettings.installRoot,
          warnings: mockCodexClientSettings.windowsInstallMode === "portable"
            ? ["当前计划会安装便携版，并在开始菜单与卸载项中登记。"]
            : [],
          capabilities: [
            {
              id: "add-appx",
              label: "Add-AppxPackage",
              status: "ok",
              detail: "MSIX 安装命令可用。"
            },
            {
              id: "msix-runtime",
              label: "MSIX 运行时",
              status: "ok",
              detail: "Windows PackageManager 可激活。"
            }
          ]
        }
      : null,
    stagingDir: "~/.codestudio-lite/downloads/codex-client",
    notes: [
      "Codex 客户端管理复刻 Codex-App-Manager 的安装、更新、卸载、启动和镜像源流程。",
      "不会修改 Codex 安装包内容；下载后先做 SHA-256 校验，再进入安装步骤。"
    ]
  };
}

function mockCodexClientStageReport(): CodexClientStageReport {
  return {
    upToDate: false,
    stagedPath: "~/.codestudio-lite/downloads/codex-client/OpenAI.Codex_26.609.4994.0_x64__2p2nqsd0c76g0.Msix",
    packageMoniker: "OpenAI.Codex_26.609.4994.0_x64__2p2nqsd0c76g0",
    downloadSize: 552187367,
    sha256: "547618a744149221078a27febdfff65c924b46ff85ab2fe1595180e128be8d85",
    hashVerified: true,
    route: mockCodexClientSettings.windowsInstallMode === "portable" ? "portable-fallback" : "msix-sideload",
    notes: ["安装包已下载并通过 SHA-256 校验。"]
  };
}

async function simulateCodexClientProgress(steps: CodexClientProgress[]) {
  for (const step of steps) {
    codexClientProgressListeners.forEach((listener) => listener(step));
    await new Promise((resolve) => window.setTimeout(resolve, 160));
  }
}

function mockProfiles(): ProfileSummary {
  return {
    configDir: "~/.codestudio-lite",
    profilesDir: "~/.codestudio-lite/profiles",
    backupsDir: "~/.codestudio-lite/backups",
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
  const activeProfileId = activeProfiles[app] ?? (app === "codex" ? activeProfiles["codex-app"] : undefined);
  return activeProfileId === profile.id;
}

function mockDefaultActiveProfile(): ProfileDraft | null {
  const activeProfiles = mockActiveProfilesByMode.gateway;
  const preferredApps = ["codex", "claude-desktop", "claude", "gemini", "gemini-code-assist", "opencode", "openclaw", "hermes"];
  for (const app of preferredApps) {
    const profileId = activeProfiles[app] ?? (app === "codex" ? activeProfiles["codex-app"] : undefined);
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
    return mockNonCodexNativeConfigPreview(profile, nativeConfigPath, mode);
  }

  if (mode === "config") {
    const wireApi = mockCodexWireApi(profile.protocol);
    if (!wireApi) {
      return null;
    }
    if (profile.provider === "official") {
      return {
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
            key: "model_providers.openai.wire_api",
            action: "update",
            before: "responses",
            after: wireApi,
            detail: "Uses Codex's selected provider wire API."
          },
          {
            key: "model_providers.openai.requires_openai_auth",
            action: "add",
            before: null,
            after: "true",
            detail: "Keeps Codex official login as the authentication source."
          },
          {
            key: "model_providers.openai.experimental_bearer_token",
            action: "remove",
            before: "<redacted>",
            after: null,
            detail: "Official login does not require a Provider API key."
          }
        ],
        warnings: [
          "Official provider uses the target client's own login.",
          "No Provider API key or model override is required."
        ]
      };
    }

    const providerId = `codestudio-${slugify(profile.provider)}`;
    return {
      tool: "codex",
      path: nativeConfigPath ?? "~/.codex/config.toml",
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "model_provider",
          action: "update",
          before: "custom",
          after: providerId,
          detail: "Selects the direct provider entry managed by CodeStudio Lite."
        },
        {
          key: "model",
          action: "update",
          before: "gpt-5-codex",
          after: profile.model || "codestudio-default",
          detail: "Sets Codex to the selected upstream model."
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
          after: "false",
          detail: "Disables Codex official OpenAI auth for this custom upstream entry."
        },
        {
          key: `model_providers.${providerId}.experimental_bearer_token`,
          action: "add",
          before: null,
          after: profile.authRef ? "keychain:****" : "(missing keychain secret)",
          detail: "Stores the selected Provider API key from the system keychain."
        }
      ],
      warnings: [
        "Config file mode writes Codex's provider entry directly to the selected upstream Provider.",
        "The preview masks the Provider API key. The actual key is loaded from the system keychain during apply.",
        "Changing Codex config usually requires restarting Codex or opening a new Codex session."
      ]
    };
  }

  const gatewayBaseUrl = mockGatewayBaseUrlForTool(profile.app);
  return {
    tool: "codex",
    path: nativeConfigPath ?? "~/.codex/config.toml",
    status: "preview",
    writeEnabled: true,
    changes: [
      {
        key: "model_provider",
        action: "update",
        before: "custom",
        after: "codestudio-local",
        detail: "Selects the CodeStudio Lite localhost provider."
      },
      {
        key: "model",
        action: "update",
        before: "gpt-5-codex",
        after: "codestudio-default",
        detail: "Sets Codex to the virtual model name resolved by the Local Gateway."
      },
      {
        key: "model_providers.codestudio-local.base_url",
        action: "add",
        before: null,
        after: gatewayBaseUrl,
        detail: "Points Codex at the tool-scoped CodeStudio Lite Local Gateway."
      },
      {
        key: "model_providers.codestudio-local.requires_openai_auth",
        action: "add",
        before: null,
        after: "true",
        detail: "Keeps the official Codex login path available while routing model requests through the Local Gateway."
      },
      {
        key: "model_providers.codestudio-local.experimental_bearer_token",
        action: "add",
        before: null,
        after: "codestudio-local-****7f3a2c",
        detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
      }
    ],
    warnings: [
      "Gateway mode is a one-time relay injection target, not a direct Provider switch.",
      "Switching profiles later changes only the Gateway active profile for this tool.",
      "The preview masks the local CodeStudio token. Real Provider API keys are never written to Codex config.",
      "Codex official login is still required for the desktop app; the Local Gateway only takes over model requests.",
      "If Codex is already running, restart Codex or open a new Codex session after bootstrap so it reloads config.toml."
    ]
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

  if (providerIsOfficial(profile.provider)) {
    return null;
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
  const providerId = `codestudio-${slugify(profile.provider) || "provider"}`;
  const secret = profile.authRef ? "keychain:****" : "(missing keychain secret)";
  const model = profile.model.trim();
  const path =
    mockNativeConfigPath(app, mode, profile.provider) ??
    nativeConfigPath ??
    mockToolConfigPath(app) ??
    "~/.codestudio-lite/native-config";

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
        "Config file mode writes Claude Code user settings under the env section.",
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
        "Hermes config file mode currently targets OpenAI Chat Completions endpoints."
      ]
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
        "Claude Desktop config file mode writes the 3P profile system used by Claude Desktop.",
        "CodeStudio Lite enables Claude Desktop developer mode before writing the 3P profile if it is not already enabled.",
        "The selected endpoint must be Anthropic Messages compatible; generic OpenAI-only endpoints need Gateway mode.",
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
      "Claude Desktop gateway mode writes the 3P profile to the tool-scoped CodeStudio Lite Local Gateway URL.",
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
  const providerId = "codestudio-local";
  const providerName = "CodeStudio Lite Local Gateway";
  const localToken = "codestudio-local-****7f3a2c";
  const localModel = "codestudio-default";
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
        "Gateway mode writes Claude Code settings to the tool-scoped local gateway URL.",
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
        "Gateway mode writes Gemini CLI environment values to the tool-scoped local gateway URL.",
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
        "Gateway mode writes OpenCode's provider entry to the tool-scoped local gateway URL.",
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
        "Gateway mode writes OpenClaw's provider entry to the tool-scoped local gateway URL.",
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
        "Gateway mode writes Hermes custom provider settings to the tool-scoped local gateway URL.",
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
  throw new Error(`Config file mode does not support ${mockProtocolLabel(protocol)} for '${canonicalProfileApp(app)}'.`);
}

function mockConfigProtocolSupported(profile: ProfileDraft): boolean {
  return mockConfigProtocolSupportedFields(profile.app, profile.provider, profile.protocol);
}

function mockGatewayBaseUrlForTool(toolId: string): string {
  return `http://127.0.0.1:43112/tools/${canonicalProfileApp(toolId)}/v1`;
}

function parseMockImportProfiles(content: string): Array<Partial<ProfileDraft>> {
  const value = JSON.parse(content) as
    | Array<Partial<ProfileDraft>>
    | { profiles?: Array<Partial<ProfileDraft>>; drafts?: Array<Partial<ProfileDraft>>; bundle?: { profiles?: Array<Partial<ProfileDraft>> } };
  if (Array.isArray(value)) {
    return value;
  }
  if (Array.isArray(value.profiles)) {
    return value.profiles;
  }
  if (Array.isArray(value.drafts)) {
    return value.drafts;
  }
  if (Array.isArray(value.bundle?.profiles)) {
    return value.bundle.profiles;
  }
  throw new Error("Import file must contain a profiles array.");
}

function requireMockField(label: string, value: unknown): string {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`${label} is required`);
  }
  return value.trim();
}

function requireMockToken(label: string, value: unknown): string {
  const trimmed = requireMockField(label, value);
  if (!/^[A-Za-z0-9_-]+$/.test(trimmed)) {
    throw new Error(`${label} can only contain letters, numbers, '-' and '_'`);
  }
  return trimmed;
}

function normalizeMockTimeout(value: unknown): number {
  const timeout = typeof value === "number" && Number.isFinite(value) ? value : 120;
  if (timeout < 5 || timeout > 600) {
    throw new Error("Timeout must be between 5 and 600 seconds.");
  }
  return Math.trunc(timeout);
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
    ? `Config file mode does not support ${mockProtocolLabel(profile.protocol)} for '${profile.app}'.`
    : !configSupported && !isOfficial
    ? `Config file mode adapter is not implemented for '${profile.app}'.`
    : !profile.authRef && providerRequiresApiKey(profile.provider)
      ? "Config file mode needs a stored Provider API key for this Provider."
      : null;
  const gatewayWritesNativeConfig = Boolean(gatewayNativeDiff);
  const gatewaySupported = !isOfficial;

  return [
    {
      mode: "config",
      label: "CC Switch config file mode",
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
            "Direct config file mode writes Provider connection details into the client config.",
            "Frequent Provider switching may require the client to reload its own config."
          ]
        : []
    },
    {
      mode: "gateway",
      label: "Gateway mode",
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
  try {
    const parsed = new URL(trimmed);
    if (!["http:", "https:"].includes(parsed.protocol) || !parsed.host) {
      throw new Error("invalid");
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
      detail: "Base URL must start with http:// or https:// and include a host."
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

function mockProfileTomlContent(input: {
  id: string;
  name: string;
  app: string;
  mode: ProviderApplyMode;
  provider: string;
  protocol: string;
  model: string;
  baseUrl: string;
  authRef: string | null;
  timeoutSeconds: number;
  timestamp: string;
  secretStatus: string;
}): string {
  return `id = "${mockTomlString(input.id)}"
name = "${mockTomlString(input.name)}"
app = "${mockTomlString(input.app)}"
provider = "${mockTomlString(input.provider)}"
mode = "${input.mode}"
protocol = "${mockTomlString(input.protocol)}"
model = "${mockTomlString(input.model)}"
base_url = "${mockTomlString(input.baseUrl)}"
timeout_seconds = ${input.timeoutSeconds}

[auth]
api_key = "${mockTomlString(input.authRef ?? "")}"

[metadata]
created_at = "${mockTomlString(input.timestamp)}"
updated_at = "${mockTomlString(input.timestamp)}"
last_test_status = "pending"
secret_status = "${mockTomlString(input.secretStatus)}"
`;
}

function mockTomlString(value: string): string {
  return value.replaceAll("\\", "\\\\").replaceAll("\"", "\\\"").replaceAll("\n", "\\n").replaceAll("\r", "\\r");
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

function defaultMockProfileMode(provider: string): ProviderApplyMode {
  return providerIsOfficial(provider) ? "config" : "gateway";
}

function normalizeMockProfileMode(
  provider: string,
  requested?: ProviderApplyMode | null
): ProviderApplyMode {
  const mode = requested ?? defaultMockProfileMode(provider);
  if (providerIsOfficial(provider) && mode === "gateway") {
    throw new Error("Official provider uses the client login directly and cannot use Gateway mode.");
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

function mockRestartMessageForProfile(profile: ProfileDraft): string {
  const labels: Record<string, string> = {
    codex: "Codex 客户端、Codex CLI 或 Codex VS Code",
    "claude-desktop": "Claude Desktop",
    claude: "Claude Code 或 Claude VS Code",
    gemini: "Gemini CLI",
    "gemini-code-assist": "Gemini Code Assist",
    opencode: "OpenCode",
    openclaw: "OpenClaw",
    hermes: "Hermes"
  };
  const app = canonicalProfileApp(profile.app);
  return `未检测到正在运行的 ${labels[app] ?? profile.app}，无需重启。`;
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
