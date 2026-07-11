import type { GatewayStatus, ProfileDraft, ProfileSummary } from "../types";

export function upsertProfileDraftInSummary(
  summary: ProfileSummary | null,
  profile: ProfileDraft
): ProfileSummary | null {
  if (!summary) {
    return null;
  }

  const existingIndex = summary.drafts.findIndex((draft) => draft.id === profile.id);
  const drafts = existingIndex === -1
    ? [...summary.drafts, profile]
    : summary.drafts.map((draft, index) => (index === existingIndex ? profile : draft));

  return {
    ...summary,
    activeProfileName: summary.activeProfile === profile.id ? profile.name : summary.activeProfileName,
    drafts
  };
}

export function updateGatewayProfileDisplay(
  status: GatewayStatus | null,
  profile: ProfileDraft
): GatewayStatus | null {
  if (!status || status.activeProfileId !== profile.id || status.activeProfileName === profile.name) {
    return status;
  }

  return {
    ...status,
    activeProfileName: profile.name
  };
}
