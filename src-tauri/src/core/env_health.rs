use crate::core::activity_log;
use crate::core::app_paths::display_path;
use crate::core::detector;
use crate::core::platform::{
    hidden_command, hidden_command_with_args, repair_candidate_for_command,
    resolve_command_on_path, run_powershell,
};
use crate::core::storage;
use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};
use crate::core::types::{
    ClearEnvironmentVariablesRequest, ClearEnvironmentVariablesResult, EnvironmentVariableConflict,
    PathRepairHint, RepairToolPathRequest, RepairToolPathResult, Severity,
};
use serde::{Deserialize, Serialize};
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
const PATH_REPAIR_DIRS_STATE_KEY: &str = "env_health.path_repair_dirs";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedPathRepairDirsState {
    directories: Vec<String>,
}

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
            "Found {} in a common install directory, but the current PATH cannot resolve command {}.",
            display_path(&candidate),
            definition.command
        ),
    })
}

pub fn repair_tool_path_for_install(tool_id: &str) -> Result<Option<RepairToolPathResult>, String> {
    let definition = tool_definition(tool_id)
        .ok_or_else(|| format!("Tool '{tool_id}' is not allowed for PATH repair."))?;
    if resolve_command_on_path(definition.command).is_some() {
        return Ok(None);
    }
    if repair_candidate_for_command(definition.command).is_none() {
        return Ok(None);
    }
    repair_tool_path(RepairToolPathRequest {
        tool_id: definition.id.to_string(),
        confirm: true,
    })
    .map(Some)
}

pub fn restore_persisted_path_repairs() -> Result<usize, String> {
    let directories = load_persisted_path_repair_dirs()?;
    let mut restored = 0;
    let mut notes = Vec::new();
    for directory in directories {
        if !directory.is_dir() {
            continue;
        }
        if refresh_process_path(&directory, &mut notes) {
            restored += 1;
        }
        if cfg!(target_os = "macos") {
            refresh_launchctl_path_macos(&directory, &mut notes);
        }
    }

    if restored > 0 {
        let _ = activity_log::append(
            Severity::Info,
            format!("Restored {restored} PATH repair directories for this app session."),
        );
    }
    for note in notes {
        let _ = activity_log::append(Severity::Warning, note);
    }
    Ok(restored)
}

