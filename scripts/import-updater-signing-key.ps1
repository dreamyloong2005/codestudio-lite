param(
    [Parameter(Mandatory = $true)]
    [string]$BundlePath,
    [string]$StorePath = (Join-Path $HOME ".codestudio-lite\updater"),
    [Security.SecureString]$MigrationPassphrase,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Security

function Get-PlainText([Security.SecureString]$SecureValue) {
    $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($SecureValue)
    try {
        return [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
    }
}

function Get-DerivedKey([string]$Password, [byte[]]$Salt, [int]$Iterations) {
    $derive = [Security.Cryptography.Rfc2898DeriveBytes]::new(
        $Password,
        $Salt,
        $Iterations,
        [Security.Cryptography.HashAlgorithmName]::SHA256
    )
    try {
        return $derive.GetBytes(64)
    } finally {
        $derive.Dispose()
    }
}

function Get-MacInput([byte[]]$Salt, [int]$Iterations, [byte[]]$Iv, [byte[]]$CipherText) {
    $iterationBytes = [BitConverter]::GetBytes($Iterations)
    $result = New-Object byte[] ($Salt.Length + $iterationBytes.Length + $Iv.Length + $CipherText.Length)
    $offset = 0
    foreach ($part in @($Salt, $iterationBytes, $Iv, $CipherText)) {
        [Array]::Copy($part, 0, $result, $offset, $part.Length)
        $offset += $part.Length
    }
    return $result
}

function FixedTimeEquals([byte[]]$Left, [byte[]]$Right) {
    if ($Left.Length -ne $Right.Length) {
        return $false
    }
    $difference = 0
    for ($index = 0; $index -lt $Left.Length; $index += 1) {
        $difference = $difference -bor ($Left[$index] -bxor $Right[$index])
    }
    return $difference -eq 0
}

function Set-PrivateDirectoryAcl([string]$Path) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    & icacls.exe $Path /inheritance:r /grant:r "${identity}:(OI)(CI)F" | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to restrict updater key store permissions."
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "This importer stores the signing password with Windows DPAPI and must run on Windows."
}

$privateKeyPath = Join-Path $StorePath "updater.key"
$publicKeyPath = "$privateKeyPath.pub"
$passwordPath = Join-Path $StorePath "password.dpapi"
if (-not $Force -and ((Test-Path $privateKeyPath) -or (Test-Path $passwordPath))) {
    throw "Updater signing material already exists at $StorePath. Use -Force only when intentionally restoring the same trust key."
}

$bundle = Get-Content -Raw -LiteralPath $BundlePath | ConvertFrom-Json
if ($bundle.format -ne "codestudio-lite-updater-key-v1") {
    throw "Unsupported updater key backup format."
}

$migrationSecret = if ($MigrationPassphrase) { $MigrationPassphrase } else { Read-Host "Migration passphrase" -AsSecureString }
$migrationPassword = Get-PlainText $migrationSecret
$salt = [Convert]::FromBase64String([string]$bundle.salt)
$iv = [Convert]::FromBase64String([string]$bundle.iv)
$cipherText = [Convert]::FromBase64String([string]$bundle.cipherText)
$expectedTag = [Convert]::FromBase64String([string]$bundle.hmacSha256)
$iterations = [int]$bundle.iterations
$derivedKey = Get-DerivedKey $migrationPassword $salt $iterations
$encryptionKey = New-Object byte[] 32
$macKey = New-Object byte[] 32
[Array]::Copy($derivedKey, 0, $encryptionKey, 0, 32)
[Array]::Copy($derivedKey, 32, $macKey, 0, 32)

$macInput = $null
$actualTag = $null
$plainBytes = $null
$passwordBytes = $null
$protectedPassword = $null
$payload = $null
try {
    $macInput = Get-MacInput $salt $iterations $iv $cipherText
    $hmac = [Security.Cryptography.HMACSHA256]::new($macKey)
    try {
        $actualTag = $hmac.ComputeHash($macInput)
    } finally {
        $hmac.Dispose()
    }
    if (-not (FixedTimeEquals $expectedTag $actualTag)) {
        throw "Updater key backup authentication failed. Check the migration passphrase and file integrity."
    }

    $aes = [Security.Cryptography.Aes]::Create()
    try {
        $aes.KeySize = 256
        $aes.Mode = [Security.Cryptography.CipherMode]::CBC
        $aes.Padding = [Security.Cryptography.PaddingMode]::PKCS7
        $aes.Key = $encryptionKey
        $aes.IV = $iv
        $decryptor = $aes.CreateDecryptor()
        try {
            $plainBytes = $decryptor.TransformFinalBlock($cipherText, 0, $cipherText.Length)
        } finally {
            $decryptor.Dispose()
        }
    } finally {
        $aes.Dispose()
    }

    $payload = [Text.Encoding]::UTF8.GetString($plainBytes) | ConvertFrom-Json
    if ([int]$payload.schemaVersion -ne 1 -or [string]::IsNullOrWhiteSpace([string]$payload.privateKey) -or [string]::IsNullOrWhiteSpace([string]$payload.publicKey)) {
        throw "Updater key backup payload is incomplete."
    }

    New-Item -ItemType Directory -Force -Path $StorePath | Out-Null
    $payload.privateKey | Set-Content -LiteralPath $privateKeyPath -NoNewline -Encoding UTF8
    $payload.publicKey | Set-Content -LiteralPath $publicKeyPath -NoNewline -Encoding UTF8
    $passwordBytes = [Text.Encoding]::UTF8.GetBytes([string]$payload.signingPassword)
    $protectedPassword = [Security.Cryptography.ProtectedData]::Protect(
        $passwordBytes,
        $null,
        [Security.Cryptography.DataProtectionScope]::CurrentUser
    )
    [IO.File]::WriteAllBytes($passwordPath, $protectedPassword)
    Set-PrivateDirectoryAcl $StorePath
    Write-Host "Updater signing key restored to: $StorePath"
} finally {
    $migrationPassword = $null
    $payload = $null
    foreach ($buffer in @($derivedKey, $encryptionKey, $macKey, $salt, $iv, $cipherText, $expectedTag, $macInput, $actualTag, $plainBytes, $passwordBytes, $protectedPassword)) {
        if ($null -ne $buffer) {
            [Array]::Clear($buffer, 0, $buffer.Length)
        }
    }
}
