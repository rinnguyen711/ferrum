import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Mounted under /studio when served from axum. Dev server still uses "/"
// (no prefix) so http://localhost:5173/ keeps working.
export default defineConfig(({ command }) => ({
  plugins: [react()],
  base: command === "build" ? "/studio/" : "/",
  server: {
    port: 5173,
    proxy: {
      "/api": "http://localhost:8080",
      "/admin": "http://localhost:8080",
      "/healthz": "http://localhost:8080",
    },
  },
}));
