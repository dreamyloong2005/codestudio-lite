<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { fade, fly } from "svelte/transition";
  import { detectEnvironment, openExternalUrl, previewProfileWrite, saveProfileDraft, startCodexOAuthLogin } from "../lib/api";
  import { t, type TranslationKey } from "../lib/i18n";
  import AppIcon from "../components/AppIcon.svelte";
  import SecretInput from "../components/SecretInput.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import {
    actionButtonRecipe,
    wizardActionsRecipe,
    wizardButtonRowRecipe,
    wizardChoiceButtonRecipe,
    wizardChoiceGridRecipe,
    wizardCodexAuthCardRecipe,
    wizardFieldErrorRecipe,
    wizardFormGridRecipe,
    wizardInlineNoticeRecipe,
    wizardModeChoiceRecipe,
    wizardPanelRecipe,
    wizardPreviewBoxRecipe,
    wizardPreviewHeadingRecipe,
    wizardPreviewWarningsRecipe,
    routeStackRecipe,
    topStripRecipe,
    wizardSecurityNoteRecipe,
    wizardStepContentRecipe,
    wizardStepItemRecipe,
    wizardStepperRecipe,
    spinRecipe,
    wizardWideFieldRecipe,
    wizardWriteContentPreviewRecipe,
    wizardWritePreviewListRecipe,
    wizardWritePreviewMetaRecipe,
    wizardWritePreviewRowRecipe
  } from "../../styled-system/recipes";
  import type {
    DetectionSnapshot,
    InstallState,
    PreviewProfileWriteResult,
    ProfileWritePreviewItem,
    ProviderApplyMode,
    SaveProfileDraftRequest,
    WizardPrefill
  } from "../types";

  const CODEX_AUTH_URL = "https://developers.openai.com/codex/auth";
  const wizardStepEnter = { y: 14, duration: 240, opacity: 0, easing: cubicOut };
  const wizardStepExit = { duration: 110 };

  export let onProfileSaved: (mode: ProviderApplyMode) => void | Promise<void> = () => {};
  export let prefill: WizardPrefill | null = null;
  export let snapshot: DetectionSnapshot | null = null;

  type ToolDefaults = {
    id: string;
    label: string;
    profileNameKey: TranslationKey;
    protocol: string;
    baseUrl: string;
    model: string;
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
    "claude-desktop": ["anthropic-messages"],
    claude: ["anthropic-messages"],
    gemini: ["google-gemini"],
    "gemini-code-assist": ["google-gemini"],
    opencode: ["openai-chat-completions", "openai-responses"],
    openclaw: ["openai-chat-completions"],
    hermes: ["openai-chat-completions"]
  };

  const steps: TranslationKey[] = [
    "wizard.step.bootstrapTarget",
    "wizard.step.chooseProvider",
    "wizard.step.preview"
  ];

  const toolDefaults: ToolDefaults[] = [
    {
      id: "codex",
      label: "Codex",
      profileNameKey: "wizard.defaultProfile.codex",
      protocol: "openai-responses",
      baseUrl: "",
      model: ""
    },
    {
      id: "claude-desktop",
      label: "Claude Desktop",
      profileNameKey: "wizard.defaultProfile.claudeDesktop",
      protocol: "anthropic-messages",
      baseUrl: "",
      model: ""
    },
    {
      id: "claude",
      label: "Claude Code",
      profileNameKey: "wizard.defaultProfile.claude",
      protocol: "anthropic-messages",
      baseUrl: "",
      model: ""
    },
    {
      id: "gemini",
      label: "Gemini CLI",
      profileNameKey: "wizard.defaultProfile.gemini",
      protocol: "google-gemini",
      baseUrl: "",
      model: ""
    },
    {
      id: "gemini-code-assist",
      label: "Gemini Code Assist",
      profileNameKey: "wizard.defaultProfile.geminiCodeAssist",
      protocol: "google-gemini",
      baseUrl: "",
      model: ""
    },
    {
      id: "opencode",
      label: "OpenCode",
      profileNameKey: "wizard.defaultProfile.opencode",
      protocol: "openai-chat-completions",
      baseUrl: "",
      model: ""
    },
    {
      id: "openclaw",
      label: "OpenClaw",
      profileNameKey: "wizard.defaultProfile.openclaw",
      protocol: "openai-chat-completions",
      baseUrl: "",
      model: ""
    },
    {
      id: "hermes",
      label: "Hermes",
      profileNameKey: "wizard.defaultProfile.hermes",
      protocol: "openai-chat-completions",
      baseUrl: "",
      model: ""
    }
  ];

  let currentStep = 0;
  let selectedTool = "codex";
  let provider = "compatible";
  let profileMode: ProviderApplyMode = "config";
  let protocol = "openai-responses";
  let profileName = $t("wizard.defaultProfile.codex");
  let profileRemark = "";
  let apiKey = "";
  let baseUrl = "";
  let model = "";
  let saving = false;
  let saveError: string | null = null;
  let savedProfileName: string | null = null;
  let appliedPrefillKey: string | null = null;
  let previewing = false;
  let previewError: string | null = null;
  let writePreview: PreviewProfileWriteResult | null = null;
  let writePreviewKey: string | null = null;
  let codexOAuthConfig = false;
  let codexAuthChecking = false;
  let codexAuthError: string | null = null;
  let codexAuthMessage: string | null = null;
  let localCodexAuth = snapshot?.codexAuth ?? null;

  $: if (prefill && appliedPrefillKey !== prefillKey(prefill)) {
    if (prefill.toolId) {
      applyToolDefaults(prefill.toolId, prefill.toolName, prefill.mode ?? "config");
    } else {
      setProfileMode(prefill.mode ?? "config");
      resetDraftState();
    }
    appliedPrefillKey = prefillKey(prefill);
  }

  $: if (snapshot?.codexAuth) {
    localCodexAuth = snapshot.codexAuth;
  }
  $: selectedToolInstalled = toolCanCreateProfile(selectedTool);
  $: visibleToolDefaults = toolDefaults.filter((tool) => toolVisibleInSnapshot(tool.id));
  $: canUseCodexOAuthConfig = canonicalProfileToolId(selectedTool) === "codex" && profileMode === "config";
  $: if (!canUseCodexOAuthConfig && codexOAuthConfig) {
    codexOAuthConfig = false;
  }
  $: activeProvider = codexOAuthConfig ? "official" : provider;
  $: activeProtocol = codexOAuthConfig ? "openai-responses" : protocol;
  $: activeModel = codexOAuthConfig ? "" : model;
  $: activeBaseUrl = codexOAuthConfig ? "" : baseUrl;
  $: activeApiKey = codexOAuthConfig ? "" : apiKey;
  $: activeSecretProvided = !codexOAuthConfig && apiKey.trim().length > 0;
  $: codexOAuthAuthorized = codexAuthIsOAuth(localCodexAuth);
  $: availableProtocolOptions = protocolOptionsFor(selectedTool, profileMode);
  $: if (
    availableProtocolOptions.length > 0 &&
    !protocolOptionAvailable(availableProtocolOptions, protocol)
  ) {
    protocol = availableProtocolOptions[0].id;
  }
  $: previewRequestKey = [
    profileName.trim(),
    profileRemark.trim(),
    selectedTool.trim(),
    profileMode,
    activeProvider.trim(),
    activeProtocol.trim(),
    activeModel.trim(),
    activeBaseUrl.trim(),
    activeSecretProvided ? "secret" : "no-secret",
    codexOAuthConfig ? "codex-oauth" : "api"
  ].join("|");
  $: baseUrlErrorKey = providerNeedsBaseUrl(activeProvider) ? baseUrlValidationErrorKey(activeBaseUrl) : null;
  $: visibleBaseUrlErrorKey =
    baseUrlErrorKey === "wizard.error.baseUrlRequired" ? null : baseUrlErrorKey;
  $: if (currentStep === steps.length - 1 && canApply && writePreviewKey !== previewRequestKey && !previewing) {
    void refreshWritePreview(previewRequestKey);
  }
  $: canApply =
    profileName.trim().length > 0 &&
    selectedTool.trim().length > 0 &&
    selectedToolInstalled &&
    activeProvider.trim().length > 0 &&
    isProtocolAllowedForToolMode(selectedTool, profileMode, activeProtocol) &&
    (!providerNeedsBaseUrl(activeProvider) || baseUrlErrorKey === null) &&
    (!providerRequiresApiKey(activeProvider) || activeSecretProvided) &&
    (!codexOAuthConfig || codexOAuthAuthorized) &&
    !saving;
  $: canContinue =
    currentStep === 0
      ? selectedToolInstalled
      : currentStep === 1
        ? profileName.trim().length > 0 &&
          activeProvider.trim().length > 0 &&
          isProtocolAllowedForToolMode(selectedTool, profileMode, activeProtocol) &&
          (!providerNeedsBaseUrl(activeProvider) || baseUrlErrorKey === null) &&
          (!providerRequiresApiKey(activeProvider) || activeSecretProvided) &&
          (!codexOAuthConfig || codexOAuthAuthorized)
        : true;

  function prefillKey(value: WizardPrefill) {
    return `${value.toolId ?? ""}:${value.toolName ?? ""}:${value.mode ?? "config"}`;
  }

  function setProfileMode(nextMode: ProviderApplyMode) {
    profileMode = nextMode;
    if (nextMode !== "config") {
      codexOAuthConfig = false;
    }
    writePreview = null;
    writePreviewKey = null;
    previewError = null;
  }

  function resetDraftState() {
    currentStep = 0;
    saveError = null;
    savedProfileName = null;
    previewError = null;
    writePreview = null;
    writePreviewKey = null;
  }

  function applyToolDefaults(toolId: string, fallbackName?: string, mode: ProviderApplyMode = "config") {
    const canonicalToolId = canonicalProfileToolId(toolId);
    const defaults = toolDefaults.find((tool) => tool.id === canonicalToolId);
    selectedTool = defaults?.id ?? canonicalToolId;
    provider = "compatible";
    setProfileMode(mode);
    protocol = defaults?.protocol ?? "openai-chat-completions";
    profileName = defaults?.profileNameKey
      ? $t(defaults.profileNameKey)
      : $t("wizard.defaultProfile.generic", { name: fallbackName ?? toolId });
    profileRemark = "";
    apiKey = "";
    baseUrl = defaults?.baseUrl ?? "";
    model = defaults?.model ?? "";
    codexOAuthConfig = false;
    codexAuthError = null;
    codexAuthMessage = null;
    resetDraftState();
  }

  function selectCodexOAuthConfig(nextValue: boolean) {
    codexOAuthConfig = nextValue;
    if (nextValue) {
      apiKey = "";
      baseUrl = "";
      model = "";
      protocol = "openai-responses";
    } else {
      provider = "compatible";
    }
    writePreview = null;
    writePreviewKey = null;
    previewError = null;
    saveError = null;
  }

  function selectedToolLabel(toolId: string) {
    const canonicalToolId = canonicalProfileToolId(toolId);
    return toolDefaults.find((tool) => tool.id === canonicalToolId)?.label ?? canonicalToolId;
  }

  function codexAuthIsOAuth(auth: DetectionSnapshot["codexAuth"] | null | undefined) {
    return Boolean(auth?.available) && (auth?.method === "chat_gpt" || auth?.method === "access_token");
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

  function isProtocolAllowedForToolMode(toolId: string, mode: ProviderApplyMode, value: string) {
    return protocolOptionAvailable(protocolOptionsFor(toolId, mode), value);
  }

  function providerRequiresApiKey(providerId: string) {
    return providerId.trim() !== "official";
  }

  function providerNeedsBaseUrl(providerId: string) {
    return providerId.trim() !== "official";
  }

  function buildProfileDraftRequest(): SaveProfileDraftRequest {
    return {
      name: profileName,
      icon: null,
      remark: profileRemark,
      app: selectedTool,
      mode: profileMode,
      provider: activeProvider,
      protocol: activeProtocol,
      model: activeModel,
      baseUrl: normalizeBaseUrl(activeBaseUrl),
      secretProvided: activeSecretProvided,
      apiKey: activeApiKey
    };
  }

  async function handleApply() {
    if (!canApply) {
      saveError = applyBlockedMessage();
      return;
    }

    saving = true;
    saveError = null;
    savedProfileName = null;

    try {
      const profile = await saveProfileDraft(buildProfileDraftRequest());
      savedProfileName = profile.name;
      await onProfileSaved(profile.mode);
    } catch (err) {
      saveError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      saving = false;
    }
  }

  function handlePrimaryAction() {
    if (currentStep === steps.length - 1) {
      void handleApply();
      return;
    }

    if (!canContinue) {
      saveError = applyBlockedMessage();
      return;
    }
    saveError = null;
    currentStep += 1;
  }

  async function refreshWritePreview(expectedKey = previewRequestKey) {
    previewing = true;
    previewError = null;
    writePreviewKey = expectedKey;

    try {
      const nextPreview = await previewProfileWrite(buildProfileDraftRequest());
      if (writePreviewKey === expectedKey) {
        writePreview = nextPreview;
      }
    } catch (err) {
      if (writePreviewKey === expectedKey) {
        writePreview = null;
        previewError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (writePreviewKey === expectedKey) {
        previewing = false;
      }
    }
  }

  async function startCodexAuthorization() {
    codexAuthChecking = true;
    codexAuthError = null;
    codexAuthMessage = null;
    try {
      const result = await startCodexOAuthLogin();
      codexAuthMessage = result.message || $t("wizard.codexOAuth.loginStarted");
    } catch (err) {
      codexAuthError = errorLabel(err instanceof Error ? err.message : String(err));
      try {
        await openExternalUrl(CODEX_AUTH_URL);
        codexAuthMessage = $t("wizard.codexOAuth.loginFallbackOpened");
      } catch (openErr) {
        codexAuthError = errorLabel(openErr instanceof Error ? openErr.message : String(openErr));
      }
    } finally {
      codexAuthChecking = false;
    }
  }

  async function refreshCodexAuthStatus() {
    codexAuthChecking = true;
    codexAuthError = null;
    codexAuthMessage = null;
    try {
      const nextSnapshot = await detectEnvironment();
      localCodexAuth = nextSnapshot.codexAuth;
      codexAuthMessage = codexAuthIsOAuth(nextSnapshot.codexAuth)
        ? $t("wizard.codexOAuth.authDetected")
        : $t("wizard.codexOAuth.authStillMissing");
    } catch (err) {
      codexAuthError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      codexAuthChecking = false;
    }
  }

  function actionLabel(action: string) {
    if (action === "create") {
      return $t("common.create");
    }
    if (action === "create_or_update") {
      return $t("common.save");
    }
    if (action === "not_modified") {
      return $t("common.noWrite");
    }
    if (action === "mode_dependent") {
      return $t("profiles.providerModeTitle");
    }
    if (action === "future_confirmation_required") {
      return $t("common.future");
    }
    if (action === "pending_keychain") {
      return $t("common.keychainLater");
    }
    if (action === "missing") {
      return $t("common.missing");
    }
    return action.replaceAll("_", " ");
  }

  function installStateLabel(state: InstallState) {
    return $t(`status.${state}` as TranslationKey);
  }

  function providerLabel(providerId: string) {
    return providerId === "official" ? $t("wizard.provider.official") : $t("wizard.provider.compatible");
  }

  function applyModeLabel(mode: ProviderApplyMode) {
    return mode === "config" ? $t("profiles.mode.config") : $t("profiles.mode.gateway");
  }

  function protocolLabel(value: string) {
    const normalized = normalizeProtocol(value);
    const option = protocolOptions.find((item) => item.id === normalized);
    return option ? $t(option.labelKey) : value;
  }

  function normalizeProtocol(value: string) {
    return value.trim();
  }

  function credentialDetailLabel(providerId: string, secretProvided: boolean) {
    if (providerId.trim() === "official") {
      return $t("wizard.check.officialLoginNoKey");
    }
    if (secretProvided) {
      return $t("wizard.check.credentialReady");
    }
    return $t("wizard.check.credentialMissing");
  }

  function writePreviewLabel(item: ProfileWritePreviewItem) {
    if (item.label === "Profile row") {
      return $t("wizard.preview.profileRow");
    }
    if (item.label === "Active tool profile pointer") {
      return $t("wizard.preview.activeProfilePointer");
    }
    if (item.label === "Credential") {
      return $t("wizard.preview.credential");
    }
    if (item.label.endsWith(" config")) {
      return $t("wizard.preview.toolConfig", { name: item.label.replace(/\sconfig$/, "") });
    }
    return item.label;
  }

  function writePreviewDetail(item: ProfileWritePreviewItem) {
    if (item.label === "Profile row") {
      return $t("wizard.preview.profileRowDetail", {
        protocol: protocolLabel(activeProtocol),
        provider: providerLabel(activeProvider)
      });
    }
    if (item.label === "Active tool profile pointer") {
      return $t("wizard.preview.activeProfilePointerDetail");
    }
    if (item.label === "Credential") {
      return credentialDetailLabel(activeProvider, activeSecretProvided);
    }
    if (item.label.endsWith(" config")) {
      return $t("wizard.preview.toolConfigDetail");
    }
    return item.detail;
  }

  function warningLabel(warning: string) {
    const duplicateMatch = warning.match(/Profile id '([^']+)' already exists, so this draft will use '([^']+)'/);
    if (duplicateMatch) {
      return $t("wizard.warning.profileIdExists", {
        oldId: duplicateMatch[1],
        newId: duplicateMatch[2]
      });
    }
    const missingToolMatch = warning.match(/Tool '([^']+)' is not in (?:the preview|the local) registry/);
    if (missingToolMatch) {
      return $t("wizard.warning.toolNotInRegistry", { tool: missingToolMatch[1] });
    }
    return warning;
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
    const configProtocolMatch = message.match(/Config (?:file mode does|profiles do) not support (.+) for '([^']+)'\./);
    if (configProtocolMatch) {
      return $t("wizard.error.configProtocolUnsupported", {
        protocol: configProtocolMatch[1],
        tool: selectedToolLabel(configProtocolMatch[2])
      });
    }
    if (message === "Provider API key is required for non-official providers.") {
      return $t("wizard.check.credentialMissing");
    }
    if (message === "Official profiles are built in and cannot be saved as custom profiles.") {
      return $t("profiles.officialCustomSaveBlocked");
    }
    if (
      message === "Official provider uses the client login directly and cannot use Gateway mode." ||
      message === "Official provider uses the client login directly and cannot use Gateway profile." ||
      message === "Official provider uses the client login directly and cannot use Gateway profiles."
    ) {
      return $t("profiles.officialGatewayBlocked");
    }
    const toolNotInstalledMatch = message.match(/Tool '([^']+)' is not installed, so a profile cannot be created for it\./);
    if (toolNotInstalledMatch) {
      return $t("wizard.error.toolNotInstalled", {
        tool: selectedToolLabel(toolNotInstalledMatch[1])
      });
    }
    return message;
  }

  function toolStatusForProfileTool(toolId: string) {
    const canonicalToolId = canonicalProfileToolId(toolId);
    const matches = snapshot?.tools.filter((tool) => canonicalProfileToolId(tool.id) === canonicalToolId) ?? [];
    return matches.find((tool) => tool.installState === "installed") ?? matches[0] ?? null;
  }

  function toolVisibleInSnapshot(toolId: string) {
    if (!snapshot || canonicalProfileToolId(toolId) !== "gemini-code-assist") {
      return true;
    }
    return snapshot.tools.some((tool) => canonicalProfileToolId(tool.id) === "gemini-code-assist");
  }

  function toolCanCreateProfile(toolId: string) {
    return toolStatusForProfileTool(toolId)?.installState === "installed";
  }

  function normalizeBaseUrl(value: string) {
    const trimmed = value.trim();
    if (/^https?:/i.test(trimmed) && !/^https?:\/\//i.test(trimmed)) {
      return trimmed;
    }
    if (!trimmed || /^[a-z][a-z\d+\-.]*:\/\//i.test(trimmed)) {
      return trimmed;
    }
    return `https://${trimmed}`;
  }

  function shouldAutoPrefixBaseUrlInput(value: string) {
    const trimmed = value.trim();
    if (!trimmed || /^[a-z][a-z\d+\-.]*:\/\//i.test(trimmed)) {
      return false;
    }
    if (/^[a-z][a-z\d+\-.]*:\/?$/i.test(trimmed)) {
      return false;
    }
    return trimmed.includes(".") || trimmed.includes(":") || trimmed.toLowerCase() === "localhost";
  }

  function handleBaseUrlInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    baseUrl = shouldAutoPrefixBaseUrlInput(value) ? normalizeBaseUrl(value) : value;
  }

  function normalizeBaseUrlInput() {
    baseUrl = normalizeBaseUrl(baseUrl);
  }

  function baseUrlValidationErrorKey(value: string): TranslationKey | null {
    const input = value.trim();
    const trimmed = normalizeBaseUrl(input);
    if (!input) {
      return "wizard.error.baseUrlRequired";
    }
    if (/\s/.test(input)) {
      return "wizard.error.baseUrlWhitespace";
    }
    if (!/^https?:\/\//i.test(trimmed)) {
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

  function applyBlockedMessage() {
    if (!selectedToolInstalled) {
      return snapshot
        ? $t("wizard.error.toolNotInstalled", { tool: selectedToolLabel(selectedTool) })
        : $t("wizard.error.toolDetectionPending");
    }
    if (baseUrlErrorKey) {
      return $t(baseUrlErrorKey);
    }
    if (codexOAuthConfig && !codexOAuthAuthorized) {
      return $t("wizard.codexOAuth.authorizationRequired");
    }
    return $t("wizard.applyRequired");
  }
</script>

<div class={routeStackRecipe({ width: "full" })}>
  <section class={topStripRecipe()}>
    <div>
      <h1>{$t(steps[currentStep])}</h1>
      <p>{$t("wizard.progress", { current: currentStep + 1, total: steps.length })}</p>
    </div>
    <div class={wizardActionsRecipe()}>
      <button class={actionButtonRecipe()} title={$t("common.back")} disabled={currentStep === 0} on:click={() => (currentStep -= 1)}>
        <AppIcon name="arrowLeft" size={16} />
        {$t("common.back")}
      </button>
      <button
        class={actionButtonRecipe({ tone: "primary" })}
        title={$t(currentStep === steps.length - 1 ? "common.save" : "common.next")}
        disabled={currentStep === steps.length - 1 ? !canApply || previewing : !canContinue}
        on:click={handlePrimaryAction}
      >
        {#if currentStep === steps.length - 1}
          {saving ? $t("common.saving") : $t("common.save")}
          <AppIcon name="check" size={16} />
        {:else}
          {$t("common.next")}
          <AppIcon name="arrowRight" size={16} />
        {/if}
      </button>
    </div>
  </section>

  <div class={wizardStepperRecipe()}>
    {#each steps as step, index}
      <div
        class={wizardStepItemRecipe()}
        data-step-state={index === currentStep ? "active" : index < currentStep ? "done" : "idle"}
        title={$t(step)}
        aria-current={index === currentStep ? "step" : undefined}
      >
        {#if index < currentStep}
          <AppIcon name="check" size={14} />
        {:else}
          <span>{index + 1}</span>
        {/if}
      </div>
    {/each}
  </div>

  <section class={wizardPanelRecipe()}>
    {#key currentStep}
    <div class={wizardStepContentRecipe()} in:fly={wizardStepEnter} out:fade={wizardStepExit}>
    {#if currentStep === 0}
      <div class={wizardPreviewHeadingRecipe()}>
        <div>
          <h2>{$t("wizard.chooseClientTitle")}</h2>
          <p>{$t("wizard.chooseClientDescription")}</p>
        </div>
      </div>
      <div class={wizardChoiceGridRecipe({ kind: "tool" })}>
        {#each visibleToolDefaults as tool}
          {@const toolStatus = toolStatusForProfileTool(tool.id)}
          {@const installed = toolCanCreateProfile(tool.id)}
          <button
            class={wizardChoiceButtonRecipe({ kind: "tool" })}
            data-selected={selectedTool === tool.id}
            disabled={!installed}
            title={installed ? tool.label : $t("wizard.error.toolNotInstalled", { tool: tool.label })}
            on:click={() => applyToolDefaults(tool.id, undefined, profileMode)}
          >
            <ToolIcon toolId={tool.id} label={tool.label} variant="choice" />
            <span>{tool.label}</span>
            <small>{toolStatus ? installStateLabel(toolStatus.installState) : $t("status.unknown")}</small>
          </button>
        {/each}
      </div>
      <div class={wizardModeChoiceRecipe()}>
        <strong>{$t("profiles.providerModeTitle")}</strong>
        <div class={wizardChoiceGridRecipe({ kind: "compact" })}>
          <button
            class={wizardChoiceButtonRecipe({ kind: "compact" })}
            data-selected={profileMode === "config"}
            type="button"
            on:click={() => setProfileMode("config")}
          >
            <AppIcon name="profiles" size={18} />
            <span>{$t("profiles.mode.config")}</span>
          </button>
          <button
            class={wizardChoiceButtonRecipe({ kind: "compact" })}
            data-selected={profileMode === "gateway"}
            type="button"
            on:click={() => setProfileMode("gateway")}
          >
            <AppIcon name="gateway" size={18} />
            <span>{$t("profiles.mode.gateway")}</span>
          </button>
        </div>
      </div>
      {#if !selectedToolInstalled}
        <div class={wizardInlineNoticeRecipe({ tone: "error" })}>{applyBlockedMessage()}</div>
      {/if}
    {:else if currentStep === 1}
      <div class={wizardPreviewHeadingRecipe()}>
        <div>
          <h2>{$t("wizard.connectionTitle")}</h2>
          <p>{$t("wizard.connectionDescription", { mode: applyModeLabel(profileMode) })}</p>
        </div>
      </div>

      {#if canUseCodexOAuthConfig}
        <div class={wizardChoiceGridRecipe({ kind: "compact" })}>
          <button
            class={wizardChoiceButtonRecipe({ kind: "compact" })}
            data-selected={!codexOAuthConfig}
            type="button"
            on:click={() => selectCodexOAuthConfig(false)}
          >
            <AppIcon name="key" size={18} />
            <span>{$t("wizard.codexOAuth.typeApi")}</span>
          </button>
          <button
            class={wizardChoiceButtonRecipe({ kind: "compact" })}
            data-selected={codexOAuthConfig}
            type="button"
            on:click={() => selectCodexOAuthConfig(true)}
          >
            <AppIcon name="user" size={18} />
            <span>{$t("wizard.codexOAuth.typeOAuth")}</span>
          </button>
        </div>
      {/if}

      <div class={wizardFormGridRecipe()}>
        <label>
          {$t("wizard.profileName")}
          <input bind:value={profileName} />
        </label>
        <label class={wizardWideFieldRecipe()}>
          {$t("profiles.remarkLabel")}
          <textarea bind:value={profileRemark} rows="2" placeholder={$t("profiles.remarkPlaceholder")}></textarea>
        </label>
        {#if !codexOAuthConfig}
          <label>
            {$t("wizard.protocol")}
            <select bind:value={protocol}>
              {#each availableProtocolOptions as option}
                <option value={option.id}>{$t(option.labelKey)}</option>
              {/each}
            </select>
          </label>
          <label>
            {$t("wizard.providerApiKey")}
            <SecretInput bind:value={apiKey} />
          </label>
          <label>
            {$t("wizard.providerBaseUrl")}
            <input value={baseUrl} on:input={handleBaseUrlInput} on:blur={normalizeBaseUrlInput} />
            {#if visibleBaseUrlErrorKey}
              <small class={wizardFieldErrorRecipe()}>{$t(visibleBaseUrlErrorKey)}</small>
            {/if}
          </label>
          <label>
            {$t("wizard.modelOptional")}
            <input bind:value={model} />
          </label>
        {/if}
      </div>
      {#if codexOAuthConfig}
        <div class={wizardCodexAuthCardRecipe()}>
          <div>
            <strong>{$t("wizard.codexOAuth.authTitle")}</strong>
            <span>
              {#if codexOAuthAuthorized}
                {$t("wizard.codexOAuth.authReady")}
              {:else if localCodexAuth?.available && localCodexAuth?.method === "api_key"}
                {$t("wizard.codexOAuth.apiKeyNotOAuth")}
              {:else}
                {$t("wizard.codexOAuth.authRequired")}
              {/if}
            </span>
            {#if localCodexAuth?.path}
              <small>{localCodexAuth.path}</small>
            {/if}
          </div>
          <div class={wizardButtonRowRecipe()}>
            <button class={actionButtonRecipe()} type="button" disabled={codexAuthChecking} on:click={startCodexAuthorization}>
              {#if codexAuthChecking}
                <AppIcon name="loading" class={spinRecipe()} size={16} />
                {$t("common.loading")}
              {:else}
                <AppIcon name="externalLink" size={16} />
                {$t("wizard.codexOAuth.openLogin")}
              {/if}
            </button>
            <button class={actionButtonRecipe()} type="button" disabled={codexAuthChecking} on:click={refreshCodexAuthStatus}>
              <AppIcon name={codexAuthChecking ? "loading" : "refresh"} class={codexAuthChecking ? spinRecipe() : ""} size={16} />
              {$t("wizard.codexOAuth.recheck")}
            </button>
          </div>
        </div>
        {#if codexAuthError}
          <div class={wizardInlineNoticeRecipe({ tone: "error" })}>{codexAuthError}</div>
        {/if}
        {#if codexAuthMessage}
          <div class={wizardInlineNoticeRecipe({ tone: "success" })}>{codexAuthMessage}</div>
        {/if}
      {:else}
        <div class={wizardSecurityNoteRecipe()}>
          <AppIcon name="key" size={18} />
          {$t("wizard.securityNote")}
        </div>
      {/if}
    {:else if currentStep === 2}
      <div class={wizardPreviewBoxRecipe()}>
        <div class={wizardPreviewHeadingRecipe()}>
          <div>
            <h2>{$t("wizard.writePreview")}</h2>
            {#if writePreview}
              <p>{writePreview.profileId} / {new Date(writePreview.generatedAt).toLocaleString()}</p>
            {:else}
              <p>{previewing ? $t("wizard.buildingPreview") : $t("wizard.reviewBeforeSaving")}</p>
            {/if}
          </div>
        </div>

        {#if previewError}
          <div class={wizardInlineNoticeRecipe({ tone: "error" })}>{errorLabel(previewError)}</div>
        {/if}
        {#if saveError}
          <div class={wizardInlineNoticeRecipe({ tone: "error" })}>{saveError}</div>
        {/if}
        {#if savedProfileName}
          <div class={wizardInlineNoticeRecipe({ tone: "success" })}>{$t("wizard.savedProfile", { name: savedProfileName })}</div>
        {/if}

        {#if writePreview}
          <div class={wizardWritePreviewListRecipe()}>
            {#each writePreview.items as item}
              <div class={wizardWritePreviewRowRecipe()}>
                <strong>{writePreviewLabel(item)}</strong>
                <span class={wizardWritePreviewMetaRecipe()}>
                  <b>{actionLabel(item.action)}</b>
                </span>
                {#if item.path}
                  <code>{item.path}</code>
                {/if}
                <span>{writePreviewDetail(item)}</span>
                {#if item.content}
                  <div class={wizardWriteContentPreviewRecipe()}>
                    <strong>{$t("wizard.writeContentPreview")}</strong>
                    <pre>{item.content}</pre>
                  </div>
                {/if}
              </div>
            {/each}
          </div>

          {#if writePreview.warnings.length > 0}
            <div class={wizardPreviewWarningsRecipe()}>
              {#each writePreview.warnings as warning}
                <span>{warningLabel(warning)}</span>
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    {/if}
    </div>
    {/key}
  </section>
</div>
