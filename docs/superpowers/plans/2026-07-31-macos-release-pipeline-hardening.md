# macOS Release Pipeline Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make dependency upgrades and macOS updater releases reproducible, signed, stale-artifact-safe, and consistently verified locally and in GitHub Actions.

**Architecture:** Keep the existing shell entry points, add a small JavaScript artifact-contract module for exact target/version paths and filesystem normalization, and make both local and CI builds delegate to one per-target updater script. The local dual-target script builds Vite once; CI matrix jobs build their isolated frontend once each.

**Tech Stack:** npm/npm-lockfile, Panda CSS, Vite, Tauri 2, Bash, Node.js `node:test`, Rust Cargo, GitHub Actions.

---

### Task 1: Upgrade Panda CSS and establish the dependency gate

**Files:**
- Modify: `package.json:48-57`
- Modify: `package-lock.json`
- Create: `src/lib/releasePipeline.test.mjs`

- [ ] **Step 1: Write the failing dependency assertion**

Add a test that reads `package.json` and asserts `devDependencies["@pandacss/dev"]` is `^1.12.0` or another explicitly approved 1.x version, and rejects `0.31.0`.

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `node --test --test-name-pattern='Panda' src/lib/releasePipeline.test.mjs`

Expected: FAIL because the current package declares `^0.31.0`.

- [ ] **Step 3: Upgrade and regenerate the lockfile**

Change `@pandacss/dev` to `^1.12.0`, run `npm install`, and allow the existing `prepare` hook to regenerate Panda output. Do not use `npm audit fix --force`, because it can mutate unrelated application dependencies.

- [ ] **Step 4: Run the dependency checks**

Run:

```bash
npm run panda:codegen
npm run check
npm run test:unit
npm audit --audit-level=high
```

Expected: code generation, type checks, and tests pass; audit reports no high-severity vulnerabilities. Moderate advisories must be reviewed and recorded if they remain.

- [ ] **Step 5: Commit the dependency change**

```bash
git add package.json package-lock.json src/lib/releasePipeline.test.mjs
git commit -m "fix: restore maintained Panda CSS dependency"
```

### Task 2: Add a tested macOS artifact contract

**Files:**
- Create: `scripts/macos-artifact-contract.mjs`
- Modify: `src/lib/releasePipeline.test.mjs`
- Modify: `package.json:33-34`

- [ ] **Step 1: Write failing contract tests**

In `src/lib/releasePipeline.test.mjs`, import the contract module and test:

