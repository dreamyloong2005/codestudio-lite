export type InstallState = "installed" | "missing" | "unknown";
export type ConfigState = "configured" | "unconfigured" | "not_applicable" | "unknown";
export type Severity = "ok" | "info" | "warning" | "error";
export type DetectionSource = "live" | "preview" | "cached";
export type ChatGPTDesktopProductGeneration = "current" | "legacy";

export interface ToolStatus {
  id: string;
  name: string;
  category: "ai_tool" | "system";
  command: string;
  pathRepair: PathRepairHint | null;
  version: string | null;
  latestVersion: string | null;
  updateAvailable: boolean;
  updateCommand: string | null;
  installState: InstallState;
  configState: ConfigState;
  configPath: string | null;
  installPath: string | null;
  installCommand: string | null;
  details: string | null;
  installKind?: string | null;
  running: boolean;
}

export interface PathRepairHint {
  status: Severity;
  candidatePath: string;
  directory: string;
  message: string;
}

export interface RepairToolPathRequest {
  toolId: string;
  confirm: boolean;
}

export interface RepairToolPathResult {
  success: boolean;
  toolId: string;
  toolName: string;
  addedPath: string | null;
  message: string;
  currentStatus: ToolStatus | null;
  notes: string[];
}

export interface ToolInstallStep {
  label: string;
  detail: string;
}

export interface ToolInstallCommand {
  toolId: string;
  toolName: string;
  stage: string;
  manager: string;
  command: string;
  requiresAdmin: boolean;
  interactive: boolean;
}

export interface ToolInstallPrerequisite {
  toolId: string;
  toolName: string;
  manager: string;
  command: string;
  installed: boolean;
  canInstall: boolean;
  reason: string;
}

export interface ToolInstallPlan {
  toolId: string;
  toolName: string;
  manager: string;
  command: string;
  interactive: boolean;
  commands: ToolInstallCommand[];
  prerequisites: ToolInstallPrerequisite[];
  requiresPrerequisites: boolean;
  canInstall: boolean;
  alreadyInstalled: boolean;
  requiresAdmin: boolean;
  steps: ToolInstallStep[];
  warnings: string[];
  blocker: string | null;
}

export interface ToolInstallRequest {
  toolId: string;
  confirm: boolean;
  installKind?: "msix" | "exe" | null;
  installPrerequisites?: boolean;
}

export interface ToolUninstallRequest {
  toolId: string;
  confirm: boolean;
  installKind?: "msix" | "exe" | null;
}

export interface ToolInstallProgress {
  rootToolId: string;
  toolId: string;
  toolName: string;
  stage: string;
  command: string;
  installKind?: "msix" | "exe" | null;
  phase?: string | null;
  message?: string | null;
  downloaded?: number | null;
  total?: number | null;
  percent?: number | null;
  step?: number | null;
  stepTotal?: number | null;
  stream: "stdout" | "stderr" | "status" | string;
  chunk: string;
  done: boolean;
  exitCode: number | null;
}

export interface StartInstallTerminalRequest {
  toolId: string;
  command: string;
  shellId?: string | null;
  profileId?: string | null;
  workingDirectory?: string | null;
  localize?: boolean | null;
  keepOpen?: boolean | null;
  cols?: number;
  rows?: number;
}

export interface StartInstallTerminalResult {
  sessionId: string;
  toolId: string;
  command: string;
  started: boolean;
}

export type LaunchMode = "embedded" | "external";

export interface ExternalToolLaunchResult {
  started: boolean;
  toolId: string;
  command: string;
}

export interface InstallTerminalInputRequest {
  sessionId: string;
  data: string;
}

export interface InstallTerminalResizeRequest {
  sessionId: string;
  cols: number;
  rows: number;
}

export interface StopInstallTerminalRequest {
  sessionId: string;
}

export interface ClaudeDesktopLaunchRequest {
  localize?: boolean | null;
}

export interface ClaudeDesktopPendingLaunch {
  action: "launch";
  localize: boolean;
  requestedAt: string | null;
}

export interface ClaudeDesktopLocalizationProgress {
  phase: "launching" | "debugger" | "injecting" | "done" | "failed";
  message: string;
  attempt: number;
  maxAttempts: number;
  done: boolean;
  success: boolean;
  attached?: number | null;
  error?: string | null;
}

export interface ClaudeDesktopPlan {
  downloadUrl: string;
  sha256: string;
  installLocation: string;
}

