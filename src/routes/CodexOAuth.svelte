<script lang="ts">
  import AppIcon from "../components/AppIcon.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import ToolIcon from "../components/ToolIcon.svelte";
  import {
    applyProfile,
    loadAppSettings,
    previewProfileApply,
    updateAppSettings
  } from "../lib/api";
  import { t, type TranslationKey } from "../lib/i18n";
  import type {
    AppSettings,
    ApplyProfileResult,
    CodexAuthStatus,
    DetectionSnapshot,
    PreviewProfileApplyResult,
    ProfileDraft,
    ProfileSummary
  } from "../types";

  export let summary: ProfileSummary | null = null;
  export let snapshot: DetectionSnapshot | null = null;
  export let onProfileSwitched: () => void | Promise<void> = () => {};

  let settings: AppSettings | null = null;
  let preserveCodexOfficialAuth = true;
  let loadingSettings = false;
  let preview: PreviewProfileApplyResult | null = null;
  let result: ApplyProfileResult | null = null;
  let busy: "preview" | "apply" | "setting" | null = null;
  let error: string | null = null;
  let message: string | null = null;
  let confirmApply = false;
  let previewRequestedFor: string | null = null;
  let settingsLoadAttempted = false;

  $: codexOAuthProfile = findCodexOAuthProfile(summary);
  $: codexConfigProfile = findActiveCodexConfigProfile(summary);
  $: codexApiProfileActive =
    Boolean(codexConfigProfile) && codexConfigProfile?.provider.trim() !== "official";
  $: codexAuth = summary?.codexAuth ?? snapshot?.codexAuth ?? null;
  $: codexAuthDetected = Boolean(codexAuth?.available);
  $: codexAuthStatusLabel = codexAuth
    ? codexAuth.available
      ? $t("codexOAuth.status.detected")
      : $t("codexOAuth.status.missing")
    : $t("common.unknown");
  $: codexAuthStatusTone = codexAuth?.available ? "ok" as const : "warning" as const;
  $: codexAuthDetail = codexAuth ? codexAuthDetailLabel(codexAuth) : $t("codexOAuth.status.pending");
  $: oauthActive = Boolean(codexOAuthProfile && isProfileActive(codexOAuthProfile));
  $: applyNeedsConfirmation = codexApiProfileActive && !preserveCodexOfficialAuth;
  $: nativeDiff = preview?.modePreviews.find((mode) => mode.mode === "config")?.nativeDiff ?? preview?.nativeDiff ?? null;

  $: if (!loadingSettings && settings === null && !settingsLoadAttempted) {
    void loadSettings();
  }

  $: if (codexOAuthProfile && !preview && previewRequestedFor !== codexOAuthProfile.id && busy === null) {
    void loadPreview();
  }

  async function loadSettings() {
    loadingSettings = true;
    settingsLoadAttempted = true;
    error = null;
    try {
      settings = await loadAppSettings();
      preserveCodexOfficialAuth = settings.preserveCodexOfficialAuth;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingSettings = false;
    }
  }

  async function updatePreservation(nextValue: boolean) {
    preserveCodexOfficialAuth = nextValue;
    busy = "setting";
    error = null;
    message = null;
    try {
      settings = await updateAppSettings({ preserveCodexOfficialAuth: nextValue });
      preserveCodexOfficialAuth = settings.preserveCodexOfficialAuth;
      if (preserveCodexOfficialAuth) {
        confirmApply = false;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      preserveCodexOfficialAuth = settings?.preserveCodexOfficialAuth ?? true;
    } finally {
      busy = null;
    }
  }

  async function loadPreview() {
    if (!codexOAuthProfile) {
      return;
    }
    busy = "preview";
    previewRequestedFor = codexOAuthProfile.id;
    error = null;
    try {
      preview = await previewProfileApply({ profileId: codexOAuthProfile.id });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = null;
    }
  }

  async function applyOAuth() {
    if (!codexOAuthProfile || busy !== null || oauthActive) {
      return;
    }

    if (applyNeedsConfirmation && !confirmApply) {
      confirmApply = true;
      return;
    }

    busy = "apply";
    error = null;
    message = null;
    result = null;

    try {
      result = await applyProfile({ profileId: codexOAuthProfile.id });
      await onProfileSwitched();
      message = $t("codexOAuth.applySuccess");
      confirmApply = false;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = null;
    }
  }

  function findCodexOAuthProfile(profileSummary: ProfileSummary | null): ProfileDraft | null {
    return profileSummary?.drafts.find((profile) =>
      canonicalProfileToolId(profile.app) === "codex" &&
      profile.mode === "config" &&
      profile.provider.trim() === "official"
    ) ?? null;
  }

  function findActiveCodexConfigProfile(profileSummary: ProfileSummary | null): ProfileDraft | null {
    const activeId =
      profileSummary?.activeProfilesByMode.config.codex ??
      profileSummary?.activeProfilesByMode.config["codex-app"] ??
      null;
    if (!activeId) {
      return null;
    }
    return profileSummary?.drafts.find((profile) => profile.id === activeId) ?? null;
  }

  function isProfileActive(profile: ProfileDraft) {
    const activeId =
      summary?.activeProfilesByMode.config.codex ??
      summary?.activeProfilesByMode.config["codex-app"] ??
      null;
    return activeId === profile.id;
  }

  function canonicalProfileToolId(toolId: string) {
    const normalized = toolId.trim().toLowerCase();
    return [
      "codex",
      "codex-app",
      "codex-client",
      "codex-desktop",
      "codex-cli",
      "codex-vscode",
      "codex-code-vscode",
      "codex-vs-code"
    ].includes(normalized)
      ? "codex"
      : normalized;
  }

  function codexAuthDetailLabel(status: CodexAuthStatus) {
    if (status.storage === "keyring" || status.storage === "auto") {
      return $t("codexOAuth.status.keyring");
    }
    if (status.available) {
      return status.path
        ? $t("codexOAuth.status.detectedAt", { path: status.path })
        : $t("codexOAuth.status.detectedDetail");
    }
    return $t("codexOAuth.status.missingDetail");
  }

  function diffActionLabel(action: string) {
    if (action === "add") {
      return $t("common.add");
    }
    if (action === "update") {
      return $t("common.update");
    }
    if (action === "remove") {
      return $t("common.remove");
    }
    if (action === "unchanged") {
      return $t("common.unchanged");
    }
    return action.replaceAll("_", " ");
  }

  function previewTextLabel(message: string) {
    const exact: Partial<Record<string, TranslationKey>> = {
      "Official provider uses the target client's own login.": "profiles.warning.officialClientLogin",
      "No Provider API key or model override is required.": "profiles.warning.noProviderKeyOrModel",
      "Changing Codex config usually requires restarting Codex or opening a new Codex session.": "profiles.warning.codexReloadRequired",
      "Selects Codex's official OpenAI provider.": "profiles.diff.selectOfficialProvider",
      "Keeps a readable label for the official provider.": "profiles.diff.officialProviderLabel",
      "Uses Codex's supported official provider wire API.": "profiles.diff.officialWireApi",
      "Uses Codex's selected provider wire API.": "profiles.diff.officialWireApi",
      "Keeps Codex official login as the authentication source.": "profiles.diff.officialLoginAuth",
      "Official login does not require a Provider API key.": "profiles.diff.officialNoApiKey",
      "Official provider can use Codex's own model default.": "profiles.diff.officialModelDefault",
      "Sets Codex to the selected official model.": "profiles.diff.officialModel",
      "Keeps Codex API tokens scoped to the active provider so auth.json can preserve the official login.": "profiles.diff.codexScopedApiTokens",
      "Removes a legacy API-key mirror from Codex config.toml without touching auth.json.": "profiles.diff.removeLegacyApiKeyMirror",
      "Removes a legacy environment-style API key from Codex config.toml.": "profiles.diff.removeLegacyEnvApiKey"
    };
    return exact[message] ? $t(exact[message]) : message;
  }
</script>

<div class="route-stack">
  <section class="top-strip">
    <div>
      <span class="eyebrow">{$t("codexOAuth.eyebrow")}</span>
      <h1>{$t("codexOAuth.title")}</h1>
      <p>{$t("codexOAuth.subtitle")}</p>
    </div>
  </section>

  {#if error}
    <div class="inline-error">{error}</div>
  {/if}
  {#if message}
    <div class="inline-success">{message}</div>
  {/if}

  <section class="panel-band codex-oauth-panel">
    <div class="section-heading compact">
      <div class="tool-section-title">
        <ToolIcon toolId="codex" label="Codex" variant="heading" />
        <div>
          <h2>{$t("codexOAuth.statusTitle")}</h2>
          <p>{codexAuthDetail}</p>
        </div>
      </div>
      <StatusPill status={codexAuthStatusTone} label={codexAuthStatusLabel} />
    </div>

    <div class="oauth-grid">
      <div class="oauth-card">
        <strong>{$t("codexOAuth.activeConfig")}</strong>
        <span>{codexConfigProfile?.name ?? $t("profiles.noActiveForToolInMode")}</span>
        <small>
          {#if oauthActive}
            {$t("codexOAuth.activeOAuth")}
          {:else if codexApiProfileActive}
            {$t("codexOAuth.activeApi", { name: codexConfigProfile?.name ?? "API" })}
          {:else}
            {$t("codexOAuth.activeNone")}
          {/if}
        </small>
      </div>

      <label class="oauth-toggle">
        <input
          type="checkbox"
          checked={preserveCodexOfficialAuth}
          disabled={busy === "setting"}
          on:change={(event) => updatePreservation(event.currentTarget.checked)}
        />
        <span>
          <strong>{$t("settings.codexAuthPreservation")}</strong>
          <small>{$t("codexOAuth.preserveDescription")}</small>
        </span>
      </label>
    </div>
  </section>

  {#if applyNeedsConfirmation && confirmApply}
    <section class="native-diff oauth-confirm-panel">
      <div class="native-diff-heading">
        <div>
          <strong>{$t("codexOAuth.confirmTitle")}</strong>
          <span>{$t("codexOAuth.confirmDescription", { name: codexConfigProfile?.name ?? "API" })}</span>
        </div>
        <button class="secondary-button" type="button" disabled={busy !== null} on:click={() => (confirmApply = false)}>
          {$t("common.cancel")}
        </button>
      </div>
    </section>
  {/if}

  <section class="panel-band">
    <div class="section-heading compact">
      <div>
        <h2>{$t("codexOAuth.applyTitle")}</h2>
        <p>{$t("codexOAuth.applyDescription")}</p>
      </div>
      <button
        class="primary-button"
        type="button"
        disabled={!codexOAuthProfile || busy !== null || oauthActive}
        on:click={applyOAuth}
      >
        {#if busy === "apply"}
          <AppIcon name="loading" class="spin" size={16} />
          {$t("common.loading")}
        {:else if oauthActive}
          <AppIcon name="check" size={16} />
          {$t("common.active")}
        {:else if applyNeedsConfirmation && confirmApply}
          <AppIcon name="warning" size={16} />
          {$t("codexOAuth.confirmApply")}
        {:else}
          <AppIcon name="apply" size={16} />
          {$t("codexOAuth.applyButton")}
        {/if}
      </button>
    </div>

    {#if !codexOAuthProfile}
      <div class="empty-row">{$t("codexOAuth.profileMissing")}</div>
    {:else if busy === "preview" && !preview}
      <div class="empty-row">
        <AppIcon name="loading" class="spin" size={18} />
        {$t("common.loading")}
      </div>
    {:else if nativeDiff}
      <div class="native-diff oauth-diff">
        <div class="native-diff-heading">
          <div>
            <strong>{$t("codexOAuth.previewTitle")}</strong>
            <span>{nativeDiff.path}</span>
          </div>
          <StatusPill status="info" label={nativeDiff.writeEnabled ? $t("common.writeEnabled") : $t("common.readOnly")} />
        </div>
        <div class="native-diff-list">
          {#each nativeDiff.changes as change}
            <div class="native-diff-row">
              <span>{diffActionLabel(change.action)}</span>
              <code>{change.key}</code>
              <div>
                <b>{$t("common.before")}</b>
                <em>{change.before ?? $t("profiles.beforeMissing")}</em>
              </div>
              <div>
                <b>{$t("common.after")}</b>
                <em>{change.after ?? $t("profiles.afterRemoved")}</em>
              </div>
              <p>{previewTextLabel(change.detail)}</p>
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="empty-row">{$t("codexOAuth.previewEmpty")}</div>
    {/if}

    {#if result}
      <div class="inline-success">{$t("codexOAuth.applyVerified", { path: result.nativePath ?? result.appliedPath })}</div>
    {/if}
  </section>
</div>
