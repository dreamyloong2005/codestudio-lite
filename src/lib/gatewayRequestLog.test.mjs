import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("Gateway request log labels privacy status without implying route misses", () => {
  const route = read("src/routes/Gateway.svelte");
  const enUS = read("src/lib/locales/en-US.ts");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");

  assert.match(route, /entry\.privacyFilterMode === "off"/);
  assert.match(route, /gateway\.privacyAction\.off/);
  assert.match(route, /gateway\.privacyAction\.noPrivacyHit/);

  assert.match(enUS, /"gateway\.privacyAction\.off": "Privacy off"/);
  assert.match(enUS, /"gateway\.privacyAction\.noPrivacyHit": "No privacy hit"/);
  assert.match(zhCN, /"gateway\.privacyAction\.off": "隐私过滤未启用"/);
  assert.match(zhCN, /"gateway\.privacyAction\.noPrivacyHit": "无隐私命中"/);
  assert.match(zhTW, /"gateway\.privacyAction\.off": "隱私過濾未啟用"/);
  assert.match(zhTW, /"gateway\.privacyAction\.noPrivacyHit": "無隱私命中"/);
});
