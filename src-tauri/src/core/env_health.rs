use crate::core::activity_log;
use crate::core::app_paths::display_path;
use crate::core::credentials;
use crate::core::detector;
use crate::core::platform::{
    hidden_command_with_args, repair_candidate_for_command, resolve_command_on_path, run_powershell,
};
use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};
use crate::core::types::{
    ClearEnvironmentVariablesRequest, ClearEnvironmentVariablesResult, EnvironmentVariableConflict,
    PathRepairHint, RepairToolPathRequest, RepairToolPathResult, Severity,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

const CLAUDE_TOOL_ID: &str = "claude";
const CLAUDE_TOOL_NAME: &str = "Claude Code";
const CLAUDE_ENV_VARS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_MODEL",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEnvVarValue {
    name: String,
    scope: String,
    value: String,
}

pub fn path_repair_hint(definition: &ToolDefinition) -> Option<PathRepairHint> {
    let (candidate, directory) = repair_candidate_for_command(definition.command)?;
    Some(PathRepairHint {
        status: Severity::Warning,
        candidate_path: display_path(&candidate),
        directory: display_path(&directory),
        message: format!(
            "已在常见安装目录发现 {}，但当前 PATH 无法直接解析命令 {}。",
            display_path(&candidate),
            definition.command
        ),
    })
}

pub fn repair_tool_path(request: RepairToolPathRequest) -> Result<RepairToolPathResult, String> {
    if !request.confirm {
        return Err("拒绝执行：修复 PATH 前必须显式确认。".to_string());
    }
    let definition = tool_definition(&request.tool_id)
        .ok_or_else(|| format!("工具 '{}' 不在 PATH 修复白名单中。", request.tool_id))?;
    if resolve_command_on_path(definition.command).is_some() {
        return Ok(RepairToolPathResult {
            success: true,
            tool_id: definition.id.to_string(),
            tool_name: definition.name.to_string(),
            added_path: None,
            message: format!("{} 已经在当前 PATH 中可用。", definition.name),
            current_status: find_current_tool_status(definition.id),
            notes: Vec::new(),
        });
    }

    let Some((_candidate, directory)) = repair_candidate_for_command(definition.command) else {
        return Ok(RepairToolPathResult {
            success: false,
            tool_id: definition.id.to_string(),
            tool_name: definition.name.to_string(),
            added_path: None,
            message: format!("没有在常见安装目录中找到 {}。", definition.name),
            current_status: find_current_tool_status(definition.id),
            notes: Vec::new(),
        });
    };

    let mut notes = Vec::new();
    let added = repair_user_path(&directory, &mut notes)?;
    let after = find_current_tool_status(definition.id);
    let success = resolve_command_on_path(definition.command).is_some()
        || after
            .as_ref()
            .map(|status| status.path_repair.is_none())
            .unwrap_or(false);
    let message = if success {
        if added {
            format!("已把 {} 加入用户 PATH。", display_path(&directory))
        } else {
            format!(
                "{} 已存在于持久 PATH，已刷新当前进程 PATH。",
                display_path(&directory)
            )
        }
    } else {
        format!(
            "已尝试修复 PATH，但 {} 仍未通过复检。新终端或重启应用后可能生效。",
            definition.name
        )
    };
    let _ = activity_log::append(
        if success {
            Severity::Ok
        } else {
            Severity::Warning
        },
        message.clone(),
    );

    Ok(RepairToolPathResult {
        success,
        tool_id: definition.id.to_string(),
        tool_name: definition.name.to_string(),
        added_path: Some(display_path(&directory)),
        message,
        current_status: after,
        notes,
    })
}

pub fn claude_env_conflicts_for_active_config(
    drafts: &[crate::core::types::ProfileDraft],
    active_config: &HashMap<String, String>,
) -> Vec<EnvironmentVariableConflict> {
    let Some(profile_id) = active_config.get(CLAUDE_TOOL_ID) else {
        return claude_env_conflicts_without_profile();
    };
    let Some(profile) = drafts.iter().find(|profile| profile.id == *profile_id) else {
        return claude_env_conflicts_without_profile();
    };
    claude_env_conflicts_for_profile(profile)
}

