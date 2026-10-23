import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // Tauri watches Rust separately. Runtime databases and WebView2 profiles are not source files.
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/release-artifacts/**", "**/.superpowers/**"] } },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "chrome105", minify: "esbuild", sourcemap: true }
});