export interface ClaudeDesktopPageState {
  snapshot: DetectionSnapshot;
  installPlan: ToolInstallPlan | null;
  updatePlan: ToolInstallPlan | null;
  plan: ClaudeDesktopPlan | null;
  capabilities: DesktopClientCapability[];
}

export interface InstallTerminalOutput {
  sessionId: string;
  stream: "output" | "status" | string;
  data: string;
  done: boolean;
  exitCode: number | null;
}

export interface ToolLaunchShellOption {
  id: string;
  label: string;
  command: string;
  available: boolean;
  default: boolean;
}

export interface ToolLaunchProfileOption {
  id: string;
  name: string;
  mode: ProviderApplyMode;
  provider: string;
  baseUrl: string;
  isBuiltin: boolean;
}

export interface ToolLaunchPlan {
  toolId: string;
  toolName: string;
  command: string;
  canLaunch: boolean;
  blocker: string | null;
  shells: ToolLaunchShellOption[];
  profiles: ToolLaunchProfileOption[];
}

export interface ToolInstallResult {
  success: boolean;
  toolId: string;
  toolName: string;
  action: string;
  message: string;
  command: string;
  exitCode: number | null;
  stdoutTail: string;
  stderrTail: string;
  currentStatus: ToolStatus | null;
  stageResults: ToolInstallStageResult[];
  notes: string[];
}

export interface ToolInstallStageResult {
  toolId: string;
  toolName: string;
  stage: string;
  command: string;
  success: boolean;
  exitCode: number | null;
  stdoutTail: string;
  stderrTail: string;
  message: string;
}

export interface WizardPrefill {
  toolId?: string;
  toolName?: string;
  mode?: ProviderApplyMode;
}

export interface Problem {
  id: string;
  severity: Severity;
  title: string;
  detail: string;
  actionLabel: string | null;
}

export interface EnvironmentVariableConflict {
  toolId: string;
  toolName: string;
  variable: string;
  currentValuePreview: string;
  expectedValuePreview: string | null;
  scope: string;
  severity: Severity;
  message: string;
}

export interface ClearEnvironmentVariablesRequest {
  toolId: string;
  variables: string[];
  confirm: boolean;
}

export interface ClearEnvironmentVariablesResult {
  success: boolean;
  toolId: string;
  cleared: string[];
  skipped: string[];
  message: string;
  conflicts: EnvironmentVariableConflict[];
}

export interface DetectionSnapshot {
  generatedAt: string;
  source: DetectionSource;
  platform: string;
  homeDir: string;
  appConfigDir: string;
  activeProfile: string | null;
  activeProfileName: string | null;
  codexAuth: CodexAuthStatus;
  tools: ToolStatus[];
  system: ToolStatus[];
  problems: Problem[];
  envConflicts: EnvironmentVariableConflict[];
  chatgptDesktopProductGeneration: ChatGPTDesktopProductGeneration;
  claudeInstallKinds?: ClaudeDesktopInstallKinds | null;
  chatgptDesktopInstallKinds?: ChatGPTDesktopInstallKinds | null;
}

export type CodexAuthMethod = "chat_gpt" | "api_key" | "access_token" | "unknown" | "none";
export type CodexAuthStorage = "auth_json" | "keyring" | "auto" | "none" | "unknown";

export interface CodexAuthStatus {
  available: boolean;
  method: CodexAuthMethod;
  storage: CodexAuthStorage;
  path: string | null;
  detail: string;
}

export interface StartCodexOAuthLoginResult {
  started: boolean;
  command: string | null;
  message: string;
}

export interface DoctorCheck {
  id: string;
  group: string;
  label: string;
  status: Severity;
  detail: string;
}

export interface DoctorReport {
  generatedAt: string;
  checks: DoctorCheck[];
  problems: Problem[];
}

export type Locale = "zh-CN" | "zh-TW" | "en-US";

export interface AppSettings {
  theme: "system" | "light" | "dark";
  language: Locale;
  backupBeforeWrite: boolean;
  redactSecrets: boolean;
  confirmInstallCommands: boolean;
  confirmConfigWrites: boolean;
}

export interface UpdateAppSettingsRequest {
  theme?: AppSettings["theme"] | null;
  language?: Locale | null;
}

export interface ProfileDraft {
  id: string;
  name: string;
  icon: string | null;
  remark: string | null;
  app: string;
  isBuiltin: boolean;
  mode: ProviderApplyMode;
  provider: string;
  protocol: string;
  model: string;
  modelMappings: ProfileModelMapping[];
  baseUrl: string;
  authRef: string | null;
  createdAt: string | null;
  updatedAt: string | null;
  lastTestStatus: string | null;
  usageEnabled: boolean;
  sortOrder: number;
}

