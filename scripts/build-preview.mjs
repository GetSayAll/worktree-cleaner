// Inline the Vite build output into a zero-dependency single file at dist/preview.html.
// Purpose: the browser preview needs to open by double-click without a server or a Node
// environment. It renders the same pages as the desktop app and labels its data as sample.
// Usage: node scripts/build-preview.mjs (prerequisite: vite build --base ./ has already run)

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const html = readFileSync(join(dist, "index.html"), "utf8");

const jsName = html.match(/src="\.\/assets\/([^"]+\.js)"/)?.[1];
const cssName = html.match(/href="\.\/assets\/([^"]+\.css)"/)?.[1];
if (!jsName || !cssName) {
  throw new Error("Could not find ./assets references in dist/index.html; build with --base ./ first");
}

const js = readFileSync(join(dist, "assets", jsName), "utf8").replaceAll("</script", "<\\/script");
const css = readFileSync(join(dist, "assets", cssName), "utf8").replaceAll("</style", "<\\/style");

// Two things to watch out for:
// 1) The replacement must be a function. A string replacement treats $& / $` / $' in the JS
//    as replacement patterns, which pollutes the inlined content with HTML fragments and
//    produces a file with syntax errors.
// 2) The inline script must go before </body>. Classic scripts do not auto-defer like modules,
//    so placing it in <head> runs it before #app is parsed, the Vue mount target does not exist,
//    and no error is thrown. We also stay away from type="module": under file:// such a script
//    cannot load a second chunk, and it would refuse to run at all if the bundle contained
//    `import.meta` — which Vite emits for any lazily imported module. `vite.config.ts` keeps
//    the build to a single chunk for the same reason.
const out = html
  .replace(/<script type="module"[^>]*><\/script>\s*/, "")
  .replace(/<link rel="stylesheet"[^>]*>/, () => `<style>\n${css}\n</style>`)
  .replace(/<\/body>/, () => `<script>\n${js}\n</script>\n</body>`);

if (out.includes("assets/index-")) throw new Error("External references remain; inlining did not finish");
if (!out.includes("<script>")) throw new Error("The script was not inlined");

writeFileSync(join(dist, "preview.html"), out);
console.log(`dist/preview.html generated (${out.length} bytes)`);
