export type InstallState = "installed" | "missing" | "unknown";
export type ConfigState = "configured" | "unconfigured" | "not_applicable" | "unknown";
export type Severity = "ok" | "info" | "warning" | "error";
export type DetectionSource = "live" | "preview" | "cached";

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
  installCommand: string | null;
  details: string | null;
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
  installPrerequisites?: boolean;
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
  toolId: string;
  toolName: string;
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
  homeDir: string;
  appConfigDir: string;
  activeProfile: string | null;
  activeProfileName: string | null;
  tools: ToolStatus[];
  system: ToolStatus[];
  problems: Problem[];
  envConflicts: EnvironmentVariableConflict[];
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
  app: string;
  isBuiltin: boolean;
  mode: ProviderApplyMode;
  provider: string;
  protocol: string;
  model: string;
  baseUrl: string;
  authRef: string | null;
  timeoutSeconds: number;
  createdAt: string | null;
  updatedAt: string | null;
  lastTestStatus: string | null;
}

export type ProviderApplyMode = "config" | "gateway";

export interface ActiveProfilesByMode {
  config: Record<string, string>;
  gateway: Record<string, string>;
}

export interface ProfileExportBundle {
  schemaVersion: number;
  app: string;
  exportedAt: string;
  activeProfilesByMode: ActiveProfilesByMode;
  profiles: ProfileDraft[];
  warnings: string[];
}

export interface ExportProfilesResult {
  fileName: string;
  bundle: ProfileExportBundle;
}

export interface ImportProfilesRequest {
  content: string;
}

export interface ImportProfilesResult {
  imported: ProfileDraft[];
  skipped: string[];
  summary: ProfileSummary;
}

export interface SaveProfileDraftRequest {
  name: string;
  app: string;
  mode?: ProviderApplyMode | null;
  provider: string;
  protocol?: string | null;
  model: string;
  baseUrl: string;
  secretProvided: boolean;
  apiKey?: string | null;
  timeoutSeconds?: number | null;
}

export interface UpdateProfileDraftRequest {
  profileId: string;
  name: string;
  mode?: ProviderApplyMode | null;
  provider: string;
  protocol?: string | null;
  model: string;
  baseUrl: string;
  apiKey?: string | null;
  timeoutSeconds?: number | null;
}

export interface DuplicateProfileDraftRequest {
  profileId: string;
}

export interface PreviewProfileWriteRequest {
  name: string;
  app: string;
  mode?: ProviderApplyMode | null;
  provider: string;
  protocol?: string | null;
  model: string;
  baseUrl: string;
  secretProvided: boolean;
  apiKey?: string | null;
  timeoutSeconds?: number | null;
}

export interface ProfileWritePreviewItem {
  label: string;
  path: string | null;
  action: string;
  backupRequired: boolean;
  detail: string;
  content: string | null;
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
  timeoutSeconds?: number | null;
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
  profilesDir: string;
  backupsDir: string;
  activeProfile: string | null;
  activeProfileName: string | null;
  activeProfilesByMode: ActiveProfilesByMode;
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
}

export interface GatewayStatus {
  running: boolean;
  host: string;
  port: number;
  baseUrl: string;
  healthUrl: string;
  authEnabled: boolean;
  tokenPreview: string;
  activeProfileId: string | null;
  activeProfileName: string | null;
  activeModel: string | null;
  startedAt: string | null;
  lastError: string | null;
}

export interface GatewayControlResult {
  status: GatewayStatus;
}

export interface CodexClientSettings {
  source: "mirror" | "official";
  customUrl: string;
  autoCheck: boolean;
  askBefore: boolean;
  signedOnly: boolean;
  windowsInstallMode: "msix" | "portable";
  installRoot: string;
  keepUserDataOnUninstall: boolean;
}

export interface UpdateCodexClientSettingsRequest {
  source?: CodexClientSettings["source"] | null;
  customUrl?: string | null;
  autoCheck?: boolean | null;
  askBefore?: boolean | null;
  windowsInstallMode?: CodexClientSettings["windowsInstallMode"] | null;
  installRoot?: string | null;
  keepUserDataOnUninstall?: boolean | null;
}

export interface InstalledCodexClient {
  path: string;
  version: string;
  arch: string | null;
  source: string;
  packageFamilyName: string | null;
  installedAt: string | null;
}

export interface CodexClientRelease {
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

export interface CodexClientCapability {
  id: string;
  label: string;
  status: Severity;
  detail: string;
}

export interface CodexClientPlan {
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
  capabilities: CodexClientCapability[];
}

export interface CodexClientState {
  generatedAt: string;
  platform: string;
  settings: CodexClientSettings;
  installed: InstalledCodexClient | null;
  installClass: "managed" | "external" | "none" | string;
  release: CodexClientRelease | null;
  plan: CodexClientPlan | null;
  stagingDir: string;
  notes: string[];
}

export interface CodexClientStageReport {
  upToDate: boolean;
  stagedPath: string | null;
  packageMoniker: string;
  downloadSize: number;
  sha256: string;
  hashVerified: boolean;
  route: string;
  notes: string[];
}

export interface CodexClientProgress {
  phase: string;
  message: string;
  downloaded: number | null;
  total: number | null;
  percent: number | null;
  step: number | null;
  stepTotal: number | null;
}

export interface CodexClientInstallRequest {
  confirm: boolean;
  expectedCurrentVersion?: string | null;
  expectedLatestVersion?: string | null;
  expectedRoute?: string | null;
}

export interface CodexClientUninstallRequest {
  confirm: boolean;
  purgeUserData: boolean;
}

export interface CodexClientOperationResult {
  success: boolean;
  action: string;
  message: string;
  installed: InstalledCodexClient | null;
  stage: CodexClientStageReport | null;
  notes: string[];
}
