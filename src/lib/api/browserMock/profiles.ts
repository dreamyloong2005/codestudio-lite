import type {
  DeleteProfileDraftRequest,
  DetectionSnapshot,
  DuplicateProfileDraftRequest,
  ListProfileModelsRequest,
  ListProfileModelsResult,
  ProfileDraft,
  ProfileSummary,
  ReorderProfileDraftsRequest,
  SaveProfileDraftRequest,
  TestProfileConnectionRequest,
  TestProfileConnectionResult,
  UpdateProfileDraftRequest
} from "../../../types";
import { canonicalProfileToolId } from "../../profiles/catalog";
import {
  ensureBrowserProfileProtocolSupported,
  ensureCustomOfficialProfileAllowed,
  normalizeBrowserProfileMode,
  normalizeBrowserProtocol,
  providerIsOfficial,
  providerRequiresApiKey,
  browserCredentialDetail,
  browserCredentialStatus,
  browserProfileModels,
  browserProtocolLabel,
  validateBrowserBaseUrlForProvider,
  validateBrowserBaseUrlForProviderOrThrow
} from "./profilePolicy";
import type { BrowserMockState } from "./state";
import { createBrowserProfileStore } from "./profileStore";

export type BrowserProfileDependencies = {
  summary(): ProfileSummary;
  detection(): DetectionSnapshot;
  recordActivity(level: "ok" | "info" | "warning" | "error", message: string): void;
};

