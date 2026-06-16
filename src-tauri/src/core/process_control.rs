use crate::core::platform::{hidden_command_with_args, run_powershell};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct ProcessTerminationReport {
    pub total: u64,
    pub forced: u64,
    pub remaining: u64,
}

impl ProcessTerminationReport {
    pub fn note(&self, label: &str) -> Option<String> {
        if self.total == 0 {
            return None;
        }
        if self.forced > 0 {
            Some(format!(
                "检测到正在运行的 {label}，已强制结束 {} 个进程后继续更新。",
                self.forced
            ))
        } else {
            Some(format!("检测到正在运行的 {label}，已自动关闭后继续更新。"))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProcessTerminationReport {
    total: u64,
    forced: u64,
    remaining: u64,
}

pub fn close_processes_for_update(
    label: &str,
    process_names: &[&str],
    root_filter: Option<&Path>,
) -> Result<ProcessTerminationReport, String> {
    if process_names.is_empty() {
        return Ok(ProcessTerminationReport::default());
    }
    close_processes(label, process_names, &[], root_filter, 8)
}

pub fn close_appx_package_for_update(
    label: &str,
    package_identity: &str,
) -> Result<ProcessTerminationReport, String> {
    if !cfg!(target_os = "windows") {
        return Ok(ProcessTerminationReport::default());
    }

    let package_identity = ps_quote(package_identity);
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$PackageIdentity = {package_identity}
$packages = @(Get-AppxPackage -Name $PackageIdentity -ErrorAction SilentlyContinue)
$packageFullNames = @($packages | ForEach-Object {{ [string]$_.PackageFullName }})
$packageFamilyNames = @($packages | ForEach-Object {{ [string]$_.PackageFamilyName }})
$installRoots = @($packages | ForEach-Object {{
  try {{
    if ($_.InstallLocation) {{
      [System.IO.Path]::GetFullPath([string]$_.InstallLocation).TrimEnd('\')
    }}
  }} catch {{}}
}} | Where-Object {{ $_ }})
function Test-RootMatch([string]$PathValue) {{
  if (-not $PathValue) {{ return $false }}
  foreach ($root in $installRoots) {{
    try {{
      $full = [System.IO.Path]::GetFullPath($PathValue)
      if ($full.Equals($root, [System.StringComparison]::OrdinalIgnoreCase) -or
          $full.StartsWith($root + '\', [System.StringComparison]::OrdinalIgnoreCase)) {{
        return $true
      }}
    }} catch {{}}
  }}
  return $false
}}
function Test-IdentityMarker([string]$Value) {{
  if (-not $Value) {{ return $false }}
  if ($Value.IndexOf($PackageIdentity, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {{
    return $true
  }}
  foreach ($name in $packageFullNames) {{
    if ($name -and $Value.IndexOf($name, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {{
      return $true
    }}
  }}
  foreach ($name in $packageFamilyNames) {{
    if ($name -and $Value.IndexOf($name, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {{
      return $true
    }}
  }}
  return $false
}}
function Test-ProcessPackage([object]$Process) {{
  try {{
    $packageFullName = $Process.PSObject.Properties['PackageFullName']
    if ($null -ne $packageFullName -and (Test-IdentityMarker ([string]$packageFullName.Value))) {{
      return $true
    }}
  }} catch {{}}
  try {{
    $packageFamilyName = $Process.PSObject.Properties['PackageFamilyName']
    if ($null -ne $packageFamilyName -and (Test-IdentityMarker ([string]$packageFamilyName.Value))) {{
      return $true
    }}
  }} catch {{}}
  try {{
    if (Test-RootMatch ([string]$Process.Path)) {{ return $true }}
  }} catch {{}}
  return $false
}}
$targets = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {{
  Test-ProcessPackage $_
}})
$targets += @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {{
  $_.ProcessId -ne $PID -and (
    (Test-RootMatch ([string]$_.ExecutablePath)) -or
    (Test-IdentityMarker ([string]$_.CommandLine))
  )
}} | ForEach-Object {{
  Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
}})
$targets = @($targets | Where-Object {{ $null -ne $_ }} | Sort-Object -Property Id -Unique)
$targetIds = @($targets | ForEach-Object {{ $_.Id }})
foreach ($p in $targets) {{
  try {{
    if ($p.MainWindowHandle -ne 0) {{ [void]$p.CloseMainWindow() }}
  }} catch {{}}
}}
$deadline = (Get-Date).AddSeconds(12)
while ((Get-Date) -lt $deadline) {{
  Start-Sleep -Milliseconds 250
  $remaining = @()
  foreach ($id in $targetIds) {{
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -ne $p) {{ $remaining += $p }}
  }}
  if ($remaining.Count -eq 0) {{ break }}
}}
$remaining = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $remaining += $p }}
}}
$forced = 0
foreach ($p in $remaining) {{
  try {{
    Stop-Process -Id $p.Id -Force -ErrorAction Stop
    $forced += 1
  }} catch {{}}
}}
Start-Sleep -Milliseconds 1500
$still = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $still += $p }}
}}
[pscustomobject]@{{
  total = [int]$targetIds.Count
  forced = [int]$forced
  remaining = [int]$still.Count
}} | ConvertTo-Json -Compress
"#
    );
    let json = run_powershell(&script)?;
    let report: RawProcessTerminationReport = serde_json::from_str(&json)
        .map_err(|err| format!("解析 {label} AppX 进程关闭结果失败：{err}"))?;
    if report.remaining > 0 {
        return Err(format!("仍有 {label} 进程无法结束，未继续更新。"));
    }
    Ok(ProcessTerminationReport {
        total: report.total,
        forced: report.forced,
        remaining: report.remaining,
    })
}

