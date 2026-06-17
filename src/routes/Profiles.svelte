<script lang="ts">
  import {
    applyProfile,
    clearClaudeEnvironmentVariables,
    duplicateProfileDraft,
    exportProfiles,
    importProfiles,
    loadAppSettings,
    previewProfileApply,
    updateProfileDraft
  } from "../lib/api";
  import { t, type TranslationKey } from "../lib/i18n";
  import AppIcon from "../components/AppIcon.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import type {
    ApplyProfileResult,
    DetectionSnapshot,
    PreviewProfileApplyResult,
    ProfileDraft,
    ProfileSummary,
    ProviderApplyMode
  } from "../types";

  export let summary: ProfileSummary | null = null;
  export let snapshot: DetectionSnapshot | null = null;
  export let onProfileSwitched: () => void | Promise<void> = () => {};

  type ProfileGroup = {
    id: string;
    label: string;
    activeProfileId: string | null;
    activeProfileName: string | null;
    profiles: ProfileDraft[];
  };

  type ProfileModeSection = {
    mode: ProviderApplyMode;
    titleKey: TranslationKey;
    descriptionKey: TranslationKey;
    groups: ProfileGroup[];
  };

  type EditProfileForm = {
    name: string;
    mode: ProviderApplyMode;
    provider: string;
    protocol: string;
    model: string;
    baseUrl: string;
    apiKey: string;
    timeoutSeconds: number;
  };

  let pendingEdit: ProfileDraft | null = null;
  let editForm: EditProfileForm = emptyEditForm();
  let editingId: string | null = null;
  let applyingId: string | null = null;
  let duplicatingId: string | null = null;
  let pendingApply: ProfileDraft | null = null;
  let applyPreview: PreviewProfileApplyResult | null = null;
  let applyResult: ApplyProfileResult | null = null;
  let selectedApplyMode: ProviderApplyMode = "gateway";
  let pendingApplyMode: ProviderApplyMode = "gateway";
  let importFileInput: HTMLInputElement | null = null;
  let profileIoBusy: "import" | "export" | null = null;
  let editError: string | null = null;
  let applyError: string | null = null;
  let clearingEnvConflict = false;
  let profileIoError: string | null = null;
  let profileIoMessage: string | null = null;
  let syncClaudeVsCodePlugin = false;
  let preserveCodexOfficialAuth = true;
  let codexAuthConflictConfirmed = false;

  const toolOrder = ["codex", "claude-desktop", "claude", "gemini", "gemini-code-assist", "opencode", "openclaw", "hermes"];
  const toolLabels: Record<string, string> = {
    codex: "Codex",
    "claude-desktop": "Claude Desktop",
    claude: "Claude Code",
    gemini: "Gemini CLI",
    "gemini-code-assist": "Gemini Code Assist",
    opencode: "OpenCode",
    openclaw: "OpenClaw",
    hermes: "Hermes"
  };

  const protocolOptions = [
    { id: "openai-chat-completions", labelKey: "wizard.protocol.openaiChatCompletions" },
    { id: "openai-responses", labelKey: "wizard.protocol.openaiResponses" },
    { id: "anthropic-messages", labelKey: "wizard.protocol.anthropicMessages" },
    { id: "google-gemini", labelKey: "wizard.protocol.googleGemini" }
  ] as const;
  type ProtocolOption = (typeof protocolOptions)[number];

  const configModeProtocolIdsByTool: Record<string, readonly string[]> = {
    codex: ["openai-chat-completions", "openai-responses"],
    "claude-desktop": [],
    claude: ["anthropic-messages"],
    gemini: ["google-gemini"],
    "gemini-code-assist": ["google-gemini"],
    opencode: ["openai-chat-completions", "openai-responses"],
    openclaw: ["openai-chat-completions"],
    hermes: ["openai-chat-completions"]
  };

  $: installedProfileToolIds = buildInstalledProfileToolIds(snapshot);
  $: profileModeSections = buildProfileModeSections(summary, installedProfileToolIds);
  $: customProfileCount = summary?.drafts.filter((profile) => !profile.isBuiltin).length ?? 0;
  $: visibleProfileCount = profileModeSections.reduce((count, section) => count + section.groups.reduce((groupCount, group) => groupCount + group.profiles.length, 0), 0);
  $: selectedModePreview =
    applyPreview?.modePreviews.find((mode) => mode.mode === selectedApplyMode) ?? null;
  $: selectedNativeDiff = selectedModePreview?.nativeDiff ?? null;
  $: selectedModeSupported = selectedModePreview?.supported ?? false;
  $: applyEnvConflicts = applyResult?.envConflicts ?? applyPreview?.envConflicts ?? [];
  $: pendingApplyDisplacesCodexOAuth = Boolean(
    pendingApply &&
    canonicalProfileToolId(pendingApply.app) === "codex" &&
    pendingApply.mode === "config" &&
    !providerIsOfficial(pendingApply.provider) &&
    !preserveCodexOfficialAuth &&
    (summary?.codexAuth.available || activeCodexConfigProfileIsOfficial(summary))
  );
  $: canSyncClaudeVsCodePlugin =
    Boolean(pendingApply) &&
    canonicalProfileToolId(pendingApply?.app ?? "") === "claude" &&
    selectedApplyMode === "config" &&
    Boolean(selectedModePreview?.writesNativeConfig);
  $: if (!canSyncClaudeVsCodePlugin && syncClaudeVsCodePlugin) {
    syncClaudeVsCodePlugin = false;
  }
  $: editTimeoutSeconds = Number(editForm.timeoutSeconds);
  $: editBaseUrlErrorKey = providerNeedsBaseUrl(editForm.provider)
    ? baseUrlValidationErrorKey(editForm.baseUrl)
    : null;
  $: availableEditProtocolOptions = pendingEdit
    ? protocolOptionsFor(pendingEdit.app, editForm.mode)
    : protocolOptions;
  $: canSaveEdit =
    Boolean(pendingEdit) &&
    editForm.name.trim().length > 0 &&
    editForm.provider.trim().length > 0 &&
    !providerIsOfficial(editForm.provider) &&
    isProtocolAllowedForToolMode(pendingEdit?.app ?? "", editForm.mode, editForm.protocol) &&
    (!providerNeedsBaseUrl(editForm.provider) || editBaseUrlErrorKey === null) &&
    (!providerRequiresApiKey(editForm.provider) || Boolean(pendingEdit?.authRef) || editForm.apiKey.trim().length > 0) &&
    !pendingEdit?.isBuiltin &&
    editTimeoutSeconds >= 5 &&
    editTimeoutSeconds <= 600 &&
    editingId === null;

  async function openApply(profile: ProfileDraft) {
    if (isProfileActive(profile)) {
      return;
    }
    pendingApply = profile;
    pendingApplyMode = profile.mode;
    applyPreview = null;
    applyResult = null;
    applyError = null;
    selectedApplyMode = profile.mode;
    syncClaudeVsCodePlugin = false;
    codexAuthConflictConfirmed = false;
    applyingId = actionKey(profile.id, profile.mode);

    try {
      applyPreview = await previewProfileApply({ profileId: profile.id });
      selectedApplyMode = profile.mode;
      preserveCodexOfficialAuth = await loadCodexAuthPreservationSetting();
    } catch (err) {
      applyError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      applyingId = null;
    }
  }

  function emptyEditForm(): EditProfileForm {
    return {
      name: "",
      mode: "gateway",
      provider: "",
      protocol: "openai-chat-completions",
      model: "",
      baseUrl: "",
      apiKey: "",
      timeoutSeconds: 120
    };
  }

  function openEdit(profile: ProfileDraft) {
    if (profile.isBuiltin) {
      return;
    }
    pendingEdit = profile;
    editError = null;
    const nextForm = {
      name: profile.name,
      mode: profile.mode,
      provider: profile.provider,
      protocol: profile.protocol,
      model: profile.model,
      baseUrl: profile.baseUrl,
      apiKey: "",
      timeoutSeconds: profile.timeoutSeconds
    };
    editForm = {
      ...nextForm,
      protocol: coerceProtocolForToolMode(profile.app, nextForm.mode, nextForm.protocol)
    };
  }

  function closeEdit() {
    if (editingId !== null) {
      return;
    }
    pendingEdit = null;
    editError = null;
    editForm = emptyEditForm();
  }

  function selectEditMode(mode: ProviderApplyMode) {
    if (editingId !== null) {
      return;
    }

    editError = null;
    const nextForm = {
      ...editForm,
      mode
    };
    editForm = {
      ...nextForm,
      protocol: pendingEdit
        ? coerceProtocolForToolMode(pendingEdit.app, mode, nextForm.protocol)
        : nextForm.protocol
    };
  }

  async function handleEditSave() {
    if (!pendingEdit || !canSaveEdit) {
      editError = editBaseUrlErrorKey ? $t(editBaseUrlErrorKey) : $t("profiles.editRequired");
      return;
    }

    editingId = pendingEdit.id;
    editError = null;

    try {
      await updateProfileDraft({
        profileId: pendingEdit.id,
        name: editForm.name,
        mode: editForm.mode,
        provider: editForm.provider,
        protocol: editForm.protocol,
        model: editForm.model,
        baseUrl: editForm.baseUrl,
        apiKey: editForm.apiKey.trim().length > 0 ? editForm.apiKey : null,
        timeoutSeconds: editTimeoutSeconds
      });
      await onProfileSwitched();
      pendingEdit = null;
      editForm = emptyEditForm();
    } catch (err) {
      editError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      editingId = null;
    }
  }

  async function handleApplyWithOptions(profileId: string, restartAfterApply = false) {
    if (pendingApply && isProfileActive(pendingApply)) {
      applyError = $t("profiles.alreadyActiveBlocked");
      return;
    }

    if (pendingApplyDisplacesCodexOAuth && !codexAuthConflictConfirmed) {
      codexAuthConflictConfirmed = true;
      return;
    }

    const syncClaudeVsCode = canSyncClaudeVsCodePlugin && syncClaudeVsCodePlugin;
    applyingId = actionKey(profileId, selectedApplyMode, restartAfterApply, syncClaudeVsCode);
    applyError = null;
    applyResult = null;

    try {
      applyResult = await applyProfile({ profileId, restartAfterApply, syncClaudeVsCode });
      await onProfileSwitched();
    } catch (err) {
      applyError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      applyingId = null;
    }
  }

  async function clearApplyEnvConflicts() {
    if (!pendingApply || applyEnvConflicts.length === 0 || clearingEnvConflict) {
      return;
    }
    clearingEnvConflict = true;
    applyError = null;
    try {
      await clearClaudeEnvironmentVariables({
        toolId: "claude",
        variables: applyEnvConflicts.map((conflict) => conflict.variable),
        confirm: true
      });
      applyPreview = await previewProfileApply({ profileId: pendingApply.id });
      applyResult = null;
      await onProfileSwitched();
    } catch (err) {
      applyError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      clearingEnvConflict = false;
    }
  }

  async function handleDuplicate(profile: ProfileDraft) {
    if (profile.isBuiltin || duplicatingId !== null || applyingId !== null || editingId !== null) {
      return;
    }

    duplicatingId = profile.id;
    profileIoError = null;
    profileIoMessage = null;

    try {
      const duplicated = await duplicateProfileDraft({ profileId: profile.id });
      await onProfileSwitched();
      profileIoMessage = $t("profiles.duplicateSuccess", { name: duplicated.name });
    } catch (err) {
      profileIoError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      duplicatingId = null;
    }
  }

  function openImportDialog() {
    if (profileIoBusy !== null) {
      return;
    }
    profileIoError = null;
    profileIoMessage = null;
    importFileInput?.click();
  }

  async function handleImportFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) {
      return;
    }

    profileIoBusy = "import";
    profileIoError = null;
    profileIoMessage = null;

    try {
      const content = await file.text();
      const result = await importProfiles({ content });
      await onProfileSwitched();
      profileIoMessage = `${$t("profiles.importSuccess", { count: result.imported.length })}${
        result.skipped.length > 0 ? ` ${$t("profiles.importSkipped", { count: result.skipped.length })}` : ""
      }`;
      if (result.imported.length === 0 && result.skipped.length > 0) {
        profileIoError = result.skipped.join("\n");
      }
    } catch (err) {
      profileIoError = err instanceof Error ? err.message : String(err);
    } finally {
      profileIoBusy = null;
      input.value = "";
    }
  }

  async function handleExportProfiles() {
    if (!summary || customProfileCount === 0 || profileIoBusy !== null) {
      profileIoError = $t("profiles.exportEmpty");
      return;
    }

    profileIoBusy = "export";
    profileIoError = null;
    profileIoMessage = null;

    try {
      const result = await exportProfiles();
      downloadJson(result.fileName, result.bundle);
      profileIoMessage = $t("profiles.exportSuccess", { file: result.fileName });
    } catch (err) {
      profileIoError = err instanceof Error ? err.message : String(err);
    } finally {
      profileIoBusy = null;
    }
  }

  function downloadJson(fileName: string, value: unknown) {
    const blob = new Blob([JSON.stringify(value, null, 2)], { type: "application/json;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = fileName;
    document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function closeApply() {
    if (applyingId !== null) {
      return;
    }
    pendingApply = null;
    applyPreview = null;
    applyResult = null;
    applyError = null;
    selectedApplyMode = "gateway";
    pendingApplyMode = "gateway";
    syncClaudeVsCodePlugin = false;
    codexAuthConflictConfirmed = false;
  }

  function actionKey(profileId: string, mode: ProviderApplyMode, restartAfterApply = false, syncClaudeVsCode = false) {
    return `${mode}:${profileId}:${restartAfterApply ? "restart" : "apply"}:${syncClaudeVsCode ? "sync-claude-vscode" : "base"}`;
  }

  function applyModeLabel(mode: ProviderApplyMode) {
    return mode === "config" ? $t("profiles.mode.config") : $t("profiles.mode.gateway");
  }

  function providerRequiresApiKey(providerId: string) {
    return !providerIsOfficial(providerId);
  }

  function providerNeedsBaseUrl(providerId: string) {
    return !providerIsOfficial(providerId);
  }

  function providerIsOfficial(providerId: string) {
    return providerId.trim() === "official";
  }

  function profileProviderLabel(profile: ProfileDraft) {
    return providerIsOfficial(profile.provider) ? $t("profiles.builtinOfficial") : profile.provider;
  }

  function profileCredentialLabel(profile: ProfileDraft) {
    if (providerIsOfficial(profile.provider)) {
      if (canonicalProfileToolId(profile.app) === "codex" && summary?.codexAuth) {
        return summary.codexAuth.available
          ? $t("profiles.credentialCodexOAuthDetected")
          : $t("profiles.credentialCodexOAuthMissing");
      }
      return $t("profiles.credentialOfficial");
    }
    return profile.authRef ? $t("profiles.credentialLinked") : $t("profiles.credentialMissing");
  }

  function codexOfficialAuthDetail(profile: ProfileDraft) {
    if (!providerIsOfficial(profile.provider) || canonicalProfileToolId(profile.app) !== "codex") {
      return null;
    }
    const status = summary?.codexAuth;
    if (!status) {
      return $t("profiles.codexOAuthUnknown");
    }
    if (status.storage === "keyring" || status.storage === "auto") {
      return $t("profiles.codexOAuthKeyring");
    }
    if (status.available) {
      return status.path
        ? $t("profiles.codexOAuthDetectedAt", { path: status.path })
        : $t("profiles.codexOAuthDetected");
    }
    return $t("profiles.codexOAuthMissing");
  }

  function profileEndpointLabel(profile: ProfileDraft) {
    if (providerIsOfficial(profile.provider) && !profile.baseUrl.trim()) {
      return $t("profiles.officialProfileEndpoint");
    }
    return profile.baseUrl;
  }

  function protocolLabel(value: string) {
    const normalized = normalizeProtocol(value);
    const option = protocolOptions.find((item) => item.id === normalized);
    return option ? $t(option.labelKey) : value;
  }

  function normalizeProtocol(value: string) {
    return value.trim();
  }

  function applyActionLabel(action: string) {
    if (action === "update") {
      return $t("common.update");
    }
    if (action === "create") {
      return $t("common.create");
    }
    if (action === "create_or_update") {
      return $t("common.save");
    }
    if (action === "not_modified" || action === "not_written") {
      return $t("common.noWrite");
    }
    return action.replaceAll("_", " ");
  }

  function applyPreviewLabel(item: PreviewProfileApplyResult["items"][number]) {
    if (item.label === "Active tool profile pointer") {
      return $t("profiles.preview.activeProfilePointer");
    }
    if (item.label === "Managed tool binding") {
      return $t("profiles.preview.managedBinding");
    }
    if (item.label === "Credential") {
      return $t("profiles.preview.credential");
    }
    const nativeConfigMatch = item.label.match(/^(.*) native config$/);
    if (nativeConfigMatch) {
      return $t("profiles.preview.nativeConfig", { name: nativeConfigMatch[1] });
    }
    return item.label;
  }

  function applyPreviewDetail(item: PreviewProfileApplyResult["items"][number]) {
    if (item.label === "Active tool profile pointer") {
      return $t("profiles.preview.activeProfilePointerDetail", {
        app: applyPreview?.app ?? pendingApply?.app ?? "",
        id: applyPreview?.profileId ?? pendingApply?.id ?? ""
      });
    }
    if (item.label === "Managed tool binding") {
      return $t("profiles.preview.managedBindingDetail", {
        app: applyPreview?.app ?? pendingApply?.app ?? "",
        provider: applyPreview?.provider ?? pendingApply?.provider ?? ""
      });
    }
    if (item.label === "Credential") {
      return $t("profiles.preview.credentialDetail");
    }
    if (item.label.endsWith(" native config")) {
      return previewTextLabel(item.detail);
    }
    return previewTextLabel(item.detail);
  }

  function nativeDiffActionLabel(action: string) {
    if (action === "add") {
      return $t("common.add");
    }
    if (action === "update") {
      return $t("common.update");
    }
    if (action === "remove") {
      return $t("common.remove");
    }
    if (action === "unchanged") {
      return $t("common.unchanged");
    }
    return applyActionLabel(action);
  }

  function previewTextLabel(message: string) {
    const exact: Partial<Record<string, TranslationKey>> = {
      "Config file mode needs a stored Provider API key for this Provider.": "profiles.warning.configNeedsStoredKey",
      "Selected mode writes this client config; detailed file changes are shown below.": "profiles.preview.nativeWriteDetail",
      "This profile does not require a native client config write.": "profiles.preview.nativeReservedDetail",
      "Official provider uses the client login directly and does not run through the local gateway.": "profiles.warning.officialGatewayUnsupported",
      "Official provider uses the target client's own login.": "profiles.warning.officialClientLogin",
      "No Provider API key or model override is required.": "profiles.warning.noProviderKeyOrModel",
      "Changing Codex config usually requires restarting Codex or opening a new Codex session.": "profiles.warning.codexReloadRequired",
      "Direct config file mode writes Provider connection details into the client config.": "profiles.warning.directConfigWrites",
      "Frequent Provider switching may require the client to reload its own config.": "profiles.warning.frequentSwitchReload",
      "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.": "profiles.warning.upstreamKeysInKeychain",
      "The client still needs to reload config after the first gateway bootstrap.": "profiles.warning.reloadAfterFirstGateway",
      "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.": "profiles.warning.gatewayManualStart",
      "Gateway mode writes Claude Code settings to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesClaude",
      "Gateway mode writes Gemini CLI environment values to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesGemini",
      "Gateway mode writes OpenCode's provider entry to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesOpenCode",
      "Gateway mode writes OpenClaw's provider entry to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesOpenClaw",
      "Gateway mode writes Hermes custom provider settings to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesHermes",
      "Config file mode writes Codex's provider entry directly to the selected upstream Provider.": "profiles.warning.configWritesCodexProvider",
      "The preview masks the Provider API key. The actual key is loaded from the system keychain during apply.": "profiles.warning.previewMasksProviderKey",
      "Gateway mode is a one-time relay injection target, not a direct Provider switch.": "profiles.warning.gatewayRelayTarget",
      "Switching profiles later changes only the Gateway active profile for this tool.": "profiles.warning.gatewaySwitchOnly",
      "The preview masks the local CodeStudio token. Real Provider API keys are never written to Codex config.": "profiles.warning.gatewayMasksLocalToken",
      "Codex official login is still required for the desktop app; the Local Gateway only takes over model requests.": "profiles.warning.codexLoginStillRequired",
      "If Codex is already running, restart Codex or open a new Codex session after bootstrap so it reloads config.toml.": "profiles.warning.reloadAfterGateway",
      "Codex config does not exist yet; adapter would create it after confirmation.": "profiles.warning.codexConfigMissing",
      "Hermes config does not exist yet; adapter would create it after confirmation.": "profiles.warning.hermesConfigMissing",
      "Config file mode writes Claude Code user settings under the env section.": "profiles.warning.claudeSettingsEnv",
      "The selected endpoint must be Anthropic/Claude-compatible; generic OpenAI-only endpoints need a translator.": "profiles.warning.claudeAnthropicEndpoint",
      "Restart Claude Code or open a new session after applying so settings reload.": "profiles.warning.claudeReload",
      "Gemini Code Assist stores its API key in VS Code user settings.": "profiles.warning.geminiCodeAssistApiKeySetting",
      "The public Gemini Code Assist VS Code setting exposes the API key; Provider Base URL and model are kept in CodeStudio Lite but are not written to the extension config.": "profiles.warning.geminiCodeAssistNoBaseUrl",
      "Restart VS Code or reload the Gemini Code Assist extension after applying so settings reload.": "profiles.warning.geminiCodeAssistReload",
      "Gemini CLI reads API key and base URL from environment variables, so this adapter writes ~/.gemini/.env.": "profiles.warning.geminiCliEnv",
      "Restart Gemini CLI or open a new terminal session after applying so environment variables reload.": "profiles.warning.geminiCliReload",
      "OpenCode custom providers are written to opencode.json using the OpenAI-compatible provider package.": "profiles.warning.opencodeConfigWrites",
      "OpenClaw providers are written in models.mode=merge so existing provider definitions can stay available.": "profiles.warning.openclawConfigWrites",
      "Hermes custom providers are written to ~/.hermes/config.yaml under the model section.": "profiles.warning.hermesConfigWrites",
      "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file.": "profiles.warning.jsoncCommentsLost",
      "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file.": "profiles.warning.json5CommentsLost",
      "Existing YAML comments are not preserved when CodeStudio Lite writes the file.": "profiles.warning.yamlCommentsLost",
      "Hermes config file mode currently targets OpenAI Chat Completions endpoints.": "profiles.warning.hermesChatOnly",
      "Selects Codex's official OpenAI provider.": "profiles.diff.selectOfficialProvider",
      "Keeps a readable label for the official provider.": "profiles.diff.officialProviderLabel",
      "Uses Codex's supported official provider wire API.": "profiles.diff.officialWireApi",
      "Keeps Codex official login as the authentication source.": "profiles.diff.officialLoginAuth",
      "Official login does not require a Provider API key.": "profiles.diff.officialNoApiKey",
      "Official provider can use Codex's own model default.": "profiles.diff.officialModelDefault",
      "Sets Codex to the selected official model.": "profiles.diff.officialModel",
      "Selects the direct provider entry managed by CodeStudio Lite.": "profiles.diff.directProviderEntry",
      "Sets Codex to the selected upstream model.": "profiles.diff.upstreamModel",
      "Adds a readable provider label for this upstream Provider.": "profiles.diff.upstreamProviderLabel",
      "Uses Codex's supported provider wire API for custom providers.": "profiles.diff.customWireApi",
      "Points Codex directly at the upstream Provider Base URL.": "profiles.diff.upstreamBaseUrl",
      "Disables Codex official OpenAI auth for this custom upstream entry.": "profiles.diff.disableOfficialAuth",
      "Stores the selected Provider API key from the system keychain.": "profiles.diff.storeProviderKey",
      "Selects the CodeStudio Lite localhost provider.": "profiles.diff.localhostProvider",
      "Sets Codex to the virtual model name resolved by the Local Gateway.": "profiles.diff.gatewayVirtualModel",
      "Adds a readable provider label for the Local Gateway.": "profiles.diff.gatewayProviderLabel",
      "Points Codex at the CodeStudio Lite Local Gateway.": "profiles.diff.gatewayBaseUrl",
      "Points Codex at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayBaseUrl",
      "Points Claude Code at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayClaudeBaseUrl",
      "Points Gemini CLI at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayGeminiBaseUrl",
      "Points OpenCode at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayOpenCodeBaseUrl",
      "Points OpenClaw at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayOpenClawBaseUrl",
      "Points Hermes at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayHermesBaseUrl",
      "Sets Claude Code to the virtual model name resolved by the Local Gateway.": "profiles.diff.gatewayClaudeModel",
      "Sets Gemini CLI to the virtual model name resolved by the Local Gateway.": "profiles.diff.gatewayGeminiModel",
      "Sets Hermes to the virtual model name resolved by the Local Gateway.": "profiles.diff.gatewayHermesModel",
      "Keeps the local gateway virtual model available to Claude Code environment consumers.": "profiles.diff.gatewayClaudeEnvModel",
      "Selects the local gateway provider/model pair in OpenCode.": "profiles.diff.gatewayOpenCodeModel",
      "Selects the local gateway provider/model pair as OpenClaw's primary default.": "profiles.diff.gatewayOpenClawModel",
      "Registers the local gateway virtual model under the managed provider.": "profiles.diff.gatewayModelRegistration",
      "Keeps the official Codex login path available while routing model requests through the Local Gateway.": "profiles.diff.keepOfficialLoginForGateway",
      "Keeps Codex API tokens scoped to the active provider so auth.json can preserve the official login.": "profiles.diff.codexScopedApiTokens",
      "Removes a legacy API-key mirror from Codex config.toml without touching auth.json.": "profiles.diff.removeLegacyApiKeyMirror",
      "Removes a legacy environment-style API key from Codex config.toml.": "profiles.diff.removeLegacyEnvApiKey",
      "Stores only the local CodeStudio token, not the real upstream Provider API key.": "profiles.diff.storeLocalToken",
      "Points Claude Code at the selected upstream Provider Base URL.": "profiles.diff.claudeBaseUrl",
      "Stores the selected Provider API key as Claude Code's bearer token.": "profiles.diff.claudeAuthToken",
      "Sets Claude Code to the selected upstream model.": "profiles.diff.claudeModel",
      "Keeps the model override available to Claude Code environment consumers.": "profiles.diff.claudeEnvModel",
      "Model is optional; no Claude model override will be written.": "profiles.diff.claudeModelOptional",
      "Model is optional; no Claude model environment override will be written.": "profiles.diff.claudeEnvModelOptional",
      "Stores the selected Provider API key for Gemini CLI.": "profiles.diff.geminiCliApiKey",
      "Points Gemini CLI at the selected upstream Provider Base URL.": "profiles.diff.geminiCliBaseUrl",
      "Sets Gemini CLI to the selected upstream model.": "profiles.diff.geminiCliModel",
      "Model is optional; no Gemini model override will be written.": "profiles.diff.geminiCliModelOptional",
      "Stores the selected Provider API key for Gemini Code Assist.": "profiles.diff.geminiCodeAssistApiKey",
      "Gemini Code Assist does not expose a VS Code setting for custom Base URL; this stays in the CodeStudio Lite profile.": "profiles.diff.geminiCodeAssistBaseUrlNotWritten",
      "Gemini Code Assist does not expose a VS Code setting for model override; this stays in the CodeStudio Lite profile.": "profiles.diff.geminiCodeAssistModelNotWritten",
      "Model is optional and Gemini Code Assist has no model override setting to write.": "profiles.diff.geminiCodeAssistModelOptional",
      "Keeps OpenCode config aligned with the published schema.": "profiles.diff.opencodeSchema",
      "Uses OpenCode's OpenAI-compatible provider package.": "profiles.diff.opencodeProviderPackage",
      "Points OpenCode at the selected upstream Provider Base URL.": "profiles.diff.opencodeBaseUrl",
      "Stores the selected Provider API key for OpenCode.": "profiles.diff.opencodeApiKey",
      "Selects the provider/model pair in OpenCode.": "profiles.diff.opencodeModel",
      "Registers the selected model under the managed provider.": "profiles.diff.opencodeModelRegistration",
      "Model is optional; no OpenCode model override will be written.": "profiles.diff.opencodeModelOptional",
      "Merges CodeStudio Lite provider definitions with existing OpenClaw providers.": "profiles.diff.openclawMergeMode",
      "Uses OpenClaw's OpenAI-compatible API adapter.": "profiles.diff.openclawApiAdapter",
      "Points OpenClaw at the selected upstream Provider Base URL.": "profiles.diff.openclawBaseUrl",
      "Stores the selected Provider API key for OpenClaw.": "profiles.diff.openclawApiKey",
      "Selects the provider/model pair as OpenClaw's primary default.": "profiles.diff.openclawModel",
      "Model is optional; no OpenClaw model override will be written.": "profiles.diff.openclawModelOptional",
      "Selects Hermes custom provider mode.": "profiles.diff.hermesCustomProvider",
      "Points Hermes at the selected upstream Provider Base URL.": "profiles.diff.hermesBaseUrl",
      "Stores the selected Provider API key for Hermes.": "profiles.diff.hermesApiKey",
      "Uses Hermes' OpenAI Chat Completions custom endpoint mode.": "profiles.diff.hermesApiMode",
      "Sets Hermes to the selected upstream model.": "profiles.diff.hermesModel",
      "Model is optional; no Hermes model override will be written.": "profiles.diff.hermesModelOptional"
    };
    const exactKey = exact[message];
    if (exactKey) {
      return $t(exactKey);
    }

    const adapterMatch = message.match(/Config file mode adapter is not implemented for '([^']+)'\./);
    if (adapterMatch) {
      return $t("profiles.warning.configAdapterMissing", { app: adapterMatch[1] });
    }

    const noNativeGatewayMatch = message.match(/No native gateway bootstrap is written for '([^']+)'; configure the client to use the Gateway URL manually or wait for a validated adapter\./);
    if (noNativeGatewayMatch) {
      return $t("profiles.warning.noNativeGatewayBootstrap", { app: noNativeGatewayMatch[1] });
    }

    const localRegistryMatch = message.match(/Tool '([^']+)' is not in (?:the local|the preview) registry(?:, so this profile cannot be applied yet)?\./);
    if (localRegistryMatch) {
      return $t("profiles.warning.toolCannotApply", { app: localRegistryMatch[1] });
    }

    const parseErrorMatch = message.match(/Existing Codex config could not be parsed, so only create-style preview is available: (.+)/);
    if (parseErrorMatch) {
      return $t("profiles.warning.codexConfigParseFailed", { error: parseErrorMatch[1] });
    }

    const hermesParseErrorMatch = message.match(/Existing Hermes config could not be parsed, so only create-style preview is available: (.+)/);
    if (hermesParseErrorMatch) {
      return $t("profiles.warning.hermesConfigParseFailed", { error: hermesParseErrorMatch[1] });
    }

    const missingConfigMatch = message.match(/^(.+) does not exist yet; adapter would create it after confirmation\.$/);
    if (missingConfigMatch) {
      return $t("profiles.warning.genericConfigMissing", { name: missingConfigMatch[1] });
    }

    const parseErrorGenericMatch = message.match(/^Existing (.+) could not be parsed, so only create-style preview is available: (.+)$/);
    if (parseErrorGenericMatch) {
      return $t("profiles.warning.genericConfigParseFailed", { name: parseErrorGenericMatch[1], error: parseErrorGenericMatch[2] });
    }

    return message;
  }

  function errorLabel(message: string) {
    if (message === "Profile Name is required") {
      return $t("wizard.error.profileNameRequired");
    }
    if (message === "Base URL must start with http:// or https://") {
      return $t("wizard.error.baseUrlScheme");
    }
    if (message === "Base URL cannot contain whitespace") {
      return $t("wizard.error.baseUrlWhitespace");
    }
    if (message === "Base URL must include a host") {
      return $t("wizard.error.baseUrlHost");
    }
    if (message === "Unsupported Provider API protocol.") {
      return $t("wizard.error.protocolUnsupported");
    }
    const configProtocolMatch = message.match(/Config file mode does not support (.+) for '([^']+)'\./);
    if (configProtocolMatch) {
      return $t("wizard.error.configProtocolUnsupported", {
        protocol: configProtocolMatch[1],
        tool: toolLabels[canonicalProfileToolId(configProtocolMatch[2])] ?? configProtocolMatch[2]
      });
    }
    if (message === "Timeout must be between 5 and 600 seconds.") {
      return $t("wizard.error.timeoutRange");
    }
    if (message === "Provider API key is required for non-official providers.") {
      return $t("wizard.check.credentialMissing");
    }
    if (message === "Official provider uses the client login directly and cannot use Gateway mode.") {
      return $t("profiles.officialGatewayBlocked");
    }
    if (message === "Built-in official profiles cannot be modified.") {
      return $t("profiles.builtinModifyBlocked");
    }
    if (message === "Built-in official profiles cannot be duplicated.") {
      return $t("profiles.builtinDuplicateBlocked");
    }
    if (message === "Built-in official profiles cannot be imported.") {
      return $t("profiles.officialCustomImportBlocked");
    }
    if (message === "Official profiles are built in and cannot be saved as custom profiles.") {
      return $t("profiles.officialCustomSaveBlocked");
    }
    if (message === "Official profiles are built in and cannot be imported.") {
      return $t("profiles.officialCustomImportBlocked");
    }
    if (message === "Official provider uses the client login directly and does not run through the local gateway.") {
      return $t("profiles.warning.officialGatewayUnsupported");
    }
    if (message === "Profile is already active for this tool and mode.") {
      return $t("profiles.alreadyActiveBlocked");
    }
    const toolNotInstalledMatch = message.match(/Tool '([^']+)' is not installed, so a profile cannot be created for it\./);
    if (toolNotInstalledMatch) {
      return $t("wizard.error.toolNotInstalled", {
        tool: toolLabels[canonicalProfileToolId(toolNotInstalledMatch[1])] ?? toolNotInstalledMatch[1]
      });
    }
    if (message === "Apply and restart is only available for Config file mode.") {
      return $t("profiles.restartConfigOnly");
    }
    if (message === "Apply and restart requires a native client config write for this profile.") {
      return $t("profiles.restartNeedsNativeWrite");
    }
    const applyToolMatch = message.match(/Tool '([^']+)' is not in the local registry, so this profile cannot be applied yet\./);
    if (applyToolMatch) {
      return $t("profiles.warning.toolCannotApply", { app: applyToolMatch[1] });
    }
    const unsupportedModeMatch = message.match(/(.+) mode is not supported for this profile\./);
    if (unsupportedModeMatch) {
      return $t("profiles.warning.modeUnsupported", { mode: unsupportedModeMatch[1] });
    }
    return message;
  }

  function buildProfileModeSections(
    profileSummary: ProfileSummary | null,
    installedToolIds: Set<string> | null
  ): ProfileModeSection[] {
    const drafts = profileSummary?.drafts ?? [];
    const activeByMode = profileSummary?.activeProfilesByMode ?? { config: {}, gateway: {} };
    return [
      {
        mode: "config",
        titleKey: "profiles.mode.configSectionTitle",
        descriptionKey: "profiles.mode.configSectionDescription",
        groups: buildProfileGroups(
          drafts.filter((profile) => profile.mode === "config" && profileVisibleInProfiles(profile)),
          activeByMode.config,
          installedToolIds
        )
      },
      {
        mode: "gateway",
        titleKey: "profiles.mode.gatewaySectionTitle",
        descriptionKey: "profiles.mode.gatewaySectionDescription",
        groups: buildProfileGroups(
          drafts.filter((profile) => profile.mode === "gateway" && profileVisibleInProfiles(profile)),
          activeByMode.gateway,
          installedToolIds
        )
      }
    ];
  }

  function buildProfileGroups(
    profiles: ProfileDraft[],
    activeProfiles: Record<string, string>,
    installedToolIds: Set<string> | null
  ): ProfileGroup[] {
    const grouped = new Map<string, ProfileDraft[]>();
    for (const profile of profiles) {
      const app = canonicalProfileToolId(profile.app);
      if (installedToolIds && !installedToolIds.has(app)) {
        continue;
      }
      grouped.set(app, [...(grouped.get(app) ?? []), { ...profile, app }]);
    }

    return [...grouped.entries()]
      .sort(([left], [right]) => {
        const leftIndex = toolOrder.indexOf(left);
        const rightIndex = toolOrder.indexOf(right);
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
      })
      .map(([app, appProfiles]) => {
        const activeProfileId = activeProfileIdForApp(activeProfiles, app);
        const sortedProfiles = [...appProfiles].sort((left, right) => {
          if (left.isBuiltin !== right.isBuiltin) {
            return left.isBuiltin ? -1 : 1;
          }
          return left.name.localeCompare(right.name);
        });
        const activeProfileName = sortedProfiles.find((profile) => profile.id === activeProfileId)?.name ?? null;
        return {
          id: app,
          label: toolLabels[app] ?? app,
          activeProfileId,
          activeProfileName,
          profiles: sortedProfiles
        };
      });
  }

  function buildInstalledProfileToolIds(detection: DetectionSnapshot | null): Set<string> | null {
    if (!detection) {
      return null;
    }
    return new Set(
      detection.tools
        .filter((tool) => tool.installState === "installed")
        .map((tool) => canonicalProfileToolId(tool.id))
    );
  }

  function emptyProfilesMessageKey(
    profileSummary: ProfileSummary | null,
    visibleCount: number,
    installedToolIds: Set<string> | null
  ): TranslationKey {
    if (profileSummary && installedToolIds && profileSummary.drafts.length > 0 && visibleCount === 0) {
      return "profiles.noInstalledToolProfiles";
    }
    return "profiles.noProfiles";
  }

  function canonicalProfileToolId(toolId: string) {
    const normalized = toolId.trim().toLowerCase();
    if (["codex-app", "codex-client", "codex-desktop", "codex-cli", "codex-vscode", "codex-code-vscode", "codex-vs-code"].includes(normalized)) {
      return "codex";
    }
    if (["claude-app", "claude-client"].includes(normalized)) {
      return "claude-desktop";
    }
    if (["claude-vscode", "claude-code-vscode", "claude-vs-code"].includes(normalized)) {
      return "claude";
    }
    if (["gemini-vscode", "gemini-code-vscode", "gemini-vs-code"].includes(normalized)) {
      return "gemini-code-assist";
    }
    if (normalized === "hermes-agent") {
      return "hermes";
    }
    return normalized;
  }

  function profileVisibleInProfiles(profile: ProfileDraft) {
    return !(canonicalProfileToolId(profile.app) === "codex" && providerIsOfficial(profile.provider));
  }

  async function loadCodexAuthPreservationSetting() {
    try {
      const settings = await loadAppSettings();
      return settings.preserveCodexOfficialAuth;
    } catch {
      return true;
    }
  }

  function activeCodexConfigProfileIsOfficial(profileSummary: ProfileSummary | null) {
    const activeProfileId =
      profileSummary?.activeProfilesByMode.config.codex ??
      profileSummary?.activeProfilesByMode.config["codex-app"] ??
      null;
    if (!activeProfileId) {
      return false;
    }
    const activeProfile = profileSummary?.drafts.find((profile) => profile.id === activeProfileId);
    return activeProfile?.provider.trim() === "official";
  }

  function protocolOptionsFor(toolId: string, mode: ProviderApplyMode): readonly ProtocolOption[] {
    if (mode === "gateway") {
      return protocolOptions;
    }
    const supportedIds = configModeProtocolIdsByTool[canonicalProfileToolId(toolId)] ?? [];
    return protocolOptions.filter((option) => supportedIds.includes(option.id));
  }

  function protocolOptionAvailable(options: readonly ProtocolOption[], value: string) {
    const normalized = normalizeProtocol(value);
    return options.some((option) => option.id === normalized);
  }

  function coerceProtocolForToolMode(toolId: string, mode: ProviderApplyMode, value: string) {
    const options = protocolOptionsFor(toolId, mode);
    const normalized = normalizeProtocol(value);
    return options.some((option) => option.id === normalized)
      ? normalized
      : options[0]?.id ?? normalized;
  }

  function isProtocolAllowedForToolMode(toolId: string, mode: ProviderApplyMode, value: string) {
    return protocolOptionAvailable(protocolOptionsFor(toolId, mode), value);
  }

  function activeProfileIdForApp(activeProfiles: Record<string, string>, app: string) {
    if (activeProfiles[app]) {
      return activeProfiles[app];
    }
    if (app === "codex") {
      return activeProfiles["codex-app"] ?? null;
    }
    return null;
  }

  function isProfileActive(profile: ProfileDraft) {
    if (!summary) {
      return false;
    }
    const app = canonicalProfileToolId(profile.app);
    const activeProfiles = summary.activeProfilesByMode[profile.mode];
    return activeProfileIdForApp(activeProfiles, app) === profile.id;
  }

  function baseUrlValidationErrorKey(value: string): TranslationKey | null {
    const trimmed = value.trim();
    if (!trimmed) {
      return "wizard.error.baseUrlRequired";
    }
    if (/\s/.test(trimmed)) {
      return "wizard.error.baseUrlWhitespace";
    }
    if (!/^https?:\/\//.test(trimmed)) {
      return "wizard.error.baseUrlScheme";
    }
    try {
      const parsed = new URL(trimmed);
      if (!["http:", "https:"].includes(parsed.protocol)) {
        return "wizard.error.baseUrlScheme";
      }
      if (!parsed.hostname || parsed.hostname.startsWith(".")) {
        return "wizard.error.baseUrlHost";
      }
    } catch {
      return "wizard.error.baseUrlHost";
    }
    return null;
  }
