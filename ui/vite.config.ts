import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  // Tauri serves the built assets from disk, so every URL must be relative.
  base: "./",
  build: {
    outDir: "dist",
    // Assets are inlined only below this size; anything larger becomes a file.
    // Keeping it small means the CSP stays strict without data: exceptions.
    assetsInlineLimit: 0,
    target: "es2022",
    sourcemap: false,
  },
  server: { port: 5173, strictPort: true },
});
