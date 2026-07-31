# macOS Release Pipeline Hardening Design

## Scope

This change hardens the dependency and macOS release workflow without changing application behavior or restructuring large application modules. It covers the Panda CSS dependency, local dual-architecture updater builds, GitHub Actions quality gates, updater signing, artifact normalization, verification, and artifact retention.

Large Rust and Svelte module decomposition is explicitly deferred to a separate design because it has a different risk profile and test strategy.

## Goals

- Move Panda CSS to the maintained 1.x release line and eliminate vulnerabilities introduced by the 0.31.0 dependency graph.
- Make local and CI macOS updater builds use the same per-target build and verification path.
- Build architecture-independent frontend assets only once during a local dual-architecture release.
- Prevent stale or unrelated files from being normalized as current release artifacts.
- Require updater signatures for release artifacts.
- Verify version, architecture, code signature, DMG integrity, and updater signature before an artifact is accepted.
- Run frontend and Rust quality gates before CI packaging.
- Keep GitHub build artifacts for 14 days.

## Non-Goals

- Apple Developer ID certificate provisioning.
- Mandatory Apple notarization before credentials exist.
- Publishing artifacts to R2 from GitHub Actions.
- Refactoring large application modules.
- Changing Windows or Linux packaging behavior beyond preserving their existing updater configuration path.

## Dependency Design

`@pandacss/dev` will move from `^0.31.0` to `^1.12.0`, and `package-lock.json` will be regenerated with the repository's npm version. The upgrade is accepted only if Panda code generation, Svelte checking, unit tests, production frontend build, and npm audit complete successfully. Audit results must contain no high-severity findings caused by the Panda dependency graph.

## Release Architecture

The release workflow will have three layers:

1. The existing dual-architecture macOS entry point coordinates a local release. It generates frontend assets once, prepares an updater configuration that disables Tauri's repeated frontend build hook, and invokes the per-target entry point for arm64 and x64.
2. A per-target entry point owns one Tauri updater build, exact artifact normalization, DMG signing, and verification. GitHub Actions invokes this entry point once for each matrix target.
3. Artifact helpers define exact raw and canonical paths for a target and version. They never select the first matching file from a directory.

The supported target mapping remains:

| Rust target | Release label | Mach-O architecture | Tauri DMG architecture |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | `arm64` | `arm64` | `aarch64` |
| `x86_64-apple-darwin` | `x64` | `x86_64` | `x64` |

## Build Flow

For a local dual-architecture build:

1. Validate macOS, updater private key, and updater key password inputs.
2. Run `npm run build` once.
3. Generate the temporary updater Tauri configuration with updater artifacts enabled and `beforeBuildCommand` disabled.
4. For each supported target, remove only that target's expected raw and canonical current-version artifacts.
5. Run the Tauri build without rebuilding frontend assets.
6. Rename only the exact expected raw updater archive, updater archive signature, and DMG to canonical names.
7. Sign the canonical DMG with the updater key.
8. Verify the application version, executable architecture, macOS code signature, DMG checksum, and both updater signatures.
9. Remove the temporary updater configuration on success or failure.

For GitHub Actions, the matrix job performs steps 3 through 8 for its single target. It may build frontend assets once inside that job because matrix jobs do not share a workspace.

## Artifact Contract

For version `VERSION`, the accepted outputs are:

- `CodeStudio-Lite-VERSION-macOS-arm64.dmg`
- `CodeStudio-Lite-VERSION-macOS-arm64.dmg.sig`
- `CodeStudio-Lite-VERSION-macOS-arm64.app.tar.gz`
- `CodeStudio-Lite-VERSION-macOS-arm64.app.tar.gz.sig`
- `CodeStudio-Lite-VERSION-macOS-x64.dmg`
- `CodeStudio-Lite-VERSION-macOS-x64.dmg.sig`
- `CodeStudio-Lite-VERSION-macOS-x64.app.tar.gz`
- `CodeStudio-Lite-VERSION-macOS-x64.app.tar.gz.sig`

Normalization fails if an exact expected raw artifact is absent. Existing files for older versions remain untouched and are never candidates for the current release.

## Signing and Notarization

Updater signing is mandatory. Local builds continue to read `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. GitHub Actions reads the same values from repository secrets and fails clearly if either value is missing.

Apple Developer ID signing and notarization remain optional until credentials are available. The workflow will pass supported Apple signing and notarization environment variables through when configured, but their absence will not fail packaging. Ad-hoc macOS signing remains the current fallback, so external Gatekeeper trust is not claimed.

## CI Design

The workflow will contain a quality job and a packaging matrix job.

The quality job runs:

- `npm ci`
- `npm test`
- `cargo test --locked`
- `cargo clippy --locked --all-targets -- -D warnings`

The packaging matrix depends on the quality job, installs the required Rust target, restores npm and Cargo caches, and invokes the shared per-target updater build. Each job uploads the app bundle, DMG, DMG signature, updater archive, and updater archive signature with a 14-day retention period.

Tag builds and manually dispatched builds use the same workflow. Missing updater signing secrets fail before compilation starts.

## Error Handling

- Unsupported targets fail before deleting or building artifacts.
- Missing signing variables fail before compilation.
- Only current-version target-specific artifact paths may be removed.
- Missing raw outputs fail normalization instead of falling back to directory scanning.
- Version or architecture mismatch fails before DMG signing is reported as successful.
- Empty or missing signature files fail verification.
- Temporary updater configuration is removed through a shell exit trap.
- A failed matrix target does not produce a successful release artifact set.

## Testing

Automated tests will cover:

- Target-to-label and expected-path mapping.
- Exact normalization of current-version files.
- Preservation of stale files from older versions.
- Failure when an expected raw DMG or updater archive is missing.
- Failure when a signature is missing or empty.
- Generation of an updater configuration with the frontend build hook disabled only when requested.
- Static workflow assertions for required quality commands, signed artifact uploads, and 14-day retention.
- A release-script contract proving that the dual-architecture entry point builds frontend assets once and delegates both targets.

Final verification will run the full frontend test suite, Rust tests, clippy with warnings denied, production frontend build, npm audit, and a signed local dual-architecture macOS updater build. The resulting applications and DMGs will be checked independently with `lipo`, `codesign`, and `hdiutil`.

## Security and Operational Notes

- Private keys and passwords must never be written to generated configuration or command output.
- GitHub secrets are passed only to the packaging step.
- Generated updater configuration remains ignored and is deleted after use.
- Immutable R2 publishing behavior remains unchanged.
- No release is described as notarized until Apple notarization credentials are configured and notarization succeeds.
