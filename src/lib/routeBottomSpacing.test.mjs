import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("long routes preserve workspace bottom padding while terminal still fills the viewport", () => {
  const app = read("src/App.svelte");
  const pandaConfig = read("panda.config.ts");
  const routeRecipe = pandaConfig.slice(
    pandaConfig.indexOf("appRouteTransitionRecipe:"),
    pandaConfig.indexOf("noticeRecipe:")
  );
  const routeRecipeBase = routeRecipe.slice(
    routeRecipe.indexOf("base:"),
    routeRecipe.indexOf("variants:")
  );

  assert.match(app, /appRouteTransitionRecipe\(\{ fill: route === "terminal" \}\)/);
  assert.match(routeRecipe, /base:\s*\{[\s\S]*?minHeight: "100%"/);
  assert.doesNotMatch(routeRecipeBase, /height: "100%"/);
  assert.match(routeRecipe, /variants:\s*\{[\s\S]*?fill:\s*\{[\s\S]*?true:\s*\{[\s\S]*?height: "100%"/);
});
