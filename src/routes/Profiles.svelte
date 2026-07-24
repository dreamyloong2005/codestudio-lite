<script lang="ts">
  import { onDestroy } from "svelte";
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
  import {
    canonicalProfileToolId,
    configProtocolIdsForTool,
    OFFICIAL_PROFILE_NAME_KEYS,
    PROFILE_PROTOCOL_OPTIONS,
    PROFILE_TOOL_LABELS,
    profileSupportsModelMappings as catalogSupportsModelMappings
  } from "../lib/profiles/catalog";
  import {
    emptyProfileModelMappingForm,
    modelMappingFormsFromProfile,
    modelMappingsForRequest,
    profileModelMappingsAreValid,
    type ProfileModelMappingForm
  } from "../lib/profiles/form";
  import {
    installedProfileToolIds as resolveInstalledProfileToolIds,
    profileIsActive,
    profileModeSections as buildProfileModeSections,
    shouldShowNoInstalledProfiles,
    type ProfileGroup
  } from "../lib/profiles/grouping";
  import {
    normalizedProfileIcon,
    profileDisplayName as resolveProfileDisplayName,
    profileIconIsImage,
    profileIconTextTooLong,
    profileModelOptionLabel,
    providerIsOfficial
  } from "../lib/profiles/presentation";
  import AppIcon from "../components/AppIcon.svelte";
  import DismissibleNotice from "../components/DismissibleNotice.svelte";
  import ModelSelectInput from "../components/ModelSelectInput.svelte";
  import ProfileList from "../components/profiles/ProfileList.svelte";
  import ProfileToolTabs from "../components/profiles/ProfileToolTabs.svelte";
  import StatusPill from "../components/StatusPill.svelte";
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
    profileDiffHeadingRecipe,
    profileDiffListRecipe,
    profileDiffPanelRecipe,
    profileDiffRowRecipe,
    profileEmbeddedStackRecipe,
    profileFieldErrorRecipe,
    profileFormGridRecipe,
    profileIconActionsRecipe,
    profileIconEditorRecipe,
    profileInlineNoticeRecipe,
    profileModeLayoutRecipe,
    profileModeSwitcherRecipe,
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
  export let onProfileSwitched: (profile?: ProfileDraft) => void | Promise<void> = () => {};
  export let onCreateProfile: (prefill?: WizardPrefill) => void = () => {};

  const profileViewOptions: Array<{ value: ProviderApplyMode; labelKey: TranslationKey }> = [
    { value: "config", labelKey: "profiles.view.config" },
    { value: "gateway", labelKey: "profiles.view.gateway" }
  ];

  type EditProfileForm = {
    name: string;
    icon: string;
    remark: string;
    mode: ProviderApplyMode;
    provider: string;
    protocol: string;
    model: string;
    reviewModel: string;
    modelMappings: ProfileModelMappingForm[];
    baseUrl: string;
    apiKey: string;
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

  const toolLabels = PROFILE_TOOL_LABELS;
  const officialProfileNameKeys = OFFICIAL_PROFILE_NAME_KEYS as Record<string, TranslationKey>;
  const protocolOptions = PROFILE_PROTOCOL_OPTIONS;
  type ProtocolOption = (typeof protocolOptions)[number];
  const usageTemplateOptions: Array<{ id: UsageScriptTemplateType; labelKey: TranslationKey }> = [
    { id: "general", labelKey: "profiles.usage.template.general" },
    { id: "newapi", labelKey: "profiles.usage.template.newapi" },
    { id: "balance", labelKey: "profiles.usage.template.balance" },
    { id: "token_plan", labelKey: "profiles.usage.template.tokenPlan" },
    { id: "custom", labelKey: "profiles.usage.template.custom" }
  ];

  $: installedProfileToolIds = resolveInstalledProfileToolIds(snapshot);
  $: normalizedModeFilter = (modeFilter === "gateway" ? "gateway" : "config") as ProviderApplyMode;
  $: profileModeSections = buildProfileModeSections(summary, installedProfileToolIds, normalizedModeFilter);
  $: profileToolGroups = profileModeSections.flatMap((section) => section.groups);
  $: syncSelectedProfileTool(profileToolGroups);
  $: selectedProfileGroup = profileToolGroups.find((group) => group.id === selectedToolId) ?? null;
  $: visibleProfileCount = selectedProfileGroup?.profiles.length ?? 0;
  $: selectedModePreview =
    applyPreview?.modePreviews.find((mode) => mode.mode === selectedApplyMode) ?? null;
  $: selectedNativeDiff = selectedModePreview?.nativeDiff ?? null;
  $: selectedNativeDiffVisible = Boolean(
    selectedNativeDiff?.writeEnabled && selectedNativeDiff.changes.length > 0
  );
  $: selectedModeSupported = selectedModePreview?.supported ?? false;
  $: canApplyAndRestart =
    pendingApply?.mode === "config" &&
    selectedApplyMode === "config" &&
    Boolean(selectedModePreview?.writesNativeConfig);
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
  $: editSupportsModelMappings = Boolean(pendingEdit) && catalogSupportsModelMappings(pendingEdit?.app ?? "");
  $: editSupportsReviewModel = Boolean(pendingEdit) && canonicalProfileToolId(pendingEdit?.app ?? "") === "codex";
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
    if (profileIsActive(summary, profile)) {
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
      reviewModel: "",
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
      reviewModel: profile.reviewModel ?? "",
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
      const updated = await updateProfileDraft({
        profileId: pendingEdit.id,
        name: editForm.name,
        icon: normalizedProfileIcon(editForm.icon),
        remark: editForm.remark,
        mode: pendingEdit.mode,
        provider: editForm.provider,
        protocol: editForm.protocol,
        model: editForm.model,
        reviewModel: editSupportsReviewModel ? editForm.reviewModel.trim() || null : null,
        modelMappings: modelMappingsForRequest(pendingEdit.app, editForm.modelMappings),
        baseUrl: normalizeBaseUrl(editForm.baseUrl),
        apiKey: editForm.apiKey.trim().length > 0 ? editForm.apiKey : null
      });
      await onProfileSwitched(updated);
      pendingEdit = null;
      editForm = emptyEditForm();
    } catch (err) {
      editError = errorLabel(err instanceof Error ? err.message : String(err));
    } finally {
      editingId = null;
    }
  }

  async function handleApplyWithOptions(profileId: string, restartAfterApply = false) {
    if (pendingApply && profileIsActive(summary, pendingApply)) {
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
      await onProfileSwitched(duplicated);
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

  const modelOptionLabel = profileModelOptionLabel;


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

  function profileDisplayName(profile: ProfileDraft) {
    const nameKey = officialProfileNameKeys[canonicalProfileToolId(profile.app)];
    return resolveProfileDisplayName(profile, nameKey ? $t(nameKey) : undefined);
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

  async function persistProfileOrder(profileIds: string[]) {
    if (!selectedProfileGroup) return;
    try {
      profileIoError = null;
      profileIoMessage = null;
      const nextSummary = await reorderProfileDrafts({
        app: selectedProfileGroup.id,
        mode: normalizedModeFilter,
        profileIds
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
      profileIoError = errorLabel(err instanceof Error ? err.message : String(err));
      throw err;
    }
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
      "OpenCode custom providers are written to opencode.json using the OpenAI-compatible provider package.": "profiles.warning.opencodeConfigWrites",
      "OpenClaw providers are written in models.mode=merge so existing provider definitions can stay available.": "profiles.warning.openclawConfigWrites",
      "Hermes custom providers are written to ~/.hermes/config.yaml under the model section.": "profiles.warning.hermesConfigWrites",
      "Grok custom models are written to ~/.grok/config.toml under [models] and [model.codestudio].": "profiles.warning.grokConfigWrites",
      "Restart Grok or open a new session after applying so the model catalog reloads.": "profiles.warning.grokRestart",
      "Official provider removes CodeStudio Lite managed Pi Agent provider entries.": "profiles.warning.piOfficialRestore",
      "Pi Agent custom providers are written to ~/.pi/agent/models.json.": "profiles.warning.piConfigWrites",
      "Gateway profiles write Pi Agent provider settings to the tool-scoped local gateway URL.": "profiles.warning.piGatewayWrites",
      "Open /model in Pi after applying to select the managed provider model.": "profiles.warning.piSelectModel",
      "Existing JSON comments are not preserved when CodeStudio Lite writes the file.": "profiles.warning.jsonCommentsLost",
      "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file.": "profiles.warning.jsoncCommentsLost",
      "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file.": "profiles.warning.json5CommentsLost",
      "Existing YAML comments are not preserved when CodeStudio Lite writes the file.": "profiles.warning.yamlCommentsLost",
      "Existing TOML comments are not preserved when CodeStudio Lite writes the file.": "profiles.warning.tomlCommentsLost",
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
      "Points OpenCode at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayOpenCodeBaseUrl",
      "Points OpenClaw at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayOpenClawBaseUrl",
      "Points Hermes at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.gatewayHermesBaseUrl",
      "Sets Claude Code to the virtual model name resolved by the Local Gateway.": "profiles.diff.gatewayClaudeModel",
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
      "Model is optional; no Hermes model override will be written.": "profiles.diff.hermesModelOptional",
      "Points Pi Agent at the selected upstream Provider Base URL.": "profiles.diff.piBaseUrl",
      "Selects the Pi Agent API adapter for this provider.": "profiles.diff.piApiAdapter",
      "Stores the selected Provider API key for Pi Agent.": "profiles.diff.piApiKey",
      "Registers the selected model under the managed Pi provider.": "profiles.diff.piModelRegistration",
      "Uses system-role prompts for broader OpenAI-compatible endpoint support.": "profiles.diff.piSystemRoleCompat",
      "Avoids unsupported reasoning_effort fields on compatible endpoints.": "profiles.diff.piReasoningCompat",
      "Removes CodeStudio Lite managed Pi Agent provider entries.": "profiles.diff.piRemoveManagedProvider",
      "Deletes the managed CodeStudio Lite Pi provider.": "profiles.diff.piRemoveManagedProvider",
      "Points Pi Agent at the tool-scoped CodeStudio Lite Local Gateway.": "profiles.diff.piGatewayBaseUrl",
      "Uses OpenAI Chat Completions against the Local Gateway.": "profiles.diff.gatewayOpenAiCompletions",
      "Uses system-role prompts for Local Gateway compatibility.": "profiles.diff.piGatewaySystemRole",
      "Registers the virtual gateway model under the managed Pi provider.": "profiles.diff.piGatewayModelRegistration"
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

  function emptyProfilesMessageKey(
    profileSummary: ProfileSummary | null,
    visibleCount: number,
    installedToolIds: Set<string> | null
  ): TranslationKey {
    if (shouldShowNoInstalledProfiles(profileSummary, visibleCount, installedToolIds)) {
      return "profiles.noInstalledToolProfiles";
    }
    return "profiles.noProfiles";
  }

  function protocolOptionsFor(toolId: string, mode: ProviderApplyMode): readonly ProtocolOption[] {
    if (mode === "gateway") {
      return protocolOptions;
    }
    const supportedIds = configProtocolIdsForTool(toolId);
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
  function handleModalEscape(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    if (pendingUsageProfile && usageBusy === null) {
      closeUsage();
    } else if (pendingEdit && editingId === null) {
      closeEdit();
    } else if (pendingDelete && deletingId === null) {
      closeDelete();
    } else if (pendingApply && applyingId === null) {
      closeApply();
    } else {
      return;
    }
    event.preventDefault();
  }

  function handleModalEnter(event: KeyboardEvent) {
    if (event.key !== "Enter" || keyboardTargetOwnsEnter(event.target)) return;
    if (pendingUsageProfile && canSaveUsage && usageBusy === null) {
      event.preventDefault();
      void handleUsageSave();
    } else if (pendingEdit && canSaveEdit) {
      event.preventDefault();
      void handleEditSave();
    } else if (pendingDelete && deletingId === null) {
      event.preventDefault();
      void handleDeleteConfirm();
    } else if (
      pendingApply &&
      !applyResult &&
      applyingId === null &&
      Boolean(applyPreview?.canApply) &&
      selectedModeSupported
    ) {
      event.preventDefault();
      void handleApplyWithOptions(pendingApply.id);
    }
  }

  function keyboardTargetOwnsEnter(target: EventTarget | null) {
    if (!(target instanceof HTMLElement)) return false;
    return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName);
  }
</script>

<svelte:window on:keydown={handleModalEscape} on:keydown={handleModalEnter} />

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
        <ProfileToolTabs
          groups={profileToolGroups}
          {selectedToolId}
          activeProfileLabel={loginTypeLabel}
          onSelect={selectProfileTool}
        />

        {#if selectedProfileGroup}
          <ProfileList
            profiles={selectedProfileGroup.profiles}
            activeProfileId={selectedProfileGroup.activeProfileId}
            toolId={selectedProfileGroup.id}
            mode={normalizedModeFilter}
            {applyingId}
            {duplicatingId}
            {deletingId}
            {editingId}
            onApply={openApply}
            onUsage={openUsage}
            onEdit={openEdit}
            onDuplicate={handleDuplicate}
            onDelete={openDelete}
            onReorder={persistProfileOrder}
          />
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
          {#if editSupportsReviewModel}
            <div class={modelPickerClass}>
              <label for={`${editModelListId}-review-input`}>{$t("profiles.reviewModelLabel")}</label>
              <ModelSelectInput
                id={`${editModelListId}-review-input`}
                bind:value={editForm.reviewModel}
                options={editModelOptions}
                optionLabel={modelOptionLabel}
                toggleTitle={$t("profiles.reviewModelLabel")}
                placeholder={$t("profiles.reviewModelPlaceholder")}
                disabled={editingId !== null}
              />
            </div>
          {/if}
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
            {#if canApplyAndRestart}
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
