import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

/**
 * The version, read from `package.json`.
 *
 * It is injected rather than imported so that the number in the running app and
 * the number in `Cargo.toml` and `tauri.conf.json` cannot drift apart: the build
 * takes it from the one file that is the source of truth. `src/version.ts` reads
 * it back through the `__APP_VERSION__` constant.
 */
const pkg = JSON.parse(
  readFileSync(new URL("./package.json", import.meta.url), "utf8")
) as { version: string };

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { port: 2430, strictPort: true },
  envPrefix: ["VITE_", "TAURI_"],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version)
  },
  build: {
    rollupOptions: {
      output: {
        // One chunk, always.
        //
        // The browser preview inlines the build into a single HTML file that has
        // to open by double-click with no server, and it is a classic script, so
        // it can neither fetch a second file nor contain `import.meta` (which is
        // what Vite's chunk loader uses to resolve a lazily imported module).
        // The app has one entry and no route-level splitting to lose, so keeping
        // the desktop-only imports in the same file costs a download that was
        // happening anyway.
        inlineDynamicImports: true
      }
    }
  }
});