pub fn close_processes(
    label: &str,
    process_names: &[&str],
    command_line_markers: &[&str],
    root_filter: Option<&Path>,
    graceful_seconds: u64,
) -> Result<ProcessTerminationReport, String> {
    if cfg!(target_os = "macos") {
        return close_processes_macos(
            label,
            process_names,
            command_line_markers,
            root_filter,
            graceful_seconds,
        );
    }
    if !cfg!(target_os = "windows") {
        return Ok(ProcessTerminationReport::default());
    }
    if process_names.is_empty() && command_line_markers.is_empty() {
        return Ok(ProcessTerminationReport::default());
    }

    let names = ps_array(process_names);
    let markers = ps_array(command_line_markers);
    let root = root_filter
        .map(|path| ps_quote(&path.to_string_lossy()))
        .unwrap_or_else(|| "$null".to_string());
    let wait_ms = graceful_seconds.max(1) * 1000;
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$Names = @({names})
$Markers = @({markers})
$RootFilter = {root}
if ($null -ne $RootFilter) {{
  try {{ $RootFilter = [System.IO.Path]::GetFullPath($RootFilter).TrimEnd('\') }} catch {{}}
}}
function Test-PathMatch([string]$PathValue) {{
  if ($null -eq $RootFilter) {{ return $true }}
  if (-not $PathValue) {{ return $false }}
  try {{
    $full = [System.IO.Path]::GetFullPath($PathValue)
    return $full.Equals($RootFilter, [System.StringComparison]::OrdinalIgnoreCase) -or
      $full.StartsWith($RootFilter + '\', [System.StringComparison]::OrdinalIgnoreCase)
  }} catch {{
    return $false
  }}
}}
function Test-MarkerMatch([string]$CommandLine) {{
  if ($Markers.Count -eq 0) {{ return $false }}
  if (-not $CommandLine) {{ return $false }}
  foreach ($marker in $Markers) {{
    if ($CommandLine.IndexOf($marker, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {{
      return $true
    }}
  }}
  return $false
}}
$targets = @()
if ($Names.Count -gt 0) {{
  foreach ($name in $Names) {{
    $clean = [System.IO.Path]::GetFileNameWithoutExtension([string]$name)
    $targets += @(Get-Process -Name $clean -ErrorAction SilentlyContinue | Where-Object {{
      Test-PathMatch ([string]$_.Path)
    }})
  }}
}}
if ($Markers.Count -gt 0) {{
  $targets += @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {{
    $_.ProcessId -ne $PID -and (Test-MarkerMatch ([string]$_.CommandLine))
  }} | ForEach-Object {{
    Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
  }})
}}
$targets = @($targets | Where-Object {{ $null -ne $_ }} | Sort-Object -Property Id -Unique)
$targetIds = @($targets | ForEach-Object {{ $_.Id }})
foreach ($p in $targets) {{
  try {{
    if ($p.MainWindowHandle -ne 0) {{ [void]$p.CloseMainWindow() }}
  }} catch {{}}
}}
$deadline = (Get-Date).AddMilliseconds({wait_ms})
while ((Get-Date) -lt $deadline) {{
  Start-Sleep -Milliseconds 250
  $remaining = @()
  foreach ($id in $targetIds) {{
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -ne $p) {{ $remaining += $p }}
  }}
  if ($remaining.Count -eq 0) {{ break }}
}}
$remaining = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $remaining += $p }}
}}
$forced = 0
foreach ($p in $remaining) {{
  try {{
    Stop-Process -Id $p.Id -Force -ErrorAction Stop
    $forced += 1
  }} catch {{}}
}}
Start-Sleep -Milliseconds 300
$still = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $still += $p }}
}}
[pscustomobject]@{{
  total = [int]$targetIds.Count
  forced = [int]$forced
  remaining = [int]$still.Count
}} | ConvertTo-Json -Compress
"#
    );
    let json = run_powershell(&script)?;
    let report: RawProcessTerminationReport = serde_json::from_str(&json)
        .map_err(|err| format!("解析 {label} 进程关闭结果失败：{err}"))?;
    if report.remaining > 0 {
        return Err(format!("仍有 {label} 进程无法结束，未继续更新。"));
    }
    Ok(ProcessTerminationReport {
        total: report.total,
        forced: report.forced,
        remaining: report.remaining,
    })
}

