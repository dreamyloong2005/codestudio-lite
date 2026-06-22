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
    void stopInstallTerminal({ sessionId: oldId }).catch(() => {});
  }
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
      cols: 100,
      rows: 24
    });
    activeSessionId = result.sessionId;
    terminalSession.update((state) => ({ ...state, sessionId: result.sessionId }));
    // Resize is non-critical — fire it without blocking so the terminal
    // panel can start rendering output immediately.
    void resizeInstallTerminal({ sessionId: result.sessionId, cols: 100, rows: 24 }).catch(() => {});
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
  if (!activeSessionId) return;
  void resizeInstallTerminal({ sessionId: activeSessionId, cols, rows }).catch(() => {});
}

export function stopEmbeddedSession(): void {
  if (!activeSessionId) return;
  const sessionId = activeSessionId;
  activeSessionId = null;
  // Fire-and-forget: don't block the UI on the backend kill IPC.
  void stopInstallTerminal({ sessionId }).catch(() => {});
  terminalSession.update((state) => ({ ...state, running: false }));
}

export function clearTerminalSession(): void {
  if (inputFlushTimer) { clearTimeout(inputFlushTimer); inputFlushTimer = null; }
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
  inputBuffer = "";
  activeSessionId = null;
  outputHandler = null;
  pendingOutput = [];
  terminalSession.set(initialState);
}
