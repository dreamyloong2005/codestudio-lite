<script lang="ts">
  import { onMount } from "svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import Profiles from "./Profiles.svelte";
  import { loadGatewayRequestLog } from "../lib/api";
  import { t } from "../lib/i18n";
  import type {
    DetectionSnapshot,
    GatewayRequestLogEntry,
    GatewayStatus,
    PrivacyFilterMode,
    ProfileSummary,
    WizardPrefill
  } from "../types";

  export let summary: ProfileSummary | null = null;
  export let snapshot: DetectionSnapshot | null = null;
  export let gatewayStatus: GatewayStatus | null = null;
  export let gatewayBusy = false;
  export let onGatewayAction: (action: "start" | "stop" | "restart") => void | Promise<void> = () => {};
  export let onPrivacyFilterChange: (mode: PrivacyFilterMode) => void | Promise<void> = () => {};
  export let onCopyGatewayUrl: () => void | Promise<void> = () => {};
  export let onProfileSwitched: () => void | Promise<void> = () => {};
  export let onCreateProfile: (prefill: WizardPrefill) => void = () => {};

  const privacyModes: Array<{ value: PrivacyFilterMode; labelKey: Parameters<typeof $t>[0] }> = [
    { value: "off", labelKey: "gateway.privacy.off" },
    { value: "detect", labelKey: "gateway.privacy.detect" },
    { value: "redact", labelKey: "gateway.privacy.redact" },
    { value: "block", labelKey: "gateway.privacy.block" }
  ];

  let privacyBusy = false;
  let requestLog: GatewayRequestLogEntry[] = [];
  let requestLogLoading = false;
  let requestLogError: string | null = null;

  $: gatewayState = gatewayStatus?.running ? $t("common.running") : $t("common.stopped");
  $: gatewayTone = gatewayStatus?.running ? "online" : "offline";
  $: activeProfileName = gatewayStatus?.activeProfileName ?? $t("dashboard.notConfigured");
  $: activeModel = gatewayStatus?.activeModel ?? $t("common.none");
  $: baseUrl = gatewayStatus?.baseUrl ?? "http://127.0.0.1:43112/v1";
  $: privacyFilterMode = gatewayStatus?.privacyFilterMode ?? "off";

  async function setPrivacyMode(mode: PrivacyFilterMode) {
    if (privacyBusy || mode === privacyFilterMode) {
      return;
    }
    privacyBusy = true;
    try {
      await onPrivacyFilterChange(mode);
      await refreshRequestLog();
    } finally {
      privacyBusy = false;
    }
  }

  async function runGatewayAction(action: "start" | "stop" | "restart") {
    await onGatewayAction(action);
    await refreshRequestLog();
  }

  async function refreshRequestLog() {
    requestLogLoading = true;
    requestLogError = null;
    try {
      requestLog = await loadGatewayRequestLog();
    } catch (err) {
      requestLogError = err instanceof Error ? err.message : String(err);
    } finally {
      requestLogLoading = false;
    }
  }

  function privacyActionLabel(entry: GatewayRequestLogEntry) {
    if (entry.privacyFilterHitCount <= 0 || entry.privacyFilterAction === "none") {
      return $t("gateway.privacyAction.none");
    }
    return $t(`gateway.privacyAction.${entry.privacyFilterAction}` as Parameters<typeof $t>[0], {
      count: entry.privacyFilterHitCount
    });
  }

  function formatTime(value: string) {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
      return value;
    }
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  onMount(() => {
    void refreshRequestLog();
  });
</script>

