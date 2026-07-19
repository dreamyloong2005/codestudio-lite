use crate::core::types::Severity;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{hidden_command, powershell_exe, run_powershell};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMsixPackage {
    pub path: String,
    pub version: String,
    pub arch: Option<String>,
    pub package_family_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstalledMacosApp {
    pub path: String,
    pub version: String,
    pub bundle_identifier: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MacosDmgInstallReport {
    pub installed: Option<InstalledMacosApp>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsixInstallReport {
    pub success: bool,
    pub message: String,
    pub installed: Option<InstalledMsixPackage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsixRemoveReport {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsixPayloadRemoveReport {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub removed_payloads: Vec<String>,
    #[serde(default)]
    pub remaining_payloads: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PackageCapability {
    pub id: String,
    pub label: String,
    pub status: Severity,
    pub detail: String,
}

pub struct PortableAppRegistration<'a> {
    pub display_name: &'a str,
    pub publisher: &'a str,
    pub install_root: &'a Path,
    pub executable_name: &'a str,
    pub shortcut_name: &'a str,
    pub version: &'a str,
    pub uninstall_key: &'a str,
}

pub fn detect_msix_package(package_identity: &str) -> Option<InstalledMsixPackage> {
    if !cfg!(target_os = "windows") {
        return None;
    }

    let script = format!(
        r#"
$p = Get-AppxPackage -Name {name} -ErrorAction SilentlyContinue |
  Sort-Object -Property Version -Descending |
  Select-Object -First 1
if ($null -ne $p) {{
  [pscustomobject]@{{
    path = [string]$p.InstallLocation
    version = [string]$p.Version
    arch = $null
    packageFamilyName = [string]$p.PackageFamilyName
  }} | ConvertTo-Json -Compress
}}
"#,
        name = ps_quote(package_identity)
    );
    let output = run_powershell(&script).ok()?;
    if output.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&output).ok()
}

pub fn detect_first_msix_package(package_identities: &[&str]) -> Option<InstalledMsixPackage> {
    package_identities
        .iter()
        .find_map(|package_identity| detect_msix_package(package_identity))
        // Fallback for environments where the Get-AppxPackage cmdlet returns
        // nothing even though the package is registered (observed on some VMs
        // where the Appx module/profile load behaves differently). The AppModel
        // package repository is the same registry hive Get-AppxPackage reads
        // from, so reading it directly via Get-ItemProperty avoids the cmdlet
        // entirely. Scans for any Claude package on a supported architecture regardless of the
        // package-identity list, since the registry key is keyed by full
        // PackageFullName.
        .or_else(detect_claude_msix_package_from_registry)
}

/// Read the AppModel package repository registry directly to detect a Claude
/// MSIX install without relying on the Get-AppxPackage cmdlet. Returns the
/// highest-version Claude x64 or arm64 package found under the current user's
/// package repository. Used as a fallback when Get-AppxPackage yields nothing.
fn detect_claude_msix_package_from_registry() -> Option<InstalledMsixPackage> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let script = r#"
$root = "HKCU:\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages"
if (-not (Test-Path $root)) { exit 0 }
$best = Get-ChildItem $root -ErrorAction SilentlyContinue | Where-Object {
  $_.PSChildName -match '^Claude_[^_]+_(x64|arm64)__'
} | Sort-Object -Descending | Select-Object -First 1
if (-not $best) { exit 0 }
$props = Get-ItemProperty $best.PSPath -ErrorAction SilentlyContinue
$id = [string]$best.PSChildName
$parts = $id -split '_'
$name = $parts[0]
$version = $parts[1]
$pfn = $name + "_" + ($id -replace '^.*_(x64|arm64)__','')
[pscustomobject]@{
  path = [string]$props.PackageRootFolder
  version = $version
  arch = $null
  packageFamilyName = $pfn
} | ConvertTo-Json -Compress
"#;
    let output = run_powershell(script).ok()?;
    if output.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&output).ok()
}

pub fn detect_macos_app(
    candidate_paths: &[PathBuf],
    bundle_identifier: Option<&str>,
) -> Option<InstalledMacosApp> {
    if !cfg!(target_os = "macos") {
        return None;
    }

    for path in candidate_paths {
        if !path.exists() {
            continue;
        }

        let detected_bundle_id = read_macos_plist_value(path, "CFBundleIdentifier");
        if let (Some(expected), Some(actual)) = (bundle_identifier, detected_bundle_id.as_deref()) {
            if actual != expected {
                continue;
            }
        }

        return Some(InstalledMacosApp {
            path: path.to_string_lossy().to_string(),
            version: read_macos_plist_value(path, "CFBundleShortVersionString")
                .unwrap_or_else(|| "installed".to_string()),
            bundle_identifier: detected_bundle_id.or_else(|| bundle_identifier.map(str::to_string)),
        });
    }

    None
}

pub fn macos_bundle_executable_name(app: &Path) -> Option<String> {
    read_macos_plist_value(app, "CFBundleExecutable")
        .filter(|value| !value.is_empty() && !value.contains('/') && !value.contains('\\'))
}

pub fn macos_app_executable_name(app: &Path) -> Option<String> {
    macos_bundle_executable_name(app).or_else(|| {
        app.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
            .then(|| "Codex".to_string())
    })
}

pub fn macos_app_running(app: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    macos_app_process_names(app)
        .iter()
        .any(|process_name| macos_process_running(process_name))
}

pub fn install_msix_package(
    path: &Path,
    package_identity: &str,
) -> Result<MsixInstallReport, String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
try {{
  $cmd = Get-Command Add-AppxPackage -ErrorAction Stop
  $args = @{{ ErrorAction = 'Stop' }}
  if ($cmd.Parameters.ContainsKey('LiteralPath')) {{
    $args['LiteralPath'] = {path}
  }} else {{
    $args['Path'] = {path}
  }}
  if ($cmd.Parameters.ContainsKey('ForceUpdateFromAnyVersion')) {{
    $args['ForceUpdateFromAnyVersion'] = $true
  }}
  if ($cmd.Parameters.ContainsKey('ForceApplicationShutdown')) {{
    $args['ForceApplicationShutdown'] = $true
  }}
  Add-AppxPackage @args
  $p = Get-AppxPackage -Name {name} -ErrorAction SilentlyContinue |
    Sort-Object -Property Version -Descending |
    Select-Object -First 1
  [pscustomobject]@{{
    success = $true
    message = 'Add-AppxPackage succeeded'
    installed = if ($null -ne $p) {{
      [pscustomobject]@{{
        path = [string]$p.InstallLocation
        version = [string]$p.Version
        arch = $null
        packageFamilyName = [string]$p.PackageFamilyName
      }}
    }} else {{ $null }}
  }} | ConvertTo-Json -Compress -Depth 4
}} catch {{
  [pscustomobject]@{{
    success = $false
    message = [string]$_.Exception.Message
    installed = $null
  }} | ConvertTo-Json -Compress -Depth 4
}}
"#,
        path = ps_quote(&path.to_string_lossy()),
        name = ps_quote(package_identity)
    );
    let json = run_powershell(&script)?;
    serde_json::from_str(&json).map_err(|err| format!("Failed to parse MSIX install result: {err}"))
}

