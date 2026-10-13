import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { port: 2430, strictPort: true },
  envPrefix: ["VITE_", "TAURI_"],
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
