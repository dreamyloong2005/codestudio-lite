param(
    [Parameter(Mandatory = $true)]
    [string]$BundlePath,

    [Parameter(Mandatory = $true)]
    [string]$WixToolsPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$bundle = (Resolve-Path $BundlePath).Path
$dark = Join-Path $WixToolsPath "dark.exe"
if (-not (Test-Path $dark)) {
    throw "WiX dark.exe was not found under $WixToolsPath."
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$extractRoot = Join-Path $repoRoot "src-tauri\target\burn\verify\$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $extractRoot | Out-Null

& $dark -nologo -x $extractRoot $bundle
if ($LASTEXITCODE -ne 0) {
    throw "WiX dark failed with exit code $LASTEXITCODE."
}

$manifestPath = Join-Path $extractRoot "UX\manifest.xml"
$applicationDataPath = Join-Path $extractRoot "UX\BootstrapperApplicationData.xml"
if (-not (Test-Path $manifestPath) -or -not (Test-Path $applicationDataPath)) {
    throw "The Burn manifest or bootstrapper application data was not extracted."
}

[xml]$manifest = Get-Content -Raw $manifestPath
$burnNs = New-Object System.Xml.XmlNamespaceManager($manifest.NameTable)
$burnNs.AddNamespace("burn", "http://schemas.microsoft.com/wix/2008/Burn")
[xml]$applicationData = Get-Content -Raw $applicationDataPath
$applicationDataNs = New-Object System.Xml.XmlNamespaceManager($applicationData.NameTable)
$applicationDataNs.AddNamespace("ba", "http://schemas.microsoft.com/wix/2010/BootstrapperApplicationData")

$relatedBundle = $manifest.SelectSingleNode("//burn:RelatedBundle", $burnNs)
if ($relatedBundle.Id -ne "{6B78C4D8-8C90-4C11-A1D8-893160DA17A7}") {
    throw "Unexpected Burn UpgradeCode: $($relatedBundle.Id)"
}

$expectedPackages = @("NetFx48Web", "MsiBase")
$actualPackages = @($manifest.SelectNodes("//burn:Chain/burn:ExePackage | //burn:Chain/burn:MsiPackage", $burnNs) | ForEach-Object { $_.Id })
if (@(Compare-Object $expectedPackages $actualPackages).Count -ne 0) {
    throw "Unexpected Burn package IDs: $($actualPackages -join ', ')"
}

$netFxPackage = $manifest.SelectSingleNode("//burn:Chain/burn:ExePackage[@Id='NetFx48Web']", $burnNs)
if (-not $netFxPackage -or $netFxPackage.PerMachine -ne "yes" -or $netFxPackage.Permanent -ne "yes" -or
    $netFxPackage.Protocol -ne "netfx4" -or $netFxPackage.DetectCondition -notmatch "NETFRAMEWORK45.*528040") {
    throw "The .NET Framework prerequisite is not authored as the expected permanent per-machine package."
}
$netFxPayload = $manifest.SelectSingleNode("//burn:Payload[@Id='NetFx48Web']", $burnNs)
if (-not $netFxPayload -or $netFxPayload.Packaging -ne "external" -or
    $netFxPayload.DownloadUrl -ne "https://go.microsoft.com/fwlink/?LinkId=2085155") {
    throw "The .NET Framework prerequisite is not using the expected external Microsoft web payload."
}
$managedPrerequisite = $applicationData.SelectSingleNode("//ba:WixMbaPrereqInformation", $applicationDataNs)
if (-not $managedPrerequisite -or $managedPrerequisite.PackageId -ne "NetFx48Web") {
    throw "The managed bootstrapper host is not bound to the .NET Framework prerequisite."
}

$embeddedPayloadPaths = @($manifest.SelectNodes("//burn:Payload[@Container='WixAttachedContainer' and @Packaging='embedded']", $burnNs) | ForEach-Object { $_.FilePath })
if ($embeddedPayloadPaths.Count -ne 1 -or $embeddedPayloadPaths[0] -notlike "*.msi") {
    throw "Unexpected embedded installer payloads: $($embeddedPayloadPaths -join ', ')"
}

$embeddedMsi = Get-ChildItem (Join-Path $extractRoot "AttachedContainer") -File -Filter "*.msi"
if (@($embeddedMsi).Count -ne 1) {
    throw "Expected exactly one extracted MSI payload."
}

Add-Type -Path (Join-Path $WixToolsPath "sdk\Microsoft.Deployment.WindowsInstaller.dll")
$database = New-Object Microsoft.Deployment.WindowsInstaller.Database(
    $embeddedMsi.FullName,
    [Microsoft.Deployment.WindowsInstaller.DatabaseOpenMode]::ReadOnly
)
try {
    $arpActionType = @($database.ExecuteStringQuery("SELECT ``Type`` FROM ``CustomAction`` WHERE ``Action`` = 'SetARPINSTALLLOCATION'"))
    $arpActionSource = @($database.ExecuteStringQuery("SELECT ``Source`` FROM ``CustomAction`` WHERE ``Action`` = 'SetARPINSTALLLOCATION'"))
    $arpActionTarget = @($database.ExecuteStringQuery("SELECT ``Target`` FROM ``CustomAction`` WHERE ``Action`` = 'SetARPINSTALLLOCATION'"))
    $arpActionSequence = @($database.ExecuteStringQuery("SELECT ``Sequence`` FROM ``InstallExecuteSequence`` WHERE ``Action`` = 'SetARPINSTALLLOCATION'"))
    $costFinalizeSequence = @($database.ExecuteStringQuery("SELECT ``Sequence`` FROM ``InstallExecuteSequence`` WHERE ``Action`` = 'CostFinalize'"))
} finally {
    $database.Dispose()
}
if ($arpActionType.Count -ne 1 -or $arpActionType[0] -ne "51" -or
    $arpActionSource.Count -ne 1 -or $arpActionSource[0] -ne "ARPINSTALLLOCATION" -or
    $arpActionTarget.Count -ne 1 -or $arpActionTarget[0] -ne "[INSTALLDIR]") {
    throw "The embedded MSI does not persist INSTALLDIR through SetARPINSTALLLOCATION."
}
if ($arpActionSequence.Count -ne 1 -or $costFinalizeSequence.Count -ne 1 -or
    [int]$arpActionSequence[0] -le [int]$costFinalizeSequence[0]) {
    throw "SetARPINSTALLLOCATION must run after CostFinalize."
}

$installDirectoryProperty = $manifest.SelectSingleNode("//burn:MsiPackage[@Id='MsiBase']/burn:MsiProperty[@Id='INSTALLDIR']", $burnNs)
if (-not $installDirectoryProperty -or $installDirectoryProperty.Value -ne "[InstallFolder]") {
    throw "The base MSI is not wired to the selected installation folder."
}

$uxPayloads = @($manifest.SelectNodes("//burn:UX/burn:Payload", $burnNs) | ForEach-Object { $_.FilePath })
foreach ($requiredPayload in @("CodeStudioBootstrapper.dll", "BootstrapperCore.config", "CodeStudioLite.ico", "CodeStudioLite.png")) {
    if ($uxPayloads -notcontains $requiredPayload) {
        throw "Missing bootstrapper payload: $requiredPayload"
    }
}

$bundleSize = (Get-Item $bundle).Length
if ($bundleSize -ge 25MB) {
    throw "Burn bundle is unexpectedly large: $bundleSize bytes."
}

$planInstallFolder = Join-Path $extractRoot "CodeStudio Lite Burn Verification"
$planLogPath = Join-Path $extractRoot "plan.log"
Add-Type -TypeDefinition @"
using System.Runtime.InteropServices;
public static class BurnVerifyUiLanguage {
    [DllImport("kernel32.dll")]
    public static extern ushort GetUserDefaultUILanguage();

    [DllImport("kernel32.dll")]
    public static extern ushort GetSystemDefaultUILanguage();
}
"@
$uiLanguageId = [BurnVerifyUiLanguage]::GetUserDefaultUILanguage()
if ($uiLanguageId -eq 0) {
    $uiLanguageId = [BurnVerifyUiLanguage]::GetSystemDefaultUILanguage()
}
$uiCulture = [Globalization.CultureInfo]::GetCultureInfo([int]$uiLanguageId).Name
$expectedLanguage = if ($uiCulture -in @("zh-TW", "zh-HK", "zh-MO")) {
    "zh-TW"
} elseif ($uiCulture.StartsWith("zh", [StringComparison]::OrdinalIgnoreCase)) {
    "zh-CN"
} else {
    "en-US"
}
$planProcess = Start-Process -FilePath $bundle `
    -ArgumentList @("-quiet", "-log", "`"$planLogPath`"", "-VerifyPlanOnly=1", "-InstallFolder=`"$planInstallFolder`"") `
    -PassThru `
    -WindowStyle Hidden
if (-not $planProcess.WaitForExit(30000)) {
    Stop-Process -Id $planProcess.Id -Force -ErrorAction SilentlyContinue
    throw "Burn install planning did not exit within 30 seconds."
}
if ($planProcess.ExitCode -ne 0) {
    throw "Burn install planning failed with exit code $($planProcess.ExitCode)."
}

if (-not (Test-Path $planLogPath)) {
    throw "Burn install planning did not produce a log."
}
$planLogContent = Get-Content -Raw $planLogPath
if ($planLogContent -notmatch 'Plan complete, result: 0x0') {
    throw "Burn install planning did not complete successfully."
}
if ($planLogContent -notmatch [Regex]::Escape("Variable: InstallFolder = $planInstallFolder")) {
    throw "Burn install planning did not preserve the selected installation folder."
}
if ($planLogContent -notmatch [Regex]::Escape("Variable: SelectedLanguage = $expectedLanguage")) {
    throw "Burn did not select the current Windows UI language: expected $expectedLanguage."
}

$unsupportedLanguageLogPath = Join-Path $extractRoot "unsupported-language-plan.log"
$unsupportedLanguageProcess = Start-Process -FilePath $bundle `
    -ArgumentList @("-quiet", "-log", "`"$unsupportedLanguageLogPath`"", "-VerifyPlanOnly=1", "-SelectedLanguage=ja-JP", "-InstallFolder=`"$planInstallFolder`"") `
    -PassThru `
    -WindowStyle Hidden