pub fn remove_msix_package(package_identity: &str) -> Result<MsixRemoveReport, String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$notes = @()
try {{
  $packages = Get-AppxPackage -Name {name} -ErrorAction SilentlyContinue
  if (-not $packages) {{
    [pscustomobject]@{{ success = $true; message = 'MSIX package was not installed'; notes = $notes }} | ConvertTo-Json -Compress
    exit 0
  }}
  foreach ($p in $packages) {{
    Remove-AppxPackage -Package $p.PackageFullName -ErrorAction Stop
  }}
  [pscustomobject]@{{ success = $true; message = 'Remove-AppxPackage succeeded'; notes = $notes }} | ConvertTo-Json -Compress
}} catch {{
  [pscustomobject]@{{ success = $false; message = [string]$_.Exception.Message; notes = $notes }} | ConvertTo-Json -Compress
}}
"#,
        name = ps_quote(package_identity)
    );
    let json = run_powershell(&script)?;
    serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse MSIX uninstall result: {err}"))
}

pub fn remove_first_msix_package(package_identities: &[&str]) -> Result<MsixRemoveReport, String> {
    if !cfg!(target_os = "windows") {
        return Err("MSIX uninstall is only supported on Windows.".to_string());
    }
    let Some(installed) = detect_first_msix_package(package_identities) else {
        return Ok(MsixRemoveReport {
            success: true,
            message: "MSIX package was not installed".to_string(),
            notes: Vec::new(),
        });
    };
    let Some(package_name) = installed
        .package_family_name
        .as_deref()
        .and_then(|family| {
            package_identities
                .iter()
                .find(|identity| family.starts_with(&format!("{identity}_")))
                .copied()
        })
        .or_else(|| {
            package_identities
                .iter()
                .find(|identity| installed.path.contains(**identity))
                .copied()
        })
    else {
        return Err("Unable to resolve packaged app identity for uninstall.".to_string());
    };
    remove_msix_package(package_name)
}

pub fn remove_claude_msix_payloads(
    package_identities: &[&str],
    publisher_suffix: &str,
) -> Result<MsixPayloadRemoveReport, String> {
    if !cfg!(target_os = "windows") {
        return Ok(MsixPayloadRemoveReport {
            success: true,
            message: "MSIX/AppX payload cleanup is only needed on Windows.".to_string(),
            notes: Vec::new(),
            removed_payloads: Vec::new(),
            remaining_payloads: Vec::new(),
        });
    }
    let script = claude_msix_payload_cleanup_script(package_identities, publisher_suffix);
    let json = run_powershell(&script)?;
    serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse MSIX payload cleanup result: {err}"))
}