export function createBrowserProfiles(state: BrowserMockState, dependencies: BrowserProfileDependencies) {
  const store = createBrowserProfileStore(state);

  const cleanActive = () => {
    const next = { config: {} as Record<string, string>, gateway: {} as Record<string, string> };
    for (const mode of ["config", "gateway"] as const) {
      for (const [app, profileId] of Object.entries(state.activeProfilesByMode[mode])) {
        const canonical = canonicalProfileToolId(app);
        const profile = store.all().find((item) =>
          item.id === profileId && canonicalProfileToolId(item.app) === canonical && item.mode === mode
        );
        if (profile) next[mode][canonical] = profile.id;
      }
    }
    state.activeProfilesByMode = next;
  };

  const gatewayWillAutoActivate = (app: string, mode: string) => {
    if (mode !== "gateway") return false;
    const canonical = canonicalProfileToolId(app);
    return !Object.entries(state.activeProfilesByMode.gateway).some(([activeApp, profileId]) =>
      canonicalProfileToolId(activeApp) === canonical && store.all().some((profile) =>
        profile.id === profileId && canonicalProfileToolId(profile.app) === canonical && profile.mode === "gateway"
      )
    );
  };

  const activateGatewayIfUnset = (profile: ProfileDraft) => {
    if (!gatewayWillAutoActivate(profile.app, profile.mode)) return;
    cleanActive();
    state.activeProfilesByMode = {
      ...state.activeProfilesByMode,
      gateway: { ...state.activeProfilesByMode.gateway, [canonicalProfileToolId(profile.app)]: profile.id }
    };
  };

  return {
    gatewayWillAutoActivate,
    async testConnection(request: TestProfileConnectionRequest): Promise<TestProfileConnectionResult> {
      const tool = dependencies.detection().tools.find((item) => item.id === request.app);
      const checks: TestProfileConnectionResult["checks"] = [];
      const protocol = normalizeBrowserProtocol(request.protocol);
      if (tool) {
        checks.push({
          id: "tool-install", label: "Target tool",
          status: tool.installState === "installed" ? "ok" : "warning",
          detail: tool.version ? `${tool.name} is installed: ${tool.version}` : `${tool.name} is missing${tool.installCommand ? `. Suggested command: ${tool.installCommand}` : "."}`
        });
        checks.push({
          id: "tool-config", label: "Existing tool config",
          status: tool.configState === "configured" ? "ok" : "info",
          detail: tool.configPath ? `${formatConfigState(tool.configState)} at ${tool.configPath}` : "No config path is known for this tool."
        });
      } else {
        checks.push({ id: "tool-install", label: "Target tool", status: "error", detail: `Tool '${request.app}' is not in the registry.` });
      }
      checks.push(validateBrowserBaseUrlForProvider(request.provider, request.baseUrl));
      checks.push({ id: "protocol", label: "Protocol", status: "ok", detail: `Selected upstream API protocol: ${browserProtocolLabel(protocol)}.` });
      checks.push({ id: "model", label: "Model", status: request.model.trim() ? "ok" : "info", detail: request.model.trim() || "Model is not specified." });
      checks.push({
        id: "credential", label: "Credential",
        status: browserCredentialStatus(request.provider, request.secretProvided),
        detail: request.apiKey?.trim()
          ? "Provider API key is ready to be stored in the system keychain when this profile is saved."
          : browserCredentialDetail(request.provider, request.secretProvided)
      });
      checks.push({ id: "network", label: "Provider ping", status: "info", detail: "Network provider checks are not sent yet." });
      const status = aggregateStatus(checks.map((check) => check.status));
      dependencies.recordActivity(status, `Ran profile connection checks for ${request.app}/${request.provider}.`);
      return { generatedAt: new Date().toISOString(), status, checks };
    },

    async listModels(request: ListProfileModelsRequest): Promise<ListProfileModelsResult> {
      const protocol = normalizeBrowserProtocol(request.protocol);
      validateBrowserBaseUrlForProviderOrThrow(request.provider, request.baseUrl);
      if (providerRequiresApiKey(request.provider) && !request.apiKey?.trim() && !request.profileId) {
        throw new Error("Provider API key is required to fetch models.");
      }
      return {
        generatedAt: new Date().toISOString(), provider: request.provider.trim(), protocol,
        baseUrl: request.baseUrl.trim(), models: browserProfileModels(protocol)
      };
    },
    async save(request: SaveProfileDraftRequest): Promise<ProfileDraft> {
      const app = canonicalProfileToolId(request.app);
      const mode = normalizeBrowserProfileMode(request.provider, request.mode);
      ensureCustomOfficialProfileAllowed(app, request.provider, mode);
      if (providerRequiresApiKey(request.provider) && !request.secretProvided) {
        throw new Error("Provider API key is required for non-official providers.");
      }
      validateBrowserBaseUrlForProviderOrThrow(request.provider, request.baseUrl);
      const protocol = normalizeBrowserProtocol(request.protocol);
      ensureBrowserProfileProtocolSupported(app, mode, request.provider, protocol);
      const profileId = uniqueId(state, slugify(request.name), store.isBuiltinId);
      const now = new Date().toISOString();
      const profile: ProfileDraft = {
        id: profileId,
        name: request.name.trim(),
        icon: store.normalizeIcon(request.icon),
        remark: store.normalizeRemark(request.remark),
        app,
        isBuiltin: false,
        mode,
        provider: request.provider,
        protocol,
        model: request.model.trim(),
        reviewModel: store.normalizeReviewModel(app, request.reviewModel),
        modelMappings: store.normalizeModelMappings(app, request.modelMappings),
        baseUrl: request.baseUrl.trim(),
        authRef: providerIsOfficial(request.provider) ? null : `keychain:codestudio-lite/${profileId}/api_key`,
        createdAt: now,
        updatedAt: now,
        lastTestStatus: "pending",
        usageEnabled: false,
        sortOrder: store.nextSortOrder(app, mode)
      };
      state.profileDrafts = [...state.profileDrafts, profile];
      activateGatewayIfUnset(profile);
      dependencies.recordActivity("ok", `Saved profile draft '${profile.name}' for ${profile.app}/${profile.provider}.`);
      return profile;
    },

    async update(request: UpdateProfileDraftRequest): Promise<ProfileDraft> {
      if (store.isBuiltinId(request.profileId)) throw new Error("Built-in official profiles cannot be modified.");
      const index = state.profileDrafts.findIndex((draft) => draft.id === request.profileId);
      if (index === -1) throw new Error(`Profile '${request.profileId}' does not exist`);
      if (!request.name.trim()) throw new Error("Profile Name is required");
      const existing = state.profileDrafts[index];
      const mode = normalizeBrowserProfileMode(request.provider, request.mode ?? existing.mode);
      const protocol = normalizeBrowserProtocol(request.protocol ?? existing.protocol);
      const app = canonicalProfileToolId(existing.app);
      ensureCustomOfficialProfileAllowed(app, request.provider, mode);
      ensureBrowserProfileProtocolSupported(app, mode, request.provider, protocol);
      validateBrowserBaseUrlForProviderOrThrow(request.provider, request.baseUrl);
      if (providerRequiresApiKey(request.provider) && !existing.authRef && !request.apiKey?.trim()) {
        throw new Error("Provider API key is required for non-official providers.");
      }
      const updated: ProfileDraft = {
        ...existing,
        name: request.name.trim(),
        icon: store.normalizeIcon(request.icon),
        remark: store.normalizeRemark(request.remark),
        app,
        mode,
        provider: request.provider.trim(),
        protocol,
        model: request.model.trim(),
        reviewModel: store.normalizeReviewModel(app, request.reviewModel),
        modelMappings: store.normalizeModelMappings(app, request.modelMappings ?? existing.modelMappings),
        baseUrl: request.baseUrl.trim(),
        authRef: providerIsOfficial(request.provider) ? null : request.apiKey?.trim() ? existing.authRef ?? `keychain:codestudio-lite/${existing.id}/api_key` : existing.authRef,
        updatedAt: new Date().toISOString(),
        lastTestStatus: "pending",
        usageEnabled: state.usageScripts.get(existing.id)?.enabled ?? existing.usageEnabled
      };
      state.profileDrafts = [...state.profileDrafts.slice(0, index), updated, ...state.profileDrafts.slice(index + 1)];
      cleanActive();
      dependencies.recordActivity("ok", `Updated profile draft '${updated.name}' for ${updated.app}/${updated.provider}.`);
      return updated;
    },

    async duplicate(request: DuplicateProfileDraftRequest): Promise<ProfileDraft> {
      const existing = store.all().find((draft) => draft.id === request.profileId);
      if (!existing) throw new Error(`Profile '${request.profileId}' does not exist`);
      if (existing.isBuiltin) throw new Error("Built-in official profiles cannot be duplicated.");
      const id = uniqueId(state, slugify(existing.name), store.isBuiltinId);
      const now = new Date().toISOString();
      const duplicated = {
        ...existing, id, isBuiltin: false,
        authRef: existing.authRef ? `keychain:codestudio-lite/${id}/api_key` : null,
        createdAt: now, updatedAt: now,
        sortOrder: store.nextSortOrder(canonicalProfileToolId(existing.app), existing.mode)
      };
      state.profileDrafts = [...state.profileDrafts, duplicated];
      dependencies.recordActivity("ok", `Duplicated profile draft '${existing.name}' as '${duplicated.name}'.`);
      return duplicated;
    },

    async delete(request: DeleteProfileDraftRequest): Promise<ProfileSummary> {
      const existing = store.all().find((draft) => draft.id === request.profileId);
      if (!existing) throw new Error(`Profile '${request.profileId}' does not exist`);
      if (existing.isBuiltin || store.isBuiltinId(existing.id)) throw new Error("Built-in official profiles cannot be deleted.");
      state.profileDrafts = state.profileDrafts.filter((draft) => draft.id !== request.profileId);
      for (const [app, id] of Object.entries(state.activeProfilesByMode.config)) {
        if (id === request.profileId) {
          const canonical = canonicalProfileToolId(app);
          delete state.activeProfilesByMode.config[app];
          state.activeProfilesByMode.config[canonical] = store.builtinId(canonical);
        }
      }
      for (const [app, id] of Object.entries(state.activeProfilesByMode.gateway)) {
        if (id === request.profileId) delete state.activeProfilesByMode.gateway[app];
      }
      cleanActive();
      dependencies.recordActivity("ok", `Deleted profile draft '${existing.name}' for ${existing.app}/${existing.provider}.`);
      return dependencies.summary();
    },

    async reorder(request: ReorderProfileDraftsRequest): Promise<ProfileSummary> {
      const app = canonicalProfileToolId(request.app);
      const profiles = store.all().filter((profile) => canonicalProfileToolId(profile.app) === app && profile.mode === request.mode);
      const expected = new Set(profiles.map((profile) => profile.id));
      const requested = new Set(request.profileIds);
      if (expected.size !== requested.size || [...expected].some((id) => !requested.has(id))) {
        throw new Error("Profile order must include every profile in this tool category.");
      }
      state.profileOrder[store.orderKey(app, request.mode)] = [...request.profileIds];
      const order = new Map(request.profileIds.map((id, index) => [id, index]));
      state.profileDrafts = state.profileDrafts.map((profile) =>
        order.has(profile.id) ? { ...profile, sortOrder: order.get(profile.id)! } : profile
      );
      return dependencies.summary();
    }
  };
}

function formatConfigState(state: string): string {
  if (state === "configured") return "Configured";
  if (state === "unconfigured") return "Not configured";
  if (state === "missing") return "Config file missing";
  return "Config state unknown";
}

function aggregateStatus(
  statuses: Array<TestProfileConnectionResult["status"]>
): TestProfileConnectionResult["status"] {
  if (statuses.includes("error")) return "error";
  if (statuses.includes("warning")) return "warning";
  if (statuses.includes("ok")) return "ok";
  return "info";
}

function slugify(value: string): string {
  return value.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "profile";
}

function uniqueId(state: BrowserMockState, base: string, isBuiltin: (id: string) => boolean): string {
  let candidate = base;
  let suffix = 2;
  while (isBuiltin(candidate) || state.profileDrafts.some((profile) => profile.id === candidate)) {
    candidate = `${base}-${suffix++}`;
  }
  return candidate;
}
