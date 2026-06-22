param(
  [switch]$Elevated,
  [string]$LogPath = (Join-Path $PSScriptRoot 'claude-inspector-debug.log')
)

$ErrorActionPreference = 'Continue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$node = (Get-Command node -ErrorAction SilentlyContinue).Source
if (-not $node) {
  Write-Host 'Node.js was not found on PATH.'
  exit 10
}

$script = Join-Path $PSScriptRoot 'debug-claude-inspector.mjs'
if (-not (Test-Path -LiteralPath $script)) {
  Write-Host "Missing script: $script"
  exit 11
}

Remove-Item -LiteralPath $LogPath -ErrorAction SilentlyContinue
$env:CODESTUDIO_CLAUDE_INSPECTOR_LOG = $LogPath

Write-Host "Running Claude inspector debug..."
& $node $script
$exitCode = $LASTEXITCODE

if ($exitCode -eq 0) {
  Write-Host "Inspector opened successfully."
  exit 0
}

$log = if (Test-Path -LiteralPath $LogPath) { Get-Content -LiteralPath $LogPath -Raw } else { '' }
$needsElevation = $log -match 'ACCESS_DENIED|OpenProcess|run elevated'

if (-not $Elevated -and $needsElevation) {
  Write-Host "Access denied. Requesting administrator permission..."
  $args = @(
    '-NoLogo',
    '-NoProfile',
    '-ExecutionPolicy',
    'Bypass',
    '-File',
    $PSCommandPath,
    '-Elevated',
    '-LogPath',
    $LogPath
  )
  try {
    $process = Start-Process -FilePath 'powershell.exe' -ArgumentList $args -Verb RunAs -Wait -PassThru
    if (Test-Path -LiteralPath $LogPath) {
      Get-Content -LiteralPath $LogPath
    }
    if ($process.ExitCode -eq 0) {
      exit 0
    }
    exit $process.ExitCode
  } catch {
    Write-Host "Failed to request administrator permission: $($_.Exception.Message)"
    exit $exitCode
  }
}

Write-Host "Inspector debug failed. Log: $LogPath"
exit $exitCode
