param(
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [string]$StorePath = (Join-Path $HOME ".codestudio-lite\updater"),
    [Security.SecureString]$MigrationPassphrase
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

function Read-ConfirmedMigrationSecret {
    $first = Read-Host "Migration passphrase (12+ characters)" -AsSecureString
    $second = Read-Host "Confirm migration passphrase" -AsSecureString
    $firstText = Get-PlainText $first
    $secondText = Get-PlainText $second
    try {
        if ($firstText.Length -lt 12) {
            throw "Migration passphrase must contain at least 12 characters."
        }
        if ($firstText -cne $secondText) {
            throw "Migration passphrases do not match."
        }
        return $first
    } finally {
        $firstText = $null
        $secondText = $null
    }
}

$privateKeyPath = Join-Path $StorePath "updater.key"
$publicKeyPath = "$privateKeyPath.pub"
$passwordPath = Join-Path $StorePath "password.dpapi"
foreach ($path in @($privateKeyPath, $publicKeyPath, $passwordPath)) {
    if (-not (Test-Path $path)) {
        throw "Required updater signing file was not found: $path"
    }
}

$migrationSecret = if ($MigrationPassphrase) { $MigrationPassphrase } else { Read-ConfirmedMigrationSecret }
$migrationPassword = Get-PlainText $migrationSecret
if ($migrationPassword.Length -lt 12) {
    throw "Migration passphrase must contain at least 12 characters."
}

$salt = New-Object byte[] 32
$rng = [Security.Cryptography.RandomNumberGenerator]::Create()
try {
    $rng.GetBytes($salt)
} finally {
    $rng.Dispose()
}
$iterations = 300000
$derivedKey = Get-DerivedKey $migrationPassword $salt $iterations
$encryptionKey = New-Object byte[] 32
$macKey = New-Object byte[] 32
[Array]::Copy($derivedKey, 0, $encryptionKey, 0, 32)
[Array]::Copy($derivedKey, 32, $macKey, 0, 32)

$protectedPassword = $null
$passwordBytes = $null
$plainBytes = $null
$cipherText = $null
$iv = $null
$macInput = $null
$tag = $null
$localPassword = $null
$payload = $null
try {
    $protectedPassword = [IO.File]::ReadAllBytes($passwordPath)
    $passwordBytes = [Security.Cryptography.ProtectedData]::Unprotect(
        $protectedPassword,
        $null,
        [Security.Cryptography.DataProtectionScope]::CurrentUser
    )
    $localPassword = [Text.Encoding]::UTF8.GetString($passwordBytes)
    $payload = [ordered]@{
        schemaVersion = 1
        createdAt = [DateTime]::UtcNow.ToString("o")
        privateKey = Get-Content -Raw -LiteralPath $privateKeyPath
        publicKey = (Get-Content -Raw -LiteralPath $publicKeyPath).Trim()
        signingPassword = $localPassword
    } | ConvertTo-Json -Compress
    $plainBytes = [Text.Encoding]::UTF8.GetBytes($payload)

    $aes = [Security.Cryptography.Aes]::Create()
    try {
        $aes.KeySize = 256
        $aes.Mode = [Security.Cryptography.CipherMode]::CBC
        $aes.Padding = [Security.Cryptography.PaddingMode]::PKCS7
        $aes.Key = $encryptionKey
        $aes.GenerateIV()
        $iv = $aes.IV
        $encryptor = $aes.CreateEncryptor()
        try {
            $cipherText = $encryptor.TransformFinalBlock($plainBytes, 0, $plainBytes.Length)
        } finally {
            $encryptor.Dispose()
        }
    } finally {
        $aes.Dispose()
    }

    $macInput = Get-MacInput $salt $iterations $iv $cipherText
    $hmac = [Security.Cryptography.HMACSHA256]::new($macKey)
    try {
        $tag = $hmac.ComputeHash($macInput)
    } finally {
        $hmac.Dispose()
    }

    $bundle = [ordered]@{
        format = "codestudio-lite-updater-key-v1"
        kdf = "PBKDF2-HMAC-SHA256"
        iterations = $iterations
        salt = [Convert]::ToBase64String($salt)
        iv = [Convert]::ToBase64String($iv)
        cipherText = [Convert]::ToBase64String($cipherText)
        hmacSha256 = [Convert]::ToBase64String($tag)
    }
    $resolvedOutput = [IO.Path]::GetFullPath($OutputPath)
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $resolvedOutput) | Out-Null
    $bundle | ConvertTo-Json | Set-Content -LiteralPath $resolvedOutput -Encoding UTF8
    Write-Host "Portable encrypted updater key backup created: $resolvedOutput"
    Write-Host "The migration passphrase is required to import this file on another machine."
} finally {
    $migrationPassword = $null
    $localPassword = $null
    $payload = $null
    foreach ($buffer in @($derivedKey, $encryptionKey, $macKey, $salt, $protectedPassword, $passwordBytes, $plainBytes, $cipherText, $iv, $macInput, $tag)) {
        if ($null -ne $buffer) {
            [Array]::Clear($buffer, 0, $buffer.Length)
        }
    }
}
