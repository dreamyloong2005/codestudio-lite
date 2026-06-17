<script lang="ts">
  import AppIcon from "../components/AppIcon.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import ToolStatusCard from "../components/ToolStatusCard.svelte";
  import {
    clearClaudeEnvironmentVariables,
    installTool,
    planToolInstall,
    repairToolPath,
    updateTool
  } from "../lib/api";
  import { t } from "../lib/i18n";
  import type {
    DetectionSnapshot,
    ToolInstallPlan,
    ToolInstallResult,
    ToolStatus
  } from "../types";

  export let snapshot: DetectionSnapshot | null = null;
  export let onRefresh: (options?: { quiet?: boolean; scheduleFollowup?: boolean }) => void | Promise<void> = () => {};
  export let onToolStatusUpdated: (tool: ToolStatus) => void = () => {};
  export let onConfigureTool: (tool: ToolStatus) => void = () => {};
  export let onOpenCodexClient: () => void = () => {};

  let installPlan: ToolInstallPlan | null = null;
  let installResult: ToolInstallResult | null = null;
  let pendingInstallTool: ToolStatus | null = null;
  let installError: string | null = null;
  let toolActionMessage: string | null = null;
  let toolActionError: string | null = null;
  let planningToolId: string | null = null;
  let installingToolId: string | null = null;
  let updatingToolId: string | null = null;
  let repairingToolId: string | null = null;
  let clearingClaudeEnv = false;
  const vscodePluginToolIds = new Set(["codex-vscode", "claude-vscode", "gemini-code-assist"]);

  function clientSortRank(tool: ToolStatus) {
    return tool.id === "codex-app" ? 0 : 1;
  }

  function isVscodePluginTool(tool: ToolStatus) {
    return vscodePluginToolIds.has(tool.id);
  }

  function hasVsCodeHost(tools: ToolStatus[]) {
    const pluginTools = tools.filter(isVscodePluginTool);
    return (
      pluginTools.length === 0 ||
      pluginTools.some((tool) => tool.installState === "installed" || tool.details !== "Command not found")
    );
  }

  $: connectedClients = [
    ...(snapshot?.tools.filter((tool) => {
      if (tool.category !== "ai_tool") {
        return false;
      }
      return !isVscodePluginTool(tool) || hasVsCodeHost(snapshot?.tools ?? []);
    }) ?? [])
  ]
    .sort((left, right) => clientSortRank(left) - clientSortRank(right));
  $: envConflicts = snapshot?.envConflicts ?? [];
  async function copyInstallCommand() {
    if (!installPlan?.command) {
      return;
    }
    await navigator.clipboard?.writeText(installPlan.command);
  }

  async function openInstallPlan(tool: ToolStatus) {
    if (tool.id === "codex-app") {
      onOpenCodexClient();
      return;
    }

    pendingInstallTool = tool;
    installPlan = null;
    installResult = null;
    installError = null;
    toolActionMessage = null;
    toolActionError = null;
    planningToolId = tool.id;
    try {
      installPlan = await planToolInstall(tool.id);
    } catch (err) {
      installError = err instanceof Error ? err.message : String(err);
    } finally {
      planningToolId = null;
    }
  }

  function closeInstallPlan() {
    if (installingToolId) {
      return;
    }
    pendingInstallTool = null;
    installPlan = null;
    installResult = null;
    installError = null;
  }

  async function confirmInstall() {
    if (!installPlan || installingToolId) {
      return;
    }
    installingToolId = installPlan.toolId;
    installError = null;
    installResult = null;
    try {
      installResult = await installTool({
        toolId: installPlan.toolId,
        confirm: true,
        installPrerequisites: installPlan.requiresPrerequisites
      });
      if (installResult.currentStatus) {
        onToolStatusUpdated(installResult.currentStatus);
      }
      void Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false })).catch(() => {});
    } catch (err) {
      installError = err instanceof Error ? err.message : String(err);
    } finally {
      installingToolId = null;
    }
  }

  async function confirmUpdate(tool: ToolStatus) {
    if (updatingToolId || installingToolId || planningToolId) {
      return;
    }

    updatingToolId = tool.id;
    toolActionMessage = null;
    toolActionError = null;
    installError = null;

    try {
      const result = await updateTool({
        toolId: tool.id,
        confirm: true
      });
      if (result.success) {
        toolActionMessage = result.message;
      } else {
        toolActionError = result.message;
      }
      if (result.currentStatus) {
        onToolStatusUpdated(result.currentStatus);
      }
      void Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false })).catch(() => {});
    } catch (err) {
      toolActionError = err instanceof Error ? err.message : String(err);
    } finally {
      updatingToolId = null;
    }
  }

  async function confirmRepairPath(tool: ToolStatus) {
    if (!tool.pathRepair || repairingToolId || installingToolId || planningToolId || updatingToolId) {
      return;
    }

    repairingToolId = tool.id;
    toolActionMessage = null;
    toolActionError = null;
    installError = null;

    try {
      const result = await repairToolPath({
        toolId: tool.id,
        confirm: true
      });
      if (result.success) {
        toolActionMessage = result.message;
      } else {
        toolActionError = result.message;
      }
      if (result.currentStatus) {
        onToolStatusUpdated(result.currentStatus);
      }
      void Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false })).catch(() => {});
    } catch (err) {
      toolActionError = err instanceof Error ? err.message : String(err);
    } finally {
      repairingToolId = null;
    }
  }

  async function clearClaudeEnvConflicts() {
    if (clearingClaudeEnv || envConflicts.length === 0) {
      return;
    }
    clearingClaudeEnv = true;
    toolActionMessage = null;
    toolActionError = null;
    try {
      const result = await clearClaudeEnvironmentVariables({
        toolId: "claude",
        variables: envConflicts.map((conflict) => conflict.variable),
        confirm: true
      });
      if (result.success) {
        toolActionMessage = result.message;
      } else {
        toolActionError = result.message;
      }
      await Promise.resolve(onRefresh({ quiet: true, scheduleFollowup: false }));
    } catch (err) {
      toolActionError = err instanceof Error ? err.message : String(err);
    } finally {
      clearingClaudeEnv = false;
    }
  }