pub fn claude_env_conflicts_for_profile(
    profile: &crate::core::types::ProfileDraft,
) -> Vec<EnvironmentVariableConflict> {
    if canonical_tool_id(&profile.app) != CLAUDE_TOOL_ID {
        return Vec::new();
    }

    let mut expected = HashMap::new();
    expected.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        ExpectedEnvValue::Exact(profile.base_url.trim().to_string()),
    );
    let expected_secret = profile
        .auth_ref
        .as_deref()
        .and_then(|auth_ref| credentials::load_keychain_secret(auth_ref).ok())
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty());
    if let Some(secret) = expected_secret {
        expected.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            ExpectedEnvValue::Secret(secret.clone()),
        );
        expected.insert(
            "ANTHROPIC_API_KEY".to_string(),
            ExpectedEnvValue::Secret(secret),
        );
    } else {
        expected.insert("ANTHROPIC_AUTH_TOKEN".to_string(), ExpectedEnvValue::Absent);
        expected.insert("ANTHROPIC_API_KEY".to_string(), ExpectedEnvValue::Absent);
    }
    if profile.model.trim().is_empty() {
        expected.insert("ANTHROPIC_MODEL".to_string(), ExpectedEnvValue::Absent);
    } else {
        expected.insert(
            "ANTHROPIC_MODEL".to_string(),
            ExpectedEnvValue::Exact(profile.model.trim().to_string()),
        );
    }
    expected.insert(
        "CLAUDE_CODE_USE_BEDROCK".to_string(),
        ExpectedEnvValue::Absent,
    );
    expected.insert(
        "CLAUDE_CODE_USE_VERTEX".to_string(),
        ExpectedEnvValue::Absent,
    );

    claude_env_conflicts(expected)
}

pub fn claude_env_conflicts_without_profile() -> Vec<EnvironmentVariableConflict> {
    let expected = CLAUDE_ENV_VARS
        .iter()
        .map(|name| ((*name).to_string(), ExpectedEnvValue::Absent))
        .collect::<HashMap<_, _>>();
    claude_env_conflicts(expected)
}

pub fn clear_environment_variables(
    request: ClearEnvironmentVariablesRequest,
) -> Result<ClearEnvironmentVariablesResult, String> {
    if !request.confirm {
        return Err("拒绝执行：清理环境变量前必须显式确认。".to_string());
    }
    if canonical_tool_id(&request.tool_id) != CLAUDE_TOOL_ID {
        return Err("当前只支持清理 Claude 相关环境变量。".to_string());
    }

    let requested = if request.variables.is_empty() {
        CLAUDE_ENV_VARS
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    } else {
        request
            .variables
            .into_iter()
            .filter(|name| CLAUDE_ENV_VARS.contains(&name.as_str()))
            .collect::<Vec<_>>()
    };
    let mut seen = HashSet::new();
    let variables = requested
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .collect::<Vec<_>>();
    if variables.is_empty() {
        return Err("没有可清理的 Claude 环境变量。".to_string());
    }

    let mut cleared = Vec::new();
    let mut skipped = Vec::new();
    clear_process_env(&variables, &mut cleared);
    if cfg!(target_os = "windows") {
        let (user_cleared, machine_present) = clear_user_env_vars_windows(&variables)?;
        for name in user_cleared {
            if !cleared.contains(&name) {
                cleared.push(name);
            }
        }
        for name in machine_present {
            skipped.push(format!("{name} 存在于机器级环境变量，需要管理员手动清理。"));
        }
    } else if cfg!(target_os = "macos") {
        let launchctl_cleared = clear_launchctl_env_vars_macos(&variables)?;
        for name in launchctl_cleared {
            if !cleared.contains(&name) {
                cleared.push(name);
            }
        }
        skipped.push(
            "已清理当前进程和 launchctl 全局变量；如果 shell 启动文件里还有 export，需要手动移除。"
                .to_string(),
        );
    } else {
        skipped.push("当前平台仅清理了本进程环境变量；Shell 启动文件需要手动检查。".to_string());
    }

    let conflicts = claude_env_conflicts_without_profile();
    let success =
        conflicts.is_empty() || conflicts.iter().all(|conflict| conflict.scope == "machine");
    let message = if success {
        "Claude 全局环境变量已清理。".to_string()
    } else {
        "已清理可写环境变量，但仍检测到残留冲突。".to_string()
    };
    let _ = activity_log::append(
        if success {
            Severity::Ok
        } else {
            Severity::Warning
        },
        message.clone(),
    );

    Ok(ClearEnvironmentVariablesResult {
        success,
        tool_id: CLAUDE_TOOL_ID.to_string(),
        cleared,
        skipped,
        message,
        conflicts,
    })
}

