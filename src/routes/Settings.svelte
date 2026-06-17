<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import BrandLogo from "../components/BrandLogo.svelte";
  import {
    APP_NAME,
    APP_VERSION_LABEL,
    AUTHOR_GITHUB_URL,
    AUTHOR_NAME
  } from "../lib/appInfo";
  import { appUpdateState, checkForAppUpdate } from "../lib/appUpdateStore";
  import { ensureAppDirs, loadAppSettings, updateAppSettings } from "../lib/api";
  import { setLocale, supportedLocales, t } from "../lib/i18n";
  import { applyTheme } from "../lib/theme";
  import type { AppSettings, CodexAuthStatus, Locale } from "../types";

  let settings: AppSettings | null = null;
  let language: Locale = "zh-CN";
  let theme: AppSettings["theme"] = "system";
  let preserveCodexOfficialAuth = true;
  let codexAuth: CodexAuthStatus | null = null;
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
      const profileSummary = await ensureAppDirs();
      language = settings.language;
      theme = settings.theme;
      preserveCodexOfficialAuth = settings.preserveCodexOfficialAuth;
      codexAuth = profileSummary.codexAuth;
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

  async function changeCodexAuthPreservation(nextValue: boolean) {
    preserveCodexOfficialAuth = nextValue;
    await saveSettings({ preserveCodexOfficialAuth: nextValue });
  }

  async function saveSettings(request: { language?: Locale; theme?: AppSettings["theme"]; preserveCodexOfficialAuth?: boolean }) {
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

  $: codexAuthStatusLabel = codexAuth
    ? codexAuth.available
      ? $t("settings.codexAuthDetected")
      : $t("settings.codexAuthMissing")
    : $t("common.unknown");
  $: codexAuthStatusTone = codexAuth?.available ? "good" : "warn";
  $: codexAuthDetail = codexAuth ? codexAuthDetailLabel(codexAuth) : "";
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

  function codexAuthDetailLabel(status: CodexAuthStatus) {
    if (status.storage === "keyring" || status.storage === "auto") {
      return $t("settings.codexAuthKeyringDetail");
    }
    if (status.available) {
      return status.path
        ? $t("settings.codexAuthFileDetail", { path: status.path })
        : $t("settings.codexAuthDetectedDetail");
    }
    return $t("settings.codexAuthMissingDetail");
  }
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
      <span><AppIcon name="language" size={18} /> {$t("settings.language")}</span>
      <select bind:value={language} disabled={saving} on:change={(event) => changeLanguage(event.currentTarget.value as Locale)}>
        {#each supportedLocales as locale}
          <option value={locale.code}>{$t(locale.labelKey)}</option>
        {/each}
      </select>
    </label>
    <label class="settings-row">
      <span><AppIcon name="theme" size={18} /> {$t("settings.theme")}</span>
      <select bind:value={theme} disabled={saving} on:change={(event) => changeTheme(event.currentTarget.value as AppSettings["theme"])}>
        <option value="system">{$t("settings.theme.system")}</option>
        <option value="light">{$t("settings.theme.light")}</option>
        <option value="dark">{$t("settings.theme.dark")}</option>
      </select>
    </label>
    <label class="settings-row settings-toggle-row">
      <span><AppIcon name="key" size={18} /> {$t("settings.codexAuthPreservation")}</span>
      <input
        type="checkbox"
        checked={preserveCodexOfficialAuth}
        disabled={saving}
        aria-label={$t("settings.codexAuthPreservation")}
        on:change={(event) => changeCodexAuthPreservation(event.currentTarget.checked)}
      />
    </label>
    <div class="settings-row codex-auth-row">
      <span><AppIcon name="key" size={18} /> {$t("settings.codexOAuthStatus")}</span>
      <div class="settings-row-value">
        <span class={`pill ${codexAuthStatusTone}`}>{codexAuthStatusLabel}</span>
        <small>{codexAuthDetail}</small>
      </div>
    </div>
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
          <BrandLogo />
        </div>
        <div class="about-title">
          <strong>{APP_NAME}</strong>
          <span>{APP_VERSION_LABEL}</span>
        </div>
        <div class="about-update">
          <span class={`pill ${updateStatusTone}`}>{updateStatusLabel}</span>
          <button class="secondary-button" type="button" disabled={$appUpdateState.status === "checking"} on:click={() => checkForAppUpdate(true)}>
            <AppIcon name="restart" size={15} class={$appUpdateState.status === "checking" ? "spin" : ""} />
            {$t("settings.checkUpdates")}
          </button>
        </div>
      </div>

      <div class="settings-row about-row">
        <span><AppIcon name="user" size={18} /> {$t("settings.author")}</span>
        <a class="secondary-button" href={AUTHOR_GITHUB_URL} target="_blank" rel="noreferrer" on:click|preventDefault={() => openExternalUrl(AUTHOR_GITHUB_URL)}>
          {AUTHOR_NAME}
          <AppIcon name="externalLink" size={15} />
        </a>
      </div>
    </div>
  </section>
</div>
