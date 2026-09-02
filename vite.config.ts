import { readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import lucide from "@codenhub/icons/data/lucide";
import { viteIcons } from "@codenhub/icons/vite";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const resolvePath = (path: string) => fileURLToPath(new URL(path, import.meta.url));

const DEV_SERVER_PORT = 1420;
const HMR_PORT = 1421;

const ICON_SOURCE_EXTENSIONS = [".ts", ".tsx", ".html"];

/**
 * Lists the files the icon plugin should scan for `ic-*` class names.
 *
 * Its `content` option takes literal file paths, not globs: it stats each entry
 * and skips anything that is not a file. Passing a glob silently scans nothing,
 * which shows up as every icon rendering as a solid block.
 *
 * Resolved once at config load, so a newly added file needs a dev-server
 * restart before its icons appear.
 *
 * @param directory - Directory to walk.
 * @returns Every source file under it that can contain icon classes.
 */
function iconSources(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      return iconSources(path);
    }
    return ICON_SOURCE_EXTENSIONS.some((extension) => entry.name.endsWith(extension)) ? [path] : [];
  });
}

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
      content: iconSources(resolvePath("./src")),
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