pub fn repair_tool_path(request: RepairToolPathRequest) -> Result<RepairToolPathResult, String> {
    if !request.confirm {
        return Err("Refused: repairing PATH requires explicit confirmation.".to_string());
    }
    let definition = tool_definition(&request.tool_id)
        .ok_or_else(|| format!("Tool '{}' is not allowed for PATH repair.", request.tool_id))?;
    if resolve_command_on_path(definition.command).is_some() {
        return Ok(RepairToolPathResult {
            success: true,
            tool_id: definition.id.to_string(),
            tool_name: definition.name.to_string(),
            added_path: None,
            message: format!(
                "{} is already available on the current PATH.",
                definition.name
            ),
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
            message: format!(
                "Could not find {} in common install directories.",
                definition.name
            ),
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
            format!("Added {} to the user PATH.", display_path(&directory))
        } else {
            format!(
                "{} is already in the persistent PATH; refreshed the current process PATH.",
                display_path(&directory)
            )
        }
    } else {
        format!(
            "Tried to repair PATH, but {} still did not pass verification. It may take effect in a new terminal or after restarting the app.",
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
    if profile
        .auth_ref
        .as_deref()
        .map(str::trim)
        .filter(|auth_ref| !auth_ref.is_empty())
        .is_some()
    {
        expected.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            ExpectedEnvValue::StoredSecret,
        );
        expected.insert(
            "ANTHROPIC_API_KEY".to_string(),
            ExpectedEnvValue::StoredSecret,
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
        return Err(
            "Refused: clearing environment variables requires explicit confirmation.".to_string(),
        );
    }
    if canonical_tool_id(&request.tool_id) != CLAUDE_TOOL_ID {
        return Err(
            "Only Claude-related environment variables can be cleared right now.".to_string(),
        );
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
        return Err("No Claude environment variables are available to clear.".to_string());
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
            skipped.push(format!("{name} exists as a machine-level environment variable and must be cleared manually as administrator."));
        }
    } else if cfg!(target_os = "macos") {
        let launchctl_cleared = clear_launchctl_env_vars_macos(&variables)?;
        for name in launchctl_cleared {
            if !cleared.contains(&name) {
                cleared.push(name);
            }
        }
        skipped.push(
            "Cleared current-process and launchctl global variables. If shell startup files still export them, remove those manually."
                .to_string(),
        );
    } else {
        skipped.push("Only this process environment was cleared on the current platform; shell startup files must be checked manually.".to_string());
    }

    let conflicts = claude_env_conflicts_without_profile();
    let success =
        conflicts.is_empty() || conflicts.iter().all(|conflict| conflict.scope == "machine");
    let message = if success {
        "Claude global environment variables were cleared.".to_string()
    } else {
        "Writable environment variables were cleared, but conflicts are still detected.".to_string()
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
    StoredSecret,
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
                ExpectedEnvValue::StoredSecret => false,
                ExpectedEnvValue::Absent => false,
            };
            if matches {
                return None;
            }
            let message = match expected_value {
                ExpectedEnvValue::StoredSecret => format!(
                    "{} affects Claude API connections and may override the saved Provider API key.",
                    value.name
                ),
                _ => format!(
                    "{} affects Claude API connections and does not match the current CodeStudio configuration.",
                    value.name
                ),
            };
            let expected_value_preview = match expected_value {
                ExpectedEnvValue::Exact(expected) if !expected.trim().is_empty() => {
                    Some(preview_env_value(&value.name, &expected))
                }
                ExpectedEnvValue::StoredSecret => Some("saved Provider API key".to_string()),
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
                message,
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
            .map_err(|err| format!("Failed to parse Claude environment variables: {err}"))
    } else {
        serde_json::from_str::<RawEnvVarValue>(&json)
            .map(|item| vec![item])
            .map_err(|err| format!("Failed to parse Claude environment variables: {err}"))
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
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse environment variable cleanup result: {err}"))?;
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
            .map_err(|err| format!("Failed to read launchctl environment variable: {err}"))?;
        if !before.status.success() || String::from_utf8_lossy(&before.stdout).trim().is_empty() {
            continue;
        }
        let output = hidden_command_with_args("launchctl", &["unsetenv", name])
            .output()
            .map_err(|err| format!("Failed to clear launchctl environment variable: {err}"))?;
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
    Err("Automatic user PATH repair is not supported on the current platform.".to_string())
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
    remember_path_repair_dir(directory, notes);
    let _ = refresh_process_path(directory, notes);
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
    remember_path_repair_dir(directory, notes);
    let _ = refresh_process_path(directory, notes);
    refresh_launchctl_path_macos(directory, notes);
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
    .map_err(|err| format!("Failed to parse PATH: {err}"))?;
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
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create shell config directory: {err}"))?;
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
        .map_err(|err| format!("Failed to open shell config file: {err}"))?;
    file.write_all(addition.as_bytes())
        .map_err(|err| format!("Failed to write shell config file: {err}"))
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

fn remember_path_repair_dir(directory: &Path, notes: &mut Vec<String>) {
    if let Err(err) = save_persisted_path_repair_dir(directory) {
        notes.push(format!(
            "Failed to remember PATH repair for app restart: {err}"
        ));
    }
}

fn load_persisted_path_repair_dirs() -> Result<Vec<PathBuf>, String> {
    let Some(json) = storage::load_state_json(PATH_REPAIR_DIRS_STATE_KEY)? else {
        return Ok(Vec::new());
    };
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let state = serde_json::from_str::<PersistedPathRepairDirsState>(&json)
        .map_err(|err| format!("Failed to parse persisted PATH repairs: {err}"))?;
    Ok(dedupe_path_dirs(
        state.directories.into_iter().map(PathBuf::from),
    ))
}

fn save_persisted_path_repair_dir(directory: &Path) -> Result<(), String> {
    let mut directories = load_persisted_path_repair_dirs()?;
    directories.push(directory.to_path_buf());
    let state = PersistedPathRepairDirsState {
        directories: dedupe_path_dirs(directories)
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
    };
    let json = serde_json::to_string(&state).map_err(|err| err.to_string())?;
    storage::save_state_json(PATH_REPAIR_DIRS_STATE_KEY, &json)
}

fn dedupe_path_dirs(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for path in paths {
        let key = path_key(&path);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        deduped.push(path);
    }
    deduped
}

fn refresh_process_path(directory: &Path, notes: &mut Vec<String>) -> bool {
    let current = env::var_os("PATH").unwrap_or_default();
    let mut dirs = env::split_paths(&current).collect::<Vec<_>>();
    if dirs
        .iter()
        .any(|existing| path_key(existing) == path_key(directory))
    {
        return false;
    }
    dirs.push(directory.to_path_buf());
    match env::join_paths(dirs) {
        Ok(joined) => {
            env::set_var("PATH", joined);
            true
        }
        Err(err) => {
            notes.push(format!("Failed to refresh the current process PATH: {err}"));
            false
        }
    }
}

fn refresh_launchctl_path_macos(directory: &Path, notes: &mut Vec<String>) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let mut launchctl_dirs = launchctl_path_dirs_macos(notes);
    if !launchctl_dirs
        .iter()
        .any(|existing| path_key(existing) == path_key(directory))
    {
        launchctl_dirs.push(directory.to_path_buf());
    }
    let path_value = match env::join_paths(launchctl_dirs) {
        Ok(value) => value.to_string_lossy().to_string(),
        Err(err) => {
            notes.push(format!("Failed to build launchctl PATH: {err}"));
            return;
        }
    };
    let output = hidden_command("launchctl")
        .args(["setenv", "PATH", path_value.as_str()])
        .output();
    match output {
        Ok(output) if output.status.success() => {}
        Ok(output) => notes.push(format!(
            "Failed to refresh the macOS GUI session PATH: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(err) => notes.push(format!("Failed to start launchctl: {err}")),
    }
}

fn launchctl_path_dirs_macos(notes: &mut Vec<String>) -> Vec<PathBuf> {
    let output = hidden_command("launchctl")
        .args(["getenv", "PATH"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !value.is_empty() {
                return env::split_paths(&value).collect();
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if !stderr.is_empty() {
                notes.push(format!(
                    "Failed to read the macOS GUI session PATH: {stderr}"
                ));
            }
        }
        Err(err) => notes.push(format!("Failed to read launchctl PATH: {err}")),
    }

    env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn claude_env_health_does_not_read_keychain_secrets() {
        let source = include_str!("env_health.rs");

        assert!(!source.contains(&format!("{}{}", "load_keychain", "_secret")));
    }

    #[test]
    fn persisted_path_repair_dirs_are_deduped_by_path_key() {
        let dirs = dedupe_path_dirs([
            PathBuf::from("/Users/test/.npm-global/bin"),
            PathBuf::from("/Users/test/.npm-global/bin/"),
            PathBuf::from("/opt/homebrew/bin"),
        ]);

        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/Users/test/.npm-global/bin"),
                PathBuf::from("/opt/homebrew/bin"),
            ]
        );
    }
}
