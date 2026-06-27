<script lang="ts">
  import { statusPillRecipe } from "../../styled-system/recipes";
  import { t } from "../lib/i18n";
  import type { Severity } from "../types";
  import AppIcon from "./AppIcon.svelte";

  type PillTone = "good" | "bad" | "warn" | "info";

  export let status: Severity | "installed" | "missing" | "configured" | "unconfigured" | "not_applicable" | "unknown";
  export let label: string;

  let tone: PillTone = "info";

  $: tone =
    status === "ok" || status === "installed" || status === "configured"
      ? "good"
      : status === "error" || status === "missing"
        ? "bad"
      : status === "warning" || status === "unconfigured"
          ? "warn"
          : "info";
</script>

<span class={statusPillRecipe({ tone })}>
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
