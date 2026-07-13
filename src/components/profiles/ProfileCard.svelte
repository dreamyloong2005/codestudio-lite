<script lang="ts">
  import { dragHandle } from "svelte-dnd-action";
  import { t, type TranslationKey } from "../../lib/i18n";
  import { canonicalProfileToolId, OFFICIAL_PROFILE_NAME_KEYS } from "../../lib/profiles/catalog";
  import {
    profileDisplayName as resolveProfileDisplayName,
    profileEndpoint,
    profileIconIsImage,
    profileIconValue as resolveProfileIconValue,
    profileRemark,
    profileUsesToolIcon,
    providerIsOfficial
  } from "../../lib/profiles/presentation";
  import type { ProfileDraft } from "../../types";
  import AppIcon from "../AppIcon.svelte";
  import StatusPill from "../StatusPill.svelte";
  import ToolIcon from "../ToolIcon.svelte";
  import {
    actionButtonRecipe,
    iconButtonRecipe,
    profileAvatarRecipe,
    profileCardActionsRecipe,
    profileCardMainRecipe,
    profileCardRecipe,
    profileCardStatusRecipe,
    profileDragHandleRecipe,
    profileIdentityRecipe,
    spinRecipe
  } from "../../../styled-system/recipes";

  export let profile: ProfileDraft;
  export let active = false;
  export let dragActive = false;
  export let canSort = false;
  export let applyingId: string | null = null;
  export let duplicatingId: string | null = null;
  export let deletingId: string | null = null;
  export let editingId: string | null = null;
  export let actionKey: string;
  export let onApply: (profile: ProfileDraft) => void;
  export let onUsage: (profile: ProfileDraft) => void;
  export let onEdit: (profile: ProfileDraft) => void;
  export let onDuplicate: (profile: ProfileDraft) => void;
  export let onDelete: (profile: ProfileDraft) => void;

  $: displayName = profileDisplayName(profile);
  $: icon = resolveProfileIconValue(profile, displayName);
  $: remark = profileRemark(profile);
  $: canOpenUsage = profileUsesCodexOfficialOAuth(profile)
    || (!providerIsOfficial(profile.provider) && Boolean(profile.baseUrl.trim() || profile.authRef));
  $: actionsBusy = duplicatingId !== null || deletingId !== null || applyingId !== null || editingId !== null;

  function profileDisplayName(value: ProfileDraft) {
    const nameKeys = OFFICIAL_PROFILE_NAME_KEYS as Record<string, TranslationKey>;
    const nameKey = nameKeys[canonicalProfileToolId(value.app)];
    return resolveProfileDisplayName(value, nameKey ? $t(nameKey) : undefined);
  }

  function profileUrl(value: ProfileDraft) {
    return profileEndpoint(value) ?? $t("profiles.officialProfileEndpoint");
  }

  function profileUsesCodexOfficialOAuth(value: ProfileDraft) {
    return canonicalProfileToolId(value.app) === "codex" && providerIsOfficial(value.provider);
  }

  function applyModeLabel() {
    return profile.mode === "config" ? $t("profiles.mode.config") : $t("profiles.mode.gateway");
  }
</script>

<article
  class={profileCardRecipe()}
  data-profile-card
  data-active={active}
  data-builtin={profile.isBuiltin}
  data-drag-active={dragActive}
>
  <div class={profileCardMainRecipe()}>
    <span
      class={profileDragHandleRecipe()}
      aria-label={$t("profiles.dragHandle")}
      aria-disabled={!canSort}
      data-profile-drag-handle={profile.id}
      use:dragHandle
    >
      <AppIcon name="drag" size={16} />
    </span>
    <div class={profileAvatarRecipe()} data-profile-avatar aria-hidden="true">
      {#if profileUsesToolIcon(profile)}
        <ToolIcon toolId={profile.app} label={displayName} variant="heading" />
      {:else if profileIconIsImage(icon)}
        <img src={icon} alt="" />
      {:else}
        <span>{icon}</span>
      {/if}
    </div>
    <div class={profileIdentityRecipe()}>
      <h2>{displayName}</h2>
      <p>{profileUrl(profile)}</p>
      {#if remark}<p data-profile-remark>{remark}</p>{/if}
      {#if profile.reviewModel}
        <p data-profile-review-model>{$t("profiles.reviewModelLabel")}: {profile.reviewModel}</p>
      {/if}
    </div>
  </div>
  {#if profile.isBuiltin}
    <div class={profileCardStatusRecipe()}>
      <StatusPill status="info" label={$t("profiles.builtinOfficial")} />
    </div>
  {/if}
  <div class={profileCardActionsRecipe()}>
    <button
      class={actionButtonRecipe({ tone: "primary" })}
      disabled={active || applyingId !== null}
      title={active ? $t("profiles.alreadyActiveProfile") : $t("profiles.previewModeApply", { name: profile.name, mode: applyModeLabel() })}
      on:click={() => onApply(profile)}
    >
      <AppIcon name="apply" size={16} />
      {#if active}
        {$t("common.active")}
      {:else}
        {applyingId === actionKey ? $t("common.loading") : $t("common.apply")}
      {/if}
    </button>
    {#if canOpenUsage}
      <button class={iconButtonRecipe()} title={$t("profiles.usage.open")} disabled={actionsBusy} on:click={() => onUsage(profile)}>
        <AppIcon name="stats" size={16} />
      </button>
    {/if}
    {#if !profile.isBuiltin}
      <button class={iconButtonRecipe()} title={$t("profiles.editProfile")} disabled={duplicatingId !== null || deletingId !== null} on:click={() => onEdit(profile)}><AppIcon name="edit" size={16} /></button>
      <button class={iconButtonRecipe()} title={$t("profiles.duplicateProfile")} disabled={actionsBusy} on:click={() => onDuplicate(profile)}>
        <AppIcon name={duplicatingId === profile.id ? "loading" : "copy"} class={duplicatingId === profile.id ? spinRecipe() : undefined} size={16} />
      </button>
      <button class={iconButtonRecipe({ danger: true })} title={$t("profiles.deleteProfile")} disabled={actionsBusy} on:click={() => onDelete(profile)}>
        <AppIcon name={deletingId === profile.id ? "loading" : "delete"} class={deletingId === profile.id ? spinRecipe() : undefined} size={16} />
      </button>
    {/if}
  </div>
</article>