fn claude_msix_payload_cleanup_script(
    package_identities: &[&str],
    publisher_suffix: &str,
) -> String {
    let identity_prefixes = package_identities
        .iter()
        .map(|identity| ps_quote(identity))
        .collect::<Vec<_>>()
        .join(", ");
    r#"
$ErrorActionPreference = 'Continue'
$root = 'C:\Program Files\WindowsApps'
$identityPrefixes = @(__IDENTITY_PREFIXES__)
$publisherSuffix = __PUBLISHER_SUFFIX__
$notes = @()
$removedPayloads = @()
$remainingPayloads = @()
$scanFailed = $false

function Test-ClaudePackageDirectoryName {
  param([System.IO.DirectoryInfo]$Dir)
  if ($null -eq $Dir) { return $false }
  $rootFull = [System.IO.Path]::GetFullPath($root).TrimEnd('\')
  $dirFull = [System.IO.Path]::GetFullPath($Dir.FullName).TrimEnd('\')
  if (-not $dirFull.StartsWith($rootFull + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
    return $false
  }
  $matchedIdentity = $false
  foreach ($identity in $identityPrefixes) {
    $expectedPrefix = $identity + '_'
    if ($Dir.Name.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase) -and
        $Dir.Name.EndsWith('__' + $publisherSuffix, [System.StringComparison]::OrdinalIgnoreCase) -and
        $Dir.Name -match '_(x64|arm64)__') {
      $matchedIdentity = $true
      break
    }
  }
  if (-not $matchedIdentity) { return $false }
  return $true
}

function Test-ClaudeCompletePayloadDirectory {
  param([System.IO.DirectoryInfo]$Dir)
  if (-not (Test-ClaudePackageDirectoryName $Dir)) { return $false }
  $dirFull = [System.IO.Path]::GetFullPath($Dir.FullName).TrimEnd('\')
  if (-not (Test-Path -LiteralPath (Join-Path $dirFull 'AppxManifest.xml') -PathType Leaf)) {
    return $false
  }
  if (-not (Test-Path -LiteralPath (Join-Path $dirFull 'app\Claude.exe') -PathType Leaf)) {
    return $false
  }
  return $true
}

function Test-ClaudePartialPayloadDirectory {
  param([System.IO.DirectoryInfo]$Dir)
  if (-not (Test-ClaudePackageDirectoryName $Dir)) { return $false }
  $dirFull = [System.IO.Path]::GetFullPath($Dir.FullName).TrimEnd('\')
  return (
    (Test-Path -LiteralPath (Join-Path $dirFull 'app') -PathType Container) -or
    (Test-Path -LiteralPath (Join-Path $dirFull 'app\resources') -PathType Container) -or
    (Test-Path -LiteralPath (Join-Path $dirFull 'app\resources\cowork-svc.exe') -PathType Leaf)
  )
}

function Test-ClaudePayloadDirectory {
  param([System.IO.DirectoryInfo]$Dir)
  return ((Test-ClaudeCompletePayloadDirectory $Dir) -or (Test-ClaudePartialPayloadDirectory $Dir))
}

function Invoke-ElevatedClaudePayloadCleanup {
  param(
    [string[]]$Paths,
    [bool]$ScanRoot
  )
  $elevatedNotes = @()
  $elevatedRemoved = @()
  $elevatedRemaining = @()
  $elevatedScanSucceeded = (-not $ScanRoot)
  if ((-not $ScanRoot) -and ($null -eq $Paths -or $Paths.Count -eq 0)) {
    return [pscustomobject]@{ notes = @(); removedPayloads = @(); remainingPayloads = @() }
  }

  $workRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('codestudio-lite-claude-cleanup-' + [guid]::NewGuid().ToString('N'))
  $targetsPath = Join-Path $workRoot 'targets.json'
  $resultPath = Join-Path $workRoot 'result.json'
  $scriptPath = Join-Path $workRoot 'cleanup.ps1'
  try {
    New-Item -ItemType Directory -Path $workRoot -Force | Out-Null
    @($Paths) | ConvertTo-Json -Compress | Set-Content -LiteralPath $targetsPath -Encoding UTF8
    @'
param(
  [Parameter(Mandatory=$true)][string]$TargetsPath,
  [Parameter(Mandatory=$true)][string]$ResultPath,
  [switch]$ScanRoot
)
$ErrorActionPreference = 'Continue'
$root = 'C:\Program Files\WindowsApps'
$identityPrefixes = @(__IDENTITY_PREFIXES__)
$publisherSuffix = __PUBLISHER_SUFFIX__
$notes = @()
$removedPayloads = @()
$remainingPayloads = @()
$scanSucceeded = (-not $ScanRoot)

function Test-ClaudePackageDirectoryName {
  param([System.IO.DirectoryInfo]$Dir)
  if ($null -eq $Dir) { return $false }
  $rootFull = [System.IO.Path]::GetFullPath($root).TrimEnd('\')
  $dirFull = [System.IO.Path]::GetFullPath($Dir.FullName).TrimEnd('\')
  if (-not $dirFull.StartsWith($rootFull + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
    return $false
  }
  $matchedIdentity = $false
  foreach ($identity in $identityPrefixes) {
    $expectedPrefix = $identity + '_'
    if ($Dir.Name.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase) -and
        $Dir.Name.EndsWith('__' + $publisherSuffix, [System.StringComparison]::OrdinalIgnoreCase) -and
        $Dir.Name -match '_(x64|arm64)__') {
      $matchedIdentity = $true
      break
    }
  }
  if (-not $matchedIdentity) { return $false }
  return $true
}

function Test-ClaudeCompletePayloadDirectory {
  param([System.IO.DirectoryInfo]$Dir)
  if (-not (Test-ClaudePackageDirectoryName $Dir)) { return $false }
  $dirFull = [System.IO.Path]::GetFullPath($Dir.FullName).TrimEnd('\')
  if (-not (Test-Path -LiteralPath (Join-Path $dirFull 'AppxManifest.xml') -PathType Leaf)) {
    return $false
  }
  if (-not (Test-Path -LiteralPath (Join-Path $dirFull 'app\Claude.exe') -PathType Leaf)) {
    return $false
  }
  return $true
}

function Test-ClaudePartialPayloadDirectory {
  param([System.IO.DirectoryInfo]$Dir)
  if (-not (Test-ClaudePackageDirectoryName $Dir)) { return $false }
  $dirFull = [System.IO.Path]::GetFullPath($Dir.FullName).TrimEnd('\')
  return (
    (Test-Path -LiteralPath (Join-Path $dirFull 'app') -PathType Container) -or
    (Test-Path -LiteralPath (Join-Path $dirFull 'app\resources') -PathType Container) -or
    (Test-Path -LiteralPath (Join-Path $dirFull 'app\resources\cowork-svc.exe') -PathType Leaf)
  )
}

function Test-ClaudePayloadDirectory {
  param([System.IO.DirectoryInfo]$Dir)
  return ((Test-ClaudeCompletePayloadDirectory $Dir) -or (Test-ClaudePartialPayloadDirectory $Dir))
}

if ($ScanRoot) {
  try {
    $targetDirs = Get-ChildItem -LiteralPath $root -Directory -ErrorAction Stop |
      Where-Object { Test-ClaudePayloadDirectory $_ }
    $scanSucceeded = $true
  } catch {
    $notes += ('Elevated scan could not enumerate WindowsApps while verifying Claude Desktop package files: ' + [string]$_.Exception.Message)
    $targetDirs = @()
  }
} else {
  try {
    $rawTargets = Get-Content -LiteralPath $TargetsPath -Raw -ErrorAction Stop
    $parsedTargets = if ([string]::IsNullOrWhiteSpace($rawTargets)) { @() } else { $rawTargets | ConvertFrom-Json }
    $targets = @($parsedTargets)
  } catch {
    $notes += ('Failed to read elevated Claude Desktop cleanup targets: ' + [string]$_.Exception.Message)
    $targets = @()
  }
  $targetDirs = foreach ($target in $targets) {
    Get-Item -LiteralPath ([string]$target) -ErrorAction SilentlyContinue
  }
}

foreach ($dir in $targetDirs) {
  $targetPath = if ($null -ne $dir) { [string]$dir.FullName } else { '' }
  if (-not (Test-ClaudePayloadDirectory $dir)) {
    $notes += ('Skipped unsafe Claude Desktop cleanup target: ' + $targetPath)
    if (Test-Path -LiteralPath $targetPath) {
      $remainingPayloads += $targetPath
    }
    continue
  }

  try {
    Remove-Item -LiteralPath $dir.FullName -Recurse -Force -ErrorAction Stop
  } catch {
    $notes += ('Elevated direct removal failed for {0}: {1}' -f $dir.FullName, [string]$_.Exception.Message)
    $takeown = Join-Path $env:SystemRoot 'System32\takeown.exe'
    $icacls = Join-Path $env:SystemRoot 'System32\icacls.exe'
    if (Test-Path -LiteralPath $takeown) {
      Start-Process -FilePath $takeown -ArgumentList @('/F', $dir.FullName, '/R', '/D', 'Y') -WindowStyle Hidden -Wait -PassThru | Out-Null
    }
    if (Test-Path -LiteralPath $icacls) {
      $principal = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
      $grant = $principal + ':(OI)(CI)F'
      Start-Process -FilePath $icacls -ArgumentList @($dir.FullName, '/grant', $grant, '/T', '/C') -WindowStyle Hidden -Wait -PassThru | Out-Null
    }
    try {
      Remove-Item -LiteralPath $dir.FullName -Recurse -Force -ErrorAction Stop
    } catch {
      $notes += ('Elevated permission repair removal failed for {0}: {1}' -f $dir.FullName, [string]$_.Exception.Message)
    }
  }

  if (Test-Path -LiteralPath $dir.FullName) {
    $remainingPayloads += [string]$dir.FullName
  } else {
    $removedPayloads += [string]$dir.FullName
  }
}

[pscustomobject]@{
  notes = @($notes)
  removedPayloads = @($removedPayloads)
  remainingPayloads = @($remainingPayloads)
  scanSucceeded = [bool]$scanSucceeded
} | ConvertTo-Json -Compress -Depth 4 | Set-Content -LiteralPath $ResultPath -Encoding UTF8
'@ | Set-Content -LiteralPath $scriptPath -Encoding UTF8

    $powershellCandidates = @(
      $env:WINDIR,
      $env:SystemRoot,
      'C:\Windows'
    ) | Where-Object { $_ } | ForEach-Object { Join-Path $_ 'System32\WindowsPowerShell\v1.0\powershell.exe' }
    $powershell = $powershellCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (-not $powershell) { $powershell = 'powershell.exe' }
    $elevatedArgs = @(
      '-NoLogo',
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      $scriptPath,
      '-TargetsPath',
      $targetsPath,
      '-ResultPath',
      $resultPath
    )
    if ($ScanRoot) {
      $elevatedArgs += '-ScanRoot'
    }
    $process = Start-Process -FilePath $powershell -ArgumentList $elevatedArgs -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) {
      $elevatedNotes += ('Elevated Claude Desktop cleanup exited with code ' + $process.ExitCode)
    }
    if (Test-Path -LiteralPath $resultPath) {
      $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
      $elevatedNotes += @($result.notes)
      $elevatedRemoved += @($result.removedPayloads)
      $elevatedRemaining += @($result.remainingPayloads)
      $elevatedScanSucceeded = [bool]$result.scanSucceeded
    } else {
      $elevatedNotes += 'Elevated Claude Desktop cleanup did not produce a verification report.'
      foreach ($path in $Paths) {
        if (Test-Path -LiteralPath $path) {
          $elevatedRemaining += [string]$path
        }
      }
    }
  } catch {
    $elevatedNotes += ('Failed to run elevated Claude Desktop cleanup: ' + [string]$_.Exception.Message)
    foreach ($path in $Paths) {
      if (Test-Path -LiteralPath $path) {
        $elevatedRemaining += [string]$path
      }
    }
  } finally {
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
  }

  [pscustomobject]@{
    notes = @($elevatedNotes)
    removedPayloads = @($elevatedRemoved)
    remainingPayloads = @($elevatedRemaining)
    scanSucceeded = [bool]$elevatedScanSucceeded
  }
}

if (Test-Path -LiteralPath $root -PathType Container) {
  try {
    $payloads = Get-ChildItem -LiteralPath $root -Directory -ErrorAction Stop |
      Where-Object { Test-ClaudePayloadDirectory $_ }
  } catch {
    $scanFailed = $true
    $notes += ('Failed to enumerate WindowsApps while verifying Claude Desktop package files: ' + [string]$_.Exception.Message)
    $payloads = @()
  }

  foreach ($dir in $payloads) {
    try {
      Remove-Item -LiteralPath $dir.FullName -Recurse -Force -ErrorAction Stop
    } catch {
      $notes += ('Failed to remove Claude Desktop MSIX/AppX payload {0}: {1}' -f $dir.FullName, [string]$_.Exception.Message)
    }
    if (Test-Path -LiteralPath $dir.FullName) {
      $remainingPayloads += [string]$dir.FullName
    } else {
      $removedPayloads += [string]$dir.FullName
    }
  }
}

if ($remainingPayloads.Count -gt 0 -or $scanFailed) {
  $elevated = Invoke-ElevatedClaudePayloadCleanup -Paths @($remainingPayloads) -ScanRoot ([bool]$scanFailed)
  $notes += @($elevated.notes)
  $removedPayloads += @($elevated.removedPayloads)
  $remainingPayloads = @($elevated.remainingPayloads)
  if ($scanFailed -and [bool]$elevated.scanSucceeded) {
    $scanFailed = $false
  }
}

$success = (($remainingPayloads.Count -eq 0) -and (-not $scanFailed))
$message = if ($success) {
  if ($removedPayloads.Count -gt 0) {
    'Claude Desktop MSIX/AppX package files removed and verified.'
  } else {
    'No Claude Desktop MSIX/AppX package files remain.'
  }
} elseif ($scanFailed) {
  'Claude Desktop MSIX/AppX package file verification failed because WindowsApps could not be enumerated.'
} else {
  'Claude Desktop MSIX/AppX package files remain: ' + ($remainingPayloads -join '; ')
}
[pscustomobject]@{
  success = [bool]$success
  message = [string]$message
  notes = @($notes)
  removedPayloads = @($removedPayloads)
  remainingPayloads = @($remainingPayloads)
} | ConvertTo-Json -Compress -Depth 4
"#
    .replace("__IDENTITY_PREFIXES__", &identity_prefixes)
    .replace("__PUBLISHER_SUFFIX__", &ps_quote(publisher_suffix))
}

pub fn probe_msix_capabilities() -> Vec<PackageCapability> {
    if !cfg!(target_os = "windows") {
        return vec![PackageCapability {
            id: "platform".to_string(),
            label: "Platform".to_string(),
            status: Severity::Info,
            detail: "The current platform is not Windows, so the MSIX/portable execution path is unavailable.".to_string(),
        }];
    }

    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$add = Get-Command Add-AppxPackage -ErrorAction SilentlyContinue
$svc = Get-Service AppXSvc -ErrorAction SilentlyContinue
$pmOk = $false
$pmError = ''
try {
  $pm = New-Object -TypeName Windows.Management.Deployment.PackageManager -ErrorAction Stop
  $pmOk = ($null -ne $pm)
} catch {
  $pmOk = $false
  $pmError = [string]$_.Exception.Message
}
[pscustomobject]@{
  addAppx = [bool]$add
  appxSvc = [bool]$svc
  appxSvcStatus = if ($svc) { [string]$svc.Status } else { '' }
  appxSvcStart = if ($svc) { [string]$svc.StartType } else { '' }
  packageManager = $pmOk
  packageManagerError = $pmError
} | ConvertTo-Json -Compress
"#;
    let value = run_powershell(script)
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok());
    let Some(value) = value else {
        return vec![PackageCapability {
            id: "probe".to_string(),
            label: "Capability check".to_string(),
            status: Severity::Warning,
            detail: "PowerShell capability probing failed; portable fallback will be allowed conservatively.".to_string(),
        }];
    };

    let add_appx = value
        .get("addAppx")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let appx_svc = value
        .get("appxSvc")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let appx_start = value
        .get("appxSvcStart")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let package_manager = value
        .get("packageManager")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let package_manager_error = value
        .get("packageManagerError")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    vec![
        PackageCapability {
            id: "add-appx".to_string(),
            label: "Add-AppxPackage".to_string(),
            status: if add_appx {
                Severity::Ok
            } else {
                Severity::Error
            },
            detail: if add_appx {
                "MSIX install command is available.".to_string()
            } else {
                "Add-AppxPackage is unavailable; portable fallback will be used.".to_string()
            },
        },
        PackageCapability {
            id: "appx-service".to_string(),
            label: "AppXSvc".to_string(),
            status: if appx_svc && !appx_start.eq_ignore_ascii_case("Disabled") {
                Severity::Ok
            } else {
                Severity::Error
            },
            detail: if appx_svc {
                format!("AppXSvc start type: {appx_start}")
            } else {
                "AppXSvc service is missing.".to_string()
            },
        },
        PackageCapability {
            id: "msix-runtime".to_string(),
            label: "MSIX runtime".to_string(),
            status: if package_manager {
                Severity::Ok
            } else {
                Severity::Error
            },
            detail: if package_manager {
                "Windows PackageManager can be activated.".to_string()
            } else {
                msix_runtime_unavailable_message(Some(package_manager_error))
            },
        },
    ]
}

pub fn probe_macos_dmg_capabilities() -> Vec<PackageCapability> {
    if !cfg!(target_os = "macos") {
        return vec![PackageCapability {
            id: "platform".to_string(),
            label: "Platform".to_string(),
            status: Severity::Info,
            detail: "The current platform is not macOS, so the DMG install path is unavailable."
                .to_string(),
        }];
    }

    let hdiutil = command_available("hdiutil");
    let ditto = command_available("ditto");
    vec![
        PackageCapability {
            id: "hdiutil".to_string(),
            label: "hdiutil".to_string(),
            status: if hdiutil {
                Severity::Ok
            } else {
                Severity::Error
            },
            detail: if hdiutil {
                "DMG mount command is available.".to_string()
            } else {
                "hdiutil is unavailable, so DMG files cannot be mounted.".to_string()
            },
        },
        PackageCapability {
            id: "ditto".to_string(),
            label: "ditto".to_string(),
            status: if ditto { Severity::Ok } else { Severity::Error },
            detail: if ditto {
                "App copy command is available.".to_string()
            } else {
                "ditto is unavailable, so .app bundles cannot be copied.".to_string()
            },
        },
    ]
}

pub fn launch_msix_package_with_args(
    package_identity: &str,
    arguments: &[String],
) -> Result<u32, String> {
    if !cfg!(target_os = "windows") {
        return Err("MSIX launch arguments are only supported on Windows.".to_string());
    }
    let app_user_model_id = msix_app_user_model_id(package_identity)?;
    let arguments = command_line_arguments(arguments);
    activate_packaged_app(&app_user_model_id, &arguments)
}

pub fn register_msix_manifest(manifest_path: &Path) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("MSIX registration is only supported on Windows.".to_string());
    }
    if !manifest_path.is_file() {
        return Err(format!(
            "MSIX manifest was not found: {}",
            manifest_path.display()
        ));
    }
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
Add-AppxPackage -Register {manifest} -DisableDevelopmentMode -ForceApplicationShutdown -ErrorAction Stop
"#,
        manifest = ps_quote(&manifest_path.to_string_lossy())
    );
    run_powershell(&script).map(|_| ())
}

pub fn launch_first_msix_package_with_args(
    package_identities: &[&str],
    arguments: &[String],
) -> Result<u32, String> {
    if !cfg!(target_os = "windows") {
        return Err("MSIX launch arguments are only supported on Windows.".to_string());
    }
    let Some(installed) = detect_first_msix_package(package_identities) else {
        return Err("Packaged app is not installed.".to_string());
    };
    let Some(package_name) = installed
        .package_family_name
        .as_deref()
        .and_then(|family| {
            package_identities
                .iter()
                .find(|identity| family.starts_with(&format!("{identity}_")))
                .copied()
        })
        .or_else(|| {
            package_identities
                .iter()
                .find(|identity| installed.path.contains(**identity))
                .copied()
        })
    else {
        return Err("Unable to resolve packaged app identity.".to_string());
    };
    launch_msix_package_with_args(package_name, arguments).or_else(|activation_err| {
        launch_desktop_package_fallback(&installed, arguments)
            .map(|_| 0)
            .map_err(|fallback_err| {
                format!("{activation_err}; desktop package fallback failed: {fallback_err}")
            })
    })
}

pub fn launch_first_desktop_package_with_args(
    package_identities: &[&str],
    arguments: &[String],
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("Desktop package execution is only supported on Windows.".to_string());
    }
    let Some(installed) = detect_first_msix_package(package_identities) else {
        return Err("Packaged desktop app is not installed.".to_string());
    };
    launch_desktop_package_fallback(&installed, arguments)
}

fn launch_desktop_package_fallback(
    installed: &InstalledMsixPackage,
    arguments: &[String],
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("Desktop package fallback is only supported on Windows.".to_string());
    }
    let Some(package_family_name) = installed.package_family_name.as_deref() else {
        return Err("Package family name is unavailable.".to_string());
    };
    let script = desktop_package_fallback_script(package_family_name, arguments);
    run_powershell(&script).map(|_| ())
}

fn msix_app_user_model_id(package_identity: &str) -> Result<String, String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$pkg = Get-AppxPackage -Name {name} | Sort-Object -Property Version -Descending | Select-Object -First 1
if ($null -eq $pkg) {{ throw 'Package is not installed' }}
$app = (Get-AppxPackageManifest $pkg).Package.Applications.Application
if ($app -is [array]) {{ $app = $app[0] }}
$id = $app.Id
if (-not $id) {{ $id = 'App' }}
$pkg.PackageFamilyName + "!" + $id
"#,
        name = ps_quote(package_identity)
    );
    let id = run_powershell(&script)?;
    if id.trim().is_empty() {
        Err("MSIX app user model id is empty.".to_string())
    } else {
        Ok(id.trim().to_string())
    }
}

