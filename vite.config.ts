import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { iconsPlugin } from "./plugins/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  root: "./src",
  build: {
    outDir: "../dist",
  },
  plugins: [tailwindcss(), iconsPlugin()],
  // Prevent Vite from obscuring rust errors
  clearScreen: false,
  // Tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Tell Vite to ignore watching tauri directory
      ignored: ["**/tauri/**"],
    },
  },
}));
