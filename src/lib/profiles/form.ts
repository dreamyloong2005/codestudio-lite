import type { ProfileDraft, ProfileModelMapping } from "../../types";
import { profileSupportsModelMappings } from "./catalog";

export type ProfileModelMappingForm = {
  alias: string;
  model: string;
  supports1m: boolean;
  description: string;
};

export function emptyProfileModelMappingForm(): ProfileModelMappingForm {
  return { alias: "", model: "", supports1m: false, description: "" };
}

export function modelMappingFormsFromProfile(profile: ProfileDraft): ProfileModelMappingForm[] {
  return (profile.modelMappings ?? []).map((mapping) => ({
    alias: mapping.alias,
    model: mapping.model,
    supports1m: Boolean(mapping.supports1m),
    description: mapping.description ?? ""
  }));
}

export function modelMappingsForRequest(
  toolId: string,
  mappings: ProfileModelMappingForm[]
): ProfileModelMapping[] {
  if (!profileSupportsModelMappings(toolId)) return [];
  return mappings
    .map((mapping) => ({
      alias: mapping.alias.trim(),
      model: mapping.model.trim(),
      supports1m: Boolean(mapping.supports1m),
      description: mapping.description.trim() || null
    }))
    .filter((mapping) => mapping.alias || mapping.model || mapping.description);
}

export function profileModelMappingsAreValid(mappings: ProfileModelMappingForm[]): boolean {
  const aliases = new Set<string>();
  for (const mapping of mappings) {
    const alias = mapping.alias.trim();
    const model = mapping.model.trim();
    const description = mapping.description.trim();
    if (!alias && !model && !description) continue;
    if (!alias || !model) return false;
    const aliasKey = alias.toLowerCase();
    if (aliases.has(aliasKey)) return false;
    aliases.add(aliasKey);
  }
  return true;
}
