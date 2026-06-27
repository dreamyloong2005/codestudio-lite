<script lang="ts">
  import { onMount } from "svelte";
  import {
    openCodexClientPath,
  } from "../lib/api";
  import {
    codexClientView,
    installOrUpdateCodexClient,
    launchManagedCodexClient,
    refreshCodexClient,
    removeCodexClient,
    setCodexClientConfirmUninstall,
    setCodexClientSelectedKind,
    stageCodexClientPackage,
    startCodexClientProgressListener,
    updateCodexClientDraft,
    type CodexClientNoticeMessage
  } from "../lib/codexClientStore";
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
    eyebrowRecipe,
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
    CodexClientProgress,
    Severity
  } from "../types";

  $: view = $codexClientView;
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
  $: installed = state?.installed ?? null;
  $: plan = state?.plan ?? null;
  $: release = state?.release ?? null;
  $: planRefreshing = kindView.planRefreshing;
  $: planUnavailable = kindView.planStale;
  $: planUnavailableText = $t("codexClient.planStale");
  $: effectivePlan = planUnavailable ? null : plan;
  $: effectiveRelease = planUnavailable ? null : release;
  $: platform = state?.platform ?? view.kindViews.msix.state?.platform ?? view.kindViews.portable.state?.platform;
  $: isWindows = platform === "windows";
  $: isMacos = platform === "macos";
  $: statusLabel = installed ? $t("common.installed") : $t("common.missing");
  $: statusTone = (installed ? "ok" : "warning") as Severity;
  $: canStage = Boolean(effectivePlan && !effectivePlan.upToDate && effectivePlan.route !== "unsupported");
  $: canInstall = canStage;
  $: isRunning = state?.running ?? false;
  $: canLaunch = Boolean(installed);
  $: canUninstall = Boolean(installed);
  $: progressPercent = progress?.percent ?? null;
  $: progressStepLabel = progress?.step && progress.stepTotal
    ? $t("codexClient.progressStep", { current: progress.step, total: progress.stepTotal })
    : "";
  $: showProgress = Boolean(progress && (busyAction === "stage" || busyAction === "install" || progress.phase === "done"));

  onMount(() => {
    startCodexClientProgressListener();
  });

  async function stagePackage() {
    await stageCodexClientPackage();
  }

  async function installOrUpdate() {
    await installOrUpdateCodexClient();
  }

  async function removeCodex() {
    await removeCodexClient();
  }

  async function launchCodex() {
    await launchManagedCodexClient();
  }

  async function refreshCodex() {
    await refreshCodexClient();
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
    const key = `codexClient.phase.${value}` as Parameters<typeof $t>[0];
    const label = $t(key);
    return label === key ? value : label;
  }

  function formatNoticeMessage(message: CodexClientNoticeMessage | null) {
    if (!message) {
      return "";
    }
    if (typeof message === "string") {
      return message;
    }
    return $t(message.key, message.values);
  }

  function formatProgressMessage(message: string) {
    if (message.startsWith("codexClient.")) {
      return $t(message as TranslationKey);
    }
    return message;
  }

  function progressByteLabel(value: CodexClientProgress) {
    if (value.downloaded !== null && value.total !== null) {
      return $t("codexClient.progressBytes", {
        downloaded: formatBytes(value.downloaded),
        total: formatBytes(value.total)
      });
    }
    if (value.downloaded !== null) {
      return $t("codexClient.progressDownloaded", {
        downloaded: formatBytes(value.downloaded)
      });
    }
    return $t("codexClient.progressWorking");
  }

  function dismissError() {
    codexClientView.update((current) => ({ ...current, error: null }));
  }

  function dismissSuccess() {
    codexClientView.update((current) => ({ ...current, success: null }));
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
</script>

<div class={routeStackRecipe({ width: "desktopClient" })}>
  <section class={topStripRecipe()}>
    <div>
      <span class={eyebrowRecipe()}>{$t("codexClient.eyebrow")}</span>
      <h1>{$t("codexClient.title")}</h1>
      <p>{$t("codexClient.subtitle")}</p>
      <div class={statusStripRecipe()}>
        <StatusPill status={statusTone} label={statusLabel} />
        <span>{state ? $t("dashboard.lastScan", { time: new Date(state.generatedAt).toLocaleString() }) : $t("dashboard.waitingForScan")}</span>
      </div>
    </div>
    <div class={topActionsRecipe()}>
      <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canLaunch || busyAction !== null} on:click={launchCodex}>
        <AppIcon name="play" size={16} />
        {$t(isRunning ? "codexClient.restart" : "codexClient.launch")}
      </button>
      <button class={actionButtonRecipe()} disabled={kindView.loading || busyAction !== null} on:click={refreshCodex}>
        <AppIcon name={kindView.loading ? "loading" : "refresh"} size={16} class={kindView.loading ? spinRecipe() : ""} />
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
        on:click={() => setCodexClientSelectedKind("msix")}
      >
        {$t("desktopClient.kind.windowsApp")}
      </button>
      <button
        role="tab"
        data-selected={effectiveSelectedKind === "portable"}
        aria-selected={effectiveSelectedKind === "portable"}
        on:click={() => setCodexClientSelectedKind("portable")}
      >
        {$t("desktopClient.kind.exe")}
      </button>
    </div>
  {/if}

  {#if error}
    <DismissibleNotice tone="error" message={error} on:dismiss={dismissError} />
  {/if}
  {#if success}
    <DismissibleNotice tone="success" message={formatNoticeMessage(success)} on:dismiss={dismissSuccess} />
  {/if}

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("codexClient.launchOptionsTitle")}</h2>
      </div>
    </div>
    {#if settingsDraft}
      <div class={desktopClientSettingsListRecipe({ layout: "grid" })}>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.syncHistoryOnLaunch}
            on:change={(event) => updateCodexClientDraft({ syncHistoryOnLaunch: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("codexClient.syncHistoryOnLaunch")}</strong>
          </span>
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.patchForcePluginUnlock}
            on:change={(event) => updateCodexClientDraft({ patchForcePluginUnlock: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("codexClient.patchForcePluginUnlock")}</strong>
          </span>
        </label>
      </div>
    {/if}
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("codexClient.statusTitle")}</h2>
        <p>{installed?.path ?? $t("codexClient.notInstalled")}</p>
      </div>
      <StatusPill status={statusTone} label={statusLabel} />
    </div>
    <div class={desktopClientMetricsRecipe()}>
      <div>
        <span>{$t("codexClient.currentVersion")}</span>
        <strong>{installed?.version ?? $t("common.none")}</strong>
        <small>{installed?.source ?? $t("common.unknown")}</small>
      </div>
      <div>
        <span>{$t("codexClient.latestVersion")}</span>
        <strong>{effectiveRelease?.version ?? $t("common.unknown")}</strong>
        <small>{planUnavailable ? planUnavailableText : effectiveRelease?.packageMoniker ?? $t("codexClient.planNotLoaded")}</small>
      </div>
      <div>
        <span>{$t("codexClient.packageSize")}</span>
        <strong>{formatBytes(effectivePlan?.downloadSize ?? effectiveRelease?.contentLength)}</strong>
        <small>{effectiveRelease?.sha256 ? `${effectiveRelease.sha256.slice(0, 12)}...` : $t("common.unknown")}</small>
      </div>
    </div>
    <div class={desktopClientActionsRecipe()}>
      <button class={actionButtonRecipe()} disabled={!canStage || busyAction !== null} on:click={stagePackage}>
        <AppIcon name="download" size={16} />
        {busyAction === "stage" ? $t("codexClient.staging") : $t("codexClient.stage")}
      </button>
      <button class={actionButtonRecipe()} on:click={() => openCodexClientPath("staging")}>
        <AppIcon name="folder" size={16} />
        {$t("codexClient.openStagingPath")}
      </button>
      <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canInstall || busyAction !== null} on:click={installOrUpdate}>
        <AppIcon name="rocket" size={16} />
        {busyAction === "install" ? $t("codexClient.installing") : installed ? $t("codexClient.update") : $t("codexClient.install")}
      </button>
      <button class={actionButtonRecipe()} disabled={!canUninstall || busyAction !== null} on:click={() => setCodexClientConfirmUninstall(true)}>
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
          <span>{progressPercent === null ? $t("codexClient.progressUnknown") : `${progressPercent.toFixed(0)}%`}</span>
          <span>{progressByteLabel(progress)}</span>
        </div>
      </div>
    {/if}
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("codexClient.planTitle")}</h2>
        <p>{planUnavailable ? planUnavailableText : effectiveRelease?.manifestUrl ?? $t("codexClient.planNotLoaded")}</p>
      </div>
      <div class={sectionActionsClass}>
        {#if planUnavailable}
          <StatusPill status="info" label={planUnavailableText} />
        {:else if effectivePlan}
          <StatusPill status={effectivePlan.upToDate ? "ok" : "warning"} label={effectivePlan.upToDate ? $t("codexClient.upToDate") : $t("codexClient.updateAvailable")} />
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
          <strong>{$t("codexClient.downloadUrl")}</strong>
          <span>{effectivePlan.packageUrl}</span>
        </div>
        <div>
          <strong>SHA-256</strong>
          <span>{effectivePlan.sha256}</span>
        </div>
        <div>
          <strong>{$t("codexClient.installRoot")}</strong>
          <span>{effectivePlan.installRoot ?? $t("common.none")}</span>
        </div>
        {#if stageReport}
          <div>
            <strong>{$t("codexClient.stageReport")}</strong>
            <span>
              {stageReport.stagedPath ?? $t("common.none")} / {formatBytes(stageReport.downloadSize)}
              / {stageReport.hashVerified ? $t("codexClient.hashVerified") : $t("common.error")}
            </span>
          </div>
        {/if}
        {#if operationResult}
          <div>
            <strong>{$t("codexClient.lastOperation")}</strong>
            <span>{operationResult.action} / {operationResult.notes.join(" ")}</span>
          </div>
        {/if}
        {#each effectivePlan.warnings as warning}
          <div class={warningRowClass}>
            <strong>{$t("status.warning")}</strong>
            <span>{warning}</span>
          </div>
        {/each}
      {:else}
        <div class={emptyRowRecipe()}>{$t("codexClient.planNotLoaded")}</div>
      {/if}
    </div>
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("codexClient.capabilities")}</h2>
        <p>{$t("codexClient.capabilityHint")}</p>
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
              <h3>{capability.label}</h3>
              <p>{capability.detail}</p>
            </div>
          </div>
        {:else}
          <div class={emptyRowRecipe()}>{$t("codexClient.capabilityEmpty")}</div>
        {/each}
      {/if}
    </div>
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("codexClient.settingsTitle")}</h2>
        <p>{$t("codexClient.settingsHint")}</p>
      </div>
    </div>
    {#if settingsDraft}
      <div class={desktopClientSettingsListRecipe()}>
        {#if isMacos}
          <label>
            {$t("codexClient.source")}
            <select
              value={settingsDraft.source}
              on:change={(event) => updateCodexClientDraft({ source: event.currentTarget.value as "mirror" | "official" })}
            >
              <option value="mirror">{$t("codexClient.source.mirror")}</option>
              <option value="official">{$t("codexClient.source.official")}</option>
            </select>
          </label>
        {/if}
        <label>
          {$t("codexClient.installRoot")}
          <input
            value={settingsDraft.installRoot}
            on:input={(event) => updateCodexClientDraft({ installRoot: event.currentTarget.value })}
          />
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.autoCheck}
            on:change={(event) => updateCodexClientDraft({ autoCheck: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("codexClient.autoCheck")}</strong>
            <small>{$t("codexClient.autoCheckHint")}</small>
          </span>
        </label>
        <label class={nativeToggleRecipe()} data-native-toggle>
          <input
            type="checkbox"
            checked={settingsDraft.keepUserDataOnUninstall}
            on:change={(event) => updateCodexClientDraft({ keepUserDataOnUninstall: event.currentTarget.checked })}
          />
          <span>
            <strong>{$t("codexClient.keepUserData")}</strong>
            <small>{$t("codexClient.keepUserDataHint")}</small>
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
        <span class={eyebrowRecipe()}>{$t("codexClient.uninstallEyebrow")}</span>
        <h2>{$t("codexClient.uninstallTitle")}</h2>
        <p>{$t("codexClient.uninstallDescription")}</p>
      </div>
      <div class={desktopClientPreviewListRecipe()}>
        <div>
          <strong>{$t("codexClient.currentVersion")}</strong>
          <span>{installed?.version ?? $t("common.none")} / {installed?.path ?? $t("common.none")}</span>
        </div>
        <div>
          <strong>{$t("codexClient.keepUserData")}</strong>
          <span>{settingsDraft?.keepUserDataOnUninstall ? $t("common.enabled") : $t("common.disabled")}</span>
        </div>
      </div>
      </div>

      <div class={desktopClientModalActionsRecipe()}>
        <button class={actionButtonRecipe()} on:click={() => setCodexClientConfirmUninstall(false)}>{$t("common.cancel")}</button>
        <button class={actionButtonRecipe({ tone: "primary" })} disabled={busyAction !== null} on:click={removeCodex}>
          <AppIcon name="delete" size={16} />
          {busyAction === "uninstall" ? $t("codexClient.uninstalling") : $t("codexClient.confirmUninstall")}
        </button>
      </div>
    </div>
  </div>
{/if}
