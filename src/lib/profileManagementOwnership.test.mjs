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
  const wizard = read("src/routes/SetupWizard.svelte");

  assert.match(app, /function applySavedProfile\(profile: ProfileDraft\)/);
  assert.match(
    app,
    /async function refreshAfterProfileChange\(profile\?: ProfileDraft\) \{[\s\S]*?applySavedProfile\(profile\);[\s\S]*?await refreshProfileAndGatewayOnly\(\);/
  );
  assert.match(profiles, /const updated = await updateProfileDraft\(\{[\s\S]*?await onProfileSwitched\(updated\);/);
  assert.match(profiles, /const duplicated = await duplicateProfileDraft\(\{ profileId: profile\.id \}\);[\s\S]*?await onProfileSwitched\(duplicated\);/);
  assert.match(profiles, /const nextKey = profileListContentKey\(`\$\{mode\}:\$\{group\?\.id \?\? ""\}`, nextProfiles\);/);
  assert.doesNotMatch(profiles, /profileIdsFromItems\(nextProfiles\)\.join\("\|"\)/);
  assert.match(wizard, /export let onProfileSaved: \(profile: ProfileDraft\) => void \| Promise<void>/);
  assert.match(wizard, /const profile = await saveProfileDraft\([\s\S]*?await onProfileSaved\(profile\);/);
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
  const recipe = pandaConfig.match(/profileModeSwitcherRecipe: \{[\s\S]*?\n        \},\n        profileToolSwitcherRecipe:/)?.[0];

  assert.ok(recipe, "profileModeSwitcherRecipe is missing");
  assert.match(recipe, /gridTemplateColumns: "repeat\(2, minmax\(0, 1fr\)\)"/);
  assert.match(recipe, /"& button\[data-selected='true'\]"/);
});