export type ProviderApplyMode = "config" | "gateway";

export interface ProfileModelMapping {
  alias: string;
  model: string;
  supports1m: boolean;
  description?: string | null;
}

export type UsageScriptTemplateType = "custom" | "general" | "newapi" | "token_plan" | "balance";

export interface UsageScriptConfig {
  profileId: string;
  enabled: boolean;
  templateType: UsageScriptTemplateType;
  code: string;
  apiKey: string | null;
  baseUrl: string | null;
  accessToken: string | null;
  userId: string | null;
  timeoutSeconds: number;
  autoQueryIntervalMinutes: number;
  updatedAt: string | null;
}

export interface UsageScriptSaveRequest {
  profileId: string;
  enabled: boolean;
  templateType: UsageScriptTemplateType;
  code: string;
  apiKey?: string | null;
  baseUrl?: string | null;
  accessToken?: string | null;
  userId?: string | null;
  timeoutSeconds?: number | null;
  autoQueryIntervalMinutes?: number | null;
}

export interface UsageData {
  isValid?: boolean | null;
  invalidMessage?: string | null;
  remaining?: number | null;
  unit?: string | null;
  planName?: string | null;
  total?: number | null;
  used?: number | null;
  extra?: string | null;
}

export interface UsageQueryResult {
  success: boolean;
  data: UsageData[];
  error: string | null;
  queriedAt: string;
  source: string;
}

export interface UsageScriptState {
  profileId: string;
  config: UsageScriptConfig | null;
  lastResult: UsageQueryResult | null;
  defaultCode: string;
}

export interface ActiveProfilesByMode {
  config: Record<string, string>;
  gateway: Record<string, string>;
}

export interface SaveProfileDraftRequest {
  name: string;
  icon?: string | null;
  remark?: string | null;
  app: string;
  mode?: ProviderApplyMode | null;
  provider: string;
  protocol?: string | null;
  model: string;
  modelMappings?: ProfileModelMapping[] | null;
  baseUrl: string;
  secretProvided: boolean;
  apiKey?: string | null;
}

export interface UpdateProfileDraftRequest {
  profileId: string;
  name: string;
  icon?: string | null;
  remark?: string | null;
  mode?: ProviderApplyMode | null;
  provider: string;
  protocol?: string | null;
  model: string;
  modelMappings?: ProfileModelMapping[] | null;
  baseUrl: string;
  apiKey?: string | null;
}

export interface DuplicateProfileDraftRequest {
  profileId: string;
}

export interface DeleteProfileDraftRequest {
  profileId: string;
}

export interface ReorderProfileDraftsRequest {
  app: string;
  mode: ProviderApplyMode;
  profileIds: string[];
}

export interface PreviewProfileWriteRequest {
  name: string;
  icon?: string | null;
  remark?: string | null;
  app: string;
  mode?: ProviderApplyMode | null;
  provider: string;
  protocol?: string | null;
  model: string;
  modelMappings?: ProfileModelMapping[] | null;
  baseUrl: string;
  secretProvided: boolean;
  apiKey?: string | null;
}

export interface ProfileWritePreviewItem {
  label: string;
  path: string | null;
  action: string;
  backupRequired: boolean;
  detail: string;
  content?: string | null;
}

export interface PreviewProfileWriteResult {
  generatedAt: string;
  profileId: string;
  profilePath: string;
  targetToolPath: string | null;
  backupRequired: boolean;
  items: ProfileWritePreviewItem[];
  warnings: string[];
}

export interface PreviewProfileApplyRequest {
  profileId: string;
}

export interface ProfileApplyPreviewItem {
  label: string;
  path: string | null;
  action: string;
  backupRequired: boolean;
  detail: string;
}

export interface NativeConfigDiffLine {
  key: string;
  action: string;
  before: string | null;
  after: string | null;
  detail: string;
}

export interface NativeConfigPreview {
  tool: string;
  path: string;
  status: string;
  writeEnabled: boolean;
  changes: NativeConfigDiffLine[];
  warnings: string[];
  content?: string | null;
}

export interface ProviderApplyModePreview {
  mode: ProviderApplyMode;
  label: string;
  description: string;
  supported: boolean;
  recommended: boolean;
  writesNativeConfig: boolean;
  startsGateway: boolean;
  blockedReason: string | null;
  nativeDiff: NativeConfigPreview | null;
  warnings: string[];
}

