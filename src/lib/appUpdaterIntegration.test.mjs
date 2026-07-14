import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("Tauri updater is registered with least-privilege desktop permissions", () => {
  const packageJson = JSON.parse(read("package.json"));
  const cargoToml = read("src-tauri/Cargo.toml");
  const tauriLib = read("src-tauri/src/lib.rs");
  const capability = read("src-tauri/capabilities/default.json");

  assert.equal(packageJson.dependencies["@tauri-apps/plugin-updater"], "^2.10.1");
  assert.equal(packageJson.dependencies["@tauri-apps/plugin-process"], "^2.3.1");
  assert.match(cargoToml, /tauri-plugin-updater\s*=\s*"2\.10\.1"/);
  assert.match(cargoToml, /tauri-plugin-process\s*=\s*"2\.3\.1"/);
  assert.match(tauriLib, /tauri_plugin_updater::Builder::new\(\)\.build\(\)/);
  assert.match(tauriLib, /tauri_plugin_process::init\(\)/);
  assert.match(capability, /"updater:default"/);
  assert.match(capability, /"process:allow-restart"/);
});

test("release builds generate updater configuration from R2 inputs", () => {
  const packageJson = JSON.parse(read("package.json"));
  const publicConfig = JSON.parse(read("updater.config.json"));
  const gitignore = read(".gitignore");
  const generator = read("scripts/prepare-updater-config.mjs");
  const burnBuild = read("installer/windows/burn/build-burn.ps1");
  const macosBuild = read("scripts/build-macos-updater.sh");
  const macosNormalizer = read("scripts/normalize-macos-artifacts.sh");
  const linuxBuild = read("scripts/build-linux-updater.sh");
  const linuxNormalizer = read("scripts/normalize-linux-artifacts.sh");

  assert.equal(packageJson.scripts["updater:config"], "node scripts/prepare-updater-config.mjs");
  assert.equal(publicConfig.baseUrl, "https://download.codestudio.build");
  assert.equal(typeof publicConfig.pubkey, "string");
  assert.match(packageJson.scripts["tauri:build:updater"], /tauri\.updater\.generated\.conf\.json/);
  assert.match(packageJson.scripts["updater:build:windows"], /installer\/windows\/burn\/build-burn\.ps1/);
  assert.match(packageJson.scripts["updater:build:windows"], /-RequireUpdater/);
  assert.equal(packageJson.scripts["updater:build:macos"], "bash scripts/build-macos-updater.sh");
  assert.equal(packageJson.scripts["updater:build:linux"], "bash scripts/build-linux-updater.sh");
  assert.equal(packageJson.scripts["normalize:linux"], "bash scripts/normalize-linux-artifacts.sh");
  assert.match(generator, /CODESTUDIO_UPDATE_BASE_URL/);
  assert.match(generator, /TAURI_UPDATER_PUBKEY/);
  assert.match(generator, /updater\.config\.json/);
  assert.match(generator, /replace\(\/\^\\uFEFF\//);
  assert.match(generator, /stable\/latest\.json/);
  assert.match(generator, /createUpdaterArtifacts/);
  assert.match(gitignore, /src-tauri\/tauri\.updater\.generated\.conf\.json/);
  assert.match(burnBuild, /tauri:build:updater -- --bundles msi/);
  assert.match(macosBuild, /tauri:build:updater -- --target universal-apple-darwin/);
  assert.match(macosNormalizer, /CodeStudio-Lite-\$\{VERSION\}-macOS-universal/);
  assert.match(linuxBuild, /normalize-linux-artifacts\.sh/);
  assert.match(linuxNormalizer, /CodeStudio-Lite-\$\{VERSION\}-Linux-\$\{ARCH_LABEL\}/);
  assert.match(linuxNormalizer, /command -v node/);
  assert.match(linuxNormalizer, /python3.*json/s);
  assert.match(linuxNormalizer, /\.AppImage/);
  assert.match(linuxNormalizer, /\.deb/);
  assert.match(linuxNormalizer, /\.rpm/);
  assert.match(burnBuild, /\[switch\]\$RequireUpdater/);
  assert.match(burnBuild, /Updater build requires CODESTUDIO_UPDATE_BASE_URL/);
});

test("R2 updater release commands do not depend on GitHub workflows", () => {
  const packageJson = JSON.parse(read("package.json"));
  const documentation = read("docs/r2-auto-update.md");

  assert.match(packageJson.scripts["updater:build:windows"], /build-burn\.ps1/);
  assert.match(packageJson.scripts["updater:build:macos"], /build-macos-updater\.sh/);
  assert.equal(packageJson.scripts["updater:publish"], "node scripts/publish-update-r2.mjs");
  assert.doesNotMatch(documentation, /GitHub Actions|GitHub Secrets|GitHub Variables/);
  assert.match(documentation, /GitHub is not required/);
});

test("updater signing key storage is locally protected and portably exportable", () => {
  const packageJson = JSON.parse(read("package.json"));
  const setup = read("scripts/setup-updater-signing-key.ps1");
  const exporter = read("scripts/export-updater-signing-key.ps1");
  const importer = read("scripts/import-updater-signing-key.ps1");
  const burnBuild = read("installer/windows/burn/build-burn.ps1");
  const gitignore = read(".gitignore");

  assert.match(packageJson.scripts["updater:key:init"], /setup-updater-signing-key\.ps1/);
  assert.match(packageJson.scripts["updater:key:export"], /export-updater-signing-key\.ps1/);
  assert.match(packageJson.scripts["updater:key:import"], /import-updater-signing-key\.ps1/);
  assert.match(setup, /RandomNumberGenerator/);
  assert.match(setup, /ProtectedData.*Protect/s);
  assert.match(setup, /updater\.config\.json/);
  assert.match(setup, /UTF8Encoding\(\$false\)/);
  assert.match(exporter, /Rfc2898DeriveBytes/);
  assert.match(exporter, /HashAlgorithmName\]::SHA256/);
  assert.match(exporter, /Aes.*Create/s);
  assert.match(exporter, /HMACSHA256/);
  assert.match(importer, /FixedTimeEquals/);
  assert.match(importer, /ProtectedData.*Protect/s);
  assert.match(importer, /password\.dpapi/);
  assert.match(burnBuild, /password\.dpapi/);
  assert.match(gitignore, /\*\.csl-updater-key/);
});

test("R2 publisher uploads immutable artifacts before the mutable channel manifest", () => {
  const packageJson = JSON.parse(read("package.json"));
  const publisher = read("scripts/publish-update-r2.mjs");

  assert.equal(packageJson.scripts["updater:publish"], "node scripts/publish-update-r2.mjs");
  assert.match(publisher, /head-object/);
  assert.match(publisher, /sha256File/);
  assert.match(publisher, /Refusing to overwrite immutable R2 object with different content/);
  assert.match(publisher, /stable\/latest\.json/);
  assert.match(publisher, /for \(const upload of immutableUploads\)[\s\S]*uploadMutable/);
  assert.match(publisher, /dry-run/);
  assert.match(publisher, /must use kebab-case/);
  assert.match(publisher, /must include .* between version and architecture/);
});

test("settings hands signed installer updates to Burn or DMG", () => {
  const store = read("src/lib/appUpdateStore.ts");
  const appInfo = read("src/lib/appInfo.ts");
  const settings = read("src/routes/Settings.svelte");
  const api = read("src/lib/api.ts");
  const commands = read("src-tauri/src/commands/app_updater.rs");
  const core = read("src-tauri/src/core/app_updater.rs");
  const tauriLib = read("src-tauri/src/lib.rs");
  const cargo = read("src-tauri/Cargo.toml");
  const locales = ["en-US", "zh-CN", "zh-TW"].map((locale) => read(`src/lib/locales/${locale}.ts`));

  assert.match(store, /@tauri-apps\/plugin-updater/);
  assert.match(store, /@tauri-apps\/plugin-process/);
  assert.match(store, /!isTauri\(\) \|\| !APP_UPDATER_ENABLED/);
  assert.match(store, /installInFlight/);
  assert.match(store, /status:\s*"unconfigured"/);
  assert.doesNotMatch(store, /api\.github\.com|fetchGitHubRelease|GITHUB_RELEASES_API_URL/);
  assert.doesNotMatch(appInfo, /GITHUB_RELEASES_API_URL|api\.github\.com/);
  assert.match(store, /pendingUpdate\.rawJson/);
  assert.match(store, /installerArtifactForCurrentPlatform/);
  assert.match(store, /installApplicationUpdate/);
  assert.match(store, /listen<AppUpdateProgress>\("app-update-progress"/);
  assert.match(store, /update\.downloadAndInstall\(/);
  assert.doesNotMatch(store, /update\.download\(|update\.install\(|relaunch/);
  assert.match(api, /invoke\("install_application_update"/);
  assert.match(commands, /pub async fn install_application_update/);
  assert.match(core, /download_http::download_to_file/);
  assert.match(core, /minisign_verify::\{PublicKey, Signature\}/);
  assert.match(core, /verify_stream/);
  assert.match(core, /launch_windows_burn/);
  assert.match(core, /-quiet/);
  assert.match(core, /-norestart/);
  assert.match(core, /launch_macos_dmg_helper/);
  assert.match(core, /hdiutil/);
  assert.match(core, /ditto/);
  assert.match(core, /gateway::shutdown_for_app_exit/);
  assert.match(core, /app\.exit\(0\)/);
  assert.match(tauriLib, /commands::app_updater::install_application_update/);
  assert.match(cargo, /minisign-verify\s*=\s*"0\.2\.5"/);
  assert.match(store, /installAppUpdate/);
  assert.match(settings, /installAppUpdate/);
  const versionLabelIndex = settings.indexOf("<span>{APP_VERSION_LABEL}</span>");
  const updatePillIndex = settings.lastIndexOf("<span class={settingsUpdatePillRecipe");
  const installUpdateIndex = settings.lastIndexOf('title={$t("settings.installUpdate")}');
  const checkUpdatesIndex = settings.lastIndexOf('{$t("settings.checkUpdates")}');
  assert.ok(versionLabelIndex >= 0);
  assert.ok(versionLabelIndex < updatePillIndex);
  assert.ok(updatePillIndex < installUpdateIndex);
  assert.ok(installUpdateIndex < checkUpdatesIndex);
  assert.match(settings, /settings\.updateNow/);
  assert.match(settings, /downloadedBytes/);
  for (const locale of locales) {
    assert.match(locale, /"settings\.updateNow"/);
    assert.match(locale, /"settings\.downloadingUpdate"/);
    assert.match(locale, /"settings\.installingUpdate"/);
    assert.match(locale, /"settings\.updaterNotConfigured"/);
  }
});
