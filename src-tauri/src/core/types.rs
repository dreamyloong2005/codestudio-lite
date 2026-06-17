use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    pub name: String,
    pub category: ToolCategory,
    pub command: String,
    #[serde(default)]
    pub path_repair: Option<PathRepairHint>,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub update_command: Option<String>,
    pub install_state: InstallState,
    pub config_state: ConfigState,
    pub config_path: Option<String>,
    pub install_command: Option<String>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathRepairHint {
    pub status: Severity,
    pub candidate_path: String,
    pub directory: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairToolPathRequest {
    pub tool_id: String,
    pub confirm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairToolPathResult {
    pub success: bool,
    pub tool_id: String,
    pub tool_name: String,
    pub added_path: Option<String>,
    pub message: String,
    pub current_status: Option<ToolStatus>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallPlan {
    pub tool_id: String,
    pub tool_name: String,
    pub manager: String,
    pub command: String,
    pub commands: Vec<ToolInstallCommand>,
    pub prerequisites: Vec<ToolInstallPrerequisite>,
    pub requires_prerequisites: bool,
    pub can_install: bool,
    pub already_installed: bool,
    pub requires_admin: bool,
    pub steps: Vec<ToolInstallStep>,
    pub warnings: Vec<String>,
    pub blocker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallCommand {
    pub tool_id: String,
    pub tool_name: String,
    pub stage: String,
    pub manager: String,
    pub command: String,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallPrerequisite {
    pub tool_id: String,
    pub tool_name: String,
    pub manager: String,
    pub command: String,
    pub installed: bool,
    pub can_install: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallStep {
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallRequest {
    pub tool_id: String,
    pub confirm: bool,
    #[serde(default)]
    pub install_prerequisites: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallResult {
    pub success: bool,
    pub tool_id: String,
    pub tool_name: String,
    pub action: String,
    pub message: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub current_status: Option<ToolStatus>,
    pub stage_results: Vec<ToolInstallStageResult>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallStageResult {
    pub tool_id: String,
    pub tool_name: String,
    pub stage: String,
    pub command: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    AiTool,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    Installed,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigState {
    Configured,
    Unconfigured,
    NotApplicable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Ok,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    Live,
    Preview,
    Cached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub action_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVariableConflict {
    pub tool_id: String,
    pub tool_name: String,
    pub variable: String,
    pub current_value_preview: String,
    pub expected_value_preview: Option<String>,
    pub scope: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearEnvironmentVariablesRequest {
    pub tool_id: String,
    pub variables: Vec<String>,
    pub confirm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearEnvironmentVariablesResult {
    pub success: bool,
    pub tool_id: String,
    pub cleared: Vec<String>,
    pub skipped: Vec<String>,
    pub message: String,
    pub conflicts: Vec<EnvironmentVariableConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionSnapshot {
    pub generated_at: String,
    pub source: DetectionSource,
    pub home_dir: String,
    pub app_config_dir: String,
    pub active_profile: Option<String>,
    pub active_profile_name: Option<String>,
    pub codex_auth: CodexAuthStatus,
    pub tools: Vec<ToolStatus>,
    pub system: Vec<ToolStatus>,
    pub problems: Vec<Problem>,
    #[serde(default)]
    pub env_conflicts: Vec<EnvironmentVariableConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexAuthMethod {
    ChatGpt,
    ApiKey,
    AccessToken,
    Unknown,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexAuthStorage {
    AuthJson,
    Keyring,
    Auto,
    None,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAuthStatus {
    pub available: bool,
    pub method: CodexAuthMethod,
    pub storage: CodexAuthStorage,
    pub path: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorCheck {
    pub id: String,
    pub group: String,
    pub label: String,
    pub status: Severity,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub generated_at: String,
    pub checks: Vec<DoctorCheck>,
    pub problems: Vec<Problem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub backup_before_write: bool,
    pub redact_secrets: bool,
    pub confirm_install_commands: bool,
    pub confirm_config_writes: bool,
    pub preserve_codex_official_auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppSettingsRequest {
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub preserve_codex_official_auth: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDraft {
    pub id: String,
    pub name: String,
    pub app: String,
    #[serde(default)]
    pub is_builtin: bool,
    #[serde(default)]
    pub mode: ProviderApplyMode,
    pub provider: String,
    pub protocol: String,
    pub model: String,
    pub base_url: String,
    #[serde(default)]
    pub auth_ref: Option<String>,
    pub timeout_seconds: u16,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub last_test_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExportBundle {
    pub schema_version: u16,
    pub app: String,
    pub exported_at: String,
    pub active_profiles_by_mode: ActiveProfilesByMode,
    pub profiles: Vec<ProfileDraft>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProfilesResult {
    pub file_name: String,
    pub bundle: ProfileExportBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProfilesRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProfilesResult {
    pub imported: Vec<ProfileDraft>,
    pub skipped: Vec<String>,
    pub summary: ProfileSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProfileDraftRequest {
    pub name: String,
    pub app: String,
    #[serde(default)]
    pub mode: Option<ProviderApplyMode>,
    pub provider: String,
    #[serde(default)]
    pub protocol: Option<String>,
    pub model: String,
    pub base_url: String,
    pub secret_provided: bool,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileDraftRequest {
    pub profile_id: String,
    pub name: String,
    #[serde(default)]
    pub mode: Option<ProviderApplyMode>,
    pub provider: String,
    #[serde(default)]
    pub protocol: Option<String>,
    pub model: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateProfileDraftRequest {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProfileWriteRequest {
    pub name: String,
    pub app: String,
    #[serde(default)]
    pub mode: Option<ProviderApplyMode>,
    pub provider: String,
    #[serde(default)]
    pub protocol: Option<String>,
    pub model: String,
    pub base_url: String,
    pub secret_provided: bool,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileWritePreviewItem {
    pub label: String,
    pub path: Option<String>,
    pub action: String,
    pub backup_required: bool,
    pub detail: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProfileWriteResult {
    pub generated_at: String,
    pub profile_id: String,
    pub profile_path: String,
    pub target_tool_path: Option<String>,
    pub backup_required: bool,
    pub items: Vec<ProfileWritePreviewItem>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProfileApplyRequest {
    pub profile_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderApplyMode {
    Config,
    Gateway,
}

impl Default for ProviderApplyMode {
    fn default() -> Self {
        Self::Gateway
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveProfilesByMode {
    #[serde(default)]
    pub config: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub gateway: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileApplyPreviewItem {
    pub label: String,
    pub path: Option<String>,
    pub action: String,
    pub backup_required: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeConfigDiffLine {
    pub key: String,
    pub action: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeConfigPreview {
    pub tool: String,
    pub path: String,
    pub status: String,
    pub write_enabled: bool,
    pub changes: Vec<NativeConfigDiffLine>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderApplyModePreview {
    pub mode: ProviderApplyMode,
    pub label: String,
    pub description: String,
    pub supported: bool,
    pub recommended: bool,
    pub writes_native_config: bool,
    pub starts_gateway: bool,
    pub blocked_reason: Option<String>,
    pub native_diff: Option<NativeConfigPreview>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProfileApplyResult {
    pub generated_at: String,
    pub profile_id: String,
    pub profile_name: String,
    pub app: String,
    pub provider: String,
    pub can_apply: bool,
    pub items: Vec<ProfileApplyPreviewItem>,
    pub native_diff: Option<NativeConfigPreview>,
    pub mode_previews: Vec<ProviderApplyModePreview>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub env_conflicts: Vec<EnvironmentVariableConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProfileRequest {
    pub profile_id: String,
    #[serde(default)]
    pub restart_after_apply: bool,
    #[serde(default)]
    pub sync_claude_vs_code: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProfileResult {
    pub summary: ProfileSummary,
    pub mode: ProviderApplyMode,
    pub backup: BackupManifest,
    pub applied_path: String,
    pub verified: bool,
    pub native_path: Option<String>,
    pub native_verified: bool,
    pub restart_requested: bool,
    pub restart_performed: bool,
    pub restart_message: Option<String>,
    pub gateway_status: Option<GatewayStatus>,
    #[serde(default)]
    pub env_conflicts: Vec<EnvironmentVariableConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProfileConnectionRequest {
    pub app: String,
    pub provider: String,
    #[serde(default)]
    pub protocol: Option<String>,
    pub model: String,
    pub base_url: String,
    pub secret_provided: bool,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileConnectionCheck {
    pub id: String,
    pub label: String,
    pub status: Severity,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProfileConnectionResult {
    pub generated_at: String,
    pub status: Severity,
    pub checks: Vec<ProfileConnectionCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchActiveProfileRequest {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub id: String,
    pub reason: String,
    pub profile: Option<String>,
    pub changed_files: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupRequest {
    pub backup_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupResult {
    pub restored: BackupManifest,
    pub safety_backup: BackupManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub config_dir: String,
    pub profiles_dir: String,
    pub backups_dir: String,
    pub active_profile: Option<String>,
    pub active_profile_name: Option<String>,
    pub active_profiles_by_mode: ActiveProfilesByMode,
    pub codex_auth: CodexAuthStatus,
    pub drafts: Vec<ProfileDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub id: String,
    pub level: Severity,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRequestLogEntry {
    pub id: String,
    pub timestamp: String,
    pub client: String,
    pub method: String,
    pub path: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub status: u16,
    pub latency_ms: u128,
    pub error_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatus {
    pub running: bool,
    pub host: String,
    pub port: u16,
    pub base_url: String,
    pub health_url: String,
    pub auth_enabled: bool,
    pub token_preview: String,
    pub active_profile_id: Option<String>,
    pub active_profile_name: Option<String>,
    pub active_model: Option<String>,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayControlResult {
    pub status: GatewayStatus,
}
