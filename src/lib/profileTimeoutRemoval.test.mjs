import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

function source(path) {
  return readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
}

function between(content, start, end) {
  const startIndex = content.indexOf(start);
  assert.notEqual(startIndex, -1, `Missing start marker: ${start}`);
  const endIndex = content.indexOf(end, startIndex + start.length);
  assert.notEqual(endIndex, -1, `Missing end marker: ${end}`);
  return content.slice(startIndex, endIndex);
}

test("profile setup and edit UI do not expose profile-level timeout", () => {
  const setupWizard = source("src/routes/SetupWizard.svelte");
  const profiles = source("src/routes/Profiles.svelte");

  assert.equal(setupWizard.includes("timeoutSeconds"), false);
  assert.equal(setupWizard.includes("wizard.timeoutSeconds"), false);
  assert.equal(profiles.includes("editForm.timeoutSeconds"), false);
  assert.equal(profiles.includes("editTimeoutSeconds"), false);
  assert.equal(profiles.includes("wizard.timeoutSeconds"), false);
});

test("profile request and storage types do not include profile-level timeout", () => {
  const types = source("src/types.ts");
  const rustTypes = source("src-tauri/src/core/types.rs");

  for (const [name, start, end] of [
    ["ProfileDraft", "export interface ProfileDraft", "export type ProviderApplyMode"],
    ["SaveProfileDraftRequest", "export interface SaveProfileDraftRequest", "export interface UpdateProfileDraftRequest"],
    ["UpdateProfileDraftRequest", "export interface UpdateProfileDraftRequest", "export interface DuplicateProfileDraftRequest"],
    ["PreviewProfileWriteRequest", "export interface PreviewProfileWriteRequest", "export interface ProfileWritePreviewItem"]
  ]) {
    assert.equal(between(types, start, end).includes("timeoutSeconds"), false, `${name} still has timeoutSeconds`);
  }

  for (const [name, start, end] of [
    ["ProfileDraft", "pub struct ProfileDraft", "pub enum UsageScriptTemplateType"],
    ["SaveProfileDraftRequest", "pub struct SaveProfileDraftRequest", "pub struct UpdateProfileDraftRequest"],
    ["UpdateProfileDraftRequest", "pub struct UpdateProfileDraftRequest", "pub struct DuplicateProfileDraftRequest"],
    ["PreviewProfileWriteRequest", "pub struct PreviewProfileWriteRequest", "pub struct ProfileWritePreviewItem"]
  ]) {
    assert.equal(between(rustTypes, start, end).includes("timeout_seconds"), false, `${name} still has timeout_seconds`);
  }
});

test("profiles table and preview content do not persist profile-level timeout", () => {
  const storage = source("src-tauri/src/core/storage.rs");
  const api = source("src/lib/api.ts");

  const profileTable = between(storage, "CREATE TABLE IF NOT EXISTS profiles", "CREATE TABLE IF NOT EXISTS active_profiles");
  assert.equal(profileTable.includes("timeout_seconds"), false);
  assert.equal(between(api, "function mockProfileSqlPreviewContent", "function mockProfileIconPreview").includes("timeout_seconds"), false);
  assert.equal(api.includes("Network provider checks are not sent yet. Timeout is set"), false);
});
