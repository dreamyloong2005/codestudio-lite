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
} from "../../types";

export interface ProfileAdapter {
  ensureAppDirs(): Promise<ProfileSummary>;
  loadSummary(): Promise<ProfileSummary>;
  testConnection(request: TestProfileConnectionRequest): Promise<TestProfileConnectionResult>;
  listModels(request: ListProfileModelsRequest): Promise<ListProfileModelsResult>;
  save(request: SaveProfileDraftRequest): Promise<ProfileDraft>;
  startCodexOAuthLogin(): Promise<StartCodexOAuthLoginResult>;
  update(request: UpdateProfileDraftRequest): Promise<ProfileDraft>;
  duplicate(request: DuplicateProfileDraftRequest): Promise<ProfileDraft>;
  delete(request: DeleteProfileDraftRequest): Promise<ProfileSummary>;
  reorder(request: ReorderProfileDraftsRequest): Promise<ProfileSummary>;
  loadUsage(profileId: string): Promise<UsageScriptState>;
  saveUsage(request: UsageScriptSaveRequest): Promise<UsageScriptState>;
  testUsage(request: UsageScriptSaveRequest): Promise<UsageQueryResult>;
  queryUsage(profileId: string): Promise<UsageQueryResult>;
  deleteUsage(profileId: string): Promise<UsageScriptState>;
  previewWrite(request: PreviewProfileWriteRequest): Promise<PreviewProfileWriteResult>;
  previewApply(request: PreviewProfileApplyRequest): Promise<PreviewProfileApplyResult>;
  apply(request: ApplyProfileRequest): Promise<ApplyProfileResult>;
}
