import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

function componentInvocation(source, component) {
  const match = source.match(new RegExp(`<${component}\\b[\\s\\S]*?\\/>`));
  assert.ok(match, `${component} invocation is missing`);
  return match[0];
}

test("App owns the active profile-management mode and returns every saved profile to Access Profiles", () => {
  const app = read("src/App.svelte");
  const profilesInvocation = componentInvocation(app, "Profiles");
  const gatewayInvocation = componentInvocation(app, "Gateway");

  assert.match(app, /let profileManagementMode: ProviderApplyMode = "config";/);
  assert.match(
    app,
    /onProfileSaved=\{\(profile\) => \{[\s\S]*?applySavedProfile\(profile\);[\s\S]*?profileManagementMode = profile\.mode;[\s\S]*?route = "profiles";/
  );
  assert.doesNotMatch(app, /route = mode === "gateway" \? "gateway" : "profiles"/);

  assert.match(profilesInvocation, /bind:modeFilter=\{profileManagementMode\}/);
  assert.match(profilesInvocation, /onCreateProfile=\{\(prefill\) => openWizard\(prefill \?\? null\)\}/);
  assert.doesNotMatch(profilesInvocation, /modeFilter="config"/);
  assert.doesNotMatch(profilesInvocation, /mode: "config"/);

  assert.doesNotMatch(gatewayInvocation, /\bsummary=/);
  assert.doesNotMatch(gatewayInvocation, /\{snapshot\}/);
  assert.doesNotMatch(gatewayInvocation, /onProfileSwitched=/);
  assert.doesNotMatch(gatewayInvocation, /onCreateProfile=/);
});

test("saved and edited profiles update the shared summary before background refresh", () => {
  const app = read("src/App.svelte");
  const profiles = read("src/routes/Profiles.svelte");
  const profileList = read("src/components/profiles/ProfileList.svelte");
  const wizard = read("src/routes/SetupWizard.svelte");

  assert.match(app, /function applySavedProfile\(profile: ProfileDraft\)/);
  assert.match(
    app,
    /async function refreshAfterProfileChange\(profile\?: ProfileDraft\) \{[\s\S]*?applySavedProfile\(profile\);[\s\S]*?await refreshProfileAndGatewayOnly\(\);/
  );
  assert.match(profiles, /const updated = await updateProfileDraft\(\{[\s\S]*?await onProfileSwitched\(updated\);/);
  assert.match(profiles, /const duplicated = await duplicateProfileDraft\(\{ profileId: profile\.id \}\);[\s\S]*?await onProfileSwitched\(duplicated\);/);
  assert.match(profileList, /const nextKey = profileListContentKey\(`\$\{nextMode\}:\$\{nextToolId\}`, nextProfiles\);/);
  assert.doesNotMatch(profileList, /profileIdsFromItems\(nextProfiles\)\.join\("\|"\)/);
  assert.match(wizard, /export let onProfileSaved: \(profile: ProfileDraft\) => void \| Promise<void>/);
  assert.match(wizard, /const profile = await saveProfileDraft\([\s\S]*?await onProfileSaved\(profile\);/);
});

test("new gateway profiles auto-select only when unset and never expose restart", () => {
  const api = read("src/lib/api.ts");
  const browserProfiles = read("src/lib/api/browserMock/profiles.ts");
  const browserWritePreview = read("src/lib/api/browserMock/profileWritePreview.ts");
  const rustProfile = read("src-tauri/src/core/profile/manager.rs");
  const profiles = read("src/routes/Profiles.svelte");
  const wizard = read("src/routes/SetupWizard.svelte");
  const enUS = read("src/lib/locales/en-US.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");

  assert.match(
    browserProfiles,
    /state\.profileDrafts = \[\.\.\.state\.profileDrafts, profile\];\s*activateGatewayIfUnset\(profile\);/
  );
  assert.match(
    browserProfiles,
    /const activateGatewayIfUnset = \(profile: ProfileDraft\) => \{[\s\S]*?!gatewayWillAutoActivate\(profile\.app, profile\.mode\)[\s\S]*?cleanActive\(\);[\s\S]*?state\.activeProfilesByMode =/
  );
  assert.match(browserWritePreview, /const autoActivateGateway = dependencies\.gatewayWillAutoActivate\(app, mode\);/);
  assert.match(browserWritePreview, /action: autoActivateGateway \? "update" : "not_modified"/);
  assert.match(
    rustProfile,
    /let auto_activate_gateway = if preview_profile\.mode == ProviderApplyMode::Gateway \{[\s\S]*?gateway_profile_will_auto_activate\(&config, &preview_profile, &drafts\)/
  );
  assert.match(
    rustProfile,
    /action: if auto_activate_gateway \{\s*"update"\.to_string\(\)[\s\S]*?"not_modified"\.to_string\(\)/
  );
  assert.match(
    profiles,
    /\$: canApplyAndRestart =\s*pendingApply\?\.mode === "config" &&\s*selectedApplyMode === "config" &&\s*Boolean\(selectedModePreview\?\.writesNativeConfig\);/
  );
  assert.match(profiles, /\{#if canApplyAndRestart\}[\s\S]*?\$t\("profiles\.applyAndRestart"\)/);
  assert.doesNotMatch(
    profiles,
    /\{#if selectedApplyMode === "config" && selectedModePreview\?\.writesNativeConfig\}/
  );
  assert.match(wizard, /if \(action === "update"\) \{\s*return \$t\("common\.update"\);/);
  assert.match(
    wizard,
    /item\.label === "Active tool profile pointer"[\s\S]*?item\.action === "update"[\s\S]*?wizard\.preview\.activeProfilePointerAutoApplyDetail/
  );
  assert.match(enUS, /"wizard\.preview\.activeProfilePointerAutoApplyDetail":/);
  assert.match(zhCN, /"wizard\.preview\.activeProfilePointerAutoApplyDetail":/);
  assert.match(zhTW, /"wizard\.preview\.activeProfilePointerAutoApplyDetail":/);
});

test("Access Profiles exposes a header switch for configuration-file and gateway profiles", () => {
  const profiles = read("src/routes/Profiles.svelte");

  assert.match(profiles, /profileModeSwitcherRecipe/);
  assert.match(profiles, /value: "config", labelKey: "profiles\.view\.config"/);
  assert.match(profiles, /value: "gateway", labelKey: "profiles\.view\.gateway"/);
  assert.match(profiles, /<h1>\{\$t\("profiles\.title"\)\}<\/h1>/);
  assert.match(profiles, /role="group" aria-label=\{\$t\("profiles\.viewSwitcherLabel"\)\}/);
  assert.match(profiles, /data-selected=\{normalizedModeFilter === option\.value\}/);
  assert.match(profiles, /aria-pressed=\{normalizedModeFilter === option\.value\}/);
  assert.match(profiles, /on:click=\{\(\) => \(modeFilter = option\.value\)\}/);
  assert.match(profiles, /mode: normalizedModeFilter/);
  assert.doesNotMatch(profiles, /routeTitleKey/);
  assert.doesNotMatch(profiles, /gateway\.profileTitle/);
});

test("Gateway route owns runtime controls and logs but no profile-management surface", () => {
  const gateway = read("src/routes/Gateway.svelte");

  assert.doesNotMatch(gateway, /import Profiles from/);
  assert.doesNotMatch(gateway, /<Profiles\b/);
  assert.doesNotMatch(gateway, /export let summary:/);
  assert.doesNotMatch(gateway, /export let snapshot:/);
  assert.doesNotMatch(gateway, /export let onProfileSwitched:/);
  assert.doesNotMatch(gateway, /export let onCreateProfile:/);

  assert.match(gateway, /onGatewayAction/);
  assert.match(gateway, /onPrivacyFilterChange/);
  assert.match(gateway, /onCopyGatewayUrl/);
  assert.match(gateway, /gateway\.requestLogTitle/);
  assert.match(gateway, /loadGatewayRequestLog/);
});

test("profile-management switch copy is localized and Gateway copy describes runtime ownership", () => {
  const enUS = read("src/lib/locales/en-US.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");

  assert.match(enUS, /"profiles\.viewSwitcherLabel": "Switch profile type"/);
  assert.match(enUS, /"profiles\.view\.config": "Direct configuration"/);
  assert.match(enUS, /"profiles\.view\.gateway": "Gateway configuration"/);
  assert.match(enUS, /"gateway\.subtitle": "Manage gateway runtime, privacy filtering, and request logs\."/);

  assert.match(zhCN, /"profiles\.viewSwitcherLabel": "切换接入配置类型"/);
  assert.match(zhCN, /"profiles\.view\.config": "直连配置"/);
  assert.match(zhCN, /"profiles\.view\.gateway": "网关配置"/);
  assert.match(zhCN, /"gateway\.subtitle": "管理网关运行状态、隐私过滤和请求日志。"/);

  assert.match(zhTW, /"profiles\.viewSwitcherLabel": "切換串接設定類型"/);
  assert.match(zhTW, /"profiles\.view\.config": "直連設定"/);
  assert.match(zhTW, /"profiles\.view\.gateway": "閘道設定"/);
  assert.match(zhTW, /"gateway\.subtitle": "管理閘道執行狀態、隱私過濾和請求日誌。"/);
});

test("Panda defines a stable two-option profile mode switch", () => {
  const pandaConfig = read("panda.config.ts");
  const recipe = pandaConfig.match(/profileModeSwitcherRecipe: \{[\s\S]*?\r?\n        \},\r?\n        profileToolSwitcherRecipe:/)?.[0];

  assert.ok(recipe, "profileModeSwitcherRecipe is missing");
  assert.match(recipe, /gridTemplateColumns: "repeat\(2, minmax\(0, 1fr\)\)"/);
  assert.match(recipe, /"& button\[data-selected='true'\]"/);
});

test("Codex profiles own an optional review model across forms, cards, mocks, and locales", () => {
  const types = read("src/types.ts");
  const wizard = read("src/routes/SetupWizard.svelte");
  const profiles = read("src/routes/Profiles.svelte");
  const api = read("src/lib/api.ts");
  const browserProfileStore = read("src/lib/api/browserMock/profileStore.ts");
  const browserWritePreview = read("src/lib/api/browserMock/profileWritePreview.ts");
  const browserNativePreview = read("src/lib/api/browserMock/nativePreview.ts");
  const rustTypes = read("src-tauri/src/core/types.rs");
  const enUS = read("src/lib/locales/en-US.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");

  assert.match(types, /reviewModel: string \| null;/);
  assert.equal((types.match(/reviewModel\?: string \| null;/g) ?? []).length, 3);
  assert.match(rustTypes, /pub review_model: Option<String>,/);

  assert.match(wizard, /let reviewModel = "";/);
  assert.match(wizard, /\$: supportsReviewModel = canonicalProfileToolId\(selectedTool\) === "codex";/);
  assert.match(wizard, /reviewModel: activeReviewModel/);
  assert.match(wizard, /\{#if supportsReviewModel\}[\s\S]*?profiles\.reviewModelLabel[\s\S]*?bind:value=\{reviewModel\}/);

  assert.match(profiles, /reviewModel: string;/);
  assert.match(profiles, /reviewModel: profile\.reviewModel \?\? ""/);
  assert.match(profiles, /reviewModel: editSupportsReviewModel[\s\S]*?editForm\.reviewModel\.trim\(\)/);
  assert.match(profiles, /\{#if editSupportsReviewModel\}[\s\S]*?profiles\.reviewModelLabel[\s\S]*?editForm\.reviewModel/);
  assert.match(profiles, /profile\.reviewModel[\s\S]*?profiles\.reviewModelLabel/);

  assert.match(browserWritePreview, /reviewModel: store\.normalizeReviewModel\(app, request\.reviewModel\)/);
  assert.match(browserProfileStore, /normalizeReviewModel\(app: string, value\?: string \| null\)/);
  assert.match(browserNativePreview, /function effectiveMockCodexReviewModel\(/);
  assert.match(browserNativePreview, /key: "review_model"/);
  assert.match(browserWritePreview, /review_model: input\.reviewModel/);

  assert.match(enUS, /"profiles\.reviewModelPlaceholder": "Leave blank to follow the primary model"/);
  assert.match(zhCN, /"profiles\.reviewModelPlaceholder": "留空时跟随主模型"/);
  assert.match(zhTW, /"profiles\.reviewModelPlaceholder": "留空時跟隨主模型"/);
});

test("Pi Agent is owned across lifecycle, profiles, previews, assets, locales, and docs", () => {
  const api = read("src/lib/api.ts");
  const wizard = read("src/routes/SetupWizard.svelte");
  const profiles = read("src/routes/Profiles.svelte");
  const icon = read("src/components/ToolIcon.svelte");
  const panda = read("panda.config.ts");
  const registry = read("src-tauri/src/core/tool_registry.rs");
  const detector = read("src-tauri/src/core/detector.rs");
  const installer = read("src-tauri/src/core/tool_installer.rs");
  const launcher = read("src-tauri/src/core/tool_launch.rs");
  const toolCatalog = read("src-tauri/src/core/tool_catalog.rs");
  const profileCatalog = read("src/lib/profiles/catalog.ts");
  const profileGrouping = read("src/lib/profiles/grouping.ts");
  const browserProfileStore = read("src/lib/api/browserMock/profileStore.ts");
  const browserProfilePolicy = read("src/lib/api/browserMock/profilePolicy.ts");
  const browserNativePreview = read("src/lib/api/browserMock/nativePreview.ts");
  const enUS = read("src/lib/locales/en-US.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const readme = read("README.md");
  const piIcon = read("public/tool-icons/pi.svg");

  assert.match(registry, /id: "pi",[\s\S]*?name: "Pi Agent",[\s\S]*?command: "pi",[\s\S]*?config_relative_path: Some\("\.pi\/agent\/models\.json"\),[\s\S]*?@earendil-works\/pi-coding-agent/);
  assert.match(detector, /"pi" => Some\("@earendil-works\/pi-coding-agent"\)/);
  assert.match(detector, /npm install -g --ignore-scripts @earendil-works\/pi-coding-agent@latest/);
  assert.match(installer, /"pi" => InstallAction::NpmGlobalIgnoreScripts\("@earendil-works\/pi-coding-agent"\)/);
  assert.match(launcher, /canonical_tool_id as canonical_profile_app/);
  assert.match(toolCatalog, /"pi" \| "pi-agent" \| "pi-coding-agent" => "pi"\.to_string\(\)/);

  assert.match(api, /pi: "0\.80\.6"/);
  assert.match(browserProfileStore, /\["pi", "Pi Agent Official", "anthropic-messages"\]/);
  assert.match(api, /id: "pi",[\s\S]*?name: "Pi Agent",[\s\S]*?configPath: "~\/\.pi\/agent\/models\.json"/);
  assert.match(api, /pi: \{\s*toolName: "Pi Agent",\s*manager: "npm",\s*command: "npm install -g --ignore-scripts @earendil-works\/pi-coding-agent"/s);
  assert.match(api, /canonicalProfileToolId as canonicalProfileApp/);
  assert.ok((browserNativePreview.match(/app === "pi"/g) ?? []).length >= 3, "Pi mock previews should cover official, direct, and gateway modes");
  assert.match(browserProfilePolicy, /profileSupportsConfigProtocol\(app, normalized\)/);
  assert.match(api, /pi: "~\/\.pi\/agent\/models\.json"/);

  assert.match(profileCatalog, /id: "pi",[\s\S]*?label: "Pi Agent",[\s\S]*?defaultProfileNameKey: "wizard\.defaultProfile\.pi"/);
  assert.match(profileCatalog, /configProtocols: \["openai-chat-completions", "openai-responses", "anthropic-messages", "google-gemini"\]/);
  assert.match(profileCatalog, /\["pi", "pi-agent", "pi-coding-agent"\]\.includes\(normalized\)/);
  assert.match(wizard, /PROFILE_TOOL_CATALOG/);
  assert.match(profileGrouping, /PROFILE_TOOL_ORDER/);
  assert.match(profiles, /OFFICIAL_PROFILE_NAME_KEYS/);
  assert.match(profiles, /profiles\.warning\.piConfigWrites/);
  assert.match(profiles, /profiles\.warning\.piSelectModel/);

  assert.match(icon, /pi: \{ src: "\/tool-icons\/pi\.svg", tone: "pi" \}/);
  assert.match(icon, /case "pi-agent":[\s\S]*?case "pi-coding-agent":[\s\S]*?return "pi"/);
  assert.match(panda, /data-tool-icon-tone='pi'/);
  assert.match(panda, /pi: \{[\s\S]*?background:/);
  assert.match(piIcon, /<svg\b/);

  for (const dictionary of [enUS, zhCN, zhTW]) {
    assert.match(dictionary, /"profiles\.officialProfile\.pi"/);
    assert.match(dictionary, /"profiles\.warning\.piConfigWrites"/);
    assert.match(dictionary, /"profiles\.warning\.piSelectModel"/);
    assert.match(dictionary, /"wizard\.defaultProfile\.pi"/);
  }
  assert.equal((readme.match(/Pi Agent/g) ?? []).length >= 4, true);
});
