import type {
  DetectionSnapshot,
  ProfileDraft,
  ProfileSummary,
  ProviderApplyMode
} from "../../types";
import {
  canonicalProfileToolId,
  PROFILE_TOOL_LABELS,
  PROFILE_TOOL_ORDER
} from "./catalog";

export type ProfileGroup = {
  id: string;
  label: string;
  activeProfileId: string | null;
  activeProfileName: string | null;
  profiles: ProfileDraft[];
};

export type ProfileModeSection = {
  mode: ProviderApplyMode;
  groups: ProfileGroup[];
};

export function installedProfileToolIds(detection: DetectionSnapshot | null): Set<string> | null {
  if (!detection) {
    return null;
  }
  return new Set(
    detection.tools
      .filter((tool) => tool.installState === "installed")
      .map((tool) => canonicalProfileToolId(tool.id))
  );
}

export function profileModeSections(
  summary: ProfileSummary | null,
  installedToolIds: Set<string> | null,
  mode: ProviderApplyMode
): ProfileModeSection[] {
  const drafts = summary?.drafts ?? [];
  const activeByMode = summary?.activeProfilesByMode ?? { config: {}, gateway: {} };
  return [{
    mode,
    groups: profileGroups(
      drafts.filter((profile) => profile.mode === mode),
      activeByMode[mode],
      installedToolIds
    )
  }];
}

export function activeProfileIdForTool(activeProfiles: Record<string, string>, toolId: string): string | null {
  if (activeProfiles[toolId]) {
    return activeProfiles[toolId];
  }
  if (toolId === "codex") {
    return activeProfiles["chatgpt-desktop"]
      ?? activeProfiles["codex-app"]
      ?? activeProfiles["codex-client"]
      ?? activeProfiles["codex-desktop"]
      ?? null;
  }
  return null;
}

export function profileIsActive(summary: ProfileSummary | null, profile: ProfileDraft): boolean {
  if (!summary) {
    return false;
  }
  const toolId = canonicalProfileToolId(profile.app);
  return activeProfileIdForTool(summary.activeProfilesByMode[profile.mode], toolId) === profile.id;
}

export function shouldShowNoInstalledProfiles(
  summary: ProfileSummary | null,
  visibleCount: number,
  installedToolIds: Set<string> | null
): boolean {
  return Boolean(summary && installedToolIds && summary.drafts.length > 0 && visibleCount === 0);
}

function profileGroups(
  profiles: ProfileDraft[],
  activeProfiles: Record<string, string>,
  installedToolIds: Set<string> | null
): ProfileGroup[] {
  const grouped = new Map<string, ProfileDraft[]>();
  for (const profile of profiles) {
    const toolId = canonicalProfileToolId(profile.app);
    if (installedToolIds && !installedToolIds.has(toolId)) {
      continue;
    }
    grouped.set(toolId, [...(grouped.get(toolId) ?? []), { ...profile, app: toolId }]);
  }

  return [...grouped.entries()]
    .sort(([left], [right]) => compareTools(left, right))
    .map(([toolId, toolProfiles]) => {
      const activeProfileId = activeProfileIdForTool(activeProfiles, toolId);
      const profiles = [...toolProfiles].sort(compareProfiles);
      return {
        id: toolId,
        label: PROFILE_TOOL_LABELS[toolId] ?? toolId,
        activeProfileId,
        activeProfileName: profiles.find((profile) => profile.id === activeProfileId)?.name ?? null,
        profiles
      };
    });
}

function compareTools(left: string, right: string): number {
  const leftIndex = PROFILE_TOOL_ORDER.indexOf(left);
  const rightIndex = PROFILE_TOOL_ORDER.indexOf(right);
  if (leftIndex === -1 && rightIndex === -1) {
    return left.localeCompare(right);
  }
  if (leftIndex === -1) {
    return 1;
  }
  if (rightIndex === -1) {
    return -1;
  }
  return leftIndex - rightIndex;
}

function compareProfiles(left: ProfileDraft, right: ProfileDraft): number {
  if (left.isBuiltin !== right.isBuiltin) {
    return left.isBuiltin ? -1 : 1;
  }
  return left.sortOrder - right.sortOrder || left.name.localeCompare(right.name);
}
