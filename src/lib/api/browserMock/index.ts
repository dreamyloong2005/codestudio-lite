import type { ProfileAdapter } from "../profiles";
import type { BrowserMockState } from "./state";

export type BrowserProfileModules = {
  summary: ProfileAdapter["ensureAppDirs"];
  testConnection: ProfileAdapter["testConnection"];
  listModels: ProfileAdapter["listModels"];
  save: ProfileAdapter["save"];
  startCodexOAuthLogin: ProfileAdapter["startCodexOAuthLogin"];
  update: ProfileAdapter["update"];
  duplicate: ProfileAdapter["duplicate"];
  delete: ProfileAdapter["delete"];
  reorder: ProfileAdapter["reorder"];
  loadUsage: ProfileAdapter["loadUsage"];
  saveUsage: ProfileAdapter["saveUsage"];
  testUsage: ProfileAdapter["testUsage"];
  queryUsage: ProfileAdapter["queryUsage"];
  deleteUsage: ProfileAdapter["deleteUsage"];
  previewWrite: ProfileAdapter["previewWrite"];
  previewApply: ProfileAdapter["previewApply"];
  apply: ProfileAdapter["apply"];
};

export function browserProfileAdapter(
  _state: BrowserMockState,
  modules: BrowserProfileModules
): ProfileAdapter {
  return {
    ensureAppDirs: modules.summary,
    loadSummary: modules.summary,
    testConnection: modules.testConnection,
    listModels: modules.listModels,
    save: modules.save,
    startCodexOAuthLogin: modules.startCodexOAuthLogin,
    update: modules.update,
    duplicate: modules.duplicate,
    delete: modules.delete,
    reorder: modules.reorder,
    loadUsage: modules.loadUsage,
    saveUsage: modules.saveUsage,
    testUsage: modules.testUsage,
    queryUsage: modules.queryUsage,
    deleteUsage: modules.deleteUsage,
    previewWrite: modules.previewWrite,
    previewApply: modules.previewApply,
    apply: modules.apply
  };
}
