<script lang="ts">
  import { flip } from "svelte/animate";
  import {
    dragHandleZone,
    SHADOW_ITEM_MARKER_PROPERTY_NAME,
    SHADOW_PLACEHOLDER_ITEM_ID,
    TRIGGERS,
    type DndEvent
  } from "svelte-dnd-action";
  import { nextSortableProfileIds, profileDragDisabled, profileListContentKey } from "../../lib/profileSortable";
  import type { ProfileDraft, ProviderApplyMode } from "../../types";
  import ProfileCard from "./ProfileCard.svelte";
  import { profileGridRecipe, profileSortableRowRecipe, profileToolSectionRecipe } from "../../../styled-system/recipes";

  export let profiles: ProfileDraft[] = [];
  export let activeProfileId: string | null = null;
  export let toolId: string;
  export let mode: ProviderApplyMode;
  export let applyingId: string | null = null;
  export let duplicatingId: string | null = null;
  export let deletingId: string | null = null;
  export let editingId: string | null = null;
  export let onApply: (profile: ProfileDraft) => void;
  export let onUsage: (profile: ProfileDraft) => void;
  export let onEdit: (profile: ProfileDraft) => void;
  export let onDuplicate: (profile: ProfileDraft) => void;
  export let onDelete: (profile: ProfileDraft) => void;
  export let onReorder: (profileIds: string[]) => Promise<void>;

  let items: ProfileDraft[] = [];
  let sourceItems: ProfileDraft[] = [];
  let listKey = "";
  let activeDragId: string | null = null;
  let saving = false;
  const flipDurationMs = 220;
  const dropTargetStyle = { outline: "none" };

  $: syncItems(profiles, toolId, mode);
  $: dragDisabled = profileDragDisabled({ deletingId, applyingId, editingId, sortableSaving: saving });

  function syncItems(nextProfiles: ProfileDraft[], nextToolId: string, nextMode: ProviderApplyMode) {
    const nextKey = profileListContentKey(`${nextMode}:${nextToolId}`, nextProfiles);
    if (nextKey === listKey) return;
    listKey = nextKey;
    items = nextProfiles;
    sourceItems = nextProfiles;
    activeDragId = null;
  }

  function isShadow(profile: ProfileDraft) {
    return Boolean((profile as ProfileDraft & Record<string, unknown>)[SHADOW_ITEM_MARKER_PROPERTY_NAME]);
  }

  function sortableKey(profile: ProfileDraft) {
    return `${profile.id}:${isShadow(profile) ? "shadow" : "item"}`;
  }

  function handleConsider(event: CustomEvent<DndEvent<ProfileDraft>>) {
    if (event.detail.info.trigger === TRIGGERS.DRAG_STARTED) {
      sourceItems = items.filter((profile) => profile.id !== SHADOW_PLACEHOLDER_ITEM_ID);
    }
    activeDragId = String(event.detail.info.id);
    items = event.detail.items;
  }

  async function handleFinalize(event: CustomEvent<DndEvent<ProfileDraft>>) {
    const nextItems = event.detail.items.filter((profile) => profile.id !== SHADOW_PLACEHOLDER_ITEM_ID);
    items = nextItems;
    activeDragId = null;
    const nextIds = nextSortableProfileIds(sourceItems, nextItems);
    if (!nextIds) {
      sourceItems = nextItems;
      return;
    }
    saving = true;
    try {
      await onReorder(nextIds);
      sourceItems = nextItems;
    } catch (error) {
      items = sourceItems;
      throw error;
    } finally {
      saving = false;
      activeDragId = null;
    }
  }

  function styleDraggedElement(element?: HTMLElement) {
    if (!element) return;
    element.setAttribute("data-sortable-active", "true");
    element.querySelector("[data-profile-card]")?.setAttribute("data-drag-active", "true");
  }

  function cardActionKey(profile: ProfileDraft) {
    return `${profile.mode}:${profile.id}:apply:base`;
  }
</script>

<section class={profileToolSectionRecipe()}>
  <div
    class={profileGridRecipe()}
    role="list"
    use:dragHandleZone={{
      items,
      flipDurationMs,
      dragDisabled,
      dropFromOthersDisabled: true,
      dropTargetStyle,
      transformDraggedElement: styleDraggedElement,
      zoneTabIndex: -1,
      zoneItemTabIndex: -1
    }}
    on:consider={handleConsider}
    on:finalize={handleFinalize}
  >
    {#each items as profile (sortableKey(profile))}
      <div
        class={profileSortableRowRecipe()}
        role="listitem"
        data-profile-sortable-id={profile.id}
        data-sortable-active={activeDragId === profile.id}
        data-is-dnd-shadow-item-hint={isShadow(profile)}
        animate:flip={{ duration: flipDurationMs }}
      >
        <ProfileCard
          {profile}
          active={activeProfileId === profile.id}
          dragActive={activeDragId === profile.id}
          canSort={!dragDisabled}
          {applyingId}
          {duplicatingId}
          {deletingId}
          {editingId}
          actionKey={cardActionKey(profile)}
          {onApply}
          {onUsage}
          {onEdit}
          {onDuplicate}
          {onDelete}
        />
      </div>
    {/each}
  </div>
</section>
