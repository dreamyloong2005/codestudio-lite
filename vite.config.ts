import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import packageJson from "./package.json" with { type: "json" };

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
    __APP_VERSION__: JSON.stringify(packageJson.version)
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
