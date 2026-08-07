import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflow = readFileSync(new URL("../../.github/workflows/build-macos.yml", import.meta.url), "utf8");

test("macOS workflow gates packaging on frontend and Rust quality checks", () => {
  assert.match(workflow, /npm test/);
  assert.match(workflow, /cargo test --locked/);
  assert.match(workflow, /cargo clippy --locked --all-targets -- -D warnings/);
  assert.match(workflow, /needs: quality/);
});

test("macOS workflow uses the signed per-target updater path", () => {
  assert.match(workflow, /build-macos-updater-target\.sh/);
  assert.match(workflow, /TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(workflow, /TAURI_SIGNING_PRIVATE_KEY_PASSWORD/);
  assert.match(workflow, /\.dmg\.sig/);
  assert.match(workflow, /\.app\.tar\.gz/);
  assert.match(workflow, /retention-days: 14/);
});