</script>

<div class="route-stack">
  <section class="top-strip">
    <div>
      <span class="eyebrow">{$t("dashboard.eyebrow")}</span>
      <h1>{$t("dashboard.title")}</h1>
      <p>{$t("dashboard.subtitle")}</p>
    </div>
  </section>

  <section class="panel-band">
    {#if toolActionMessage}
      <div class="inline-success">{toolActionMessage}</div>
    {/if}
    {#if toolActionError}
      <div class="inline-error">{toolActionError}</div>
    {/if}
    {#if envConflicts.length > 0}
      <div class="inline-error env-conflict-banner">
        <div>
          <strong>{$t("envConflict.title")}</strong>
          <span>{$t("envConflict.dashboardDescription", { count: envConflicts.length })}</span>
          <div class="conflict-chip-list">
            {#each envConflicts as conflict}
              <code>{conflict.scope}:{conflict.variable}={conflict.currentValuePreview}</code>
            {/each}
          </div>
        </div>
        <button class="secondary-button" disabled={clearingClaudeEnv} on:click={clearClaudeEnvConflicts}>
          {#if clearingClaudeEnv}
            <AppIcon name="loading" size={16} class="spin" />
          {:else}
            <AppIcon name="repair" size={16} />
          {/if}
          {$t("envConflict.clearAction")}
        </button>
      </div>
    {/if}

    <div class="section-heading">
      <div>
        <h2>{$t("dashboard.connectedClients")}</h2>
        <p>{snapshot ? $t("dashboard.toolsTracked", { count: connectedClients.length }) : $t("app.state.scanning")}</p>
      </div>
    </div>
    <div class="tool-grid">
      {#each connectedClients as tool}
        <ToolStatusCard
          {tool}
          onConfigure={onConfigureTool}
          onInstall={openInstallPlan}
          onUpdate={confirmUpdate}
          onRepairPath={confirmRepairPath}
          installing={planningToolId === tool.id || installingToolId === tool.id}
          updating={updatingToolId === tool.id}
          repairing={repairingToolId === tool.id}
        />
      {:else}
        <div class="empty-row">{$t("dashboard.noClientSnapshot")}</div>
      {/each}
    </div>
  </section>

  <section class="panel-band">
    <div class="section-heading">
      <div>
        <h2>{$t("dashboard.system")}</h2>
        <p>{$t("dashboard.systemDeps")}</p>
      </div>
    </div>
    <div class="system-grid">
      {#each snapshot?.system ?? [] as tool}
        <article class="system-card">
          <div class="system-main">
            <ToolIcon toolId={tool.id} label={tool.name} category={tool.category} />
            <div class="system-copy">
              <h3>{tool.name}</h3>
              <p>{tool.version ?? tool.details ?? tool.command}</p>
              {#if tool.updateAvailable && tool.latestVersion}
                <span class="tool-path">{$t("tool.latestVersion", { version: tool.latestVersion })}</span>
              {/if}
            </div>
          </div>

          <div class="system-card-state">
            <StatusPill
              status={tool.installState}
              label={$t(`status.${tool.installState}` as Parameters<typeof $t>[0])}
            />
            {#if tool.installState === "missing"}
              {#if tool.pathRepair}
                <button
                  class="secondary-button"
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={planningToolId === tool.id || installingToolId === tool.id || updatingToolId === tool.id || repairingToolId === tool.id}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              <button
                class="secondary-button"
                title={$t("tool.installCommand", { name: tool.name })}
                disabled={planningToolId === tool.id || installingToolId === tool.id || updatingToolId === tool.id || repairingToolId === tool.id}
                on:click={() => openInstallPlan(tool)}
              >
                {#if planningToolId === tool.id || installingToolId === tool.id}
                  <AppIcon name="loading" size={16} class="spin" />
                {:else}
                  <AppIcon name="install" size={16} />
                {/if}
                {planningToolId === tool.id || installingToolId === tool.id ? $t("tool.installing") : $t("common.install")}
              </button>
            {:else if tool.updateAvailable}
              {#if tool.pathRepair}
                <button
                  class="secondary-button"
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={planningToolId === tool.id || installingToolId === tool.id || updatingToolId === tool.id || repairingToolId === tool.id}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {/if}
              <button
                class="secondary-button"
                title={$t("tool.updateCommand", { name: tool.name })}
                disabled={planningToolId === tool.id || installingToolId === tool.id || updatingToolId === tool.id || repairingToolId === tool.id}
                on:click={() => confirmUpdate(tool)}
              >
                {#if updatingToolId === tool.id}
                  <AppIcon name="loading" size={16} class="spin" />
                {:else}
                  <AppIcon name="update" size={16} />
                {/if}
                {updatingToolId === tool.id ? $t("tool.updating") : $t("common.update")}
              </button>
            {:else}
              {#if tool.pathRepair}
                <button
                  class="secondary-button"
                  title={$t("tool.repairPathTitle", { name: tool.name })}
                  disabled={planningToolId === tool.id || installingToolId === tool.id || updatingToolId === tool.id || repairingToolId === tool.id}
                  on:click={() => confirmRepairPath(tool)}
                >
                  {#if repairingToolId === tool.id}
                    <AppIcon name="loading" size={16} class="spin" />
                  {:else}
                    <AppIcon name="repair" size={16} />
                  {/if}
                  {repairingToolId === tool.id ? $t("tool.repairingPath") : $t("tool.repairPath")}
                </button>
              {:else}
                <span class="quiet">{$t("common.ready")}</span>
              {/if}
            {/if}
          </div>
        </article>
      {:else}
        <div class="empty-row">{$t("dashboard.noSystemSnapshot")}</div>
      {/each}
    </div>
  </section>

</div>

{#if pendingInstallTool}
  <div class="modal-backdrop" role="presentation">
    <div class="modal-panel wide-modal" role="dialog" aria-modal="true" aria-labelledby="tool-install-title">
      <div>
        <span class="eyebrow">{$t("toolInstall.eyebrow")}</span>
        <h2 id="tool-install-title">{$t("toolInstall.title", { name: pendingInstallTool.name })}</h2>
        <p>{$t("toolInstall.description")}</p>
      </div>

      {#if planningToolId}
        <div class="install-progress" aria-live="polite">
          <div class="progress-copy">
            <strong>{$t("toolInstall.planning")}</strong>
            <span>{pendingInstallTool.name}</span>
          </div>
          <div class="progress-track indeterminate">
            <span class="progress-fill"></span>
          </div>
        </div>
      {/if}

      {#if installPlan}
        <div class="install-command-box">
          <div>
            <strong>{$t("toolInstall.command")}</strong>
            <span>{$t("toolInstall.manager", { manager: installPlan.manager })}</span>
          </div>
          <div class="install-command-list">
            {#each installPlan.commands as command}
              <div>
                <span>{command.stage === "prerequisite" ? $t("toolInstall.stage.prerequisite") : $t("toolInstall.stage.target")}</span>
                <code>{command.command}</code>
              </div>
            {/each}
          </div>
          <button class="icon-button" title={$t("toolInstall.copyCommand")} on:click={copyInstallCommand}>
            <AppIcon name="copy" size={18} />
          </button>
        </div>

        <div class="install-meta">
          <span>{installPlan.requiresAdmin ? $t("toolInstall.adminMayPrompt") : $t("toolInstall.userScope")}</span>
        </div>

        {#if installPlan.prerequisites.length > 0}
          <div class="preview-list">
            {#each installPlan.prerequisites as prerequisite}
              <div>
                <strong>{prerequisite.toolName}</strong>
                <span>{prerequisite.reason}</span>
                <code>{prerequisite.command}</code>
              </div>
            {/each}
          </div>
        {/if}

        {#if installPlan.blocker}
          <div class="inline-error">{installPlan.blocker}</div>
        {/if}

        <div class="preview-list">
          {#each installPlan.steps as step}
            <div>
              <strong>{step.label}</strong>
              <span>{step.detail}</span>
            </div>
          {/each}
        </div>

        {#if installPlan.warnings.length > 0}
          <div class="preview-warnings">
            {#each installPlan.warnings as warning}
              <span>{warning}</span>
            {/each}
          </div>
        {/if}
      {/if}

      {#if installingToolId}
        <div class="install-progress" aria-live="polite">
          <div class="progress-copy">
            <strong>{$t("tool.installing")}</strong>
            <span>{installPlan?.command}</span>
          </div>
          <div class="progress-track indeterminate">
            <span class="progress-fill"></span>
          </div>
        </div>
      {/if}

      {#if installResult}
        <div class={installResult.success ? "inline-success" : "inline-error"}>
          {installResult.message}
        </div>
        <div class="install-result-grid">
          <div>
            <strong>{$t("toolInstall.exitCode")}</strong>
            <span>{installResult.exitCode ?? $t("common.none")}</span>
          </div>
          <div>
            <strong>{$t("common.status")}</strong>
            <span>{installResult.currentStatus?.installState ?? $t("common.unknown")}</span>
          </div>
        </div>
        {#if installResult.stageResults.length > 0}
          <div class="preview-list">
            {#each installResult.stageResults as stage}
              <div>
                <strong>{stage.stage === "prerequisite" ? $t("toolInstall.stage.prerequisite") : $t("toolInstall.stage.target")} / {stage.toolName}</strong>
                <span>{stage.message}</span>
                <code>{stage.command}</code>
              </div>
            {/each}
          </div>
        {/if}
        {#if installResult.stdoutTail}
          <div class="install-log">
            <strong>{$t("toolInstall.stdout")}</strong>
            <pre>{installResult.stdoutTail}</pre>
          </div>
        {/if}
        {#if installResult.stderrTail}
          <div class="install-log">
            <strong>{$t("toolInstall.stderr")}</strong>
            <pre>{installResult.stderrTail}</pre>
          </div>
        {/if}
        {#if installResult.notes.length > 0}
          <div class="preview-warnings">
            {#each installResult.notes as note}
              <span>{note}</span>
            {/each}
          </div>
        {/if}
      {/if}

      {#if installError}
        <div class="inline-error">{installError}</div>
      {/if}

      <div class="modal-actions">
        <button class="secondary-button" on:click={closeInstallPlan} disabled={Boolean(installingToolId)}>
          {$t(installResult ? "common.close" : "common.cancel")}
        </button>
        {#if installPlan && !installResult}
          <button
            class="primary-button"
            on:click={confirmInstall}
            disabled={!installPlan.canInstall || Boolean(installingToolId)}
          >
            {#if installingToolId}
              <AppIcon name="loading" size={16} class="spin" />
              {$t("tool.installing")}
            {:else}
              <AppIcon name="install" size={16} />
              {$t(installPlan.requiresPrerequisites ? "toolInstall.confirmInstallWithPrerequisites" : "toolInstall.confirmInstall")}
            {/if}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
