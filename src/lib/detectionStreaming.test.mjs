import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("environment detection streams each completed tool into the dashboard", () => {
  const command = read("src-tauri/src/commands/detect.rs");
  const detector = read("src-tauri/src/core/detector.rs");
  const api = read("src/lib/api.ts");
  const app = read("src/App.svelte");

  assert.match(command, /Channel<DetectionProgress>/);
  assert.match(command, /detect_environment_with_progress/);
  assert.match(detector, /progress\(DetectionProgress/);
  assert.match(detector, /CompletedDetection::Ai/);
  assert.match(detector, /CompletedDetection::ChatGptDesktop/);
  assert.match(detector, /CompletedDetection::System/);
  assert.match(detector, /for result in receiver/);
  assert.match(detector, /tool_status_with_generation/);
  assert.doesNotMatch(detector, /fn snapshot[\s\S]*detected_product_generation\(\)/);
  assert.match(command, /progress\.send\(event\)/);
  assert.match(api, /onProgress[\s\S]*new Channel<DetectionProgress>/);
  assert.match(app, /onProgress:[\s\S]*applyDetectionProgress/);
  assert.match(app, /mergeDetectionProgressSnapshot\(snapshot, progress\.snapshot\)/);
});