#[derive(Debug, Clone)]
enum ExpectedEnvValue {
    Exact(String),
    Secret(String),
    Absent,
}

fn claude_env_conflicts(
    expected: HashMap<String, ExpectedEnvValue>,
) -> Vec<EnvironmentVariableConflict> {
    read_claude_env_values()
        .into_iter()
        .filter(|value| !value.value.trim().is_empty())
        .filter_map(|value| {
            let expected_value = expected
                .get(&value.name)
                .cloned()
                .unwrap_or(ExpectedEnvValue::Absent);
            let matches = match &expected_value {
                ExpectedEnvValue::Exact(expected) => value.value.trim() == expected.trim(),
                ExpectedEnvValue::Secret(expected) => value.value.trim() == expected.trim(),
                ExpectedEnvValue::Absent => false,
            };
            if matches {
                return None;
            }
            let expected_value_preview = match expected_value {
                ExpectedEnvValue::Exact(expected) if !expected.trim().is_empty() => {
                    Some(preview_env_value(&value.name, &expected))
                }
                ExpectedEnvValue::Secret(_) => Some("已保存的 Provider API Key".to_string()),
                ExpectedEnvValue::Exact(_) | ExpectedEnvValue::Absent => None,
            };
            Some(EnvironmentVariableConflict {
                tool_id: CLAUDE_TOOL_ID.to_string(),
                tool_name: CLAUDE_TOOL_NAME.to_string(),
                variable: value.name.clone(),
                current_value_preview: preview_env_value(&value.name, &value.value),
                expected_value_preview,
                scope: value.scope,
                severity: Severity::Warning,
                message: format!(
                    "{} 会影响 Claude API 连接，且与当前 CodeStudio 配置不一致。",
                    value.name
                ),
            })
        })
        .collect()
}

fn read_claude_env_values() -> Vec<RawEnvVarValue> {
    if cfg!(target_os = "windows") {
        read_claude_env_values_windows().unwrap_or_else(|_| read_process_env_values())
    } else if cfg!(target_os = "macos") {
        read_claude_env_values_macos()
    } else {
        read_process_env_values()
    }
}

fn read_process_env_values() -> Vec<RawEnvVarValue> {
    CLAUDE_ENV_VARS
        .iter()
        .filter_map(|name| {
            env::var(name).ok().map(|value| RawEnvVarValue {
                name: (*name).to_string(),
                scope: "process".to_string(),
                value,
            })
        })
        .collect()
}

fn read_claude_env_values_windows() -> Result<Vec<RawEnvVarValue>, String> {
    let names = CLAUDE_ENV_VARS
        .iter()
        .map(|name| ps_quote(name))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        r#"
$names = @({names})
$items = @()
foreach ($name in $names) {{
  foreach ($scope in @('Process','User','Machine')) {{
    $value = [Environment]::GetEnvironmentVariable($name, $scope)
    if (-not [string]::IsNullOrWhiteSpace($value)) {{
      $items += [pscustomobject]@{{ name = $name; scope = $scope.ToLowerInvariant(); value = [string]$value }}
    }}
  }}
}}
$items | ConvertTo-Json -Compress -Depth 3
"#
    );
    let json = run_powershell(&script)?;
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    if json.trim_start().starts_with('[') {
        serde_json::from_str::<Vec<RawEnvVarValue>>(&json)
            .map_err(|err| format!("解析 Claude 环境变量失败：{err}"))
    } else {
        serde_json::from_str::<RawEnvVarValue>(&json)
            .map(|item| vec![item])
            .map_err(|err| format!("解析 Claude 环境变量失败：{err}"))
    }
}

fn read_claude_env_values_macos() -> Vec<RawEnvVarValue> {
    let mut values = read_process_env_values();
    for name in CLAUDE_ENV_VARS {
        let Ok(output) = hidden_command_with_args("launchctl", &["getenv", name]).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            continue;
        }
        values.push(RawEnvVarValue {
            name: (*name).to_string(),
            scope: "launchctl".to_string(),
            value,
        });
    }
    values
}

fn clear_process_env(variables: &[String], cleared: &mut Vec<String>) {
    for name in variables {
        if env::var_os(name).is_some() {
            env::remove_var(name);
            cleared.push(name.clone());
        }
    }
}

