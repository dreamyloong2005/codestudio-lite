import type { ProfileDraft, ProfileModelMapping, ProviderApplyMode } from "../../../types";
import { canonicalProfileToolId } from "../../profiles/catalog";
import type { BrowserMockState } from "./state";

const BUILTIN_DEFINITIONS = [
  ["codex", "Codex Official", "openai-responses"],
  ["claude-desktop", "Claude Desktop Official", "anthropic-messages"],
  ["claude", "Claude Code Official", "anthropic-messages"],
  ["gemini", "Gemini CLI Official", "google-gemini"],
  ["gemini-code-assist", "Gemini Code Assist Official", "google-gemini"],
  ["opencode", "OpenCode Official", "openai-chat-completions"],
  ["openclaw", "OpenClaw Official", "openai-chat-completions"],
  ["hermes", "Hermes Official", "openai-chat-completions"],
  ["grok", "Grok Official", "openai-responses"],
  ["pi", "Pi Agent Official", "anthropic-messages"]
] as const;

export function createBrowserProfileStore(state: BrowserMockState) {
  const orderKey = (app: string, mode: ProviderApplyMode) =>
    `${canonicalProfileToolId(app)}:${mode}`;

  const builtinId = (app: string) => `builtin-official-${canonicalProfileToolId(app)}`;

  const builtins = (): ProfileDraft[] => BUILTIN_DEFINITIONS.map(([app, name, protocol]) => ({
    id: builtinId(app), name, icon: null, remark: null, app, isBuiltin: true, mode: "config",
    provider: "official", protocol, model: "", reviewModel: null, modelMappings: [], baseUrl: "",
    authRef: null, createdAt: null, updatedAt: null, lastTestStatus: "builtin", usageEnabled: false, sortOrder: 0
  }));

  const compare = (left: ProfileDraft, right: ProfileDraft) =>
    canonicalProfileToolId(left.app).localeCompare(canonicalProfileToolId(right.app))
    || left.mode.localeCompare(right.mode)
    || left.sortOrder - right.sortOrder
    || left.name.localeCompare(right.name);

  const all = (): ProfileDraft[] => {
    const profiles = [...builtins(), ...state.profileDrafts].map((profile) => ({
      ...profile,
      usageEnabled: state.usageScripts.get(profile.id)?.enabled ?? profile.usageEnabled
    }));
    const groups = new Set(profiles.map((profile) => orderKey(profile.app, profile.mode)));
    for (const group of groups) {
      const storedOrder = state.profileOrder[group];
      if (!storedOrder?.length) continue;
      const orderById = new Map(storedOrder.map((profileId, index) => [profileId, index]));
      let nextIndex = storedOrder.length;
      for (const profile of profiles.filter((item) => orderKey(item.app, item.mode) === group).sort(compare)) {
        profile.sortOrder = orderById.get(profile.id) ?? nextIndex++;
      }
    }
    return profiles.sort(compare);
  };

  return {
    all,
    builtinId,
    isBuiltinId: (profileId: string) => profileId.startsWith("builtin-official-"),
    orderKey,
    uniqueId(baseId: string): string {
      let candidate = baseId;
      let suffix = 2;
      while (candidate.startsWith("builtin-official-") || state.profileDrafts.some((profile) => profile.id === candidate)) {
        candidate = `${baseId}-${suffix++}`;
      }
      return candidate;
    },
    nextSortOrder: (app: string, mode: ProviderApplyMode) => all()
      .filter((profile) => canonicalProfileToolId(profile.app) === app && profile.mode === mode)
      .reduce((max, profile) => Math.max(max, profile.sortOrder), -1) + 1,
    normalizeIcon(value?: string | null): string | null {
      const trimmed = value?.trim() ?? "";
      if (!trimmed) return null;
      if (trimmed.startsWith("data:image/")) {
        if (trimmed.length > 512 * 1024) throw new Error("Profile icon image is too large.");
        return trimmed;
      }
      if ([...trimmed].length > 4) throw new Error("Profile icon text cannot be longer than 4 characters.");
      return trimmed;
    },
    normalizeRemark: (value?: string | null) => value?.trim() || null,
    normalizeReviewModel(app: string, value?: string | null): string | null {
      if (canonicalProfileToolId(app) !== "codex") return null;
      return value?.trim() || null;
    },
    normalizeModelMappings(app: string, mappings?: ProfileModelMapping[] | null): ProfileModelMapping[] {
      if (canonicalProfileToolId(app) !== "claude") return [];
      const normalized: ProfileModelMapping[] = [];
      const aliases = new Set<string>();
      for (const mapping of mappings ?? []) {
        const alias = mapping.alias.trim();
        const model = mapping.model.trim();
        const description = mapping.description?.trim() || null;
        if (!alias && !model && !description) continue;
        if (!alias || !model) throw new Error("Claude Code model mappings require both alias and target model.");
        const key = alias.toLowerCase();
        if (aliases.has(key)) throw new Error(`Claude Code model mapping alias '${alias}' is duplicated.`);
        aliases.add(key);
        normalized.push({ alias, model, supports1m: Boolean(mapping.supports1m), description });
      }
      return normalized;
    }
  };
}
