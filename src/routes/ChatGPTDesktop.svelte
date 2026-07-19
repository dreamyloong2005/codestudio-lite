<script lang="ts">
  import { onMount } from "svelte";
  import {
    openChatGPTDesktopPath,
  } from "../lib/api";
  import {
    chatgptDesktopView,
    applyChatGPTHistoryIndexCleanup,
    installOrUpdateChatGPTDesktop,
    launchManagedChatGPTDesktop,
    loadChatGPTHistorySyncManagement,
    previewChatGPTHistoryIndexCleanup,
    refreshChatGPTDesktop,
    removeChatGPTDesktop,
    runChatGPTHistorySync,
    setChatGPTDesktopConfirmUninstall,
    setChatGPTDesktopSelectedKind,
    stageChatGPTDesktopPackage,
    startChatGPTDesktopProgressListener,
    updateChatGPTDesktopDraft,
    type ChatGPTDesktopNoticeMessage
  } from "../lib/chatgptDesktopStore";
  import {
    brandChatGPTDesktopText,
    chatgptDesktopGeneration
  } from "../lib/chatgptDesktopBranding";
  import { t, type TranslationKey } from "../lib/i18n";
  import AppIcon from "../components/AppIcon.svelte";
  import DismissibleNotice from "../components/DismissibleNotice.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import { css, cx } from "../../styled-system/css";
  import {
    actionButtonRecipe,
    desktopClientActionsRecipe,
    desktopClientModalActionsRecipe,
    desktopClientModalBackdropRecipe,
    desktopClientModalBodyRecipe,
    desktopClientModalPanelRecipe,
    desktopClientMetricsRecipe,
    desktopClientPreviewListRecipe,
    desktopClientProgressRecipe,
    desktopClientSettingsListRecipe,
    desktopClientTabsRecipe,
    doctorListRecipe,
    doctorRowRecipe,
    emptyRowRecipe,
    nativeToggleRecipe,
    panelRecipe,
    routeStackRecipe,
    sectionHeadingRecipe,
    spinRecipe,
    statusStripRecipe,
    topActionsRecipe,
    topStripRecipe
  } from "../../styled-system/recipes";
  import type {
    ChatGPTDesktopProgress,
    Severity
  } from "../types";

  $: view = $chatgptDesktopView;
  $: installKinds = view.installKinds;
  $: selectedKind = view.selectedKind;
  $: effectiveSelectedKind = selectedKind;
  $: kindView = view.kindViews[effectiveSelectedKind];
  $: state = kindView.state;
  $: settingsDraft = view.settingsDraft;
  $: busyAction = kindView.busyAction;
  $: error = view.error;
  $: success = view.success;
  $: stageReport = kindView.stageReport;
  $: operationResult = kindView.operationResult;
  $: progress = kindView.progress;
  $: confirmUninstall = view.confirmUninstall;
  $: historySyncTargets = view.historySyncTargets;
  $: historySyncResult = view.historySyncResult;
  $: historySyncBusyAction = view.historySyncBusyAction;
  $: sessionIndexCleanupPreview = view.sessionIndexCleanupPreview;
  $: sessionIndexCleanupResult = view.sessionIndexCleanupResult;
  $: installed = state?.installed ?? null;
  $: plan = state?.plan ?? null;
  $: release = state?.release ?? null;
  $: planRefreshing = kindView.planRefreshing;
  $: planUnavailable = kindView.planStale;
  $: planUnavailableText = $t("chatgptDesktop.planStale");
  $: effectivePlan = planUnavailable ? null : plan;
  $: effectiveRelease = planUnavailable ? null : release;
  $: platform = state?.platform ?? view.kindViews.msix.state?.platform ?? view.kindViews.portable.state?.platform;
  $: isWindows = platform === "windows";
  $: isMacos = platform === "macos";
  $: statusLabel = installed ? $t("common.installed") : $t("common.missing");
  $: statusTone = (installed ? "ok" : "warning") as Severity;
  $: canStage = Boolean(effectivePlan && !effectivePlan.upToDate && effectivePlan.route !== "unsupported");
  $: canInstall = canStage;
  $: canLaunch = Boolean(installed);
  $: canUninstall = Boolean(installed);
  $: progressPercent = progress?.percent ?? null;
  $: progressStepLabel = progress?.step && progress.stepTotal
    ? $t("chatgptDesktop.progressStep", { current: progress.step, total: progress.stepTotal })
    : "";
  $: showProgress = Boolean(progress && (
    busyAction === "stage"
    || busyAction === "install"
    || progress.phase === "done"
    || progress.phase === "error"
  ));
  let selectedCleanupThreadIds = new Set<string>();

  onMount(() => {
    startChatGPTDesktopProgressListener();
    void loadChatGPTHistorySyncManagement();
  });

  function brandDesktopText(value: string) {
    return brandChatGPTDesktopText(value, $chatgptDesktopGeneration);
  }

  async function stagePackage() {
    await stageChatGPTDesktopPackage();
  }

  async function installOrUpdate() {
    await installOrUpdateChatGPTDesktop();
  }

  async function removeCodex() {
    await removeChatGPTDesktop();
  }

  async function launchCodex() {
    await launchManagedChatGPTDesktop();
  }

  async function refreshCodex() {
    await refreshChatGPTDesktop();
  }

  async function previewIndexCleanup() {
    selectedCleanupThreadIds = new Set();
    await previewChatGPTHistoryIndexCleanup();
  }

  function toggleCleanupThread(id: string, checked: boolean) {
    const next = new Set(selectedCleanupThreadIds);
    if (checked) {
      next.add(id);
    } else {
      next.delete(id);
    }
    selectedCleanupThreadIds = next;
  }

  function toggleAllCleanupThreads(checked: boolean) {
    selectedCleanupThreadIds = checked
      ? new Set(sessionIndexCleanupPreview?.candidates.map((candidate) => candidate.id) ?? [])
      : new Set();
  }

  async function applyIndexCleanup() {
    await applyChatGPTHistoryIndexCleanup([...selectedCleanupThreadIds]);
    selectedCleanupThreadIds = new Set();
  }

  function formatBytes(value: number | null | undefined) {
    if (!value) {
      return $t("common.unknown");
    }
    const units = ["B", "KB", "MB", "GB"];
    let size = value;
    let unit = 0;
    while (size >= 1024 && unit < units.length - 1) {
      size /= 1024;
      unit += 1;
    }
    return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
  }

  function progressPhaseLabel(value: string) {
    if (value === "error") {
      return $t("common.error");
    }
    const key = `chatgptDesktop.phase.${value}` as Parameters<typeof $t>[0];
    const label = $t(key);
    return label === key ? value : label;
  }

  function formatNoticeMessage(message: ChatGPTDesktopNoticeMessage | null) {
    if (!message) {
      return "";
    }
    if (typeof message === "string") {
      return message;
    }
    return $t(message.key, message.values);
  }

  function formatProgressMessage(message: string) {
    if (message.startsWith("chatgptDesktop.")) {
      return $t(message as TranslationKey);
    }
    return message;
  }

  function progressByteLabel(value: ChatGPTDesktopProgress) {
    if (value.downloaded !== null && value.total !== null) {
      return $t("chatgptDesktop.progressBytes", {
        downloaded: formatBytes(value.downloaded),
        total: formatBytes(value.total)
      });
    }
    if (value.downloaded !== null) {
      return $t("chatgptDesktop.progressDownloaded", {
        downloaded: formatBytes(value.downloaded)
      });
    }
    return $t("chatgptDesktop.progressWorking");
  }

  function dismissError() {
    chatgptDesktopView.update((current) => ({ ...current, error: null }));
  }

  function dismissSuccess() {
    chatgptDesktopView.update((current) => ({ ...current, success: null }));
  }

  const headingCopyClass = css({
    minWidth: 0
  });
  const sectionActionsClass = css({
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: "9px",
    flexWrap: "wrap",
    minWidth: 0
  });
  const inlineEmptyRowClass = css({
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    gap: "8px"
  });
  const warningRowClass = css({
    borderColor: "color-mix(in srgb, var(--amber) 35%, transparent) !important",
    background: "color-mix(in srgb, var(--amber) 8%, var(--surface-strong)) !important"
  });
  const historyControlsClass = css({
    display: "grid",
    gridTemplateColumns: "minmax(220px, 1fr) auto",
    gap: "10px",
    alignItems: "end",
    mdDown: { gridTemplateColumns: "1fr" }
  });
  const historyFieldClass = css({
    display: "grid",
    gap: "7px",
    minWidth: 0,
    '& > span': { color: "var(--text-muted)", fontSize: "0.78rem" },
    '& > input': {
      width: "100%",
      minWidth: 0,
      border: "1px solid var(--line)",
      background: "var(--surface-strong)",
      color: "var(--text)",
      padding: "9px 10px"
    }
  });
  const historyWarningClass = css({
    marginTop: "12px",
    borderLeft: "3px solid var(--amber)",
    padding: "9px 11px",
    color: "var(--text)",
    background: "color-mix(in srgb, var(--amber) 8%, transparent)",
    fontSize: "0.82rem",
    lineHeight: 1.55
  });
  const cleanupActionsClass = css({
    display: "flex",
    gap: "9px",
    alignItems: "center",
    justifyContent: "space-between",
    flexWrap: "wrap",
    marginTop: "14px"
  });
  const historyDangerButtonClass = css({
    borderColor: "color-mix(in srgb, var(--danger) 45%, transparent) !important",
    color: "var(--danger) !important",
    _hover: { background: "color-mix(in srgb, var(--danger) 9%, var(--surface-hover)) !important" }
  });
