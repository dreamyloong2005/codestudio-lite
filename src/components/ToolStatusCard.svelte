<script lang="ts">
  import { t } from "../lib/i18n";
  import type { ToolStatus } from "../types";
  import AppIcon from "./AppIcon.svelte";
  import StatusPill from "./StatusPill.svelte";
  import ToolIcon from "./ToolIcon.svelte";

  export let tool: ToolStatus;
  export let onConfigure: (tool: ToolStatus) => void = () => {};
  export let onInstall: (tool: ToolStatus) => void = () => {};
  export let onUpdate: (tool: ToolStatus) => void = () => {};
  export let onRepairPath: (tool: ToolStatus) => void = () => {};
  export let installing = false;
  export let updating = false;
  export let repairing = false;

  $: resolvedDetail = tool.details?.startsWith("Resolved: ") ? tool.details.replace("Resolved: ", "") : null;
</script>

<article class="tool-card">
  <div class="tool-main">
    <ToolIcon toolId={tool.id} label={tool.name} category={tool.category} />
    <div class="tool-copy">
      <h3>{tool.name}</h3>
      <p>{tool.version ?? tool.details ?? tool.command}</p>
      {#if resolvedDetail}
        <span class="tool-path">{resolvedDetail}</span>
      {/if}
      {#if tool.updateAvailable && tool.latestVersion}
        <span class="tool-path">{$t("tool.latestVersion", { version: tool.latestVersion })}</span>
      {/if}
      {#if tool.pathRepair}
        <span class="tool-path">{tool.pathRepair.message}</span>
      {/if}
    </div>
  </div>

  <div class="tool-state">
    <StatusPill
      status={tool.installState}
      label={$t(`status.${tool.installState}` as Parameters<typeof $t>[0])}
    />
  </div>

  <div class="tool-action">
    {#if tool.pathRepair}
      <button
        class="secondary-button"
        title={$t("tool.repairPathTitle", { name: tool.name })}
        disabled={installing || updating || repairing}
        on:click={() => onRepairPath(tool)}
      >
        {#if repairing}
          <AppIcon name="loading" size={16} class="spin" />
        {:else}
          <AppIcon name="repair" size={16} />
        {/if}
        {repairing ? $t("tool.repairingPath") : $t("tool.repairPath")}
      </button>
    {/if}
    {#if tool.installState === "missing"}
      <button
        class="secondary-button"
        title={$t("tool.installCommand", { name: tool.name })}
        disabled={installing || updating}
        on:click={() => onInstall(tool)}
      >
        {#if installing}
          <AppIcon name="loading" size={16} class="spin" />
        {:else}
          <AppIcon name="install" size={16} />
        {/if}
        {installing ? $t("tool.installing") : $t("common.install")}
      </button>
    {:else if tool.category === "ai_tool"}
      {#if tool.updateAvailable}
        <button
          class="secondary-button"
          title={$t("tool.updateCommand", { name: tool.name })}
          disabled={installing || updating}
          on:click={() => onUpdate(tool)}
        >
          {#if updating}
            <AppIcon name="loading" size={16} class="spin" />
          {:else}
            <AppIcon name="update" size={16} />
          {/if}
          {updating ? $t("tool.updating") : $t("common.update")}
        </button>
      {/if}
      <button class="secondary-button" title={$t("tool.createConfig", { name: tool.name })} disabled={updating} on:click={() => onConfigure(tool)}>
        <AppIcon name="settings" size={16} />
        {$t("common.createConfig")}
      </button>
    {:else if tool.updateAvailable}
      <button
        class="secondary-button"
        title={$t("tool.updateCommand", { name: tool.name })}
        disabled={installing || updating}
        on:click={() => onUpdate(tool)}
      >
        {#if updating}
          <AppIcon name="loading" size={16} class="spin" />
        {:else}
          <AppIcon name="update" size={16} />
        {/if}
        {updating ? $t("tool.updating") : $t("common.update")}
      </button>
    {/if}
  </div>
</article>
