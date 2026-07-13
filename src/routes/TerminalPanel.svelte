<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { FitAddon } from "@xterm/addon-fit";
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import AppIcon from "../components/AppIcon.svelte";
  import { t } from "../lib/i18n";
  import { actionButtonRecipe, terminalPanelActionsRecipe, terminalPanelFrameRecipe, terminalPanelHeaderRecipe, terminalPanelRecipe, terminalPanelStatusRecipe, terminalPanelTitleRecipe } from "../../styled-system/recipes";
  import {
    clearTerminalSession,
    resizeSession,
    setOutputHandler,
    stopEmbeddedSession,
    terminalSession,
    writeSessionInput
  } from "../lib/terminalSessionStore";

  export let onBack: () => void = () => {};

  type TerminalStatusTone = "running" | "exited" | "idle" | "error";

  let container: HTMLDivElement | null = null;
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let terminalResizeDisposable: { dispose: () => void } | null = null;
  let sessionToolName = "";
  let sessionRunning = false;
  let sessionExitCode: number | null = null;
  let sessionError: string | null = null;
  let terminalStatusTone: TerminalStatusTone = "idle";

  const unsubscribe = terminalSession.subscribe((state) => {
    sessionToolName = state.toolName;
    sessionRunning = state.running;
    sessionExitCode = state.exitCode;
    sessionError = state.error;
  });

  let fitFrame: number | null = null;
  let focusAfterFit = false;
  let observedWidth = -1;
  let observedHeight = -1;

  function scheduleTerminalFit(focus = false) {
    focusAfterFit ||= focus;
    if (!term || !container) return;
    if (fitFrame !== null) return;
    fitFrame = requestAnimationFrame(() => {
      fitFrame = null;
      if (!term || !fitAddon || !container || !container.isConnected) return;
      if (container.clientWidth <= 0 || container.clientHeight <= 0) return;
      try {
        fitAddon.fit();
      } catch (_) {
        return;
      }
      if (focusAfterFit) {
        focusAfterFit = false;
        term.focus();
      }
    });
  }

  function handleObservedResize(entries: ResizeObserverEntry[]) {
    const entry = entries[0];
    if (!entry) return;
    const width = Math.round(entry.contentRect.width);
    const height = Math.round(entry.contentRect.height);
    if (width === observedWidth && height === observedHeight) return;
    observedWidth = width;
    observedHeight = height;
    scheduleTerminalFit();
  }

  function clearScheduledFit() {
    if (fitFrame !== null) {
      cancelAnimationFrame(fitFrame);
      fitFrame = null;
    }
    focusAfterFit = false;
  }

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
    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);
    term.onData((data: string) => {
      writeSessionInput(data);
    });
    terminalResizeDisposable = term.onResize(({ cols, rows }) => {
      resizeSession(cols, rows);
    });
    setOutputHandler((data: string) => {
      term?.write(data);
    });
    await tick();
    resizeObserver = new ResizeObserver(handleObservedResize);
    resizeObserver.observe(container);
    scheduleTerminalFit(true);
  }

  let mounted = false;
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
    clearScheduledFit();
    setOutputHandler(null);
    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }
    if (terminalResizeDisposable) {
      terminalResizeDisposable.dispose();
      terminalResizeDisposable = null;
    }
    if (term) {
      term.dispose();
      term = null;
    }
    fitAddon = null;
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

  $: terminalStatusTone = sessionError
    ? "error"
    : sessionRunning
      ? "running"
    : sessionExitCode !== null
        ? "exited"
        : "idle";
  function handleEscape(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    handleBack();
  }
</script>

<svelte:window on:keydown={handleEscape} />

<section class={terminalPanelRecipe()}>
  <header class={terminalPanelHeaderRecipe()}>
    <div class={terminalPanelTitleRecipe()}>
      <AppIcon name="system" size={18} />
      <strong>{$t("terminal.title", { name: sessionToolName || $t("terminal.console") })}</strong>
    </div>
    <div class={terminalPanelStatusRecipe({ tone: terminalStatusTone })}>
      {#if sessionError}
        <span>{sessionError}</span>
      {:else if sessionRunning}
        <span>{$t("common.running")}</span>
      {:else if sessionExitCode !== null}
        <span>{$t("terminal.exitCode", { code: sessionExitCode })}</span>
      {:else}
        <span>{$t("terminal.ready")}</span>
      {/if}
    </div>
    <div class={terminalPanelActionsRecipe()}>
      {#if sessionRunning}
        <button class={actionButtonRecipe()} type="button" on:click={handleStop}>
          <AppIcon name="stop" size={16} />
          {$t("common.stop")}
        </button>
      {/if}
      <button class={actionButtonRecipe()} type="button" on:click={handleBack}>
        <AppIcon name="arrowLeft" size={16} />
        {$t("terminal.back")}
      </button>
    </div>
  </header>

  <div class={terminalPanelFrameRecipe()} data-terminal-frame bind:this={container}></div>
</section>
