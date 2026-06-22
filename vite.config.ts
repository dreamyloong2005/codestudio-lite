import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
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
