<script lang="ts">
  import { Clock3 } from "@lucide/svelte";
  import { t } from "../lib/i18n";
  import type { ActivityEvent } from "../types";
  import StatusPill from "./StatusPill.svelte";

  export let events: ActivityEvent[] = [];
</script>

<section class="panel-band">
  <div class="section-heading compact">
    <div>
      <h2>{$t("activity.title")}</h2>
      <p>{$t("activity.subtitle")}</p>
    </div>
    <Clock3 size={18} />
  </div>

  <div class="activity-list">
    {#each events as event}
      <article class="activity-row">
        <StatusPill status={event.level} label={$t(`status.${event.level}` as Parameters<typeof $t>[0])} />
        <p>{event.message}</p>
        <time datetime={event.createdAt}>{new Date(event.createdAt).toLocaleTimeString()}</time>
      </article>
    {:else}
      <div class="empty-row">{$t("activity.empty")}</div>
    {/each}
  </div>
</section>
