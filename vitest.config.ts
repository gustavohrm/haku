import { fileURLToPath } from "node:url";

import { defineConfig } from "vitest/config";

const resolvePath = (path: string) => fileURLToPath(new URL(path, import.meta.url));

export default defineConfig({
  resolve: {
    alias: {
      "@app": resolvePath("./src/app"),
      "@bindings": resolvePath("./src/bindings.ts"),
      "@ipc": resolvePath("./src/ipc"),
      "@features": resolvePath("./src/features"),
      "@shared": resolvePath("./src/shared"),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    coverage: { provider: "v8", include: ["src/**/*.ts", "src/**/*.tsx"] },
  },
});
