<script lang="ts">
  import { afterUpdate, onMount } from "svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import DismissibleNotice from "../components/DismissibleNotice.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import {
    launchClaudeDesktop,
    restartClaudeDesktopAfterAccessibilityGrant
  } from "../lib/api";
  import { locale, t, type TranslationKey } from "../lib/i18n";
  import {
    claudeDesktopView,
    claudeDesktopVisibleInstallKinds,
    consumeClaudeDesktopPendingLaunchAfterRestart,
    dismissClaudeDesktopError,
    dismissClaudeDesktopSuccess,
    installOrUpdateClaudeDesktopKind,
    openClaudeDesktopStagingPath,
    refreshClaudeDesktop,
    removeClaudeDesktop,
    setClaudeDesktopConfirmUninstall,
    setClaudeDesktopLocalizeLaunch,
    setClaudeDesktopPendingLaunchAfterRestart,
    setClaudeDesktopSelectedKind,
    startClaudeDesktopLocalizationProgressListener,
    startClaudeDesktopProgressListener
  } from "../lib/claudeDesktopStore";
  import { css } from "../../styled-system/css";
  import {
    actionButtonRecipe,
    desktopClientActionsRecipe,
    desktopClientLogRecipe,
    desktopClientLogStageRecipe,
    desktopClientLogViewportRecipe,
    desktopClientMetricsRecipe,
    desktopClientModalActionsRecipe,
    desktopClientModalBackdropRecipe,
    desktopClientModalBodyRecipe,
    desktopClientModalPanelRecipe,
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
  import type { Severity, ToolInstallProgress } from "../types";

  $: view = $claudeDesktopView;
  $: installKinds = view.installKinds;
  $: selectedKind = view.selectedKind;
  $: isWindowsKind = view.snapshot?.platform === "windows";
  $: visibleInstallKinds = claudeDesktopVisibleInstallKinds(view);
  $: effectiveSelectedKind = visibleInstallKinds.includes(selectedKind) ? selectedKind : "msix";
  $: kindView = view.kindViews[effectiveSelectedKind];
  $: status = kindView.status;
  $: installPlan = kindView.installPlan;
  $: updatePlan = kindView.updatePlan;
  $: activePlanDetails = kindView.plan;
  $: busyAction = kindView.busyAction;
  $: progress = kindView.progress;
  $: localizationProgress = view.localizationProgress;
  $: progressPercent = progress?.percent ?? null;
  $: progressStepLabel = progress?.step && progress.stepTotal
    ? $t("claudeDesktop.progressStep", { current: progress.step, total: progress.stepTotal })
    : "";
  $: showProgress = Boolean(progress && (busyAction === "install" || busyAction === "update" || progress.phase === "done"));
  $: installed = status?.installState === "installed";
  $: statusLabel = installed ? $t("common.installed") : $t("common.missing");
  $: statusTone = (installed ? "ok" : "warning") as Severity;
  $: canInstall = !installed && Boolean(installPlan?.canInstall) && busyAction === null;
  $: canUpdate = installed && Boolean(status?.updateAvailable && updatePlan?.canInstall) && busyAction === null;
  $: canUninstall = installed && busyAction === null;
  $: canLaunch = installed && busyAction === null && !launching;
  $: activePlan = installed ? updatePlan : installPlan;
  $: activePlanAvailable = Boolean(activePlan?.canInstall);
  $: activePlanUpToDate = Boolean(installed && activePlanDetails && !status?.updateAvailable);
  $: activePlanBlocker = activePlan?.blocker && !activePlanUpToDate ? activePlan.blocker : null;
  $: activePlanStatus = !activePlanDetails
      ? $t("desktopClient.planNotLoaded")
    : activePlanBlocker
      ? activePlanBlocker
      : activePlanAvailable
        ? versionStatusHint
        : activePlanUpToDate
      ? $t("desktopClient.upToDate")
      : $t("desktopClient.planNotLoaded");
  $: liveLogGroups = groupedProgressLogs(kindView.progressLogs);
  $: hasLogs = liveLogGroups.length > 0;
  // Only call it "up to date" when the app is actually installed and we know
  // the latest version and there is no update. For a missing install we want
  // to show the latest version as available-to-install, not "up to date"; when
  // the latest is unknown we show unknown rather than a misleading up-to-date.
  $: versionStatusHint = !installed
      ? (status?.latestVersion ? $t("desktopClient.updateAvailable") : $t("common.unknown"))
    : status?.updateAvailable
      ? $t("desktopClient.updateAvailable")
      : (status?.latestVersion ? $t("desktopClient.upToDate") : $t("common.unknown"));

  $: localizeClaudeLaunch = view.localizeLaunch;
  $: exeInstallDetected = installed && status?.installKind === "exe";
  let exeWarningDismissed = false;
  $: capabilities = view.capabilities;
  $: isWindowsAppTab = isWindowsKind && effectiveSelectedKind === "msix";
  let installLogViewport: HTMLDivElement | null = null;
  let launchError: string | null = null;
  let launching = false;
  let accessibilityPromptOpen = false;
  let accessibilityRestarting = false;
  let accessibilityLaunchLocalize = false;
  let pendingLaunchConsumed = false;

  onMount(() => {
    startClaudeDesktopProgressListener();
    startClaudeDesktopLocalizationProgressListener();
    void initializeClaudeDesktopPage();
  });

  afterUpdate(() => {
    if (installLogViewport && hasLogs) {
      installLogViewport.scrollTop = installLogViewport.scrollHeight;
    }
  });

  function formatDate(value: string | null | undefined) {
    if (!value) {
      return $t("dashboard.waitingForScan");
    }
    return new Date(value).toLocaleString();
  }

  function stageLabel(stage: string) {
    if (stage === "update") {
      return $t("common.update");
    }
    if (stage === "uninstall") {
      return $t("common.uninstall");
    }
    if (stage === "prerequisite") {
      return $t("toolInstall.stage.prerequisite");
    }
    return $t("toolInstall.stage.target");
  }

  function groupedProgressLogs(logs: ToolInstallProgress[]) {
    const groups: Array<{
      key: string;
      label: string;
      command: string;
      stdout: string;
      stderr: string;
      exitCode: number | null;
      done: boolean;
    }> = [];
    const index = new Map<string, (typeof groups)[number]>();
    for (const item of logs) {
      const key = `${item.stage}:${item.toolId}:${item.command}`;
      let group = index.get(key);
      if (!group) {
        group = {
          key,
          label: `${stageLabel(item.stage)} / ${item.toolName}`,
          command: item.command,
          stdout: "",
          stderr: "",
          exitCode: null,
          done: false
        };
        index.set(key, group);
        groups.push(group);
      }
      if (item.stream === "stderr") {
        group.stderr += item.chunk;
      } else if (item.stream === "stdout") {
        group.stdout += item.chunk;
      }
      if (item.done) {
        group.done = true;
        group.exitCode = item.exitCode;
      }
    }
    return groups;
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

  function progressPhaseLabel(value: string | null | undefined) {
    if (!value) {
      return $t("claudeDesktop.phase.preparing");
    }
    const key = `claudeDesktop.phase.${value}` as Parameters<typeof $t>[0];
    const label = $t(key);
    return label === key ? value : label;
  }

  const localizationMessageFallbacks: Record<string, Record<string, string>> = {
    "claudeDesktop.localizationLaunching": {
      "zh-CN": "已启动 Claude Desktop，正在后台准备汉化...",
      "zh-TW": "已啟動 Claude Desktop，正在背景準備漢化...",
      "en-US": "Claude Desktop has launched; preparing localization in the background..."
    },
    "claudeDesktop.localizationDebugger": {
      "zh-CN": "正在检测并开启 Claude 主进程调试器...",
      "zh-TW": "正在偵測並啟用 Claude 主進程偵錯器...",
      "en-US": "Checking and enabling Claude Main Process Debugger..."
    },
    "claudeDesktop.localizationInjecting": {
      "zh-CN": "正在通过调试器注入中文界面资源...",
      "zh-TW": "正在透過偵錯器注入中文介面資源...",
      "en-US": "Injecting Chinese UI resources through the debugger..."
    },
    "claudeDesktop.localizationDone": {
      "zh-CN": "Claude Desktop 汉化已生效。",
      "zh-TW": "Claude Desktop 漢化已生效。",
      "en-US": "Claude Desktop localization is active."
    },
    "claudeDesktop.localizationFailed": {
      "zh-CN": "Claude Desktop 汉化未能自动生效。",
      "zh-TW": "Claude Desktop 漢化未能自動生效。",
      "en-US": "Claude Desktop localization did not activate automatically."
    }
  };

  function formatProgressMessage(message: string | null | undefined) {
    if (!message) {
      return $t("claudeDesktop.progressWorking");
    }
    if (message.startsWith("claudeDesktop.")) {
      const localized = $t(message as TranslationKey);
      return localized === message
        ? localizationMessageFallbacks[message]?.[$locale] ?? localizationMessageFallbacks[message]?.["en-US"] ?? localized
        : localized;
    }
    return message;
  }

  function progressByteLabel(value: ToolInstallProgress) {
    if (value.downloaded !== null && value.downloaded !== undefined && value.total !== null && value.total !== undefined) {
      return $t("claudeDesktop.progressBytes", {
        downloaded: formatBytes(value.downloaded),
        total: formatBytes(value.total)
      });
    }
    if (value.downloaded !== null && value.downloaded !== undefined) {
      return $t("claudeDesktop.progressDownloaded", {
        downloaded: formatBytes(value.downloaded)
      });
    }
    return $t("claudeDesktop.progressWorking");
  }

  async function installClaude() {
    await installOrUpdateClaudeDesktopKind(effectiveSelectedKind, "install");
  }

  async function updateClaude() {
    await installOrUpdateClaudeDesktopKind(effectiveSelectedKind, "update");
  }

  async function uninstallClaude() {
    await removeClaudeDesktop(effectiveSelectedKind);
  }

  async function launchClaude() {
    if (!canLaunch) {
      return;
    }
    await launchClaudeWithLocalization(localizeClaudeLaunch);
  }

  async function initializeClaudeDesktopPage() {
    await resumePendingLaunchAfterRestart();
  }

  async function launchClaudeWithLocalization(localize: boolean) {
    launchError = null;
    launching = true;
    try {
      await launchClaudeDesktop({ localize });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message.includes("ACCESSIBILITY_NOT_TRUSTED")) {
        accessibilityLaunchLocalize = localize;
        accessibilityPromptOpen = true;
      } else {
        launchError = message;
      }
    } finally {
      launching = false;
    }
  }

  async function resumePendingLaunchAfterRestart() {
    if (pendingLaunchConsumed) {
      return;
    }
    pendingLaunchConsumed = true;
    const pending = consumeClaudeDesktopPendingLaunchAfterRestart();
    if (!pending) {
      return;
    }
    accessibilityPromptOpen = false;
    await launchClaudeWithLocalization(pending.localize);
  }

  async function restartAfterAccessibilityGrant() {
    accessibilityRestarting = true;
    launchError = null;
    try {
      await restartClaudeDesktopAfterAccessibilityGrant({ localize: accessibilityLaunchLocalize });
    } catch (err) {
      launchError = err instanceof Error ? err.message : String(err);
      accessibilityRestarting = false;
    }
  }

  function cancelAccessibilityLaunch() {
    accessibilityPromptOpen = false;
    accessibilityRestarting = false;
    accessibilityLaunchLocalize = false;
    setClaudeDesktopPendingLaunchAfterRestart(null);
  }

  function dismissLaunchError() {
    launchError = null;
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
  const stderrClass = css({
    borderColor: "color-mix(in srgb, var(--danger) 28%, var(--border))"
  });
</script>

<div class={routeStackRecipe({ width: "desktopClient" })}>
  <section class={topStripRecipe()}>
    <div>
      <h1>{$t("claudeDesktop.title")}</h1>
      <p>{$t("claudeDesktop.subtitle")}</p>
      <div class={statusStripRecipe()}>
        <StatusPill status={statusTone} label={statusLabel} />
        <span>{view.snapshot ? $t("dashboard.lastScan", { time: formatDate(view.snapshot.generatedAt) }) : $t("dashboard.waitingForScan")}</span>
      </div>
    </div>
    <div class={topActionsRecipe()}>
      <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canLaunch || launching} title={$t("toolLaunch.actionTitle", { name: "Claude Desktop" })} on:click={launchClaude}>
        {#if launching}
          <AppIcon name="loading" size={16} class={spinRecipe()} />
          {$t("toolLaunch.starting")}
        {:else}
          <AppIcon name="play" size={16} />
          {$t("toolLaunch.action")}
        {/if}
      </button>
      <button class={actionButtonRecipe()} data-refresh-button="true" disabled={kindView.loading || busyAction !== null} on:click={() => refreshClaudeDesktop(true, effectiveSelectedKind, { forceFresh: true })}>
        <AppIcon name={kindView.loading ? "loading" : "refresh"} size={15} class={kindView.loading ? spinRecipe() : ""} />
        {$t(kindView.loading ? "common.refreshing" : "common.refresh")}
      </button>
    </div>
  </section>

  {#if view.error}
    <DismissibleNotice tone="error" message={view.error} on:dismiss={dismissClaudeDesktopError} />
  {/if}
  {#if view.success}
    <DismissibleNotice tone="success" message={formatProgressMessage(view.success)} on:dismiss={dismissClaudeDesktopSuccess} />
  {/if}
  {#if launchError}
    <DismissibleNotice tone="error" message={launchError} on:dismiss={dismissLaunchError} />
  {/if}

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("claudeDesktop.launchOptionsTitle")}</h2>
      </div>
    </div>
    <div class={desktopClientSettingsListRecipe({ layout: "grid" })}>
      <label class={nativeToggleRecipe()} data-native-toggle>
        <input
          type="checkbox"
          checked={localizeClaudeLaunch}
          disabled={launching}
          on:change={(event) => setClaudeDesktopLocalizeLaunch(event.currentTarget.checked)}
        />
        <span>
          <strong>{$t("claudeDesktop.localizeLaunch")}</strong>
        </span>
      </label>
    </div>
  </section>

  {#if isWindowsKind && installKinds && visibleInstallKinds.length > 1}
    <div class={desktopClientTabsRecipe()} role="tablist">
      {#each visibleInstallKinds as kind}
        <button
          role="tab"
          data-selected={effectiveSelectedKind === kind}
          aria-selected={effectiveSelectedKind === kind}
          on:click={() => setClaudeDesktopSelectedKind(kind)}
        >
          {kind === "msix" ? $t("desktopClient.kind.windowsApp") : $t("desktopClient.kind.exe")}
        </button>
      {/each}
    </div>
  {/if}

  {#if exeInstallDetected && !exeWarningDismissed}
    <DismissibleNotice tone="error" message={$t("claudeDesktop.exeInstallWarning")} on:dismiss={() => { exeWarningDismissed = true; }} />
  {/if}

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
        <h2>{$t("claudeDesktop.statusTitle")}</h2>
        <p>{status?.details ?? $t("claudeDesktop.notInstalled")}</p>
      </div>
      <StatusPill status={statusTone} label={statusLabel} />
    </div>

    <div class={desktopClientMetricsRecipe()}>
      <div>
                  <span>{$t("desktopClient.currentVersion")}</span>
        <strong>{status?.version ?? $t("common.none")}</strong>
        <small>{status?.command ?? "Claude"}</small>
      </div>
      <div>
                  <span>{$t("desktopClient.latestVersion")}</span>
        <strong>{status?.latestVersion ?? $t("common.unknown")}</strong>
        <small>{versionStatusHint}</small>
      </div>
      <div>
                  <span>{$t("desktopClient.installRoot")}</span>
        <strong>{status?.installPath ?? $t("common.unknown")}</strong>
        <small>{$t("claudeDesktop.managedByToolInstaller")}</small>
      </div>
      <div>
                  <span>{$t("desktopClient.configRoot")}</span>
        <strong>{status?.configPath ?? $t("common.unknown")}</strong>
        <small>{$t("claudeDesktop.managedByToolInstaller")}</small>
      </div>
    </div>

    <div class={desktopClientActionsRecipe()}>
      <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canInstall} on:click={installClaude}>
        <AppIcon name={busyAction === "install" ? "loading" : "install"} size={16} class={busyAction === "install" ? spinRecipe() : ""} />
        {busyAction === "install" ? $t("tool.installing") : $t("common.install")}
      </button>
      <button class={actionButtonRecipe()} on:click={openClaudeDesktopStagingPath}>
        <AppIcon name="folder" size={16} />
        {$t("claudeDesktop.openStagingPath")}
      </button>
      <button class={actionButtonRecipe()} disabled={!canUpdate} on:click={updateClaude}>
        <AppIcon name={busyAction === "update" ? "loading" : "update"} size={16} class={busyAction === "update" ? spinRecipe() : ""} />
        {busyAction === "update" ? $t("tool.updating") : $t("common.update")}
      </button>
      <button class={actionButtonRecipe()} disabled={!canUninstall} on:click={() => setClaudeDesktopConfirmUninstall(true)}>
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
          <span>{progressPercent === null ? $t("claudeDesktop.progressUnknown") : `${progressPercent.toFixed(0)}%`}</span>
          <span>{progressByteLabel(progress)}</span>
        </div>
      </div>
    {/if}
    {#if localizationProgress && (localizationProgress.phase !== "done" || localizationProgress.success)}
      <div class={desktopClientProgressRecipe()} aria-live="polite">
        <div data-desktop-client-progress-copy>
          <strong>{progressPhaseLabel(localizationProgress.phase)}</strong>
          <span>{formatProgressMessage(localizationProgress.error ?? localizationProgress.message)}</span>
        </div>
        <div data-desktop-client-progress-track data-indeterminate={localizationProgress.phase !== "done" && localizationProgress.phase !== "failed"}>
          <span
            data-desktop-client-progress-fill
            style={`width: ${localizationProgress.phase === "done" ? 100 : localizationProgress.phase === "failed" ? 100 : 38}%`}
          ></span>
        </div>
        <div data-desktop-client-progress-meta>
          <span>{localizationProgress.attempt}/{localizationProgress.maxAttempts}</span>
          <span>{localizationProgress.attached ?? $t("claudeDesktop.progressUnknown")}</span>
        </div>
      </div>
    {/if}
  </section>

  <section class={panelRecipe()}>
    <div class={sectionHeadingRecipe()}>
      <div class={headingCopyClass}>
            <h2>{$t("desktopClient.planTitle")}</h2>
        <p>{activePlanStatus}</p>
      </div>
      <div class={sectionActionsClass}>
        {#if activePlanDetails}
          <StatusPill
            status={activePlanAvailable ? "warning" : activePlanBlocker ? "warning" : "ok"}
              label={activePlanAvailable ? $t("desktopClient.updateAvailable") : activePlanBlocker ? $t("status.warning") : $t("desktopClient.upToDate")}
          />
        {/if}
      </div>
    </div>
    <div class={desktopClientPreviewListRecipe()}>
      {#if activePlanDetails}
        <div>
                  <strong>{$t("desktopClient.downloadUrl")}</strong>
          <span>{activePlanDetails.downloadUrl}</span>
        </div>
        <div>
          <strong>SHA-256</strong>
          <span>{activePlanDetails.sha256}</span>
        </div>
        <div>
                  <strong>{$t("desktopClient.installRoot")}</strong>
          <span>{activePlanDetails.installLocation}</span>
        </div>
      {:else}
            <div class={emptyRowRecipe()}>{$t("desktopClient.planNotLoaded")}</div>
      {/if}
    </div>
  </section>

  {#if isWindowsAppTab}
    <section class={panelRecipe()}>
      <div class={sectionHeadingRecipe()}>
        <div class={headingCopyClass}>
          <h2>{$t("claudeDesktop.capabilities")}</h2>
          <p>{$t("claudeDesktop.capabilityHint")}</p>
        </div>
      </div>
      <div class={doctorListRecipe()}>
        {#each capabilities as capability}
          <div class={doctorRowRecipe()}>
            <StatusPill status={capability.status} label={$t(`status.${capability.status}` as Parameters<typeof $t>[0])} />
            <div>
              <h3>{capability.label}</h3>
              <p>{capability.detail}</p>
            </div>
          </div>
        {:else}
          <div class={emptyRowRecipe()}>{$t("claudeDesktop.capabilityEmpty")}</div>
        {/each}
      </div>
    </section>
  {/if}

  {#if busyAction || hasLogs}
    <section class={panelRecipe()}>
      <div class={sectionHeadingRecipe()}>
        <div class={headingCopyClass}>
          <h2>{$t("toolInstall.consoleOutput")}</h2>
        </div>
      </div>
      <div class={desktopClientLogRecipe({ live: true })}>
        <div class={desktopClientLogViewportRecipe()} bind:this={installLogViewport}>
          {#each liveLogGroups as group (group.key)}
            <div class={desktopClientLogStageRecipe()}>
              <span>
                {group.label}
                {#if group.done}
                  · {$t("toolInstall.exitCode")}: {group.exitCode ?? $t("common.none")}
                {/if}
              </span>
              {#if group.stdout}
                <pre>{group.stdout}</pre>
              {/if}
              {#if group.stderr}
                <pre class={stderrClass}>{group.stderr}</pre>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    </section>
  {/if}

</div>

{#if accessibilityPromptOpen}
  <div class={desktopClientModalBackdropRecipe()}>
    <div class={desktopClientModalPanelRecipe()}>
      <div class={desktopClientModalBodyRecipe()}>
        <div>
          <h2>{$t("claudeDesktop.accessibilityTitle")}</h2>
          <p>{$t("claudeDesktop.accessibilityDescription")}</p>
        </div>
      </div>

      <div class={desktopClientModalActionsRecipe()}>
        <button class={actionButtonRecipe()} disabled={accessibilityRestarting} on:click={cancelAccessibilityLaunch}>
          {$t("common.cancel")}
        </button>
        <button class={actionButtonRecipe({ tone: "primary" })} disabled={accessibilityRestarting} on:click={restartAfterAccessibilityGrant}>
          <AppIcon name={accessibilityRestarting ? "loading" : "restart"} size={16} class={accessibilityRestarting ? spinRecipe() : ""} />
          {$t(accessibilityRestarting ? "claudeDesktop.accessibilityRestarting" : "claudeDesktop.accessibilityRestart")}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if view.confirmUninstall}
  <div class={desktopClientModalBackdropRecipe()}>
    <div class={desktopClientModalPanelRecipe()}>
      <div class={desktopClientModalBodyRecipe()}>
        <div>
          <h2>{$t("claudeDesktop.uninstallTitle")}</h2>
          <p>{$t("claudeDesktop.uninstallDescription")}</p>
        </div>
        <div class={desktopClientPreviewListRecipe()}>
          <div>
                  <strong>{$t("desktopClient.currentVersion")}</strong>
            <span>{status?.version ?? $t("common.none")}</span>
          </div>
          <div>
                  <strong>{$t("desktopClient.installRoot")}</strong>
            <span>{status?.details ?? status?.command ?? $t("common.none")}</span>
          </div>
        </div>
      </div>

      <div class={desktopClientModalActionsRecipe()}>
        <button class={actionButtonRecipe()} on:click={() => setClaudeDesktopConfirmUninstall(false)}>{$t("common.cancel")}</button>
        <button class={actionButtonRecipe({ tone: "primary" })} disabled={busyAction !== null} on:click={uninstallClaude}>
          <AppIcon name={busyAction === "uninstall" ? "loading" : "delete"} size={16} class={busyAction === "uninstall" ? spinRecipe() : ""} />
          {busyAction === "uninstall" ? $t("claudeDesktop.uninstalling") : $t("claudeDesktop.confirmUninstall")}
        </button>
      </div>
    </div>
  </div>
{/if}
