<script lang="ts">
  import { onMount } from "svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import { loadGatewayRequestLog } from "../lib/api";
  import { t } from "../lib/i18n";
  import { css, cx } from "../../styled-system/css";
  import {
    actionButtonRecipe,
    emptyRowRecipe,
    gatewayHeroRecipe,
    gatewayInlineErrorRecipe,
    gatewayMetricsRecipe,
    gatewayPanelRecipe,
    gatewayRequestListRecipe,
    gatewayRequestPanelRecipe,
    gatewayRequestRowRecipe,
    gatewaySegmentedRecipe,
    gatewaySettingRowRecipe,
    panelRecipe,
    routeStackRecipe,
    sectionHeadingRecipe,
    spinRecipe,
    topActionsRecipe,
    topStripRecipe
  } from "../../styled-system/recipes";
  import type {
    GatewayRequestLogEntry,
    GatewayStatus,
    PrivacyFilterMode
  } from "../types";

  type GatewayTone = "online" | "offline";

  export let gatewayStatus: GatewayStatus | null = null;
  export let gatewayBusy = false;
  export let onGatewayAction: (action: "start" | "stop" | "restart") => void | Promise<void> = () => {};
  export let onPrivacyFilterChange: (mode: PrivacyFilterMode) => void | Promise<void> = () => {};
  export let onCopyGatewayUrl: () => void | Promise<void> = () => {};

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

  const gatewayHeadingClass = css({
    boxShadow: "none",
    padding: 0,
    "& h2": {
      marginTop: "2px",
      lineHeight: "1.25"
    },
    "@media (max-width: 860px)": {
      alignItems: "stretch",
      flexDirection: "column"
    }
  });

  $: gatewayState = gatewayStatus?.running ? $t("common.running") : $t("common.stopped");
  $: gatewayTone = (gatewayStatus?.running ? "online" : "offline") as GatewayTone;
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
    if (entry.privacyFilterMode === "off") {
      return $t("gateway.privacyAction.off");
    }
    if (entry.privacyFilterHitCount <= 0 || entry.privacyFilterAction === "none") {
      return $t("gateway.privacyAction.noPrivacyHit");
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

<div class={routeStackRecipe({ width: "full" })}>
  <section class={cx(topStripRecipe(), gatewayHeroRecipe({ tone: gatewayTone }))}>
    <div>
      <h1>{$t("gateway.title")}</h1>
      <p>{$t("gateway.subtitle")}</p>
    </div>
    <div class={topActionsRecipe()}>
      <button class={actionButtonRecipe({ tone: "primary" })} on:click={() => runGatewayAction("start")} disabled={gatewayBusy || gatewayStatus?.running}>
        <AppIcon name={gatewayBusy ? "loading" : "power"} class={gatewayBusy ? spinRecipe() : ""} size={16} />
        {$t("common.start")}
      </button>
      <button class={actionButtonRecipe()} on:click={() => runGatewayAction("restart")} disabled={gatewayBusy}>
        <AppIcon name={gatewayBusy ? "loading" : "restart"} class={gatewayBusy ? spinRecipe() : ""} size={16} />
        {$t("common.restart")}
      </button>
      <button class={actionButtonRecipe()} on:click={() => runGatewayAction("stop")} disabled={gatewayBusy || !gatewayStatus?.running}>
        <AppIcon name="stop" size={16} />
        {$t("common.stop")}
      </button>
      <button class={actionButtonRecipe()} on:click={onCopyGatewayUrl} disabled={!gatewayStatus?.baseUrl}>
        <AppIcon name="copy" size={16} />
        {$t("dashboard.copyGatewayUrl")}
      </button>
    </div>
  </section>

  <section class={cx(panelRecipe(), gatewayPanelRecipe())}>
    <div class={gatewayMetricsRecipe()}>
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
    <div class={gatewaySettingRowRecipe()}>
      <span>{$t("gateway.privacyFilter")}</span>
      <div class={gatewaySegmentedRecipe()} role="group" aria-label={$t("gateway.privacyFilter")}>
        {#each privacyModes as mode}
          <button
            type="button"
            data-selected={privacyFilterMode === mode.value}
            disabled={privacyBusy}
            on:click={() => setPrivacyMode(mode.value)}
          >
            {$t(mode.labelKey)}
          </button>
        {/each}
      </div>
    </div>
    {#if gatewayStatus?.lastError}
      <div class={gatewayInlineErrorRecipe()}>{gatewayStatus.lastError}</div>
    {/if}
  </section>

  <section class={cx(panelRecipe(), gatewayRequestPanelRecipe())}>
    <div class={cx(sectionHeadingRecipe({ compact: true }), gatewayHeadingClass)}>
      <div>
        <h2>{$t("gateway.requestLogTitle")}</h2>
      </div>
      <button class={actionButtonRecipe()} data-refresh-button="true" on:click={refreshRequestLog} disabled={requestLogLoading}>
        <AppIcon name={requestLogLoading ? "loading" : "refresh"} class={requestLogLoading ? spinRecipe() : ""} size={15} />
        {$t("common.refresh")}
      </button>
    </div>
    {#if requestLogError}
      <div class={gatewayInlineErrorRecipe()}>{requestLogError}</div>
    {:else if requestLog.length === 0}
      <div class={emptyRowRecipe()}>{$t("gateway.noRequests")}</div>
    {:else}
      <div class={gatewayRequestListRecipe()}>
        {#each requestLog.slice(0, 12) as entry}
          <div class={gatewayRequestRowRecipe()} data-privacy-action={entry.privacyFilterAction}>
            <div>
              <strong>{entry.client}</strong>
              <small>{entry.method} {entry.path}</small>
            </div>
            <span>{entry.status}</span>
            <span>{entry.latencyMs}ms</span>
            <span data-gateway-request-time>{formatTime(entry.timestamp)}</span>
            <em>{privacyActionLabel(entry)}</em>
          </div>
        {/each}
      </div>
    {/if}
  </section>

</div>