export interface PreviewProfileApplyResult {
  generatedAt: string;
  profileId: string;
  profileName: string;
  app: string;
  provider: string;
  canApply: boolean;
  items: ProfileApplyPreviewItem[];
  nativeDiff: NativeConfigPreview | null;
  modePreviews: ProviderApplyModePreview[];
  warnings: string[];
  envConflicts: EnvironmentVariableConflict[];
}

export interface ApplyProfileRequest {
  profileId: string;
  restartAfterApply?: boolean;
  syncClaudeVsCode?: boolean;
}

export interface ApplyProfileResult {
  summary: ProfileSummary;
  mode: ProviderApplyMode | "profile_only";
  backup: BackupManifest;
  appliedPath: string;
  verified: boolean;
  nativePath: string | null;
  nativeVerified: boolean;
  restartRequested: boolean;
  restartPerformed: boolean;
  restartMessage: string | null;
  gatewayStatus: GatewayStatus | null;
  envConflicts: EnvironmentVariableConflict[];
}

export interface TestProfileConnectionRequest {
  app: string;
  provider: string;
  protocol?: string | null;
  model: string;
  baseUrl: string;
  secretProvided: boolean;
  apiKey?: string | null;
}

export interface ProfileConnectionCheck {
  id: string;
  label: string;
  status: Severity;
  detail: string;
}

export interface TestProfileConnectionResult {
  generatedAt: string;
  status: Severity;
  checks: ProfileConnectionCheck[];
}

export interface ListProfileModelsRequest {
  profileId?: string | null;
  app: string;
  mode?: ProviderApplyMode | null;
  provider: string;
  protocol?: string | null;
  baseUrl: string;
  apiKey?: string | null;
}

export interface ProfileModelOption {
  id: string;
  name?: string | null;
  ownedBy?: string | null;
  supports1m: boolean;
}

export interface ListProfileModelsResult {
  generatedAt: string;
  provider: string;
  protocol: string;
  baseUrl: string;
  models: ProfileModelOption[];
}

export interface BackupManifest {
  id: string;
  reason: string;
  profile: string | null;
  changedFiles: string[];
  createdAt: string;
}

export interface RestoreBackupRequest {
  backupId: string;
}

export interface RestoreBackupResult {
  restored: BackupManifest;
  safetyBackup: BackupManifest;
}

export interface ProfileSummary {
  configDir: string;
  activeProfile: string | null;
  activeProfileName: string | null;
  activeProfilesByMode: ActiveProfilesByMode;
  codexAuth: CodexAuthStatus;
  drafts: ProfileDraft[];
}

export interface ActivityEvent {
  id: string;
  level: Severity;
  message: string;
  createdAt: string;
}

export interface GatewayRequestLogEntry {
  id: string;
  timestamp: string;
  client: string;
  method: string;
  path: string;
  provider: string | null;
  model: string | null;
  status: number;
  latencyMs: number;
  errorSummary: string | null;
  privacyFilterMode: PrivacyFilterMode;
  privacyFilterHitCount: number;
  privacyFilterAction: "none" | "detected" | "redacted" | "blocked";
}

export type PrivacyFilterMode = "off" | "detect" | "redact" | "block";

export interface GatewayStatus {
  running: boolean;
  host: string;
  port: number;
  baseUrl: string;
  healthUrl: string;
  authEnabled: boolean;
  tokenPreview: string;
  privacyFilterMode: PrivacyFilterMode;
  activeProfileId: string | null;
  activeProfileName: string | null;
  activeModel: string | null;
  startedAt: string | null;
  lastError: string | null;
}

export interface GatewayControlResult {
  status: GatewayStatus;
}

export interface UpdateGatewaySettingsRequest {
  privacyFilterMode?: PrivacyFilterMode | null;
}

export interface ChatGPTDesktopSettings {
  source: "mirror" | "official";
  customUrl: string;
  autoCheck: boolean;
  askBefore: boolean;
  signedOnly: boolean;
  windowsInstallMode: "msix" | "portable";
  installRoot: string;
  keepUserDataOnUninstall: boolean;
  syncHistoryOnLaunch: boolean;
  pluginMarketplaceUnlockOnLaunch: boolean;
  pluginAutoExpandOnLaunch: boolean;
  modelWhitelistUnlockOnLaunch: boolean;
  serviceTierControlsOnLaunch: boolean;
  officialRemotePluginCacheOnLaunch: boolean;
  computerUseGuardOnLaunch: boolean;
}

