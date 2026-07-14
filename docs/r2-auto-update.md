# Cloudflare R2 Automatic Updates

CodeStudio Lite uses the Tauri v2 updater with signed update artifacts stored in Cloudflare R2. R2 is the public distribution origin; Tauri signatures remain the trust boundary for installing an update. GitHub is not required for building, publishing, or serving updates.

## Release Layout

The production custom domain is `https://download.codestudio.build`. Do not use an `r2.dev` URL for production releases because Cloudflare documents it as a rate-limited development endpoint.

```text
stable/latest.json
releases/1.5.0/CodeStudio-Lite-1.5.0-Windows-x64-setup.exe
releases/1.5.0/CodeStudio-Lite-1.5.0-Windows-x64-setup.exe.sig
releases/1.5.0/CodeStudio-Lite-1.5.0-macOS-arm64.dmg
releases/1.5.0/CodeStudio-Lite-1.5.0-macOS-arm64.dmg.sig
releases/1.5.0/CodeStudio-Lite-1.5.0-macOS-x64.dmg
releases/1.5.0/CodeStudio-Lite-1.5.0-macOS-x64.dmg.sig
releases/1.5.0/CodeStudio-Lite-1.5.0-Linux-x64.AppImage
releases/1.5.0/CodeStudio-Lite-1.5.0-Linux-x64.AppImage.sig
```

The exact generated artifact names should be discovered from the build output instead of duplicated in release-runner configuration. The manifest generator maps each supported Tauri target to its uploaded object URL and signature.

Every downloadable filename uses `CodeStudio-Lite-<version>-<OS>-<architecture>-...`. The OS token is exactly `Windows`, `Linux`, or `macOS`; filenames may not contain spaces or underscores. Tauri platform identifiers such as `windows-x86_64` remain JSON protocol keys, but artifacts are stored directly under `releases/<version>/`; the filename is the only OS and architecture discriminator in R2.

The Settings update check still uses Tauri updater version selection, but Windows and macOS hand the signed installer URL to the application backend. The backend downloads from `download.codestudio.build`, verifies the adjacent Tauri minisign signature, and only then starts the platform installer.

## Windows Contract

- Use the branded WiX Burn executable for both first installation and updates.
- Sign the final Burn EXE with `tauri signer sign` and publish the EXE plus `.sig`.
- After verification, CodeStudio Lite starts Burn with `-quiet -norestart -LaunchAfterInstall=1`, stops its gateway, and exits. Burn launches the newly installed application only after a successful apply that does not require a Windows restart.
- Preserve the existing installation directory through the MSI upgrade contract.

## macOS Contract

- Build separate arm64 and x64 DMGs for both manual installation and updates.
- Sign each normalized DMG with `tauri signer sign` and publish each DMG plus its `.sig`.
- After verification, a detached helper waits for CodeStudio Lite to exit, mounts the DMG, replaces the application bundle with rollback protection, and opens the updated app.
- Map the arm64 DMG to `darwin-aarch64` and the x64 DMG to `darwin-x86_64` in `latest.json`.
- The Tauri updater sends the running target and current application version when checking `stable/latest.json`; only a newer version is offered, and the backend downloads the exact architecture entry selected for that build.
- Production distribution still requires the normal Apple signing and notarization path; the Tauri updater signature does not replace Gatekeeper requirements.

## Linux Contract

- Use the AppImage and adjacent signature as the Tauri automatic-update payload.
- Normalize AppImage, DEB, RPM, updater archive, and signature names after each Linux package build.
- Pass the exact Tauri platform key, such as `linux-x86_64` or `linux-aarch64`, when generating and publishing a manifest.

## Publishing Order

1. Build and sign every platform artifact.
2. Upload artifacts and `.sig` files to immutable `releases/<version>/...` keys.
3. HEAD-check every uploaded object and validate its size and SHA-256 metadata before publishing the channel manifest.
4. Generate and validate the complete Tauri updater manifest.
5. Upload `stable/latest.json` last. This is the release commit point.
6. Keep the previous manifest so rollback can repoint `stable/latest.json` without rebuilding artifacts.

Never overwrite a versioned artifact. If a build is wrong, publish a new version.

## Cache Policy

Recommended response headers:

| Object | Cache-Control |
| --- | --- |
| `releases/<version>/**` | `public, max-age=31536000, immutable` |
| `stable/latest.json` | `no-cache, max-age=0, must-revalidate` |

Use an R2 custom domain so Cloudflare Cache rules can enforce these policies. The app should use a cache-busting query only as a diagnostic fallback, not as its normal update protocol.

## Security Boundaries