fn command_line_arguments(args: &[String]) -> String {
    args.iter()
        .map(|arg| quote_windows_argument(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn desktop_package_fallback_script(package_family_name: &str, arguments: &[String]) -> String {
    let argument_line = command_line_arguments(arguments);
    format!(
        r#"
$ErrorActionPreference = 'Stop'
if (-not (Get-Command Invoke-CommandInDesktopPackage -ErrorAction SilentlyContinue)) {{
  throw 'Invoke-CommandInDesktopPackage is unavailable.'
}}
$pkg = Get-AppxPackage -ErrorAction SilentlyContinue |
  Where-Object {{ $_.PackageFamilyName -eq {package_family_name} }} |
  Sort-Object -Property Version -Descending |
  Select-Object -First 1
if ($null -eq $pkg) {{ throw 'Package is not installed.' }}
$app = (Get-AppxPackageManifest $pkg).Package.Applications.Application
if ($app -is [array]) {{ $app = $app[0] }}
$appId = [string]$app.Id
if (-not $appId) {{ $appId = 'App' }}
$command = [string]$app.Executable
if (-not $command) {{ throw 'Package executable is unavailable.' }}
$commandPath = Join-Path $pkg.InstallLocation $command
if (-not (Test-Path -LiteralPath $commandPath)) {{ throw "Package executable was not found: $commandPath" }}
$argsLine = {arguments}
Invoke-CommandInDesktopPackage -PackageFamilyName $pkg.PackageFamilyName -AppId $appId -Command $commandPath -Args $argsLine -ErrorAction Stop
"#,
        package_family_name = ps_quote(package_family_name),
        arguments = ps_quote(&argument_line),
    )
}

fn quote_windows_argument(arg: &str) -> String {
    if !arg.is_empty() && !arg.bytes().any(|byte| matches!(byte, b' ' | b'\t' | b'"')) {
        return arg.to_string();
    }
    let mut output = String::from("\"");
    let mut backslashes = 0;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                output.push_str(&"\\".repeat(backslashes * 2 + 1));
                output.push('"');
                backslashes = 0;
            }
            _ => {
                output.push_str(&"\\".repeat(backslashes));
                output.push(ch);
                backslashes = 0;
            }
        }
    }
    output.push_str(&"\\".repeat(backslashes * 2));
    output.push('"');
    output
}

