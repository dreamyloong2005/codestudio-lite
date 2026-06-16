<script lang="ts">
  import { ExternalLink, Languages, RefreshCw, SunMoon, UserRound } from "@lucide/svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import {
    APP_NAME,
    APP_VERSION_LABEL,
    AUTHOR_GITHUB_URL,
    AUTHOR_NAME
  } from "../lib/appInfo";
  import { appUpdateState, checkForAppUpdate } from "../lib/appUpdateStore";
  import { loadAppSettings, updateAppSettings } from "../lib/api";
  import { setLocale, supportedLocales, t } from "../lib/i18n";
  import { applyTheme } from "../lib/theme";
  import type { AppSettings, Locale } from "../types";

  let settings: AppSettings | null = null;
  let language: Locale = "zh-CN";
  let theme: AppSettings["theme"] = "system";
  let saving = false;
  let message: string | null = null;
  let error: string | null = null;

  onMount(() => {
    void loadSettings();
    if ($appUpdateState.status === "idle") {
      void checkForAppUpdate();
    }
  });

  async function loadSettings() {
    try {
      settings = await loadAppSettings();
      language = settings.language;
      theme = settings.theme;
      setLocale(language);
      applyTheme(theme);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function changeLanguage(nextLanguage: Locale) {
    language = nextLanguage;
    setLocale(nextLanguage);
    await saveSettings({ language: nextLanguage });
  }

  async function changeTheme(nextTheme: AppSettings["theme"]) {
    theme = nextTheme;
    applyTheme(nextTheme);
    await saveSettings({ theme: nextTheme });
  }

  async function saveSettings(request: { language?: Locale; theme?: AppSettings["theme"] }) {
    saving = true;
    error = null;
    message = null;
    try {
      settings = await updateAppSettings(request);
      message = $t("settings.saved");
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err);
      error = $t("settings.saveFailed", { message: detail });
    } finally {
      saving = false;
    }
  }

  async function openExternalUrl(url: string) {
    try {
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noreferrer");
    }
  }

  $: updateStatusLabel = (() => {
    if ($appUpdateState.status === "checking") {
      return $t("settings.checkingUpdates");
    }
    if ($appUpdateState.status === "available" && $appUpdateState.latestVersion) {
      return $t("settings.updateAvailable", { version: $appUpdateState.latestVersion });
    }
    if ($appUpdateState.status === "upToDate") {
      return $t("settings.upToDate");
    }
    if ($appUpdateState.status === "noRelease") {
      return $t("settings.noRelease");
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
</script>

<div class="route-stack">
  <section class="top-strip">
    <div>
      <span class="eyebrow">{$t("settings.eyebrow")}</span>
      <h1>{$t("settings.title")}</h1>
      <p>{$t("settings.subtitle")}</p>
    </div>
  </section>

  {#if message}
    <div class="inline-success">{message}</div>
  {/if}
  {#if error}
    <div class="inline-error">{error}</div>
  {/if}

  <section class="panel-band settings-list">
    <label class="settings-row">
      <span><Languages size={18} /> {$t("settings.language")}</span>
      <select bind:value={language} disabled={saving} on:change={(event) => changeLanguage(event.currentTarget.value as Locale)}>
        {#each supportedLocales as locale}
          <option value={locale.code}>{$t(locale.labelKey)}</option>
        {/each}
      </select>
    </label>
    <label class="settings-row">
      <span><SunMoon size={18} /> {$t("settings.theme")}</span>
      <select bind:value={theme} disabled={saving} on:change={(event) => changeTheme(event.currentTarget.value as AppSettings["theme"])}>
        <option value="system">{$t("settings.theme.system")}</option>
        <option value="light">{$t("settings.theme.light")}</option>
        <option value="dark">{$t("settings.theme.dark")}</option>
      </select>
    </label>
  </section>

  <section class="panel-band about-panel">
    <div class="section-heading compact">
      <div>
        <h2>{$t("settings.about")}</h2>
        <p>{$t("settings.aboutDescription")}</p>
      </div>
    </div>

    <div class="about-content">
      <div class="about-summary">
        <div class="brand-mark about-mark">
          <svg viewBox="0 0 256 256" role="img" aria-hidden="true">
            <rect x="12" y="12" width="232" height="232" rx="50" fill="var(--brand-icon-bg)" />
            <path
              d="M210 128H176L151 202L105 54L80 128H46"
              fill="none"
              stroke="var(--brand-icon-ink)"
              stroke-width="24"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </div>
        <div class="about-title">
          <strong>{APP_NAME}</strong>
          <span>{APP_VERSION_LABEL}</span>
        </div>
        <div class="about-update">
          <span class={`pill ${updateStatusTone}`}>{updateStatusLabel}</span>
          <button class="secondary-button" type="button" disabled={$appUpdateState.status === "checking"} on:click={() => checkForAppUpdate(true)}>
            <RefreshCw size={15} class={$appUpdateState.status === "checking" ? "spin" : ""} />
            {$t("settings.checkUpdates")}
          </button>
        </div>
      </div>

      <div class="settings-row about-row">
        <span><UserRound size={18} /> {$t("settings.author")}</span>
        <a class="secondary-button" href={AUTHOR_GITHUB_URL} target="_blank" rel="noreferrer" on:click|preventDefault={() => openExternalUrl(AUTHOR_GITHUB_URL)}>
          {AUTHOR_NAME}
          <ExternalLink size={15} />
        </a>
      </div>
    </div>
  </section>
</div>
