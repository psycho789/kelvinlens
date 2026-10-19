import { defineConfig } from "vite";

// Tauri serves the built files from dist/ in release builds and this dev server in `tauri dev`.
export default defineConfig({
  clearScreen: false,
  // KELVINLENS_DEV_HOOKS=1 exposes UI internals for automated checks (debug builds only).
  define: { __DEV_HOOKS__: JSON.stringify(process.env.KELVINLENS_DEV_HOOKS === "1") },
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/target/**"] } },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true, sourcemap: false },
});
