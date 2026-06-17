<script lang="ts">
  import { t } from "../lib/i18n";
  import type { Severity } from "../types";
  import AppIcon from "./AppIcon.svelte";

  export let status: Severity | "installed" | "missing" | "configured" | "unconfigured" | "not_applicable" | "unknown";
  export let label: string;

  $: tone =
    status === "ok" || status === "installed" || status === "configured"
      ? "good"
      : status === "error" || status === "missing"
        ? "bad"
      : status === "warning" || status === "unconfigured"
          ? "warn"
          : "info";
</script>

<span class={`pill ${tone}`}>
  {#if tone === "good"}
    <AppIcon name="check" size={14} />
  {:else if tone === "bad"}
    <AppIcon name="error" size={14} />
  {:else if tone === "warn"}
    <AppIcon name="warning" size={14} />
  {:else}
    <AppIcon name="info" size={14} />
  {/if}
  {label || $t(`status.${status}` as Parameters<typeof $t>[0])}
</span>