fn clear_user_env_vars_windows(variables: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    let names = variables
        .iter()
        .map(|name| ps_quote(name))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        r#"
$names = @({names})
$cleared = @()
$machine = @()
foreach ($name in $names) {{
  $user = [Environment]::GetEnvironmentVariable($name, 'User')
  if (-not [string]::IsNullOrWhiteSpace($user)) {{
    [Environment]::SetEnvironmentVariable($name, $null, 'User')
    $cleared += $name
  }}
  $machineValue = [Environment]::GetEnvironmentVariable($name, 'Machine')
  if (-not [string]::IsNullOrWhiteSpace($machineValue)) {{
    $machine += $name
  }}
}}
[pscustomobject]@{{ cleared = $cleared; machine = $machine }} | ConvertTo-Json -Compress -Depth 3
"#
    );
    let json = run_powershell(&script)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|err| format!("解析环境变量清理结果失败：{err}"))?;
    Ok((
        json_string_array(&value["cleared"]),
        json_string_array(&value["machine"]),
    ))
}

fn clear_launchctl_env_vars_macos(variables: &[String]) -> Result<Vec<String>, String> {
    let mut cleared = Vec::new();
    for name in variables {
        let before = hidden_command_with_args("launchctl", &["getenv", name])
            .output()
            .map_err(|err| format!("读取 launchctl 环境变量失败：{err}"))?;
        if !before.status.success() || String::from_utf8_lossy(&before.stdout).trim().is_empty() {
            continue;
        }
        let output = hidden_command_with_args("launchctl", &["unsetenv", name])
            .output()
            .map_err(|err| format!("清理 launchctl 环境变量失败：{err}"))?;
        if output.status.success() {
            cleared.push(name.clone());
        }
    }
    Ok(cleared)
}

fn repair_user_path(directory: &Path, notes: &mut Vec<String>) -> Result<bool, String> {
    if cfg!(target_os = "windows") {
        return repair_user_path_windows(directory, notes);
    }
    if cfg!(target_os = "macos") {
        return repair_user_path_macos(directory, notes);
    }
    Err("当前平台暂不支持自动修复用户 PATH。".to_string())
}

fn repair_user_path_windows(directory: &Path, notes: &mut Vec<String>) -> Result<bool, String> {
    let directory_text = directory.to_string_lossy().to_string();
    let persistent_dirs = persistent_windows_path_dirs()?;
    let already_persistent = persistent_dirs
        .iter()
        .any(|existing| path_key(existing) == path_key(directory));
    let added = if already_persistent {
        false
    } else {
        append_user_path_windows(&directory_text)?;
        true
    };
    refresh_process_path(directory, notes);
    Ok(added)
}

fn repair_user_path_macos(directory: &Path, notes: &mut Vec<String>) -> Result<bool, String> {
    let directory_text = directory.to_string_lossy().to_string();
    let persistent_dirs = persistent_macos_path_dirs();
    let already_persistent = persistent_dirs
        .iter()
        .any(|existing| path_key(existing) == path_key(directory));
    let added = if already_persistent {
        false
    } else {
        append_user_path_macos(&directory_text)?;
        true
    };
    refresh_process_path(directory, notes);
    Ok(added)
}

fn persistent_windows_path_dirs() -> Result<Vec<PathBuf>, String> {
    let script = r#"
$items = @()
foreach ($scope in @('User','Machine')) {
  $value = [Environment]::GetEnvironmentVariable('Path', $scope)
  if (-not [string]::IsNullOrWhiteSpace($value)) {
    foreach ($part in $value -split ';') {
      if (-not [string]::IsNullOrWhiteSpace($part)) {
        $items += [pscustomobject]@{ path = [Environment]::ExpandEnvironmentVariables($part.Trim()) }
      }
    }
  }
}
$items | ConvertTo-Json -Compress -Depth 3
"#;
    let json = run_powershell(script)?;
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    #[derive(Deserialize)]
    struct PathItem {
        path: String,
    }
    let items = if json.trim_start().starts_with('[') {
        serde_json::from_str::<Vec<PathItem>>(&json)
    } else {
        serde_json::from_str::<PathItem>(&json).map(|item| vec![item])
    }
    .map_err(|err| format!("解析 PATH 失败：{err}"))?;
    Ok(items
        .into_iter()
        .map(|item| PathBuf::from(item.path))
        .collect())
}

