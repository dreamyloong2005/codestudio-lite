import type {
  ApplyProfileRequest,
  ApplyProfileResult,
  DeleteProfileDraftRequest,
  DuplicateProfileDraftRequest,
  ListProfileModelsRequest,
  ListProfileModelsResult,
  PreviewProfileApplyRequest,
  PreviewProfileApplyResult,
  PreviewProfileWriteRequest,
  PreviewProfileWriteResult,
  ProfileDraft,
  ProfileSummary,
  ReorderProfileDraftsRequest,
  SaveProfileDraftRequest,
  StartCodexOAuthLoginResult,
  TestProfileConnectionRequest,
  TestProfileConnectionResult,
  UpdateProfileDraftRequest,
  UsageQueryResult,
  UsageScriptSaveRequest,
  UsageScriptState
} from "../../../types";
import type { RuntimeAdapter } from "../runtime";
import type { ProfileAdapter } from "../profiles";

export function tauriProfileAdapter(runtime: RuntimeAdapter): ProfileAdapter {
  return {
    ensureAppDirs: () => runtime.invoke<ProfileSummary>("ensure_app_dirs"),
    loadSummary: () => runtime.invoke<ProfileSummary>("load_profile_summary"),
    testConnection: (request: TestProfileConnectionRequest) =>
      runtime.invoke<TestProfileConnectionResult>("test_profile_connection", { request }),
    listModels: (request: ListProfileModelsRequest) =>
      runtime.invoke<ListProfileModelsResult>("list_profile_models", { request }),
    save: (request: SaveProfileDraftRequest) =>
      runtime.invoke<ProfileDraft>("save_profile_draft", { request }),
    startCodexOAuthLogin: () => runtime.invoke<StartCodexOAuthLoginResult>("start_codex_oauth_login"),
    update: (request: UpdateProfileDraftRequest) =>
      runtime.invoke<ProfileDraft>("update_profile_draft", { request }),
    duplicate: (request: DuplicateProfileDraftRequest) =>
      runtime.invoke<ProfileDraft>("duplicate_profile_draft", { request }),
    delete: (request: DeleteProfileDraftRequest) =>
      runtime.invoke<ProfileSummary>("delete_profile_draft", { request }),
    reorder: (request: ReorderProfileDraftsRequest) =>
      runtime.invoke<ProfileSummary>("reorder_profile_drafts", { request }),
    loadUsage: (profileId: string) =>
      runtime.invoke<UsageScriptState>("load_usage_script_state", { profileId }),
    saveUsage: (request: UsageScriptSaveRequest) =>
      runtime.invoke<UsageScriptState>("save_usage_script", { request }),
    testUsage: (request: UsageScriptSaveRequest) =>
      runtime.invoke<UsageQueryResult>("test_usage_script", { request }),
    queryUsage: (profileId: string) =>
      runtime.invoke<UsageQueryResult>("query_profile_usage", { profileId }),
    deleteUsage: (profileId: string) =>
      runtime.invoke<UsageScriptState>("delete_usage_script", { profileId }),
    previewWrite: (request: PreviewProfileWriteRequest) =>
      runtime.invoke<PreviewProfileWriteResult>("preview_profile_write", { request }),
    previewApply: (request: PreviewProfileApplyRequest) =>
      runtime.invoke<PreviewProfileApplyResult>("preview_profile_apply", { request }),
    apply: (request: ApplyProfileRequest) =>
      runtime.invoke<ApplyProfileResult>("apply_profile", { request })
  };
}
