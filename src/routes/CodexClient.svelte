<script lang="ts">
  import { onMount } from "svelte";
  import {
    Download,
    FolderOpen,
    Play,
    Rocket,
    Trash2
  } from "@lucide/svelte";
  import {
    openCodexClientPath,
  } from "../lib/api";
  import {
    codexClientView,
    ensureCodexClientLoaded,
    installOrUpdateCodexClient,
    launchManagedCodexClient,
    removeCodexClient,
    setCodexClientConfirmUninstall,
    stageCodexClientPackage,
    startCodexClientProgressListener,
    updateCodexClientDraft
  } from "../lib/codexClientStore";
  import { t } from "../lib/i18n";
  import StatusPill from "../components/StatusPill.svelte";
  import type {
    CodexClientProgress,
    Severity
  } from "../types";

  $: view = $codexClientView;
  $: state = view.state;
  $: settingsDraft = view.settingsDraft;
  $: busyAction = view.busyAction;
  $: error = view.error;
  $: success = view.success;
  $: stageReport = view.stageReport;
  $: operationResult = view.operationResult;
  $: progress = view.progress;
  $: confirmUninstall = view.confirmUninstall;
  $: installed = state?.installed ?? null;
  $: plan = state?.plan ?? null;
  $: release = state?.release ?? null;
  $: isWindows = state?.platform === "windows";
  $: isMacos = state?.platform === "macos";
  $: statusLabel = installed ? $t("common.installed") : $t("common.missing");
  $: statusTone = (installed ? "ok" : "warning") as Severity;
  $: canStage = Boolean(plan && !plan.upToDate && plan.route !== "unsupported");
  $: canInstall = canStage;
  $: canLaunch = Boolean(installed);
  $: canUninstall = Boolean(installed);
  $: progressPercent = progress?.percent ?? null;
  $: progressStepLabel = progress?.step && progress.stepTotal
    ? $t("codexClient.progressStep", { current: progress.step, total: progress.stepTotal })
    : "";
  $: showProgress = Boolean(progress && (busyAction === "stage" || busyAction === "install" || progress.phase === "done"));

  onMount(() => {
    startCodexClientProgressListener();
    void ensureCodexClientLoaded();
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
</script>

<div class="route-stack codex-client-route">
  <section class="top-strip">
    <div>
      <span class="eyebrow">{$t("codexClient.eyebrow")}</span>
      <h1>{$t("codexClient.title")}</h1>
      <p>{$t("codexClient.subtitle")}</p>
      <div class="status-strip">
        <StatusPill status={statusTone} label={statusLabel} />
        <span>{state ? $t("dashboard.lastScan", { time: new Date(state.generatedAt).toLocaleString() }) : $t("dashboard.waitingForScan")}</span>
      </div>
    </div>
    <div class="top-actions">
      <button class="primary-button" disabled={!canLaunch || busyAction !== null} on:click={launchCodex}>
        <Play size={16} />
        {$t("codexClient.launch")}
      </button>
    </div>
  </section>

  {#if error}
    <div class="inline-error">{error}</div>
  {/if}
  {#if success}
    <div class="inline-success">{success}</div>
  {/if}

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("codexClient.statusTitle")}</h2>
        <p>{installed?.path ?? $t("codexClient.notInstalled")}</p>
      </div>
      <StatusPill status={statusTone} label={statusLabel} />
    </div>
    <div class="gateway-metrics codex-client-metrics">
      <div>
        <span>{$t("codexClient.currentVersion")}</span>
        <strong>{installed?.version ?? $t("common.none")}</strong>
        <small>{installed?.source ?? $t("common.unknown")}</small>
      </div>
      <div>
        <span>{$t("codexClient.latestVersion")}</span>
        <strong>{release?.version ?? $t("common.unknown")}</strong>
        <small>{release?.packageMoniker ?? $t("codexClient.planNotLoaded")}</small>
      </div>
      <div>
        <span>{$t("codexClient.packageSize")}</span>
        <strong>{formatBytes(plan?.downloadSize ?? release?.contentLength)}</strong>
        <small>{release?.sha256 ? `${release.sha256.slice(0, 12)}...` : $t("common.unknown")}</small>
      </div>
    </div>
    <div class="gateway-actions codex-client-actions">
      <button class="secondary-button" disabled={!canStage || busyAction !== null} on:click={stagePackage}>
        <Download size={16} />
        {busyAction === "stage" ? $t("codexClient.staging") : $t("codexClient.stage")}
      </button>
      <button class="secondary-button" on:click={() => openCodexClientPath("staging")}>
        <FolderOpen size={16} />
        {$t("codexClient.openStagingPath")}
      </button>
      <button class="primary-button" disabled={!canInstall || busyAction !== null} on:click={installOrUpdate}>
        <Rocket size={16} />
        {busyAction === "install" ? $t("codexClient.installing") : installed ? $t("codexClient.update") : $t("codexClient.install")}
      </button>
      <button class="secondary-button" disabled={!canUninstall || busyAction !== null} on:click={() => setCodexClientConfirmUninstall(true)}>
        <Trash2 size={16} />
        {$t("common.delete")}
      </button>
    </div>
    {#if showProgress && progress}
      <div class="install-progress" aria-live="polite">
        <div class="progress-copy">
          <strong>{progressStepLabel ? `${progressStepLabel} / ${progressPhaseLabel(progress.phase)}` : progressPhaseLabel(progress.phase)}</strong>
          <span>{progress.message}</span>
        </div>
        <div class="progress-track" class:indeterminate={progressPercent === null}>
          <span
            class="progress-fill"
            style={`width: ${progressPercent === null ? 38 : Math.max(2, Math.min(100, progressPercent)).toFixed(1)}%`}
          ></span>
        </div>
        <div class="progress-meta">
          <span>{progressPercent === null ? $t("codexClient.progressUnknown") : `${progressPercent.toFixed(0)}%`}</span>
          <span>{progressByteLabel(progress)}</span>
        </div>
      </div>
    {/if}
  </section>

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("codexClient.planTitle")}</h2>
        <p>{release?.manifestUrl ?? $t("codexClient.planNotLoaded")}</p>
      </div>
      <div class="section-actions">
        {#if plan}
          <StatusPill status={plan.upToDate ? "ok" : "warning"} label={plan.upToDate ? $t("codexClient.upToDate") : $t("codexClient.updateAvailable")} />
        {/if}
      </div>
    </div>
    <div class="preview-list codex-client-list">
      {#if plan}
        <div>
          <strong>{$t("codexClient.downloadUrl")}</strong>
          <span>{plan.packageUrl}</span>
        </div>
        <div>
          <strong>SHA-256</strong>
          <span>{plan.sha256}</span>
        </div>
        <div>
          <strong>{$t("codexClient.installRoot")}</strong>
          <span>{plan.installRoot ?? $t("common.none")}</span>
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
        {#each plan.warnings as warning}
          <div class="warning-row">
            <strong>{$t("status.warning")}</strong>
            <span>{warning}</span>
          </div>
        {/each}
      {:else}
        <div class="empty-row">{$t("codexClient.planNotLoaded")}</div>
      {/if}
    </div>
  </section>

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("codexClient.capabilities")}</h2>
        <p>{$t("codexClient.capabilityHint")}</p>
      </div>
    </div>
    <div class="doctor-list">
      {#each plan?.capabilities ?? [] as capability}
        <div class="doctor-row">
          <StatusPill status={capability.status} label={$t(`status.${capability.status}` as Parameters<typeof $t>[0])} />
          <div>
            <h3>{capability.label}</h3>
            <p>{capability.detail}</p>
          </div>
        </div>
      {:else}
        <div class="empty-row">{$t("codexClient.capabilityEmpty")}</div>
      {/each}
    </div>
  </section>

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("codexClient.settingsTitle")}</h2>
        <p>{$t("codexClient.settingsHint")}</p>
      </div>
    </div>
    {#if settingsDraft}
      <div class="settings-list codex-client-settings">
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
        {#if isWindows}
          <label>
            {$t("codexClient.installMode")}
            <select
              value={settingsDraft.windowsInstallMode}
              on:change={(event) => updateCodexClientDraft({ windowsInstallMode: event.currentTarget.value as "msix" | "portable" })}
            >
              <option value="msix">{$t("codexClient.mode.msix")}</option>
              <option value="portable">{$t("codexClient.mode.portable")}</option>
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
        <label class="native-write-toggle">
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
        <label class="native-write-toggle">
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
  <div class="modal-backdrop">
    <div class="modal-panel">
      <div>
        <span class="eyebrow">{$t("codexClient.uninstallEyebrow")}</span>
        <h2>{$t("codexClient.uninstallTitle")}</h2>
        <p>{$t("codexClient.uninstallDescription")}</p>
      </div>
      <div class="preview-list">
        <div>
          <strong>{$t("codexClient.currentVersion")}</strong>
          <span>{installed?.version ?? $t("common.none")} / {installed?.path ?? $t("common.none")}</span>
        </div>
        <div>
          <strong>{$t("codexClient.keepUserData")}</strong>
          <span>{settingsDraft?.keepUserDataOnUninstall ? $t("common.enabled") : $t("common.disabled")}</span>
        </div>
      </div>
      <div class="modal-actions">
        <button class="secondary-button" on:click={() => setCodexClientConfirmUninstall(false)}>{$t("common.cancel")}</button>
        <button class="primary-button" disabled={busyAction !== null} on:click={removeCodex}>
          <Trash2 size={16} />
          {busyAction === "uninstall" ? $t("codexClient.uninstalling") : $t("codexClient.confirmUninstall")}
        </button>
      </div>
    </div>
  </div>
{/if}
