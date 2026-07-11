import { writable } from "svelte/store";
import {
  listenInstallTerminalOutput,
  resizeInstallTerminal,
  startInstallTerminal,
  stopInstallTerminal,
  writeInstallTerminal
} from "./api";
import type { InstallTerminalOutput, StartInstallTerminalRequest } from "../types";

export interface TerminalSessionState {
  toolId: string;
  toolName: string;
  command: string;
  sessionId: string | null;
  running: boolean;
  exitCode: number | null;
  error: string | null;
}

const initialState: TerminalSessionState = {
  toolId: "",
  toolName: "",
  command: "",
  sessionId: null,
  running: false,
  exitCode: null,
  error: null
};

export const terminalSession = writable<TerminalSessionState>(initialState);

let activeSessionId: string | null = null;
let unlistenOutput: (() => void) | null = null;
let outputHandler: ((data: string) => void) | null = null;
let pendingOutput: string[] = [];
let pendingResize: { cols: number; rows: number } | null = null;
let resizeFlushTimer: ReturnType<typeof setTimeout> | null = null;
let lastResizeKey = "";
const TERMINAL_RESIZE_FLUSH_DELAY_MS = 80;
const INITIAL_TERMINAL_COLS = 100;
const INITIAL_TERMINAL_ROWS = 24;

function handleOutput(output: InstallTerminalOutput): void {
  if (!activeSessionId || output.sessionId !== activeSessionId) return;
  if (output.data && output.stream === "output") {
    if (outputHandler) {
      outputHandler(output.data);
    } else {
      pendingOutput.push(output.data);
    }
  }
  if (output.done) {
    terminalSession.update((state) => ({
      ...state,
      running: false,
      exitCode: output.exitCode ?? null
    }));
  }
}

function flushPending(): void {
  if (!outputHandler) return;
  for (const data of pendingOutput) outputHandler(data);
  pendingOutput = [];
}

function clearPendingResize(): void {
  if (resizeFlushTimer) {
    clearTimeout(resizeFlushTimer);
    resizeFlushTimer = null;
  }
  pendingResize = null;
  lastResizeKey = "";
}

function flushResize(): void {
  resizeFlushTimer = null;
  if (!activeSessionId || !pendingResize) return;
  const { cols, rows } = pendingResize;
  const resizeKey = `${activeSessionId}:${cols}x${rows}`;
  pendingResize = null;
  if (resizeKey === lastResizeKey) return;
  lastResizeKey = resizeKey;
  void resizeInstallTerminal({ sessionId: activeSessionId, cols, rows }).catch(() => {});
}

function schedulePendingResize(): void {
  if (!activeSessionId || !pendingResize) return;
  if (resizeFlushTimer) clearTimeout(resizeFlushTimer);
  resizeFlushTimer = setTimeout(flushResize, TERMINAL_RESIZE_FLUSH_DELAY_MS);
}

function normalizedResizeDimension(value: number, minimum: number): number {
  if (!Number.isFinite(value)) return minimum;
  return Math.max(minimum, Math.floor(value));
}

export function setOutputHandler(handler: ((data: string) => void) | null): void {
  outputHandler = handler;
  if (handler) flushPending();
}

export async function startEmbeddedSession(
  request: StartInstallTerminalRequest,
  toolName: string
): Promise<string | null> {
  if (activeSessionId) {
    // Kill the old session without blocking — don't await the IPC.
    const oldId = activeSessionId;
    activeSessionId = null;
    clearPendingResize();
    void stopInstallTerminal({ sessionId: oldId }).catch(() => {});
  }
  clearPendingResize();
  // Set the store state immediately so the terminal panel can render a
  // "connecting" state before the backend PTY is ready.
  terminalSession.set({
    toolId: request.toolId,
    toolName,
    command: request.command,
    sessionId: null,
    running: true,
    exitCode: null,
    error: null
  });
  pendingOutput = [];
  // Ensure the output listener is registered before starting the PTY so no
  // early output is missed. This is async but typically resolves in <1ms
  // (Tauri event listener setup, not a network round-trip).
  if (!unlistenOutput) {
    unlistenOutput = await listenInstallTerminalOutput(handleOutput);
  }
  try {
    const result = await startInstallTerminal({
      ...request,
      keepOpen: true,
      cols: INITIAL_TERMINAL_COLS,
      rows: INITIAL_TERMINAL_ROWS
    });
    activeSessionId = result.sessionId;
    lastResizeKey = `${result.sessionId}:${INITIAL_TERMINAL_COLS}x${INITIAL_TERMINAL_ROWS}`;
    terminalSession.update((state) => ({ ...state, sessionId: result.sessionId }));
    schedulePendingResize();
    return result.sessionId;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    terminalSession.update((state) => ({ ...state, running: false, error: message }));
    return null;
  }
}

let inputBuffer = "";
let inputFlushTimer: ReturnType<typeof setTimeout> | null = null;
const INPUT_FLUSH_DELAY_MS = 16;

function flushInputBuffer(): void {
  if (!activeSessionId || !inputBuffer) {
    inputBuffer = "";
    return;
  }
  const data = inputBuffer;
  inputBuffer = "";
  void writeInstallTerminal({ sessionId: activeSessionId, data }).catch(() => {});
}

export function writeSessionInput(data: string): void {
  if (!activeSessionId) return;
  inputBuffer += data;
  if (inputFlushTimer) clearTimeout(inputFlushTimer);
  // Flush immediately for control sequences (escape, enter, etc.) so the
  // terminal stays responsive; batch printable chars on a short timer.
  if (data.charCodeAt(0) < 32 || data === "\r" || data === "\x1b" || data.startsWith("\x1b")) {
    flushInputBuffer();
  } else {
    inputFlushTimer = setTimeout(flushInputBuffer, INPUT_FLUSH_DELAY_MS);
  }
}

export function resizeSession(cols: number, rows: number): void {
  pendingResize = {
    cols: normalizedResizeDimension(cols, 20),
    rows: normalizedResizeDimension(rows, 10)
  };
  schedulePendingResize();
}

export function stopEmbeddedSession(): void {
  if (!activeSessionId) return;
  const sessionId = activeSessionId;
  activeSessionId = null;
  clearPendingResize();
  // Fire-and-forget: don't block the UI on the backend kill IPC.
  void stopInstallTerminal({ sessionId }).catch(() => {});
  terminalSession.update((state) => ({ ...state, running: false }));
}

export function clearTerminalSession(): void {
  if (inputFlushTimer) { clearTimeout(inputFlushTimer); inputFlushTimer = null; }
  clearPendingResize();
  inputBuffer = "";
  activeSessionId = null;
  outputHandler = null;
  pendingOutput = [];
  terminalSession.set(initialState);
}

export function disposeTerminalSession(): void {
  if (unlistenOutput) {
    unlistenOutput();
    unlistenOutput = null;
  }
  if (inputFlushTimer) { clearTimeout(inputFlushTimer); inputFlushTimer = null; }
  clearPendingResize();
  inputBuffer = "";
  activeSessionId = null;
  outputHandler = null;
  pendingOutput = [];
  terminalSession.set(initialState);
}