#[cfg(windows)]
fn activate_packaged_app(app_user_model_id: &str, arguments: &str) -> Result<u32, String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        ApplicationActivationManager, IApplicationActivationManager, ACTIVATEOPTIONS,
    };

    unsafe {
        let coinit = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let should_uninitialize = coinit.is_ok();
        coinit
            .ok()
            .or_else(|error| {
                const RPC_E_CHANGED_MODE: i32 = -2147417850;
                if error.code().0 == RPC_E_CHANGED_MODE {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|err| format!("Failed to initialize COM for MSIX launch: {err}"))?;

        let result: windows::core::Result<u32> = (|| {
            let manager: IApplicationActivationManager =
                CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)?;
            manager.ActivateApplication(
                &HSTRING::from(app_user_model_id),
                &HSTRING::from(arguments),
                ACTIVATEOPTIONS(0),
            )
        })();

        if should_uninitialize {
            CoUninitialize();
        }
        result.map_err(|err| format!("Failed to launch MSIX app with arguments: {err}"))
    }
}

#[cfg(not(windows))]
fn activate_packaged_app(_app_user_model_id: &str, _arguments: &str) -> Result<u32, String> {
    Err("Packaged app activation is only supported on Windows.".to_string())
}

pub fn launch_macos_app(path: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Launching macOS apps is not supported on the current platform.".to_string());
    }

    hidden_command("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Failed to launch macOS app: {err}"))
}

