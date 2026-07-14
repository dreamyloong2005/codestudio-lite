param(
    [string]$StorePath = (Join-Path $HOME ".codestudio-lite\updater"),
    [switch]$Force
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Security

function Set-PrivateDirectoryAcl([string]$Path) {
    $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
    & icacls.exe $Path /inheritance:r /grant:r "${identity}:(OI)(CI)F" | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to restrict updater key store permissions."
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "Initial updater key setup must run on Windows because the local password store uses DPAPI."
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$privateKeyPath = Join-Path $StorePath "updater.key"
$publicKeyPath = "$privateKeyPath.pub"
$passwordPath = Join-Path $StorePath "password.dpapi"

if (-not $Force -and ((Test-Path $privateKeyPath) -or (Test-Path $passwordPath))) {
    throw "Updater signing material already exists at $StorePath. Refusing to rotate the production trust key without -Force."
}

New-Item -ItemType Directory -Force -Path $StorePath | Out-Null
Set-PrivateDirectoryAcl $StorePath

$randomBytes = New-Object byte[] 48
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
try {
    $rng.GetBytes($randomBytes)
} finally {
    $rng.Dispose()
}
$password = [Convert]::ToBase64String($randomBytes).TrimEnd("=").Replace("+", "-").Replace("/", "_")

try {
    Push-Location $repoRoot
    try {
        & npx.cmd tauri signer generate --ci --force "--write-keys=$privateKeyPath" "--password=$password"
        if ($LASTEXITCODE -ne 0) {
            throw "Tauri signer failed with exit code $LASTEXITCODE."
        }
    } finally {
        Pop-Location
    }

    if (-not (Test-Path $privateKeyPath) -or -not (Test-Path $publicKeyPath)) {
        throw "Tauri signer did not create the expected private/public key pair."
    }

    $passwordBytes = [Text.Encoding]::UTF8.GetBytes($password)
    $protectedPassword = $null
    try {
        $protectedPassword = [Security.Cryptography.ProtectedData]::Protect(
            $passwordBytes,
            $null,
            [Security.Cryptography.DataProtectionScope]::CurrentUser
        )
        [IO.File]::WriteAllBytes($passwordPath, $protectedPassword)
    } finally {
        [Array]::Clear($passwordBytes, 0, $passwordBytes.Length)
        if ($protectedPassword) {
            [Array]::Clear($protectedPassword, 0, $protectedPassword.Length)
        }
    }
    Set-PrivateDirectoryAcl $StorePath

    $configPath = Join-Path $repoRoot "updater.config.json"
    $config = Get-Content -Raw -LiteralPath $configPath | ConvertFrom-Json
    $config.pubkey = (Get-Content -Raw -LiteralPath $publicKeyPath).Trim()
    $configJson = ($config | ConvertTo-Json -Depth 4) + [Environment]::NewLine
    [IO.File]::WriteAllText($configPath, $configJson, (New-Object Text.UTF8Encoding($false)))

    Write-Host "Updater signing key created."
    Write-Host "Private key store: $StorePath"
    Write-Host "Public key written to: $configPath"
    Write-Host "Create a portable encrypted backup with: npm run updater:key:export -- -OutputPath <path>.csl-updater-key"
} finally {
    $password = $null
    $configJson = $null
    [Array]::Clear($randomBytes, 0, $randomBytes.Length)
}
