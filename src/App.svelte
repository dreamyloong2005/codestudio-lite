<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { fade, fly } from "svelte/transition";
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
    stopGateway,
    takePendingClaudeDesktopLaunchAfterRestart,
    updateGatewaySettings
  } from "./lib/api";
  import { appUpdateState, checkForAppUpdate } from "./lib/appUpdateStore";
  import {
    ensureClaudeDesktopLoaded,
    setClaudeDesktopPendingLaunchAfterRestart
  } from "./lib/claudeDesktopStore";
  import { ensureCodexClientLoaded } from "./lib/codexClientStore";
  import { setLocale, t } from "./lib/i18n";
  import { REFRESH_CACHE_TTL_MS, readRefreshTimestamp, refreshTimestampFresh, writeRefreshTimestamp } from "./lib/refreshCache";
  import { applyTheme } from "./lib/theme";
  import { disposeTerminalSession } from "./lib/terminalSessionStore";
  import { appBrandMarkRecipe, appBrandRecipe, appErrorBannerRecipe, appNavButtonRecipe, appNavLabelRecipe, appNavRecipe, appNavUpdateDotRecipe, appRouteTransitionRecipe, appShellRecipe, appSidebarRecipe, appWorkspaceRecipe } from "../styled-system/recipes";
  import ClaudeDesktop from "./routes/ClaudeDesktop.svelte";
  import CodexClient from "./routes/CodexClient.svelte";
  import Dashboard from "./routes/Dashboard.svelte";
  import Gateway from "./routes/Gateway.svelte";
  import Profiles from "./routes/Profiles.svelte";
  import SettingsRoute from "./routes/Settings.svelte";
  import TerminalPanel from "./routes/TerminalPanel.svelte";
  import SetupWizard from "./routes/SetupWizard.svelte";
  import type {
    DetectionSnapshot,
    GatewayStatus,
    PrivacyFilterMode,
    ProfileSummary,
    ToolStatus,
    WizardPrefill
  } from "./types";

  type Route = "dashboard" | "codexClient" | "claudeDesktop" | "wizard" | "profiles" | "gateway" | "settings" | "terminal";

  let route: Route = "dashboard";
  let dashboardLoading = true;
  let dashboardRefreshing = false;
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
  let dashboardRefreshIndicatorRunId: number | null = null;
  let lastRouteRefreshRoute: Route = route;
  let pendingClaudeDesktopRouteRestore = false;
  let lastDashboardRefreshAt = readRefreshTimestamp("detection");
  const DASHBOARD_NAVIGATION_REFRESH_TTL_MS = REFRESH_CACHE_TTL_MS;

  // detect_environment returns local detection immediately and can kick off update checks.
  // Keep automatic re-detection coarse: recent successful scans are reused across app restarts.
  const BACKGROUND_DETECTION_MIN_DELAY_MS = 60_000;

  const navItems: Array<{ id: Route; labelKey: Parameters<typeof $t>[0]; icon: AppIconName }> = [
    { id: "dashboard", labelKey: "app.nav.dashboard", icon: "dashboard" },
    { id: "codexClient", labelKey: "app.nav.codexClient", icon: "codexClient" },
    { id: "claudeDesktop", labelKey: "app.nav.claudeDesktop", icon: "claudeDesktop" },
    { id: "profiles", labelKey: "app.nav.profiles", icon: "profiles" },
    { id: "gateway", labelKey: "app.nav.gateway", icon: "gateway" },
    { id: "settings", labelKey: "app.nav.settings", icon: "settings" }
  ];
  const routeEnterTransition = { y: 22, duration: 320, opacity: 0, easing: cubicOut };
  const routeExitTransition = { duration: 140 };

  $: desktopClientPagesAvailable = ["windows", "macos"].includes(snapshot?.platform ?? "");
  $: visibleNavItems = navItems.filter((item) => !["codexClient", "claudeDesktop"].includes(item.id) || desktopClientPagesAvailable);
  $: if (snapshot && pendingClaudeDesktopRouteRestore) {
    pendingClaudeDesktopRouteRestore = false;
  }
  $: if (["codexClient", "claudeDesktop"].includes(route) && !desktopClientRouteAllowed(route)) {
    route = "dashboard";
  }
  $: if (route !== lastRouteRefreshRoute) {
    lastRouteRefreshRoute = route;
    void refreshCurrentRouteAfterSwitch(route);
  }

  async function copyGatewayUrl() {
    if (!gatewayStatus?.baseUrl) {
      return;
    }
    await navigator.clipboard?.writeText(gatewayStatus.baseUrl);
  }

  function selectRoute(nextRoute: Route) {
    if (["codexClient", "claudeDesktop"].includes(nextRoute) && !desktopClientPagesAvailable) {
      route = "dashboard";
      return;
    }
    if (nextRoute === "wizard") {
      wizardPrefill = null;
    }
    route = nextRoute;
  }

  function desktopClientRouteAllowed(currentRoute: Route) {
    return desktopClientPagesAvailable || (currentRoute === "claudeDesktop" && pendingClaudeDesktopRouteRestore);
  }

  function openTerminal() {
    route = "terminal";
  }

  function navigateToClient(toolId: string) {
    if (toolId === "codex-app") {
      selectRoute("codexClient");
    } else if (toolId === "claude-desktop") {
      selectRoute("claudeDesktop");
    }
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

  function dashboardRefreshDelayMs() {
    const age = Date.now() - lastDashboardRefreshAt;
    return Math.max(DASHBOARD_NAVIGATION_REFRESH_TTL_MS - age, BACKGROUND_DETECTION_MIN_DELAY_MS);
  }

  function startBackgroundDetection() {
    clearBackgroundDetection();
    if (route !== "dashboard") {
      return;
    }
    backgroundDetectionTimers = [
      window.setTimeout(() => {
        if (route === "dashboard") {
          void refreshDashboard({ quiet: true, scheduleFollowup: true });
        }
      }, dashboardRefreshDelayMs())
    ];
  }

  type RefreshDashboardOptions = { quiet?: boolean; scheduleFollowup?: boolean; showRefreshIndicator?: boolean };

  function detectionSnapshotUiPayload(value: DetectionSnapshot) {
    const { generatedAt, source, ...stableSnapshot } = value;
    return stableSnapshot;
  }

  function detectionSnapshotUiChanged(current: DetectionSnapshot | null, next: DetectionSnapshot) {
    if (!current) {
      return true;
    }
    return JSON.stringify(detectionSnapshotUiPayload(current)) !== JSON.stringify(detectionSnapshotUiPayload(next));
  }

  function applyDetectionSnapshot(nextSnapshot: DetectionSnapshot) {
    if (detectionSnapshotUiChanged(snapshot, nextSnapshot)) {
      snapshot = nextSnapshot;
    }
  }

  function profileSummaryUiChanged(current: ProfileSummary | null, next: ProfileSummary) {
    if (!current) {
      return true;
    }
    return JSON.stringify(current) !== JSON.stringify(next);
  }

  function applyProfileSummary(nextSummary: ProfileSummary) {
    if (profileSummaryUiChanged(profileSummary, nextSummary)) {
      profileSummary = nextSummary;
    }
  }

  function gatewayStatusUiChanged(current: GatewayStatus | null, next: GatewayStatus | null) {
    if (!current || !next) {
      return current !== next;
    }
    return JSON.stringify(current) !== JSON.stringify(next);
  }

  function applyGatewayStatus(nextStatus: GatewayStatus | null) {
    if (gatewayStatusUiChanged(gatewayStatus, nextStatus)) {
      gatewayStatus = nextStatus;
    }
  }

  async function refreshDashboard(options: RefreshDashboardOptions = {}) {
    const quiet = options.quiet ?? false;
    const scheduleFollowup = options.scheduleFollowup ?? true;
    const showRefreshIndicator = options.showRefreshIndicator ?? route === "dashboard";
    if (scheduleFollowup) {
      clearBackgroundDetection();
    }
    const runId = ++dashboardRefreshRunId;
    if (showRefreshIndicator) {
      dashboardRefreshIndicatorRunId = runId;
      dashboardRefreshing = true;
    }
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
      applyProfileSummary(nextProfileSummary);
      applyDetectionSnapshot(nextSnapshot);
      applyGatewayStatus(nextGatewayStatus);
      lastDashboardRefreshAt = Date.now();
      writeRefreshTimestamp("detection", lastDashboardRefreshAt);
    } catch (err) {
      if (!quiet && visibleRefreshRunId === runId) {
        error = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (showRefreshIndicator && dashboardRefreshIndicatorRunId === runId) {
        dashboardRefreshing = false;
        dashboardRefreshIndicatorRunId = null;
      }
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

  async function loadDashboardWithCache(options: { showRefreshIndicator?: boolean } = {}) {
    const showRefreshIndicator = options.showRefreshIndicator ?? route === "dashboard";
    let cachedSnapshot: DetectionSnapshot | null = null;
    if (showRefreshIndicator) {
      dashboardRefreshing = true;
    }
    dashboardLoading = true;
    error = null;
    try {
      const [nextProfileSummary, nextCachedSnapshot, nextGatewayStatus] = await Promise.all([
        ensureAppDirs(),
        loadCachedDetection(),
        loadGatewayStatus()
      ]);
      applyProfileSummary(nextProfileSummary);
      applyGatewayStatus(nextGatewayStatus);
      cachedSnapshot = nextCachedSnapshot;
      if (cachedSnapshot) {
        applyDetectionSnapshot(cachedSnapshot);
        dashboardLoading = false;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }

    if (cachedSnapshot && refreshTimestampFresh("detection", DASHBOARD_NAVIGATION_REFRESH_TTL_MS)) {
      startBackgroundDetection();
      if (showRefreshIndicator) {
        dashboardRefreshing = false;
      }
      return;
    }

    await refreshDashboard({
      quiet: snapshot !== null,
      scheduleFollowup: true,
      showRefreshIndicator
    });
  }

  async function restorePendingClaudeDesktopLaunch() {
    try {
      const pending = await takePendingClaudeDesktopLaunchAfterRestart();
      if (!pending) {
        return;
      }
      setClaudeDesktopPendingLaunchAfterRestart(pending);
      pendingClaudeDesktopRouteRestore = true;
      route = "claudeDesktop";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
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

  async function refreshProfileAndGatewayOnly() {
    const runId = ++dashboardRefreshRunId;
    try {
      const [nextProfileSummary, nextGatewayStatus] = await Promise.all([
        ensureAppDirs(),
        loadGatewayStatus()
      ]);
      if (runId !== dashboardRefreshRunId) {
        return;
      }
      applyProfileSummary(nextProfileSummary);
      applyGatewayStatus(nextGatewayStatus);
    } catch (err) {
      if (visibleRefreshRunId === runId) {
        error = err instanceof Error ? err.message : String(err);
      }
    }
  }

  async function refreshAfterProfileChange() {
    // Profile changes only need the lightweight summary/gateway refresh in the
    // foreground. The heavier environment scan stays in the background.
    await refreshProfileAndGatewayOnly();
    void refreshDashboard({ quiet: true, scheduleFollowup: false });
  }

  async function refreshCurrentRouteAfterSwitch(currentRoute: Route) {
    if (currentRoute !== "dashboard") {
      clearBackgroundDetection();
    }
    if (currentRoute === "dashboard") {
      lastDashboardRefreshAt = readRefreshTimestamp("detection");
      const stale = Date.now() - lastDashboardRefreshAt > DASHBOARD_NAVIGATION_REFRESH_TTL_MS;
      if (snapshot && !stale) {
        await refreshProfileAndGatewayOnly();
        startBackgroundDetection();
      } else {
        await refreshDashboard({ quiet: true, scheduleFollowup: true, showRefreshIndicator: true });
      }
    } else if (currentRoute === "codexClient") {
      await ensureCodexClientLoaded();
    } else if (currentRoute === "claudeDesktop") {
      await ensureClaudeDesktopLoaded();
    } else if (currentRoute === "profiles" || currentRoute === "gateway") {
      await refreshAfterProfileChange();
    } else if (currentRoute === "settings") {
      await refreshSettings();
      if ($appUpdateState.status === "idle") {
        await checkForAppUpdate();
      }
    }
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
      applyGatewayStatus(result.status);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      applyGatewayStatus(await loadGatewayStatus().catch(() => gatewayStatus));
    } finally {
      gatewayBusy = false;
    }
  }

  async function updateGatewayPrivacyFilter(mode: PrivacyFilterMode) {
    const result = await updateGatewaySettings({ privacyFilterMode: mode });
    applyGatewayStatus(result.status);
  }

  async function initializeDashboardOnMount() {
    await restorePendingClaudeDesktopLaunch();
    await loadDashboardWithCache({ showRefreshIndicator: route === "dashboard" });
  }

  onMount(() => {
    applyTheme("system");
    void refreshSettings();
    void initializeDashboardOnMount();
    void checkForAppUpdate();
  });

  onDestroy(() => {
    clearBackgroundDetection();
    disposeTerminalSession();
  });

  function openWizard(prefill: WizardPrefill | null = null) {
    wizardPrefill = prefill;
    route = "wizard";
  }

  function configureTool(tool: ToolStatus) {
    openWizard({
      toolId: tool.id,
      toolName: tool.name,
      mode: "config"
    });
  }
</script>

<main class={appShellRecipe()}>
  <aside class={appSidebarRecipe()}>
    <div class={appBrandRecipe()}>
      <div class={appBrandMarkRecipe()}>
        <BrandLogo />
      </div>
      <div>
        <strong>CodeStudio Lite</strong>
      </div>
    </div>

    <nav class={appNavRecipe()} aria-label="Primary">
      {#each visibleNavItems as item}
        <button class={appNavButtonRecipe()} data-active={route === item.id} title={$t(item.labelKey)} on:click={() => selectRoute(item.id)}>
          <AppIcon name={item.icon} size={18} />
          <span class={appNavLabelRecipe()}>{$t(item.labelKey)}</span>
          {#if item.id === "settings" && $appUpdateState.updateAvailable}
            <span class={appNavUpdateDotRecipe()} aria-label={$t("settings.updateAvailableDot")}></span>
          {/if}
        </button>
      {/each}
    </nav>
  </aside>

  <section class={appWorkspaceRecipe()}>
    {#if error}
      <div class={appErrorBannerRecipe()}>{error}</div>
    {/if}

    {#key route}
      <div class={appRouteTransitionRecipe()} in:fly={routeEnterTransition} out:fade={routeExitTransition}>
        {#if route === "dashboard"}
          <Dashboard
            {snapshot}
            refreshingExternally={dashboardRefreshing}
            onRefresh={refreshDashboard}
            onToolStatusUpdated={mergeToolStatus}
            onConfigureTool={configureTool}
            onOpenTerminal={openTerminal}
            onNavigateToClient={navigateToClient}
          />
        {:else if route === "codexClient"}
          <CodexClient />
        {:else if route === "claudeDesktop"}
          <ClaudeDesktop />
        {:else if route === "wizard"}
          <SetupWizard {snapshot} prefill={wizardPrefill} onProfileSaved={async (mode) => {
            await refreshAfterProfileChange();
            route = mode === "gateway" ? "gateway" : "profiles";
          }} />
        {:else if route === "profiles"}
          <Profiles
            summary={profileSummary}
            {snapshot}
            modeFilter="config"
            onProfileSwitched={refreshAfterProfileChange}
            onCreateProfile={(prefill) => openWizard({ ...prefill, mode: "config" })}
          />
        {:else if route === "gateway"}
          <Gateway
            summary={profileSummary}
            {snapshot}
            {gatewayStatus}
            {gatewayBusy}
            onGatewayAction={runGatewayAction}
            onPrivacyFilterChange={updateGatewayPrivacyFilter}
            onCopyGatewayUrl={copyGatewayUrl}
            onProfileSwitched={refreshAfterProfileChange}
            onCreateProfile={openWizard}
          />
        {:else if route === "terminal"}
          <TerminalPanel onBack={() => { route = "dashboard"; }} />
        {:else}
          <SettingsRoute />
        {/if}
      </div>
    {/key}
  </section>
</main>
