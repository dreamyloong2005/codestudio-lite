import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("Claude Desktop exposes Anthropic Messages protocol in config profile mode", () => {
  for (const source of [
    read("src/routes/SetupWizard.svelte"),
    read("src/routes/Profiles.svelte")
  ]) {
    assert.match(source, /"claude-desktop":\s*\["anthropic-messages"\]/);
    assert.match(source, /if \(mode === "gateway"\) \{\s*return protocolOptions;\s*\}/);
  }
});
