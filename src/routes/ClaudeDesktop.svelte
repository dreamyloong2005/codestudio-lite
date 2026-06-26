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
  import { t, type TranslationKey } from "../lib/i18n";
  import {
    claudeDesktopView,
    claudeDesktopVisibleInstallKinds,
    consumeClaudeDesktopPendingLaunchAfterRestart,
    dismissClaudeDesktopError,
    dismissClaudeDesktopSuccess,
    ensureClaudeDesktopLoaded,
    installOrUpdateClaudeDesktopKind,
    openClaudeDesktopStagingPath,
    refreshClaudeDesktop,
    removeClaudeDesktop,
    setClaudeDesktopConfirmUninstall,
    setClaudeDesktopLocalizeLaunch,
    setClaudeDesktopPendingLaunchAfterRestart,
    setClaudeDesktopSelectedKind,
    startClaudeDesktopProgressListener
  } from "../lib/claudeDesktopStore";
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
  $: planRefreshing = kindView.planRefreshing;
  $: busyAction = kindView.busyAction;
  $: progress = kindView.progress;
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
  $: isRunning = status?.running ?? false;
  $: canLaunch = installed && busyAction === null && !launching;
  $: activePlan = installed ? updatePlan : installPlan;
  $: activePlanAvailable = Boolean(activePlan?.canInstall);
  $: activePlanUpToDate = Boolean(installed && activePlanDetails && !status?.updateAvailable);
  $: activePlanBlocker = activePlan?.blocker && !activePlanUpToDate ? activePlan.blocker : null;
  $: activePlanStatus = !activePlanDetails
    ? $t("codexClient.planNotLoaded")
    : activePlanBlocker
      ? activePlanBlocker
      : activePlanAvailable
        ? versionStatusHint
        : activePlanUpToDate
          ? $t("codexClient.upToDate")
          : $t("codexClient.planNotLoaded");
  $: liveLogGroups = groupedProgressLogs(kindView.progressLogs);
  $: hasLogs = liveLogGroups.length > 0;
  // Only call it "up to date" when the app is actually installed and we know
  // the latest version and there is no update. For a missing install we want
  // to show the latest version as available-to-install, not "up to date"; when
  // the latest is unknown we show unknown rather than a misleading up-to-date.
  $: versionStatusHint = !installed
    ? (status?.latestVersion ? $t("codexClient.updateAvailable") : $t("common.unknown"))
    : status?.updateAvailable
      ? $t("codexClient.updateAvailable")
      : (status?.latestVersion ? $t("codexClient.upToDate") : $t("common.unknown"));

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

  function formatProgressMessage(message: string | null | undefined) {
    if (!message) {
      return $t("claudeDesktop.progressWorking");
    }
    if (message.startsWith("claudeDesktop.")) {
      return $t(message as TranslationKey);
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
    await ensureClaudeDesktopLoaded();
    await resumePendingLaunchAfterRestart();
  }

  async function launchClaudeWithLocalization(localize: boolean) {
    launchError = null;
    launching = true;
    try {
      await launchClaudeDesktop({ localize });
      await new Promise((resolve) => setTimeout(resolve, 2500));
      await refreshClaudeDesktop();
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
</script>

<div class="route-stack codex-client-route">
  <section class="top-strip">
    <div>
      <span class="eyebrow">{$t("claudeDesktop.eyebrow")}</span>
      <h1>{$t("claudeDesktop.title")}</h1>
      <p>{$t("claudeDesktop.subtitle")}</p>
      <div class="status-strip">
        <StatusPill status={statusTone} label={statusLabel} />
        <span>{view.snapshot ? $t("dashboard.lastScan", { time: formatDate(view.snapshot.generatedAt) }) : $t("dashboard.waitingForScan")}</span>
      </div>
    </div>
    <div class="top-actions">
      <button class="primary-button" disabled={!canLaunch} title={$t(isRunning ? "toolLaunch.restartTitle" : "toolLaunch.actionTitle", { name: "Claude Desktop" })} on:click={launchClaude}>
        {#if launching}
          <AppIcon name="loading" size={16} class="spin" />
          {$t(isRunning ? "toolLaunch.restarting" : "toolLaunch.starting")}
        {:else}
          <AppIcon name="play" size={16} />
          {$t(isRunning ? "toolLaunch.restart" : "toolLaunch.action")}
        {/if}
      </button>
      <button class="secondary-button" disabled={kindView.loading || busyAction !== null} on:click={() => refreshClaudeDesktop(false, effectiveSelectedKind)}>
        <AppIcon name={kindView.loading ? "loading" : "refresh"} size={16} class={kindView.loading ? "spin" : ""} />
        {$t(kindView.loading ? "common.refreshing" : "common.refresh")}
      </button>
    </div>
  </section>

  {#if view.error}
    <DismissibleNotice tone="error" message={view.error} on:dismiss={dismissClaudeDesktopError} />
  {/if}
  {#if view.success}
    <DismissibleNotice tone="success" message={view.success} on:dismiss={dismissClaudeDesktopSuccess} />
  {/if}
  {#if launchError}
    <DismissibleNotice tone="error" message={launchError} on:dismiss={dismissLaunchError} />
  {/if}

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("claudeDesktop.launchOptionsTitle")}</h2>
      </div>
    </div>
    <div class="settings-list codex-client-settings launch-options-grid">
      <label class="native-write-toggle">
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
    <div class="install-kind-tabs">
      {#each visibleInstallKinds as kind}
        <button class:active={effectiveSelectedKind === kind} on:click={() => setClaudeDesktopSelectedKind(kind)}>
          {kind === "msix" ? $t("desktopClient.kind.windowsApp") : $t("desktopClient.kind.exe")}
        </button>
      {/each}
    </div>
  {/if}

  {#if exeInstallDetected && !exeWarningDismissed}
    <DismissibleNotice tone="error" message={$t("claudeDesktop.exeInstallWarning")} on:dismiss={() => { exeWarningDismissed = true; }} />
  {/if}

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("claudeDesktop.statusTitle")}</h2>
        <p>{status?.details ?? $t("claudeDesktop.notInstalled")}</p>
      </div>
      <StatusPill status={statusTone} label={statusLabel} />
    </div>

    <div class="gateway-metrics codex-client-metrics">
      <div>
        <span>{$t("codexClient.currentVersion")}</span>
        <strong>{status?.version ?? $t("common.none")}</strong>
        <small>{status?.command ?? "Claude"}</small>
      </div>
      <div>
        <span>{$t("codexClient.latestVersion")}</span>
        <strong>{status?.latestVersion ?? $t("common.unknown")}</strong>
        <small>{versionStatusHint}</small>
      </div>
      <div>
        <span>{$t("codexClient.installRoot")}</span>
        <strong>{status?.installPath ?? $t("common.unknown")}</strong>
        <small>{$t("claudeDesktop.managedByToolInstaller")}</small>
      </div>
      <div>
        <span>{$t("codexClient.configRoot")}</span>
        <strong>{status?.configPath ?? $t("common.unknown")}</strong>
        <small>{$t("claudeDesktop.managedByToolInstaller")}</small>
      </div>
    </div>

    <div class="gateway-actions codex-client-actions">
      <button class="primary-button" disabled={!canInstall} on:click={installClaude}>
        <AppIcon name={busyAction === "install" ? "loading" : "install"} size={16} class={busyAction === "install" ? "spin" : ""} />
        {busyAction === "install" ? $t("tool.installing") : $t("common.install")}
      </button>
      <button class="secondary-button" on:click={openClaudeDesktopStagingPath}>
        <AppIcon name="folder" size={16} />
        {$t("claudeDesktop.openStagingPath")}
      </button>
      <button class="secondary-button" disabled={!canUpdate} on:click={updateClaude}>
        <AppIcon name={busyAction === "update" ? "loading" : "update"} size={16} class={busyAction === "update" ? "spin" : ""} />
        {busyAction === "update" ? $t("tool.updating") : $t("common.update")}
      </button>
      <button class="secondary-button" disabled={!canUninstall} on:click={() => setClaudeDesktopConfirmUninstall(true)}>
        <AppIcon name="delete" size={16} />
        {$t("common.uninstall")}
      </button>
    </div>
    {#if showProgress && progress}
      <div class="install-progress" aria-live="polite">
        <div class="progress-copy">
          <strong>{progressStepLabel ? `${progressStepLabel} / ${progressPhaseLabel(progress.phase)}` : progressPhaseLabel(progress.phase)}</strong>
          <span>{formatProgressMessage(progress.message)}</span>
        </div>
        <div class="progress-track" class:indeterminate={progressPercent === null}>
          <span
            class="progress-fill"
            style={`width: ${progressPercent === null ? 38 : Math.max(2, Math.min(100, progressPercent)).toFixed(1)}%`}
          ></span>
        </div>
        <div class="progress-meta">
          <span>{progressPercent === null ? $t("claudeDesktop.progressUnknown") : `${progressPercent.toFixed(0)}%`}</span>
          <span>{progressByteLabel(progress)}</span>
        </div>
      </div>
    {/if}
  </section>

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("codexClient.planTitle")}</h2>
        <p>{planRefreshing && activePlanDetails ? $t("codexClient.planRefreshing") : activePlanStatus}</p>
      </div>
      <div class="section-actions">
        {#if activePlanDetails}
          <StatusPill
            status={activePlanAvailable ? "warning" : activePlanBlocker ? "warning" : "ok"}
            label={activePlanAvailable ? $t("codexClient.updateAvailable") : activePlanBlocker ? $t("status.warning") : $t("codexClient.upToDate")}
          />
          {#if planRefreshing}
            <StatusPill status="info" label={$t("codexClient.planRefreshing")} />
          {/if}
        {/if}
      </div>
    </div>
    <div class="preview-list codex-client-list">
      {#if activePlanDetails}
        {#if planRefreshing}
          <div class="empty-row">
            <AppIcon name="loading" class="spin" size={18} />
            {$t("codexClient.planRefreshing")}
          </div>
        {/if}
        <div>
          <strong>{$t("codexClient.downloadUrl")}</strong>
          <span>{activePlanDetails.downloadUrl}</span>
        </div>
        <div>
          <strong>SHA-256</strong>
          <span>{activePlanDetails.sha256}</span>
        </div>
        <div>
          <strong>{$t("codexClient.installRoot")}</strong>
          <span>{activePlanDetails.installLocation}</span>
        </div>
      {:else}
        <div class="empty-row">{$t("codexClient.planNotLoaded")}</div>
      {/if}
    </div>
  </section>

  {#if isWindowsAppTab}
    <section class="panel-band">
      <div class="section-heading">
        <div>
          <h2>{$t("claudeDesktop.capabilities")}</h2>
          <p>{$t("claudeDesktop.capabilityHint")}</p>
        </div>
      </div>
      <div class="doctor-list">
        {#each capabilities as capability}
          <div class="doctor-row">
            <StatusPill status={capability.status} label={$t(`status.${capability.status}` as Parameters<typeof $t>[0])} />
            <div>
              <h3>{capability.label}</h3>
              <p>{capability.detail}</p>
            </div>
          </div>
        {:else}
          <div class="empty-row">{$t("claudeDesktop.capabilityEmpty")}</div>
        {/each}
      </div>
    </section>
  {/if}

  {#if busyAction || hasLogs}
    <section class="panel-band">
      <div class="section-heading">
        <div>
          <h2>{$t("toolInstall.consoleOutput")}</h2>
        </div>
      </div>
      <div class="install-log live-install-log">
        <div class="install-log-viewport" bind:this={installLogViewport}>
          {#each liveLogGroups as group (group.key)}
            <div class="install-log-stage">
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
                <pre class="stderr">{group.stderr}</pre>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    </section>
  {/if}

</div>

{#if accessibilityPromptOpen}
  <div class="modal-backdrop">
    <div class="modal-panel">
      <div class="modal-body">
        <div>
          <span class="eyebrow">{$t("claudeDesktop.accessibilityEyebrow")}</span>
          <h2>{$t("claudeDesktop.accessibilityTitle")}</h2>
          <p>{$t("claudeDesktop.accessibilityDescription")}</p>
        </div>
      </div>

      <div class="modal-actions">
        <button class="secondary-button" disabled={accessibilityRestarting} on:click={cancelAccessibilityLaunch}>
          {$t("common.cancel")}
        </button>
        <button class="primary-button" disabled={accessibilityRestarting} on:click={restartAfterAccessibilityGrant}>
          <AppIcon name={accessibilityRestarting ? "loading" : "restart"} size={16} class={accessibilityRestarting ? "spin" : ""} />
          {$t(accessibilityRestarting ? "claudeDesktop.accessibilityRestarting" : "claudeDesktop.accessibilityRestart")}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if view.confirmUninstall}
  <div class="modal-backdrop">
    <div class="modal-panel">
      <div class="modal-body">
        <div>
          <span class="eyebrow">{$t("claudeDesktop.uninstallEyebrow")}</span>
          <h2>{$t("claudeDesktop.uninstallTitle")}</h2>
          <p>{$t("claudeDesktop.uninstallDescription")}</p>
        </div>
        <div class="preview-list">
          <div>
            <strong>{$t("codexClient.currentVersion")}</strong>
            <span>{status?.version ?? $t("common.none")}</span>
          </div>
          <div>
            <strong>{$t("codexClient.installRoot")}</strong>
            <span>{status?.details ?? status?.command ?? $t("common.none")}</span>
          </div>
        </div>
      </div>

      <div class="modal-actions">
        <button class="secondary-button" on:click={() => setClaudeDesktopConfirmUninstall(false)}>{$t("common.cancel")}</button>
        <button class="primary-button" disabled={busyAction !== null} on:click={uninstallClaude}>
          <AppIcon name={busyAction === "uninstall" ? "loading" : "delete"} size={16} class={busyAction === "uninstall" ? "spin" : ""} />
          {busyAction === "uninstall" ? $t("claudeDesktop.uninstalling") : $t("claudeDesktop.confirmUninstall")}
        </button>
      </div>
    </div>
  </div>
{/if}
