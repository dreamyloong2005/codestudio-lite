export interface RuntimeAdapter {
  readonly kind: "tauri" | "browser";
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

export function runtimeAdapter(): RuntimeAdapter {
  return window.__TAURI_INTERNALS__ ? tauriAdapter : browserAdapter;
}

const browserAdapter: RuntimeAdapter = {
  kind: "browser",
  async invoke<T>(command: string): Promise<T> {
    throw new Error(`Native command '${command}' is unavailable in the browser adapter.`);
  }
};

const tauriAdapter: RuntimeAdapter = {
  kind: "tauri",
  async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  }
};
