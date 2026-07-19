import type { ProfileDraft, ProfileModelOption } from "../../types";

export function providerIsOfficial(providerId: string): boolean {
  return providerId.trim() === "official";
}

export function profileUsesToolIcon(profile: ProfileDraft): boolean {
  return profile.isBuiltin && providerIsOfficial(profile.provider);
}

export function profileDisplayName(profile: ProfileDraft, officialName?: string): string {
  if (profileUsesToolIcon(profile) && officialName) {
    return officialName;
  }
  return profile.name;
}

export function profileEndpoint(profile: ProfileDraft): string | null {
  if (providerIsOfficial(profile.provider) && !profile.baseUrl.trim()) {
    return null;
  }
  return profile.baseUrl.trim() || null;
}

export function profileRemark(profile: ProfileDraft): string {
  return profile.remark?.trim() ?? "";
}

export function profileIconValue(profile: ProfileDraft, displayName: string): string {
  const icon = profile.icon?.trim();
  return icon || displayName.trim().charAt(0).toUpperCase() || "?";
}

export function profileIconIsImage(value: string): boolean {
  return value.startsWith("data:image/");
}

export function normalizedProfileIcon(value: string): string | null {
  const trimmed = value.trim();
  return trimmed || null;
}

export function profileIconTextTooLong(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.length > 0 && !profileIconIsImage(trimmed) && [...trimmed].length > 4;
}

export function profileModelOptionLabel(option: ProfileModelOption): string {
  const label = option.name && option.name !== option.id ? `${option.id} - ${option.name}` : option.id;
  return option.supports1m ? `${label} (1M)` : label;
}
