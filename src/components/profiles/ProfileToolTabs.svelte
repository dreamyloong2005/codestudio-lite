<script lang="ts">
  import { t } from "../../lib/i18n";
  import type { ProfileGroup } from "../../lib/profiles/grouping";
  import type { ProfileDraft } from "../../types";
  import ToolIcon from "../ToolIcon.svelte";
  import { profileToolSwitcherRecipe, profileToolTabsRecipe } from "../../../styled-system/recipes";

  export let groups: ProfileGroup[] = [];
  export let selectedToolId: string | null = null;
  export let activeProfileLabel: (profile: ProfileDraft) => string;
  export let onSelect: (toolId: string) => void;
</script>

<section class={profileToolSwitcherRecipe()} aria-label={$t("profiles.toolSwitcherLabel")}>
  <div class={profileToolTabsRecipe()} role="tablist">
    {#each groups as group}
      {@const activeProfile = group.profiles.find((profile) => profile.id === group.activeProfileId) ?? null}
      <button
        type="button"
        data-selected={selectedToolId === group.id}
        role="tab"
        aria-selected={selectedToolId === group.id}
        title={group.label}
        on:click={() => onSelect(group.id)}
      >
        <ToolIcon toolId={group.id} label={group.label} variant="choice" />
        <span>
          <strong>{group.label}</strong>
          <small>{activeProfile ? activeProfileLabel(activeProfile) : $t("profiles.noActiveProfile")}</small>
        </span>
      </button>
    {/each}
  </div>
</section>
