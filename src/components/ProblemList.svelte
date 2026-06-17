<script lang="ts">
  import { t } from "../lib/i18n";
  import type { Problem } from "../types";
  import AppIcon from "./AppIcon.svelte";
  import StatusPill from "./StatusPill.svelte";

  export let problems: Problem[] = [];
</script>

<section class="panel-band">
  <div class="section-heading">
    <div>
      <h2>{$t("problems.title")}</h2>
      <p>{problems.length === 0 ? $t("problems.none") : $t("problems.needsAttention", { count: problems.length })}</p>
    </div>
    <button class="primary-button" disabled={problems.length === 0} title={$t("problems.fixSelected")}>
      <AppIcon name="apply" size={16} />
      {$t("problems.fixSelected")}
    </button>
  </div>

  {#if problems.length === 0}
    <div class="empty-row">{$t("problems.ready")}</div>
  {:else}
    <div class="problem-list">
      {#each problems as problem}
        <article class="problem-row">
          <StatusPill status={problem.severity} label={$t(`status.${problem.severity}` as Parameters<typeof $t>[0])} />
          <div>
            <h3>{problem.title}</h3>
            <p>{problem.detail}</p>
          </div>
          {#if problem.actionLabel}
            <button class="icon-button" title={problem.actionLabel}>
              <AppIcon name="repair" size={16} />
            </button>
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</section>
