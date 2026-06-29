/// <reference types="svelte" />
/// <reference types="vite/client" />

declare global {
  const __APP_VERSION__: string;

  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export {};
