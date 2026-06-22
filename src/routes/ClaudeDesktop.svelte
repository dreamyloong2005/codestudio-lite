<script lang="ts">
  import { afterUpdate, onMount } from "svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import DismissibleNotice from "../components/DismissibleNotice.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import {
    launchClaudeDesktop
  } from "../lib/api";
  import { t } from "../lib/i18n";
  import {
    claudeDesktopView,
    dismissClaudeDesktopError,
    dismissClaudeDesktopSuccess,
    ensureClaudeDesktopLoaded,
    installOrUpdateClaudeDesktop,
    refreshClaudeDesktop,
    removeClaudeDesktop,
    setClaudeDesktopConfirmUninstall,
    setClaudeDesktopLocalizeLaunch,
    setClaudeDesktopSelectedKind,
    startClaudeDesktopProgressListener
  } from "../lib/claudeDesktopStore";
  import type { Severity, ToolInstallProgress } from "../types";

  $: view = $claudeDesktopView;
  $: status = view.status;
  $: installKinds = view.installKinds;
  $: selectedKind = view.selectedKind;
  $: isWindowsKind = view.snapshot?.platform === "windows";
  $: exeKindInstalled = Boolean(installKinds?.exe?.installed);
  // The EXE tab is hidden unless an EXE install is detected; if the user had
  // it selected and the EXE install later disappears, fall back to MSIX for
  // display without mutating the persisted selection.
  $: effectiveSelectedKind = selectedKind === "exe" && !exeKindInstalled ? "msix" : selectedKind;
  $: installPlan = view.installPlan;
  $: updatePlan = view.updatePlan;
  $: busyAction = view.busyAction;
  $: installed = status?.installState === "installed";
  $: statusLabel = installed ? $t("common.installed") : $t("common.missing");
  $: statusTone = (installed ? "ok" : "warning") as Severity;
  $: canInstall = !installed && Boolean(installPlan?.canInstall) && busyAction === null;
  $: canUpdate = installed && Boolean(status?.updateAvailable && updatePlan?.canInstall) && busyAction === null;
  $: canUninstall = installed && busyAction === null;
  $: isRunning = status?.running ?? false;
  $: canLaunch = installed && busyAction === null && !launching;
  $: liveLogGroups = groupedProgressLogs(view.progressLogs);
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

  onMount(() => {
    startClaudeDesktopProgressListener();
    void ensureClaudeDesktopLoaded();
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

  async function installClaude() {
    await installOrUpdateClaudeDesktop("install");
  }

  async function updateClaude() {
    await installOrUpdateClaudeDesktop("update");
  }

  async function uninstallClaude() {
    await removeClaudeDesktop();
  }

  async function launchClaude() {
    if (!canLaunch) {
      return;
    }
    launchError = null;
    launching = true;
    try {
      await launchClaudeDesktop({ localize: localizeClaudeLaunch });
      await new Promise((resolve) => setTimeout(resolve, 2500));
      await refreshClaudeDesktop();
    } catch (err) {
      launchError = err instanceof Error ? err.message : String(err);
    } finally {
      launching = false;
    }
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
      <button class="secondary-button" disabled={view.loading || busyAction !== null} on:click={() => refreshClaudeDesktop()}>
        <AppIcon name={view.loading ? "loading" : "refresh"} size={16} class={view.loading ? "spin" : ""} />
        {$t(view.loading ? "common.refreshing" : "common.refresh")}
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

  {#if isWindowsKind && installKinds}
    <div class="install-kind-tabs">
      <button class:active={effectiveSelectedKind === "msix"} on:click={() => setClaudeDesktopSelectedKind("msix")}>
        {$t("desktopClient.kind.windowsApp")}
        
      </button>
      {#if exeKindInstalled}
        <button class:active={effectiveSelectedKind === "exe"} on:click={() => setClaudeDesktopSelectedKind("exe")}>
          {$t("desktopClient.kind.exe")}
          </button>
      {/if}
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
      <button class="secondary-button" disabled={!canUpdate} on:click={updateClaude}>
        <AppIcon name={busyAction === "update" ? "loading" : "update"} size={16} class={busyAction === "update" ? "spin" : ""} />
        {busyAction === "update" ? $t("tool.updating") : $t("common.update")}
      </button>
      <button class="secondary-button" disabled={!canUninstall} on:click={() => setClaudeDesktopConfirmUninstall(true)}>
        <AppIcon name="delete" size={16} />
        {$t("common.uninstall")}
      </button>
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