if (-not $unsupportedLanguageProcess.WaitForExit(30000)) {
    Stop-Process -Id $unsupportedLanguageProcess.Id -Force -ErrorAction SilentlyContinue
    throw "Burn unsupported-language fallback verification did not exit within 30 seconds."
}
if ($unsupportedLanguageProcess.ExitCode -ne 0) {
    throw "Burn unsupported-language fallback verification failed with exit code $($unsupportedLanguageProcess.ExitCode)."
}
$unsupportedLanguageLogContent = Get-Content -Raw $unsupportedLanguageLogPath
if ($unsupportedLanguageLogContent -notmatch [Regex]::Escape("Variable: SelectedLanguage = en-US")) {
    throw "Burn did not fall back an unsupported installer language to en-US."
}

$sameVersionRelatedBundles = [Regex]::Matches(
    $planLogContent,
    'Detected related bundle: (?<id>\{[^}]+\}), type: Upgrade,.*operation: None'
) | ForEach-Object { $_.Groups['id'].Value }
foreach ($bundleId in $sameVersionRelatedBundles) {
    $plannedAbsent = 'Planned related bundle: ' + [Regex]::Escape($bundleId) + '.*ba requested: Absent'
    if ($planLogContent -notmatch $plannedAbsent) {
        throw "Same-version related bundle was not planned absent: $bundleId"
    }
}
if ($planLogContent -match 'Apply begin') {
    throw "Plan-only verification unexpectedly started installation."
}

