<script lang="ts">
  import { AlertTriangle, CheckCircle2, CircleAlert, Info } from "@lucide/svelte";
  import { t } from "../lib/i18n";
  import type { Severity } from "../types";

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
    <CheckCircle2 size={14} />
  {:else if tone === "bad"}
    <CircleAlert size={14} />
  {:else if tone === "warn"}
    <AlertTriangle size={14} />
  {:else}
    <Info size={14} />
  {/if}
  {label || $t(`status.${status}` as Parameters<typeof $t>[0])}
</span>