pub fn quit_macos_app(app_name: &str) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }

    let display_name = app_name.trim_end_matches(".app");
    let script = format!("tell application \"{display_name}\" to quit");
    let _ = hidden_command("osascript").args(["-e", &script]).output();
    thread::sleep(Duration::from_secs(3));

    if !macos_process_running(display_name) {
        return Ok(());
    }
    let _ = hidden_command("pkill")
        .args(["-TERM", "-x", display_name])
        .output();
    thread::sleep(Duration::from_secs(1));
    if !macos_process_running(display_name) {
        return Ok(());
    }
    let _ = hidden_command("pkill")
        .args(["-KILL", "-x", display_name])
        .output();
    thread::sleep(Duration::from_millis(500));
    if macos_process_running(display_name) {
        Err(format!("{display_name} is still running."))
    } else {
        Ok(())
    }
}

pub fn quit_macos_app_bundle(app: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }

    let display_name = app
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("ChatGPT");
    let script = read_macos_plist_value(app, "CFBundleIdentifier")
        .filter(|bundle_id| !bundle_id.is_empty())
        .map(|bundle_id| {
            format!(
                "tell application id \"{}\" to quit",
                bundle_id.replace('"', "\\\"")
            )
        })
        .unwrap_or_else(|| {
            format!(
                "tell application \"{}\" to quit",
                display_name.replace('"', "\\\"")
            )
        });
    let _ = hidden_command("osascript").args(["-e", &script]).output();
    thread::sleep(Duration::from_secs(3));

    let process_names = macos_app_process_names(app);
    if !process_names
        .iter()
        .any(|process_name| macos_process_running(process_name))
    {
        return Ok(());
    }
    for process_name in &process_names {
        let _ = hidden_command("pkill")
            .args(["-TERM", "-x", process_name])
            .output();
    }
    thread::sleep(Duration::from_secs(1));
    if !process_names
        .iter()
        .any(|process_name| macos_process_running(process_name))
    {
        return Ok(());
    }
    for process_name in &process_names {
        let _ = hidden_command("pkill")
            .args(["-KILL", "-x", process_name])
            .output();
    }
    thread::sleep(Duration::from_millis(500));
    let remaining = process_names
        .iter()
        .filter(|process_name| macos_process_running(process_name))
        .cloned()
        .collect::<Vec<_>>();
    if remaining.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "macOS app processes are still running: {}.",
            remaining.join(", ")
        ))
    }
}

pub fn install_macos_dmg(
    dmg_path: &Path,
    app_name: &str,
    destination: &Path,
    bundle_identifier: Option<&str>,
) -> Result<MacosDmgInstallReport, String> {
    install_macos_dmg_with_app_candidates(dmg_path, &[app_name], destination, bundle_identifier)
}