$windowLogPath = Join-Path $extractRoot "window-ready.log"
$windowProcess = Start-Process -FilePath $bundle `
    -ArgumentList @("-log", "`"$windowLogPath`"") `
    -PassThru
$bundleFileName = [IO.Path]::GetFileName($bundle)
$windowDeadline = [DateTime]::UtcNow.AddSeconds(30)
$readyWindowProcess = $null
try {
    while ([DateTime]::UtcNow -lt $windowDeadline -and -not $readyWindowProcess) {
        $bundleProcesses = @(Get-CimInstance Win32_Process -Filter "Name = '$bundleFileName'" -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -like "*$windowLogPath*" })
        foreach ($candidate in $bundleProcesses) {
            $process = Get-Process -Id $candidate.ProcessId -ErrorAction SilentlyContinue
            if ($process -and $process.MainWindowHandle -ne [IntPtr]::Zero -and
                $process.MainWindowTitle -like "CodeStudio Lite*") {
                $process.Refresh()
                if ($process.Responding) {
                    $readyWindowProcess = $process
                    break
                }
            }
        }
        if (-not $readyWindowProcess) {
            Start-Sleep -Milliseconds 100
        }
    }
} finally {
    $testProcessIds = @(Get-CimInstance Win32_Process -Filter "Name = '$bundleFileName'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like "*$windowLogPath*" } |
        ForEach-Object { $_.ProcessId })
    $processIds = @($testProcessIds + $windowProcess.Id | Select-Object -Unique)
    foreach ($processId in $processIds) {
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
    foreach ($processId in $processIds) {
        Wait-Process -Id $processId -Timeout 5 -ErrorAction SilentlyContinue
    }
}
if (-not $readyWindowProcess) {
    throw "Burn did not create a visible and responsive interactive window within 30 seconds."
}
if (-not (Test-Path $windowLogPath)) {
    throw "Burn interactive window verification did not produce a log."
}
$windowLogContent = Get-Content -Raw $windowLogPath
if ($windowLogContent -match 'Apply begin') {
    throw "Interactive window verification unexpectedly started installation."
}

$verifyRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot "src-tauri\target\burn\verify"))
$resolvedExtractRoot = [IO.Path]::GetFullPath($extractRoot)
if (-not $resolvedExtractRoot.StartsWith($verifyRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to clean an unexpected verification path: $resolvedExtractRoot"
}
Remove-Item -LiteralPath $resolvedExtractRoot -Recurse -Force

Write-Host "Burn verification passed: compact payload structure and non-installing Install planning are valid."
