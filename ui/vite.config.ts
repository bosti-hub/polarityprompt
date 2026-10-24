import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port (see apps/desktop/tauri.conf.json → build.devUrl).
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: {
      // Do not let the Vite watcher follow the Rust build output.
      ignored: ["**/target/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // Tauri uses WebKit on macOS and Chromium (WebView2) on Windows.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
