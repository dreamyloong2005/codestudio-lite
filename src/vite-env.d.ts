/// <reference types="svelte" />
/// <reference types="vite/client" />

declare global {
  const __APP_VERSION__: string;
  const __APP_UPDATER_ENABLED__: boolean;

  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export {};