pub fn install_macos_dmg_with_app_candidates(
    dmg_path: &Path,
    app_names: &[&str],
    destination: &Path,
    bundle_identifier: Option<&str>,
) -> Result<MacosDmgInstallReport, String> {
    if !cfg!(target_os = "macos") {
        return Err(
            "Installing macOS DMG packages is not supported on the current platform.".to_string(),
        );
    }
    if app_names.is_empty() {
        return Err("No macOS app bundle names were provided for the DMG.".to_string());
    }
    if !dmg_path.is_file() {
        return Err("The macOS DMG installer does not exist.".to_string());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "The macOS app install path has no parent directory.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("Failed to create macOS app parent directory: {err}"))?;

    let mount_point = temporary_macos_mount_point();
    if mount_point.exists() {
        fs::remove_dir_all(&mount_point)
            .map_err(|err| format!("Failed to clean old mount directory: {err}"))?;
    }
    fs::create_dir_all(&mount_point)
        .map_err(|err| format!("Failed to create DMG mount directory: {err}"))?;

    let attach = hidden_command("hdiutil")
        .arg("attach")
        .arg("-nobrowse")
        .arg("-readonly")
        .arg("-mountpoint")
        .arg(&mount_point)
        .arg(dmg_path)
        .output()
        .map_err(|err| format!("Failed to start hdiutil to mount DMG: {err}"))?;
    if !attach.status.success() {
        let _ = fs::remove_dir_all(&mount_point);
        return Err(format!(
            "Failed to mount macOS DMG: {}",
            String::from_utf8_lossy(&attach.stderr).trim()
        ));
    }

    let install_result =
        install_macos_app_from_mount(&mount_point, app_names, destination, bundle_identifier);
    let detach_result = detach_macos_mount(&mount_point);
    let _ = fs::remove_dir_all(&mount_point);

    let mut report = install_result?;
    if let Err(err) = detach_result {
        report.notes.push(err);
    }
    Ok(report)
}

pub fn msix_runtime_unavailable_message(detail: Option<&str>) -> String {
    let mut message = "Windows MSIX deployment runtime is unavailable. This often happens on trimmed systems, virtual machine images, or environments where App Installer, AppXSvc, or app deployment components were removed. The app will automatically use portable installation; to use MSIX, restore App Installer, enable AppXSvc, and make sure the Windows app deployment runtime is intact.".to_string();
    if let Some(detail) = detail.map(str::trim).filter(|value| !value.is_empty()) {
        message.push_str(" Original error: ");
        message.push_str(detail);
    }
    message
}

pub fn create_portable_start_menu_shortcut(
    registration: &PortableAppRegistration<'_>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let shortcut = start_menu_shortcut_path(registration.shortcut_name)?;
    let exe = registration.install_root.join(registration.executable_name);
    let script = format!(
        r#"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut({shortcut})
$shortcut.TargetPath = {target}
$shortcut.WorkingDirectory = {workdir}
$shortcut.IconLocation = {icon}
$shortcut.Save()
"#,
        shortcut = ps_quote(&shortcut.to_string_lossy()),
        target = ps_quote(&exe.to_string_lossy()),
        workdir = ps_quote(&registration.install_root.to_string_lossy()),
        icon = ps_quote(&format!("{},0", exe.to_string_lossy()))
    );
    run_powershell(&script).map(|_| ())
}

pub fn create_portable_uninstall_entry(
    registration: &PortableAppRegistration<'_>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let exe = registration.install_root.join(registration.executable_name);
    let escaped_root = registration
        .install_root
        .to_string_lossy()
        .replace('\'', "''");
    let escaped_shortcut_name = registration.shortcut_name.replace('\'', "''");
    let escaped_uninstall_key = registration.uninstall_key.replace('\'', "''");
    let uninstall_script = format!(
        "if ($env:APPDATA) {{ $Shortcut = Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs\\{escaped_shortcut_name}'; Remove-Item -LiteralPath $Shortcut -Force -ErrorAction SilentlyContinue }}; Remove-Item -LiteralPath '{escaped_root}' -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item -LiteralPath '{escaped_uninstall_key}' -Recurse -Force -ErrorAction SilentlyContinue"
    );
    let powershell = quote_windows_argument(&powershell_exe().to_string_lossy());
    let uninstall_string =
        format!("{powershell} -NoProfile -ExecutionPolicy Bypass -Command \"{uninstall_script}\"");
    let script = format!(
        r#"
$key = {key}
New-Item -Path $key -Force | Out-Null
New-ItemProperty -Path $key -Name DisplayName -Value {display_name} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name DisplayVersion -Value {version} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name Publisher -Value {publisher} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name InstallLocation -Value {install_root} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name DisplayIcon -Value {icon} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name UninstallString -Value {uninstall_string} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name QuietUninstallString -Value {uninstall_string} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name NoModify -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $key -Name NoRepair -Value 1 -PropertyType DWord -Force | Out-Null
"#,
        key = ps_quote(registration.uninstall_key),
        display_name = ps_quote(registration.display_name),
        version = ps_quote(registration.version),
        publisher = ps_quote(registration.publisher),
        install_root = ps_quote(&registration.install_root.to_string_lossy()),
        icon = ps_quote(&format!("{},0", exe.to_string_lossy())),
        uninstall_string = ps_quote(&uninstall_string)
    );
    run_powershell(&script).map(|_| ())
}

pub fn remove_portable_start_menu_shortcut(shortcut_name: &str) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let shortcut = start_menu_shortcut_path(shortcut_name)?;
    if shortcut.exists() {
        fs::remove_file(shortcut).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn remove_portable_uninstall_entry(uninstall_key: &str) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let script = format!(
        r#"
$key = {key}
if (Test-Path $key) {{
  Remove-Item -Path $key -Recurse -Force
}}
"#,
        key = ps_quote(uninstall_key)
    );
    run_powershell(&script).map(|_| ())
}

fn macos_process_running(process_name: &str) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    hidden_command("pgrep")
        .args(["-x", process_name])
        .output()
        .map(|output| output.status.success() && !output.stdout.is_empty())
        .unwrap_or(false)
}

fn macos_app_process_names(app: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(executable) = macos_app_executable_name(app) {
        names.push(executable);
    }
    if let Some(display_name) = app
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
    {
        if !names.iter().any(|name| name == display_name) {
            names.push(display_name.to_string());
        }
    }
    if names.is_empty() {
        names.push("ChatGPT".to_string());
    }
    names
}

fn command_available(command: &str) -> bool {
    hidden_command("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn temporary_macos_mount_point() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "codestudio-lite-codex-dmg-{}-{suffix}",
        std::process::id()
    ))
}

fn install_macos_app_from_mount(
    mount_point: &Path,
    app_names: &[&str],
    destination: &Path,
    bundle_identifier: Option<&str>,
) -> Result<MacosDmgInstallReport, String> {
    let source_app = find_macos_app_bundle_from_candidates(mount_point, app_names)?;
    let parent = destination
        .parent()
        .ok_or_else(|| "The macOS app install path has no parent directory.".to_string())?;
    let rollback = parent.join(format!(
        "{}.rollback",
        destination
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Codex")
    ));
    if rollback.exists() {
        fs::remove_dir_all(&rollback)
            .map_err(|err| format!("Failed to clean old rollback directory: {err}"))?;
    }

    let had_previous = destination.exists();
    if had_previous {
        fs::rename(destination, &rollback)
            .map_err(|err| format!("Failed to create macOS rollback backup: {err}"))?;
    }

    let copy = hidden_command("ditto")
        .arg(&source_app)
        .arg(destination)
        .output()
        .map_err(|err| format!("Failed to start ditto to copy macOS app: {err}"));
    match copy {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            if had_previous && rollback.exists() {
                let _ = fs::rename(&rollback, destination);
            }
            return Err(format!(
                "Failed to copy macOS app; rollback was attempted: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Err(err) => {
            if had_previous && rollback.exists() {
                let _ = fs::rename(&rollback, destination);
            }
            return Err(err);
        }
    }

    let mut notes = Vec::new();
    if had_previous && rollback.exists() {
        if let Err(err) = fs::remove_dir_all(&rollback) {
            notes.push(format!("Failed to clean macOS rollback backup: {err}"));
        }
    }
    let installed =
        detect_macos_app(&[destination.to_path_buf()], bundle_identifier).or_else(|| {
            Some(InstalledMacosApp {
                path: destination.to_string_lossy().to_string(),
                version: "installed".to_string(),
                bundle_identifier: bundle_identifier.map(str::to_string),
            })
        });
    Ok(MacosDmgInstallReport { installed, notes })
}

fn find_macos_app_bundle_from_candidates(
    root: &Path,
    app_names: &[&str],
) -> Result<PathBuf, String> {
    for app_name in app_names {
        if let Some(path) = find_macos_app_bundle(root, app_name)? {
            return Ok(path);
        }
    }
    Err(format!(
        "None of the supported macOS app bundles were found in the DMG: {}.",
        app_names.join(", ")
    ))
}

fn find_macos_app_bundle(root: &Path, app_name: &str) -> Result<Option<PathBuf>, String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|err| format!("Failed to scan DMG mount directory: {err}"))?
        {
            let entry =
                entry.map_err(|err| format!("Failed to read DMG mount directory entry: {err}"))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|err| format!("Failed to read DMG file type: {err}"))?;
            if file_type.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == app_name)
            {
                return Ok(Some(path));
            }
            if file_type.is_dir() {
                stack.push(path);
            }
        }
    }
    Ok(None)
}

