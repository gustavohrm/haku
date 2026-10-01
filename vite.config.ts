import { fileURLToPath } from "node:url";

import lucide from "@codenhub/icons/data/lucide";
import { viteIcons } from "@codenhub/icons/vite";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const resolvePath = (path: string) => fileURLToPath(new URL(path, import.meta.url));

const DEV_SERVER_PORT = 1420;
const HMR_PORT = 1421;

/**
 * Sources the icon plugin reads up front, so the first paint in dev already has
 * every icon rather than gaining them as modules are transformed.
 *
 * Forward slashes because the pattern is a glob, where a backslash escapes.
 */
const ICON_SOURCES = `${resolvePath("./src").replaceAll("\\", "/")}/**/*.{html,ts,tsx}`;

export default defineConfig({
  root: "./src",
  build: { outDir: "../dist", emptyOutDir: true, target: "chrome110" },
  resolve: {
    alias: {
      "@app": resolvePath("./src/app"),
      "@bindings": resolvePath("./src/bindings.ts"),
      "@ipc": resolvePath("./src/ipc"),
      "@features": resolvePath("./src/features"),
      "@shared": resolvePath("./src/shared"),
    },
  },
  plugins: [
    react(),
    tailwindcss(),
    viteIcons({
      content: [ICON_SOURCES],
      families: [lucide],
      defaultPrefix: "lucide",
    }),
  ],
  // Keep Rust compiler errors visible in the dev output.
  clearScreen: false,
  server: {
    port: DEV_SERVER_PORT,
    strictPort: true,
    hmr: { port: HMR_PORT },
    watch: { ignored: ["**/tauri/**"] },
  },
});