</script>

<div class={routeStackRecipe({ width: "desktopClient" })}>
  <section class={topStripRecipe()}>
    <div>
      <h1>{$t("chatgptDesktop.title")}</h1>
      <p>{$t("chatgptDesktop.subtitle")}</p>
      <div class={statusStripRecipe()}>
        <StatusPill status={statusTone} label={statusLabel} />
        <span>{state ? $t("dashboard.lastScan", { time: new Date(state.generatedAt).toLocaleString() }) : $t("dashboard.waitingForScan")}</span>
      </div>
    </div>
    <div class={topActionsRecipe()}>
      <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canLaunch || busyAction !== null} on:click={launchCodex}>
        {#if busyAction === "launch"}
          <AppIcon name="loading" size={16} class={spinRecipe()} />
          {$t("toolLaunch.starting")}
        {:else}
          <AppIcon name="play" size={16} />
          {$t("chatgptDesktop.launch")}
        {/if}
      </button>
      <button class={actionButtonRecipe()} data-refresh-button="true" disabled={kindView.loading || busyAction !== null} on:click={refreshCodex}>
        <AppIcon name={kindView.loading ? "loading" : "refresh"} size={15} class={kindView.loading ? spinRecipe() : ""} />
        {$t(kindView.loading ? "common.refreshing" : "common.refresh")}
      </button>
    </div>
  </section>

  {#if isWindows && installKinds}
    <div class={desktopClientTabsRecipe()} role="tablist">
      <button
        role="tab"
        data-selected={effectiveSelectedKind === "msix"}
        aria-selected={effectiveSelectedKind === "msix"}
        on:click={() => setChatGPTDesktopSelectedKind("msix")}
      >
        {$t("desktopClient.kind.windowsApp")}
      </button>
      <button
        role="tab"
        data-selected={effectiveSelectedKind === "portable"}
        aria-selected={effectiveSelectedKind === "portable"}
        on:click={() => setChatGPTDesktopSelectedKind("portable")}
      >
        {$t("desktopClient.kind.exe")}
      </button>
    </div>
  {/if}

  {#if error}
    <DismissibleNotice tone="error" message={brandDesktopText(error)} on:dismiss={dismissError} />
  {/if}
  {#if success}
    <DismissibleNotice tone="success" message={brandDesktopText(formatNoticeMessage(success))} on:dismiss={dismissSuccess} />
  {/if}

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("chatgptDesktop.launchOptionsTitle")}</h2>
      </div>
    </div>
    {#if settingsDraft}
      <div class={desktopClientSettingsListRecipe({ layout: "grid" })}>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.syncHistoryOnLaunch}
            on:change={(event) => updateChatGPTDesktopDraft({ syncHistoryOnLaunch: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("chatgptDesktop.syncHistoryOnLaunch")}</strong>
          </span>
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.pluginMarketplaceUnlockOnLaunch}
            on:change={(event) => updateChatGPTDesktopDraft({ pluginMarketplaceUnlockOnLaunch: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("chatgptDesktop.pluginMarketplaceUnlockOnLaunch")}</strong>
          </span>
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.officialRemotePluginCacheOnLaunch}
            on:change={(event) => updateChatGPTDesktopDraft({ officialRemotePluginCacheOnLaunch: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("chatgptDesktop.officialRemotePluginCacheOnLaunch")}</strong>
            <small>{$t("chatgptDesktop.officialRemotePluginCacheOnLaunchHint")}</small>
          </span>
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.pluginAutoExpandOnLaunch}
            on:change={(event) => updateChatGPTDesktopDraft({ pluginAutoExpandOnLaunch: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("chatgptDesktop.pluginAutoExpandOnLaunch")}</strong>
          </span>
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.modelWhitelistUnlockOnLaunch}
            on:change={(event) => updateChatGPTDesktopDraft({ modelWhitelistUnlockOnLaunch: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("chatgptDesktop.modelWhitelistUnlockOnLaunch")}</strong>
          </span>
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.serviceTierControlsOnLaunch}
            on:change={(event) => updateChatGPTDesktopDraft({ serviceTierControlsOnLaunch: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("chatgptDesktop.serviceTierControlsOnLaunch")}</strong>
          </span>
        </label>
        {#if isWindows}
          <label class={nativeToggleRecipe()} data-native-toggle>
            <input
              type="checkbox"
              checked={settingsDraft.computerUseGuardOnLaunch}
              on:change={(event) => updateChatGPTDesktopDraft({ computerUseGuardOnLaunch: event.currentTarget.checked })}
            />
            <span>
              <strong>{$t("chatgptDesktop.computerUseGuardOnLaunch")}</strong>
              <small>{$t("chatgptDesktop.computerUseGuardOnLaunchHint")}</small>
            </span>
          </label>
        {/if}
      </div>
    {/if}
  </section>

  <section class={panelRecipe()} data-history-sync-management>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("chatgptDesktop.historySyncTitle")}</h2>
        <p>{$t("chatgptDesktop.historySyncHint")}</p>
      </div>
      {#if historySyncResult}
        <StatusPill
          status={historySyncResult.status === "synced" ? "ok" : "warning"}
          label={$t(`chatgptDesktop.historySyncStatus.${historySyncResult.status}` as TranslationKey)}
        />
      {/if}
    </div>
    <div class={historyControlsClass}>
      <label class={historyFieldClass}>
        <span>{$t("chatgptDesktop.historySyncProvider")}</span>
        <input
          list="chatgpt-history-provider-targets"
          value={settingsDraft?.historySyncTargetProvider ?? historySyncTargets?.currentProvider ?? ""}
          on:input={(event) => updateChatGPTDesktopDraft({ historySyncTargetProvider: event.currentTarget.value })}
          placeholder={historySyncTargets?.currentProvider ?? "openai"}
          disabled={historySyncBusyAction !== null}
        />
        <datalist id="chatgpt-history-provider-targets">
          {#each historySyncTargets?.targets ?? [] as target}
            <option value={target.id}>{target.sources.join(" / ")}</option>
          {/each}
        </datalist>
      </label>
      <button
        class={actionButtonRecipe({ tone: "primary" })}
        disabled={historySyncBusyAction !== null}
        on:click={runChatGPTHistorySync}
      >
        <AppIcon name={historySyncBusyAction === "sync" ? "loading" : "refresh"} class={historySyncBusyAction === "sync" ? spinRecipe() : ""} size={16} />
        {$t(historySyncBusyAction === "sync" ? "chatgptDesktop.historySyncRunning" : "chatgptDesktop.historySyncNow")}
      </button>
    </div>
    {#if historySyncResult}
      <div class={desktopClientPreviewListRecipe()}>
        <div><strong>{$t("chatgptDesktop.historySyncFiles")}</strong><span>{historySyncResult.changedSessionFiles}</span></div>
        <div><strong>{$t("chatgptDesktop.historySyncDatabaseRows")}</strong><span>{historySyncResult.sqliteRowsUpdated}</span></div>
        <div><strong>{$t("chatgptDesktop.historySyncWorkspaceFields")}</strong><span>{historySyncResult.updatedWorkspaceRoots}</span></div>
        <div><strong>{$t("chatgptDesktop.historySyncBackup")}</strong><span>{historySyncResult.backupDir ?? $t("common.none")}</span></div>
      </div>
      {#if historySyncResult.encryptedContentWarning}
        <div class={historyWarningClass}>{historySyncResult.encryptedContentWarning}</div>
      {/if}
    {/if}
    <div class={cleanupActionsClass}>
      <div>
        <strong>{$t("chatgptDesktop.sessionIndexCleanupTitle")}</strong>
        <p>{$t("chatgptDesktop.sessionIndexCleanupHint")}</p>
      </div>
      <button class={actionButtonRecipe()} disabled={historySyncBusyAction !== null} on:click={previewIndexCleanup}>
        <AppIcon name={historySyncBusyAction === "preview" ? "loading" : "eye"} class={historySyncBusyAction === "preview" ? spinRecipe() : ""} size={16} />
        {$t("chatgptDesktop.sessionIndexPreview")}
      </button>
    </div>
    {#if sessionIndexCleanupPreview}
      {#if sessionIndexCleanupPreview.candidates.length > 0}
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={selectedCleanupThreadIds.size === sessionIndexCleanupPreview.candidates.length}
            on:change={(event) => toggleAllCleanupThreads(event.currentTarget.checked)}
          />
          <span><strong>{$t("chatgptDesktop.sessionIndexSelectAll")}</strong></span>
        </label>
        <div class={desktopClientSettingsListRecipe()}>
          {#each sessionIndexCleanupPreview.candidates as candidate}
            <label class={nativeToggleRecipe()} data-native-toggle>
              <input
                type="checkbox"
                checked={selectedCleanupThreadIds.has(candidate.id)}
                on:change={(event) => toggleCleanupThread(candidate.id, event.currentTarget.checked)}
              />
              <span>
                <strong>{candidate.threadName || candidate.id}</strong>
                <small>{candidate.id}{candidate.updatedAt ? ` / ${candidate.updatedAt}` : ""}</small>
              </span>
            </label>
          {/each}
        </div>
        <div class={cleanupActionsClass}>
          <span>{$t("chatgptDesktop.sessionIndexSelected", { count: selectedCleanupThreadIds.size })}</span>
          <button
            class={cx(actionButtonRecipe(), historyDangerButtonClass)}
            disabled={historySyncBusyAction !== null || selectedCleanupThreadIds.size === 0}
            on:click={applyIndexCleanup}
          >
            <AppIcon name={historySyncBusyAction === "cleanup" ? "loading" : "delete"} class={historySyncBusyAction === "cleanup" ? spinRecipe() : ""} size={16} />
            {$t("chatgptDesktop.sessionIndexClean")}
          </button>
        </div>
      {:else}
        <div class={emptyRowRecipe()}>{$t("chatgptDesktop.sessionIndexEmpty")}</div>
      {/if}
    {/if}
    {#if sessionIndexCleanupResult}
      <div class={historyWarningClass}>
        {$t("chatgptDesktop.sessionIndexCleaned", { count: sessionIndexCleanupResult.prunedEntries })}
        {sessionIndexCleanupResult.backupDir}
      </div>
    {/if}
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("chatgptDesktop.statusTitle")}</h2>
        <p>{installed?.path ?? $t("chatgptDesktop.notInstalled")}</p>
      </div>
      <StatusPill status={statusTone} label={statusLabel} />
    </div>
    <div class={desktopClientMetricsRecipe()}>
      <div>
        <span>{$t("chatgptDesktop.currentVersion")}</span>
        <strong>{installed?.version ?? $t("common.none")}</strong>
        <small>{installed?.source ?? $t("common.unknown")}</small>
      </div>
      <div>
        <span>{$t("chatgptDesktop.latestVersion")}</span>
        <strong>{effectiveRelease?.version ?? $t("common.unknown")}</strong>
        <small>{planUnavailable ? planUnavailableText : effectiveRelease?.packageMoniker ?? $t("chatgptDesktop.planNotLoaded")}</small>
      </div>
      <div>
        <span>{$t("chatgptDesktop.packageSize")}</span>
        <strong>{formatBytes(effectivePlan?.downloadSize ?? effectiveRelease?.contentLength)}</strong>
        <small>{effectiveRelease?.sha256 ? `${effectiveRelease.sha256.slice(0, 12)}...` : $t("common.unknown")}</small>
      </div>
    </div>
    <div class={desktopClientActionsRecipe()}>
      <button class={actionButtonRecipe()} disabled={!canStage || busyAction !== null} on:click={stagePackage}>
        <AppIcon name="download" size={16} />
        {busyAction === "stage" ? $t("chatgptDesktop.staging") : $t("chatgptDesktop.stage")}
      </button>
      <button class={actionButtonRecipe()} on:click={() => openChatGPTDesktopPath("staging")}>
        <AppIcon name="folder" size={16} />
        {$t("chatgptDesktop.openStagingPath")}
      </button>
      <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canInstall || busyAction !== null} on:click={installOrUpdate}>
        <AppIcon name="rocket" size={16} />
        {busyAction === "install" ? $t("chatgptDesktop.installing") : installed ? $t("chatgptDesktop.update") : $t("chatgptDesktop.install")}
      </button>
      <button class={actionButtonRecipe()} disabled={!canUninstall || busyAction !== null} on:click={() => setChatGPTDesktopConfirmUninstall(true)}>
        <AppIcon name="delete" size={16} />
        {$t("common.uninstall")}
      </button>
    </div>
    {#if showProgress && progress}
      <div class={desktopClientProgressRecipe()} aria-live="polite">
        <div data-desktop-client-progress-copy>
          <strong>{progressStepLabel ? `${progressStepLabel} / ${progressPhaseLabel(progress.phase)}` : progressPhaseLabel(progress.phase)}</strong>
          <span>{formatProgressMessage(progress.message)}</span>
        </div>
        <div data-desktop-client-progress-track data-indeterminate={progressPercent === null}>
          <span
            data-desktop-client-progress-fill
            style={`width: ${progressPercent === null ? 38 : Math.max(2, Math.min(100, progressPercent)).toFixed(1)}%`}
          ></span>
        </div>
        <div data-desktop-client-progress-meta>
          <span>{progressPercent === null ? $t("chatgptDesktop.progressUnknown") : `${progressPercent.toFixed(0)}%`}</span>
          <span>{progressByteLabel(progress)}</span>
        </div>
      </div>
    {/if}
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("chatgptDesktop.planTitle")}</h2>
        <p>{planUnavailable ? planUnavailableText : effectiveRelease?.manifestUrl ?? $t("chatgptDesktop.planNotLoaded")}</p>
      </div>
      <div class={sectionActionsClass}>
        {#if planUnavailable}
          <StatusPill status="info" label={planUnavailableText} />
        {:else if effectivePlan}
          <StatusPill status={effectivePlan.upToDate ? "ok" : "warning"} label={effectivePlan.upToDate ? $t("chatgptDesktop.upToDate") : $t("chatgptDesktop.updateAvailable")} />
        {/if}
      </div>
    </div>
    <div class={desktopClientPreviewListRecipe()}>
      {#if planUnavailable}
        <div class={cx(emptyRowRecipe(), inlineEmptyRowClass)}>
          <AppIcon name={planRefreshing ? "loading" : "info"} class={planRefreshing ? spinRecipe() : ""} size={18} />
          {planUnavailableText}
        </div>
      {:else if effectivePlan}
        <div>
          <strong>{$t("chatgptDesktop.downloadUrl")}</strong>
          <span>{effectivePlan.packageUrl}</span>
        </div>
        <div>
          <strong>SHA-256</strong>
          <span>{effectivePlan.sha256}</span>
        </div>
        <div>
          <strong>{$t("chatgptDesktop.installRoot")}</strong>
          <span>{effectivePlan.installRoot ?? $t("common.none")}</span>
        </div>
        {#if stageReport}
          <div>
            <strong>{$t("chatgptDesktop.stageReport")}</strong>
            <span>
              {stageReport.stagedPath ?? $t("common.none")} / {formatBytes(stageReport.downloadSize)}
              / {stageReport.hashVerified ? $t("chatgptDesktop.hashVerified") : $t("common.error")}
            </span>
          </div>
        {/if}
        {#if operationResult}
          <div>
            <strong>{$t("chatgptDesktop.lastOperation")}</strong>
            <span>{operationResult.action} / {brandDesktopText(operationResult.notes.join(" "))}</span>
          </div>
        {/if}
        {#each effectivePlan.warnings as warning}
          <div class={warningRowClass}>
            <strong>{$t("status.warning")}</strong>
            <span>{brandDesktopText(warning)}</span>
          </div>
        {/each}
      {:else}
        <div class={emptyRowRecipe()}>{$t("chatgptDesktop.planNotLoaded")}</div>
      {/if}
    </div>
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("chatgptDesktop.capabilities")}</h2>
        <p>{$t("chatgptDesktop.capabilityHint")}</p>
      </div>
    </div>
    <div class={doctorListRecipe()}>
      {#if planUnavailable}
        <div class={cx(emptyRowRecipe(), inlineEmptyRowClass)}>
          <AppIcon name={planRefreshing ? "loading" : "info"} class={planRefreshing ? spinRecipe() : ""} size={18} />
          {planUnavailableText}
        </div>
      {:else}
        {#each effectivePlan?.capabilities ?? [] as capability}
          <div class={doctorRowRecipe()}>
            <StatusPill status={capability.status} label={$t(`status.${capability.status}` as Parameters<typeof $t>[0])} />
            <div>
              <h3>{brandDesktopText(capability.label)}</h3>
              <p>{brandDesktopText(capability.detail)}</p>
            </div>
          </div>
        {:else}
          <div class={emptyRowRecipe()}>{$t("chatgptDesktop.capabilityEmpty")}</div>
        {/each}
      {/if}
    </div>
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("chatgptDesktop.settingsTitle")}</h2>
        <p>{$t("chatgptDesktop.settingsHint")}</p>
      </div>
    </div>
    {#if settingsDraft}
      <div class={desktopClientSettingsListRecipe()}>
        {#if isMacos}
          <label>
            {$t("chatgptDesktop.source")}
            <select
              value={settingsDraft.source}
              on:change={(event) => updateChatGPTDesktopDraft({ source: event.currentTarget.value as "mirror" | "official" })}
            >
              <option value="mirror">{$t("chatgptDesktop.source.mirror")}</option>
              <option value="official">{$t("chatgptDesktop.source.official")}</option>
            </select>
          </label>
        {/if}
        <label>
          {$t("chatgptDesktop.installRoot")}
          <input
            value={settingsDraft.installRoot}
            on:input={(event) => updateChatGPTDesktopDraft({ installRoot: event.currentTarget.value })}
          />
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.autoCheck}
            on:change={(event) => updateChatGPTDesktopDraft({ autoCheck: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("chatgptDesktop.autoCheck")}</strong>
            <small>{$t("chatgptDesktop.autoCheckHint")}</small>
          </span>
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.keepUserDataOnUninstall}
            on:change={(event) => updateChatGPTDesktopDraft({ keepUserDataOnUninstall: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("chatgptDesktop.keepUserData")}</strong>
            <small>{$t("chatgptDesktop.keepUserDataHint")}</small>
          </span>
        </label>
      </div>
    {/if}
  </section>

</div>

{#if confirmUninstall}
  <div class={desktopClientModalBackdropRecipe()}>
    <div class={desktopClientModalPanelRecipe()}>
      <div class={desktopClientModalBodyRecipe()}>
        <div>
        <h2>{$t("chatgptDesktop.uninstallTitle")}</h2>
        <p>{$t("chatgptDesktop.uninstallDescription")}</p>
      </div>
      <div class={desktopClientPreviewListRecipe()}>
        <div>
          <strong>{$t("chatgptDesktop.currentVersion")}</strong>
          <span>{installed?.version ?? $t("common.none")} / {installed?.path ?? $t("common.none")}</span>
        </div>
        <div>
          <strong>{$t("chatgptDesktop.keepUserData")}</strong>
          <span>{settingsDraft?.keepUserDataOnUninstall ? $t("common.enabled") : $t("common.disabled")}</span>
        </div>
      </div>
      </div>

      <div class={desktopClientModalActionsRecipe()}>
        <button class={actionButtonRecipe()} on:click={() => setChatGPTDesktopConfirmUninstall(false)}>{$t("common.cancel")}</button>
        <button class={actionButtonRecipe({ tone: "primary" })} disabled={busyAction !== null} on:click={removeCodex}>
          <AppIcon name="delete" size={16} />
          {busyAction === "uninstall" ? $t("chatgptDesktop.uninstalling") : $t("chatgptDesktop.confirmUninstall")}
        </button>
      </div>
    </div>
  </div>
{/if}
