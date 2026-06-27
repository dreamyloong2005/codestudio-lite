<script lang="ts">
  import { css, cx } from "../../styled-system/css";
  import { actionButtonRecipe, spinRecipe, toolCardRecipe, toolMainRecipe, toolStateRecipe, toolActionRecipe } from "../../styled-system/recipes";
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

  const toolCopyClass = css({
    minWidth: 0
  });
  const toolPathClass = css({
    display: "block",
    marginTop: "4px",
    color: "var(--text-muted)",
    fontFamily: 'ui-monospace, "SFMono-Regular", Consolas, monospace',
    fontSize: "11px",
    lineHeight: "1.35",
    overflowWrap: "anywhere"
  });
</script>

<article class={toolCardRecipe()}>
  <div class={toolMainRecipe()}>
    <ToolIcon toolId={tool.id} label={tool.name} category={tool.category} />
    <div class={toolCopyClass}>
      <h3>{tool.name}</h3>
      <p>{tool.version ?? tool.details ?? tool.command}</p>
      {#if resolvedDetail}
        <span class={toolPathClass}>{resolvedDetail}</span>
      {/if}
      {#if tool.updateAvailable && tool.latestVersion}
        <span class={toolPathClass}>{$t("tool.latestVersion", { version: tool.latestVersion })}</span>
      {/if}
      {#if tool.pathRepair}
        <span class={toolPathClass}>{tool.pathRepair.message}</span>
      {/if}
    </div>
  </div>

  <div class={toolStateRecipe()}>
    <StatusPill
      status={tool.installState}
      label={$t(`status.${tool.installState}` as Parameters<typeof $t>[0])}
    />
  </div>

  <div class={toolActionRecipe()}>
    {#if tool.pathRepair}
      <button
        class={cx(actionButtonRecipe({ compact: true }))}
        title={$t("tool.repairPathTitle", { name: tool.name })}
        disabled={installing || updating || repairing}
        on:click={() => onRepairPath(tool)}
      >
        {#if repairing}
          <AppIcon name="loading" size={16} class={spinRecipe()} />
        {:else}
          <AppIcon name="repair" size={16} />
        {/if}
        {repairing ? $t("tool.repairingPath") : $t("tool.repairPath")}
      </button>
    {/if}
    {#if tool.installState === "missing"}
      <button
        class={cx(actionButtonRecipe({ compact: true }))}
        title={$t("tool.installCommand", { name: tool.name })}
        disabled={installing || updating}
        on:click={() => onInstall(tool)}
      >
        {#if installing}
          <AppIcon name="loading" size={16} class={spinRecipe()} />
        {:else}
          <AppIcon name="install" size={16} />
        {/if}
        {installing ? $t("tool.installing") : $t("common.install")}
      </button>
    {:else if tool.category === "ai_tool"}
      {#if tool.updateAvailable}
        <button
          class={cx(actionButtonRecipe({ compact: true }))}
          title={$t("tool.updateCommand", { name: tool.name })}
          disabled={installing || updating}
          on:click={() => onUpdate(tool)}
        >
          {#if updating}
            <AppIcon name="loading" size={16} class={spinRecipe()} />
          {:else}
            <AppIcon name="update" size={16} />
          {/if}
          {updating ? $t("tool.updating") : $t("common.update")}
        </button>
      {/if}
      <button class={cx(actionButtonRecipe({ compact: true }))} title={$t("tool.createConfig", { name: tool.name })} disabled={updating} on:click={() => onConfigure(tool)}>
        <AppIcon name="settings" size={16} />
        {$t("common.createConfig")}
      </button>
    {:else if tool.updateAvailable}
      <button
        class={cx(actionButtonRecipe({ compact: true }))}
        title={$t("tool.updateCommand", { name: tool.name })}
        disabled={installing || updating}
        on:click={() => onUpdate(tool)}
      >
        {#if updating}
          <AppIcon name="loading" size={16} class={spinRecipe()} />
        {:else}
          <AppIcon name="update" size={16} />
        {/if}
        {updating ? $t("tool.updating") : $t("common.update")}
      </button>
    {/if}
  </div>
</article>