- Embed only the updater public key and public manifest URL in the application.
- Store `TAURI_SIGNING_PRIVATE_KEY` and its password in the secret store of the machine or release runner that performs the build.
- Store the Cloudflare account ID, R2 access key ID, and R2 secret access key separately in that release environment.
- Scope the R2 token to object read/write access for the release bucket only.
- Never expose Cloudflare credentials or the updater private key to frontend code, build artifacts, logs, or R2.
- Require HTTPS and reject unsigned or invalidly signed updater artifacts.

## Initial Channel Policy

The first implementation uses a stable-only channel. Prerelease channels can be added later as separate mutable manifests such as `beta/latest.json`; they must not share a pointer with stable releases.

## Update Package Policy

The first implementation uses complete signed update packages. It will not implement binary delta, file-level incremental, or patch-chain updates.

- Windows downloads the complete signed Burn updater artifact.
- macOS downloads the complete signed DMG for the running machine architecture.
- A failed download or installation can be retried without reconstructing an application from partial patches.
- Differential updates should only be reconsidered if release artifacts become large enough for the additional patch generation, fallback, rollback, and signing complexity to be justified.

## Required Inputs

Before enabling the updater in production, provide:

1. The R2 bucket name.
2. The configured public custom-domain base URL: `https://download.codestudio.build`.
3. A Tauri updater public key generated from a securely stored signing key.
4. Scoped release credentials for uploading to the bucket.

The repository can implement and test the client, manifest generator, and dry-run publisher before production credentials are added. A real update installation should only be tested after an older signed build and a newer signed build are both available.

## Cloudflare R2 Setup

1. In Cloudflare Dashboard, open **R2 Object Storage** and create a bucket such as `codestudio-lite-updates`.
2. Open the bucket's **Settings** and connect the custom domain `download.codestudio.build`.
3. Disable the public `r2.dev` development URL after the custom domain works.
4. Create an R2 API token restricted to **Object Read & Write** for this bucket. Record its access key ID and secret access key only in the secure release environment.
5. Add Cache Rules for the custom domain:
   - `/releases/*`: cache eligible, browser/edge TTL one year, immutable response metadata.
   - `/stable/latest.json`: bypass cache or require revalidation with a zero-second TTL.
6. Verify that `https://download.codestudio.build/stable/latest.json` can be fetched without authentication after the first manifest is uploaded.

The S3-compatible upload endpoint is:

```text
https://<CLOUDFLARE_ACCOUNT_ID>.r2.cloudflarestorage.com
```

Use region `auto` with S3-compatible tools.

## Tauri Signing Setup

Generate the production updater key once on a trusted machine:

```powershell
npx tauri signer generate --ci --write-keys "$HOME/.codestudio-lite/updater.key" --password "<strong-password>"
```

Back up the private key and password separately. Losing either prevents existing installations from accepting future updates.

On Windows, the repository provides managed key storage:

```powershell
npm run updater:key:init
```

This generates a random signing password, stores it using Windows DPAPI for the current user, restricts the key directory ACL, and writes only the public key to `updater.config.json`. Windows updater builds load this local key store automatically.

Create a portable encrypted migration backup:

```powershell
npm run updater:key:export -- -OutputPath "D:\SecureBackup\codestudio-lite.csl-updater-key"
```

The export command asks for a separate migration passphrase. Keep the encrypted bundle and migration passphrase in separate locations. Restore on another Windows machine with:

```powershell
npm run updater:key:import -- -BundlePath "D:\SecureBackup\codestudio-lite.csl-updater-key"
```

Configure these environment variables on the trusted build or release machine. The same names work in a local shell, a self-hosted runner, or any CI provider:

| Name | Value |
| --- | --- |
| `CODESTUDIO_UPDATE_BASE_URL` | `https://download.codestudio.build` (optional override; already checked in) |
| `TAURI_UPDATER_PUBKEY` | Contents of `updater.key.pub` |
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Key password |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account ID |
| `R2_ACCESS_KEY_ID` | Scoped R2 access key ID |
| `R2_SECRET_ACCESS_KEY` | Scoped R2 secret access key |
| `R2_BUCKET` | `codestudio-lite-updates` |

The current Tauri CLI must receive the private-key contents in `TAURI_SIGNING_PRIVATE_KEY`; its advertised path variable was not accepted by the verified Windows signing build.

## Signed Build Commands

Windows PowerShell:

```powershell
$env:CODESTUDIO_UPDATE_BASE_URL = "https://download.codestudio.build"
$env:TAURI_UPDATER_PUBKEY = (Get-Content -Raw "$HOME/.codestudio-lite/updater.key.pub").Trim()
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content -Raw "$HOME/.codestudio-lite/updater.key").Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<key-password>"
npm run updater:build:windows
```

