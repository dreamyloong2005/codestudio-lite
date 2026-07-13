import type {
  ApplyProfileRequest,
  ApplyProfileResult,
  BackupManifest,
  DetectionSnapshot,
  PreviewProfileApplyRequest,
  PreviewProfileApplyResult,
  ProfileDraft,
  ProfileSummary
} from "../../../types";
import { canonicalProfileToolId } from "../../profiles/catalog";
import { createBrowserProfileStore } from "./profileStore";
import type { BrowserMockState } from "./state";

export type BrowserProfileApplyDependencies = {
  detection(): DetectionSnapshot;
  toolConfigPath(toolId: string): string | null;
  nativeConfigPath(profile: ProfileDraft): string | null;
  nativePreview(profile: ProfileDraft, path: string | null, mode: "config" | "gateway"): PreviewProfileApplyResult["nativeDiff"];
  modePreviews(
    profile: ProfileDraft,
    config: PreviewProfileApplyResult["nativeDiff"],
    gateway: PreviewProfileApplyResult["nativeDiff"]
  ): PreviewProfileApplyResult["modePreviews"];
  envConflicts(): PreviewProfileApplyResult["envConflicts"];
  summary(): ProfileSummary;
  restartMessage(profile: ProfileDraft, syncClaudeVsCode: boolean): string;
  recordActivity(message: string): void;
};

export function createBrowserProfileApply(state: BrowserMockState, dependencies: BrowserProfileApplyDependencies) {
  const store = createBrowserProfileStore(state);

  const preview = async (request: PreviewProfileApplyRequest): Promise<PreviewProfileApplyResult> => {
    const profile = store.all().find((draft) => draft.id === request.profileId);
    if (!profile) throw new Error(`Profile '${request.profileId}' does not exist`);
    const toolId = canonicalProfileToolId(profile.app);
    const tool = dependencies.detection().tools.find((item) => item.id === profile.app);
    const nativeConfigPath = dependencies.nativeConfigPath(profile) ?? tool?.configPath ?? dependencies.toolConfigPath(profile.app);
    const canApply = Boolean(tool) || toolId === "codex";
    const configNativeDiff = dependencies.nativePreview(profile, nativeConfigPath, "config");
    const gatewayNativeDiff = dependencies.nativePreview(profile, nativeConfigPath, "gateway");
    const nativeDiff = profile.mode === "config" ? configNativeDiff : gatewayNativeDiff;
    return {
      generatedAt: new Date().toISOString(), profileId: profile.id, profileName: profile.name,
      app: profile.app, provider: profile.provider, canApply, nativeDiff,
      modePreviews: dependencies.modePreviews(profile, configNativeDiff, gatewayNativeDiff),
      warnings: canApply ? [] : [`Tool '${profile.app}' is not in the preview registry.`],
      envConflicts: toolId === "claude" ? dependencies.envConflicts() : [],
      items: [
        { label: "Active tool profile pointer", path: "~/.codestudio-lite/app_state.sqlite", action: "update", backupRequired: false, detail: `Sets the SQLite active profile pointer for '${profile.app}' to '${profile.id}' before refreshing detection.` },
        { label: `${tool?.name ?? "Target tool"} native config`, path: nativeConfigPath, action: nativeDiff ? "create_or_update" : "not_modified", backupRequired: Boolean(nativeDiff), detail: nativeDiff ? "Selected profile type writes this client config; detailed file changes are shown below." : "This profile does not require a native client config write." },
        { label: "Credential", path: null, action: "not_written", backupRequired: false, detail: "CodeStudio Lite profile metadata never stores plaintext API keys. Config profiles may write the selected Provider key into the target client's native config." }
      ]
    };
  };

  const apply = async (request: ApplyProfileRequest): Promise<ApplyProfileResult> => {
    const profile = store.all().find((draft) => draft.id === request.profileId);
    if (!profile) throw new Error(`Profile '${request.profileId}' does not exist`);
    const active = state.activeProfilesByMode[profile.mode];
    if (Object.entries(active).some(([app, id]) => canonicalProfileToolId(app) === canonicalProfileToolId(profile.app) && id === profile.id)) {
      throw new Error("Profile is already active for this tool and profile category.");
    }
    const result = await preview(request);
    if (!result.canApply) throw new Error(`Profile '${request.profileId}' cannot be applied yet.`);
    const mode = profile.mode;
    if (request.restartAfterApply && mode !== "config") throw new Error("Apply and restart is only available for Config profiles.");
    const syncClaudeVsCode = Boolean(request.syncClaudeVsCode) && mode === "config" && canonicalProfileToolId(profile.app) === "claude";
    const selected = result.modePreviews.find((item) => item.mode === mode);
    if (!selected?.supported) throw new Error(selected?.blockedReason ?? `${mode} is not supported for this profile.`);
    if (request.restartAfterApply && !selected.writesNativeConfig) {
      throw new Error("Apply and restart requires a native client config write for this profile.");
    }
    const backupId = new Date().toISOString().replaceAll(":", "-");
    const nativePath = selected.writesNativeConfig ? selected.nativeDiff?.path ?? null : null;
    state.backupSnapshots[backupId] = cloneActive(state.activeProfilesByMode);
    state.activeProfilesByMode = {
      ...state.activeProfilesByMode,
      [mode]: { ...state.activeProfilesByMode[mode], [canonicalProfileToolId(profile.app)]: profile.id }
    };
    const backup: BackupManifest = {
      id: backupId, reason: "apply-profile", profile: profile.id,
      changedFiles: [...(nativePath ? [nativePath] : []), ...(syncClaudeVsCode ? ["~/.claude/config.json"] : [])],
      createdAt: new Date().toISOString()
    };
    state.backups = [backup, ...state.backups];
    dependencies.recordActivity(mode === "gateway"
      ? `Applied profile '${profile.name}' for ${profile.app}/${profile.provider} in Gateway profile.`
      : `Applied profile '${profile.name}' for ${profile.app}/${profile.provider} through direct client config profile.`);
    return {
      summary: dependencies.summary(), mode, backup, appliedPath: "~/.codestudio-lite/app_state.sqlite",
      verified: true, nativePath, nativeVerified: Boolean(nativePath),
      restartRequested: Boolean(request.restartAfterApply), restartPerformed: false,
      restartMessage: request.restartAfterApply ? dependencies.restartMessage(profile, syncClaudeVsCode) : null,
      gatewayStatus: null,
      envConflicts: canonicalProfileToolId(profile.app) === "claude" ? dependencies.envConflicts() : []
    };
  };

  return { preview, apply };
}

function cloneActive(value: BrowserMockState["activeProfilesByMode"]): BrowserMockState["activeProfilesByMode"] {
  return { config: { ...value.config }, gateway: { ...value.gateway } };
}
