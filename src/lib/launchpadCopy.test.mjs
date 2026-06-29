import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("launchpad and API profile navigation copy is localized consistently", () => {
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  assert.match(zhCN, /"app\.nav\.dashboard": "启动台"/);
  assert.match(zhCN, /"dashboard\.title": "工具启动台"/);
  assert.match(zhCN, /"dashboard\.subtitle": "管理工具状态、依赖和启动入口。"/);
  assert.match(zhCN, /"app\.nav\.profiles": "接入配置"/);
  assert.match(zhCN, /"app\.nav\.wizard": "新建配置"/);
  assert.match(zhCN, /"common\.createConfig": "新建配置"/);
  assert.match(zhCN, /"profiles\.loading": "正在加载配置\.\.\."/);
  assert.match(zhCN, /"profiles\.preview\.credentialDetail": "CodeStudio Lite 的配置元数据不会明文保存 API Key；配置可能会把所选 Provider Key 写入目标客户端原生配置。"/);
  assert.doesNotMatch(zhCN, /"[^"]*": "[^"]*(仪表盘|新建档案|配置档案)[^"]*"/);

  assert.match(enUS, /"app\.nav\.dashboard": "Launchpad"/);
  assert.match(enUS, /"dashboard\.title": "Tool Launchpad"/);
  assert.match(enUS, /"dashboard\.subtitle": "Manage tool status, dependencies, and launch actions\."/);
  assert.match(enUS, /"app\.nav\.profiles": "API Profiles"/);
  assert.match(enUS, /"app\.nav\.wizard": "New Configuration"/);
  assert.match(enUS, /"common\.createConfig": "New Configuration"/);
  assert.match(enUS, /"gateway\.subtitle": "Control the gateway runtime and choose independent gateway configurations by tool\."/);
  assert.match(enUS, /"gateway\.profileTitle": "Gateway configurations"/);
  assert.match(enUS, /"profiles\.loading": "Loading API profiles\.\.\."/);
  assert.match(enUS, /"profiles\.noActiveForToolInMode": "No API profile enabled for this configuration type"/);
  assert.match(enUS, /"profiles\.builtinOfficial": "Built-in official API profile"/);
  assert.match(enUS, /"profiles\.profileIsActive": "API profile is active"/);
  assert.match(enUS, /"profiles\.editProfile": "Edit API profile"/);
  assert.match(enUS, /"profiles\.duplicateProfile": "Duplicate API profile"/);
  assert.match(enUS, /"profiles\.deleteProfile": "Delete API profile"/);
  assert.match(enUS, /"profiles\.providerModeTitle": "Configuration type"/);
  assert.match(enUS, /"profiles\.mode\.config": "Direct configuration"/);
  assert.match(enUS, /"profiles\.mode\.gateway": "Gateway configuration"/);
  assert.match(enUS, /"profiles\.applyConfigMode": "Apply direct configuration"/);
  assert.match(enUS, /"profiles\.applyGatewayMode": "Apply gateway configuration"/);
  assert.match(enUS, /"profiles\.warning\.gatewayWritesClaude": "Gateway configurations write Claude Code settings to the tool-scoped local gateway URL\."/);
  assert.match(enUS, /"profiles\.warning\.configAdapterMissing": "Direct configuration adapter is not implemented for \{app\}\."/);
  assert.doesNotMatch(enUS, /"[^"]*": "[^"]*(Loading profiles|New Profile|Config profile|Gateway profile|gateway profiles)[^"]*"/);

  assert.match(zhTW, /"app\.nav\.dashboard": "啟動台"/);
  assert.match(zhTW, /"dashboard\.title": "工具啟動台"/);
  assert.match(zhTW, /"dashboard\.subtitle": "管理工具狀態、相依項和啟動入口。"/);
  assert.match(zhTW, /"app\.nav\.profiles": "串接設定"/);
  assert.match(zhTW, /"app\.nav\.wizard": "新建設定"/);
  assert.match(zhTW, /"common\.createConfig": "新建設定"/);
  assert.match(zhTW, /"profiles\.loading": "正在載入設定\.\.\."/);
  assert.match(zhTW, /"profiles\.iconImageOnly": "只能匯入圖片檔案。"/);
  assert.match(zhTW, /"profiles\.preview\.credentialDetail": "CodeStudio Lite 的設定元數據不會明文儲存 API Key；設定可能會把所選 Provider Key 寫入目標用戶端原生設定。"/);
  assert.doesNotMatch(zhTW, /"[^"]*": "[^"]*(儀表盤|新建檔案)[^"]*"/);
});

test("configuration name validation accepts old and new backend messages", () => {
  const setupWizard = read("src/routes/SetupWizard.svelte");
  const profiles = read("src/routes/Profiles.svelte");
  const acceptsRenamedMessage = /message === "Profile Name is required"\s*\|\|\s*message === "Configuration name is required"/;

  assert.match(setupWizard, acceptsRenamedMessage);
  assert.match(profiles, acceptsRenamedMessage);
});