<div class="route-stack gateway-route">
  <section class={`top-strip gateway-hero ${gatewayTone}`}>
    <div>
      <span class="eyebrow">{$t("gateway.eyebrow")}</span>
      <h1>{$t("gateway.title")}</h1>
      <p>{$t("gateway.subtitle")}</p>
    </div>
    <div class="gateway-actions">
      <button class="primary-button" on:click={() => runGatewayAction("start")} disabled={gatewayBusy || gatewayStatus?.running}>
        <AppIcon name={gatewayBusy ? "loading" : "power"} class={gatewayBusy ? "spin" : ""} size={16} />
        {$t("common.start")}
      </button>
      <button class="secondary-button" on:click={() => runGatewayAction("restart")} disabled={gatewayBusy}>
        <AppIcon name={gatewayBusy ? "loading" : "restart"} class={gatewayBusy ? "spin" : ""} size={16} />
        {$t("common.restart")}
      </button>
      <button class="secondary-button" on:click={() => runGatewayAction("stop")} disabled={gatewayBusy || !gatewayStatus?.running}>
        <AppIcon name="stop" size={16} />
        {$t("common.stop")}
      </button>
      <button class="secondary-button" on:click={onCopyGatewayUrl} disabled={!gatewayStatus?.baseUrl}>
        <AppIcon name="copy" size={16} />
        {$t("dashboard.copyGatewayUrl")}
      </button>
    </div>
  </section>

  <section class="panel-band gateway-panel">
    <div class="gateway-metrics">
      <div>
        <span>{$t("common.status")}</span>
        <strong>{gatewayState}</strong>
      </div>
      <div>
        <span>{$t("common.url")}</span>
        <code>{baseUrl}</code>
      </div>
      <div>
        <span>{$t("dashboard.activeProfile")}</span>
        <strong>{activeProfileName}</strong>
      </div>
      <div>
        <span>{$t("dashboard.currentVirtualModel")}</span>
        <strong>{activeModel}</strong>
      </div>
    </div>
    <div class="gateway-setting-row">
      <span>{$t("gateway.privacyFilter")}</span>
      <div class="gateway-segmented" role="group" aria-label={$t("gateway.privacyFilter")}>
        {#each privacyModes as mode}
          <button
            type="button"
            class:selected={privacyFilterMode === mode.value}
            disabled={privacyBusy}
            on:click={() => setPrivacyMode(mode.value)}
          >
            {$t(mode.labelKey)}
          </button>
        {/each}
      </div>
    </div>
    {#if gatewayStatus?.lastError}
      <div class="sidebar-gateway-error">{gatewayStatus.lastError}</div>
    {/if}
  </section>

  <section class="panel-band gateway-request-panel">
    <div class="section-heading compact-heading">
      <div>
        <span class="eyebrow">{$t("gateway.recentRequests")}</span>
        <h2>{$t("gateway.requestLogTitle")}</h2>
      </div>
      <button class="secondary-button" on:click={refreshRequestLog} disabled={requestLogLoading}>
        <AppIcon name={requestLogLoading ? "loading" : "refresh"} class={requestLogLoading ? "spin" : ""} size={16} />
        {$t("common.refresh")}
      </button>
    </div>
    {#if requestLogError}
      <div class="sidebar-gateway-error">{requestLogError}</div>
    {:else if requestLog.length === 0}
      <div class="empty-state">{$t("gateway.noRequests")}</div>
    {:else}
      <div class="gateway-request-list">
        {#each requestLog.slice(0, 12) as entry}
          <div class={`gateway-request-row privacy-${entry.privacyFilterAction}`}>
            <div>
              <strong>{entry.client}</strong>
              <small>{entry.method} {entry.path}</small>
            </div>
            <span class="gateway-request-cell">{entry.status}</span>
            <span class="gateway-request-cell">{entry.latencyMs}ms</span>
            <span class="gateway-request-time gateway-request-cell">{formatTime(entry.timestamp)}</span>
            <em>{privacyActionLabel(entry)}</em>
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <Profiles
    {summary}
    {snapshot}
    modeFilter="gateway"
    embedded
    onProfileSwitched={onProfileSwitched}
    onCreateProfile={(prefill) => onCreateProfile({ ...prefill, mode: "gateway" })}
  />
</div>
