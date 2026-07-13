import type {
  ActiveProfilesByMode,
  BackupManifest,
  ProfileDraft,
  UsageQueryResult,
  UsageScriptConfig
} from "../../../types";

export type BrowserMockState = {
  activeProfilesByMode: ActiveProfilesByMode;
  backups: BackupManifest[];
  backupSnapshots: Record<string, ActiveProfilesByMode>;
  profileDrafts: ProfileDraft[];
  profileOrder: Record<string, string[]>;
  usageScripts: Map<string, UsageScriptConfig>;
  usageResults: Map<string, UsageQueryResult>;
};

export function createBrowserMockState(): BrowserMockState {
  return {
    activeProfilesByMode: { config: {}, gateway: {} },
    backups: [],
    backupSnapshots: {},
    profileDrafts: [],
    profileOrder: {},
    usageScripts: new Map(),
    usageResults: new Map()
  };
}
