import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vitest/config";

// Tauri expects a fixed port and must not have its Rust output watched by Vite.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: { alias: { "@": new URL("./src", import.meta.url).pathname } },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: { outDir: "dist", target: "safari16.4", sourcemap: false },
  test: { environment: "node", include: ["src/**/*.test.{ts,tsx}"] },
});
