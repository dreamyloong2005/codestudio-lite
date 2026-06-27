<script lang="ts">
  import { css, cx } from "../../styled-system/css";
  import { activityListRecipe, activityRowRecipe, emptyRowRecipe, panelRecipe, sectionHeadingRecipe } from "../../styled-system/recipes";
  import { t } from "../lib/i18n";
  import type { ActivityEvent } from "../types";
  import AppIcon from "./AppIcon.svelte";
  import StatusPill from "./StatusPill.svelte";

  export let events: ActivityEvent[] = [];

  const headingCopyClass = css({
    minWidth: 0
  });
</script>

<section class={panelRecipe()}>
  <div class={sectionHeadingRecipe({ compact: true })}>
    <div class={headingCopyClass}>
      <h2>{$t("activity.title")}</h2>
      <p>{$t("activity.subtitle")}</p>
    </div>
    <AppIcon name="clock" size={18} />
  </div>

  <div class={activityListRecipe()}>
    {#each events as event}
      <article class={activityRowRecipe()}>
        <StatusPill status={event.level} label={$t(`status.${event.level}` as Parameters<typeof $t>[0])} />
        <p>{event.message}</p>
        <time datetime={event.createdAt}>{new Date(event.createdAt).toLocaleTimeString()}</time>
      </article>
    {:else}
      <div class={cx(emptyRowRecipe())}>{$t("activity.empty")}</div>
    {/each}
  </div>
</section>