```js
test("macOS target metadata maps Rust targets to release labels", () => {
  assert.deepEqual(targetMetadata("aarch64-apple-darwin"), {
    architecture: "arm64",
    tauriArchitecture: "aarch64",
    executableArchitecture: "arm64"
  });
  assert.deepEqual(targetMetadata("x86_64-apple-darwin"), {
    architecture: "x64",
    tauriArchitecture: "x64",
    executableArchitecture: "x86_64"
  });
});

test("macOS artifact paths use exact raw and canonical names", () => {
  const paths = macosArtifactPaths("/tmp/bundle", "1.5.2", "aarch64-apple-darwin");
  assert.equal(paths.raw.dmg, "/tmp/bundle/dmg/CodeStudio Lite_1.5.2_aarch64.dmg");
  assert.equal(paths.canonical.dmg, "/tmp/bundle/dmg/CodeStudio-Lite-1.5.2-macOS-arm64.dmg");
  assert.equal(paths.canonical.archiveSignature, "/tmp/bundle/macos/CodeStudio-Lite-1.5.2-macOS-arm64.app.tar.gz.sig");
});

test("unsupported macOS targets fail before creating paths", () => {
  assert.throws(() => targetMetadata("darwin-universal"), /Unsupported macOS target/);
});
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run: `node --test --test-name-pattern='macOS target|macOS artifact|unsupported' src/lib/releasePipeline.test.mjs`

Expected: FAIL because `scripts/macos-artifact-contract.mjs` does not exist.

- [ ] **Step 3: Implement the contract module**

Export `MACOS_TARGETS`, `targetMetadata(target)`, and `macosArtifactPaths(bundleRoot, version, target)`. Return exact raw paths for Tauri's current output and exact canonical paths for the updater server. Reject every target not in the two-entry map.

- [ ] **Step 4: Include the script tests in the normal unit command**

Change `test:unit` to run both `src/lib/*.test.mjs` and `scripts/tests/*.test.mjs` after the existing TypeScript test compilation. Keep all tests executable with `node --test` directly.

- [ ] **Step 5: Run the focused and existing tests**

Run:

```bash
node --test src/lib/releasePipeline.test.mjs
npm run test:unit
```

Expected: all tests pass.

- [ ] **Step 6: Commit the contract**

```bash
git add package.json scripts/macos-artifact-contract.mjs src/lib/releasePipeline.test.mjs
git commit -m "test: define macOS release artifact contract"
```

### Task 3: Make updater configuration and normalization stale-safe

**Files:**
- Modify: `scripts/prepare-updater-config.mjs`
- Create: `scripts/normalize-macos-artifacts.mjs`
- Modify: `scripts/normalize-macos-artifacts.sh`
- Create: `scripts/tests/macosArtifactNormalization.test.mjs`

- [ ] **Step 1: Write failing normalization tests**

Create a temporary bundle fixture containing a current raw DMG, current raw updater archive, both raw archive signatures, and an older unrelated DMG. Test that normalization moves only the exact current raw files, creates the canonical names, preserves the older file, and throws when a required current raw DMG is absent.

- [ ] **Step 2: Run the normalization tests to verify they fail**

Run: `node --test scripts/tests/macosArtifactNormalization.test.mjs`

Expected: FAIL because the exact normalization module does not exist.

- [ ] **Step 3: Implement exact normalization**

Implement `normalizeMacosArtifacts({ bundleRoot, version, target })` in the Node module. Use `macosArtifactPaths` and `renameSync`; never use a wildcard or “first match” lookup. Refuse to overwrite an existing canonical artifact unless it was explicitly removed by the caller for the same version. Move archive signatures alongside their archive and fail if an expected raw file or signature is missing.

- [ ] **Step 4: Add opt-in frontend build skipping to updater config generation**

Parse `--skip-before-build` in `prepare-updater-config.mjs`. When present, add `build: { beforeBuildCommand: "" }` to the generated config; otherwise preserve the current config shape so Windows and Linux callers remain unchanged. Preserve HTTPS endpoint validation and never write signing secrets.

- [ ] **Step 5: Add config-generation tests**

Extend `src/lib/releasePipeline.test.mjs` to execute the config generator with a temporary output path and assert that `--skip-before-build` emits an empty `build.beforeBuildCommand`, while the default output omits the override.

- [ ] **Step 6: Run all focused tests**

Run:

```bash
node --test src/lib/releasePipeline.test.mjs scripts/tests/macosArtifactNormalization.test.mjs
```

Expected: all tests pass with stale files preserved.

- [ ] **Step 7: Commit the stale-safe release helpers**

```bash
git add scripts/prepare-updater-config.mjs scripts/normalize-macos-artifacts.mjs scripts/normalize-macos-artifacts.sh scripts/tests/macosArtifactNormalization.test.mjs src/lib/releasePipeline.test.mjs
git commit -m "fix: reject stale macOS release artifacts"
```

### Task 4: Share per-target builds and avoid duplicate local frontend builds

**Files:**
- Create: `scripts/build-macos-updater-target.sh`
- Modify: `scripts/build-macos-updater.sh`
- Create: `scripts/verify-macos-artifacts.sh`
- Modify: `scripts/tests/macosArtifactNormalization.test.mjs`

- [ ] **Step 1: Write the release-script contract test**

Add a test that reads both shell scripts and asserts the dual-target script invokes `npm run build` exactly once, delegates both supported targets to `build-macos-updater-target.sh`, and does not call the generic `tauri:build` path directly.

- [ ] **Step 2: Run the contract test to verify it fails**

Run: `node --test --test-name-pattern='dual-target' scripts/tests/macosArtifactNormalization.test.mjs`

Expected: FAIL because the per-target entry point and one-build orchestration do not exist.

- [ ] **Step 3: Implement the per-target updater script**

The script must:

1. Require macOS, a supported target, `TAURI_SIGNING_PRIVATE_KEY`, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
2. Read the version from `src-tauri/tauri.conf.json`.
3. Remove only current-version raw/canonical files from the target bundle directory.
4. Run `npm run build` unless `--frontend-ready` is supplied.
5. Generate updater config with `--skip-before-build` after frontend assets are ready.
6. Run `npx tauri build --config src-tauri/tauri.updater.generated.conf.json --target "$target"`.
7. Normalize exact raw paths.
8. Sign the canonical DMG with `npx tauri signer sign`.
9. Run `verify-macos-artifacts.sh`.
10. Remove the generated config through an exit trap.

- [ ] **Step 4: Update the dual-target script**

Run `npm run build` once, then invoke the per-target script twice with `--frontend-ready`. Keep the existing target-to-label mapping and base URL/signing environment behavior. Stop immediately on either target failure.

- [ ] **Step 5: Implement independent macOS artifact verification**

`verify-macos-artifacts.sh` must validate the canonical DMG, DMG signature, updater archive, archive signature, and app bundle. It must run `lipo -archs`, `codesign --verify --deep --strict`, `hdiutil verify`, and non-empty signature checks, failing with a target-specific message on any mismatch.

- [ ] **Step 6: Run shell syntax and contract checks**

Run:

```bash
bash -n scripts/build-macos-updater.sh scripts/build-macos-updater-target.sh scripts/verify-macos-artifacts.sh scripts/normalize-macos-artifacts.sh
node --test scripts/tests/macosArtifactNormalization.test.mjs
```

Expected: syntax checks and contract tests pass. A real signed build remains the final integration test.

- [ ] **Step 7: Commit the shared build path**

```bash
git add scripts/build-macos-updater.sh scripts/build-macos-updater-target.sh scripts/verify-macos-artifacts.sh scripts/normalize-macos-artifacts.sh scripts/normalize-macos-artifacts.mjs scripts/tests/macosArtifactNormalization.test.mjs
git commit -m "build: share verified macOS updater packaging"
```

### Task 5: Align GitHub Actions with the verified release path

**Files:**
- Modify: `.github/workflows/build-macos.yml`
- Modify: `scripts/tests/releaseWorkflow.test.mjs`
- Modify: `package.json:33-34` only if the test file requires an additional glob

- [ ] **Step 1: Write failing workflow contract tests**

Assert that the workflow contains `npm test`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, the updater signing secret names, the shared per-target script, `needs: quality`, all four updater artifact patterns, and `retention-days: 14`.

- [ ] **Step 2: Run the workflow tests to verify they fail**

Run: `node --test scripts/tests/releaseWorkflow.test.mjs`

Expected: FAIL because the current workflow uses ordinary `tauri:build`, has no quality job, and does not upload updater signatures.

- [ ] **Step 3: Add the quality job**

Create a macOS quality job that runs `npm ci`, `npm test`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings` from `src-tauri`. Keep it independent from the packaging matrix so failures are reported clearly.

- [ ] **Step 4: Replace packaging commands with the shared target script**

Make the matrix jobs depend on `quality`, install the target with `dtolnay/rust-toolchain`, restore npm/Cargo caches, and invoke `bash scripts/build-macos-updater-target.sh` with the matrix target. Pass `CODESTUDIO_UPDATE_BASE_URL`, updater key, and updater password through the job environment. Pass optional Apple variables when corresponding secrets are configured.

- [ ] **Step 5: Upload only verified release outputs**

Upload the app bundle, DMG, DMG signature, updater archive, and updater archive signature. Set `if-no-files-found: error` and `retention-days: 14`.

- [ ] **Step 6: Run workflow contract and YAML checks**

Run:

```bash
node --test scripts/tests/releaseWorkflow.test.mjs
git diff --check
```

Expected: tests pass and the workflow diff has no whitespace errors.

- [ ] **Step 7: Commit CI alignment**

```bash
git add .github/workflows/build-macos.yml scripts/tests/releaseWorkflow.test.mjs package.json
git commit -m "ci: verify and publish signed macOS updater artifacts"
```

### Task 6: Full verification and release handoff

**Files:**
- No source changes expected.

- [ ] **Step 1: Run frontend and Rust quality checks**

Run:

```bash
npm ci
npm test
(cd src-tauri && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings)
```

Expected: all commands exit `0` with no warnings denied by Clippy.

- [ ] **Step 2: Run dependency and frontend build checks**

Run:

```bash
npm audit --audit-level=high
npm run build
```

Expected: no high-severity advisories and a successful Vite build.

- [ ] **Step 3: Run the signed local dual-architecture updater build**

With the existing Desktop `updater.key` and `password.txt` injected without printing secrets, run `npm run updater:build:macos`.

Expected: arm64 and x64 builds exit `0`, generate canonical DMGs and updater archives with signatures, and pass independent verification.

- [ ] **Step 4: Verify artifacts independently**

Run `lipo -archs`, `codesign --verify --deep --strict --verbose=2`, and `hdiutil verify` against both architectures. Confirm all eight canonical artifact files are non-empty.

- [ ] **Step 5: Confirm repository state and summarize operational prerequisites**

Run:

```bash
git diff --check
git status --short --branch
```

Expected: only intentional source/documentation changes remain. Report that updater signing secrets are required in CI and Apple notarization remains disabled until credentials are configured.

- [ ] **Step 6: Commit final verified changes**

```bash
git add package.json package-lock.json scripts .github/workflows/build-macos.yml src/lib
git commit -m "chore: harden macOS release pipeline"
```