export interface UpdateChatGPTDesktopSettingsRequest {
  source?: ChatGPTDesktopSettings["source"] | null;
  customUrl?: string | null;
  autoCheck?: boolean | null;
  askBefore?: boolean | null;
  windowsInstallMode?: ChatGPTDesktopSettings["windowsInstallMode"] | null;
  installRoot?: string | null;
  keepUserDataOnUninstall?: boolean | null;
  syncHistoryOnLaunch?: boolean | null;
  pluginMarketplaceUnlockOnLaunch?: boolean | null;
  pluginAutoExpandOnLaunch?: boolean | null;
  modelWhitelistUnlockOnLaunch?: boolean | null;
  serviceTierControlsOnLaunch?: boolean | null;
  officialRemotePluginCacheOnLaunch?: boolean | null;
  computerUseGuardOnLaunch?: boolean | null;
}

export interface PlanChatGPTDesktopUpdateRequest {
  installKind?: "msix" | "portable" | null;
}

export interface StageChatGPTDesktopUpdateRequest {
  installKind?: "msix" | "portable" | null;
}

export interface InstalledChatGPTDesktop {
  path: string;
  version: string;
  arch: string | null;
  source: string;
  generation: ChatGPTDesktopProductGeneration;
  packageFamilyName: string | null;
  installedAt: string | null;
}

export interface ChatGPTDesktopRelease {
  version: string;
  packageMoniker: string;
  architecture: string | null;
  packageKind: string;
  packageSource: string;
  contentLength: number | null;
  etag: string | null;
  packageIdentity: string | null;
  packageUrl: string;
  checksumsUrl: string;
  manifestUrl: string;
  sha256: string;
  macosArm64Version: string | null;
  macosX64Version: string | null;
}

export interface DesktopClientCapability {
  id: string;
  label: string;
  status: Severity;
  detail: string;
}

export interface ChatGPTDesktopPlan {
  upToDate: boolean;
  currentVersion: string | null;
  latestVersion: string;
  route: string;
  packageUrl: string;
  downloadSize: number | null;
  sha256: string;
  stagedPath: string | null;
  installRoot: string | null;
  warnings: string[];
  capabilities: DesktopClientCapability[];
}

export interface ChatGPTDesktopState {
  installKind: "msix" | "portable";
  generatedAt: string;
  platform: string;
  settings: ChatGPTDesktopSettings;
  installed: InstalledChatGPTDesktop | null;
  installClass: "managed" | "external" | "none" | string;
  release: ChatGPTDesktopRelease | null;
  plan: ChatGPTDesktopPlan | null;
  stagingDir: string;
  notes: string[];
  running: boolean;
}

export type ChatGPTDesktopStateCache = Partial<Record<"msix" | "portable", ChatGPTDesktopState>>;

export interface ChatGPTDesktopStageReport {
  installKind: "msix" | "portable";
  upToDate: boolean;
  stagedPath: string | null;
  packageMoniker: string;
  downloadSize: number;
  sha256: string;
  hashVerified: boolean;
  route: string;
  notes: string[];
}

export interface ChatGPTDesktopProgress {
  installKind: "msix" | "portable";
  phase: string;
  message: string;
  downloaded: number | null;
  total: number | null;
  percent: number | null;
  step: number | null;
  stepTotal: number | null;
}

export interface ChatGPTDesktopInstallRequest {
  confirm: boolean;
  expectedCurrentVersion?: string | null;
  expectedLatestVersion?: string | null;
  expectedRoute?: string | null;
  installKind?: "msix" | "portable" | null;
}

export interface ChatGPTDesktopUninstallRequest {
  confirm: boolean;
  purgeUserData: boolean;
  installKind?: "msix" | "portable" | null;
}

export interface ChatGPTDesktopOperationResult {
  installKind: "msix" | "portable";
  success: boolean;
  action: string;
  message: string;
  installed: InstalledChatGPTDesktop | null;
  stage: ChatGPTDesktopStageReport | null;
  notes: string[];
}

export interface DesktopInstallKindInfo {
  installed: boolean;
  version: string | null;
  path: string | null;
}

export interface ClaudeDesktopInstallKinds {
  msix: DesktopInstallKindInfo;
  exe: DesktopInstallKindInfo;
}

export interface ChatGPTDesktopInstallKinds {
  msix: DesktopInstallKindInfo;
  portable: DesktopInstallKindInfo;
}
