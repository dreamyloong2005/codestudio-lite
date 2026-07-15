param(
    [switch]$SkipTauriBuild,
    [switch]$RequireUpdater
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Security

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..\..")).Path
$tauriTarget = Join-Path $repoRoot "src-tauri\target"
$wixWorkDir = Join-Path $tauriTarget "burn\wix"
$baOutputDir = Join-Path $tauriTarget "burn\ba"
$bundleOutputDir = Join-Path $tauriTarget "release\bundle\burn"
$package = Get-Content -Raw (Join-Path $repoRoot "package.json") | ConvertFrom-Json
$publicUpdaterConfig = Get-Content -Raw (Join-Path $repoRoot "updater.config.json") | ConvertFrom-Json
$version = [string]$package.version

function Resolve-PackagedFile([string[]]$CandidatePaths) {
    foreach ($candidatePath in $CandidatePaths) {
        if (Test-Path $candidatePath) {
            return $candidatePath
        }
    }
    throw "Required packaged file was not found: $($CandidatePaths -join ', ')"
}

function Move-PublishedArtifact([string]$SourcePath, [string]$DestinationPath) {
    if (-not [string]::Equals($SourcePath, $DestinationPath, [StringComparison]::OrdinalIgnoreCase)) {
        Move-Item -LiteralPath $SourcePath -Destination $DestinationPath -Force
    }
    $sourceSignature = "$SourcePath.sig"
    $destinationSignature = "$DestinationPath.sig"
    if (Test-Path $sourceSignature) {
        if (-not [string]::Equals($sourceSignature, $destinationSignature, [StringComparison]::OrdinalIgnoreCase)) {
            Move-Item -LiteralPath $sourceSignature -Destination $destinationSignature -Force
        }
    }
}

function Normalize-WindowsPublishedDirectory([string]$DirectoryPath) {
    if (-not (Test-Path $DirectoryPath)) {
        return
    }
    Get-ChildItem -LiteralPath $DirectoryPath -File | ForEach-Object {
        $normalizedName = [regex]::Replace($_.Name, "[ _]+", "-")
        $unqualifiedPrefix = "CodeStudio-Lite-${version}-"
        $qualifiedPrefix = "CodeStudio-Lite-${version}-Windows-"
        if ($normalizedName.StartsWith($unqualifiedPrefix, [StringComparison]::Ordinal) `
            -and -not $normalizedName.StartsWith($qualifiedPrefix, [StringComparison]::Ordinal)) {
            $normalizedName = $qualifiedPrefix + $normalizedName.Substring($unqualifiedPrefix.Length)
        }
        if ($normalizedName -cne $_.Name) {
            Move-Item -LiteralPath $_.FullName -Destination (Join-Path $DirectoryPath $normalizedName) -Force
        }
    }
}

function Remove-StaleLocalizedMsiArtifacts([string]$DirectoryPath) {
    if (-not (Test-Path $DirectoryPath)) {
        return
    }
    $versionPrefix = "CodeStudio-Lite-${version}-Windows-x64-"
    $keepPrefix = "${versionPrefix}en-US.msi"
    Get-ChildItem -LiteralPath $DirectoryPath -File | Where-Object {
        $_.Name.StartsWith($versionPrefix, [StringComparison]::Ordinal) `
            -and -not $_.Name.StartsWith($keepPrefix, [StringComparison]::Ordinal) `
            -and ($_.Name.EndsWith(".msi", [StringComparison]::OrdinalIgnoreCase) `
                -or $_.Name.EndsWith(".msi.sig", [StringComparison]::OrdinalIgnoreCase))
    } | Remove-Item -Force
}

if (-not $SkipTauriBuild) {
    $localUpdaterStore = Join-Path $HOME ".codestudio-lite\updater"
    $localPrivateKey = Join-Path $localUpdaterStore "updater.key"
    $localPassword = Join-Path $localUpdaterStore "password.dpapi"
    if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY) -and (Test-Path $localPrivateKey)) {
        $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content -Raw -LiteralPath $localPrivateKey).Trim()
    }
    if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) -and (Test-Path $localPassword)) {
        $protectedPassword = [IO.File]::ReadAllBytes($localPassword)
        $passwordBytes = [Security.Cryptography.ProtectedData]::Unprotect(
            $protectedPassword,
            $null,
            [Security.Cryptography.DataProtectionScope]::CurrentUser
        )
        try {
            $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [Text.Encoding]::UTF8.GetString($passwordBytes)
        } finally {
            [Array]::Clear($protectedPassword, 0, $protectedPassword.Length)
            [Array]::Clear($passwordBytes, 0, $passwordBytes.Length)
        }
    }
    $updateBaseUrl = if (-not [string]::IsNullOrWhiteSpace($env:CODESTUDIO_UPDATE_BASE_URL)) {
        $env:CODESTUDIO_UPDATE_BASE_URL
    } else {
        [string]$publicUpdaterConfig.baseUrl
    }
    $updatePublicKey = if (-not [string]::IsNullOrWhiteSpace($env:TAURI_UPDATER_PUBKEY)) {
        $env:TAURI_UPDATER_PUBKEY
    } else {
        [string]$publicUpdaterConfig.pubkey
    }
    $updaterRequiredValues = @(
        $updateBaseUrl,
        $updatePublicKey,
        $env:TAURI_SIGNING_PRIVATE_KEY
    )
    $configuredUpdaterValues = @($updaterRequiredValues | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $updaterRequested = $RequireUpdater `
        -or -not [string]::IsNullOrWhiteSpace($env:CODESTUDIO_UPDATE_BASE_URL) `
        -or -not [string]::IsNullOrWhiteSpace($env:TAURI_UPDATER_PUBKEY) `
        -or -not [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)
    if ($updaterRequested -and $configuredUpdaterValues.Count -ne $updaterRequiredValues.Count) {
        throw "Updater build requires CODESTUDIO_UPDATE_BASE_URL, TAURI_UPDATER_PUBKEY, and TAURI_SIGNING_PRIVATE_KEY. The build was stopped before creating an updater-disabled installer."
    }
    if ($updaterRequested) {
        $env:CODESTUDIO_UPDATE_BASE_URL = $updateBaseUrl
        $env:TAURI_UPDATER_PUBKEY = $updatePublicKey
        & npm.cmd run tauri:build:updater -- --bundles msi
    } else {
        & npm.cmd run tauri:build
    }
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri build failed with exit code $LASTEXITCODE."
    }
}

$wixTools = if ($env:TAURI_WIX_TOOLS) {
    $env:TAURI_WIX_TOOLS
} else {
    Get-ChildItem (Join-Path $env:LOCALAPPDATA "tauri") -Directory -Filter "WixTools*" |
        Sort-Object Name -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}

if (-not $wixTools -or -not (Test-Path (Join-Path $wixTools "candle.exe"))) {
    throw "WiX tools were not found. Run a Tauri Windows build first or set TAURI_WIX_TOOLS."
}

$msiDirectory = Join-Path $tauriTarget "release\bundle\msi"
$publishedMsiEnUs = Join-Path $msiDirectory "CodeStudio-Lite-${version}-Windows-x64-en-US.msi"
$msiEnUs = Resolve-PackagedFile -CandidatePaths @(
    (Join-Path $msiDirectory "CodeStudio Lite_${version}_x64_en-US.msi"),
    $publishedMsiEnUs,
    (Join-Path $msiDirectory "CodeStudio-Lite-${version}-x64-en-US.msi")
)

if (-not (Test-Path $msiEnUs)) {
    throw "Required en-US MSI was not generated: $msiEnUs"
}

New-Item -ItemType Directory -Force -Path $wixWorkDir, $baOutputDir, $bundleOutputDir | Out-Null

$projectPath = Join-Path $scriptDir "CodeStudioBootstrapper.csproj"
& dotnet build $projectPath `
    --configuration Release `
    "-p:WixToolsPath=$wixTools" `
    "-p:BaseOutputPath=$baOutputDir\" `
    "-p:BaseIntermediateOutputPath=$(Join-Path $tauriTarget 'burn\obj')\"
if ($LASTEXITCODE -ne 0) {
    throw "Managed bootstrapper build failed with exit code $LASTEXITCODE."
}

$baAssembly = Join-Path $baOutputDir "Release\net48\CodeStudioBootstrapper.dll"
if (-not (Test-Path $baAssembly)) {
    throw "Managed bootstrapper output was not found: $baAssembly"
}

$baConfig = Join-Path $scriptDir "BootstrapperCore.config"
$bundleSource = Join-Path $scriptDir "bundle.wxs"
$bundleObject = Join-Path $wixWorkDir "bundle.wixobj"
$bundleOutput = Join-Path $bundleOutputDir "CodeStudio-Lite-${version}-Windows-x64-setup.exe"
$icon = Join-Path $repoRoot "src-tauri\icons\icon.ico"
$brandImage = Join-Path $repoRoot "src-tauri\icons\128x128.png"
$balExtension = Join-Path $wixTools "WixBalExtension.dll"
$netFxExtension = Join-Path $wixTools "WixNetFxExtension.dll"

if (-not (Test-Path $netFxExtension)) {
    throw "WiX .NET Framework extension was not found: $netFxExtension"
}

& (Join-Path $wixTools "candle.exe") `
    -nologo `
    -ext $balExtension `
    -ext $netFxExtension `
    "-dBundleVersion=$version" `
    "-dBundleIcon=$icon" `
    "-dBrandImage=$brandImage" `
    "-dBaAssembly=$baAssembly" `
    "-dBaConfig=$baConfig" `
    "-dMsiBase=$msiEnUs" `
    -out $bundleObject `
    $bundleSource
if ($LASTEXITCODE -ne 0) {
    throw "WiX candle failed with exit code $LASTEXITCODE."
}

& (Join-Path $wixTools "light.exe") `
    -nologo `
    -ext $balExtension `
    -ext $netFxExtension `
    -out $bundleOutput `
    $bundleObject
if ($LASTEXITCODE -ne 0) {
    throw "WiX light failed with exit code $LASTEXITCODE."
}

& (Join-Path $scriptDir "verify-burn.ps1") -BundlePath $bundleOutput -WixToolsPath $wixTools
if ($LASTEXITCODE -ne 0) {
    throw "Burn verification failed with exit code $LASTEXITCODE."
}

& npx.cmd tauri signer sign $bundleOutput
if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$bundleOutput.sig")) {
    throw "Burn updater signature was not generated: $bundleOutput.sig"
}

Move-PublishedArtifact $msiEnUs $publishedMsiEnUs
Normalize-WindowsPublishedDirectory $msiDirectory
Remove-StaleLocalizedMsiArtifacts $msiDirectory
Normalize-WindowsPublishedDirectory (Join-Path $tauriTarget "release\bundle\nsis")
Normalize-WindowsPublishedDirectory $bundleOutputDir

Write-Host "Burn bundle created: $bundleOutput"
Write-Host "Burn updater signature created: $bundleOutput.sig"
Write-Host "Published updater MSI: $publishedMsiEnUs"