fn close_processes_macos(
    label: &str,
    process_names: &[&str],
    command_line_markers: &[&str],
    root_filter: Option<&Path>,
    graceful_seconds: u64,
) -> Result<ProcessTerminationReport, String> {
    if process_names.is_empty() && command_line_markers.is_empty() {
        return Ok(ProcessTerminationReport::default());
    }

    let target_ids = collect_macos_target_pids(process_names, command_line_markers, root_filter)?;
    if target_ids.is_empty() {
        return Ok(ProcessTerminationReport::default());
    }

    for name in process_names {
        quit_macos_app_by_name(name);
    }
    wait_for_macos_process_exit(&target_ids, Duration::from_secs(graceful_seconds.max(1)));

    let remaining_after_quit = target_ids
        .iter()
        .copied()
        .filter(|pid| macos_pid_alive(*pid))
        .collect::<Vec<_>>();
    for pid in &remaining_after_quit {
        let _ = hidden_command_with_args("kill", &["-TERM", &pid.to_string()]).output();
    }
    wait_for_macos_process_exit(&remaining_after_quit, Duration::from_secs(2));

    let remaining_after_term = remaining_after_quit
        .iter()
        .copied()
        .filter(|pid| macos_pid_alive(*pid))
        .collect::<Vec<_>>();
    let mut forced = 0;
    for pid in &remaining_after_term {
        let output = hidden_command_with_args("kill", &["-KILL", &pid.to_string()])
            .output()
            .map_err(|err| format!("强制结束 {label} 进程失败：{err}"))?;
        if output.status.success() {
            forced += 1;
        }
    }
    wait_for_macos_process_exit(&remaining_after_term, Duration::from_millis(500));

    let still_running = target_ids
        .iter()
        .copied()
        .filter(|pid| macos_pid_alive(*pid))
        .count() as u64;
    if still_running > 0 {
        return Err(format!("仍有 {label} 进程无法结束，未继续更新。"));
    }

    Ok(ProcessTerminationReport {
        total: target_ids.len() as u64,
        forced,
        remaining: still_running,
    })
}

fn collect_macos_target_pids(
    process_names: &[&str],
    command_line_markers: &[&str],
    root_filter: Option<&Path>,
) -> Result<Vec<u32>, String> {
    let mut ids = BTreeSet::new();
    for name in process_names {
        let clean_name = macos_process_name(name);
        if clean_name.is_empty() {
            continue;
        }
        for pid in pgrep_macos(&["-x", clean_name.as_str()])? {
            ids.insert(pid);
        }
    }
    for marker in command_line_markers {
        if marker.trim().is_empty() {
            continue;
        }
        for pid in pgrep_macos(&["-f", marker])? {
            ids.insert(pid);
        }
    }

    let current_pid = std::process::id();
    Ok(ids
        .into_iter()
        .filter(|pid| *pid != current_pid)
        .filter(|pid| macos_root_filter_matches(*pid, root_filter))
        .collect())
}

fn pgrep_macos(args: &[&str]) -> Result<Vec<u32>, String> {
    let output = hidden_command_with_args("pgrep", args)
        .output()
        .map_err(|err| format!("执行 pgrep 失败：{err}"))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect())
}

fn macos_root_filter_matches(pid: u32, root_filter: Option<&Path>) -> bool {
    let Some(root) = root_filter else {
        return true;
    };
    let Some(command_line) = macos_process_command_line(pid) else {
        return false;
    };
    let root = root.to_string_lossy();
    command_line.contains(root.as_ref())
}

fn macos_process_command_line(pid: u32) -> Option<String> {
    let pid = pid.to_string();
    let output = hidden_command_with_args("ps", &["-p", &pid, "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn quit_macos_app_by_name(name: &str) {
    let clean_name = macos_process_name(name);
    if clean_name.is_empty() {
        return;
    }
    let script = format!("tell application \"{clean_name}\" to quit");
    let _ = hidden_command_with_args("osascript", &["-e", &script]).output();
}

fn wait_for_macos_process_exit(pids: &[u32], timeout: Duration) {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if pids.iter().all(|pid| !macos_pid_alive(*pid)) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn macos_pid_alive(pid: u32) -> bool {
    let pid = pid.to_string();
    hidden_command_with_args("kill", &["-0", &pid])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn macos_process_name(name: &str) -> String {
    name.trim()
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .trim_end_matches(".bat")
        .trim_end_matches(".ps1")
        .to_string()
}

fn ps_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| ps_quote(value))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(windows)]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_process_names_trim_windows_shell_suffixes() {
        assert_eq!(macos_process_name("Claude.exe"), "Claude");
        assert_eq!(macos_process_name("tool.cmd"), "tool");
        assert_eq!(macos_process_name("Code - Insiders"), "Code - Insiders");
    }
}

#[cfg(not(windows))]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
