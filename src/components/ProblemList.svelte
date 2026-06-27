<script lang="ts">
  import { css, cx } from "../../styled-system/css";
  import {
    actionButtonRecipe,
    emptyRowRecipe,
    iconButtonRecipe,
    panelRecipe,
    problemListRecipe,
    problemRowRecipe,
    sectionHeadingRecipe
  } from "../../styled-system/recipes";
  import { t } from "../lib/i18n";
  import type { Problem } from "../types";
  import AppIcon from "./AppIcon.svelte";
  import StatusPill from "./StatusPill.svelte";

  export let problems: Problem[] = [];

  const rowCopyClass = css({
    minWidth: 0
  });
  const headingCopyClass = css({
    minWidth: 0
  });
</script>

<section class={panelRecipe()}>
  <div class={sectionHeadingRecipe()}>
    <div class={headingCopyClass}>
      <h2>{$t("problems.title")}</h2>
      <p>{problems.length === 0 ? $t("problems.none") : $t("problems.needsAttention", { count: problems.length })}</p>
    </div>
    <button class={actionButtonRecipe({ tone: "primary" })} disabled={problems.length === 0} title={$t("problems.fixSelected")}>
      <AppIcon name="apply" size={16} />
      {$t("problems.fixSelected")}
    </button>
  </div>

  {#if problems.length === 0}
    <div class={emptyRowRecipe()}>{$t("problems.ready")}</div>
  {:else}
    <div class={problemListRecipe()}>
      {#each problems as problem}
        <article class={problemRowRecipe()}>
          <StatusPill status={problem.severity} label={$t(`status.${problem.severity}` as Parameters<typeof $t>[0])} />
          <div class={rowCopyClass}>
            <h3>{problem.title}</h3>
            <p>{problem.detail}</p>
          </div>
          {#if problem.actionLabel}
            <button class={cx(iconButtonRecipe())} title={problem.actionLabel} aria-label={problem.actionLabel}>
              <AppIcon name="repair" size={16} />
            </button>
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</section>
