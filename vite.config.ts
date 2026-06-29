import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import packageJson from "./package.json" with { type: "json" };

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(packageJson.version)
  },
  // The app ships as a single bundle inside the Tauri binary, so there is no
  // network benefit to code-splitting and the main chunk legitimately exceeds
  // Vite's 500 kB heuristic. Raise the limit to keep the build warning-free.
  build: {
    chunkSizeWarningLimit: 1000
  },
  server: {
    strictPort: true,
    host: "127.0.0.1",
    port: 1420
  }
});
