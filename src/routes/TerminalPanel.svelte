<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import AppIcon from "../components/AppIcon.svelte";
  import { t } from "../lib/i18n";
  import {
    clearTerminalSession,
    disposeTerminalSession,
    resizeSession,
    setOutputHandler,
    stopEmbeddedSession,
    terminalSession,
    writeSessionInput
  } from "../lib/terminalSessionStore";

  export let onBack: () => void = () => {};

  let container: HTMLDivElement | null = null;
  let term: Terminal | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let sessionToolName = "";
  let sessionRunning = false;
  let sessionExitCode: number | null = null;
  let sessionError: string | null = null;

  // Subscribe to store but only read fields we actually render — avoid
  // pulling sessionId into a reactive variable that would toggle {#if}.
  const unsubscribe = terminalSession.subscribe((state) => {
    sessionToolName = state.toolName;
    sessionRunning = state.running;
    sessionExitCode = state.exitCode;
    sessionError = state.error;
  });

  let resizeTimer: ReturnType<typeof setTimeout> | null = null;
  let lastCols = 100;
  let lastRows = 24;

  function fitTerminal() {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(doFit, 80);
  }

  let initTime = 0;
  function doFit() {
    if (!term || !container) return;
    // Skip resize during the route enter transition (fly, 320ms) to avoid
    // a resize storm from the continuously-changing layout.
    if (Date.now() - initTime < 450) return;
    // Use xterm's own cell dimensions for accurate sizing instead of
    // measuring the DOM (which is unreliable and causes resize loops).
    const cell = (term as any)._core?._renderService?._dimensions;
    const cellW = cell?.cellWidth ?? 7.2;
    const cellH = cell?.cellHeight ?? 14;
    const padding = 16;
    const cols = Math.max(20, Math.floor((container.clientWidth - padding) / cellW));
    const rows = Math.max(10, Math.floor((container.clientHeight - padding) / cellH));
    if (cols !== lastCols || rows !== lastRows) {
      lastCols = cols;
      lastRows = rows;
      try { term.resize(cols, rows); } catch (_) { return; }
      resizeSession(cols, rows);
    }
  }

  let mounted = false;
  async function initTerminal() {
    if (term || !container) return;
    term = new Terminal({
      convertEol: true,
      cursorBlink: true,
      fontFamily: 'ui-monospace, "SFMono-Regular", Consolas, monospace',
      fontSize: 13,
      rows: 24,
      cols: 100,
      theme: {
        background: "#0f172a",
        foreground: "#e5edf6",
        cursor: "#facc15",
        selectionBackground: "#334155"
      }
    });
    term.open(container);
    initTime = Date.now();
    term.focus();
    term.onData((data: string) => {
      writeSessionInput(data);
    });
    setOutputHandler((data: string) => {
      term?.write(data);
    });
    // Fit after the DOM has settled (xterm needs a frame to measure fonts).
    await tick();
    requestAnimationFrame(() => { if (term && container) doFit(); });
    resizeObserver = new ResizeObserver(() => fitTerminal());
    resizeObserver.observe(container);
  }

  onMount(() => {
    mounted = true;
    if (container) {
      void initTerminal();
    } else {
      // container may not be bound yet; retry on next tick.
      setTimeout(() => { if (mounted && container) void initTerminal(); }, 0);
    }
  });

  onDestroy(() => {
    mounted = false;
    if (resizeTimer) { clearTimeout(resizeTimer); resizeTimer = null; }
    setOutputHandler(null);
    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }
    if (term) {
      term.dispose();
      term = null;
    }
    unsubscribe();
  });

  function handleStop() {
    stopEmbeddedSession();
  }

  function handleBack() {
    // Non-blocking: stop in the background, navigate immediately.
    stopEmbeddedSession();
    clearTerminalSession();
    onBack();
  }
</script>

<section class="terminal-panel">
  <header class="terminal-panel-header">
    <div class="terminal-panel-title">
      <AppIcon name="system" size={18} />
      <strong>{$t("terminal.title", { name: sessionToolName || $t("terminal.console") })}</strong>
    </div>
    <div class="terminal-panel-status">
      {#if sessionError}
        <span class="terminal-error">{sessionError}</span>
      {:else if sessionRunning}
        <span class="terminal-running">{$t("common.running")}</span>
      {:else if sessionExitCode !== null}
        <span class="terminal-exited">{$t("terminal.exitCode", { code: sessionExitCode })}</span>
      {:else}
        <span class="terminal-idle">{$t("terminal.ready")}</span>
      {/if}
    </div>
    <div class="terminal-panel-actions">
      {#if sessionRunning}
        <button class="secondary-button" type="button" on:click={handleStop}>
          <AppIcon name="stop" size={16} />
          {$t("common.stop")}
        </button>
      {/if}
      <button class="secondary-button" type="button" on:click={handleBack}>
        <AppIcon name="arrowLeft" size={16} />
        {$t("terminal.back")}
      </button>
    </div>
  </header>

  <div class="terminal-panel-frame" bind:this={container}></div>
</section>

<style>
  .terminal-panel {
    display: grid;
    grid-template-rows: auto 1fr;
    gap: 14px;
    min-height: 0;
    height: 100%;
  }
  .terminal-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .terminal-panel-title {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }
  .terminal-panel-title strong {
    font-size: 15px;
  }
  .terminal-panel-status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 750;
  }
  .terminal-running { color: #22c55e; }
  .terminal-exited { color: var(--text-muted); }
  .terminal-idle { color: var(--text-muted); }
  .terminal-error { color: var(--danger, #ef4444); }
  .terminal-panel-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .terminal-panel-frame {
    min-height: 0;
    height: 100%;
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: #0f172a;
    padding: 8px;
  }
  :global(.terminal-panel-frame .xterm) {
    height: 100%;
  }
  :global(.terminal-panel-frame .xterm-viewport) {
    border-radius: 8px;
  }
</style>
