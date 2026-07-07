import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8").replace(/\r\n/g, "\n");

test("profile model pickers show the full fetched model list instead of datalist filtering", () => {
  const setupWizard = read("src/routes/SetupWizard.svelte");
  const profiles = read("src/routes/Profiles.svelte");
  const picker = read("src/components/ModelSelectInput.svelte");

  for (const source of [setupWizard, profiles]) {
    assert.match(source, /ModelSelectInput/);
    assert.doesNotMatch(source, /<datalist\b/);
    assert.doesNotMatch(source, /\slist=\{[^}]*modelOptions/i);
  }

  assert.match(picker, /{#each options as option, index}/);
  assert.doesNotMatch(picker, /filteredOptions|options\.filter/);
  assert.match(picker, /aria-autocomplete="none"/);
});
