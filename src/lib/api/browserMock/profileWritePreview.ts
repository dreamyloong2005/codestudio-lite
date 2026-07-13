import type {
  DetectionSnapshot,
  PreviewProfileWriteRequest,
  PreviewProfileWriteResult,
  ProfileModelMapping
} from "../../../types";
import { canonicalProfileToolId } from "../../profiles/catalog";
import {
  browserCredentialDetail,
  browserProtocolLabel,
  ensureBrowserProfileProtocolSupported,
  ensureCustomOfficialProfileAllowed,
  normalizeBrowserProfileMode,
  normalizeBrowserProtocol,
  providerIsOfficial,
  providerRequiresApiKey,
  validateBrowserBaseUrlForProviderOrThrow
} from "./profilePolicy";
import { createBrowserProfileStore } from "./profileStore";
import type { BrowserMockState } from "./state";

export type BrowserWritePreviewDependencies = {
  detection(): DetectionSnapshot;
  toolConfigPath(toolId: string): string | null;
  gatewayWillAutoActivate(toolId: string, mode: "config" | "gateway"): boolean;
};

export function createBrowserProfileWritePreview(
  state: BrowserMockState,
  dependencies: BrowserWritePreviewDependencies
) {
  const store = createBrowserProfileStore(state);
  return async (request: PreviewProfileWriteRequest): Promise<PreviewProfileWriteResult> => {
    const app = canonicalProfileToolId(request.app);
    const baseId = slugify(request.name);
    const profileId = store.uniqueId(baseId);
    const profilePath = "~/.codestudio-lite/app_state.sqlite";
    const tool = dependencies.detection().tools.find((item) => item.id === app);
    const targetToolPath = tool?.configPath ?? dependencies.toolConfigPath(app);
    const warnings: string[] = [];
    if (!request.name.trim()) throw new Error("Profile Name is required");
    const mode = normalizeBrowserProfileMode(request.provider, request.mode);
    ensureCustomOfficialProfileAllowed(app, request.provider, mode);
    if (providerRequiresApiKey(request.provider) && !request.secretProvided) {
      throw new Error("Provider API key is required for non-official providers.");
    }
    validateBrowserBaseUrlForProviderOrThrow(request.provider, request.baseUrl);
    const protocol = normalizeBrowserProtocol(request.protocol);
    ensureBrowserProfileProtocolSupported(app, mode, request.provider, protocol);
    const icon = store.normalizeIcon(request.icon);
    const autoActivateGateway = dependencies.gatewayWillAutoActivate(app, mode);
    if (profileId !== baseId) warnings.push(`Profile id '${baseId}' already exists, so this draft will use '${profileId}'.`);
    if (!tool) warnings.push(`Tool '${app}' is not in the preview registry.`);
    const generatedAt = new Date().toISOString();
    const content = profileSqlPreview({
      id: profileId, name: request.name.trim(), icon, remark: store.normalizeRemark(request.remark), app, mode,
      provider: request.provider.trim(), protocol, model: request.model.trim(),
      reviewModel: store.normalizeReviewModel(app, request.reviewModel),
      modelMappings: store.normalizeModelMappings(app, request.modelMappings), baseUrl: request.baseUrl.trim(),
      authRef: providerIsOfficial(request.provider) ? null : request.secretProvided ? `keychain:codestudio-lite/${profileId}/api_key` : null,
      timestamp: generatedAt,
      secretStatus: providerIsOfficial(request.provider) ? "oauth" : request.secretProvided ? "pending_keychain" : "missing"
    });
    return {
      generatedAt, profileId, profilePath, targetToolPath, backupRequired: false, warnings,
      items: [
        { label: "Profile row", path: profilePath, action: "create", backupRequired: false, detail: `Save Profile Draft stores normalized metadata in SQLite for ${browserProtocolLabel(protocol)}/${request.provider} and excludes API keys.`, content },
        { label: "Active tool profile pointer", path: profilePath, action: autoActivateGateway ? "update" : "not_modified", backupRequired: false, detail: autoActivateGateway ? `Saving the first Gateway profile for '${app}' makes it the active Gateway profile.` : "Saving this draft preserves the current active profile.", content: null },
        { label: `${tool?.name ?? "Target tool"} config`, path: targetToolPath, action: "future_confirmation_required", backupRequired: Boolean(targetToolPath), detail: "Client config is not modified when saving a Provider Profile. Client Bootstrap remains a separate confirmation flow.", content: null },
        { label: "Credential", path: null, action: request.secretProvided ? "pending_keychain" : "missing", backupRequired: false, detail: browserCredentialDetail(request.provider, request.secretProvided), content: null }
      ]
    };
  };
}

function slugify(value: string): string {
  return value.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "profile";
}

function profileSqlPreview(input: {
  id: string; name: string; icon: string | null; remark: string | null; app: string; mode: string;
  provider: string; protocol: string; model: string; reviewModel: string | null;
  modelMappings: ProfileModelMapping[]; baseUrl: string; authRef: string | null; timestamp: string; secretStatus: string;
}): string {
  return JSON.stringify({
    table: "profiles",
    row: {
      id: input.id, name: input.name,
      icon: input.icon?.startsWith("data:image/") ? `image data url (${input.icon.length} bytes)` : input.icon,
      remark: input.remark, app: input.app, mode: input.mode, provider: input.provider, protocol: input.protocol,
      model: input.model, review_model: input.reviewModel, model_mappings: input.modelMappings,
      base_url: input.baseUrl, auth_ref: input.authRef, created_at: input.timestamp, updated_at: input.timestamp,
      last_test_status: "pending", secret_status: input.secretStatus
    },
    secrets: "API keys are stored in the system keychain and never written into SQLite."
  }, null, 2);
}
