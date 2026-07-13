import type {
  ProfileDraft,
  UsageQueryResult,
  UsageScriptConfig,
  UsageScriptSaveRequest,
  UsageScriptState,
  UsageScriptTemplateType
} from "../../../types";
import { canonicalProfileToolId } from "../../profiles/catalog";
import { providerIsOfficial } from "./profilePolicy";
import type { BrowserMockState } from "./state";

export type BrowserUsageDependencies = {
  allProfiles(): ProfileDraft[];
  defaultScript(template: UsageScriptTemplateType): string;
};

export function createBrowserUsage(
  state: BrowserMockState,
  dependencies: BrowserUsageDependencies
) {
  const profile = (profileId: string) => dependencies.allProfiles().find((item) => item.id === profileId);
  const isCodexOfficial = (item: ProfileDraft) =>
    canonicalProfileToolId(item.app) === "codex" && providerIsOfficial(item.provider);

  const usageState = (profileId: string): UsageScriptState => {
    const config = state.usageScripts.get(profileId) ?? null;
    return {
      profileId,
      config,
      lastResult: state.usageResults.get(profileId) ?? null,
      defaultCode: dependencies.defaultScript(config?.templateType ?? "general")
    };
  };

  const result = (profileId: string, source: string): UsageQueryResult => {
    const item = profile(profileId);
    if (!item) throw new Error(`Profile '${profileId}' does not exist`);
    if (isCodexOfficial(item)) {
      return {
        success: true,
        data: [
          { isValid: true, planName: "Codex 5h limit (pro)", remaining: 58, used: 42, total: 100, unit: "%", extra: "Window: 5h / Reset: 1h" },
          { isValid: true, planName: "Codex weekly limit (pro)", remaining: 93, used: 7, total: 100, unit: "%", extra: "Window: 7d" },
          { isValid: true, planName: "Lifetime tokens (pro)", remaining: null, used: 123456, total: null, unit: "tokens", extra: "Mock official OAuth usage" }
        ],
        error: null,
        queriedAt: new Date().toISOString(),
        source: "codex_official_oauth"
      };
    }
    return {
      success: true,
      data: [{
        isValid: true,
        planName: item.provider === "newapi" ? "Default" : "API Balance",
        remaining: 18.42,
        used: 6.58,
        total: 25,
        unit: "USD",
        extra: "Mock query result"
      }],
      error: null,
      queriedAt: new Date().toISOString(),
      source
    };
  };

  const configFromRequest = (
    request: UsageScriptSaveRequest,
    existing?: UsageScriptConfig
  ): UsageScriptConfig => {
    const item = profile(request.profileId);
    const official = item ? isCodexOfficial(item) : false;
    return {
      profileId: request.profileId,
      enabled: request.enabled,
      templateType: official ? "general" : request.templateType,
      code: official ? "" : request.code.trim() || dependencies.defaultScript(request.templateType),
      apiKey: official ? null : request.apiKey?.trim() ? `keychain:codestudio-lite/${request.profileId}/usage_api_key` : null,
      baseUrl: official ? null : request.baseUrl?.trim() || null,
      accessToken: official ? null : request.accessToken?.trim() ? `keychain:codestudio-lite/${request.profileId}/usage_access_token` : null,
      userId: official ? null : request.userId?.trim() || null,
      timeoutSeconds: request.timeoutSeconds ?? existing?.timeoutSeconds ?? 10,
      autoQueryIntervalMinutes: request.autoQueryIntervalMinutes ?? existing?.autoQueryIntervalMinutes ?? 0,
      updatedAt: new Date().toISOString()
    };
  };

  return {
    load: async (profileId: string) => usageState(profileId),
    async save(request: UsageScriptSaveRequest) {
      if (!profile(request.profileId)) throw new Error(`Profile '${request.profileId}' does not exist`);
      state.usageScripts.set(request.profileId, configFromRequest(request, state.usageScripts.get(request.profileId)));
      return usageState(request.profileId);
    },
    async test(request: UsageScriptSaveRequest) {
      const item = profile(request.profileId);
      if (item && isCodexOfficial(item)) {
        throw new Error("Codex official OAuth usage can be queried directly; no custom script test is needed.");
      }
      return result(request.profileId, "test");
    },
    async query(profileId: string) {
      const config = state.usageScripts.get(profileId);
      if (!config) throw new Error("Usage query is not configured for this profile.");
      if (!config.enabled) throw new Error("Usage query is disabled for this profile.");
      const next = result(profileId, "query");
      state.usageResults.set(profileId, next);
      return next;
    },
    async delete(profileId: string) {
      state.usageScripts.delete(profileId);
      state.usageResults.delete(profileId);
      return usageState(profileId);
    }
  };
}
