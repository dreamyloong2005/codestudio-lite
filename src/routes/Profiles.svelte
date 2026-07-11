<script lang="ts">
  import { flip } from "svelte/animate";
  import { onDestroy } from "svelte";
  import {
    dragHandle,
    dragHandleZone,
    SHADOW_ITEM_MARKER_PROPERTY_NAME,
    SHADOW_PLACEHOLDER_ITEM_ID,
    TRIGGERS,
    type DndEvent
  } from "svelte-dnd-action";
  import {
    applyProfile,
    clearClaudeEnvironmentVariables,
    deleteProfileDraft,
    deleteUsageScript,
    duplicateProfileDraft,
    listProfileModels,
    loadUsageScriptState,
    previewProfileApply,
    queryProfileUsage,
    reorderProfileDrafts,
    saveUsageScript,
    testUsageScript,
    updateProfileDraft
  } from "../lib/api";
  import { t, type TranslationKey } from "../lib/i18n";
  import AppIcon from "../components/AppIcon.svelte";
  import DismissibleNotice from "../components/DismissibleNotice.svelte";
  import ModelSelectInput from "../components/ModelSelectInput.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import { css, cx } from "../../styled-system/css";
  import {
    actionButtonRecipe,
    desktopClientModalActionsRecipe,
    desktopClientModalBackdropRecipe,
    desktopClientModalBodyRecipe,
    desktopClientModalPanelRecipe,
    desktopClientPreviewListRecipe,
    emptyRowRecipe,
    iconButtonRecipe,
    nativeToggleRecipe,
    profileAvatarRecipe,
    profileCardActionsRecipe,
    profileCardMainRecipe,
    profileCardRecipe,
    profileCardStatusRecipe,
    profileDiffHeadingRecipe,
    profileDiffListRecipe,
    profileDiffPanelRecipe,
    profileDiffRowRecipe,
    profileDragHandleRecipe,
    profileEmbeddedStackRecipe,
    profileFieldErrorRecipe,
    profileFormGridRecipe,
    profileGridRecipe,
    profileIdentityRecipe,
    profileIconActionsRecipe,
    profileIconEditorRecipe,
    profileInlineNoticeRecipe,
    profileModeLayoutRecipe,
    profileModeSwitcherRecipe,
    profileSortableRowRecipe,
    profileToolSectionRecipe,
    profileToolSwitcherRecipe,
    profileToolTabsRecipe,
    profileUsageCodeFieldRecipe,
    profileUsageResultCardRecipe,
    profileUsageResultGridRecipe,
    profileUsageTemplateRowRecipe,
    profileUsageOfficialPanelRecipe,
    profileWriteContentPreviewRecipe,
    routeStackRecipe,
    sectionActionsRecipe,
    spinRecipe,
    topActionsRecipe,
    topStripRecipe,
    panelRecipe
  } from "../../styled-system/recipes";
  import {
    nextSortableProfileIds,
    profileDragDisabled as resolveProfileDragDisabled,
    profileIdsFromItems
  } from "../lib/profileSortable";
  import type {
    ApplyProfileResult,
    DetectionSnapshot,
    PreviewProfileApplyResult,
    ProfileDraft,
    ProfileModelMapping,
    ProfileModelOption,
    ProfileSummary,
    ProviderApplyMode,
    UsageData,
    UsageQueryResult,
    WizardPrefill,
    UsageScriptSaveRequest,
    UsageScriptState,
    UsageScriptTemplateType
  } from "../types";

  export let summary: ProfileSummary | null = null;
  export let snapshot: DetectionSnapshot | null = null;
  export let modeFilter: ProviderApplyMode = "config";
  export let embedded = false;
  export let onProfileSwitched: () => void | Promise<void> = () => {};
  export let onCreateProfile: (prefill?: WizardPrefill) => void = () => {};

  const profileViewOptions: Array<{ value: ProviderApplyMode; labelKey: TranslationKey }> = [
    { value: "config", labelKey: "profiles.view.config" },
    { value: "gateway", labelKey: "profiles.view.gateway" }
  ];

  type ProfileGroup = {
    id: string;
    label: string;
    activeProfileId: string | null;
    activeProfileName: string | null;
    profiles: ProfileDraft[];
  };

  type ProfileModeSection = {
    mode: ProviderApplyMode;
    groups: ProfileGroup[];
  };

  type EditProfileForm = {
    name: string;
    icon: string;
    remark: string;
    mode: ProviderApplyMode;
    provider: string;
    protocol: string;
    model: string;
    modelMappings: ProfileModelMappingForm[];
    baseUrl: string;
    apiKey: string;
  };

  type ProfileModelMappingForm = {
    alias: string;
    model: string;
    supports1m: boolean;
    description: string;
  };

  type UsageForm = {
    enabled: boolean;
    templateType: UsageScriptTemplateType;
    code: string;
    apiKey: string;
    baseUrl: string;
    accessToken: string;
    userId: string;
    timeoutSeconds: number;
    autoQueryIntervalMinutes: number;
  };

  type ProfileUsageEntry = {
    result: UsageQueryResult | null;
    state: "idle" | "loading" | "querying";
    configured: boolean;
    error: string | null;
    updatedAt: string | null;
  };

  let pendingEdit: ProfileDraft | null = null;
  let editForm: EditProfileForm = emptyEditForm();
  let editingId: string | null = null;
  let editModelOptions: ProfileModelOption[] = [];
  let editModelLoading = false;
  let editModelError: string | null = null;
  let editModelLoadedKey = "";
  let applyingId: string | null = null;
  let duplicatingId: string | null = null;
  let deletingId: string | null = null;
  let pendingDelete: ProfileDraft | null = null;
  let pendingApply: ProfileDraft | null = null;
  let applyPreview: PreviewProfileApplyResult | null = null;
  let applyResult: ApplyProfileResult | null = null;
  let selectedApplyMode: ProviderApplyMode = "gateway";
  let pendingApplyMode: ProviderApplyMode = "gateway";
  let editError: string | null = null;
  let applyError: string | null = null;
  let clearingEnvConflict = false;
  let profileIoError: string | null = null;
  let profileIoMessage: string | null = null;
  let syncClaudeVsCodePlugin = false;
  let pendingUsageProfile: ProfileDraft | null = null;
  let usageState: UsageScriptState | null = null;
  let usageForm: UsageForm = emptyUsageForm();
  let usageError: string | null = null;
  let usageMessage: string | null = null;
  let usageBusy: "load" | "save" | "test" | "query" | "delete" | null = null;
  let usageResult: UsageQueryResult | null = null;
  let usageAutoQueryTimer: ReturnType<typeof window.setInterval> | null = null;
  let usageAutoQueryKey = "";
  let profileUsageEntries: Record<string, ProfileUsageEntry> = {};
  let selectedToolId: string | null = null;
  let profileIconInput: HTMLInputElement | null = null;
  let sortableProfiles: ProfileDraft[] = [];
  let sortableSourceProfiles: ProfileDraft[] = [];
  let sortableListKey = "";
  let sortableActiveId: string | null = null;
  let sortableSaving = false;
  const profileFlipDurationMs = 220;
  const profileDropTargetStyle = { outline: "none" };
  const modalPanelWideClass = css({
    width: "min(760px, calc(100vw - 40px))",
    "@supports (width: 100dvw)": {
      width: "min(760px, calc(100dvw - 40px))"
    }
  });
  const usageModalPanelClass = css({
    width: "min(900px, calc(100vw - 40px))",
    "@supports (width: 100dvw)": {
      width: "min(900px, calc(100dvw - 40px))"
    }
  });
  const dangerButtonClass = css({
    borderColor: "color-mix(in srgb, var(--danger) 40%, transparent)",
    background: "color-mix(in srgb, var(--danger) 14%, transparent)",
    color: "var(--danger-text)",
    _hover: {
      borderColor: "color-mix(in srgb, var(--danger) 55%, transparent)",
      background: "color-mix(in srgb, var(--danger) 18%, transparent)"
    }
  });
  const usageToggleClass = css({
    borderColor: "color-mix(in srgb, var(--accent) 30%, transparent)",
    background: "color-mix(in srgb, var(--accent) 8%, transparent)"
  });
  const inlineEmptyClass = css({
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    gap: "8px"
  });
  const embeddedProfileActionsClass = css({
    justifyContent: "flex-end"
  });
  const conflictPreviewListClass = css({
    "& code": {
      width: "100%"
    }
  });
  const modelPickerClass = css({
    display: "grid",
    gap: "6px",
    color: "var(--text-soft)",
    fontSize: "13px",
    fontWeight: 800,
    minWidth: 0
  });
  const modelPickerRowClass = css({
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr) auto",
    alignItems: "center",
    gap: "8px",
    minWidth: 0,
    "& > button": {
      height: "38px"
    },
    "@media (max-width: 860px)": {
      gridTemplateColumns: "1fr",
      alignItems: "stretch"
    }
  });
  const modelPickerStatusClass = css({
    color: "var(--text-muted)",
    fontSize: "12px",
    fontWeight: 700,
    lineHeight: 1.35,
    overflowWrap: "anywhere"
  });
  const modelMappingPanelClass = css({
    display: "grid",
    gap: "10px",
    gridColumn: "1 / -1",
    padding: "12px",
    border: "1px solid var(--border)",
    borderRadius: "8px",
    background: "var(--surface-muted)",
    minWidth: 0
  });
  const modelMappingHeaderClass = css({
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "10px",
    minWidth: 0,
    "& strong": {
      color: "var(--text)",
      fontSize: "13px",
      fontWeight: 800
    }
  });
  const modelMappingRowsClass = css({
    display: "grid",
    gap: "8px",
    minWidth: 0
  });
  const modelMappingRowClass = css({
    display: "grid",
    gridTemplateColumns: "minmax(120px, 1fr) minmax(120px, 1fr) minmax(120px, 1fr) auto auto",
    gap: "8px",
    alignItems: "end",
    minWidth: 0,
    "& label": {
      minWidth: 0
    },
    "@media (max-width: 980px)": {
      gridTemplateColumns: "1fr"
    }
  });
  const modelMappingToggleClass = css({
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    color: "var(--text-soft)",
    fontSize: "13px",
    fontWeight: 800,
    minHeight: "38px"
  });
  const modelMappingFieldClass = css({
    display: "grid",
    gap: "6px",
    minWidth: 0,
    color: "var(--text-soft)",
    fontSize: "13px",
    fontWeight: 800
  });

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
  const officialProfileNameKeys: Record<string, TranslationKey> = {
    codex: "profiles.officialProfile.codex",
    "claude-desktop": "profiles.officialProfile.claudeDesktop",
    claude: "profiles.officialProfile.claude",
    gemini: "profiles.officialProfile.gemini",
    "gemini-code-assist": "profiles.officialProfile.geminiCodeAssist",
    opencode: "profiles.officialProfile.opencode",
    openclaw: "profiles.officialProfile.openclaw",
    hermes: "profiles.officialProfile.hermes"
  };

  const protocolOptions = [
    { id: "openai-chat-completions", labelKey: "wizard.protocol.openaiChatCompletions" },
    { id: "openai-responses", labelKey: "wizard.protocol.openaiResponses" },
    { id: "anthropic-messages", labelKey: "wizard.protocol.anthropicMessages" },
    { id: "google-gemini", labelKey: "wizard.protocol.googleGemini" }
  ] as const;
  type ProtocolOption = (typeof protocolOptions)[number];
  const usageTemplateOptions: Array<{ id: UsageScriptTemplateType; labelKey: TranslationKey }> = [
    { id: "general", labelKey: "profiles.usage.template.general" },
    { id: "newapi", labelKey: "profiles.usage.template.newapi" },
    { id: "balance", labelKey: "profiles.usage.template.balance" },
    { id: "token_plan", labelKey: "profiles.usage.template.tokenPlan" },
    { id: "custom", labelKey: "profiles.usage.template.custom" }
  ];
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

  $: installedProfileToolIds = buildInstalledProfileToolIds(snapshot);
  $: normalizedModeFilter = (modeFilter === "gateway" ? "gateway" : "config") as ProviderApplyMode;
  $: profileModeSections = buildProfileModeSections(summary, installedProfileToolIds, normalizedModeFilter);
  $: profileToolGroups = profileModeSections.flatMap((section) => section.groups);
  $: syncSelectedProfileTool(profileToolGroups);
  $: selectedProfileGroup = profileToolGroups.find((group) => group.id === selectedToolId) ?? null;
  $: syncSortableProfiles(selectedProfileGroup, normalizedModeFilter);
  $: displayedProfiles = sortableProfiles;
  $: visibleProfileCount = selectedProfileGroup?.profiles.length ?? 0;
  $: selectedModePreview =
    applyPreview?.modePreviews.find((mode) => mode.mode === selectedApplyMode) ?? null;
  $: selectedNativeDiff = selectedModePreview?.nativeDiff ?? null;
  $: selectedNativeDiffVisible = Boolean(
    selectedNativeDiff?.writeEnabled && selectedNativeDiff.changes.length > 0
  );
  $: selectedModeSupported = selectedModePreview?.supported ?? false;
  $: applyEnvConflicts = applyResult?.envConflicts ?? applyPreview?.envConflicts ?? [];
  $: canSyncClaudeVsCodePlugin =
    Boolean(pendingApply) &&
    canonicalProfileToolId(pendingApply?.app ?? "") === "claude" &&
    selectedApplyMode === "config" &&
    Boolean(selectedModePreview?.writesNativeConfig);
  $: if (!canSyncClaudeVsCodePlugin && syncClaudeVsCodePlugin) {
    syncClaudeVsCodePlugin = false;
  }
  $: editIconTooLong = profileIconTextTooLong(editForm.icon);
  $: editBaseUrlErrorKey = providerNeedsBaseUrl(editForm.provider)
    ? baseUrlValidationErrorKey(editForm.baseUrl)
    : null;
  $: availableEditProtocolOptions = pendingEdit
    ? protocolOptionsFor(pendingEdit.app, editForm.mode)
    : protocolOptions;
  $: editSupportsModelMappings = Boolean(pendingEdit) && profileSupportsModelMappings(pendingEdit?.app ?? "");
  $: editModelMappingsValid =
    !editSupportsModelMappings || profileModelMappingsAreValid(editForm.modelMappings);
  $: editModelListId = pendingEdit
    ? `edit-model-options-${domSafeId(pendingEdit.id)}`
    : "edit-model-options";
  $: editModelRequestKey = pendingEdit
    ? profileModelRequestKey({
        profileId: pendingEdit.id,
        app: pendingEdit.app,
        mode: editForm.mode,
        provider: editForm.provider,
        protocol: editForm.protocol,
        baseUrl: editForm.baseUrl,
        apiKey: editForm.apiKey
      })
    : "";
  $: if (editModelLoadedKey && editModelLoadedKey !== editModelRequestKey) {
    editModelOptions = [];
    editModelError = null;
    editModelLoadedKey = "";
  }
  $: pendingUsageIsCodexOfficialOAuth = pendingUsageProfile
    ? profileUsesCodexOfficialOAuth(pendingUsageProfile)
    : false;
  $: canSaveEdit =
    Boolean(pendingEdit) &&
    editForm.name.trim().length > 0 &&
    !editIconTooLong &&
    editForm.provider.trim().length > 0 &&
    (!providerIsOfficial(editForm.provider) || editableOfficialProfileAllowed(pendingEdit, editForm.mode)) &&
    isProtocolAllowedForToolMode(pendingEdit?.app ?? "", editForm.mode, editForm.protocol) &&
    (!providerNeedsBaseUrl(editForm.provider) || editBaseUrlErrorKey === null) &&
    (!providerRequiresApiKey(editForm.provider) || Boolean(pendingEdit?.authRef) || editForm.apiKey.trim().length > 0) &&
    editModelMappingsValid &&
    !pendingEdit?.isBuiltin &&
    editingId === null;
  $: canFetchEditModels =
    Boolean(pendingEdit) &&
    !providerIsOfficial(editForm.provider) &&
    editForm.provider.trim().length > 0 &&
    isProtocolAllowedForToolMode(pendingEdit?.app ?? "", editForm.mode, editForm.protocol) &&
    (!providerNeedsBaseUrl(editForm.provider) || editBaseUrlErrorKey === null) &&
    (!providerRequiresApiKey(editForm.provider) ||
      editForm.apiKey.trim().length > 0 ||
      (pendingEdit?.provider === editForm.provider && Boolean(pendingEdit?.authRef))) &&
    editingId === null &&
    !editModelLoading;
  $: editModelFetchDisabled =
    !pendingEdit || providerIsOfficial(editForm.provider) || editingId !== null || editModelLoading;
  $: editModelStatus = editModelLoading
    ? $t("profiles.fetchingModels")
    : editModelError
      ? editModelError
      : editModelOptions.length > 0
        ? $t("profiles.modelListLoaded", { count: editModelOptions.length })
        : null;
  $: canSaveUsage =
    Boolean(pendingUsageProfile) &&
    (!usageForm.enabled || pendingUsageIsCodexOfficialOAuth || usageForm.code.trim().length > 0) &&
    usageForm.timeoutSeconds >= 2 &&
    usageForm.timeoutSeconds <= 60 &&
    usageForm.autoQueryIntervalMinutes >= 0 &&
    usageForm.autoQueryIntervalMinutes <= 1440 &&
    usageBusy !== "load" &&
    usageBusy !== "save";
  $: configureUsageAutoQuery(
    pendingUsageProfile?.id ?? "",
    usageState?.config?.enabled ? usageState.config.autoQueryIntervalMinutes : 0
  );

  onDestroy(() => {
    clearUsageAutoQueryTimer();
  });

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
    applyingId = actionKey(profile.id, profile.mode);

    try {
      applyPreview = await previewProfileApply({ profileId: profile.id });
      selectedApplyMode = profile.mode;
    } catch (err) {
      applyError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      applyingId = null;
    }
  }

  function emptyEditForm(): EditProfileForm {
    return {
      name: "",
      icon: "",
      remark: "",
      mode: "config",
      provider: "",
      protocol: "openai-chat-completions",
      model: "",
      modelMappings: [],
      baseUrl: "",
      apiKey: ""
    };
  }

  function emptyUsageForm(): UsageForm {
    return {
      enabled: false,
      templateType: "general",
      code: "",
      apiKey: "",
      baseUrl: "",
      accessToken: "",
      userId: "",
      timeoutSeconds: 10,
      autoQueryIntervalMinutes: 0
    };
  }

  function emptyProfileUsageEntry(profile?: ProfileDraft): ProfileUsageEntry {
    return {
      result: null,
      state: profile?.usageEnabled ? "loading" : "idle",
      configured: Boolean(profile?.usageEnabled),
      error: null,
      updatedAt: null
    };
  }

  function setProfileUsageEntry(profileId: string, entry: ProfileUsageEntry) {
    profileUsageEntries = {
      ...profileUsageEntries,
      [profileId]: entry
    };
  }

  function setProfileUsageResult(profileId: string, result: UsageQueryResult | null) {
    const current = profileUsageEntries[profileId] ?? emptyProfileUsageEntry();
    setProfileUsageEntry(profileId, {
      ...current,
      result,
      state: "idle",
      configured: true,
      error: null,
      updatedAt: result?.queriedAt ?? current.updatedAt
    });
  }

  async function openUsage(profile: ProfileDraft) {
    if (!profileCanOpenUsage(profile) || usageBusy !== null) {
      return;
    }
    pendingUsageProfile = profile;
    usageState = null;
    usageResult = null;
    usageError = null;
    usageMessage = null;
    usageBusy = "load";

    try {
      const state = await loadUsageScriptState(profile.id);
      usageState = state;
      usageResult = state.lastResult;
      setProfileUsageEntry(profile.id, {
        result: state.lastResult,
        state: "idle",
        configured: Boolean(state.config?.enabled),
        error: null,
        updatedAt: state.lastResult?.queriedAt ?? profileUsageEntries[profile.id]?.updatedAt ?? null
      });
      usageForm = usageFormFromState(profile, state);
    } catch (err) {
      usageError = errorLabel(err instanceof Error ? err.message : String(err));
      usageForm = usageFormFromState(profile, null);
    } finally {
      usageBusy = null;
    }
  }

  function closeUsage() {
    if (usageBusy !== null) {
      return;
    }
    pendingUsageProfile = null;
    usageState = null;
    usageResult = null;
    usageError = null;
    usageMessage = null;
    usageForm = emptyUsageForm();
  }

  function usageFormFromState(profile: ProfileDraft, state: UsageScriptState | null): UsageForm {
    const config = state?.config;
    const templateType = config?.templateType ?? "general";
    return {
      enabled: config?.enabled ?? false,
      templateType,
      code: config?.code || state?.defaultCode || "",
      apiKey: "",
      baseUrl: config?.baseUrl ?? profile.baseUrl,
      accessToken: "",
      userId: config?.userId ?? "",
      timeoutSeconds: config?.timeoutSeconds ?? 10,
      autoQueryIntervalMinutes: config?.autoQueryIntervalMinutes ?? 0
    };
  }

  function selectUsageTemplate(templateType: UsageScriptTemplateType) {
    if (usageBusy !== null) {
      return;
    }
    usageForm = {
      ...usageForm,
      templateType,
      code: usageDefaultCode(templateType)
    };
  }

  function usageDefaultCode(templateType: UsageScriptTemplateType) {
    if (usageState?.config?.templateType === templateType && usageState.config.code.trim()) {
      return usageState.config.code;
    }
    if (!usageState?.config && usageState?.defaultCode && templateType === "general") {
      return usageState.defaultCode;
    }
    if (templateType === "newapi") {
      return `({
  request: {
    url: "{{baseUrl}}/api/user/self",
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      "Authorization": "Bearer {{accessToken}}",
      "User-Agent": "codestudio-lite/1.0",
      "New-Api-User": "{{userId}}"
    }
  },
  extractor: function(response) {
    if (response.success && response.data) {
      return {
        planName: response.data.group || "Default",
        remaining: response.data.quota / 500000,
        used: response.data.used_quota / 500000,
        total: (response.data.quota + response.data.used_quota) / 500000,
        unit: "USD"
      };
    }
    return { isValid: false, invalidMessage: response.message || "Query failed" };
  }
})`;
    }
    if (templateType === "balance") {
      return `({
  request: {
    url: "{{baseUrl}}/dashboard/billing/credit_grants",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    var total = response.total_granted || response.total_available || response.balance || 0;
    var used = response.total_used || 0;
    return {
      remaining: response.total_available !== undefined ? response.total_available : Math.max(total - used, 0),
      used: used,
      total: total,
      unit: "USD"
    };
  }
})`;
    }
    if (templateType === "token_plan") {
      return `({
  request: {
    url: "{{baseUrl}}/api/user/self",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    var data = response.data || response;
    var total = data.total || data.quota || data.entitlement || 0;
    var used = data.used || data.used_quota || 0;
    return {
      planName: data.plan || data.plan_name || data.group || "Token plan",
      remaining: data.remaining !== undefined ? data.remaining : Math.max(total - used, 0),
      used: used,
      total: total,
      unit: data.unit || "tokens"
    };
  }
})`;
    }
    return `({
  request: {
    url: "{{baseUrl}}/user/balance",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    return {
      isValid: response.is_active !== false,
      remaining: response.balance,
      unit: "USD"
    };
  }
})`;
  }

  function buildUsageRequest(): UsageScriptSaveRequest | null {
    if (!pendingUsageProfile) {
      return null;
    }
    return {
      profileId: pendingUsageProfile.id,
      enabled: usageForm.enabled,
      templateType: usageForm.templateType,
      code: usageForm.code,
      apiKey: usageForm.apiKey.trim() ? usageForm.apiKey : null,
      baseUrl: usageForm.baseUrl.trim() ? normalizeBaseUrl(usageForm.baseUrl) : null,
      accessToken: usageForm.accessToken.trim() ? usageForm.accessToken : null,
      userId: usageForm.userId.trim() ? usageForm.userId : null,
      timeoutSeconds: Number(usageForm.timeoutSeconds),
      autoQueryIntervalMinutes: Number(usageForm.autoQueryIntervalMinutes)
    };
  }

  async function handleUsageSave() {
    const request = buildUsageRequest();
    if (!request || !canSaveUsage) {
      usageError = $t("profiles.usage.saveRequired");
      return;
    }
    usageBusy = "save";
    usageError = null;
    usageMessage = null;
    try {
      const state = await saveUsageScript(request);
      usageState = state;
      usageResult = state.config?.enabled ? state.lastResult : null;
      usageForm = usageFormFromState(pendingUsageProfile!, state);
      const usageEnabled = Boolean(state.config?.enabled);
      setProfileUsageEntry(pendingUsageProfile!.id, {
        result: usageEnabled ? state.lastResult : null,
        state: "idle",
        configured: usageEnabled,
        error: null,
        updatedAt: usageEnabled
          ? state.lastResult?.queriedAt ?? profileUsageEntries[pendingUsageProfile!.id]?.updatedAt ?? null
          : null
      });
      usageMessage = $t("profiles.usage.saveSuccess");
    } catch (err) {
      usageError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      usageBusy = null;
    }
  }

  async function handleUsageTest() {
    if (pendingUsageIsCodexOfficialOAuth) {
      return;
    }
    const request = buildUsageRequest();
    if (!request || !canSaveUsage) {
      usageError = $t("profiles.usage.saveRequired");
      return;
    }
    usageBusy = "test";
    usageError = null;
    usageMessage = null;
    try {
      usageResult = await testUsageScript(request);
      setProfileUsageResult(pendingUsageProfile!.id, usageResult);
      usageMessage = $t("profiles.usage.testSuccess");
    } catch (err) {
      usageError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      usageBusy = null;
    }
  }

  async function handleUsageQuery() {
    if (!pendingUsageProfile) {
      return;
    }
    if (!usageState?.config?.enabled) {
      return;
    }
    usageBusy = "query";
    usageError = null;
    usageMessage = null;
    const queryEntry = profileUsageEntries[pendingUsageProfile.id] ?? emptyProfileUsageEntry(pendingUsageProfile);
    setProfileUsageEntry(pendingUsageProfile.id, {
      ...queryEntry,
      state: "querying",
      configured: true,
      error: null
    });
    try {
      usageResult = await queryProfileUsage(pendingUsageProfile.id);
      setProfileUsageResult(pendingUsageProfile.id, usageResult);
      usageMessage = $t("profiles.usage.querySuccess");
    } catch (err) {
      usageError = errorLabel(err instanceof Error ? err.message : String(err));
      const current = profileUsageEntries[pendingUsageProfile.id] ?? emptyProfileUsageEntry();
      setProfileUsageEntry(pendingUsageProfile.id, {
        ...current,
        state: "idle",
        error: usageError
      });
    } finally {
      usageBusy = null;
    }
  }

  function configureUsageAutoQuery(profileId: string, intervalMinutes: number) {
    const normalizedInterval = Number(intervalMinutes) || 0;
    const nextKey = profileId && normalizedInterval > 0 ? `${profileId}:${normalizedInterval}` : "";
    if (usageAutoQueryKey === nextKey) {
      return;
    }
    clearUsageAutoQueryTimer();
    usageAutoQueryKey = nextKey;
    if (!profileId || normalizedInterval <= 0) {
      return;
    }
    usageAutoQueryTimer = window.setInterval(() => {
      if (!pendingUsageProfile || pendingUsageProfile.id !== profileId || usageBusy !== null) {
        return;
      }
      void handleUsageQuery();
    }, Math.max(normalizedInterval, 1) * 60 * 1000);
  }

  function clearUsageAutoQueryTimer() {
    if (usageAutoQueryTimer !== null) {
      window.clearInterval(usageAutoQueryTimer);
      usageAutoQueryTimer = null;
    }
    usageAutoQueryKey = "";
  }

  async function handleUsageDelete() {
    if (!pendingUsageProfile) {
      return;
    }
    usageBusy = "delete";
    usageError = null;
    usageMessage = null;
    try {
      const state = await deleteUsageScript(pendingUsageProfile.id);
      usageState = state;
      usageResult = null;
      usageForm = usageFormFromState(pendingUsageProfile, state);
      setProfileUsageEntry(pendingUsageProfile.id, {
        result: null,
        state: "idle",
        configured: false,
        error: null,
        updatedAt: null
      });
      usageMessage = $t("profiles.usage.deleteSuccess");
    } catch (err) {
      usageError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      usageBusy = null;
    }
  }

  function openEdit(profile: ProfileDraft) {
    if (profile.isBuiltin) {
      return;
    }
    pendingEdit = profile;
    editError = null;
    resetEditModels();
    const nextForm = {
      name: profile.name,
      icon: profile.icon ?? "",
      remark: profile.remark ?? "",
      mode: profile.mode,
      provider: profile.provider,
      protocol: profile.protocol,
      model: profile.model,
      modelMappings: modelMappingFormsFromProfile(profile),
      baseUrl: profile.baseUrl,
      apiKey: ""
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
    resetEditModels();
    editForm = emptyEditForm();
  }

  function resetEditModels() {
    editModelOptions = [];
    editModelLoading = false;
    editModelError = null;
    editModelLoadedKey = "";
  }

  async function refreshEditModels() {
    if (!pendingEdit) {
      return;
    }
    if (!canFetchEditModels) {
      editModelError = $t("profiles.modelListNeedsConfig");
      return;
    }

    editModelLoading = true;
    editModelError = null;
    const requestKey = editModelRequestKey;

    try {
      const result = await listProfileModels({
        profileId: pendingEdit.id,
        app: pendingEdit.app,
        mode: editForm.mode,
        provider: editForm.provider,
        protocol: editForm.protocol,
        baseUrl: normalizeBaseUrl(editForm.baseUrl),
        apiKey: editForm.apiKey.trim() || null
      });
      editModelOptions = result.models;
      editModelLoadedKey = requestKey;
      if (result.models.length === 0) {
        editModelError = $t("profiles.modelListEmpty");
      }
    } catch (err) {
      editModelOptions = [];
      editModelLoadedKey = "";
      editModelError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      editModelLoading = false;
    }
  }

  async function handleEditSave() {
    if (!pendingEdit || !canSaveEdit) {
      editError = editIconTooLong
        ? $t("profiles.iconTooLong")
        : editBaseUrlErrorKey ? $t(editBaseUrlErrorKey)
          : !editModelMappingsValid ? $t("profiles.modelMappingsInvalid")
            : $t("profiles.editRequired");
      return;
    }

    editingId = pendingEdit.id;
    editError = null;

    try {
      await updateProfileDraft({
        profileId: pendingEdit.id,
        name: editForm.name,
        icon: normalizedProfileIcon(editForm.icon),
        remark: editForm.remark,
        mode: pendingEdit.mode,
        provider: editForm.provider,
        protocol: editForm.protocol,
        model: editForm.model,
        modelMappings: modelMappingsForRequest(pendingEdit.app, editForm.modelMappings),
        baseUrl: normalizeBaseUrl(editForm.baseUrl),
        apiKey: editForm.apiKey.trim().length > 0 ? editForm.apiKey : null
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
    if (profile.isBuiltin || duplicatingId !== null || deletingId !== null || applyingId !== null || editingId !== null) {
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

  function openDelete(profile: ProfileDraft) {
    if (profile.isBuiltin || deletingId !== null || applyingId !== null || editingId !== null) {
      return;
    }
    pendingDelete = profile;
    profileIoError = null;
    profileIoMessage = null;
  }

  function closeDelete() {
    if (deletingId !== null) {
      return;
    }
    pendingDelete = null;
  }

  async function handleDeleteConfirm() {
    if (!pendingDelete || pendingDelete.isBuiltin || deletingId !== null) {
      return;
    }

    deletingId = pendingDelete.id;
    profileIoError = null;
    profileIoMessage = null;

    try {
      const deletedName = pendingDelete.name;
      await deleteProfileDraft({ profileId: pendingDelete.id });
      await onProfileSwitched();
      pendingDelete = null;
      pendingApply = null;
      pendingEdit = null;
      applyPreview = null;
      applyResult = null;
      profileIoMessage = $t("profiles.deleteSuccess", { name: deletedName });
    } catch (err) {
      profileIoError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      deletingId = null;
    }
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
  }

  function syncSelectedProfileTool(groups: ProfileGroup[]) {
    if (groups.length === 0) {
      if (selectedToolId !== null) {
        selectedToolId = null;
      }
      return;
    }
    if (!selectedToolId || !groups.some((group) => group.id === selectedToolId)) {
      selectedToolId = groups[0].id;
    }
  }

  function selectProfileTool(toolId: string) {
    selectedToolId = toolId;
  }

  function createProfileForCurrentTool() {
    onCreateProfile({
      mode: normalizedModeFilter,
      toolId: selectedProfileGroup?.id ?? undefined,
      toolName: selectedProfileGroup?.label ?? undefined
    });
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

  function domSafeId(value: string) {
    return value.replace(/[^a-zA-Z0-9_-]+/g, "-") || "model";
  }

  function profileModelRequestKey(input: {
    profileId?: string | null;
    app: string;
    mode: ProviderApplyMode;
    provider: string;
    protocol: string;
    baseUrl: string;
    apiKey: string;
  }) {
    return [
      input.profileId ?? "",
      input.app.trim(),
      input.mode,
      input.provider.trim(),
      input.protocol.trim(),
      normalizeBaseUrl(input.baseUrl),
      input.apiKey.trim() ? "inline-key" : "stored-key"
    ].join("|");
  }

  function modelOptionLabel(option: ProfileModelOption) {
    const label = option.name && option.name !== option.id ? `${option.id} - ${option.name}` : option.id;
    return option.supports1m ? `${label} (1M)` : label;
  }

  function profileSupportsModelMappings(toolId: string) {
    return canonicalProfileToolId(toolId) === "claude";
  }

  function emptyProfileModelMappingForm(): ProfileModelMappingForm {
    return {
      alias: "",
      model: "",
      supports1m: false,
      description: ""
    };
  }

  function modelMappingFormsFromProfile(profile: ProfileDraft): ProfileModelMappingForm[] {
    return (profile.modelMappings ?? []).map((mapping) => ({
      alias: mapping.alias,
      model: mapping.model,
      supports1m: Boolean(mapping.supports1m),
      description: mapping.description ?? ""
    }));
  }

  function modelMappingsForRequest(
    toolId: string,
    mappings: ProfileModelMappingForm[]
  ): ProfileModelMapping[] {
    if (!profileSupportsModelMappings(toolId)) {
      return [];
    }
    return mappings
      .map((mapping) => ({
        alias: mapping.alias.trim(),
        model: mapping.model.trim(),
        supports1m: Boolean(mapping.supports1m),
        description: mapping.description.trim() || null
      }))
      .filter((mapping) => mapping.alias || mapping.model || mapping.description);
  }

  function profileModelMappingsAreValid(mappings: ProfileModelMappingForm[]) {
    const aliases = new Set<string>();
    for (const mapping of mappings) {
      const alias = mapping.alias.trim();
      const model = mapping.model.trim();
      const description = mapping.description.trim();
      if (!alias && !model && !description) {
        continue;
      }
      if (!alias || !model) {
        return false;
      }
      const aliasKey = alias.toLowerCase();
      if (aliases.has(aliasKey)) {
        return false;
      }
      aliases.add(aliasKey);
    }
    return true;
  }

  function addEditModelMapping() {
    editForm = {
      ...editForm,
      modelMappings: [...editForm.modelMappings, emptyProfileModelMappingForm()]
    };
  }

  function updateEditModelMapping(index: number, patch: Partial<ProfileModelMappingForm>) {
    editForm = {
      ...editForm,
      modelMappings: editForm.modelMappings.map((mapping, itemIndex) =>
        itemIndex === index ? { ...mapping, ...patch } : mapping
      )
    };
  }

  function updateEditModelMappingModel(index: number, value: string) {
    const option = editModelOptions.find((item) => item.id === value.trim());
    const current = editForm.modelMappings[index];
    updateEditModelMapping(index, {
      model: value,
      supports1m: option?.supports1m ?? current?.supports1m ?? false,
      description: current?.description || option?.name || ""
    });
  }

  function removeEditModelMapping(index: number) {
    editForm = {
      ...editForm,
      modelMappings: editForm.modelMappings.filter((_, itemIndex) => itemIndex !== index)
    };
  }

  function editableOfficialProfileAllowed(profile: ProfileDraft | null, mode: ProviderApplyMode) {
    return Boolean(profile && canonicalProfileToolId(profile.app) === "codex" && mode === "config");
  }

  function loginTypeLabel(profile: ProfileDraft) {
    return providerIsOfficial(profile.provider) ? $t("profiles.login.official") : $t("profiles.login.api");
  }

  function profileEndpointLabel(profile: ProfileDraft) {
    if (providerIsOfficial(profile.provider) && !profile.baseUrl.trim()) {
      return $t("profiles.officialProfileEndpoint");
    }
    return profile.baseUrl;
  }

  function profileUrlLabel(profile: ProfileDraft) {
    return profileEndpointLabel(profile) || $t("common.none");
  }

  function profileRemarkLabel(profile: ProfileDraft) {
    return profile.remark?.trim() ?? "";
  }

  function profileDisplayName(profile: ProfileDraft) {
    const canonicalApp = canonicalProfileToolId(profile.app);
    if (profileUsesToolIcon(profile)) {
      const nameKey = officialProfileNameKeys[canonicalApp];
      if (nameKey) {
        return $t(nameKey);
      }
    }
    const toolName = toolLabels[canonicalApp];
    if (!toolName) {
      return profile.name;
    }
    return profile.name
      .replace(new RegExp(`^${escapeRegExp(toolName)}\\s*[-:/]?\\s*`, "i"), "")
      .trim() || profile.name;
  }

  function profileIconValue(profile: ProfileDraft) {
    const icon = profile.icon?.trim();
    if (icon) {
      return icon;
    }
    return profileDisplayName(profile).trim().charAt(0).toUpperCase() || "?";
  }

  function profileUsesToolIcon(profile: ProfileDraft) {
    return profile.isBuiltin && providerIsOfficial(profile.provider);
  }

  function profileIconIsImage(value: string) {
    return value.startsWith("data:image/");
  }

  function normalizedProfileIcon(value: string) {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  function profileIconTextTooLong(value: string) {
    const trimmed = value.trim();
    return trimmed.length > 0 && !profileIconIsImage(trimmed) && [...trimmed].length > 4;
  }

  function triggerProfileIconImport() {
    profileIconInput?.click();
  }

  async function handleProfileIconImport(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    input.value = "";
    if (!file) {
      return;
    }
    if (!file.type.startsWith("image/")) {
      editError = $t("profiles.iconImageOnly");
      return;
    }
    if (file.size > 512 * 1024) {
      editError = $t("profiles.iconImageTooLarge");
      return;
    }
    try {
      editForm = {
        ...editForm,
        icon: await readFileAsDataUrl(file)
      };
      editError = null;
    } catch (err) {
      editError = errorLabel(err instanceof Error ? err.message : String(err));
    }
  }

  function readFileAsDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result ?? ""));
      reader.onerror = () => reject(new Error($t("profiles.iconImportFailed")));
      reader.readAsDataURL(file);
    });
  }

  function escapeRegExp(value: string) {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }

  function profileCanSort() {
    return !profileDragDisabled();
  }

  function profileDragDisabled() {
    return resolveProfileDragDisabled({
      deletingId,
      applyingId,
      editingId,
      sortableSaving
    });
  }

  function profileIsDndShadow(profile: ProfileDraft) {
    return Boolean((profile as ProfileDraft & Record<string, unknown>)[SHADOW_ITEM_MARKER_PROPERTY_NAME]);
  }

  function profileSortableKey(profile: ProfileDraft) {
    return `${profile.id}:${profileIsDndShadow(profile) ? "shadow" : "item"}`;
  }

  function syncSortableProfiles(group: ProfileGroup | null, mode: ProviderApplyMode) {
    const nextProfiles = group?.profiles ?? [];
    const nextKey = `${mode}:${group?.id ?? ""}:${profileIdsFromItems(nextProfiles).join("|")}`;
    if (nextKey === sortableListKey) {
      return;
    }
    sortableListKey = nextKey;
    sortableProfiles = nextProfiles;
    sortableSourceProfiles = nextProfiles;
    sortableActiveId = null;
  }

  function handleProfileDndConsider(event: CustomEvent<DndEvent<ProfileDraft>>) {
    const currentProfiles = sortableProfiles;
    const nextProfiles = event.detail.items;
    if (event.detail.info.trigger === TRIGGERS.DRAG_STARTED) {
      sortableSourceProfiles = currentProfiles.filter((profile) => profile.id !== SHADOW_PLACEHOLDER_ITEM_ID);
      sortableActiveId = String(event.detail.info.id);
    } else if (event.detail.info.trigger === TRIGGERS.DRAGGED_ENTERED || event.detail.info.trigger === TRIGGERS.DRAGGED_OVER_INDEX) {
      sortableActiveId = String(event.detail.info.id);
    }
    sortableProfiles = nextProfiles;
  }

  async function handleProfileDndFinalize(event: CustomEvent<DndEvent<ProfileDraft>>) {
    const nextProfiles = event.detail.items.filter((profile) => profile.id !== SHADOW_PLACEHOLDER_ITEM_ID);
    sortableProfiles = nextProfiles;
    if (event.detail.info.trigger === TRIGGERS.DRAG_STOPPED) {
      sortableActiveId = null;
    }
    const nextIds = nextSortableProfileIds(sortableSourceProfiles, nextProfiles);
    if (!nextIds || !selectedProfileGroup) {
      sortableSourceProfiles = nextProfiles;
      sortableActiveId = null;
      return;
    }
    sortableSaving = true;
    try {
      profileIoError = null;
      profileIoMessage = null;
      const nextSummary = await reorderProfileDrafts({
        app: selectedProfileGroup.id,
        mode: normalizedModeFilter,
        profileIds: nextIds
      });
      summary = nextSummary;
      void (async () => {
        try {
          await onProfileSwitched();
        } catch (err) {
          profileIoError = errorLabel(err instanceof Error ? err.message : String(err));
        }
      })();
    } catch (err) {
      sortableProfiles = sortableSourceProfiles;
      profileIoError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      sortableSaving = false;
      sortableActiveId = null;
      sortableSourceProfiles = sortableProfiles;
    }
  }

  function styleDraggedProfileElement(element?: HTMLElement) {
    if (!element) {
      return;
    }
    element.setAttribute("data-sortable-active", "true");
    element
      .querySelector("[data-profile-card]")
      ?.setAttribute("data-drag-active", "true");
  }

  function profileSupportsUsageQuery(profile: ProfileDraft) {
    return profileUsesCodexOfficialOAuth(profile) || (!providerIsOfficial(profile.provider) && Boolean(profile.baseUrl.trim() || profile.authRef));
  }

  function profileCanOpenUsage(profile: ProfileDraft) {
    return profileSupportsUsageQuery(profile);
  }

  function profileUsesCodexOfficialOAuth(profile: ProfileDraft) {
    return canonicalProfileToolId(profile.app) === "codex" && providerIsOfficial(profile.provider);
  }

  function formatUsageValue(value: number | null | undefined, unit: string | null | undefined) {
    if (typeof value !== "number" || Number.isNaN(value)) {
      return $t("common.none");
    }
    const formatted = Math.abs(value) >= 1000 ? value.toLocaleString() : value.toFixed(2).replace(/\.00$/, "");
    return unit ? `${formatted} ${unit}` : formatted;
  }

  function usageItemTitle(item: UsageData, index: number) {
    return item.planName || $t("profiles.usage.resultPlanFallback", { index: index + 1 });
  }

  function usageQueriedAt(result: UsageQueryResult | null) {
    if (!result?.queriedAt) {
      return $t("profiles.usage.neverQueried");
    }
    return new Date(result.queriedAt).toLocaleString();
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
      "Config profiles need a stored Provider API key for this Provider.": "profiles.warning.configNeedsStoredKey",
      "Selected mode writes this client config; detailed file changes are shown below.": "profiles.preview.nativeWriteDetail",
      "Selected profile type writes this client config; detailed file changes are shown below.": "profiles.preview.nativeWriteDetail",
      "This profile does not require a native client config write.": "profiles.preview.nativeReservedDetail",
      "Official provider uses the client login directly and does not run through the local gateway.": "profiles.warning.officialGatewayUnsupported",
      "Official provider uses the target client's own login.": "profiles.warning.officialClientLogin",
      "No Provider API key or model override is required.": "profiles.warning.noProviderKeyOrModel",
      "Changing Codex config usually requires restarting Codex or opening a new Codex session.": "profiles.warning.codexReloadRequired",
      "Direct config file mode writes Provider connection details into the client config.": "profiles.warning.directConfigWrites",
      "Config profiles write Provider connection details into the client config.": "profiles.warning.directConfigWrites",
      "Frequent Provider switching may require the client to reload its own config.": "profiles.warning.frequentSwitchReload",
      "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.": "profiles.warning.upstreamKeysInKeychain",
      "The client still needs to reload config after the first gateway bootstrap.": "profiles.warning.reloadAfterFirstGateway",
      "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.": "profiles.warning.gatewayManualStart",
      "Gateway mode writes Claude Code settings to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesClaude",
      "Gateway profiles write Claude Code settings to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesClaude",
      "Gateway mode writes Gemini CLI environment values to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesGemini",
      "Gateway profiles write Gemini CLI environment values to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesGemini",
      "Gateway mode writes OpenCode's provider entry to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesOpenCode",
      "Gateway profiles write OpenCode's provider entry to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesOpenCode",
      "Gateway mode writes OpenClaw's provider entry to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesOpenClaw",
      "Gateway profiles write OpenClaw's provider entry to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesOpenClaw",
      "Gateway mode writes Hermes custom provider settings to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesHermes",
      "Gateway profiles write Hermes custom provider settings to the tool-scoped local gateway URL.": "profiles.warning.gatewayWritesHermes",
      "Config file mode writes Codex's provider entry directly to the selected upstream Provider.": "profiles.warning.configWritesCodexProvider",
      "Config profiles write Codex's provider entry directly to the selected upstream Provider.": "profiles.warning.configWritesCodexProvider",
      "The preview masks the Provider API key. Apply writes the actual key from the system keychain to Codex auth.json.": "profiles.warning.previewMasksProviderKey",
      "Gateway mode is a one-time relay injection target, not a direct Provider switch.": "profiles.warning.gatewayRelayTarget",
      "Gateway profiles are a one-time relay injection target, not a direct Provider switch.": "profiles.warning.gatewayRelayTarget",
      "Switching profiles later changes only the Gateway active profile for this tool.": "profiles.warning.gatewaySwitchOnly",
      "The preview masks the local CodeStudio token. Apply writes only this local token to Codex auth.json; upstream Provider keys stay in the system keychain.": "profiles.warning.gatewayMasksLocalToken",
      "Codex official login is still required for the desktop app; the Local Gateway only takes over model requests.": "profiles.warning.codexLoginStillRequired",
      "If Codex is already running, restart Codex or open a new Codex session after bootstrap so it reloads config.toml.": "profiles.warning.reloadAfterGateway",
      "Codex config does not exist yet; adapter would create it after confirmation.": "profiles.warning.codexConfigMissing",
      "Hermes config does not exist yet; adapter would create it after confirmation.": "profiles.warning.hermesConfigMissing",
      "Config file mode writes Claude Code user settings under the env section.": "profiles.warning.claudeSettingsEnv",
      "Config profiles write Claude Code user settings under the env section.": "profiles.warning.claudeSettingsEnv",
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
      "Hermes config profiles currently target OpenAI Chat Completions endpoints.": "profiles.warning.hermesChatOnly",
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
      "Uses file-backed Codex authentication so managed credentials are read from auth.json.": "profiles.diff.codexScopedApiTokens",
      "Disables Codex's built-in OpenAI auth requirement for this managed provider.": "profiles.diff.disableManagedCodexOpenAiAuth",
      "Adds the CodeStudio Lite actor-authorization header to this managed provider.": "profiles.diff.actorAuthorizationHeader",
      "Removes a legacy API-key mirror from Codex config.toml without touching auth.json.": "profiles.diff.removeLegacyApiKeyMirror",
      "Removes a legacy environment-style API key from Codex config.toml.": "profiles.diff.removeLegacyEnvApiKey",
      "Uses Codex auth.json with the local Gateway token for this provider entry.": "profiles.diff.storeLocalToken",
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

    const adapterMatch = message.match(/(?:Config file mode|Config profile) adapter is not implemented for '([^']+)'\./);
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
    if (message === "Profile Name is required" || message === "Configuration name is required") {
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
        tool: toolLabels[canonicalProfileToolId(configProtocolMatch[2])] ?? configProtocolMatch[2]
      });
    }
    if (message === "Provider API key is required for non-official providers.") {
      return $t("wizard.check.credentialMissing");
    }
    if (
      message === "Official provider uses the client login directly and cannot use Gateway mode." ||
      message === "Official provider uses the client login directly and cannot use Gateway profile." ||
      message === "Official provider uses the client login directly and cannot use Gateway profiles."
    ) {
      return $t("profiles.officialGatewayBlocked");
    }
    if (message === "Built-in official profiles cannot be modified.") {
      return $t("profiles.builtinModifyBlocked");
    }
    if (message === "Built-in official profiles cannot be duplicated.") {
      return $t("profiles.builtinDuplicateBlocked");
    }
    if (message === "Built-in official profiles cannot be deleted.") {
      return $t("profiles.builtinDeleteBlocked");
    }
    if (message === "Official profiles are built in and cannot be saved as custom profiles.") {
      return $t("profiles.officialCustomSaveBlocked");
    }
    if (message === "Official provider uses the client login directly and does not run through the local gateway.") {
      return $t("profiles.warning.officialGatewayUnsupported");
    }
    if (message === "Claude Code model mappings require both alias and target model.") {
      return $t("profiles.modelMappingsInvalid");
    }
    const duplicateMappingMatch = message.match(/Claude Code model mapping alias '([^']+)' is duplicated\./);
    if (duplicateMappingMatch) {
      return $t("profiles.modelMappingsDuplicate", { alias: duplicateMappingMatch[1] });
    }
    if (
      message === "Profile is already active for this tool and mode." ||
      message === "Profile is already active for this tool and profile category."
    ) {
      return $t("profiles.alreadyActiveBlocked");
    }
    const toolNotInstalledMatch = message.match(/Tool '([^']+)' is not installed, so a profile cannot be created for it\./);
    if (toolNotInstalledMatch) {
      return $t("wizard.error.toolNotInstalled", {
        tool: toolLabels[canonicalProfileToolId(toolNotInstalledMatch[1])] ?? toolNotInstalledMatch[1]
      });
    }
    if (
      message === "Apply and restart is only available for Config file mode." ||
      message === "Apply and restart is only available for Config profiles."
    ) {
      return $t("profiles.restartConfigOnly");
    }
    if (message === "Apply and restart requires a native client config write for this profile.") {
      return $t("profiles.restartNeedsNativeWrite");
    }
    const applyToolMatch = message.match(/Tool '([^']+)' is not in the local registry, so this profile cannot be applied yet\./);
    if (applyToolMatch) {
      return $t("profiles.warning.toolCannotApply", { app: applyToolMatch[1] });
    }
    const unsupportedModeMatch = message.match(/(.+)(?: mode)? is not supported for this profile\./);
    if (unsupportedModeMatch) {
      return $t("profiles.warning.modeUnsupported", { mode: unsupportedModeMatch[1] });
    }
    return message;
  }

  function buildProfileModeSections(
    profileSummary: ProfileSummary | null,
    installedToolIds: Set<string> | null,
    mode: ProviderApplyMode
  ): ProfileModeSection[] {
    const drafts = profileSummary?.drafts ?? [];
    const activeByMode = profileSummary?.activeProfilesByMode ?? { config: {}, gateway: {} };
    return [
      {
        mode,
        groups: buildProfileGroups(
          drafts.filter((profile) => profile.mode === mode && profileVisibleInProfiles(profile)),
          activeByMode[mode],
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
          const orderCompare = left.sortOrder - right.sortOrder;
          if (orderCompare !== 0) {
            return orderCompare;
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
    if (["chatgpt-desktop", "codex-app", "codex-client", "codex-desktop", "codex-cli", "codex-vscode", "codex-code-vscode", "codex-vs-code"].includes(normalized)) {
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
    return true;
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
      return activeProfiles["chatgpt-desktop"]
        ?? activeProfiles["codex-app"]
        ?? activeProfiles["codex-client"]
        ?? activeProfiles["codex-desktop"]
        ?? null;
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

  function normalizeBaseUrl(value: string) {
    return value.trim();
  }

  function handleEditBaseUrlInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    editForm = {
      ...editForm,
      baseUrl: value
    };
  }

  function handleUsageBaseUrlInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    usageForm = {
      ...usageForm,
      baseUrl: value
    };
  }

  function normalizeEditBaseUrlInput() {
    editForm = {
      ...editForm,
      baseUrl: normalizeBaseUrl(editForm.baseUrl)
    };
  }

  function normalizeUsageBaseUrlInput() {
    usageForm = {
      ...usageForm,
      baseUrl: normalizeBaseUrl(usageForm.baseUrl)
    };
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
</script>

<div class={embedded ? profileEmbeddedStackRecipe() : routeStackRecipe({ width: "full" })}>
  {#if !embedded}
    <section class={topStripRecipe({ compact: true })}>
      <div>
        <h1>{$t("profiles.title")}</h1>
      </div>
      <div class={topActionsRecipe()}>
        <div class={profileModeSwitcherRecipe()} role="group" aria-label={$t("profiles.viewSwitcherLabel")}>
          {#each profileViewOptions as option}
            <button
              type="button"
              data-selected={normalizedModeFilter === option.value}
              aria-pressed={normalizedModeFilter === option.value}
              on:click={() => (modeFilter = option.value)}
            >
              {$t(option.labelKey)}
            </button>
          {/each}
        </div>
        <button
          class={actionButtonRecipe()}
          title={$t("common.createConfig")}
          on:click={createProfileForCurrentTool}
        >
          <AppIcon name="add" size={16} />
          {$t("common.createConfig")}
        </button>
      </div>
    </section>
  {/if}

  {#if profileIoError}
    <DismissibleNotice tone="error" message={profileIoError} on:dismiss={() => (profileIoError = null)} />
  {/if}
  {#if profileIoMessage}
    <DismissibleNotice tone="success" message={profileIoMessage} on:dismiss={() => (profileIoMessage = null)} />
  {/if}

  {#if summary}
    {#if embedded}
      <div class={cx(sectionActionsRecipe(), embeddedProfileActionsClass)}>
        <button class={actionButtonRecipe()} title={$t("common.createConfig")} on:click={createProfileForCurrentTool}>
          <AppIcon name="add" size={16} />
          {$t("common.createConfig")}
        </button>
      </div>
    {/if}
    <div class={profileModeLayoutRecipe()}>
      {#if profileToolGroups.length > 0}
        <section class={profileToolSwitcherRecipe()} aria-label={$t("profiles.toolSwitcherLabel")}>
          <div class={profileToolTabsRecipe()} role="tablist">
            {#each profileToolGroups as group}
              {@const activeGroupProfile = group.profiles.find((profile) => profile.id === group.activeProfileId) ?? null}
              <button
                type="button"
                data-selected={selectedToolId === group.id}
                role="tab"
                aria-selected={selectedToolId === group.id}
                title={group.label}
                on:click={() => selectProfileTool(group.id)}
              >
                <ToolIcon toolId={group.id} label={group.label} variant="choice" />
                <span>
                  <strong>{group.label}</strong>
                  <small>{activeGroupProfile ? loginTypeLabel(activeGroupProfile) : $t("profiles.noActiveProfile")}</small>
                </span>
              </button>
            {/each}
          </div>
        </section>

        {#if selectedProfileGroup}
          <section class={profileToolSectionRecipe()}>
            <div
              class={profileGridRecipe()}
              role="list"
              use:dragHandleZone={{
                items: displayedProfiles,
                flipDurationMs: profileFlipDurationMs,
                dragDisabled: profileDragDisabled(),
                dropFromOthersDisabled: true,
                dropTargetStyle: profileDropTargetStyle,
                transformDraggedElement: styleDraggedProfileElement,
                zoneTabIndex: -1,
                zoneItemTabIndex: -1
              }}
              on:consider={handleProfileDndConsider}
              on:finalize={handleProfileDndFinalize}
            >
              {#each displayedProfiles as profile (profileSortableKey(profile))}
                {@const isActive = selectedProfileGroup.activeProfileId === profile.id}
                {@const cardActionKey = actionKey(profile.id, profile.mode)}
                {@const profileIcon = profileIconValue(profile)}
                <div
                  class={profileSortableRowRecipe()}
                  role="listitem"
                  data-profile-sortable-id={profile.id}
                  data-sortable-active={sortableActiveId === profile.id}
                  data-is-dnd-shadow-item-hint={profileIsDndShadow(profile)}
                  animate:flip={{ duration: profileFlipDurationMs }}
                >
                <article
                  class={profileCardRecipe()}
                  data-profile-card
                  data-active={isActive}
                  data-builtin={profile.isBuiltin}
                  data-drag-active={sortableActiveId === profile.id}
                >
                  <div class={profileCardMainRecipe()}>
                    <span
                      class={profileDragHandleRecipe()}
                      aria-label={$t("profiles.dragHandle")}
                      aria-disabled={!profileCanSort()}
                      data-profile-drag-handle={profile.id}
                      use:dragHandle
                    >
                      <AppIcon name="drag" size={16} />
                    </span>
                    <div class={profileAvatarRecipe()} data-profile-avatar aria-hidden="true">
                      {#if profileUsesToolIcon(profile)}
                        <ToolIcon toolId={profile.app} label={profileDisplayName(profile)} variant="heading" />
                      {:else if profileIconIsImage(profileIcon)}
                        <img src={profileIcon} alt="" />
                      {:else}
                        <span>{profileIcon}</span>
                      {/if}
                    </div>
                    <div class={profileIdentityRecipe()}>
                      <h2>{profileDisplayName(profile)}</h2>
                      <p>{profileUrlLabel(profile)}</p>
                      {#if profileRemarkLabel(profile)}
                        <p data-profile-remark>{profileRemarkLabel(profile)}</p>
                      {/if}
                    </div>
                  </div>
                  {#if profile.isBuiltin}
                    <div class={profileCardStatusRecipe()}>
                      <StatusPill
                        status="info"
                        label={$t("profiles.builtinOfficial")}
                      />
                    </div>
                  {/if}
                  <div class={profileCardActionsRecipe()}>
                    <button
                      class={actionButtonRecipe({ tone: "primary" })}
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
                    {#if profileCanOpenUsage(profile)}
                      <button
                        class={iconButtonRecipe()}
                        title={$t("profiles.usage.open")}
                        disabled={usageBusy !== null || applyingId !== null || editingId !== null}
                        on:click={() => openUsage(profile)}
                      >
                        <AppIcon name="stats" size={16} />
                      </button>
                    {/if}
                    {#if !profile.isBuiltin}
                      <button class={iconButtonRecipe()} title={$t("profiles.editProfile")} disabled={duplicatingId !== null || deletingId !== null} on:click={() => openEdit(profile)}><AppIcon name="edit" size={16} /></button>
                      <button
                        class={iconButtonRecipe()}
                        title={$t("profiles.duplicateProfile")}
                        disabled={duplicatingId !== null || deletingId !== null || applyingId !== null || editingId !== null}
                        on:click={() => handleDuplicate(profile)}
                      >
                        {#if duplicatingId === profile.id}
                          <AppIcon name="loading" class={spinRecipe()} size={16} />
                        {:else}
                          <AppIcon name="copy" size={16} />
                        {/if}
                      </button>
                      <button
                        class={iconButtonRecipe({ danger: true })}
                        title={$t("profiles.deleteProfile")}
                        disabled={duplicatingId !== null || deletingId !== null || applyingId !== null || editingId !== null}
                        on:click={() => openDelete(profile)}
                      >
                        {#if deletingId === profile.id}
                          <AppIcon name="loading" class={spinRecipe()} size={16} />
                        {:else}
                          <AppIcon name="delete" size={16} />
                        {/if}
                      </button>
                    {/if}
                  </div>
                </article>
                </div>
              {/each}
            </div>
          </section>
        {/if}
      {:else}
        <section class={panelRecipe()}>
          <div class={emptyRowRecipe()}>{$t(emptyProfilesMessageKey(summary, visibleProfileCount, installedProfileToolIds))}</div>
        </section>
      {/if}
    </div>
  {:else}
    <section class={panelRecipe()}>
      <div class={emptyRowRecipe()}>{$t("profiles.noProfiles")}</div>
    </section>
  {/if}

  {#if pendingUsageProfile}
    <div class={desktopClientModalBackdropRecipe()} role="presentation">
      <div class={cx(desktopClientModalPanelRecipe(), usageModalPanelClass)} role="dialog" aria-modal="true" aria-labelledby="usage-title">
        <div class={desktopClientModalBodyRecipe()}>
          <div>
          <h2 id="usage-title">{$t("profiles.usage.title", { name: pendingUsageProfile.name })}</h2>
        </div>

        {#if usageError}
          <div class={profileInlineNoticeRecipe({ tone: "error" })}>{usageError}</div>
        {/if}
        {#if usageMessage}
          <div class={profileInlineNoticeRecipe({ tone: "success" })}>{usageMessage}</div>
        {/if}

        {#if usageBusy === "load"}
          <div class={cx(emptyRowRecipe(), inlineEmptyClass)}>
            <AppIcon name="loading" class={spinRecipe()} size={18} />
            {$t("common.loading")}
          </div>
        {:else}
          <label class={cx(nativeToggleRecipe(), usageToggleClass)} data-native-toggle>
            <input type="checkbox" bind:checked={usageForm.enabled} disabled={usageBusy !== null} />
            <span>
              <strong>{$t("profiles.usage.enabled")}</strong>
              <small>{$t("profiles.usage.enabledDescription")}</small>
            </span>
          </label>

          {#if pendingUsageIsCodexOfficialOAuth}
            <div class={profileUsageOfficialPanelRecipe()}>
              <AppIcon name="stats" size={18} />
              <div>
                <strong>{$t("profiles.usage.officialOAuth")}</strong>
                <span>{$t("profiles.usage.officialOAuthHint")}</span>
              </div>
            </div>
          {:else}
            <div class={profileUsageTemplateRowRecipe()}>
              {#each usageTemplateOptions as option}
                <button
                  type="button"
                  data-selected={usageForm.templateType === option.id}
                  disabled={usageBusy !== null}
                  on:click={() => selectUsageTemplate(option.id)}
                >
                  {$t(option.labelKey)}
                </button>
              {/each}
            </div>

            <div class={profileFormGridRecipe({ columns: "double" })}>
              <label>
                {$t("wizard.providerBaseUrl")}
                <input
                  value={usageForm.baseUrl}
                  disabled={usageBusy !== null}
                  placeholder={pendingUsageProfile.baseUrl}
                  on:input={handleUsageBaseUrlInput}
                  on:blur={normalizeUsageBaseUrlInput}
                />
              </label>
              <label>
                {$t("wizard.providerApiKey")}
                <input
                  type="password"
                  bind:value={usageForm.apiKey}
                  disabled={usageBusy !== null}
                  placeholder={$t(pendingUsageProfile.authRef ? "profiles.usage.keepProfileKey" : "profiles.usage.keyOptional")}
                />
              </label>
              <label>
                {$t("profiles.usage.accessToken")}
                <input type="password" bind:value={usageForm.accessToken} disabled={usageBusy !== null} placeholder={$t("profiles.usage.accessTokenPlaceholder")} />
              </label>
              <label>
                {$t("profiles.usage.userId")}
                <input bind:value={usageForm.userId} disabled={usageBusy !== null} placeholder={$t("profiles.usage.userIdPlaceholder")} />
              </label>
              <label>
                {$t("profiles.usage.timeout")}
                <input type="number" min="2" max="60" bind:value={usageForm.timeoutSeconds} disabled={usageBusy !== null} />
              </label>
              <label>
                {$t("profiles.usage.autoInterval")}
                <input type="number" min="0" max="1440" bind:value={usageForm.autoQueryIntervalMinutes} disabled={usageBusy !== null} />
                <small>{$t("profiles.usage.autoIntervalHint")}</small>
              </label>
            </div>

            <label class={profileUsageCodeFieldRecipe()}>
              <span>{$t("profiles.usage.script")}</span>
              <textarea bind:value={usageForm.code} disabled={usageBusy !== null} spellcheck="false"></textarea>
            </label>
          {/if}

          <section class={profileDiffPanelRecipe()}>
            <div class={profileDiffHeadingRecipe()}>
              <div>
                <strong>{$t("profiles.usage.resultTitle")}</strong>
                <span>{$t("profiles.usage.queriedAt", { time: usageQueriedAt(usageResult) })}</span>
              </div>
              <StatusPill status={usageResult?.success ? "ok" : "info"} label={usageResult?.success ? $t("common.ok") : $t("profiles.usage.noResult")} />
            </div>
            {#if usageResult?.data.length}
              <div class={profileUsageResultGridRecipe()}>
                {#each usageResult.data as item, index}
                  <div class={profileUsageResultCardRecipe()} data-invalid={item.isValid === false}>
                    <strong>{usageItemTitle(item, index)}</strong>
                    {#if item.isValid === false}
                      <span>{item.invalidMessage ?? $t("profiles.usage.invalid")}</span>
                    {/if}
                    <dl>
                      <div>
                        <dt>{$t("profiles.usage.remaining")}</dt>
                        <dd data-usage-balance>{formatUsageValue(item.remaining, item.unit)}</dd>
                      </div>
                      <div>
                        <dt>{$t("profiles.usage.used")}</dt>
                        <dd>{formatUsageValue(item.used, item.unit)}</dd>
                      </div>
                      <div>
                        <dt>{$t("profiles.usage.total")}</dt>
                        <dd>{formatUsageValue(item.total, item.unit)}</dd>
                      </div>
                    </dl>
                    {#if item.extra}
                      <small>{item.extra}</small>
                    {/if}
                  </div>
                {/each}
              </div>
            {:else}
              <div class={emptyRowRecipe()}>{$t("profiles.usage.emptyResult")}</div>
            {/if}
          </section>
        {/if}

        </div>

        <div class={desktopClientModalActionsRecipe()}>
          <button class={actionButtonRecipe()} disabled={usageBusy !== null} on:click={closeUsage}>
            {$t("common.close")}
          </button>
          {#if usageState?.config && !pendingUsageIsCodexOfficialOAuth}
            <button class={cx(actionButtonRecipe(), dangerButtonClass)} disabled={usageBusy !== null} on:click={handleUsageDelete}>
              {#if usageBusy === "delete"}
                <AppIcon name="loading" class={spinRecipe()} size={16} />
              {:else}
                <AppIcon name="delete" size={16} />
              {/if}
              {$t("profiles.usage.delete")}
            </button>
          {/if}
          {#if !pendingUsageIsCodexOfficialOAuth}
            <button class={actionButtonRecipe()} disabled={!canSaveUsage || usageBusy !== null} on:click={handleUsageTest}>
              {#if usageBusy === "test"}
                <AppIcon name="loading" class={spinRecipe()} size={16} />
              {:else}
                <AppIcon name="play" size={16} />
              {/if}
              {$t("profiles.usage.test")}
            </button>
          {/if}
          <button
            class={actionButtonRecipe({ tone: pendingUsageIsCodexOfficialOAuth ? "primary" : "secondary" })}
            disabled={!usageState?.config?.enabled || usageBusy !== null}
            on:click={handleUsageQuery}
          >
            {#if usageBusy === "query"}
              <AppIcon name="loading" class={spinRecipe()} size={16} />
            {:else}
              <AppIcon name="stats" size={16} />
            {/if}
            {$t("profiles.usage.query")}
          </button>
          <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canSaveUsage || usageBusy !== null} on:click={handleUsageSave}>
            {#if usageBusy === "save"}
              <AppIcon name="loading" class={spinRecipe()} size={16} />
              {$t("common.saving")}
            {:else}
              <AppIcon name="apply" size={16} />
              {$t("common.save")}
            {/if}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if pendingEdit}
    <div class={desktopClientModalBackdropRecipe()} role="presentation">
      <div class={cx(desktopClientModalPanelRecipe(), modalPanelWideClass)} role="dialog" aria-modal="true" aria-labelledby="edit-title">
        <div class={desktopClientModalBodyRecipe()}>
          <div>
          <h2 id="edit-title">{$t("profiles.editTitle", { name: pendingEdit.name })}</h2>
          <p>{$t("profiles.editDescription")}</p>
        </div>

        {#if editError}
          <div class={profileInlineNoticeRecipe({ tone: "error" })}>{editError}</div>
        {/if}

        <div class={profileIconEditorRecipe()}>
          <div class={profileAvatarRecipe({ size: "large" })} aria-hidden="true">
            {#if profileIconIsImage(editForm.icon.trim())}
              <img src={editForm.icon.trim()} alt="" />
            {:else}
              <span>{editForm.icon.trim() || profileDisplayName(pendingEdit).trim().charAt(0).toUpperCase() || "?"}</span>
            {/if}
          </div>
          <label>
            {$t("profiles.iconLabel")}
            <input
              bind:value={editForm.icon}
              disabled={editingId !== null}
              placeholder={$t("profiles.iconPlaceholder")}
            />
            {#if editIconTooLong}
              <small class={profileFieldErrorRecipe()}>{$t("profiles.iconTooLong")}</small>
            {/if}
          </label>
          <div class={profileIconActionsRecipe()}>
            <button class={actionButtonRecipe()} type="button" disabled={editingId !== null} on:click={triggerProfileIconImport}>
              <AppIcon name="upload" size={16} />
              {$t("profiles.iconImport")}
            </button>
            <button class={actionButtonRecipe()} type="button" disabled={editingId !== null || editForm.icon.trim().length === 0} on:click={() => (editForm = { ...editForm, icon: "" })}>
              {$t("profiles.iconUseDefault")}
            </button>
            <input bind:this={profileIconInput} type="file" accept="image/*" on:change={handleProfileIconImport} />
          </div>
        </div>

        <div class={profileFormGridRecipe({ columns: "double" })}>
          <label>
            {$t("wizard.profileName")}
            <input bind:value={editForm.name} disabled={editingId !== null} />
          </label>
          <label>
            {$t("profiles.remarkLabel")}
            <textarea bind:value={editForm.remark} rows="2" disabled={editingId !== null} placeholder={$t("profiles.remarkPlaceholder")}></textarea>
          </label>
          <label>
            {$t("profiles.tool")}
            <input value={toolLabels[pendingEdit.app] ?? pendingEdit.app} disabled />
          </label>
          <label>
            {$t("profiles.providerModeTitle")}
            <input value={applyModeLabel(pendingEdit.mode)} disabled />
          </label>
          <label>
            {$t("common.provider")}
            <input bind:value={editForm.provider} disabled={editingId !== null} />
          </label>
        <label>
          {$t(editForm.mode === "gateway" ? "wizard.upstreamApi" : "wizard.protocol")}
          <select bind:value={editForm.protocol} disabled={editingId !== null}>
            {#each availableEditProtocolOptions as option}
              <option value={option.id}>{$t(option.labelKey)}</option>
            {/each}
          </select>
        </label>
          <div class={modelPickerClass}>
            <label for={`${editModelListId}-input`}>{$t("common.model")}</label>
            <div class={modelPickerRowClass}>
              <ModelSelectInput
                id={`${editModelListId}-input`}
                bind:value={editForm.model}
                options={editModelOptions}
                optionLabel={modelOptionLabel}
                toggleTitle={$t("common.model")}
                disabled={editingId !== null}
              />
              <button
                class={actionButtonRecipe()}
                type="button"
                data-refresh-button="true"
                disabled={editModelFetchDisabled}
                title={$t("profiles.fetchModels")}
                on:click={refreshEditModels}
              >
                <AppIcon name={editModelLoading ? "loading" : "refresh"} class={editModelLoading ? spinRecipe() : ""} size={15} />
                {editModelLoading ? $t("profiles.fetchingModels") : $t("profiles.fetchModels")}
              </button>
            </div>
            {#if editModelStatus}
              <small class={modelPickerStatusClass}>{editModelStatus}</small>
            {/if}
          </div>
          {#if editSupportsModelMappings}
            <section class={modelMappingPanelClass}>
              <div class={modelMappingHeaderClass}>
                <strong>{$t("profiles.modelMappingsTitle")}</strong>
                <button
                  class={actionButtonRecipe()}
                  type="button"
                  disabled={editingId !== null}
                  on:click={addEditModelMapping}
                >
                  <AppIcon name="add" size={15} />
                  {$t("profiles.modelMappingAdd")}
                </button>
              </div>
              {#if editForm.modelMappings.length > 0}
                <div class={modelMappingRowsClass}>
                  {#each editForm.modelMappings as mapping, index}
                    <div class={modelMappingRowClass}>
                      <label>
                        {$t("profiles.modelMappingAlias")}
                        <input
                          value={mapping.alias}
                          disabled={editingId !== null}
                          on:input={(event) => updateEditModelMapping(index, { alias: event.currentTarget.value })}
                        />
                      </label>
                      <div class={modelMappingFieldClass}>
                        <label for={`${editModelListId}-mapping-${index}`}>{$t("profiles.modelMappingTarget")}</label>
                        <ModelSelectInput
                          id={`${editModelListId}-mapping-${index}`}
                          value={mapping.model}
                          options={editModelOptions}
                          optionLabel={modelOptionLabel}
                          toggleTitle={$t("profiles.modelMappingTarget")}
                          disabled={editingId !== null}
                          on:input={(event) => updateEditModelMappingModel(index, event.detail.value)}
                        />
                      </div>
                      <label>
                        {$t("profiles.modelMappingDescription")}
                        <input
                          value={mapping.description}
                          disabled={editingId !== null}
                          on:input={(event) => updateEditModelMapping(index, { description: event.currentTarget.value })}
                        />
                      </label>
                      <label class={modelMappingToggleClass}>
                        <input
                          type="checkbox"
                          checked={mapping.supports1m}
                          disabled={editingId !== null}
                          on:change={(event) => updateEditModelMapping(index, { supports1m: event.currentTarget.checked })}
                        />
                        {$t("profiles.modelMappingSupports1m")}
                      </label>
                      <button
                        class={iconButtonRecipe({ danger: true })}
                        type="button"
                        title={$t("profiles.modelMappingRemove")}
                        disabled={editingId !== null}
                        on:click={() => removeEditModelMapping(index)}
                      >
                        <AppIcon name="delete" size={16} />
                      </button>
                    </div>
                  {/each}
                </div>
                {#if !editModelMappingsValid}
                  <small class={profileFieldErrorRecipe()}>{$t("profiles.modelMappingsInvalid")}</small>
                {/if}
              {/if}
            </section>
          {/if}
          {#if providerNeedsBaseUrl(editForm.provider)}
            <label>
              {$t("wizard.providerBaseUrl")}
              <input
                value={editForm.baseUrl}
                disabled={editingId !== null}
                on:input={handleEditBaseUrlInput}
                on:blur={normalizeEditBaseUrlInput}
              />
              {#if editBaseUrlErrorKey}
                <small class={profileFieldErrorRecipe()}>{$t(editBaseUrlErrorKey)}</small>
              {/if}
            </label>
          {/if}
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

        </div>

        <div class={desktopClientModalActionsRecipe()}>
          <button class={actionButtonRecipe()} disabled={editingId !== null} on:click={closeEdit}>
            {$t("common.cancel")}
          </button>
          <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canSaveEdit} on:click={handleEditSave}>
            {#if editingId === pendingEdit.id}
              <AppIcon name="loading" class={spinRecipe()} size={16} />
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

  {#if pendingDelete}
    <div class={desktopClientModalBackdropRecipe()} role="presentation">
      <div class={desktopClientModalPanelRecipe()} role="dialog" aria-modal="true" aria-labelledby="delete-title">
        <div class={desktopClientModalBodyRecipe()}>
          <div>
          <h2 id="delete-title">{$t("profiles.deleteTitle", { name: pendingDelete.name })}</h2>
          <p>{$t("profiles.deleteDescription")}</p>
        </div>

        </div>

        <div class={desktopClientModalActionsRecipe()}>
          <button class={actionButtonRecipe()} disabled={deletingId !== null} on:click={closeDelete}>
            {$t("common.cancel")}
          </button>
          <button class={cx(actionButtonRecipe({ tone: "primary" }), dangerButtonClass)} disabled={deletingId !== null} on:click={handleDeleteConfirm}>
            {#if deletingId === pendingDelete.id}
              <AppIcon name="loading" class={spinRecipe()} size={16} />
              {$t("profiles.deleting")}
            {:else}
              <AppIcon name="delete" size={16} />
              {$t("profiles.deleteConfirm")}
            {/if}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if pendingApply}
    <div class={desktopClientModalBackdropRecipe()} role="presentation">
      <div class={cx(desktopClientModalPanelRecipe(), modalPanelWideClass)} role="dialog" aria-modal="true" aria-labelledby="apply-title">
        <div class={desktopClientModalBodyRecipe()}>
          <div>
          <h2 id="apply-title">{$t("profiles.applyTitle", { name: pendingApply.name })}</h2>
        </div>

        {#if applyError}
          <div class={profileInlineNoticeRecipe({ tone: "error" })}>{applyError}</div>
        {/if}

        {#if applyResult}
          <div class={profileInlineNoticeRecipe({ tone: "success" })}>
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
            <section class={profileDiffPanelRecipe({ tone: "warning" })}>
              <div class={profileDiffHeadingRecipe()}>
                <div>
                  <strong>{$t("envConflict.title")}</strong>
                  <span>{$t("envConflict.applyDescription", { count: applyEnvConflicts.length })}</span>
                </div>
                <button class={actionButtonRecipe()} disabled={clearingEnvConflict || applyingId !== null} on:click={clearApplyEnvConflicts}>
                  {#if clearingEnvConflict}
                    <AppIcon name="loading" class={spinRecipe()} size={16} />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {$t("envConflict.clearAction")}
                </button>
              </div>
              <div class={cx(desktopClientPreviewListRecipe(), conflictPreviewListClass)}>
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

          {#if selectedModePreview?.blockedReason || canSyncClaudeVsCodePlugin}
          <section class={profileDiffPanelRecipe()}>
            {#if selectedModePreview?.blockedReason}
              <div class={profileInlineNoticeRecipe({ tone: "error" })}>{previewTextLabel(selectedModePreview.blockedReason)}</div>
            {/if}

            {#if canSyncClaudeVsCodePlugin}
              <label class={nativeToggleRecipe()} data-native-toggle>
                <input type="checkbox" bind:checked={syncClaudeVsCodePlugin} disabled={applyingId !== null} />
                <span>
                  <strong>{$t("profiles.syncClaudeVsCode")}</strong>
                  <small>{$t("profiles.syncClaudeVsCodeDescription")}</small>
                </span>
              </label>
            {/if}
          </section>
          {/if}

          {#if selectedNativeDiffVisible && selectedNativeDiff}
            <section class={profileDiffPanelRecipe()}>
              <div class={profileDiffHeadingRecipe()}>
                <div>
                  <strong>{$t("profiles.modificationPreview")}</strong>
                  <span>{selectedNativeDiff.path}</span>
                </div>
                <StatusPill status="info" label={selectedNativeDiff.writeEnabled ? $t("common.writeEnabled") : $t("common.readOnly")} />
              </div>

              <div class={profileDiffListRecipe()}>
                {#each selectedNativeDiff.changes as change}
                  <div class={profileDiffRowRecipe()}>
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

              {#if selectedNativeDiff.content}
                <div class={profileWriteContentPreviewRecipe()}>
                  <strong>{$t("profiles.writeContentPreview")}</strong>
                  <pre>{selectedNativeDiff.content}</pre>
                </div>
              {/if}
            </section>
          {/if}
        {:else if applyingId === actionKey(pendingApply.id, pendingApplyMode)}
          <div class={cx(emptyRowRecipe(), inlineEmptyClass)}>
            <AppIcon name="loading" class={spinRecipe()} size={18} />
            {$t("common.loading")}
          </div>
        {/if}

        </div>

        <div class={desktopClientModalActionsRecipe()}>
          <button class={actionButtonRecipe()} disabled={applyingId !== null} on:click={closeApply}>
            {applyResult ? $t("common.close") : $t("common.cancel")}
          </button>
          {#if !applyResult}
            {#if selectedApplyMode === "config" && selectedModePreview?.writesNativeConfig}
              <button
                class={actionButtonRecipe()}
                disabled={applyingId !== null || !applyPreview?.canApply || !selectedModeSupported}
                on:click={() => handleApplyWithOptions(pendingApply!.id, true)}
              >
                {#if applyingId === actionKey(pendingApply.id, selectedApplyMode, true, canSyncClaudeVsCodePlugin && syncClaudeVsCodePlugin)}
                  <AppIcon name="loading" class={spinRecipe()} size={16} />
                  {$t("common.loading")}
                {:else}
                  <AppIcon name="apply" size={16} />
                  {$t("profiles.applyAndRestart")}
                {/if}
              </button>
            {/if}
            <button
              class={actionButtonRecipe({ tone: "primary" })}
              disabled={applyingId !== null || !applyPreview?.canApply || !selectedModeSupported}
              on:click={() => handleApplyWithOptions(pendingApply!.id)}
            >
              {#if applyingId === actionKey(pendingApply.id, selectedApplyMode, false, canSyncClaudeVsCodePlugin && syncClaudeVsCodePlugin)}
                <AppIcon name="loading" class={spinRecipe()} size={16} />
                {$t("common.loading")}
              {:else}
                <AppIcon name="apply" size={16} />
                {$t(selectedApplyMode === "gateway" ? "profiles.applyGatewayMode" : "profiles.applyConfigMode")}
              {/if}
            </button>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>
