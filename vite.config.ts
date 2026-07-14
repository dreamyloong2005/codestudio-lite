import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import packageJson from "./package.json" with { type: "json" };
import updaterConfig from "./updater.config.json" with { type: "json" };

const updaterEnabled = Boolean(
  (process.env.CODESTUDIO_UPDATE_BASE_URL?.trim() || updaterConfig.baseUrl.trim()) &&
    (process.env.TAURI_UPDATER_PUBKEY?.trim() || updaterConfig.pubkey.trim())
);

function manualChunks(id: string): string | undefined {
  const normalizedId = id.replaceAll("\\", "/");

  if (normalizedId.includes("/src/lib/locales/")) {
    return "locales";
  }
  if (!normalizedId.includes("/node_modules/")) {
    return undefined;
  }
  if (normalizedId.includes("/node_modules/@xterm/")) {
    return "terminal";
  }
  return "vendor";
}

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(packageJson.version),
    __APP_UPDATER_ENABLED__: JSON.stringify(updaterEnabled)
  },
  build: {
    // Keep the largest stable runtimes separate while preserving synchronous
    // startup inside the local Tauri bundle.
    rollupOptions: {
      output: {
        manualChunks
      }
    }
  },
  server: {
    strictPort: true,
    host: "127.0.0.1",
    port: 1420
  }
});