</script>

<div class="route-stack">
  <section class="top-strip">
    <div>
      <span class="eyebrow">{$t("profiles.eyebrow")}</span>
      <h1>{$t("profiles.title")}</h1>
      <p>{summary ? $t("profiles.pathNote", { path: summary.profilesDir }) : $t("profiles.loading")}</p>
    </div>
    <div class="top-actions">
      <input
        bind:this={importFileInput}
        type="file"
        accept="application/json,.json"
        hidden
        on:change={handleImportFile}
      />
      <button
        class="secondary-button"
        title={$t("profiles.importProfile")}
        disabled={profileIoBusy !== null}
        on:click={openImportDialog}
      >
        {#if profileIoBusy === "import"}
          <AppIcon name="loading" class="spin" size={16} />
          {$t("profiles.importing")}
        {:else}
          <AppIcon name="upload" size={16} />
          {$t("common.import")}
        {/if}
      </button>
      <button
        class="primary-button"
        title={$t("profiles.exportProfile")}
        disabled={profileIoBusy !== null || !summary || customProfileCount === 0}
        on:click={handleExportProfiles}
      >
        {#if profileIoBusy === "export"}
          <AppIcon name="loading" class="spin" size={16} />
          {$t("profiles.exporting")}
        {:else}
          <AppIcon name="download" size={16} />
          {$t("common.export")}
        {/if}
      </button>
    </div>
  </section>

  {#if profileIoError}
    <div class="error-banner">{profileIoError}</div>
  {/if}
  {#if profileIoMessage}
    <div class="inline-success">{profileIoMessage}</div>
  {/if}

  {#if summary}
    <div class="profile-mode-layout">
      {#each profileModeSections as section}
        <section class="panel-band profile-mode-section">
          <div class="section-heading">
            <div>
              <h2>{$t(section.titleKey)}</h2>
              <p>{$t(section.descriptionKey)}</p>
            </div>
          </div>

          {#each section.groups as group}
            <div class="profile-tool-section">
              <div class="section-heading compact">
                <div class="tool-section-title">
                  <ToolIcon toolId={group.id} label={group.label} variant="heading" />
                  <div>
                    <h2>{group.label}</h2>
                    <p>{group.activeProfileName ?? $t("profiles.noActiveForToolInMode")}</p>
                  </div>
                </div>
                <StatusPill
                  status={group.activeProfileName ? "ok" : "info"}
                  label={group.activeProfileName ? $t("profiles.oneActivePerTool") : $t("profiles.noActiveProfile")}
                />
              </div>

              <div class="profile-grid">
                {#each group.profiles as profile}
                  {@const isActive = group.activeProfileId === profile.id}
                  {@const cardActionKey = actionKey(profile.id, profile.mode)}
                  <article class:active-profile={isActive} class:builtin-profile={profile.isBuiltin} class="profile-card">
                    <div>
                      <span class="eyebrow">{protocolLabel(profile.protocol)} / {profileProviderLabel(profile)}</span>
                      <h2>{profile.name}</h2>
                      <p>{profile.model || $t("common.none")}</p>
                      <p>{profileEndpointLabel(profile)}</p>
                      <p>
                        {profileCredentialLabel(profile)} /
                        {$t("profiles.timeoutSeconds", { seconds: profile.timeoutSeconds })}
                      </p>
                      {#if profile.isBuiltin}
                        <p class="protected-profile-note">{$t("profiles.builtinProtected")}</p>
                      {/if}
                      {#if codexOfficialAuthDetail(profile)}
                        <p class="protected-profile-note">{codexOfficialAuthDetail(profile)}</p>
                      {/if}
                    </div>
                    <div>
                      <StatusPill
                        status={isActive ? "ok" : "info"}
                        label={isActive ? $t("common.active") : profile.isBuiltin ? $t("profiles.builtinOfficial") : $t("common.ready")}
                      />
                    </div>
                    <div class="card-actions">
                      <button
                        class="primary-button"
                        disabled={isActive || applyingId !== null}
                        title={isActive ? $t("profiles.alreadyActiveProfile") : $t("profiles.previewModeApply", { name: profile.name, mode: applyModeLabel(profile.mode) })}
                        on:click={() => openApply(profile)}
                      >
                        <AppIcon name="apply" size={16} />
                        {#if isActive}
                          {$t("common.active")}
                        {:else}
                          {applyingId === cardActionKey && pendingApply?.id === profile.id ? $t("common.loading") : $t("common.apply")}
                        {/if}
                      </button>
                      {#if !profile.isBuiltin}
                        <button class="icon-button" title={$t("profiles.editProfile")} disabled={duplicatingId !== null} on:click={() => openEdit(profile)}><AppIcon name="edit" size={16} /></button>
                        <button
                          class="icon-button"
                          title={$t("profiles.duplicateProfile")}
                          disabled={duplicatingId !== null || applyingId !== null || editingId !== null}
                          on:click={() => handleDuplicate(profile)}
                        >
                          {#if duplicatingId === profile.id}
                            <AppIcon name="loading" class="spin" size={16} />
                          {:else}
                            <AppIcon name="copy" size={16} />
                          {/if}
                        </button>
                        <button class="icon-button danger" title={$t("profiles.deleteProfile")}><AppIcon name="delete" size={16} /></button>
                      {/if}
                    </div>
                  </article>
                {/each}
              </div>
            </div>
          {:else}
            <div class="empty-row">{$t(emptyProfilesMessageKey(summary, visibleProfileCount, installedProfileToolIds))}</div>
          {/each}
        </section>
      {/each}
    </div>
  {:else}
    <section class="panel-band">
      <div class="empty-row">{$t("profiles.noProfiles")}</div>
    </section>
  {/if}

  {#if pendingEdit}
    <div class="modal-backdrop" role="presentation">
      <div class="modal-panel wide-modal" role="dialog" aria-modal="true" aria-labelledby="edit-title">
        <div>
          <span class="eyebrow">{$t("profiles.editEyebrow")}</span>
          <h2 id="edit-title">{$t("profiles.editTitle", { name: pendingEdit.name })}</h2>
          <p>{$t("profiles.editDescription")}</p>
        </div>

        {#if editError}
          <div class="inline-error">{editError}</div>
        {/if}

        <div class="form-grid edit-profile-form">
          <label>
            {$t("wizard.profileName")}
            <input bind:value={editForm.name} disabled={editingId !== null} />
          </label>
          <label>
            {$t("profiles.tool")}
            <input value={toolLabels[pendingEdit.app] ?? pendingEdit.app} disabled />
          </label>
          <div class="edit-mode-field">
            {$t("profiles.providerModeTitle")}
            <div class="edit-mode-toggle" role="group" aria-label={$t("profiles.providerModeTitle")}>
              <button
                type="button"
                class:selected={editForm.mode === "config"}
                disabled={editingId !== null}
                on:click={() => selectEditMode("config")}
              >
                <span>{$t("profiles.mode.config")}</span>
                <small>{$t("profiles.mode.configShortDescription")}</small>
              </button>
              <button
                type="button"
                class:selected={editForm.mode === "gateway"}
                disabled={editingId !== null}
                on:click={() => selectEditMode("gateway")}
              >
                <span>{$t("profiles.mode.gateway")}</span>
                <small>{$t("profiles.mode.gatewayShortDescription")}</small>
              </button>
            </div>
          </div>
          <label>
            {$t("common.provider")}
            <input bind:value={editForm.provider} disabled={editingId !== null} />
          </label>
        <label>
          {$t("wizard.protocol")}
          <select bind:value={editForm.protocol} disabled={editingId !== null}>
            {#each availableEditProtocolOptions as option}
              <option value={option.id}>{$t(option.labelKey)}</option>
            {/each}
          </select>
        </label>
          <label>
            {$t("common.model")}
            <input bind:value={editForm.model} disabled={editingId !== null} />
          </label>
          {#if providerNeedsBaseUrl(editForm.provider)}
            <label>
              {$t("wizard.providerBaseUrl")}
              <input bind:value={editForm.baseUrl} disabled={editingId !== null} />
              {#if editBaseUrlErrorKey}
                <small class="field-error">{$t(editBaseUrlErrorKey)}</small>
              {/if}
            </label>
          {/if}
          <label>
            {$t("wizard.timeoutSeconds")}
            <input type="number" min="5" max="600" bind:value={editForm.timeoutSeconds} disabled={editingId !== null} />
          </label>
          {#if providerRequiresApiKey(editForm.provider)}
            <label>
              {$t("wizard.providerApiKey")}
              <input
                type="password"
                bind:value={editForm.apiKey}
                placeholder={$t(pendingEdit.authRef ? "profiles.keepExistingSecret" : "profiles.newSecretRequired")}
                disabled={editingId !== null}
              />
            </label>
          {/if}
        </div>

        <div class="modal-actions">
          <button class="secondary-button" disabled={editingId !== null} on:click={closeEdit}>
            {$t("common.cancel")}
          </button>
          <button class="primary-button" disabled={!canSaveEdit} on:click={handleEditSave}>
            {#if editingId === pendingEdit.id}
              <AppIcon name="loading" class="spin" size={16} />
              {$t("common.saving")}
            {:else}
              <AppIcon name="edit" size={16} />
              {$t("profiles.saveEdit")}
            {/if}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if pendingApply}
    <div class="modal-backdrop" role="presentation">
      <div class="modal-panel wide-modal" role="dialog" aria-modal="true" aria-labelledby="apply-title">
        <div>
          <span class="eyebrow">{$t("profiles.applyEyebrow")}</span>
          <h2 id="apply-title">{$t("profiles.applyTitle", { name: pendingApply.name })}</h2>
          <p>{$t("profiles.applyDescription")}</p>
        </div>

        {#if applyError}
          <div class="inline-error">{applyError}</div>
        {/if}

        {#if applyResult}
          <div class="inline-success">
            {$t("profiles.applySuccess", { id: applyResult.backup.id })}
            {#if applyResult.nativeVerified && applyResult.nativePath}
              {$t("profiles.codexConfigVerified", { path: applyResult.nativePath })}
            {/if}
            {#if applyResult.gatewayStatus?.running}
              {$t("profiles.gatewayStarted", { url: applyResult.gatewayStatus.baseUrl })}
            {/if}
            {#if applyResult.restartRequested && applyResult.restartMessage}
              {$t("profiles.restartCompleted", { message: applyResult.restartMessage })}
            {/if}
          </div>
        {/if}

        {#if applyPreview}
          {#if applyEnvConflicts.length > 0}
            <section class="native-diff env-conflict-panel">
              <div class="native-diff-heading">
                <div>
                  <strong>{$t("envConflict.title")}</strong>
                  <span>{$t("envConflict.applyDescription", { count: applyEnvConflicts.length })}</span>
                </div>
                <button class="secondary-button" disabled={clearingEnvConflict || applyingId !== null} on:click={clearApplyEnvConflicts}>
                  {#if clearingEnvConflict}
                    <AppIcon name="loading" class="spin" size={16} />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {$t("envConflict.clearAction")}
                </button>
              </div>
              <div class="preview-list compact-conflict-list">
                {#each applyEnvConflicts as conflict}
                  <div>
                    <strong>{conflict.variable} / {conflict.scope}</strong>
                    <span>{conflict.message}</span>
                    <code>{conflict.currentValuePreview}</code>
                  </div>
                {/each}
              </div>
            </section>
          {/if}

          {#if pendingApplyDisplacesCodexOAuth}
            <section class="native-diff auth-conflict-panel">
              <div class="native-diff-heading">
                <div>
                  <strong>{$t("profiles.codexOAuthConflictTitle")}</strong>
                  <span>{$t("profiles.codexOAuthApiDisplacesOAuth")}</span>
                </div>
                <StatusPill status={codexAuthConflictConfirmed ? "ok" : "warning"} label={codexAuthConflictConfirmed ? $t("profiles.codexOAuthConflictConfirmed") : $t("profiles.codexOAuthConflictNeedsConfirm")} />
              </div>
            </section>
          {/if}

          {#if selectedModePreview?.blockedReason || selectedModePreview?.warnings.length || canSyncClaudeVsCodePlugin}
          <section class="native-diff">
            {#if selectedModePreview?.blockedReason}
              <div class="inline-error">{previewTextLabel(selectedModePreview.blockedReason)}</div>
            {/if}

            {#if selectedModePreview?.warnings.length}
              <div class="preview-warnings">
                {#each selectedModePreview.warnings as warning}
                  <span>{previewTextLabel(warning)}</span>
                {/each}
              </div>
            {/if}

            {#if canSyncClaudeVsCodePlugin}
              <label class="native-write-toggle">
                <input type="checkbox" bind:checked={syncClaudeVsCodePlugin} disabled={applyingId !== null} />
                <span>
                  <strong>{$t("profiles.syncClaudeVsCode")}</strong>
                  <small>{$t("profiles.syncClaudeVsCodeDescription")}</small>
                </span>
              </label>
            {/if}
          </section>
          {/if}

          <div class="preview-list apply-preview-list">
            {#each applyPreview.items as item}
              <div class="apply-preview-row">
                <div class="apply-preview-meta">
                  <strong>{applyPreviewLabel(item)}</strong>
                  <span>{applyActionLabel(item.action)}</span>
                  {#if item.backupRequired}
                    <span>{$t("profiles.backupBadge")}</span>
                  {/if}
                </div>
                {#if item.path}
                  <code>{item.path}</code>
                {/if}
                <span>{applyPreviewDetail(item)}</span>
              </div>
            {/each}
          </div>

          {#if selectedNativeDiff}
            <section class="native-diff">
              <div class="native-diff-heading">
                <div>
                  <strong>{$t(selectedApplyMode === "config" ? "profiles.configModeDiff" : "profiles.gatewayModeDiff")}</strong>
                  <span>{selectedNativeDiff.path}</span>
                </div>
                <StatusPill status="info" label={selectedNativeDiff.writeEnabled ? $t("common.writeEnabled") : $t("common.readOnly")} />
              </div>

              <div class="native-diff-list">
                {#each selectedNativeDiff.changes as change}
                  <div class="native-diff-row">
                    <span>{nativeDiffActionLabel(change.action)}</span>
                    <code>{change.key}</code>
                    <div>
                      <b>{$t("common.before")}</b>
                      <em>{change.before ?? $t("profiles.beforeMissing")}</em>
                    </div>
                    <div>
                      <b>{$t("common.after")}</b>
                      <em>{change.after ?? $t("profiles.afterRemoved")}</em>
                    </div>
                    <p>{previewTextLabel(change.detail)}</p>
                  </div>
                {/each}
              </div>

              {#if selectedNativeDiff.warnings.length > 0}
                <div class="preview-warnings">
                  {#each selectedNativeDiff.warnings as warning}
                    <span>{previewTextLabel(warning)}</span>
                  {/each}
                </div>
              {/if}
            </section>
          {/if}

          {#if applyPreview.warnings.length > 0}
            <div class="preview-warnings">
              {#each applyPreview.warnings as warning}
                <span>{previewTextLabel(warning)}</span>
              {/each}
            </div>
          {/if}
        {:else if applyingId === actionKey(pendingApply.id, pendingApplyMode)}
          <div class="empty-row">
            <AppIcon name="loading" class="spin" size={18} />
            {$t("common.loading")}
          </div>
        {/if}

        <div class="modal-actions">
          <button class="secondary-button" disabled={applyingId !== null} on:click={closeApply}>
            {applyResult ? $t("common.close") : $t("common.cancel")}
          </button>
          {#if !applyResult}
            {#if selectedApplyMode === "config" && selectedModePreview?.writesNativeConfig}
              <button
                class="secondary-button"
                disabled={applyingId !== null || !applyPreview?.canApply || !selectedModeSupported}
                on:click={() => handleApplyWithOptions(pendingApply!.id, true)}
              >
                {#if applyingId === actionKey(pendingApply.id, selectedApplyMode, true, canSyncClaudeVsCodePlugin && syncClaudeVsCodePlugin)}
                  <AppIcon name="loading" class="spin" size={16} />
                  {$t("common.loading")}
                {:else}
                  <AppIcon name="apply" size={16} />
                  {$t(pendingApplyDisplacesCodexOAuth && !codexAuthConflictConfirmed ? "profiles.confirmAndApplyRestart" : "profiles.applyAndRestart")}
                {/if}
              </button>
            {/if}
            <button
              class="primary-button"
              disabled={applyingId !== null || !applyPreview?.canApply || !selectedModeSupported}
              on:click={() => handleApplyWithOptions(pendingApply!.id)}
            >
              {#if applyingId === actionKey(pendingApply.id, selectedApplyMode, false, canSyncClaudeVsCodePlugin && syncClaudeVsCodePlugin)}
                <AppIcon name="loading" class="spin" size={16} />
                {$t("common.loading")}
              {:else}
                <AppIcon name="apply" size={16} />
                {$t(pendingApplyDisplacesCodexOAuth && !codexAuthConflictConfirmed ? "profiles.confirmAndApply" : selectedApplyMode === "gateway" ? "profiles.applyGatewayMode" : "profiles.applyConfigMode")}
              {/if}
            </button>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>