fn detach_macos_mount(mount_point: &Path) -> Result<(), String> {
    let output = hidden_command("hdiutil")
        .arg("detach")
        .arg(mount_point)
        .arg("-quiet")
        .output()
        .map_err(|err| format!("Failed to start hdiutil to unmount DMG: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let forced = hidden_command("hdiutil")
        .arg("detach")
        .arg(mount_point)
        .arg("-force")
        .arg("-quiet")
        .output()
        .map_err(|err| format!("Failed to start hdiutil to force-unmount DMG: {err}"))?;
    if forced.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Failed to unmount DMG mount point: {}",
            String::from_utf8_lossy(&forced.stderr).trim()
        ))
    }
}

fn start_menu_shortcut_path(shortcut_name: &str) -> Result<PathBuf, String> {
    let appdata =
        std::env::var_os("APPDATA").ok_or_else(|| "APPDATA is unavailable.".to_string())?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(shortcut_name))
}

fn read_macos_plist_value(app: &Path, key: &str) -> Option<String> {
    let plist = app.join("Contents").join("Info.plist");
    let text = fs::read_to_string(plist).ok()?;
    plist_string_value(&text, key)
}

fn plist_string_value(text: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let key_index = text.find(&key_marker)?;
    let rest = &text[key_index + key_marker.len()..];
    let string_index = rest.find("<string>")? + "<string>".len();
    let rest = &rest[string_index..];
    let end = rest.find("</string>")?;
    Some(rest[..end].trim().to_string())
}

#[cfg(windows)]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::{
        claude_msix_payload_cleanup_script, desktop_package_fallback_script,
        find_macos_app_bundle_from_candidates, macos_app_executable_name, plist_string_value,
    };

    #[test]
    fn reads_string_values_from_macos_info_plist() {
        let plist = r#"
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.openai.codex</string>
  <key>CFBundleShortVersionString</key>
  <string>1.2.3</string>
</dict>
</plist>
"#;

        assert_eq!(
            plist_string_value(plist, "CFBundleIdentifier").as_deref(),
            Some("com.openai.codex")
        );
        assert_eq!(
            plist_string_value(plist, "CFBundleShortVersionString").as_deref(),
            Some("1.2.3")
        );
    }

    #[test]
    fn macos_bundle_helpers_support_chatgpt_app_and_plist_executable() {
        let root = std::env::temp_dir().join(format!(
            "codestudio-lite-macos-package-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let codex_app = root.join("Codex.app");
        let chatgpt_app = root.join("ChatGPT.app");
        std::fs::create_dir_all(codex_app.join("Contents")).unwrap();
        std::fs::create_dir_all(chatgpt_app.join("Contents")).unwrap();
        std::fs::write(
            chatgpt_app.join("Contents").join("Info.plist"),
            r#"<plist><dict><key>CFBundleExecutable</key><string>ChatGPT</string></dict></plist>"#,
        )
        .unwrap();

        assert_eq!(
            find_macos_app_bundle_from_candidates(
                &root,
                &["ChatGPT.app", "Codex.app", "OpenAI Codex.app"]
            )
            .unwrap(),
            chatgpt_app
        );
        assert_eq!(
            macos_app_executable_name(&chatgpt_app).as_deref(),
            Some("ChatGPT")
        );
        assert_eq!(
            macos_app_executable_name(&codex_app).as_deref(),
            Some("Codex")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn desktop_package_fallback_script_launches_async_without_placeholders() {
        let script = desktop_package_fallback_script(
            "Claude_pzs8sxrjxfjjc",
            &["--remote-debugging-port=9229".to_string()],
        );

        assert!(script.contains("Invoke-CommandInDesktopPackage"));
        assert!(script.contains("$commandPath = Join-Path $pkg.InstallLocation $command"));
        assert!(script.contains("-Command $commandPath -Args $argsLine"));
        assert!(!script.contains("-Command $command -Args $argsLine"));
        assert!(!script.contains("Start-Process"));
        assert!(!script.contains("powershell.exe"));
        assert!(script.contains("'Claude_pzs8sxrjxfjjc'"));
        assert!(script.contains("--remote-debugging-port=9229"));
        assert!(!script.contains("__CODESTUDIO"));
    }

    #[test]
    fn claude_msix_payload_cleanup_script_removes_only_verified_claude_windowsapps_dirs() {
        let script =
            claude_msix_payload_cleanup_script(&["Claude", "Anthropic.Claude"], "pzs8sxrjxfjjc");

        assert!(script.contains("C:\\Program Files\\WindowsApps"));
        assert!(script.contains("$identityPrefixes = @('Claude', 'Anthropic.Claude')"));
        assert!(script.contains("$publisherSuffix = 'pzs8sxrjxfjjc'"));
        assert!(script.contains("_(x64|arm64)__"));
        assert!(script.contains("AppxManifest.xml"));
        assert!(script.contains("app\\Claude.exe"));
        assert!(script.contains("Remove-Item -LiteralPath $dir.FullName -Recurse -Force"));
        assert!(script.contains("Test-Path -LiteralPath $dir.FullName"));
        assert!(script.contains("Invoke-ElevatedClaudePayloadCleanup"));
        assert!(script.contains("-Verb RunAs"));
        assert!(script.contains("takeown.exe"));
        assert!(script.contains("icacls.exe"));
        assert!(script.contains("scanSucceeded"));
        assert!(script.contains("remainingPayloads"));
        assert!(!script.contains("Remove-Item -LiteralPath $root -Recurse"));
    }

    #[test]
    fn claude_msix_payload_cleanup_script_includes_partial_package_residue() {
        let script =
            claude_msix_payload_cleanup_script(&["Claude", "Anthropic.Claude"], "pzs8sxrjxfjjc");

        assert!(script.contains("Test-ClaudePackageDirectoryName"));
        assert!(script.contains("Test-ClaudeCompletePayloadDirectory"));
        assert!(script.contains("Test-ClaudePartialPayloadDirectory"));
        assert!(script.contains("app\\resources\\cowork-svc.exe"));
        assert!(script.contains("Test-ClaudePayloadDirectory"));
    }
}
