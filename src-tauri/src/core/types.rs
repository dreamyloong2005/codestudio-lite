use serde::{Deserialize, Serialize};

use crate::core::privacy_filter::{PrivacyFilterAction, PrivacyFilterMode};

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
    pub install_path: Option<String>,
    pub install_command: Option<String>,
    pub details: Option<String>,
    /// How the desktop app is packaged: "msix" (Windows App / AppX under
    /// WindowsApps) or "exe" (native NSIS/Squirrel install). None when not
    /// installed or not a desktop app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_kind: Option<String>,
    #[serde(default)]
    pub running: bool,
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
    pub interactive: bool,
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
    pub interactive: bool,
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
    pub install_kind: Option<String>,
    #[serde(default)]
    pub install_prerequisites: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUninstallRequest {
    pub tool_id: String,
    pub confirm: bool,
    /// Which install kind to uninstall ("msix" or "exe" for Claude Desktop).
    /// When None, the backend falls back to the detected install kind.
    #[serde(default)]
    pub install_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallProgress {
    pub root_tool_id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub stage: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloaded: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_total: Option<u32>,
    pub stream: String,
    pub chunk: String,
    pub done: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInstallTerminalRequest {
    pub tool_id: String,
    pub command: String,
    #[serde(default)]
    pub shell_id: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub localize: Option<bool>,
    #[serde(default)]
    pub keep_open: Option<bool>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInstallTerminalResult {
    pub session_id: String,
    pub tool_id: String,
    pub command: String,
    pub started: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolLaunchResult {
    pub started: bool,
    pub tool_id: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTerminalInputRequest {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTerminalResizeRequest {
    pub session_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopInstallTerminalRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTerminalOutput {
    pub session_id: String,
    pub stream: String,
    pub data: String,
    pub done: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopLocalizationProgress {
    pub phase: String,
    pub message: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub done: bool,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLaunchShellOption {
    pub id: String,
    pub label: String,
    pub command: String,
    pub available: bool,
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLaunchProfileOption {
    pub id: String,
    pub name: String,
    pub mode: ProviderApplyMode,
    pub provider: String,
    pub base_url: String,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLaunchPlan {
    pub tool_id: String,
    pub tool_name: String,
    pub command: String,
    pub can_launch: bool,
    pub blocker: Option<String>,
    pub shells: Vec<ToolLaunchShellOption>,
    pub profiles: Vec<ToolLaunchProfileOption>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatGptDesktopProductGeneration {
    #[default]
    Current,
    Legacy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionSnapshot {
    pub generated_at: String,
    pub source: DetectionSource,
    pub platform: String,
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
    #[serde(default)]
    pub chatgpt_desktop_product_generation: ChatGptDesktopProductGeneration,
    /// Per-kind install detection for the Claude Desktop page tabs. Cached
    /// alongside the snapshot so the tabs render instantly from the on-disk
    /// detection cache before a fresh scan completes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_install_kinds: Option<ClaudeDesktopInstallKinds>,
    /// Per-kind install detection for the ChatGPT desktop client page tabs.
    #[serde(
        default,
        alias = "codexInstallKinds",
        skip_serializing_if = "Option::is_none"
    )]
    pub chatgpt_desktop_install_kinds: Option<ChatGptDesktopInstallKinds>,
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
pub struct StartCodexOAuthLoginResult {
    pub started: bool,
    pub command: Option<String>,
    pub message: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppSettingsRequest {
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileModelMapping {
    pub alias: String,
    pub model: String,
    #[serde(default)]
    pub supports_1m: bool,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDraft {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub remark: Option<String>,
    pub app: String,
    #[serde(default)]
    pub is_builtin: bool,
    #[serde(default)]
    pub mode: ProviderApplyMode,
    pub provider: String,
    pub protocol: String,
    pub model: String,
    #[serde(default)]
    pub model_mappings: Vec<ProfileModelMapping>,
    pub base_url: String,
    #[serde(default)]
    pub auth_ref: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub last_test_status: Option<String>,
    #[serde(default)]
    pub usage_enabled: bool,
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageScriptTemplateType {
    Custom,
    General,
    NewApi,
    TokenPlan,
    Balance,
}

impl Default for UsageScriptTemplateType {
    fn default() -> Self {
        Self::General
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageScriptConfig {
    pub profile_id: String,
    pub enabled: bool,
    #[serde(default)]
    pub template_type: UsageScriptTemplateType,
    pub code: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    pub timeout_seconds: u16,
    pub auto_query_interval_minutes: u16,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageScriptSaveRequest {
    pub profile_id: String,
    pub enabled: bool,
    #[serde(default)]
    pub template_type: UsageScriptTemplateType,
    pub code: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u16>,
    #[serde(default)]
    pub auto_query_interval_minutes: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageData {
    #[serde(default)]
    pub is_valid: Option<bool>,
    #[serde(default)]
    pub invalid_message: Option<String>,
    #[serde(default)]
    pub remaining: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub plan_name: Option<String>,
    #[serde(default)]
    pub total: Option<f64>,
    #[serde(default)]
    pub used: Option<f64>,
    #[serde(default)]
    pub extra: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageQueryResult {
    pub success: bool,
    #[serde(default)]
    pub data: Vec<UsageData>,
    #[serde(default)]
    pub error: Option<String>,
    pub queried_at: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageScriptState {
    pub profile_id: String,
    #[serde(default)]
    pub config: Option<UsageScriptConfig>,
    #[serde(default)]
    pub last_result: Option<UsageQueryResult>,
    pub default_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProfileDraftRequest {
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub remark: Option<String>,
    pub app: String,
    #[serde(default)]
    pub mode: Option<ProviderApplyMode>,
    pub provider: String,
    #[serde(default)]
    pub protocol: Option<String>,
    pub model: String,
    #[serde(default)]
    pub model_mappings: Option<Vec<ProfileModelMapping>>,
    pub base_url: String,
    pub secret_provided: bool,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileDraftRequest {
    pub profile_id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub remark: Option<String>,
    #[serde(default)]
    pub mode: Option<ProviderApplyMode>,
    pub provider: String,
    #[serde(default)]
    pub protocol: Option<String>,
    pub model: String,
    #[serde(default)]
    pub model_mappings: Option<Vec<ProfileModelMapping>>,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateProfileDraftRequest {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProfileDraftRequest {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderProfileDraftsRequest {
    pub app: String,
    pub mode: ProviderApplyMode,
    pub profile_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProfileWriteRequest {
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub remark: Option<String>,
    pub app: String,
    #[serde(default)]
    pub mode: Option<ProviderApplyMode>,
    pub provider: String,
    #[serde(default)]
    pub protocol: Option<String>,
    pub model: String,
    #[serde(default)]
    pub model_mappings: Option<Vec<ProfileModelMapping>>,
    pub base_url: String,
    pub secret_provided: bool,
    #[serde(default)]
    pub api_key: Option<String>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    pub content: Option<String>,
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
pub struct ListProfileModelsRequest {
    #[serde(default)]
    pub profile_id: Option<String>,
    pub app: String,
    #[serde(default)]
    pub mode: Option<ProviderApplyMode>,
    pub provider: String,
    #[serde(default)]
    pub protocol: Option<String>,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileModelOption {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub owned_by: Option<String>,
    #[serde(default)]
    pub supports_1m: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListProfileModelsResult {
    pub generated_at: String,
    pub provider: String,
    pub protocol: String,
    pub base_url: String,
    pub models: Vec<ProfileModelOption>,
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
    pub privacy_filter_mode: PrivacyFilterMode,
    pub privacy_filter_hit_count: usize,
    pub privacy_filter_action: PrivacyFilterAction,
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
    pub privacy_filter_mode: PrivacyFilterMode,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGatewaySettingsRequest {
    #[serde(default)]
    pub privacy_filter_mode: Option<PrivacyFilterMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInstallKindInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopInstallKinds {
    pub msix: DesktopInstallKindInfo,
    pub exe: DesktopInstallKindInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopPendingLaunch {
    pub action: String,
    pub localize: bool,
    pub requested_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopPlan {
    pub download_url: String,
    pub sha256: String,
    pub install_location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopPageState {
    pub snapshot: DetectionSnapshot,
    pub install_plan: Option<ToolInstallPlan>,
    pub update_plan: Option<ToolInstallPlan>,
    pub plan: Option<ClaudeDesktopPlan>,
    pub capabilities: Vec<crate::core::chatgpt_desktop::DesktopClientCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptDesktopInstallKinds {
    pub msix: DesktopInstallKindInfo,
    pub portable: DesktopInstallKindInfo,
}
