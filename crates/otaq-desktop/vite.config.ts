import react from "@vitejs/plugin-react";
import { resolve } from "node:path";
import { searchForWorkspaceRoot } from "vite";
import { defineConfig } from "vitest/config";

const here = import.meta.dirname;

export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5199,
    strictPort: true,
    // The icons are bundled from `otaq-view` (ui/src/theme/icons.ts), outside this folder.
    fs: { allow: [searchForWorkspaceRoot(here), resolve(here, "../otaq-view/assets/icons")] },
  },
  build: { outDir: "dist", emptyOutDir: true, target: "es2022", chunkSizeWarningLimit: 2000 },
  test: {
    root: "ui",
    include: ["src/**/*.test.ts"],
    environment: "node",
    // The clipboard add-on's CommonJS build needs a browser's `self`; the tests load the ES module the page bundles.
    alias: [
      { find: /^@xterm\/addon-clipboard$/, replacement: "@xterm/addon-clipboard/lib/addon-clipboard.mjs" },
    ],
  },
});
