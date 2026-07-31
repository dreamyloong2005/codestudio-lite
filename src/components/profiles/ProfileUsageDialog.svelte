<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    deleteUsageScript,
    loadUsageScriptState,
    queryProfileUsage,
    saveUsageScript,
    testUsageScript
  } from "../../lib/api";
  import { t, type TranslationKey } from "../../lib/i18n";
  import { createProfileUsageController } from "../../lib/profiles/profileUsageController";
  import type { ProfileDraft, UsageData, UsageQueryResult, UsageScriptTemplateType } from "../../types";
  import AppIcon from "../AppIcon.svelte";
  import StatusPill from "../StatusPill.svelte";
  import { css, cx } from "../../../styled-system/css";
  import {
    actionButtonRecipe,
    desktopClientModalActionsRecipe,
    desktopClientModalBackdropRecipe,
    desktopClientModalBodyRecipe,
    desktopClientModalPanelRecipe,
    emptyRowRecipe,
    nativeToggleRecipe,
    profileDiffHeadingRecipe,
    profileDiffPanelRecipe,
    profileFormGridRecipe,
    profileInlineNoticeRecipe,
    profileUsageCodeFieldRecipe,
    profileUsageOfficialPanelRecipe,
    profileUsageResultCardRecipe,
    profileUsageResultGridRecipe,
    profileUsageTemplateRowRecipe,
    spinRecipe
  } from "../../../styled-system/recipes";

  export let profile: ProfileDraft;
  export let formatError: (message: string) => string = (message) => message;
  export let onClose: () => void = () => {};

  const controller = createProfileUsageController({
    api: {
      load: loadUsageScriptState,
      save: saveUsageScript,
      test: testUsageScript,
      query: queryProfileUsage,
      remove: deleteUsageScript
    },
    scheduler: {
      setInterval: (callback, milliseconds) => window.setInterval(callback, milliseconds),
      clearInterval: (handle) => window.clearInterval(handle as number)
    }
  });

  const usageTemplateOptions: Array<{ id: UsageScriptTemplateType; labelKey: TranslationKey }> = [
    { id: "general", labelKey: "profiles.usage.template.general" },
    { id: "newapi", labelKey: "profiles.usage.template.newapi" },
    { id: "balance", labelKey: "profiles.usage.template.balance" },
    { id: "token_plan", labelKey: "profiles.usage.template.tokenPlan" },
    { id: "custom", labelKey: "profiles.usage.template.custom" }
  ];

  const usageModalPanelClass = css({
    width: "min(900px, calc(100vw - 40px))",
    "@supports (width: 100dvw)": {
      width: "min(900px, calc(100dvw - 40px))"
    }
  });
  const dangerButtonClass = css({
    borderColor: "color-mix(in srgb, var(--danger) 40%, transparent)",
    background: "color-mix(in srgb, var(--danger) 14%, transparent)",
    color: "var(--danger-text)",
    _hover: {
      borderColor: "color-mix(in srgb, var(--danger) 55%, transparent)",
      background: "color-mix(in srgb, var(--danger) 18%, transparent)"
    }
  });
  const usageToggleClass = css({
    borderColor: "color-mix(in srgb, var(--accent) 30%, transparent)",
    background: "color-mix(in srgb, var(--accent) 8%, transparent)"
  });
  const inlineEmptyClass = css({
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    gap: "8px"
  });

  $: state = $controller;
  $: busy = state.status !== "ready" && state.status !== "closed";
  $: canSave =
    Boolean(state.profile) &&
    (!state.form.enabled || state.officialOAuth || state.form.code.trim().length > 0) &&
    state.form.timeoutSeconds >= 2 &&
    state.form.timeoutSeconds <= 60 &&
    state.form.autoQueryIntervalMinutes >= 0 &&
    state.form.autoQueryIntervalMinutes <= 1440 &&
    state.status === "ready";
  $: error = state.error ? formatError(state.error) : null;
  $: notice = state.notice
    ? $t(`profiles.usage.${state.notice === "saved" ? "saveSuccess" : state.notice === "tested" ? "testSuccess" : state.notice === "queried" ? "querySuccess" : "deleteSuccess"}`)
    : null;

  onMount(() => void controller.open(profile));
  onDestroy(() => controller.dispose());

  function close() {
    if (controller.close()) onClose();
  }

  function updateText(field: "baseUrl" | "apiKey" | "accessToken" | "userId" | "code", event: Event) {
    controller.updateForm({ [field]: (event.currentTarget as HTMLInputElement | HTMLTextAreaElement).value });
  }

  function updateNumber(field: "timeoutSeconds" | "autoQueryIntervalMinutes", event: Event) {
    controller.updateForm({ [field]: Number((event.currentTarget as HTMLInputElement).value) });
  }

  function formatUsageValue(value: number | null | undefined, unit: string | null | undefined) {
    if (typeof value !== "number" || Number.isNaN(value)) return $t("common.none");
    const formatted = Math.abs(value) >= 1000 ? value.toLocaleString() : value.toFixed(2).replace(/\.00$/, "");
    return unit ? `${formatted} ${unit}` : formatted;
  }

  function usageItemTitle(item: UsageData, index: number) {
    return item.planName || $t("profiles.usage.resultPlanFallback", { index: index + 1 });
  }

  function usageQueriedAt(result: UsageQueryResult | null) {
    return result?.queriedAt ? new Date(result.queriedAt).toLocaleString() : $t("profiles.usage.neverQueried");
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      if (!busy) {
        event.preventDefault();
        close();
      }
      return;
    }
    if (event.key === "Enter" && canSave && !keyboardTargetOwnsEnter(event.target)) {
      event.preventDefault();
      void controller.save();
    }
  }

  function keyboardTargetOwnsEnter(target: EventTarget | null) {
    if (!(target instanceof HTMLElement)) return false;
    return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName);
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class={desktopClientModalBackdropRecipe()} role="presentation">
  <div class={cx(desktopClientModalPanelRecipe(), usageModalPanelClass)} role="dialog" aria-modal="true" aria-labelledby="usage-title">
    <div class={desktopClientModalBodyRecipe()}>
      <div>
        <h2 id="usage-title">{$t("profiles.usage.title", { name: profile.name })}</h2>
      </div>

      {#if error}<div class={profileInlineNoticeRecipe({ tone: "error" })}>{error}</div>{/if}
      {#if notice}<div class={profileInlineNoticeRecipe({ tone: "success" })}>{notice}</div>{/if}

      {#if state.status === "loading"}
        <div class={cx(emptyRowRecipe(), inlineEmptyClass)}>
          <AppIcon name="loading" class={spinRecipe()} size={18} />
          {$t("common.loading")}
        </div>
      {:else}
        <label class={cx(nativeToggleRecipe(), usageToggleClass)} data-native-toggle>
          <input
            type="checkbox"
            checked={state.form.enabled}
            disabled={busy}
            on:change={(event) => controller.updateForm({ enabled: event.currentTarget.checked })}
          />
          <span><strong>{$t("profiles.usage.enabled")}</strong><small>{$t("profiles.usage.enabledDescription")}</small></span>
        </label>

        {#if state.officialOAuth}
          <div class={profileUsageOfficialPanelRecipe()}>
            <AppIcon name="stats" size={18} />
            <div><strong>{$t("profiles.usage.officialOAuth")}</strong><span>{$t("profiles.usage.officialOAuthHint")}</span></div>
          </div>
        {:else}
          <div class={profileUsageTemplateRowRecipe()}>
            {#each usageTemplateOptions as option}
              <button type="button" data-selected={state.form.templateType === option.id} disabled={busy} on:click={() => controller.selectTemplate(option.id)}>
                {$t(option.labelKey)}
              </button>
            {/each}
          </div>
          <div class={profileFormGridRecipe({ columns: "double" })}>
            <label>{$t("wizard.providerBaseUrl")}<input value={state.form.baseUrl} disabled={busy} placeholder={profile.baseUrl} on:input={(event) => updateText("baseUrl", event)} on:blur={() => controller.updateForm({ baseUrl: state.form.baseUrl.trim() })} /></label>
            <label>{$t("wizard.providerApiKey")}<input type="password" value={state.form.apiKey} disabled={busy} placeholder={$t(profile.authRef ? "profiles.usage.keepProfileKey" : "profiles.usage.keyOptional")} on:input={(event) => updateText("apiKey", event)} /></label>
            <label>{$t("profiles.usage.accessToken")}<input type="password" value={state.form.accessToken} disabled={busy} placeholder={$t("profiles.usage.accessTokenPlaceholder")} on:input={(event) => updateText("accessToken", event)} /></label>
            <label>{$t("profiles.usage.userId")}<input value={state.form.userId} disabled={busy} placeholder={$t("profiles.usage.userIdPlaceholder")} on:input={(event) => updateText("userId", event)} /></label>
            <label>{$t("profiles.usage.timeout")}<input type="number" min="2" max="60" value={state.form.timeoutSeconds} disabled={busy} on:input={(event) => updateNumber("timeoutSeconds", event)} /></label>
            <label>{$t("profiles.usage.autoInterval")}<input type="number" min="0" max="1440" value={state.form.autoQueryIntervalMinutes} disabled={busy} on:input={(event) => updateNumber("autoQueryIntervalMinutes", event)} /><small>{$t("profiles.usage.autoIntervalHint")}</small></label>
          </div>
          <label class={profileUsageCodeFieldRecipe()}><span>{$t("profiles.usage.script")}</span><textarea value={state.form.code} disabled={busy} spellcheck="false" on:input={(event) => updateText("code", event)}></textarea></label>
        {/if}

        <section class={profileDiffPanelRecipe()}>
          <div class={profileDiffHeadingRecipe()}>
            <div><strong>{$t("profiles.usage.resultTitle")}</strong><span>{$t("profiles.usage.queriedAt", { time: usageQueriedAt(state.result) })}</span></div>
            <StatusPill status={state.result?.success ? "ok" : "info"} label={state.result?.success ? $t("common.ok") : $t("profiles.usage.noResult")} />
          </div>
          {#if state.result?.data.length}
            <div class={profileUsageResultGridRecipe()}>
              {#each state.result.data as item, index}
                <div class={profileUsageResultCardRecipe()} data-invalid={item.isValid === false}>
                  <strong>{usageItemTitle(item, index)}</strong>
                  {#if item.isValid === false}<span>{item.invalidMessage ?? $t("profiles.usage.invalid")}</span>{/if}
                  <dl>
                    <div><dt>{$t("profiles.usage.remaining")}</dt><dd data-usage-balance>{formatUsageValue(item.remaining, item.unit)}</dd></div>
                    <div><dt>{$t("profiles.usage.used")}</dt><dd>{formatUsageValue(item.used, item.unit)}</dd></div>
                    <div><dt>{$t("profiles.usage.total")}</dt><dd>{formatUsageValue(item.total, item.unit)}</dd></div>
                  </dl>
                  {#if item.extra}<small>{item.extra}</small>{/if}
                </div>
              {/each}
            </div>
          {:else}
            <div class={emptyRowRecipe()}>{$t("profiles.usage.emptyResult")}</div>
          {/if}
        </section>
      {/if}
    </div>

    <div class={desktopClientModalActionsRecipe()}>
      <button class={actionButtonRecipe()} disabled={busy} on:click={close}>{$t("common.close")}</button>
      {#if state.loaded?.config && !state.officialOAuth}
        <button class={cx(actionButtonRecipe(), dangerButtonClass)} disabled={busy} on:click={() => controller.remove()}>
          <AppIcon name={state.status === "deleting" ? "loading" : "delete"} class={state.status === "deleting" ? spinRecipe() : ""} size={16} />{$t("profiles.usage.delete")}
        </button>
      {/if}
      {#if !state.officialOAuth}
        <button class={actionButtonRecipe()} disabled={!canSave || busy} on:click={() => controller.test()}>
          <AppIcon name={state.status === "testing" ? "loading" : "play"} class={state.status === "testing" ? spinRecipe() : ""} size={16} />{$t("profiles.usage.test")}
        </button>
      {/if}
      <button class={actionButtonRecipe({ tone: state.officialOAuth ? "primary" : "secondary" })} disabled={!state.loaded?.config?.enabled || busy} on:click={() => controller.query()}>
        <AppIcon name={state.status === "querying" ? "loading" : "stats"} class={state.status === "querying" ? spinRecipe() : ""} size={16} />{$t("profiles.usage.query")}
      </button>
      <button class={actionButtonRecipe({ tone: "primary" })} disabled={!canSave || busy} on:click={() => controller.save()}>
        <AppIcon name={state.status === "saving" ? "loading" : "apply"} class={state.status === "saving" ? spinRecipe() : ""} size={16} />{state.status === "saving" ? $t("common.saving") : $t("common.save")}
      </button>
    </div>
  </div>
</div>