fn append_user_path_windows(directory: &str) -> Result<(), String> {
    let script = format!(
        r#"
$dir = {dir}
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if ([string]::IsNullOrWhiteSpace($path)) {{
  $next = $dir
}} else {{
  $next = $path.TrimEnd(';') + ';' + $dir
}}
[Environment]::SetEnvironmentVariable('Path', $next, 'User')
try {{
  Add-Type -Namespace Win32 -Name NativeMethods -MemberDefinition '[DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);' | Out-Null
  $result = [UIntPtr]::Zero
  [void][Win32.NativeMethods]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
}} catch {{}}
"#,
        dir = ps_quote(directory)
    );
    run_powershell(&script).map(|_| ())
}

fn persistent_macos_path_dirs() -> Vec<PathBuf> {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
}

fn append_user_path_macos(directory: &str) -> Result<(), String> {
    let profile = macos_shell_profile_path()?;
    let existing = std::fs::read_to_string(&profile).unwrap_or_default();
    if existing.lines().any(|line| line.contains(directory)) {
        return Ok(());
    }
    if let Some(parent) = profile.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("创建 shell 配置目录失败：{err}"))?;
    }
    let addition = if profile
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == "config.fish")
        .unwrap_or(false)
    {
        format!(
            "\n# Added by CodeStudio Lite PATH repair\nfish_add_path {}\n",
            sh_single_quote(directory)
        )
    } else {
        let quoted = sh_double_quote(directory);
        format!("\n# Added by CodeStudio Lite PATH repair\nexport PATH={quoted}:$PATH\n")
    };
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&profile)
        .map_err(|err| format!("打开 shell 配置文件失败：{err}"))?;
    file.write_all(addition.as_bytes())
        .map_err(|err| format!("写入 shell 配置文件失败：{err}"))
}

fn macos_shell_profile_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "home directory not found".to_string())?;
    let shell = env::var("SHELL").unwrap_or_default();
    if shell.ends_with("/bash") {
        return Ok(home.join(".bash_profile"));
    }
    if shell.ends_with("/fish") {
        return Ok(home.join(".config").join("fish").join("config.fish"));
    }
    Ok(home.join(".zprofile"))
}

fn refresh_process_path(directory: &Path, notes: &mut Vec<String>) {
    let current = env::var_os("PATH").unwrap_or_default();
    let mut dirs = env::split_paths(&current).collect::<Vec<_>>();
    if dirs
        .iter()
        .any(|existing| path_key(existing) == path_key(directory))
    {
        return;
    }
    dirs.push(directory.to_path_buf());
    match env::join_paths(dirs) {
        Ok(joined) => env::set_var("PATH", joined),
        Err(err) => notes.push(format!("当前进程 PATH 刷新失败：{err}")),
    }
}

fn find_current_tool_status(tool_id: &str) -> Option<crate::core::types::ToolStatus> {
    detector::detect_environment().ok().and_then(|snapshot| {
        snapshot
            .tools
            .into_iter()
            .chain(snapshot.system)
            .find(|tool| tool.id == tool_id)
    })
}

fn tool_definition(tool_id: &str) -> Option<ToolDefinition> {
    ai_tools()
        .into_iter()
        .chain(system_tools())
        .find(|definition| definition.id == tool_id)
}

fn canonical_tool_id(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "claude-vscode" | "claude-code-vscode" | "claude-vs-code" => "claude".to_string(),
        other => other.to_string(),
    }
}

fn preview_env_value(name: &str, value: &str) -> String {
    if name.contains("TOKEN") || name.contains("KEY") {
        return mask_secret(value);
    }
    let trimmed = value.trim();
    if trimmed.len() > 96 {
        format!("{}...", &trimmed[..96])
    } else {
        trimmed.to_string()
    }
}

fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 8 {
        return "********".to_string();
    }
    format!("{}...{}", &trimmed[..4], &trimmed[trimmed.len() - 4..])
}

fn json_string_array(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(ToString::to_string))
            .collect(),
        serde_json::Value::String(item) => vec![item.clone()],
        _ => Vec::new(),
    }
}

fn path_key(path: &Path) -> String {
    if cfg!(target_os = "windows") {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    } else {
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    }
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sh_double_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
