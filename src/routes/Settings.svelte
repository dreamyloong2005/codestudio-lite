<script lang="ts">
  import { onMount } from "svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import BrandLogo from "../components/BrandLogo.svelte";
  import {
    APP_NAME,
    APP_VERSION_LABEL,
    AUTHOR_GITHUB_URL,
    AUTHOR_NAME
  } from "../lib/appInfo";
  import { appUpdateState, checkForAppUpdate, installAppUpdate } from "../lib/appUpdateStore";
  import { loadAppSettings, openExternalUrl, updateAppSettings } from "../lib/api";
  import { setLocale, supportedLocales, t } from "../lib/i18n";
  import { applyTheme } from "../lib/theme";
  import type { AppSettings, Locale } from "../types";
  import { cx } from "../../styled-system/css";
  import {
    actionButtonRecipe,
    panelRecipe,
    profileInlineNoticeRecipe,
    routeStackRecipe,
    sectionHeadingRecipe,
    settingsAboutContentRecipe,
    settingsAboutMarkRecipe,
    settingsAboutPanelRecipe,
    settingsAboutSummaryRecipe,
    settingsAboutTitleRecipe,
    settingsAboutUpdateRecipe,
    settingsListRecipe,
    settingsRowRecipe,
    settingsUpdatePillRecipe,
    spinRecipe,
    topStripRecipe
  } from "../../styled-system/recipes";

  type UpdateStatusTone = "warn" | "bad" | "info" | "good";

  let language: Locale = "en-US";
  let theme: AppSettings["theme"] = "system";
  let saving = false;
  let error: string | null = null;
  let settingsEditRevision = 0;
  let updateStatusTone: UpdateStatusTone = "info";
  let updateProgressPercent = 0;
  let updateBusy = false;

  onMount(() => {
    void loadSettings();
    if ($appUpdateState.status === "idle") {
      void checkForAppUpdate();
    }
  });

  async function loadSettings() {
    const loadRevision = settingsEditRevision;
    try {
      const settings = await loadAppSettings();
      if (loadRevision !== settingsEditRevision) {
        return;
      }
      language = settings.language;
      theme = settings.theme;
      setLocale(language);
      applyTheme(theme);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function changeLanguage(nextLanguage: Locale) {
    settingsEditRevision += 1;
    language = nextLanguage;
    setLocale(nextLanguage);
    await saveSettings({ language: nextLanguage });
  }

  async function changeTheme(nextTheme: AppSettings["theme"]) {
    settingsEditRevision += 1;
    theme = nextTheme;
    applyTheme(nextTheme);
    await saveSettings({ theme: nextTheme });
  }

  async function saveSettings(request: {
    language?: Locale;
    theme?: AppSettings["theme"];
  }) {
    saving = true;
    try {
      await updateAppSettings(request);
    } catch {
      // Settings auto-save is best-effort; keep the UI quiet on rare write failures.
    } finally {
      saving = false;
    }
  }

  $: updateStatusLabel = (() => {
    if ($appUpdateState.status === "checking") {
      return $t("settings.checkingUpdates");
    }
    if ($appUpdateState.status === "downloading") {
      return $t("settings.downloadingUpdate", { percent: updateProgressPercent });
    }
    if ($appUpdateState.status === "installing") {
      return $t("settings.installingUpdate");
    }
    if ($appUpdateState.status === "available" && $appUpdateState.latestVersion) {
      return $t("settings.updateAvailable", { version: $appUpdateState.latestVersion });
    }
    if ($appUpdateState.status === "upToDate") {
      return $t("settings.upToDate");
    }
    if ($appUpdateState.status === "unconfigured") {
      return $t("settings.updaterNotConfigured");
    }
    if ($appUpdateState.status === "error") {
      return $t("settings.updateFailed", { message: $appUpdateState.error ?? $t("common.unknown") });
    }
    return $t("settings.updateNotChecked");
  })();

  $: updateStatusTone = $appUpdateState.updateAvailable
    ? "warn"
    : $appUpdateState.status === "error"
      ? "bad"
    : $appUpdateState.status === "idle"
        ? "info"
        : "good";
  $: updateProgressPercent = $appUpdateState.totalBytes
    ? Math.min(100, Math.round(($appUpdateState.downloadedBytes / $appUpdateState.totalBytes) * 100))
    : 0;
  $: updateBusy = ["checking", "downloading", "installing"].includes($appUpdateState.status);

</script>

<div class={routeStackRecipe({ width: "full" })}>
  <section class={topStripRecipe({ compact: true })}>
    <div>
      <h1>{$t("settings.title")}</h1>
      <p>{$t("settings.subtitle")}</p>
    </div>
  </section>

  {#if error}
    <div class={profileInlineNoticeRecipe({ tone: "error" })}>{error}</div>
  {/if}

  <section class={cx(panelRecipe(), settingsListRecipe())}>
    <label class={settingsRowRecipe()}>
      <span><AppIcon name="language" size={18} /> {$t("settings.language")}</span>
      <select bind:value={language} disabled={saving} on:change={(event) => changeLanguage(event.currentTarget.value as Locale)}>
        {#each supportedLocales as locale}
          <option value={locale.code}>{locale.label}</option>
        {/each}
      </select>
    </label>
    <label class={settingsRowRecipe()}>
      <span><AppIcon name="theme" size={18} /> {$t("settings.theme")}</span>
      <select bind:value={theme} disabled={saving} on:change={(event) => changeTheme(event.currentTarget.value as AppSettings["theme"])}>
        <option value="system">{$t("settings.theme.system")}</option>
        <option value="light">{$t("settings.theme.light")}</option>
        <option value="dark">{$t("settings.theme.dark")}</option>
      </select>
    </label>
  </section>

  <section class={cx(panelRecipe(), settingsAboutPanelRecipe())}>
    <div class={sectionHeadingRecipe({ compact: true })}>
      <div>
        <h2>{$t("settings.about")}</h2>
        <p>{$t("settings.aboutDescription")}</p>
      </div>
    </div>

    <div class={settingsAboutContentRecipe()}>
      <div class={settingsAboutSummaryRecipe()}>
        <div class={settingsAboutMarkRecipe()}>
          <BrandLogo />
        </div>
        <div class={settingsAboutTitleRecipe()}>
          <strong>{APP_NAME}</strong>
          <span>{APP_VERSION_LABEL}</span>
        </div>
        <div class={settingsAboutUpdateRecipe()}>
          <span class={settingsUpdatePillRecipe({ tone: updateStatusTone })}>{updateStatusLabel}</span>
          {#if $appUpdateState.updateAvailable && $appUpdateState.installable}
            <button
              class={actionButtonRecipe({ tone: "primary" })}
              type="button"
              title={$t("settings.installUpdate")}
              disabled={updateBusy}
              on:click={() => installAppUpdate()}
            >
              <AppIcon name="download" size={15} />
              {$t("settings.updateNow")}
            </button>
          {/if}
          <button class={actionButtonRecipe()} type="button" disabled={updateBusy} on:click={() => checkForAppUpdate(true)}>
            <AppIcon name="restart" size={15} class={updateBusy ? spinRecipe() : ""} />
            {$t("settings.checkUpdates")}
          </button>
        </div>
      </div>

      <div class={settingsRowRecipe()}>
        <span><AppIcon name="user" size={18} /> {$t("settings.author")}</span>
        <a class={actionButtonRecipe()} href={AUTHOR_GITHUB_URL} target="_blank" rel="noreferrer" on:click|preventDefault={() => openExternalUrl(AUTHOR_GITHUB_URL)}>
          {AUTHOR_NAME}
          <AppIcon name="externalLink" size={15} />
        </a>
      </div>
    </div>
  </section>
</div>
