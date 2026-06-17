<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import AppIcon, { type AppIconName } from "./components/AppIcon.svelte";
  import BrandLogo from "./components/BrandLogo.svelte";
  import {
    detectEnvironment,
    ensureAppDirs,
    loadAppSettings,
    loadCachedDetection,
    loadGatewayStatus,
    restartGateway,
    startGateway,
    stopGateway
  } from "./lib/api";
  import { appUpdateState, checkForAppUpdate } from "./lib/appUpdateStore";
  import { setLocale, t } from "./lib/i18n";
  import { applyTheme } from "./lib/theme";
  import CodexClient from "./routes/CodexClient.svelte";
  import CodexOAuth from "./routes/CodexOAuth.svelte";
  import Dashboard from "./routes/Dashboard.svelte";
  import Profiles from "./routes/Profiles.svelte";
  import SettingsRoute from "./routes/Settings.svelte";
  import SetupWizard from "./routes/SetupWizard.svelte";
  import type {
    DetectionSnapshot,
    GatewayStatus,
    ProfileSummary,
    ToolStatus,
    WizardPrefill
  } from "./types";

  type Route = "dashboard" | "codexClient" | "codexOAuth" | "wizard" | "profiles" | "settings";

  let route: Route = "dashboard";
  let dashboardLoading = true;
  let snapshot: DetectionSnapshot | null = null;
  let gatewayStatus: GatewayStatus | null = null;
  let profileSummary: ProfileSummary | null = null;
  let error: string | null = null;
  let gatewayBusy = false;
  let wizardPrefill: WizardPrefill | null = null;
  let backgroundDetectionTimers: number[] = [];
  let backgroundDetectionInterval: number | null = null;
  let dashboardRefreshRunId = 0;
  let visibleRefreshRunId: number | null = null;

  const BACKGROUND_DETECTION_WARMUP_DELAYS_MS = [3500, 12000];
  const BACKGROUND_DETECTION_INTERVAL_MS = 30000;

  const navItems: Array<{ id: Route; labelKey: Parameters<typeof $t>[0]; icon: AppIconName }> = [
    { id: "dashboard", labelKey: "app.nav.dashboard", icon: "dashboard" },
    { id: "codexClient", labelKey: "app.nav.codexClient", icon: "codexClient" },
    { id: "codexOAuth", labelKey: "app.nav.codexOAuth", icon: "key" },
    { id: "wizard", labelKey: "app.nav.wizard", icon: "wizard" },
    { id: "profiles", labelKey: "app.nav.profiles", icon: "profiles" },
    { id: "settings", labelKey: "app.nav.settings", icon: "settings" }
  ];

  $: activeProfileId = snapshot?.activeProfile ?? profileSummary?.activeProfile ?? null;
  $: activeProfileName =
    snapshot?.activeProfileName ??
    profileSummary?.activeProfileName ??
    profileSummary?.drafts.find((profile) => profile.id === activeProfileId)?.name ??
    null;
  $: sidebarGatewayState = gatewayStatus?.running ? $t("common.running") : $t("common.stopped");
  $: sidebarGatewayTone = gatewayStatus?.running ? "online" : "offline";

  async function copyGatewayUrl() {
    if (!gatewayStatus?.baseUrl) {
      return;
    }
    await navigator.clipboard?.writeText(gatewayStatus.baseUrl);
  }

  function selectRoute(nextRoute: Route) {
    if (nextRoute === "wizard") {
      wizardPrefill = null;
    }
    route = nextRoute;
  }

  function clearBackgroundDetection() {
    for (const timer of backgroundDetectionTimers) {
      window.clearTimeout(timer);
    }
    backgroundDetectionTimers = [];
    if (backgroundDetectionInterval !== null) {
      window.clearInterval(backgroundDetectionInterval);
      backgroundDetectionInterval = null;
    }
  }

  function startBackgroundDetection() {
    clearBackgroundDetection();
    backgroundDetectionTimers = BACKGROUND_DETECTION_WARMUP_DELAYS_MS.map((delay) =>
      window.setTimeout(() => {
        void refreshDashboard({ quiet: true, scheduleFollowup: false });
      }, delay)
    );
    backgroundDetectionInterval = window.setInterval(() => {
      void refreshDashboard({ quiet: true, scheduleFollowup: false });
    }, BACKGROUND_DETECTION_INTERVAL_MS);
  }

  type RefreshDashboardOptions = { quiet?: boolean; scheduleFollowup?: boolean };

  async function refreshDashboard(options: RefreshDashboardOptions = {}) {
    const quiet = options.quiet ?? false;
    const scheduleFollowup = options.scheduleFollowup ?? true;
    const runId = ++dashboardRefreshRunId;
    if (!quiet) {
      visibleRefreshRunId = runId;
      dashboardLoading = true;
      error = null;
    }
    try {
      const [nextProfileSummary, nextSnapshot, nextGatewayStatus] = await Promise.all([
        ensureAppDirs(),
        detectEnvironment(),
        loadGatewayStatus()
      ]);
      if (runId !== dashboardRefreshRunId) {
        return;
      }
      profileSummary = nextProfileSummary;
      snapshot = nextSnapshot;
      gatewayStatus = nextGatewayStatus;
    } catch (err) {
      if (!quiet && visibleRefreshRunId === runId) {
        error = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (!quiet && visibleRefreshRunId === runId) {
        dashboardLoading = false;
        visibleRefreshRunId = null;
      }
      if (runId !== dashboardRefreshRunId) {
        return;
      }
      if (scheduleFollowup) {
        startBackgroundDetection();
      }
    }
  }

  async function loadDashboardWithCache() {
    dashboardLoading = true;
    error = null;
    try {
      const [nextProfileSummary, cachedSnapshot, nextGatewayStatus] = await Promise.all([
        ensureAppDirs(),
        loadCachedDetection(),
        loadGatewayStatus()
      ]);
      profileSummary = nextProfileSummary;
      gatewayStatus = nextGatewayStatus;
      if (cachedSnapshot) {
        snapshot = cachedSnapshot;
        dashboardLoading = false;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }

    await refreshDashboard({
      quiet: snapshot !== null,
      scheduleFollowup: true
    });
  }

  async function refreshSettings() {
    try {
      const settings = await loadAppSettings();
      setLocale(settings.language);
      applyTheme(settings.theme);
    } catch {
      // Keep the local fallback language if desktop settings cannot be read.
    }
  }

  async function refreshAfterProfileChange() {
    await refreshDashboard();
  }

  function mergeToolStatus(status: ToolStatus) {
    dashboardRefreshRunId += 1;
    const nextSnapshot = snapshot;
    if (!nextSnapshot) {
      return;
    }
    const key = status.category === "system" ? "system" : "tools";
    const collection = nextSnapshot[key];
    const existingIndex = collection.findIndex((tool) => tool.id === status.id);
    const nextCollection = existingIndex >= 0
      ? collection.map((tool) => (tool.id === status.id ? status : tool))
      : [...collection, status];
    const missingProblemId = `missing-${status.id}`;
    const unconfiguredProblemId = `unconfigured-${status.id}`;
    let problems = nextSnapshot.problems.filter((problem) => {
      if (problem.id === unconfiguredProblemId) {
        return false;
      }
      if (problem.id === missingProblemId) {
        return status.installState === "missing";
      }
      return true;
    });
    if (status.installState === "missing" && !problems.some((problem) => problem.id === missingProblemId)) {
      problems = [
        ...problems,
        {
          id: missingProblemId,
          severity: "warning",
          title: `${status.name} is missing`,
          detail: status.installCommand
            ? `Suggested command: ${status.installCommand}`
            : "Install it before using related workflows.",
          actionLabel: status.installCommand ? "Install" : null
        }
      ];
    }
    snapshot = {
      ...nextSnapshot,
      generatedAt: new Date().toISOString(),
      [key]: nextCollection,
      problems
    };
  }

  async function runGatewayAction(action: "start" | "stop" | "restart") {
    gatewayBusy = true;
    error = null;
    try {
      const result = action === "start"
        ? await startGateway()
        : action === "stop"
          ? await stopGateway()
          : await restartGateway();
      gatewayStatus = result.status;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      gatewayStatus = await loadGatewayStatus().catch(() => gatewayStatus);
    } finally {
      gatewayBusy = false;
    }
  }

  onMount(() => {
    applyTheme("system");
    void refreshSettings();
    void loadDashboardWithCache();
    void checkForAppUpdate();
  });

  onDestroy(() => {
    clearBackgroundDetection();
  });

  function openWizard(prefill: WizardPrefill | null = null) {
    wizardPrefill = prefill;
    route = "wizard";
  }

  function configureTool(tool: ToolStatus) {
    openWizard({
      toolId: tool.id,
      toolName: tool.name
    });
  }
</script>

<main class="app-shell">
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">
        <BrandLogo />
      </div>
      <div>
        <strong>CodeStudio Lite</strong>
      </div>
    </div>

    <nav class="sidebar-nav" aria-label="Primary">
      <div class="nav-section-title">Workspace</div>
      {#each navItems as item}
        <button class:active={route === item.id} title={$t(item.labelKey)} on:click={() => selectRoute(item.id)}>
          <AppIcon name={item.icon} size={18} />
          <span class="nav-item-label">{$t(item.labelKey)}</span>
          {#if item.id === "settings" && $appUpdateState.updateAvailable}
            <span class="nav-update-dot" aria-label={$t("settings.updateAvailableDot")}></span>
          {/if}
        </button>
      {/each}
    </nav>

    <section class={`sidebar-gateway ${sidebarGatewayTone}`} aria-label={$t("dashboard.localGateway")}>
      <div class="sidebar-status-line">
        <span class="status-dot"></span>
        <strong>{$t("dashboard.localGateway")}</strong>
        <span>{sidebarGatewayState}</span>
      </div>

      <div class="sidebar-gateway-field">
        <span>{$t("common.url")}</span>
        <code>{gatewayStatus?.baseUrl ?? "http://127.0.0.1:43112/v1"}</code>
      </div>

      <div class="sidebar-gateway-field">
        <span>{$t("dashboard.activeProfile")}</span>
        <strong>{activeProfileName ?? $t("dashboard.notConfigured")}</strong>
      </div>

      {#if gatewayStatus?.lastError}
        <div class="sidebar-gateway-error">{gatewayStatus.lastError}</div>
      {/if}

      <div class="sidebar-gateway-actions">
        <button class="icon-button gateway-start-button" title={$t("common.start")} on:click={() => runGatewayAction("start")} disabled={gatewayBusy || gatewayStatus?.running}>
          <AppIcon name="power" size={16} />
        </button>
        <button class="icon-button" title={$t("common.restart")} on:click={() => runGatewayAction("restart")} disabled={gatewayBusy}>
          <AppIcon name="restart" size={16} class={gatewayBusy ? "spin" : ""} />
        </button>
        <button class="icon-button" title={$t("common.stop")} on:click={() => runGatewayAction("stop")} disabled={gatewayBusy || !gatewayStatus?.running}>
          <AppIcon name="stop" size={16} />
        </button>
        <button class="icon-button" title={$t("dashboard.copyGatewayUrl")} on:click={copyGatewayUrl} disabled={!gatewayStatus?.baseUrl}>
          <AppIcon name="copy" size={16} />
        </button>
      </div>
    </section>

    <div class="sidebar-status">
      <small>{$t("app.version")}</small>
    </div>
  </aside>

  <section class="workspace">
    {#if error}
      <div class="error-banner">{error}</div>
    {/if}

    {#if route === "dashboard"}
      <Dashboard
        {snapshot}
        onRefresh={refreshDashboard}
        onToolStatusUpdated={mergeToolStatus}
        onConfigureTool={configureTool}
        onOpenCodexClient={() => {
          route = "codexClient";
        }}
      />
    {:else if route === "codexClient"}
      <CodexClient />
    {:else if route === "codexOAuth"}
      <CodexOAuth summary={profileSummary} {snapshot} onProfileSwitched={refreshAfterProfileChange} />
    {:else if route === "wizard"}
      <SetupWizard {snapshot} prefill={wizardPrefill} onProfileSaved={async () => {
        await refreshAfterProfileChange();
        route = "profiles";
      }} />
    {:else if route === "profiles"}
      <Profiles summary={profileSummary} {snapshot} onProfileSwitched={refreshAfterProfileChange} />
    {:else}
      <SettingsRoute />
    {/if}
  </section>
</main>
