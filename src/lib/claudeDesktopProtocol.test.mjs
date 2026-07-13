import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("Claude Desktop exposes Anthropic Messages protocol in config profile mode", () => {
  const catalog = read("src/lib/profiles/catalog.ts");
  assert.match(catalog, /id:\s*"claude-desktop"[\s\S]*?configProtocols:\s*\["anthropic-messages"\]/);
  for (const source of [
    read("src/routes/SetupWizard.svelte"),
    read("src/routes/Profiles.svelte")
  ]) {
    assert.match(source, /configProtocolIdsForTool/);
    assert.match(source, /if \(mode === "gateway"\) \{\s*return protocolOptions;\s*\}/);
  }
});

test("gateway profile protocol field is presented as the upstream API", () => {
  const setupWizard = read("src/routes/SetupWizard.svelte");
  const profiles = read("src/routes/Profiles.svelte");
  const zhCN = read("src/lib/locales/zh-CN.ts");
  const zhTW = read("src/lib/locales/zh-TW.ts");
  const enUS = read("src/lib/locales/en-US.ts");

  assert.match(
    setupWizard,
    /\$t\(profileMode === "gateway" \? "wizard\.upstreamApi" : "wizard\.protocol"\)/
  );
  assert.match(
    profiles,
    /\$t\(editForm\.mode === "gateway" \? "wizard\.upstreamApi" : "wizard\.protocol"\)/
  );

  assert.match(zhCN, /"wizard\.upstreamApi": "上游 API"/);
  assert.match(zhTW, /"wizard\.upstreamApi": "上游 API"/);
  assert.match(enUS, /"wizard\.upstreamApi": "Upstream API"/);
});