This produces the branded Burn installer and its updater signature. The command fails before building if the R2 base URL, updater public key, or signing private key is missing; it will not silently create an updater-disabled release.

macOS shell:

```bash
export CODESTUDIO_UPDATE_BASE_URL="https://download.codestudio.build"
export TAURI_UPDATER_PUBKEY="$(cat "$HOME/.codestudio-lite/updater.key.pub")"
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.codestudio-lite/updater.key")"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<key-password>"
npm run updater:build:macos
```

This produces separate arm64 and x64 `.app` bundles, normalized DMGs, and adjacent DMG `.sig` files on a macOS build machine. Apple Developer signing and notarization should be configured separately for production distribution.

Linux shell:

```bash
export CODESTUDIO_UPDATE_BASE_URL="https://download.codestudio.build"
export TAURI_UPDATER_PUBKEY="$(cat "$HOME/.codestudio-lite/updater.key.pub")"
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.codestudio-lite/updater.key")"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<key-password>"
npm run updater:build:linux -- --target x86_64-unknown-linux-gnu
```

This produces platform-qualified AppImage, DEB, and RPM filenames when those bundle formats are enabled. Run `npm run normalize:linux -- <target-triple>` to normalize an existing Linux bundle directory without rebuilding it.

The build machine and the machine that uploads to R2 may be different. Transfer signed artifacts through any trusted internal mechanism, generate `latest.json` after both platform outputs are available, then run the R2 publisher. No GitHub release, repository artifact, or GitHub-hosted runner is part of the update protocol.

## `latest.json` Example

```json
{
  "version": "1.5.0",
  "notes": "CodeStudio Lite 1.5.0",
  "pub_date": "2026-07-14T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<contents of the Burn .exe.sig file>",
      "url": "https://download.codestudio.build/releases/1.5.0/CodeStudio-Lite-1.5.0-Windows-x64-setup.exe"
    },
    "darwin-aarch64": {
      "signature": "<contents of the arm64 .dmg.sig file>",
      "url": "https://download.codestudio.build/releases/1.5.0/CodeStudio-Lite-1.5.0-macOS-arm64.dmg"
    },
    "darwin-x86_64": {
      "signature": "<contents of the x64 .dmg.sig file>",
      "url": "https://download.codestudio.build/releases/1.5.0/CodeStudio-Lite-1.5.0-macOS-x64.dmg"
    },
    "linux-x86_64": {
      "signature": "<contents of the .AppImage.sig file>",
      "url": "https://download.codestudio.build/releases/1.5.0/CodeStudio-Lite-1.5.0-Linux-x64.AppImage"
    }
  }
}
```

Upload every artifact first and upload `stable/latest.json` last.

Generate the manifest after the signed platform builds finish:

```powershell
npm run updater:manifest -- `
  --version 1.5.0 `
  --base-url https://download.codestudio.build `
  --windows-installer "src-tauri/target/release/bundle/burn/CodeStudio-Lite-1.5.0-Windows-x64-setup.exe" `
  --macos-arm64-dmg "<downloaded signed arm64 macOS DMG>" `
  --macos-x64-dmg "<downloaded signed x64 macOS DMG>" `
  --linux-appimage "<downloaded Linux updater artifact>" `
  --linux-platform linux-x86_64 `
  --output dist-updater/latest.json
```

Install AWS CLI v2, then publish to R2:

```powershell
$env:CLOUDFLARE_ACCOUNT_ID = "<account-id>"
$env:R2_BUCKET = "codestudio-lite-updates"
$env:R2_ACCESS_KEY_ID = "<r2-access-key-id>"
$env:R2_SECRET_ACCESS_KEY = "<r2-secret-access-key>"

npm run updater:publish -- `
  --version 1.5.0 `
  --windows-installer "src-tauri/target/release/bundle/burn/CodeStudio-Lite-1.5.0-Windows-x64-setup.exe" `
  --macos-arm64-dmg "<downloaded signed arm64 macOS DMG>" `
  --macos-x64-dmg "<downloaded signed x64 macOS DMG>" `
  --linux-appimage "<downloaded Linux updater artifact>" `
  --linux-platform linux-x86_64 `
  --manifest dist-updater/latest.json `
  --dry-run
```

Remove `--dry-run` only after reviewing the object keys. The publisher records SHA-256 metadata, refuses to reuse an immutable key with different content, uploads immutable release objects first, and updates `stable/latest.json` last. To roll back, run it again with a previously saved manifest; versioned artifacts are not removed.
